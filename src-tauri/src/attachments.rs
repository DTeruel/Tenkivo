use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::{
    fs, io,
    path::{Component, Path, PathBuf},
};

#[derive(Serialize, Deserialize)]
pub struct Attachment {
    pub path: String,
    pub local: bool,
    pub folder: bool,
}
#[derive(Serialize)]
pub struct Imported {
    name: String,
    attachment: Attachment,
    image: bool,
}

fn copy_tree(source: &Path, dest: &Path) -> io::Result<()> {
    let meta = fs::symlink_metadata(source)?;
    #[cfg(windows)]
    let linked = std::os::windows::fs::MetadataExt::file_attributes(&meta) & 0x400 != 0;
    #[cfg(not(windows))]
    let linked = meta.is_symlink();
    if linked {
        return Err(io::Error::other(
            "This folder contains a filesystem link. Use Link original to preserve it.",
        ));
    }
    if meta.is_file() {
        fs::copy(source, dest)?;
    } else if meta.is_dir() {
        fs::create_dir(dest)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &dest.join(entry.file_name()))?;
        }
    } else {
        return Err(io::Error::other("Unsupported filesystem entry."));
    }
    Ok(())
}
fn mime(path: &Path) -> Option<&'static str> {
    match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "jpg" | "jpeg" => Some("image/jpeg"),
        "png" => Some("image/png"),
        "gif" => Some("image/gif"),
        "webp" => Some("image/webp"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(windows)]
    #[test]
    #[ignore = "requires an interactive Windows Explorer session"]
    fn explorer_selects_copied_attachment() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("Design.gdstudio");
        let dir = project.with_extension("assets").join("clip folder");
        fs::create_dir_all(&dir).unwrap();
        let file = dir.join("Audio, versión 2.wav");
        fs::write(&file, []).unwrap();
        reveal_attachment(project.to_string_lossy().into_owned(), Attachment {
            local:true, folder:false, path:"clip folder/Audio, versión 2.wav".into(),
        }).unwrap();
        let result = std::process::Command::new("powershell.exe")
            .args(["-NoProfile","-Command",r#"
                $shell=New-Object -ComObject Shell.Application
                for($i=0;$i -lt 40;$i++){
                    foreach($window in $shell.Windows()){
                        if($window.Document.Folder.Self.Path -eq $env:STUDIO_TEST_FOLDER){
                            if($window.Document.SelectedItems().Item(0).Path -eq $env:STUDIO_TEST_FILE){
                                $window.Quit(); exit 0
                            }
                        }
                    }
                    Start-Sleep -Milliseconds 250
                }
                exit 1"#])
            .env("STUDIO_TEST_FOLDER",&dir).env("STUDIO_TEST_FILE",&file).status().unwrap();
        assert!(result.success(), "Explorer did not select the expected file");
    }
    #[test]
    fn audio_paths_resolve_for_linked_and_copied_recordings() {
        let tmp = tempfile::tempdir().unwrap();
        let project = tmp.path().join("Notes.gdstudio");
        let clip = tmp.path().join("Cue 01.WAV");
        fs::write(&clip, b"wave fixture").unwrap();
        for local in [false, true] {
            let project = project.to_str().unwrap();
            let imported = import(project, vec![clip.to_str().unwrap().into()], local).unwrap();
            let actual = media_file(project, &imported[0].attachment).unwrap();
            assert_eq!(actual == clip.canonicalize().unwrap(), !local);
            assert_eq!(fs::read(actual).unwrap(), b"wave fixture");
        }
    }
    #[test]
    fn copied_tree_moves_with_project_and_links_keep_original_path() {
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join("project");
        let original = tmp.path().join("references");
        fs::create_dir(&work).unwrap();
        fs::create_dir(&original).unwrap();
        fs::write(original.join("design.txt"), "draft").unwrap();
        fs::write(original.join("editor.lnk"), b"shortcut bytes").unwrap();
        let project = work.join("test.gdstudio");
        fs::write(&project, "{}").unwrap();
        let src = original.to_str().unwrap().to_string();
        let copied = import(project.to_str().unwrap(), vec![src.clone()], true)
            .unwrap()
            .remove(0);
        let linked = import(project.to_str().unwrap(), vec![src], false)
            .unwrap()
            .remove(0);
        let moved = tmp.path().join("moved");
        fs::rename(&work, &moved).unwrap();
        let new_project = moved.join("test.gdstudio");
        let local = resolve(new_project.to_str().unwrap(), &copied.attachment).unwrap();
        assert_eq!(
            fs::read(local.join("editor.lnk")).unwrap(),
            b"shortcut bytes"
        );
        fs::write(original.join("design.txt"), "updated").unwrap();
        assert_eq!(
            fs::read_to_string(local.join("design.txt")).unwrap(),
            "draft"
        );
        assert_eq!(resolve("", &linked.attachment).unwrap(), original);
        assert!(import(
            new_project.to_str().unwrap(),
            vec![moved.to_str().unwrap().into()],
            true
        )
        .is_err());
    }
}
fn import(project: &str, paths: Vec<String>, local: bool) -> Result<Vec<Imported>, String> {
    let mut root = None;
    if local {
        let project = super::project_file(project)?;
        let dir = project.with_extension("assets");
        for src in &paths {
            let src = fs::canonicalize(src).map_err(|e| e.to_string())?;
            let parent = fs::canonicalize(dir.parent().unwrap()).map_err(|e| e.to_string())?;
            if parent.join(dir.file_name().unwrap()).starts_with(&src) {
                return Err(
                    "Choose Link original for folders containing the project assets.".into(),
                );
            }
        }
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let canonical = fs::canonicalize(&dir).map_err(|e| e.to_string())?;
        for src in &paths {
            if canonical.starts_with(fs::canonicalize(src).map_err(|e| e.to_string())?) {
                return Err("Cannot copy a folder into itself.".into());
            }
        }
        root = Some(
            tempfile::Builder::new()
                .prefix("import-")
                .tempdir_in(dir)
                .map_err(|e| e.to_string())?,
        );
    }
    let mut result = Vec::new();
    for (i, raw) in paths.into_iter().enumerate() {
        let src = Path::new(&raw);
        if !src.is_absolute() {
            return Err("Expected an absolute file path.".into());
        }
        let name = src
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or(&raw)
            .to_owned();
        let folder = src.metadata().map_err(|e| e.to_string())?.is_dir();
        let stored = if let Some(tmp) = &root {
            let file = src
                .file_name()
                .ok_or("Link drive roots instead of copying them.")?;
            let group = tmp.path().join(i.to_string());
            fs::create_dir(&group)
                .and_then(|_| copy_tree(src, &group.join(file)))
                .map_err(|e| e.to_string())?;
            format!(
                "{}/{}/{}",
                tmp.path().file_name().unwrap().to_string_lossy(),
                i,
                name
            )
        } else {
            raw.clone()
        };
        result.push(Imported {
            name,
            image: !folder && mime(src).is_some(),
            attachment: Attachment {
                path: stored,
                local,
                folder,
            },
        });
    }
    if let Some(dir) = root {
        let _ = dir.keep();
    }
    Ok(result)
}
#[tauri::command]
pub async fn import_attachments(
    project: String,
    paths: Vec<String>,
    local: bool,
) -> Result<Vec<Imported>, String> {
    tauri::async_runtime::spawn_blocking(move || import(&project, paths, local))
        .await
        .map_err(|e| e.to_string())?
}
pub(super) fn resolve(project: &str, a: &Attachment) -> Result<PathBuf, String> {
    let path = Path::new(&a.path);
    if a.local {
        if !path.components().all(|c| matches!(c, Component::Normal(_)))
            || path.as_os_str().is_empty()
        {
            return Err("Invalid project attachment.".into());
        }
        Ok(super::project_file(project)?
            .with_extension("assets")
            .join(path))
    } else if path.is_absolute() {
        Ok(path.into())
    } else {
        Err("Invalid external path.".into())
    }
}
#[tauri::command]
pub async fn attachment_image(project: String, attachment: Attachment) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let p = resolve(&project, &attachment)?;
        let kind = mime(&p).ok_or("Unsupported image format.")?;
        let data = fs::read(p).map_err(|e| e.to_string())?;
        Ok(format!("data:{};base64,{}", kind, STANDARD.encode(data)))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn read_embedded_image(path: String) -> Result<String, String> {
    use std::io::Read;
    tauri::async_runtime::spawn_blocking(move || {
        let path = Path::new(&path);
        if !path.is_absolute() || !path.is_file() {
            return Err("Choose a local image file.".into());
        }
        let kind = mime(path).ok_or("Choose a PNG, JPEG, GIF or WebP image.")?;
        let max = crate::online::GITHUB_FILE_LIMIT;
        let mut bytes = Vec::new();
        fs::File::open(path).map_err(|e| e.to_string())?.take((max + 1) as u64)
            .read_to_end(&mut bytes).map_err(|e| e.to_string())?;
        if bytes.len() > max {
            return Err("This image exceeds GitHub's 100 MiB file limit.".into());
        }
        Ok(format!("data:{kind};base64,{}", STANDARD.encode(bytes)))
    }).await.map_err(|error| error.to_string())?
}
fn media_file(project: &str, attachment: &Attachment) -> Result<PathBuf, String> {
    let path = fs::canonicalize(resolve(project, attachment)?).map_err(|e| e.to_string())?;
    let extension = path
        .extension()
        .and_then(|v| v.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if attachment.folder
        || !path.is_file()
        || !matches!(
            extension.as_str(),
            "mp3"
                | "wav"
                | "m4a"
                | "aac"
                | "ogg"
                | "oga"
                | "opus"
                | "flac"
                | "weba"
                | "mp4"
                | "m4v"
                | "webm"
                | "mov"
                | "ogv"
                | "mkv"
        )
    {
        return Err("Unsupported media file.".into());
    }
    Ok(path)
}

#[tauri::command]
pub async fn attachment_media(
    app: tauri::AppHandle,
    project: String,
    attachment: Attachment,
) -> Result<PathBuf, String> {
    use tauri::Manager;
    tauri::async_runtime::spawn_blocking(move || {
        let path = media_file(&project, &attachment)?;
        app.asset_protocol_scope()
            .allow_file(&path)
            .map_err(|e| e.to_string())?;
        Ok(path)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn reveal_attachment(project: String, attachment: Attachment) -> Result<(), String> {
    let path = resolve(&project, &attachment)?;
    fs::metadata(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(windows)]
    {
        use windows::core::HSTRING;
        use windows::Win32::System::Com::{CoInitializeEx, CoTaskMemFree, CoUninitialize, COINIT_APARTMENTTHREADED};
        use windows::Win32::UI::Shell::{SHOpenFolderAndSelectItems, SHParseDisplayName};
        let absolute = std::path::absolute(path).map_err(|e| e.to_string())?;
        let name = HSTRING::from(dunce::simplified(&absolute).as_os_str());
        std::thread::spawn(move || unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok()?;
            let mut item = std::ptr::null_mut();
            let result = SHParseDisplayName(&name, None, &mut item, 0, None).and_then(|()| {
                // A full item ID with no child list opens its parent and selects the item.
                let result = SHOpenFolderAndSelectItems(item, None, 0);
                CoTaskMemFree(Some(item.cast()));
                result
            });
            CoUninitialize();
            result
        }).join().map_err(|_| "Explorer selection failed.")?.map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    std::process::Command::new("open").arg("-R").arg(&path).spawn().map_err(|e|e.to_string())?;
    #[cfg(not(any(windows,target_os = "macos")))]
    system_open(path.parent().ok_or("Missing parent folder.")?)?;
    Ok(())
}

#[tauri::command]
pub fn open_attachment(project: String, attachment: Attachment) -> Result<(), String> {
    let path = resolve(&project, &attachment)?;
    fs::metadata(&path).map_err(|e| e.to_string())?;
    system_open(&path)
}
pub(super) fn system_open(path: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "shell32")]
        extern "system" {
            fn ShellExecuteW(
                hwnd: isize,
                verb: *const u16,
                file: *const u16,
                args: *const u16,
                dir: *const u16,
                show: i32,
            ) -> isize;
        }
        let wide: Vec<_> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        let nil = std::ptr::null();
        let result = unsafe { ShellExecuteW(0, nil, wide.as_ptr(), nil, nil, 1) };
        if result <= 32 {
            return Err(format!(
                "Windows could not open the attachment ({}).",
                result
            ));
        }
    }
    #[cfg(not(windows))]
    {
        let command = if cfg!(target_os = "macos") {
            "open"
        } else {
            "xdg-open"
        };
        std::process::Command::new(command)
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
fn web_url(url: &str) -> Result<reqwest::Url, String> {
    let parsed = reqwest::Url::parse(url).map_err(|e| e.to_string())?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err("Expected a web address.".into());
    }
    Ok(parsed)
}
#[tauri::command]
pub fn open_web(url: String) -> Result<(), String> {
    system_open(Path::new(web_url(&url)?.as_str()))
}
#[tauri::command]
pub async fn web_preview(url: String) -> Result<(String, String), String> {
    use std::io::Read;
    let parsed = web_url(&url)?;
    tauri::async_runtime::spawn_blocking(move || {
        let response = reqwest::blocking::Client::builder()
            .timeout(std::time::Duration::from_secs(6))
            .build()
            .map_err(|e| e.to_string())?
            .get(parsed)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.to_string())?;
        if !response
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .contains("html")
        {
            return Ok((String::new(), String::new()));
        }
        let mut bytes = Vec::new();
        response
            .take(512 * 1024)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let doc = dom_query::Document::from(String::from_utf8_lossy(&bytes).as_ref());
        let title = doc
            .select_single("meta[property='og:title']")
            .attr("content")
            .unwrap_or_else(|| doc.select_single("title").text());
        let desc = doc
            .select_single("meta[property='og:description'],meta[name='description']")
            .attr("content")
            .unwrap_or_default();
        Ok((
            title.trim().chars().take(200).collect(),
            desc.trim().chars().take(500).collect(),
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}
