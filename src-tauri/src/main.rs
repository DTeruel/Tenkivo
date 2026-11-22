#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::{fs, io::Write, path::Path};
mod attachments;
mod export;
mod updates;
#[tauri::command]
fn update_status() -> bool {
    std::env::args().nth(1).as_deref() == Some("--update-failed")
}
#[tauri::command]
fn restart_after_update(app: tauri::AppHandle) {
    app.exit(0);
}

fn project_file(path: &str) -> Result<&Path, String> {
    let file = Path::new(path);
    if file.is_absolute() && file.extension().is_some_and(|ext| ext == "gdstudio") {
        Ok(file)
    } else {
        Err("Choose a .gdstudio project file.".into())
    }
}

#[tauri::command]
fn project_availability(path: String) -> Result<&'static str, String> {
    Ok(match fs::metadata(project_file(&path)?) {
        Ok(meta) if meta.is_file() => "Available",
        Ok(_) => "Missing",
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => "Missing",
        Err(_) => "Unavailable",
    })
}

#[tauri::command]
fn open_project(path: String) -> Result<String, String> {
    fs::read_to_string(project_file(&path)?).map_err(|err| err.to_string())
}

#[tauri::command]
fn create_template(folder: String, document: String) -> Result<String, String> {
    let folder = Path::new(&folder);
    if !folder.is_absolute() || !folder.is_dir() {
        return Err("Choose an existing folder.".into());
    }
    let mut draft = tempfile::NamedTempFile::new_in(folder).map_err(|e| e.to_string())?;
    draft
        .write_all(document.as_bytes())
        .and_then(|_| draft.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    for number in 1.. {
        let name = if number == 1 {
            "Template Project.gdstudio".into()
        } else {
            format!("Template Project ({number}).gdstudio")
        };
        let path = folder.join(name);
        match draft.persist_noclobber(&path) {
            Ok(_) => return Ok(path.to_string_lossy().into_owned()),
            Err(e) if e.error.kind() == std::io::ErrorKind::AlreadyExists => draft = e.file,
            Err(e) => return Err(e.to_string()),
        }
    }
    unreachable!()
}

#[cfg(test)]
mod template_tests {
    use super::*;
    #[test]
    fn recent_file_can_be_reopened_and_saved_at_the_same_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Recent.gdstudio").to_str().unwrap().to_owned();
        assert_eq!(project_availability(path.clone()).unwrap(), "Missing");
        save_project(path.clone(), "original".into()).unwrap();
        assert_eq!(project_availability(path.clone()).unwrap(), "Available");
        assert_eq!(open_project(path.clone()).unwrap(), "original");
        save_project(path.clone(), "edited".into()).unwrap();
        assert_eq!(open_project(path.clone()).unwrap(), "edited");
        fs::remove_file(&path).unwrap();
        assert_eq!(project_availability(path).unwrap(), "Missing");
    }
    #[test]
    fn existing_projects_are_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().to_string_lossy().to_string();
        let first = create_template(folder.clone(), "one".into()).unwrap();
        let second = create_template(folder, "two".into()).unwrap();
        assert_ne!(first, second);
        assert_eq!(open_project(first).unwrap(), "one");
        assert_eq!(open_project(second).unwrap(), "two");
    }
}

#[tauri::command]
fn save_project(path: String, document: String) -> Result<(), String> {
    let target = project_file(&path)?;
    let parent = target.parent().ok_or("Invalid project location.")?;
    let mut draft = tempfile::NamedTempFile::new_in(parent).map_err(|err| err.to_string())?;
    draft
        .write_all(document.as_bytes())
        .map_err(|err| err.to_string())?;
    draft.as_file().sync_all().map_err(|err| err.to_string())?;
    draft.persist(target).map_err(|err| err.to_string())?;
    Ok(())
}

fn main() {
    if updates::apply_if_staged() {
        return;
    }
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            open_project,
            project_availability,
            create_template,
            export::export_zip,
            export::export_pdf,
            export::open_pdf_location,
            attachments::import_attachments,
            attachments::attachment_image,
            attachments::attachment_media,
            attachments::open_attachment,
            attachments::reveal_attachment,
            attachments::open_web,
            attachments::web_preview,
            save_project,
            updates::check_update,
            updates::install_update,
            restart_after_update,
            update_status
        ])
        .run(tauri::generate_context!())
        .expect("Could not start Tenkivo");
}
