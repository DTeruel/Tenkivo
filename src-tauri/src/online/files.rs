use super::{client, images, repository_url, token, Online, GITHUB_FILE_LIMIT};
use crate::attachments::Attachment;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
use tauri::State;

pub(super) const FILE_DIR: &str = ".tenkivo/files";

#[derive(Clone, Serialize, Deserialize)]
pub struct SharedFile {
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<String>,
}

fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

pub(super) fn valid_hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn validate_name(value: &str) -> Result<(), String> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.ends_with(['.', ' '])
        || value
            .chars()
            .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
        || value.encode_utf16().count() > 255
    {
        return Err("Invalid shared attachment filename.".into());
    }
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || ["COM", "LPT"].iter().any(|prefix| {
        stem.strip_prefix(prefix).is_some_and(|n| {
            matches!(
                n,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    }) {
        return Err("This filename is reserved by Windows.".into());
    }
    Ok(())
}

pub(super) fn validate_attachment(attachment: &Attachment) -> Result<&SharedFile, String> {
    validate_name(&attachment.path)?;
    let shared = attachment
        .shared
        .as_ref()
        .ok_or("Expected a shared attachment.")?;
    if !attachment.local
        || attachment.folder
        || !valid_hash(&shared.sha256)
        || shared
            .blob
            .as_deref()
            .is_some_and(|blob| !images::valid_git_sha(blob))
    {
        return Err("Invalid shared attachment.".into());
    }
    Ok(shared)
}

pub(super) fn visit_attachments(
    document: &mut Value,
    mut visit: impl FnMut(&mut Value, bool) -> Result<(), String>,
) -> Result<(), String> {
    let fields = document
        .as_object_mut()
        .ok_or("Invalid project document.")?;
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
                    if let Some(attachment) = item.get_mut("attachment") {
                        visit(attachment, field != "pageHistory")?;
                    }
                }
            }
        }
    }
    Ok(())
}

fn local_file(path: &Path) -> Result<fs::Metadata, String> {
    if !path.is_absolute() {
        return Err("Choose a local file.".into());
    }
    let metadata = fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("Choose a file. Online folder uploads are unavailable.".into());
    }
    if metadata.len() > GITHUB_FILE_LIMIT as u64 {
        return Err("This file exceeds GitHub's 100 MiB file limit.".into());
    }
    Ok(metadata)
}

fn read_bytes(path: &Path) -> Result<Vec<u8>, String> {
    local_file(path)?;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take((GITHUB_FILE_LIMIT + 1) as u64)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() > GITHUB_FILE_LIMIT {
        return Err("This file exceeds GitHub's 100 MiB file limit.".into());
    }
    Ok(bytes)
}

fn child_dir(root: &Path, name: &str) -> Result<PathBuf, String> {
    let root = root.canonicalize().map_err(|e| e.to_string())?;
    let dir = root.join(name);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    if dir.canonicalize().map_err(|e| e.to_string())? != dir {
        return Err("The attachment folder was redirected.".into());
    }
    Ok(dir)
}

fn stage_dir(state: &Online, repo: &str) -> Result<PathBuf, String> {
    repository_url(repo)?;
    let mut staging = state.3.lock().unwrap();
    if staging.is_none() {
        *staging = Some(tempfile::tempdir().map_err(|e| e.to_string())?);
    }
    child_dir(
        staging.as_ref().unwrap().path(),
        &hash(repo.to_ascii_lowercase().as_bytes()),
    )
}

fn staged_file(state: &Online, repo: &str, sha: &str) -> Result<PathBuf, String> {
    if !valid_hash(sha) {
        return Err("Invalid attachment checksum.".into());
    }
    Ok(stage_dir(state, repo)?.join(sha))
}

fn checked_bytes(path: &Path, sha: &str) -> Result<Vec<u8>, String> {
    if !fs::symlink_metadata(path).is_ok_and(|metadata| metadata.is_file()) {
        return Err("The file could not be read.".into());
    }
    let bytes = read_bytes(path)?;
    if hash(&bytes) != sha {
        return Err("The file checksum does not match.".into());
    }
    Ok(bytes)
}

pub(super) fn staged_bytes(state: &Online, repo: &str, sha: &str) -> Result<Vec<u8>, String> {
    checked_bytes(&staged_file(state, repo, sha)?, sha).map_err(|e| {
        format!("Could not read the pending attachment. Add the original file again. {e}")
    })
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut temporary =
        tempfile::NamedTempFile::new_in(path.parent().ok_or("Invalid attachment destination.")?)
            .map_err(|e| e.to_string())?;
    temporary.write_all(bytes).map_err(|e| e.to_string())?;
    temporary.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn online_attachment_size(path: String) -> Result<u64, String> {
    Ok(local_file(Path::new(&path))?.len())
}

#[tauri::command]
pub async fn stage_online_attachment(
    state: State<'_, Online>,
    repo: String,
    path: String,
) -> Result<Attachment, String> {
    let path = PathBuf::from(path);
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or("Invalid filename.")?
        .to_owned();
    validate_name(&name)?;
    let bytes = tauri::async_runtime::spawn_blocking(move || read_bytes(&path))
        .await
        .map_err(|e| e.to_string())??;
    let sha256 = hash(&bytes);
    let staged = staged_file(&state, &repo, &sha256)?;
    if !staged.is_file() {
        write_file(&staged, &bytes)?;
    }
    Ok(Attachment {
        path: name,
        local: true,
        folder: false,
        shared: Some(SharedFile { sha256, blob: None }),
    })
}

async fn materialize(
    state: &Online,
    repo: &str,
    attachment: &Attachment,
) -> Result<PathBuf, String> {
    let shared = validate_attachment(attachment)?;
    let cache = images::cache_dir(&state.1, repo)?;
    let folder = child_dir(&child_dir(&cache, "files")?, &shared.sha256)?;
    let destination = folder.join(&attachment.path);
    if checked_bytes(&destination, &shared.sha256).is_ok() {
        return Ok(destination);
    }
    let staged = staged_file(state, repo, &shared.sha256)?;
    let bytes = if staged.is_file() {
        checked_bytes(&staged, &shared.sha256)?
    } else {
        let sha = shared
            .blob
            .as_deref()
            .ok_or("This file has not been saved online yet.")?;
        let url = repository_url(repo)?;
        let auth = token(state).await?;
        let result = images::raw_file(
            client()?
                .get(format!("{url}/git/blobs/{sha}"))
                .bearer_auth(&auth),
        )
        .await?;
        if hash(&result) != shared.sha256 {
            return Err("The downloaded file checksum does not match.".into());
        }
        result
    };
    write_file(&destination, &bytes)?;
    Ok(destination)
}

#[tauri::command]
pub async fn online_attachment_path(
    state: State<'_, Online>,
    repo: String,
    attachment: Attachment,
) -> Result<PathBuf, String> {
    materialize(&state, &repo, &attachment).await
}

#[tauri::command]
pub async fn save_online_project_locally(
    state: State<'_, Online>,
    repo: String,
    path: String,
    document: String,
) -> Result<String, String> {
    let target = crate::project_file(&path)?;
    let mut document: Value =
        serde_json::from_str(&document).map_err(|_| "Invalid project document.")?;
    let mut attachments = BTreeMap::new();
    visit_attachments(&mut document, |value, _| {
        let attachment: Attachment =
            serde_json::from_value(value.clone()).map_err(|_| "Invalid attachment.")?;
        let shared = validate_attachment(&attachment)?;
        attachments
            .entry((shared.sha256.clone(), attachment.path.clone()))
            .or_insert(attachment);
        Ok(())
    })?;
    let mut locations = BTreeMap::new();
    let mut transfer = None;
    if !attachments.is_empty() {
        let assets = target.with_extension("assets");
        fs::create_dir_all(&assets).map_err(|e| e.to_string())?;
        transfer = Some(
            tempfile::Builder::new()
                .prefix("shared-")
                .tempdir_in(assets)
                .map_err(|e| e.to_string())?,
        );
        let folder = transfer.as_ref().unwrap().path();
        let parent = folder
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Invalid asset folder.")?;
        for (index, (key, attachment)) in attachments.iter().enumerate() {
            let src = materialize(&state, &repo, attachment).await?;
            let index = index.to_string();
            let dest = folder.join(&index);
            fs::create_dir(&dest)
                .and_then(|_| fs::copy(src, dest.join(&attachment.path)))
                .map_err(|e| e.to_string())?;
            locations.insert(key.clone(), format!("{parent}/{index}/{}", attachment.path));
        }
        visit_attachments(&mut document, |value, _| {
            let hash = value["shared"]["sha256"]
                .as_str()
                .ok_or("Invalid attachment.")?;
            let name = value["path"]
                .as_str()
                .ok_or("Invalid attachment filename.")?;
            let location = locations
                .get(&(hash.to_owned(), name.to_owned()))
                .ok_or("The local copy is missing an attachment.")?;
            value["path"] = Value::String(location.clone());
            value
                .as_object_mut()
                .ok_or("Invalid attachment.")?
                .remove("shared");
            Ok(())
        })?;
    }
    let document = serde_json::to_string(&document).map_err(|e| e.to_string())?;
    crate::save_project(path, document.clone())?;
    if let Some(transfer) = transfer {
        let _ = transfer.keep();
    }
    Ok(document)
}

pub(super) fn clear_file_cache(cache: &Path) -> Result<u64, String> {
    if !cache.join("files").exists() {
        return Ok(0);
    }
    let files = child_dir(cache, "files")?;
    let mut count = 0;
    for item in fs::read_dir(&files).map_err(|e| e.to_string())? {
        let item = item.map_err(|e| e.to_string())?;
        let name = item.file_name();
        let Some(name) = name.to_str().filter(|name| valid_hash(name)) else {
            continue;
        };
        if !item.file_type().map_err(|e| e.to_string())?.is_dir() {
            continue;
        }
        let directory = child_dir(&files, name)?;
        for file in fs::read_dir(&directory).map_err(|e| e.to_string())? {
            let file = file.map_err(|e| e.to_string())?;
            if file.file_type().map_err(|e| e.to_string())?.is_file() {
                match fs::remove_file(file.path()) {
                    Ok(()) => count += 1,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(e.to_string()),
                }
            }
        }
        let _ = fs::remove_dir(directory);
    }
    let _ = fs::remove_dir(files);
    Ok(count)
}
