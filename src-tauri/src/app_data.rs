use std::{
    fs,
    io::{self, ErrorKind},
    path::Path,
};

const LEGACY_DIR: &str = "studio.gamedesign.workspace";

pub fn migrate(local_data: &Path, session_dir: &Path, cache_root: &Path) {
    let legacy = local_data.join(LEGACY_DIR);
    if !legacy.symlink_metadata().is_ok_and(|info| info.is_dir()) {
        return;
    }
    move_entry(
        &legacy.join("online-image-cache"),
        cache_root,
        fs::Metadata::is_dir,
    );

    #[cfg(windows)]
    if legacy != session_dir {
        move_entry(
            &legacy.join("EBWebView"),
            &session_dir.join("EBWebView"),
            fs::Metadata::is_dir,
        );
        migrate_session(&legacy, session_dir);
        // Only remove the old folder when all of its contents have moved.
        let _ = fs::remove_dir(&legacy);
    }
    #[cfg(not(windows))]
    let _ = session_dir;
}

fn move_entry(source: &Path, destination: &Path, matches_type: fn(&fs::Metadata) -> bool) {
    let result = (|| -> io::Result<()> {
        match source.symlink_metadata() {
            Ok(info) if matches_type(&info) => {}
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        }
        match destination.symlink_metadata() {
            Ok(_) => return Ok(()),
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(source, destination)
    })();
    if let Err(error) = result {
        crate::diagnostics::error(
            "Migrate application data",
            format!("{}: {error}", source.display()),
        );
    }
}

#[cfg(windows)]
fn migrate_session(legacy: &Path, session_dir: &Path) {
    let lock_path = legacy.join("github-session.lock");
    let lock = match fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
    {
        Ok(lock) if lock.try_lock().is_ok() => Some(lock),
        Ok(_) => return,
        Err(error) if error.kind() == ErrorKind::NotFound => None,
        Err(error) => {
            crate::diagnostics::error("Open legacy GitHub session lock", error);
            return;
        }
    };
    let session_path = legacy.join("github-session.dat");
    move_entry(
        &session_path,
        &session_dir.join("github-session.dat"),
        fs::Metadata::is_file,
    );
    if matches!(session_path.symlink_metadata(), Err(error) if error.kind() == ErrorKind::NotFound)
        && lock.is_some_and(|lock| lock.metadata().is_ok_and(|info| info.len() == 0))
    {
        let _ = fs::remove_file(lock_path);
    }
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn migrates_windows_profile_and_session_alongside_existing_project_cache() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join(LEGACY_DIR);
        let current = dir.path().join("Tenkivo");
        let cache = current.join("Cache").join("Projects");
        fs::create_dir_all(&cache).unwrap();
        fs::write(cache.join("cached"), b"existing cache").unwrap();
        let profile = Path::new("EBWebView").join("Default").join("Local Storage");
        fs::create_dir_all(legacy.join(&profile)).unwrap();
        fs::write(legacy.join(&profile).join("preferences"), b"settings").unwrap();
        fs::write(legacy.join("github-session.dat"), b"protected session").unwrap();
        fs::File::create(legacy.join("github-session.lock")).unwrap();

        migrate(dir.path(), &current, &cache);
        migrate(dir.path(), &current, &cache);

        assert!(!legacy.exists());
        assert_eq!(fs::read(cache.join("cached")).unwrap(), b"existing cache");
        assert_eq!(
            fs::read(current.join(&profile).join("preferences")).unwrap(),
            b"settings"
        );
        assert_eq!(
            fs::read(current.join("github-session.dat")).unwrap(),
            b"protected session"
        );
    }

    #[test]
    fn migrates_legacy_cache_without_replacing_current_profile_or_session() {
        let dir = tempfile::tempdir().unwrap();
        let legacy = dir.path().join(LEGACY_DIR);
        let current = dir.path().join("Tenkivo");
        let cache = current.join("Cache").join("Projects");
        for root in [&legacy, &current] {
            fs::create_dir_all(root.join("EBWebView")).unwrap();
        }
        fs::create_dir(legacy.join("online-image-cache")).unwrap();
        fs::write(legacy.join("online-image-cache").join("file"), b"cached").unwrap();
        fs::write(legacy.join("github-session.dat"), b"older session").unwrap();
        fs::write(current.join("github-session.dat"), b"current session").unwrap();
        fs::write(legacy.join("EBWebView").join("settings"), b"older profile").unwrap();
        fs::write(
            current.join("EBWebView").join("settings"),
            b"current profile",
        )
        .unwrap();

        migrate(dir.path(), &current, &cache);

        assert!(!legacy.join("online-image-cache").exists());
        assert_eq!(fs::read(cache.join("file")).unwrap(), b"cached");
        assert_eq!(
            fs::read(legacy.join("github-session.dat")).unwrap(),
            b"older session"
        );
        assert_eq!(
            fs::read(current.join("github-session.dat")).unwrap(),
            b"current session"
        );
        assert_eq!(
            fs::read(legacy.join("EBWebView").join("settings")).unwrap(),
            b"older profile"
        );
        assert_eq!(
            fs::read(current.join("EBWebView").join("settings")).unwrap(),
            b"current profile"
        );
    }

    #[test]
    fn windows_build_uses_the_tenkivo_local_data_directory() {
        use tauri::utils::config::{AppDirectoriesOverride, AppDirectoryOverrides};

        let context: tauri::Context<tauri::Wry> = tauri::generate_context!();
        let config = &context.config().app;
        assert!(matches!(
            &config.app_directories_override,
            Some(AppDirectoriesOverride::Directories(AppDirectoryOverrides {
                local_data: Some(path), ..
            })) if path == Path::new("$LOCALDATA/Tenkivo")
        ));
        assert!(!config.windows[0].create);
    }
}
