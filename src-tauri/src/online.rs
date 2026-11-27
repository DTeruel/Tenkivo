use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::State;

const API: &str = "https://api.github.com";
const DOC: &str = "project.tkv";
const MAX_DOCUMENT: usize = 1024 * 1024;

#[tauri::command]
pub fn github_client_id() -> Option<&'static str> {
    option_env!("TENKIVO_GITHUB_CLIENT_ID")
        .map(str::trim)
        .filter(|id| !id.is_empty())
}

#[derive(Default)]
pub struct Online(Mutex<Session>);

#[derive(Default)]
struct Session {
    generation: u64,
    pending: Option<Device>,
    token: Option<String>,
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

#[derive(Serialize, Deserialize)]
pub struct Account {
    id: u64,
    login: String,
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
    if client_id.is_empty()
        || client_id.len() > 100
        || !client_id
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err("Enter the public Client ID of your GitHub App.".into());
    }
    let generation = {
        let mut session = state.0.lock().unwrap();
        session.generation += 1;
        session.pending = None;
        session.generation
    };
    let response = checked(
        client()?
            .post("https://github.com/login/device/code")
            .header("Accept", "application/json")
            .form(&[("client_id", &client_id)])
            .send()
            .await
            .map_err(connection_error)?,
    )?;
    let device: DeviceResponse = response.json().await.map_err(|_| {
        "GitHub could not start sign-in. Check the Client ID and enable Device Flow for the GitHub App.".to_string()
    })?;
    let now = Instant::now();
    let interval = device.interval.clamp(1, 120);
    let mut session = state.0.lock().unwrap();
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
        let mut session = state.0.lock().unwrap();
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
    if let Some(token) = data["access_token"].as_str() {
        let account: Account = checked(
            client
                .get(format!("{API}/user"))
                .bearer_auth(token)
                .send()
                .await
                .map_err(connection_error)?,
        )?
        .json()
        .await
        .map_err(connection_error)?;
        let mut session = state.0.lock().unwrap();
        if session.generation != generation {
            return Err("Sign-in was cancelled.".into());
        }
        session.token = Some(token.to_owned());
        session.pending = None;
        return Ok(Poll::SignedIn { account });
    }
    let mut session = state.0.lock().unwrap();
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
pub fn online_sign_out(state: State<'_, Online>) {
    let mut session = state.0.lock().unwrap();
    *session = Session {
        generation: session.generation + 1,
        ..Session::default()
    };
}

fn token(state: &Online) -> Result<String, String> {
    state
        .0
        .lock()
        .unwrap()
        .token
        .clone()
        .ok_or_else(|| "Sign in with GitHub first.".into())
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
    private: bool,
    default_branch: String,
    permissions: Option<Permissions>,
}

#[derive(Deserialize)]
struct Permissions {
    push: bool,
}

async fn repository(client: &Client, token: &str, url: &str) -> Result<Repository, String> {
    let details: Repository = checked(
        client
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(connection_error)?,
    )?
    .json()
    .await
    .map_err(connection_error)?;
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
    let token = token(&state)?;
    let client = client()?;
    let url = repository_url(&repo)?;
    let details = repository(&client, &token, &url).await?;
    let branch = branch.unwrap_or(details.default_branch);
    let response = client
        .get(format!("{url}/contents/{DOC}"))
        .query(&[("ref", &branch)])
        .bearer_auth(&token)
        .send()
        .await
        .map_err(connection_error)?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err(
            "There is no project.tkv on this branch, or your account cannot read it.".into(),
        );
    }
    let content: Value = checked(response)?.json().await.map_err(connection_error)?;
    let encoded = content["content"]
        .as_str()
        .ok_or("The online project is not a readable file.")?;
    if content["type"] != "file"
        || content["encoding"] != "base64"
        || encoded.len() > 2 * MAX_DOCUMENT
    {
        return Err("The online file is not a supported project or exceeds 1 MiB.".into());
    }
    let decoded = STANDARD
        .decode(encoded.split_whitespace().collect::<String>())
        .map_err(|_| "GitHub returned an invalid project file.")?;
    if decoded.len() > MAX_DOCUMENT {
        return Err("The online project exceeds 1 MiB.".into());
    }
    Ok(RemoteProject {
        document: String::from_utf8(decoded)
            .map_err(|_| "The online project is not valid UTF-8.")?,
        sha: content["sha"]
            .as_str()
            .ok_or("GitHub did not return the file revision.")?
            .into(),
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
    if document.len() > MAX_DOCUMENT {
        return Err("Online projects are limited to 1 MiB.".into());
    }
    let token = token(&state)?;
    let client = client()?;
    let url = repository_url(&repo)?;
    let details = repository(&client, &token, &url).await?;
    if !details.permissions.is_some_and(|p| p.push) {
        return Err("Your GitHub account has read-only access to this repository.".into());
    }
    let branch = branch.unwrap_or(details.default_branch);
    let mut body = json!({"message":"Save Tenkivo project","content":STANDARD.encode(document),"branch":branch});
    if let Some(ref sha) = sha {
        body["sha"] = json!(sha);
    }
    let response = client
        .put(format!("{url}/contents/{DOC}"))
        .bearer_auth(&token)
        .json(&body)
        .send()
        .await
        .map_err(connection_error)?;
    match response.status() {
        StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY if sha.is_some() => {
            return Err("GitHub rejected this revision. Export a local copy to preserve your changes, then reload the online project before continuing.".into());
        }
        StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY => {
            return Err("GitHub rejected the upload. Check that the repository is initialized, the branch allows writes, and project.tkv does not already exist.".into());
        }
        _ => {}
    }
    let result: Value = checked(response)?.json().await.map_err(connection_error)?;
    Ok(SavedProject {
        sha: result["content"]["sha"]
            .as_str()
            .ok_or("GitHub did not return the saved revision.")?
            .into(),
        branch,
    })
}
