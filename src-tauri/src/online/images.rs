use super::{checked, connection_error, repository_url, DOC, GITHUB_FILE_LIMIT};
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{Client, RequestBuilder, StatusCode, Url};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

const IMAGE_DIR: &str = ".tenkivo/images";
const IMAGE_REF: &str = "tenkivo-image:";
const TRANSFER_TIMEOUT: Duration = Duration::from_secs(120);

pub(super) type Assets = BTreeMap<String, Vec<u8>>;

pub(super) fn git_sha(value: &Value) -> Result<&str, String> {
    value
        .as_str()
        .filter(|sha| sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit()))
        .ok_or_else(|| "GitHub did not return a valid file revision.".into())
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn media(extension: &str) -> Option<&'static str> {
    match extension {
        "png" => Some("image/png"),
        "jpg" => Some("image/jpeg"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

fn asset_name(name: &str) -> Option<(&str, &'static str)> {
    let (hash, extension) = name.split_once('.')?;
    (hash.len() == 64
        && hash
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    .then_some((hash, media(extension)?))
}

fn visit_sources(
    document: &mut Value,
    mut visit: impl FnMut(&mut String) -> Result<(), String>,
) -> Result<(), String> {
    let Some(fields) = document.as_object_mut() else {
        return Err("Invalid project document.".into());
    };
    for field in ["pages", "trash", "pageHistory"] {
        let Some(pages) = fields.get_mut(field).and_then(Value::as_array_mut) else {
            continue;
        };
        for entry in pages {
            let page = if field == "pageHistory" {
                &mut entry["page"]
            } else {
                entry
            };
            if let Some(items) = page.get_mut("items").and_then(Value::as_array_mut) {
                for item in items {
                    if item["kind"] != "Image" {
                        continue;
                    }
                    if let Some(Value::String(source)) = item.get_mut("source") {
                        visit(source)?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub(super) fn separate(document: &str) -> Result<(String, Assets), String> {
    let mut content: Value =
        serde_json::from_str(document).map_err(|_| "Invalid project document.")?;
    let mut assets = BTreeMap::new();
    visit_sources(&mut content, |source| {
        let Some((mime, encoded)) = source
            .strip_prefix("data:")
            .and_then(|source| source.split_once(";base64,"))
        else {
            return Err("This project contains an image that is not embedded.".into());
        };
        let extension = ["png", "jpg", "gif", "webp"]
            .into_iter()
            .find(|ext| media(ext) == Some(mime))
            .ok_or("Unsupported embedded image format.")?;
        let bytes = STANDARD
            .decode(encoded)
            .map_err(|_| "An embedded image is invalid.")?;
        if bytes.len() > GITHUB_FILE_LIMIT {
            return Err("An embedded image exceeds GitHub's 100 MiB file limit.".into());
        }
        let name = format!("{}.{extension}", digest(&bytes));
        *source = format!("{IMAGE_REF}{name}");
        assets.entry(name).or_insert(bytes);
        Ok(())
    })?;
    Ok((
        serde_json::to_string(&content).map_err(|error| error.to_string())?,
        assets,
    ))
}

pub(super) async fn raw_file(request: RequestBuilder) -> Result<Vec<u8>, String> {
    let mut response = checked(
        request
            .header("Accept", "application/vnd.github.raw+json")
            .timeout(TRANSFER_TIMEOUT)
            .send()
            .await
            .map_err(connection_error)?,
    )?;
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(connection_error)? {
        if chunk.len() > GITHUB_FILE_LIMIT.saturating_sub(bytes.len()) {
            return Err("A GitHub file exceeded the 100 MiB file limit.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

pub(super) fn cache_dir(data_dir: &Path, repo: &str) -> Result<PathBuf, String> {
    repository_url(repo)?;
    let root = data_dir.join("online-image-cache");
    fs::create_dir_all(&root).map_err(|error| error.to_string())?;
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let directory = root.join(digest(repo.to_ascii_lowercase().as_bytes()));
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    if directory
        .canonicalize()
        .map_err(|error| error.to_string())?
        != directory
    {
        return Err("The project cache was redirected to another folder.".into());
    }
    Ok(directory)
}

fn cache_file(folder: &Path, name: &str, bytes: &[u8]) -> std::io::Result<()> {
    let mut draft = tempfile::NamedTempFile::new_in(folder)?;
    draft.write_all(bytes)?;
    draft
        .persist(folder.join(name))
        .map_err(|error| error.error)?;
    Ok(())
}

pub(super) fn clear_cache(data_dir: &Path, repo: &str) -> Result<u64, String> {
    let folder = cache_dir(data_dir, repo)?;
    let mut count = 0;
    for file in fs::read_dir(folder).map_err(|error| error.to_string())? {
        let file = file.map_err(|error| error.to_string())?;
        if file
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
            && file.file_name().to_str().and_then(asset_name).is_some()
        {
            match fs::remove_file(file.path()) {
                Ok(()) => count += 1,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.to_string()),
            }
        }
    }
    Ok(count)
}

pub(super) fn cache_saved(data_dir: &Path, repo: &str, assets: &Assets) {
    if assets.is_empty() {
        return;
    }
    if let Ok(folder) = cache_dir(data_dir, repo) {
        for (name, bytes) in assets {
            if !folder.join(name).is_file() {
                let _ = cache_file(&folder, name, bytes);
            }
        }
    }
}

pub(super) async fn hydrate(
    data_dir: &Path,
    repo: &str,
    client: &Client,
    token: &str,
    url: &str,
    branch: &str,
    document: &[u8],
) -> Result<String, String> {
    let mut document: Value =
        serde_json::from_slice(document).map_err(|_| "Invalid project document.")?;
    let mut names = BTreeSet::new();
    visit_sources(&mut document, |source| {
        if let Some(name) = source.strip_prefix(IMAGE_REF) {
            if asset_name(name).is_none() {
                return Err("The project has an invalid image reference.".into());
            }
            names.insert(name.to_owned());
        }
        Ok(())
    })?;
    let folder = if names.is_empty() {
        None
    } else {
        cache_dir(data_dir, repo).ok()
    };
    let mut sources = BTreeMap::new();
    for name in names {
        let (hash, mime) = asset_name(&name).unwrap();
        let cached = folder.as_ref().and_then(|folder| {
            let path = folder.join(&name);
            let metadata = fs::symlink_metadata(&path).ok()?;
            if !metadata.is_file() || metadata.len() > GITHUB_FILE_LIMIT as u64 {
                return None;
            }
            let bytes = fs::read(path).ok()?;
            (digest(&bytes) == hash).then_some(bytes)
        });
        let bytes = if let Some(bytes) = cached {
            bytes
        } else {
            let bytes = raw_file(
                client
                    .get(format!("{url}/contents/{IMAGE_DIR}/{name}"))
                    .query(&[("ref", branch)])
                    .bearer_auth(token),
            )
            .await
            .map_err(|error| format!("Could not download project image {name}. {error}"))?;
            if digest(&bytes) != hash {
                return Err("A downloaded project image did not pass verification.".into());
            }
            if let Some(folder) = &folder {
                let _ = cache_file(folder, &name, &bytes);
            }
            bytes
        };
        sources.insert(
            name,
            format!("data:{mime};base64,{}", STANDARD.encode(bytes)),
        );
    }
    visit_sources(&mut document, |source| {
        if let Some(name) = source.strip_prefix(IMAGE_REF) {
            *source = sources
                .get(name)
                .ok_or("A project image could not be loaded.")?
                .clone();
        }
        Ok(())
    })?;
    serde_json::to_string(&document).map_err(|error| error.to_string())
}

#[derive(Deserialize)]
struct Tree {
    tree: Vec<TreeEntry>,
    #[serde(default)]
    truncated: bool,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    sha: String,
    #[serde(rename = "type")]
    kind: String,
}

async fn tree(
    client: &Client,
    token: &str,
    url: &str,
    sha: &str,
) -> Result<Vec<TreeEntry>, String> {
    let data: Tree = checked(
        client
            .get(format!("{url}/git/trees/{sha}"))
            .bearer_auth(token)
            .send()
            .await
            .map_err(connection_error)?,
    )?
    .json()
    .await
    .map_err(connection_error)?;
    if data.truncated {
        return Err("GitHub did not return a complete repository directory.".into());
    }
    Ok(data.tree)
}

async fn git_write(request: RequestBuilder, body: Value) -> Result<Value, String> {
    let response = request
        .json(&body)
        .timeout(TRANSFER_TIMEOUT)
        .send()
        .await
        .map_err(connection_error)?;
    let status = response.status();
    let result: Value = response.json().await.map_err(connection_error)?;
    if !status.is_success() {
        let reason: String = result["message"]
            .as_str()
            .unwrap_or("Request rejected.")
            .chars()
            .take(300)
            .collect();
        return Err(format!(
            "GitHub rejected the save (HTTP {}). {reason}",
            status.as_u16()
        ));
    }
    Ok(result)
}

pub(super) async fn save_with_images(
    client: &Client,
    token: &str,
    url: &str,
    branch: &str,
    expected: Option<&str>,
    document: &str,
    assets: &Assets,
) -> Result<String, String> {
    let mut branch_url = Url::parse(&format!("{url}/branches")).map_err(|e| e.to_string())?;
    branch_url
        .path_segments_mut()
        .map_err(|_| "Invalid branch URL.")?
        .push(branch);
    let response = client
        .get(branch_url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(connection_error)?;
    if matches!(
        response.status(),
        StatusCode::NOT_FOUND | StatusCode::CONFLICT
    ) && expected.is_none()
    {
        return Err("Create a README in the private repository to initialize its default branch before saving a project with images.".into());
    }
    let branch_head: Value = checked(response)?.json().await.map_err(connection_error)?;
    let parent = git_sha(&branch_head["commit"]["sha"])?;
    let base_tree = git_sha(&branch_head["commit"]["commit"]["tree"]["sha"])?;
    let root = tree(client, token, url, base_tree).await?;
    let actual = root
        .iter()
        .find(|entry| entry.path == DOC && entry.kind == "blob")
        .map(|entry| entry.sha.as_str());
    if actual != expected || (expected.is_none() && root.iter().any(|entry| entry.path == DOC)) {
        return Err("GitHub already has a different project revision. Save as Local to keep your edits, then open or reload the online project.".into());
    }

    let mut stored = BTreeSet::new();
    if let Some(hidden) = root
        .iter()
        .find(|entry| entry.path == ".tenkivo" && entry.kind == "tree")
    {
        let directories = tree(client, token, url, &hidden.sha).await?;
        if let Some(images) = directories
            .iter()
            .find(|entry| entry.path == "images" && entry.kind == "tree")
        {
            stored.extend(
                tree(client, token, url, &images.sha)
                    .await?
                    .into_iter()
                    .filter(|entry| entry.kind == "blob")
                    .map(|entry| entry.path),
            );
        }
    }
    let mut entries = Vec::new();
    for (name, bytes) in assets {
        if stored.contains(name) {
            continue;
        }
        let blob = git_write(
            client.post(format!("{url}/git/blobs")).bearer_auth(token),
            json!({"content": STANDARD.encode(bytes), "encoding":"base64"}),
        )
        .await?;
        entries.push(json!({"path":format!("{IMAGE_DIR}/{name}"),"mode":"100644","type":"blob","sha":git_sha(&blob["sha"])?}));
    }
    let blob = git_write(
        client.post(format!("{url}/git/blobs")).bearer_auth(token),
        json!({"content":document,"encoding":"utf-8"}),
    )
    .await?;
    let project_sha = git_sha(&blob["sha"])?;
    entries.push(json!({"path":DOC,"mode":"100644","type":"blob","sha":project_sha}));
    let tree = git_write(
        client.post(format!("{url}/git/trees")).bearer_auth(token),
        json!({"base_tree":base_tree,"tree":entries}),
    )
    .await?;
    let commit = git_write(
        client.post(format!("{url}/git/commits")).bearer_auth(token),
        json!({"message":"Save Tenkivo project","tree":git_sha(&tree["sha"])?,"parents":[parent]}),
    )
    .await?;
    let mut reference = Url::parse(&format!("{url}/git/refs/heads")).map_err(|e| e.to_string())?;
    reference
        .path_segments_mut()
        .map_err(|_| "Invalid branch URL.")?
        .extend(branch.split('/'));
    git_write(client.patch(reference).bearer_auth(token),
        json!({"sha":git_sha(&commit["sha"])?,"force":false})).await
        .map_err(|error| format!("{error} If someone saved first, use Save as Local to keep your changes before reloading."))?;
    Ok(project_sha.to_owned())
}
