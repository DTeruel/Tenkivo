use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::{
    path::PathBuf,
    sync::{atomic::AtomicU64, Mutex},
};
use tauri::State;

pub mod access;
pub(super) mod files;
mod images;
mod session_store;

const API: &str = "https://api.github.com";
const DOC: &str = "project.tkv";
pub(super) const GITHUB_FILE_LIMIT: usize = 100 * 1024 * 1024;
const TENKIVO_GITHUB_CLIENT_ID: &str = "Iv23liFFdqx1jchyfonO";

#[tauri::command]
pub fn github_client_id() -> Option<&'static str> {
    option_env!("TENKIVO_GITHUB_CLIENT_ID")
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .or(Some(TENKIVO_GITHUB_CLIENT_ID))
}

pub struct Online {
    session: Mutex<Session>,
    session_dir: PathBuf,
    cache_root: PathBuf,
    discovery_request: AtomicU64,
    staging: Mutex<Option<tempfile::TempDir>>,
}

impl Online {
    pub fn new(session_dir: PathBuf, cache_root: PathBuf) -> Self {
        images::migrate_cache(&session_dir.join("online-image-cache"), &cache_root);
        Self {
            session: Mutex::new(Session::default()),
            session_dir,
            cache_root,
            discovery_request: AtomicU64::new(0),
            staging: Mutex::new(None),
        }
    }
}

#[derive(Default)]
struct Session {
    generation: u64,
    pending: Option<Device>,
    credentials: Option<SavedSession>,
}

#[derive(Clone)]
struct Device {
    client_id: String,
    code: String,
    interval: u64,
    expires: Instant,
    next_poll: Instant,
}

#[derive(Deserialize)]
struct DeviceResponse {
    device_code: String,
    user_code: String,
    interval: u64,
    expires_in: u64,
}

#[derive(Serialize)]
pub struct DevicePrompt {
    user_code: String,
    interval: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Account {
    id: u64,
    login: String,
}

#[derive(Clone, Serialize, Deserialize)]
struct SavedSession {
    account: Account,
    client_id: String,
    access_token: String,
    refresh_token: Option<String>,
    expires_at: Option<u64>,
}

#[derive(Deserialize)]
struct Grant {
    access_token: String,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
}

impl Grant {
    fn into_session(self, account: Account, client_id: String) -> SavedSession {
        SavedSession {
            account,
            client_id,
            access_token: self.access_token,
            refresh_token: self.refresh_token,
            expires_at: self
                .expires_in
                .map(|seconds| now_seconds().saturating_add(seconds)),
        }
    }
}

fn now_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

#[derive(Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum Poll {
    Pending { interval: u64 },
    SignedIn { account: Account },
}

fn client() -> Result<Client, String> {
    Client::builder()
        .user_agent("Tenkivo")
        .timeout(Duration::from_secs(25))
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|_| "Could not start the GitHub connection.".into())
}

fn failure(status: StatusCode) -> String {
    match status {
        StatusCode::UNAUTHORIZED => "Your GitHub session expired. Sign in again.".into(),
        StatusCode::FORBIDDEN => {
            "GitHub denied this action. Check the app installation and your repository permissions."
                .into()
        }
        StatusCode::NOT_FOUND => {
            "GitHub could not find this repository or file, or your account cannot access it."
                .into()
        }
        _ => format!("GitHub returned HTTP {}.", status.as_u16()),
    }
}

fn checked(response: Response) -> Result<Response, String> {
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(failure(response.status()))
    }
}

fn connection_error(_: reqwest::Error) -> String {
    "Could not complete the request to GitHub. Check your connection and try again.".into()
}

#[tauri::command]
pub async fn online_sign_in(
    state: State<'_, Online>,
    client_id: String,
) -> Result<DevicePrompt, String> {
    let client_id = client_id.trim().to_string();
    if client_id.len() < 16
        || client_id.len() > 100
        || !client_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-'))
    {
        return Err("Copy the Client ID from your GitHub App settings. Your GitHub username cannot be used here.".into());
    }
    let generation = {
        let mut session = state.session.lock().unwrap();
        session.generation += 1;
        session.pending = None;
        session.generation
    };
    let response = client()?
        .post("https://github.com/login/device/code")
        .header("Accept", "application/json")
        .query(&[
            ("client_id", client_id.as_str()),
            ("scope", "offline_access"),
        ])
        .send()
        .await
        .map_err(connection_error)?;
    let status = response.status();
    if matches!(
        status,
        StatusCode::BAD_REQUEST
            | StatusCode::UNAUTHORIZED
            | StatusCode::NOT_FOUND
            | StatusCode::UNPROCESSABLE_ENTITY
    ) {
        return Err("GitHub did not recognize this sign-in configuration. Check the GitHub App's Client ID and enable Device Flow.".into());
    }
    if !status.is_success() {
        return Err(format!(
            "GitHub could not start device sign-in (HTTP {}). Check Device Flow and try again.",
            status.as_u16()
        ));
    }
    let device: DeviceResponse = response.json().await.map_err(|_| {
        "GitHub could not start sign-in. Check the Client ID and enable Device Flow for the GitHub App.".to_string()
    })?;
    let now = Instant::now();
    let interval = device.interval.clamp(1, 120);
    let mut session = state.session.lock().unwrap();
    if session.generation != generation {
        return Err("Sign-in was cancelled.".into());
    }
    session.pending = Some(Device {
        client_id,
        code: device.device_code,
        interval,
        expires: now + Duration::from_secs(device.expires_in.min(1800)),
        next_poll: now + Duration::from_secs(interval),
    });
    Ok(DevicePrompt {
        user_code: device.user_code,
        interval,
    })
}

#[tauri::command]
pub async fn online_poll(state: State<'_, Online>) -> Result<Poll, String> {
    let (generation, device) = {
        let mut session = state.session.lock().unwrap();
        let generation = session.generation;
        let device = session
            .pending
            .as_mut()
            .ok_or("Start GitHub sign-in first.")?;
        let now = Instant::now();
        if now >= device.expires {
            session.pending = None;
            return Err("The GitHub sign-in code expired. Request a new code.".into());
        }
        if now < device.next_poll {
            return Ok(Poll::Pending {
                interval: device.interval,
            });
        }
        device.next_poll = now + Duration::from_secs(device.interval);
        (generation, device.clone())
    };
    let client = client()?;
    let response = checked(
        client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", device.client_id.as_str()),
                ("device_code", device.code.as_str()),
                ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
            ])
            .send()
            .await
            .map_err(connection_error)?,
    )?;
    let data: Value = response.json().await.map_err(connection_error)?;
    if data["access_token"].is_string() {
        let grant: Grant = serde_json::from_value(data)
            .map_err(|_| "GitHub returned an invalid sign-in response.")?;
        let account: Account = checked(
            client
                .get(format!("{API}/user"))
                .bearer_auth(&grant.access_token)
                .send()
                .await
                .map_err(connection_error)?,
        )?
        .json()
        .await
        .map_err(connection_error)?;
        let credentials = grant.into_session(account.clone(), device.client_id);
        let file = session_store::open(&state.session_dir).await?;
        let mut session = state.session.lock().unwrap();
        if session.generation != generation {
            return Err("Sign-in was cancelled.".into());
        }
        if let Some(file) = &file {
            file.write(&credentials)?;
        }
        session.credentials = Some(credentials);
        session.pending = None;
        return Ok(Poll::SignedIn { account });
    }
    let mut session = state.session.lock().unwrap();
    if session.generation != generation {
        return Err("Sign-in was cancelled.".into());
    }
    match data["error"].as_str() {
        Some("authorization_pending") => Ok(Poll::Pending {
            interval: device.interval,
        }),
        Some("slow_down") => {
            let interval = device.interval + 5;
            if let Some(device) = session.pending.as_mut() {
                device.interval = interval;
                device.next_poll = Instant::now() + Duration::from_secs(interval);
            }
            Ok(Poll::Pending { interval })
        }
        _ => {
            session.pending = None;
            Err("GitHub sign-in did not complete. Request a new code.".into())
        }
    }
}

#[tauri::command]
pub async fn online_restore(state: State<'_, Online>) -> Result<Option<Account>, String> {
    let Some(file) = session_store::open(&state.session_dir).await? else {
        return Ok(None);
    };
    let mut session = state.session.lock().unwrap();
    if session.pending.is_some() || session.credentials.is_some() {
        return Ok(session
            .credentials
            .as_ref()
            .map(|current| current.account.clone()));
    }
    session.credentials = file.read()?;
    Ok(session
        .credentials
        .as_ref()
        .map(|current| current.account.clone()))
}

#[tauri::command]
pub async fn online_sign_out(state: State<'_, Online>) -> Result<(), String> {
    let file = session_store::open(&state.session_dir).await?;
    let mut session = state.session.lock().unwrap();
    if let Some(file) = &file {
        file.clear()?;
    }
    *session = Session {
        generation: session.generation + 1,
        ..Session::default()
    };
    Ok(())
}

async fn token(state: &Online) -> Result<String, String> {
    let file = session_store::open(&state.session_dir).await?;
    let (generation, mut credentials) = {
        let session = state.session.lock().unwrap();
        (
            session.generation,
            session
                .credentials
                .clone()
                .ok_or("Sign in with GitHub first.")?,
        )
    };
    if let Some(file) = &file {
        let saved = file
            .read()?
            .ok_or("The GitHub session was signed out. Sign in again.")?;
        if saved.account.id != credentials.account.id || saved.client_id != credentials.client_id {
            return Err("The GitHub account changed in another window. Sign in again.".into());
        }
        credentials = saved;
    }

    if credentials
        .expires_at
        .is_some_and(|until| until <= now_seconds().saturating_add(120))
    {
        let refresh = credentials
            .refresh_token
            .as_deref()
            .ok_or("Your GitHub session expired. Sign in again.")?;
        let response = client()?
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .form(&[
                ("client_id", credentials.client_id.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh),
            ])
            .send()
            .await
            .map_err(connection_error)?;
        let data: Value = checked(response)?.json().await.map_err(connection_error)?;
        if !data["access_token"].is_string() {
            return Err("GitHub could not renew this session. Sign out and sign in again.".into());
        }
        let grant: Grant = serde_json::from_value(data)
            .map_err(|_| "GitHub returned an invalid session renewal response.")?;
        credentials = grant.into_session(credentials.account, credentials.client_id);
        if let Some(file) = &file {
            file.write(&credentials)?;
        }
    }
    let mut session = state.session.lock().unwrap();
    if generation != session.generation {
        return Err("The GitHub session changed. Try again.".into());
    }
    let access_token = credentials.access_token.clone();
    session.credentials = Some(credentials);
    Ok(access_token)
}

fn repository_url(repo: &str) -> Result<String, String> {
    let (owner, name) = repo
        .split_once('/')
        .ok_or("Enter the repository as owner/name.")?;
    if ![owner, name].iter().all(|part| {
        !part.is_empty()
            && *part != "."
            && *part != ".."
            && part
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'-' | b'_'))
    }) {
        return Err("Enter the repository as owner/name.".into());
    }
    Ok(format!("{API}/repos/{owner}/{name}"))
}

#[derive(Deserialize)]
struct Repository {
    full_name: String,
    private: bool,
    default_branch: String,
    permissions: Option<Permissions>,
    owner: Account,
}

#[derive(Deserialize)]
struct Permissions {
    push: bool,
    #[serde(default)]
    admin: bool,
}

async fn repository(client: &Client, token: &str, url: &str) -> Result<Repository, String> {
    let response = client
        .get(url)
        .bearer_auth(token)
        .send()
        .await
        .map_err(connection_error)?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err("GitHub did not grant Tenkivo access to this repository. Check owner/name and install the Tenkivo GitHub App on this specific repository under its owner account or organization.".into());
    }
    let details: Repository = checked(response)?.json().await.map_err(connection_error)?;
    if !details.private {
        return Err("Choose a private GitHub repository for your shared project.".into());
    }
    Ok(details)
}

#[derive(Serialize)]
pub struct RemoteProject {
    document: String,
    sha: String,
    branch: String,
    writable: bool,
}

#[tauri::command]
pub async fn online_open(
    state: State<'_, Online>,
    repo: String,
    branch: Option<String>,
) -> Result<RemoteProject, String> {
    let token = token(&state).await?;
    let client = client()?;
    let url = repository_url(&repo)?;
    let details = repository(&client, &token, &url).await?;
    let branch = branch.unwrap_or(details.default_branch);
    let response = client
        .get(format!("{url}/contents/{DOC}"))
        .query(&[("ref", &branch)])
        .header("Accept", "application/vnd.github.object+json")
        .bearer_auth(&token)
        .send()
        .await
        .map_err(connection_error)?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err(
            "There is no readable project.tkv on this branch. Create an online project or use Save as Online first. If the file already exists, check the GitHub App's Contents permission.".into(),
        );
    }
    let content: Value = checked(response)?.json().await.map_err(connection_error)?;
    if content["type"] != "file" {
        return Err("The online project is not a readable file.".into());
    }
    let sha = images::git_sha(&content["sha"])?;
    let decoded = if content["encoding"] == "base64" {
        let encoded = content["content"]
            .as_str()
            .ok_or("GitHub did not return project contents.")?;
        STANDARD
            .decode(encoded.split_whitespace().collect::<String>())
            .map_err(|_| "GitHub returned an invalid project file.")?
    } else {
        images::raw_file(
            client
                .get(format!("{url}/git/blobs/{sha}"))
                .bearer_auth(&token),
        )
        .await?
    };
    let document = images::hydrate(
        &state.cache_root,
        &details.full_name,
        &client,
        &token,
        &url,
        &branch,
        &decoded,
    )
    .await?;
    Ok(RemoteProject {
        document,
        sha: sha.into(),
        branch,
        writable: details.permissions.is_some_and(|p| p.push),
    })
}

#[derive(Serialize)]
pub struct SavedProject {
    sha: String,
    branch: String,
}

#[tauri::command]
pub async fn online_save(
    state: State<'_, Online>,
    repo: String,
    branch: Option<String>,
    sha: Option<String>,
    document: String,
) -> Result<SavedProject, String> {
    let token = token(&state).await?;
    let client = client()?;
    let url = repository_url(&repo)?;
    let details = repository(&client, &token, &url).await?;
    if details.permissions.is_some_and(|p| !p.push) {
        return Err("Your GitHub account has read-only access to this repository.".into());
    }
    let use_default_branch = branch.is_none();
    let branch = branch.unwrap_or(details.default_branch);
    let (document, assets) = images::separate(&document)?;
    if document.len() > GITHUB_FILE_LIMIT {
        return Err("GitHub cannot store a project document larger than 100 MiB. This limit applies to the document file only; images are stored separately.".into());
    }
    if !assets.is_empty() || sha.is_some() {
        let project_sha = images::save_with_assets(
            &state,
            &repo,
            &client,
            &token,
            &url,
            &branch,
            sha.as_deref(),
            &document,
            &assets,
        )
        .await?;
        if let Some(project_sha) = project_sha {
            images::cache_saved(&state.cache_root, &details.full_name, &assets);
            return Ok(SavedProject {
                sha: project_sha,
                branch,
            });
        }
    }
    let mut body = json!({"message":"Save Tenkivo project","content":STANDARD.encode(document)});
    if !use_default_branch {
        body["branch"] = json!(branch);
    }
    if let Some(ref sha) = sha {
        body["sha"] = json!(sha);
    }
    let response = client
        .put(format!("{url}/contents/{DOC}"))
        .bearer_auth(&token)
        .timeout(Duration::from_secs(120))
        .json(&body)
        .send()
        .await
        .map_err(connection_error)?;
    let status = response.status();
    if !status.is_success() {
        let data = response.json::<Value>().await.unwrap_or_default();
        let details: String = data["message"]
            .as_str()
            .unwrap_or("")
            .chars()
            .take(300)
            .collect();
        let hint = match status {
            StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY if sha.is_some() =>
                "Use File → Save as Local to preserve your changes before reopening the online project. Also check whether the branch allows direct writes.",
            StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY =>
                "If project.tkv exists, open it instead. For an empty repository, add a README to initialize the default branch and retry.",
            StatusCode::FORBIDDEN | StatusCode::NOT_FOUND =>
                "Check that the Tenkivo GitHub App is installed on this repository with Contents set to Read and write, and that your account can write to its branch.",
            _ => "Check the GitHub repository settings and try again.",
        };
        return Err(format!(
            "GitHub could not save {DOC} (HTTP {}). {details} {hint}",
            status.as_u16()
        ));
    }
    let result: Value = response.json().await.map_err(connection_error)?;
    Ok(SavedProject {
        sha: result["content"]["sha"]
            .as_str()
            .ok_or("GitHub did not return the saved revision.")?
            .into(),
        branch,
    })
}

#[tauri::command]
pub fn open_project_cache(state: State<'_, Online>, repo: String) -> Result<(), String> {
    let folder = images::cache_dir(&state.cache_root, &repo)?;
    crate::attachments::system_open(&folder)
}

#[tauri::command]
pub async fn clear_project_cache(state: State<'_, Online>, repo: String) -> Result<u64, String> {
    let cache_root = state.cache_root.clone();
    tauri::async_runtime::spawn_blocking(move || images::clear_cache(&cache_root, &repo))
        .await
        .map_err(|e| e.to_string())?
}
