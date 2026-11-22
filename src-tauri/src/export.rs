use std::{
    fs,
    io::{self, Write},
    path::Path,
};
use zip::{write::SimpleFileOptions, ZipWriter};

fn output<'a>(path: &'a str, ext: &str) -> Result<&'a Path, String> {
    let p = Path::new(path);
    if !p.is_absolute()
        || !p
            .extension()
            .and_then(|v| v.to_str())
            .is_some_and(|v| v.eq_ignore_ascii_case(ext))
    {
        return Err(format!("Choose a .{ext} file."));
    }
    Ok(p)
}
fn archive_tree(zip: &mut ZipWriter<&mut fs::File>, path: &Path, name: &str) -> Result<(), String> {
    let meta = fs::symlink_metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    #[cfg(windows)]
    let linked = std::os::windows::fs::MetadataExt::file_attributes(&meta) & 0x400 != 0;
    #[cfg(not(windows))]
    let linked = meta.is_symlink();
    if linked {
        return Err(format!(
            "Filesystem link in project assets: {}",
            path.display()
        ));
    }
    let options = SimpleFileOptions::default().large_file(true);
    if meta.is_dir() {
        zip.add_directory(format!("{name}/"), options)
            .map_err(|e| e.to_string())?;
        for entry in fs::read_dir(path).map_err(|e| e.to_string())? {
            let e = entry.map_err(|e| e.to_string())?;
            archive_tree(
                zip,
                &e.path(),
                &format!("{name}/{}", e.file_name().to_string_lossy()),
            )?;
        }
    } else if meta.is_file() {
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        io::copy(&mut fs::File::open(path).map_err(|e| e.to_string())?, zip)
            .map_err(|e| e.to_string())?;
    } else {
        return Err(format!("Unsupported asset: {}", path.display()));
    }
    Ok(())
}
fn package(project: &str, destination: &str, document: &str) -> Result<(), String> {
    let src = super::project_file(project)?;
    let dest = output(destination, "zip")?;
    let assets = src.with_extension("assets");
    let parent = dest.parent().ok_or("Choose a ZIP destination.")?;
    let data: serde_json::Value = serde_json::from_str(document).map_err(|e| e.to_string())?;
    for page in ["pages", "trash"]
        .iter()
        .filter_map(|k| data[*k].as_array())
        .flatten()
    {
        for file in page["items"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|i| i.get("attachment"))
        {
            let a: super::attachments::Attachment =
                serde_json::from_value(file.clone()).map_err(|e| e.to_string())?;
            if a.local {
                let p = super::attachments::resolve(project, &a)?;
                fs::metadata(&p)
                    .map_err(|e| format!("Missing project asset: {} ({e})", p.display()))?;
            }
        }
    }
    if assets.exists()
        && fs::canonicalize(parent)
            .map_err(|e| e.to_string())?
            .starts_with(fs::canonicalize(&assets).map_err(|e| e.to_string())?)
    {
        return Err("Save the ZIP outside the project assets folder.".into());
    }
    let mut draft = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    {
        let mut zip = ZipWriter::new(draft.as_file_mut());
        zip.start_file(
            src.file_name().unwrap().to_string_lossy(),
            SimpleFileOptions::default(),
        )
        .map_err(|e| e.to_string())?;
        zip.write_all(document.as_bytes())
            .map_err(|e| e.to_string())?;
        if assets.exists() {
            archive_tree(
                &mut zip,
                &assets,
                &assets.file_name().unwrap().to_string_lossy(),
            )?;
        }
        zip.finish().map_err(|e| e.to_string())?;
    }
    draft.as_file().sync_all().map_err(|e| e.to_string())?;
    draft.persist(dest).map_err(|e| e.to_string())?;
    Ok(())
}
#[tauri::command]
pub async fn export_zip(
    project: String,
    destination: String,
    document: String,
) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || package(&project, &destination, &document))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn open_pdf_location(destination: String, folder: bool) -> Result<(), String> {
    let file = output(&destination, "pdf")?;
    let target = if folder {
        file.parent().ok_or("Invalid PDF location.")?
    } else {
        if !file.is_file() {
            return Err("The exported PDF could not be found.".into());
        }
        file
    };
    super::attachments::system_open(target)
}

#[tauri::command]
pub async fn export_pdf(webview: tauri::WebviewWindow, destination: String) -> Result<(), String> {
    #[cfg(windows)]
    {
        use webview2_com::{
            Microsoft::Web::WebView2::Win32::{ICoreWebView2Environment6, ICoreWebView2_7},
            PrintToPdfCompletedHandler,
        };
        use windows::core::{Interface, HSTRING};
        let dest = output(&destination, "pdf")?.to_path_buf();
        let dir = tempfile::tempdir_in(dest.parent().ok_or("Invalid PDF location.")?)
            .map_err(|e| e.to_string())?;
        let draft = dir.path().join("document.pdf");
        let filename = HSTRING::from(draft.as_os_str());
        let (tx, rx) = std::sync::mpsc::channel();
        webview
            .with_webview(move |w| {
                let complete = tx.clone();
                let result = unsafe {
                    (|| -> windows::core::Result<()> {
                        let settings = w
                            .environment()
                            .cast::<ICoreWebView2Environment6>()?
                            .CreatePrintSettings()?;
                        settings.SetPageWidth(297.0 / 25.4)?;
                        settings.SetPageHeight(210.0 / 25.4)?;
                        settings.SetMarginTop(0.)?;
                        settings.SetMarginBottom(0.)?;
                        settings.SetMarginLeft(0.)?;
                        settings.SetMarginRight(0.)?;
                        settings.SetShouldPrintBackgrounds(true)?;
                        settings.SetShouldPrintHeaderAndFooter(false)?;
                        let callback =
                            PrintToPdfCompletedHandler::create(Box::new(move |result, success| {
                                let result = result.map_err(|e| e.to_string()).and_then(|_| {
                                    if success {
                                        Ok(())
                                    } else {
                                        Err("PDF rendering failed.".into())
                                    }
                                });
                                let _ = complete.send(result);
                                Ok(())
                            }));
                        w.controller()
                            .CoreWebView2()?
                            .cast::<ICoreWebView2_7>()?
                            .PrintToPdf(&filename, &settings, &callback)
                    })()
                };
                if let Err(e) = result {
                    let _ = tx.send(Err(e.to_string()));
                }
            })
            .map_err(|e| e.to_string())?;
        tauri::async_runtime::spawn_blocking(move || {
            rx.recv_timeout(std::time::Duration::from_secs(300))
                .map_err(|e| e.to_string())??;
            fs::rename(draft, dest).map_err(|e| e.to_string())?;
            drop(dir);
            Ok(())
        })
        .await
        .map_err(|e| e.to_string())?
    }
    #[cfg(not(windows))]
    {
        let _ = destination;
        webview.print().map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn archive_survives_relocation_and_failed_reexport() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("Source.gdstudio");
        let dest = tmp.path().join("shared.zip");
        let assets = src.with_extension("assets");
        fs::create_dir_all(assets.join("copies/empty")).unwrap();
        fs::write(assets.join("copies/notes.txt"), b"original content").unwrap();
        fs::write(&src, "old snapshot").unwrap();
        let attachment = json!({"path":"copies/notes.txt","local":true,"folder":false});
        let manifest = json!({"version":1,"name":"Current snapshot","pages":[{"items":[]}],
            "trash":[{"items":[{"attachment":attachment}]}]})
        .to_string();
        let pack = || package(src.to_str().unwrap(), dest.to_str().unwrap(), &manifest);
        pack().unwrap();
        let moved = tmp.path().join("another computer");
        zip::ZipArchive::new(fs::File::open(&dest).unwrap())
            .unwrap()
            .extract(&moved)
            .unwrap();
        let opened = moved.join("Source.gdstudio");
        assert_eq!(fs::read_to_string(&opened).unwrap(), manifest);
        let a = serde_json::from_value(attachment).unwrap();
        assert_eq!(
            fs::read(super::super::attachments::resolve(opened.to_str().unwrap(), &a).unwrap())
                .unwrap(),
            b"original content"
        );
        assert!(moved.join("Source.assets/copies/empty").is_dir());
        let original = fs::read(&dest).unwrap();
        fs::remove_file(assets.join("copies/notes.txt")).unwrap();
        assert!(pack().unwrap_err().contains("Missing project asset"));
        assert_eq!(fs::read(dest).unwrap(), original);
    }
}
