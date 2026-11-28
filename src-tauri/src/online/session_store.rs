use super::SavedSession;
#[cfg(windows)]
use std::path::PathBuf;

#[cfg(windows)]
mod windows_store {
    use super::*;
    use std::{fs, io::Write};
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::{LocalFree, HLOCAL},
            Security::Cryptography::{
                CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
            },
        },
    };

    pub struct SessionFile {
        path: PathBuf,
        _lock: fs::File,
    }

    fn crypt(data: &[u8], encrypt: bool) -> Result<Vec<u8>, String> {
        let mut input = data.to_vec();
        let blob = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(input.len()).map_err(|_| "GitHub session data is too large.")?,
            pbData: input.as_mut_ptr(),
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        unsafe {
            let result = if encrypt {
                CryptProtectData(
                    &blob,
                    PCWSTR::null(),
                    None,
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            } else {
                CryptUnprotectData(
                    &blob,
                    None,
                    None,
                    None,
                    None,
                    CRYPTPROTECT_UI_FORBIDDEN,
                    &mut output,
                )
            };
            result.map_err(|_| "Windows could not protect or read the saved GitHub session.")?;
            let bytes = std::slice::from_raw_parts(output.pbData, output.cbData as usize).to_vec();
            let _ = LocalFree(Some(HLOCAL(output.pbData.cast())));
            Ok(bytes)
        }
    }

    impl SessionFile {
        pub fn open(dir: PathBuf) -> Result<Self, String> {
            fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
            let lock = fs::OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(false)
                .open(dir.join("github-session.lock"))
                .map_err(|e| e.to_string())?;
            lock.lock().map_err(|e| e.to_string())?;
            Ok(Self {
                path: dir.join("github-session.dat"),
                _lock: lock,
            })
        }

        pub fn read(&self) -> Result<Option<SavedSession>, String> {
            let encrypted = match fs::read(&self.path) {
                Ok(bytes) => bytes,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
                Err(error) => return Err(error.to_string()),
            };
            let json = crypt(&encrypted, false)?;
            serde_json::from_slice(&json)
                .map(Some)
                .map_err(|_| "The saved GitHub session is invalid. Sign in again.".into())
        }

        pub fn write(&self, session: &SavedSession) -> Result<(), String> {
            let json = serde_json::to_vec(session).map_err(|e| e.to_string())?;
            let encrypted = crypt(&json, true)?;
            let mut draft = tempfile::NamedTempFile::new_in(self.path.parent().unwrap())
                .map_err(|e| e.to_string())?;
            draft
                .write_all(&encrypted)
                .and_then(|_| draft.as_file().sync_all())
                .map_err(|e| e.to_string())?;
            draft.persist(&self.path).map_err(|e| e.to_string())?;
            Ok(())
        }

        pub fn clear(&self) -> Result<(), String> {
            match fs::remove_file(&self.path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.to_string()),
            }
        }
    }
}

#[cfg(windows)]
pub use windows_store::SessionFile;

#[cfg(not(windows))]
pub struct SessionFile;

#[cfg(not(windows))]
impl SessionFile {
    pub fn read(&self) -> Result<Option<SavedSession>, String> {
        Ok(None)
    }
    pub fn write(&self, _: &SavedSession) -> Result<(), String> {
        Ok(())
    }
    pub fn clear(&self) -> Result<(), String> {
        Ok(())
    }
}

pub async fn open(dir: &std::path::Path) -> Result<Option<SessionFile>, String> {
    #[cfg(windows)]
    {
        let dir: PathBuf = dir.to_owned();
        tauri::async_runtime::spawn_blocking(move || SessionFile::open(dir))
            .await
            .map_err(|e| e.to_string())?
            .map(Some)
    }
    #[cfg(not(windows))]
    {
        let _ = dir;
        Ok(None)
    }
}
