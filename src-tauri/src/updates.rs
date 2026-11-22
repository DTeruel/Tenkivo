use reqwest::blocking::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    env, fs,
    io::{Read, Write},
    path::Path,
    process::Command,
    time::Duration,
};

const REPOSITORY: &str = "https://api.github.com/repos/DTeruel/Tenkivo";
const LATEST: &str = "https://api.github.com/repos/DTeruel/Tenkivo/releases/latest";
const PORTABLE: &str = "Tenkivo.exe";
const LEGACY_PORTABLE: &str = "GameDesignStudio.exe";
const STAGED: &str = "Tenkivo.update.exe";
const LEGACY_STAGED: &str = "GameDesignStudio.update.exe";
const LIMIT: u64 = 128 * 1024 * 1024;

#[derive(Clone, Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
    digest: Option<String>,
    size: u64,
}
#[derive(Deserialize)]
struct Release {
    tag_name: String,
    body: Option<String>,
    assets: Vec<Asset>,
}
#[derive(Serialize)]
pub struct Notice {
    version: String,
    current: String,
    notes: String,
    portable: bool,
}

enum Lookup {
    Available(Client, Notice, Asset),
    NoUpdate(String),
}

#[derive(Serialize)]
pub struct UpdateCheck {
    notice: Option<Notice>,
    message: String,
}

fn lookup() -> Result<Lookup, String> {
    let client = Client::builder()
        .user_agent("Tenkivo")
        .timeout(Duration::from_secs(40))
        .build()
        .map_err(|e| e.to_string())?;
    let response = client
        .get(LATEST)
        .header(reqwest::header::CACHE_CONTROL, "no-cache")
        .send()
        .map_err(|e| format!("Could not reach GitHub Releases: {e}"))?;
    if response.status().as_u16() == 404 {
        let repository = client
            .get(REPOSITORY)
            .header(reqwest::header::CACHE_CONTROL, "no-cache")
            .send()
            .map_err(|e| format!("Could not check repository access: {e}"))?;
        if repository.status().as_u16() == 404 {
            return Err("GitHub returned HTTP 404 for DTeruel/Tenkivo without signing in. The update repository may be private or unavailable.".into());
        }
        repository.error_for_status().map_err(|e| e.to_string())?;
        return Ok(Lookup::NoUpdate(
            "GitHub has no published stable release for this repository.".into(),
        ));
    }
    let release: Release = response
        .error_for_status()
        .map_err(|e| e.to_string())?
        .json()
        .map_err(|e| e.to_string())?;
    let version =
        Version::parse(release.tag_name.trim_start_matches('v')).map_err(|e| e.to_string())?;
    let current = env!("CARGO_PKG_VERSION");
    if version <= Version::parse(current).unwrap() {
        return Ok(Lookup::NoUpdate(format!(
            "Running version {current}. GitHub's latest stable release is {version}."
        )));
    }
    let asset = release
        .assets
        .iter()
        .find(|a| a.name == PORTABLE)
        .or_else(|| release.assets.iter().find(|a| a.name == LEGACY_PORTABLE))
        .ok_or_else(|| format!("Release {version} has no Windows portable executable."))?;
    let expected = format!(
        "https://github.com/DTeruel/Tenkivo/releases/download/{}/{}",
        release.tag_name, asset.name
    );
    if asset.browser_download_url != expected {
        return Err("Unexpected release asset URL.".into());
    }
    let portable = cfg!(windows)
        && env::current_exe()
            .ok()
            .as_deref()
            .and_then(Path::file_name)
            .is_some_and(|v| v == PORTABLE || v == LEGACY_PORTABLE);
    Ok(Lookup::Available(
        client,
        Notice {
            version: version.to_string(),
            current: current.into(),
            notes: release.body.unwrap_or_default(),
            portable,
        },
        asset.clone(),
    ))
}

#[tauri::command]
pub async fn check_update() -> Result<UpdateCheck, String> {
    tauri::async_runtime::spawn_blocking(|| match lookup()? {
        Lookup::Available(_, notice, asset) => Ok(UpdateCheck {
            message: format!(
                "Running version {}. GitHub offers {} ({}).",
                notice.current, notice.version, asset.name
            ),
            notice: Some(notice),
        }),
        Lookup::NoUpdate(message) => Ok(UpdateCheck {
            notice: None,
            message,
        }),
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn install_update(version: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let Lookup::Available(client, notice, asset) = lookup()? else {
            return Err("Release unavailable.".into());
        };
        if !notice.portable || notice.version != version {
            return Err("Check for updates again.".into());
        }
        if asset.size == 0 || asset.size > LIMIT {
            return Err("Unexpected download size.".into());
        }
        let executable = env::current_exe().map_err(|e| e.to_string())?;
        let stage_name = if executable
            .file_name()
            .is_some_and(|name| name == LEGACY_PORTABLE)
        {
            LEGACY_STAGED
        } else {
            STAGED
        };
        let dest = executable.with_file_name(stage_name);
        let dir = dest.parent().ok_or("Missing application directory.")?;
        let mut file = tempfile::NamedTempFile::new_in(dir).map_err(|e| e.to_string())?;
        let response = client
            .get(asset.browser_download_url)
            .send()
            .and_then(|r| r.error_for_status())
            .map_err(|e| e.to_string())?;
        let mut bytes = Vec::new();
        response
            .take(asset.size + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        let digest = format!("sha256:{:x}", Sha256::digest(&bytes));
        if bytes.len() as u64 != asset.size || Some(digest) != asset.digest {
            return Err("Download verification failed.".into());
        }
        file.write_all(&bytes)
            .and_then(|_| file.as_file().sync_all())
            .map_err(|e| e.to_string())?;
        file.persist(&dest).map_err(|e| e.to_string())?;
        if let Err(error) = Command::new(&dest).arg("--apply-portable-update").spawn() {
            let _ = fs::remove_file(&dest);
            return Err(error.to_string());
        }
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

// The staged executable waits for Windows to release the running executable.
pub fn apply_if_staged() -> bool {
    if env::args().nth(1).as_deref() != Some("--apply-portable-update") {
        return false;
    }
    let Ok(source) = env::current_exe() else {
        return true;
    };
    let target_name = match source.file_name().and_then(|name| name.to_str()) {
        Some(STAGED) => PORTABLE,
        Some(LEGACY_STAGED) => LEGACY_PORTABLE,
        _ => return true,
    };
    let target = source.with_file_name(target_name);
    let replace = || -> std::io::Result<()> {
        let draft = tempfile::NamedTempFile::new_in(source.parent().unwrap())?;
        fs::copy(&source, draft.path())?;
        draft.as_file().sync_all()?;
        for _ in 0..150 {
            if fs::rename(draft.path(), &target).is_ok() {
                return Ok(());
            }
            std::thread::sleep(Duration::from_millis(200));
        }
        Err(std::io::Error::other("Portable executable is in use."))
    };
    let mut restart = Command::new(&target);
    if replace().is_err() {
        restart.arg("--update-failed");
    } else {
        restart.arg("--portable-update-complete");
    }
    let _ = restart.spawn();
    true
}

// Windows releases the staged image when the updater exits after spawning us.
pub fn cleanup_after_update() {
    if env::args_os().nth(1).as_deref() != Some(std::ffi::OsStr::new("--portable-update-complete"))
    {
        return;
    }
    let Ok(executable) = env::current_exe() else {
        return;
    };
    let staged_name = match executable.file_name().and_then(|name| name.to_str()) {
        Some(PORTABLE) => STAGED,
        Some(LEGACY_PORTABLE) => LEGACY_STAGED,
        _ => return,
    };
    let staged = executable.with_file_name(staged_name);
    for _ in 0..50 {
        match fs::remove_file(&staged) {
            Ok(()) => return,
            Err(error) if matches!(error.raw_os_error(), Some(5 | 32 | 33)) => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => return,
        }
    }
}
