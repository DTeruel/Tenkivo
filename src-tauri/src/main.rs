#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
use std::{fs, io::Write, path::Path};
mod app_data;
mod attachments;
mod diagnostics;
mod export;
mod updates;
mod online;
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
    if file.is_absolute() && file.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("tkv") || ext.eq_ignore_ascii_case("gdstudio")) {
        Ok(file)
    } else {
        Err("Choose a Tenkivo project file (.tkv or .gdstudio).".into())
    }
}

#[tauri::command]
fn startup_project() -> Option<String> {
    let arg = std::env::args_os().nth(1)?;
    let path = std::path::absolute(arg).ok()?;
    let text = path.to_str()?;
    project_file(text).ok().map(|_| text.to_owned())
}

#[cfg(windows)]
fn register_project_files() -> std::io::Result<()> {
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    use windows::Win32::UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST};
    let exe = std::env::current_exe()?;
    let exe = dunce::simplified(&exe).display();
    let classes = RegKey::predef(HKEY_CURRENT_USER).create_subkey(r"Software\Classes")?.0;
    let prog_id = "Tenkivo.Project";
    let class = classes.create_subkey(prog_id)?.0;
    let command = class.create_subkey(r"shell\open\command")?.0;
    let extension = classes.create_subkey(".tkv")?.0;
    let open_command = format!(r#""{exe}" "%1""#);
    let current = extension.get_value::<String,_>("");
    if command.get_value::<String,_>("").ok().as_ref() == Some(&open_command) && current.is_ok() {
        return Ok(());
    }
    class.set_value("", &"Tenkivo Project")?;
    class.create_subkey("DefaultIcon")?.0.set_value("", &format!(r#""{exe}",0"#))?;
    command.set_value("", &open_command)?;
    extension.create_subkey("OpenWithProgids")?.0.set_value(prog_id, &"")?;
    if current.unwrap_or_default().is_empty() { extension.set_value("", &prog_id)?; }
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None); }
    Ok(())
}

#[tauri::command]
fn project_availability(path: String) -> Result<&'static str, String> {
    Ok(match fs::metadata(project_file(&path)?) {
        Ok(meta) if meta.is_file() => "Available",
        Ok(_) => "Missing",
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => "Missing",
        Err(error) => {
            diagnostics::error("Check project availability", error);
            "Unavailable"
        },
    })
}

#[tauri::command]
fn open_project(path: String) -> Result<String, String> {
    fs::read_to_string(project_file(&path)?).map_err(|err| err.to_string())
}

#[cfg(test)]
mod project_tests {
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
    updates::cleanup_after_update();
    #[cfg(windows)]
    {
        let register_only = std::env::args_os().nth(1).is_some_and(|arg| arg == "--register-project-files");
        if register_only || !cfg!(debug_assertions) {
            let result = register_project_files();
            if register_only { std::process::exit(if result.is_ok() { 0 } else { 1 }); }
            if let Err(error) = result {
                diagnostics::error("Register project file association", error);
            }
        }
    }
    tauri::Builder::default()
        .setup(|app| {
            use tauri::Manager;
            let local_data = app.path().local_data_dir()?;
            let session_dir = app.path().app_local_data_dir()?;
            let cache_root = local_data.join("Tenkivo").join("Cache").join("Projects");
            app_data::migrate(&local_data, &session_dir, &cache_root);
            app.manage(online::Online::new(session_dir, cache_root));

            // Move the WebView profile before WebView2 opens its storage.
            let config = app.config().app.windows.first().ok_or("Missing main window config")?;
            let window = tauri::WebviewWindowBuilder::from_config(app, config)?.build()?;
            #[cfg(windows)]
            {
                use webview2_com::{
                    AcceleratorKeyPressedEventHandler,
                    Microsoft::Web::WebView2::Win32::ICoreWebView2Settings3,
                };
                use windows::core::Interface;
                use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_J, VK_MENU, VK_SHIFT};
                window.with_webview(|view| unsafe {
                    let settings = view.controller().CoreWebView2()
                        .and_then(|webview| webview.Settings())
                        .and_then(|settings| settings.cast::<ICoreWebView2Settings3>())
                        .and_then(|settings| settings.SetAreBrowserAcceleratorKeysEnabled(false));
                    if let Err(error) = settings {
                        diagnostics::error("Disable browser shortcuts", error);
                    }
                    let handler = AcceleratorKeyPressedEventHandler::create(Box::new(|_, args| {
                        if let Some(args) = args {
                            let mut key = 0;
                            args.VirtualKey(&mut key)?;
                            if key == u32::from(VK_J.0) && GetKeyState(VK_CONTROL.0.into()) < 0
                                && GetKeyState(VK_SHIFT.0.into()) >= 0 && GetKeyState(VK_MENU.0.into()) >= 0 {
                                args.SetHandled(true)?;
                            }
                        }
                        Ok(())
                    }));
                    let mut token = 0;
                    if let Err(error) = view.controller().add_AcceleratorKeyPressed(&handler, &mut token) {
                        diagnostics::error("Register downloads shortcut handler", error);
                    }
                })?;
            }
            #[cfg(not(windows))]
            let _ = window;
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            diagnostics::take_native_diagnostics,
            online::github_client_id,
            online::online_sign_in,
            online::online_poll,
            online::online_restore,
            online::online_sign_out,
            online::online_open,
            online::online_save,
            online::open_project_cache,
            online::clear_project_cache,
            online::files::online_attachment_size,
            online::files::stage_online_attachment,
            online::files::online_attachment_path,
            online::files::save_online_project_locally,
            online::access::online_discover,
            online::access::online_stop_discovery,
            online::access::online_project_access,
            online::access::online_find_user,
            online::access::online_invite_editor,
            online::access::online_remove_editor,
            online::access::online_cancel_invitation,
            open_project,
            startup_project,
            project_availability,
            export::export_zip,
            export::export_pdf,
            export::open_pdf_location,
            attachments::import_attachments,
            attachments::clipboard_attachment_path,
            attachments::attachment_image,
            attachments::read_embedded_image,
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
