use super::{checked, connection_error, repository_url, DOC, GITHUB_FILE_LIMIT};
use super::{files, Online};
use crate::attachments::Attachment;
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

#[derive(Default)]
pub(super) struct Assets {
    images: BTreeMap<String, Vec<u8>>,
    attachments: BTreeMap<String, Option<String>>,
    retained: BTreeSet<String>,
}

impl Assets {
    pub(super) fn is_empty(&self) -> bool {
        self.images.is_empty() && self.attachments.is_empty()
    }
}

pub(super) fn git_sha(value: &Value) -> Result<&str, String> {
    value
        .as_str()
        .filter(|sha| valid_git_sha(sha))
        .ok_or_else(|| "GitHub did not return a valid file revision.".into())
}

pub(super) fn valid_git_sha(sha: &str) -> bool {
    sha.len() == 40 && sha.bytes().all(|b| b.is_ascii_hexdigit())
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

fn parse_reference(reference: &str) -> Result<(&str, Option<&str>), String> {
    let (name, sha) = reference
        .split_once('@')
        .map_or((reference, None), |(name, sha)| (name, Some(sha)));
    if asset_name(name).is_none() || sha.is_some_and(|sha| !valid_git_sha(sha)) {
        return Err("The project has an invalid image reference.".into());
    }
    Ok((name, sha))
}

fn visit_sources(
    document: &mut Value,
    mut visit: impl FnMut(&mut String, bool) -> Result<(), String>,
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
                        visit(source, field != "pageHistory")?;
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
    let mut assets = Assets::default();
    visit_sources(&mut content, |source, retained| {
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
        if retained {
            assets.retained.insert(format!("{IMAGE_DIR}/{name}"));
        }
        assets.images.entry(name).or_insert(bytes);
        Ok(())
    })?;
    files::visit_attachments(&mut content, |value, retained| {
        let attachment: Attachment =
            serde_json::from_value(value.clone()).map_err(|_| "Invalid attachment.")?;
        let shared = files::validate_attachment(&attachment)?;
        let path = file_path(&shared.sha256);
        if retained {
            assets.retained.insert(path);
        }
        let blob = assets
            .attachments
            .entry(shared.sha256.clone())
            .or_insert(None);
        if shared.blob.is_some() {
            *blob = shared.blob.clone();
        }
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

pub(super) fn cache_dir(cache_root: &Path, repo: &str) -> Result<PathBuf, String> {
    repository_url(repo)?;
    fs::create_dir_all(cache_root).map_err(|error| error.to_string())?;
    let root = cache_root
        .canonicalize()
        .map_err(|error| error.to_string())?;
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

pub(super) fn clear_cache(cache_root: &Path, repo: &str) -> Result<u64, String> {
    let folder = cache_dir(cache_root, repo)?;
    let mut count = files::clear_file_cache(&folder)?;
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

pub(super) fn cache_saved(cache_root: &Path, repo: &str, assets: &Assets) {
    if assets.is_empty() {
        return;
    }
    if let Ok(folder) = cache_dir(cache_root, repo) {
        for (name, bytes) in &assets.images {
            if !folder.join(name).is_file() {
                let _ = cache_file(&folder, name, bytes);
            }
        }
    }
}

pub(super) async fn hydrate(
    cache_root: &Path,
    repo: &str,
    client: &Client,
    token: &str,
    url: &str,
    branch: &str,
    document: &[u8],
) -> Result<String, String> {
    let mut document: Value =
        serde_json::from_slice(document).map_err(|_| "Invalid project document.")?;
    files::visit_attachments(&mut document, |value, _| {
        let attachment: Attachment =
            serde_json::from_value(value.clone()).map_err(|_| "Invalid attachment.")?;
        let shared = files::validate_attachment(&attachment)?;
        if shared.blob.is_none() {
            return Err("A shared file is missing its GitHub revision.".into());
        }
        Ok(())
    })?;
    let mut names = BTreeMap::new();
    visit_sources(&mut document, |source, _| {
        if let Some(reference) = source.strip_prefix(IMAGE_REF) {
            let (name, sha) = parse_reference(reference)?;
            let blob = names.entry(name.to_owned()).or_insert(None);
            if let Some(sha) = sha {
                *blob = Some(sha.to_owned());
            }
        }
        Ok(())
    })?;
    let folder = if names.is_empty() {
        None
    } else {
        cache_dir(cache_root, repo).ok()
    };
    let mut sources = BTreeMap::new();
    for (name, sha) in names {
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
            let request = if let Some(sha) = sha {
                client.get(format!("{url}/git/blobs/{sha}"))
            } else {
                client
                    .get(format!("{url}/contents/{IMAGE_DIR}/{name}"))
                    .query(&[("ref", branch)])
            };
            let bytes = raw_file(request.bearer_auth(token))
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
    visit_sources(&mut document, |source, _| {
        if let Some(reference) = source.strip_prefix(IMAGE_REF) {
            let (name, _) = parse_reference(reference)?;
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

async fn publish_tree(
    client: &Client,
    token: &str,
    url: &str,
    parent: &str,
    base_tree: &str,
    entries: Vec<Value>,
    message: &str,
) -> Result<(String, String), String> {
    let tree = git_write(
        client.post(format!("{url}/git/trees")).bearer_auth(token),
        json!({"base_tree":base_tree,"tree":entries}),
    )
    .await?;
    let tree_sha = git_sha(&tree["sha"])?;
    let commit = git_write(
        client.post(format!("{url}/git/commits")).bearer_auth(token),
        json!({"message":message,"tree":tree_sha,"parents":[parent]}),
    )
    .await?;
    Ok((git_sha(&commit["sha"])?.into(), tree_sha.into()))
}

fn file_path(hash: &str) -> String {
    format!("{}/{hash}.bin", files::FILE_DIR)
}

fn asset_entry(path: &str, sha: Option<&str>) -> Value {
    json!({"path":path,"mode":"100644","type":"blob","sha":sha})
}

fn historic_references(document: &[u8]) -> Result<BTreeMap<String, String>, String> {
    let mut document: Value =
        serde_json::from_slice(document).map_err(|_| "Invalid project revision.")?;
    let mut refs = BTreeMap::new();
    visit_sources(&mut document, |source, _| {
        if let Some(reference) = source.strip_prefix(IMAGE_REF) {
            let (name, sha) = parse_reference(reference)?;
            if let Some(sha) = sha {
                refs.insert(format!("{IMAGE_DIR}/{name}"), sha.to_owned());
            }
        }
        Ok(())
    })?;
    files::visit_attachments(&mut document, |value, _| {
        let attachment: Attachment =
            serde_json::from_value(value.clone()).map_err(|_| "Invalid attachment.")?;
        let file = files::validate_attachment(&attachment)?;
        if let Some(sha) = &file.blob {
            refs.insert(file_path(&file.sha256), sha.clone());
        }
        Ok(())
    })?;
    Ok(refs)
}

pub(super) async fn save_with_assets(
    state: &Online,
    repo: &str,
    client: &Client,
    token: &str,
    url: &str,
    branch: &str,
    expected: Option<&str>,
    document: &str,
    assets: &Assets,
) -> Result<Option<String>, String> {
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
        return Err("Create a README in the private repository to initialize its default branch before saving a project with files.".into());
    }
    let branch_head: Value = checked(response)?.json().await.map_err(connection_error)?;
    let mut parent = git_sha(&branch_head["commit"]["sha"])?.to_owned();
    let mut base_tree = git_sha(&branch_head["commit"]["commit"]["tree"]["sha"])?.to_owned();
    let root = tree(client, token, url, &base_tree).await?;
    let actual = root
        .iter()
        .find(|entry| entry.path == DOC && entry.kind == "blob")
        .map(|entry| entry.sha.as_str());
    if actual != expected || (expected.is_none() && root.iter().any(|entry| entry.path == DOC)) {
        return Err("Online save conflict: GitHub already has a different project revision. Save as Local to keep your edits, then open or reload the online project.".into());
    }

    let mut stored = BTreeMap::new();
    if let Some(hidden) = root
        .iter()
        .find(|entry| entry.path == ".tenkivo" && entry.kind == "tree")
    {
        let directories = tree(client, token, url, &hidden.sha).await?;
        for (name, prefix) in [("images", IMAGE_DIR), ("files", files::FILE_DIR)] {
            if let Some(directory) = directories
                .iter()
                .find(|entry| entry.path == name && entry.kind == "tree")
            {
                stored.extend(
                    tree(client, token, url, &directory.sha)
                        .await?
                        .into_iter()
                        .filter(|entry| {
                            entry.kind == "blob"
                                && if name == "images" {
                                    asset_name(&entry.path).is_some()
                                } else {
                                    entry
                                        .path
                                        .strip_suffix(".bin")
                                        .is_some_and(files::valid_hash)
                                }
                        })
                        .map(|entry| (format!("{prefix}/{}", entry.path), entry.sha)),
                );
            }
        }
    }
    if stored.is_empty() && assets.is_empty() {
        return Ok(None);
    }
    enum Source<'a> {
        Image(&'a [u8]),
        File(&'a str, Option<&'a str>),
    }
    let mut candidates = BTreeMap::new();
    for (name, bytes) in &assets.images {
        candidates.insert(format!("{IMAGE_DIR}/{name}"), Source::Image(bytes));
    }
    for (hash, blob) in &assets.attachments {
        candidates.insert(file_path(hash), Source::File(hash, blob.as_deref()));
    }
    let mut previous_history = BTreeMap::new();
    if let Some(previous) = expected.filter(|_| {
        candidates
            .keys()
            .any(|name| !assets.retained.contains(name) && !stored.contains_key(name))
    }) {
        if !valid_git_sha(previous) {
            return Err("Invalid project revision.".into());
        }
        let exact_document = raw_file(
            client
                .get(format!("{url}/git/blobs/{previous}"))
                .bearer_auth(token),
        )
        .await?;
        previous_history = historic_references(&exact_document)?;
    }

    let mut entries: Vec<Value> = stored
        .keys()
        .filter(|name| !assets.retained.contains(*name))
        .map(|path| asset_entry(path, None))
        .collect();
    let mut resolved = BTreeMap::new();
    let mut archive_entries = Vec::new();
    for (path, source) in candidates {
        let retained = assets.retained.contains(&path);
        if let Some(sha) = stored.get(&path) {
            resolved.insert(path, sha.clone());
            continue;
        }
        if !retained {
            if let Some(sha) = previous_history.get(&path) {
                resolved.insert(path, sha.clone());
                continue;
            }
        }
        let sha = match source {
            Source::File(_, Some(sha)) => sha.to_owned(),
            _ => {
                let encoded = match source {
                    Source::Image(bytes) => STANDARD.encode(bytes),
                    Source::File(hash, _) => {
                        STANDARD.encode(files::staged_bytes(state, repo, hash)?)
                    }
                };
                let blob = git_write(
                    client.post(format!("{url}/git/blobs")).bearer_auth(token),
                    json!({"content":encoded,"encoding":"base64"}),
                )
                .await?;
                git_sha(&blob["sha"])?.to_owned()
            }
        };
        if retained {
            entries.push(asset_entry(&path, Some(&sha)));
        } else {
            archive_entries.push(asset_entry(&path, Some(&sha)));
            entries.push(asset_entry(&path, None));
        }
        resolved.insert(path, sha);
    }
    // Images known only to local page history need a parent commit to keep their blobs reachable.
    if !archive_entries.is_empty() {
        (parent, base_tree) = publish_tree(
            client,
            token,
            url,
            &parent,
            &base_tree,
            archive_entries,
            "Archive files used by Tenkivo page history",
        )
        .await?;
    }
    let mut document: Value =
        serde_json::from_str(document).map_err(|_| "Invalid project document.")?;
    visit_sources(&mut document, |source, _| {
        if let Some(name) = source.strip_prefix(IMAGE_REF) {
            let path = format!("{IMAGE_DIR}/{name}");
            if let Some(sha) = resolved
                .get(&path)
                .filter(|_| !assets.retained.contains(&path))
            {
                *source = format!("{IMAGE_REF}{name}@{sha}");
            }
        }
        Ok(())
    })?;
    files::visit_attachments(&mut document, |value, _| {
        let hash = value["shared"]["sha256"]
            .as_str()
            .ok_or("Invalid attachment.")?;
        let sha = resolved
            .get(&file_path(hash))
            .ok_or("A shared attachment could not be saved.")?;
        value["shared"]["blob"] = json!(sha);
        Ok(())
    })?;
    let document = serde_json::to_string(&document).map_err(|error| error.to_string())?;
    if document.len() > GITHUB_FILE_LIMIT {
        return Err("GitHub cannot store a project document larger than 100 MiB.".into());
    }
    let blob = git_write(
        client.post(format!("{url}/git/blobs")).bearer_auth(token),
        json!({"content":document,"encoding":"utf-8"}),
    )
    .await?;
    let project_sha = git_sha(&blob["sha"])?;
    entries.push(json!({"path":DOC,"mode":"100644","type":"blob","sha":project_sha}));
    let (commit, _) = publish_tree(
        client,
        token,
        url,
        &parent,
        &base_tree,
        entries,
        "Save Tenkivo project",
    )
    .await?;
    let mut reference = Url::parse(&format!("{url}/git/refs/heads")).map_err(|e| e.to_string())?;
    reference
        .path_segments_mut()
        .map_err(|_| "Invalid branch URL.")?
        .extend(branch.split('/'));
    git_write(client.patch(reference).bearer_auth(token),
        json!({"sha":commit,"force":false})).await
        .map_err(|error| {
            if error.starts_with("GitHub rejected the save (HTTP 409)")
                || error.to_ascii_lowercase().contains("not a fast forward")
            {
                format!("Online save conflict: {error}")
            } else {
                error
            }
        })?;
    Ok(Some(project_sha.to_owned()))
}
