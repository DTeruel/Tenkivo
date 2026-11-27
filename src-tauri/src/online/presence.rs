use super::{
    checked, client, connection_error, images::git_sha, now_seconds, repository, repository_url,
    token, Online,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use tauri::State;

const BRANCH: &str = "tenkivo-presence";
const FILE: &str = ".tenkivo/presence.json";
const LEASE_SECONDS: u64 = 90;

#[derive(Clone, Deserialize, Serialize)]
pub struct Editor {
    page: String,
    branch: String,
    login: String,
    user_id: u64,
    expires: u64,
}

type Editors = BTreeMap<String, Editor>;

fn active_only(editors: &mut Editors, now: u64) {
    editors.retain(|_, editor| editor.expires > now && editor.expires <= now + LEASE_SECONDS * 2);
}

async fn read(
    client: &Client,
    token: &str,
    url: &str,
) -> Result<(Option<String>, Editors), String> {
    let response = client
        .get(format!("{url}/contents/{FILE}"))
        .query(&[("ref", BRANCH)])
        .bearer_auth(token)
        .send()
        .await
        .map_err(connection_error)?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok((None, Editors::new()));
    }
    let file: Value = checked(response)?.json().await.map_err(connection_error)?;
    let sha = git_sha(&file["sha"])?;
    let content = file["content"]
        .as_str()
        .ok_or("Invalid page presence file.")?;
    let bytes = STANDARD
        .decode(content.split_whitespace().collect::<String>())
        .map_err(|_| "Invalid page presence encoding.")?;
    let editors = serde_json::from_slice(&bytes).map_err(|_| "Invalid page presence data.")?;
    Ok((Some(sha.to_owned()), editors))
}

async fn ensure_branch(
    client: &Client,
    token: &str,
    url: &str,
    project_branch: &str,
) -> Result<(), String> {
    let reference = format!("{url}/git/ref/heads/{BRANCH}");
    let result = client
        .get(&reference)
        .bearer_auth(token)
        .send()
        .await
        .map_err(connection_error)?;
    if result.status() != StatusCode::NOT_FOUND {
        checked(result)?;
        return Ok(());
    }
    let commits: Vec<Value> = checked(
        client
            .get(format!("{url}/commits"))
            .query(&[("sha", project_branch), ("per_page", "1")])
            .bearer_auth(token)
            .send()
            .await
            .map_err(connection_error)?,
    )?
    .json()
    .await
    .map_err(connection_error)?;
    let head = git_sha(
        &commits
            .first()
            .ok_or("The project branch has no commits.")?["sha"],
    )?;
    let result = client
        .post(format!("{url}/git/refs"))
        .bearer_auth(token)
        .json(&json!({"ref":format!("refs/heads/{BRANCH}"),"sha":head}))
        .send()
        .await
        .map_err(connection_error)?;
    if matches!(
        result.status(),
        StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY
    ) {
        let existing = client
            .get(&reference)
            .bearer_auth(token)
            .send()
            .await
            .map_err(connection_error)?;
        if existing.status().is_success() {
            return Ok(());
        }
    }
    checked(result)?;
    Ok(())
}

#[tauri::command]
pub async fn online_page_presence(
    state: State<'_, Online>,
    repo: String,
    branch: String,
    session_id: String,
    write: bool,
    page: Option<String>,
) -> Result<Vec<Editor>, String> {
    if branch == BRANCH
        || session_id.len() != 36
        || !session_id
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
        || page
            .as_ref()
            .is_some_and(|page| page.is_empty() || page.len() > 512)
    {
        return Err("Invalid page presence request.".into());
    }
    let url = repository_url(&repo)?;
    let token = token(&state).await?;
    let account = state
        .session
        .lock()
        .unwrap()
        .credentials
        .as_ref()
        .map(|credentials| credentials.account.clone())
        .ok_or("Sign in to GitHub first.")?;
    let client = client()?;
    if write && page.is_some() {
        let details = repository(&client, &token, &url).await?;
        if !details.permissions.is_some_and(|permission| permission.push) {
            return Err("GitHub write access is required to publish page presence.".into());
        }
    }
    let existing = |editors: Editors| {
        editors
            .into_values()
            .filter(|editor| editor.branch == branch && editor.user_id != account.id)
            .collect::<Vec<_>>()
    };

    for attempt in 0..3 {
        let (sha, mut editors) = read(&client, &token, &url).await?;
        let now = now_seconds();
        active_only(&mut editors, now);
        if !write {
            return Ok(existing(editors));
        }
        if editors
            .get(&session_id)
            .is_some_and(|editor| editor.user_id != account.id)
        {
            return Err("This page presence session belongs to another account.".into());
        }
        if let Some(page) = &page {
            if sha.is_none() {
                ensure_branch(&client, &token, &url, &branch).await?;
            }
            editors.insert(
                session_id.clone(),
                Editor {
                    page: page.clone(),
                    branch: branch.clone(),
                    login: account.login.clone(),
                    user_id: account.id,
                    expires: now + LEASE_SECONDS,
                },
            );
        } else if editors.remove(&session_id).is_none() {
            return Ok(existing(editors));
        }
        let mut body = json!({
            "branch": BRANCH, "message": "Update Tenkivo page presence",
            "content": STANDARD.encode(serde_json::to_vec(&editors).map_err(|error| error.to_string())?)
        });
        if let Some(sha) = sha {
            body["sha"] = json!(sha);
        }
        let result = client
            .put(format!("{url}/contents/{FILE}"))
            .bearer_auth(&token)
            .json(&body)
            .send()
            .await
            .map_err(connection_error)?;
        if attempt < 2
            && matches!(
                result.status(),
                StatusCode::CONFLICT | StatusCode::UNPROCESSABLE_ENTITY
            )
        {
            continue;
        }
        checked(result)?;
        return Ok(existing(editors));
    }
    Err("Could not update page presence.".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_expired_and_invalid_leases() {
        let editor = |expires| Editor {
            page: "page".into(),
            branch: "main".into(),
            login: "viewer".into(),
            user_id: 1,
            expires,
        };
        let mut editors = Editors::from([
            ("expired".into(), editor(100)),
            ("active".into(), editor(101)),
            ("invalid".into(), editor(300)),
        ]);
        active_only(&mut editors, 100);
        assert_eq!(editors.keys().collect::<Vec<_>>(), vec!["active"]);
    }
}
