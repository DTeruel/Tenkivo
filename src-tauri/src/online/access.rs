use super::{
    checked, client, connection_error, repository, repository_url, token, Account, Online,
    Repository, API, DOC,
};
use reqwest::{Client, StatusCode};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::atomic::Ordering;
use tauri::State;

async fn pages<T: DeserializeOwned>(
    client: &Client,
    token: &str,
    url: &str,
    wrapper: Option<&str>,
    filters: &[(&str, &str)],
) -> Result<Vec<T>, String> {
    let mut result = Vec::new();
    for page in 1.. {
        let response = checked(
            client
                .get(url)
                .bearer_auth(token)
                .query(filters)
                .query(&[("per_page", 100), ("page", page)])
                .send()
                .await
                .map_err(connection_error)?,
        )?;
        let more = response
            .headers()
            .get("link")
            .and_then(|link| link.to_str().ok())
            .is_some_and(|links| links.contains("rel=\"next\""));
        let mut value: Value = response.json().await.map_err(connection_error)?;
        let entries = match wrapper {
            Some(key) => value
                .get_mut(key)
                .ok_or("GitHub did not return the expected list.")?
                .take(),
            None => value,
        };
        result.extend(
            serde_json::from_value::<Vec<T>>(entries)
                .map_err(|_| "GitHub returned an invalid list.")?,
        );
        if !more {
            break;
        }
    }
    Ok(result)
}

#[derive(Deserialize)]
struct Installation {
    id: u64,
}

#[derive(Serialize)]
pub struct DiscoveredProject {
    repo: String,
    writable: bool,
}

#[tauri::command]
pub async fn online_discover(
    state: State<'_, Online>,
    request_id: u64,
) -> Result<Vec<DiscoveredProject>, String> {
    state.2.fetch_max(request_id, Ordering::Relaxed);
    let cancelled = || state.2.load(Ordering::Relaxed) != request_id;
    if cancelled() {
        return Ok(Vec::new());
    }
    let token = token(&state).await?;
    if cancelled() {
        return Ok(Vec::new());
    }
    let client = client()?;
    let installations: Vec<Installation> = pages(
        &client,
        &token,
        &format!("{API}/user/installations"),
        Some("installations"),
        &[],
    )
    .await?;
    let mut candidates = BTreeMap::new();
    for installation in installations {
        if cancelled() {
            return Ok(Vec::new());
        }
        let url = format!("{API}/user/installations/{}/repositories", installation.id);
        let repositories: Vec<Repository> =
            pages(&client, &token, &url, Some("repositories"), &[]).await?;
        for repo in repositories.into_iter().filter(|repo| repo.private) {
            candidates.insert(repo.full_name.to_ascii_lowercase(), repo);
        }
    }

    let mut projects = Vec::new();
    for repo in candidates.into_values() {
        if cancelled() {
            return Ok(Vec::new());
        }
        let url = repository_url(&repo.full_name)?;
        let response = client
            .get(format!("{url}/contents"))
            .query(&[("ref", &repo.default_branch)])
            .bearer_auth(&token)
            .send()
            .await
            .map_err(connection_error)?;
        if response.status() == StatusCode::NOT_FOUND {
            continue;
        }
        let root: Vec<Value> = checked(response)?.json().await.map_err(connection_error)?;
        if root
            .iter()
            .any(|entry| entry["name"] == DOC && entry["type"] == "file")
        {
            projects.push(DiscoveredProject {
                repo: repo.full_name,
                writable: repo.permissions.is_some_and(|permission| permission.push),
            });
        }
    }
    Ok(projects)
}

#[tauri::command]
pub fn online_stop_discovery(state: State<'_, Online>, request_id: u64) {
    state.2.fetch_max(request_id, Ordering::Relaxed);
}

async fn admin_context(
    state: &Online,
    repo: &str,
) -> Result<(Client, String, String, Repository, u64), String> {
    let account_id = {
        let session = state.0.lock().unwrap();
        session
            .credentials
            .as_ref()
            .ok_or("Sign in with GitHub first.")?
            .account
            .id
    };
    let token = token(state).await?;
    let client = client()?;
    let url = repository_url(repo)?;
    let repo = repository(&client, &token, &url).await?;
    if !repo
        .permissions
        .as_ref()
        .is_some_and(|permission| permission.admin)
    {
        return Err("Only repository administrators can manage project users.".into());
    }
    Ok((client, token, url, repo, account_id))
}

#[derive(Deserialize)]
struct GitHubCollaborator {
    login: String,
    id: u64,
    role_name: String,
}

#[derive(Serialize)]
struct Collaborator {
    login: String,
    id: u64,
    role: String,
    removable: bool,
}

#[derive(Serialize, Deserialize)]
struct Invitation {
    id: u64,
    invitee: Option<Account>,
    permissions: String,
}

#[derive(Serialize)]
pub struct Access {
    collaborators: Vec<Collaborator>,
    invitations: Vec<Invitation>,
}

#[tauri::command]
pub async fn online_project_access(
    state: State<'_, Online>,
    repo: String,
) -> Result<Access, String> {
    let (client, token, url, details, signed_in) = admin_context(&state, &repo).await?;
    let collaborators: Vec<GitHubCollaborator> = pages(
        &client,
        &token,
        &format!("{url}/collaborators"),
        None,
        &[("affiliation", "direct")],
    )
    .await?;
    let invitations = pages(&client, &token, &format!("{url}/invitations"), None, &[]).await
        .map_err(|error| format!("Could not list invitations. Check the GitHub App's Administration permission. {error}"))?;
    Ok(Access {
        collaborators: collaborators
            .into_iter()
            .map(|user| Collaborator {
                removable: user.id != details.owner.id && user.id != signed_in,
                id: user.id,
                login: user.login,
                role: user.role_name,
            })
            .collect(),
        invitations,
    })
}

#[derive(Serialize, Deserialize)]
pub struct VerifiedUser {
    id: u64,
    login: String,
    name: Option<String>,
    #[serde(rename = "type", skip_serializing)]
    account_type: String,
}

async fn find_user(client: &Client, token: &str, login: &str) -> Result<VerifiedUser, String> {
    let login = login.trim().trim_start_matches('@');
    if login.is_empty()
        || login.len() > 39
        || !login
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-')
    {
        return Err("Enter a GitHub username.".into());
    }
    let response = client
        .get(format!("{API}/users/{login}"))
        .bearer_auth(token)
        .send()
        .await
        .map_err(connection_error)?;
    if response.status() == StatusCode::NOT_FOUND {
        return Err("GitHub did not find that user.".into());
    }
    let account: VerifiedUser = checked(response)?.json().await.map_err(connection_error)?;
    if account.account_type != "User" {
        return Err("Choose a personal GitHub account.".into());
    }
    Ok(account)
}

#[tauri::command]
pub async fn online_find_user(
    state: State<'_, Online>,
    login: String,
) -> Result<VerifiedUser, String> {
    find_user(&client()?, &token(&state).await?, &login).await
}

#[tauri::command]
pub async fn online_invite_editor(
    state: State<'_, Online>,
    repo: String,
    login: String,
    user_id: u64,
) -> Result<bool, String> {
    let (client, token, url, details, signed_in) = admin_context(&state, &repo).await?;
    let user = find_user(&client, &token, &login).await?;
    if user.id != user_id {
        return Err("The account changed. Verify the username again.".into());
    }
    if user.id == details.owner.id || user.id == signed_in {
        return Err("This account already administers the repository.".into());
    }
    let member_url = format!("{url}/collaborators/{}", user.login);
    // Leave admins and other existing write roles intact.
    let current = client
        .get(format!("{member_url}/permission"))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(connection_error)?;
    if current.status().is_success() {
        let permission: Value = current.json().await.map_err(connection_error)?;
        if matches!(permission["permission"].as_str(), Some("admin" | "write")) {
            return Err("This account already has write access.".into());
        }
    } else if current.status() != StatusCode::NOT_FOUND {
        checked(current)?;
    }
    let response = checked(
        client
            .put(member_url)
            .bearer_auth(&token)
            .json(&json!({"permission":"push"}))
            .send()
            .await
            .map_err(connection_error)?,
    )?;
    Ok(response.status() == StatusCode::CREATED)
}

#[tauri::command]
pub async fn online_remove_editor(
    state: State<'_, Online>,
    repo: String,
    login: String,
    user_id: u64,
) -> Result<(), String> {
    let (client, token, url, details, signed_in) = admin_context(&state, &repo).await?;
    let user = find_user(&client, &token, &login).await?;
    if user.id != user_id || user.id == details.owner.id || user.id == signed_in {
        return Err("This account cannot be removed here.".into());
    }
    checked(
        client
            .delete(format!("{url}/collaborators/{}", user.login))
            .bearer_auth(&token)
            .send()
            .await
            .map_err(connection_error)?,
    )?;
    Ok(())
}

#[tauri::command]
pub async fn online_cancel_invitation(
    state: State<'_, Online>,
    repo: String,
    invitation_id: u64,
) -> Result<(), String> {
    let (client, token, url, _, _) = admin_context(&state, &repo).await?;
    checked(
        client
            .delete(format!("{url}/invitations/{invitation_id}"))
            .bearer_auth(&token)
            .send()
            .await
            .map_err(connection_error)?,
    )?;
    Ok(())
}
