//! Last.fm's 2.0 API: every call signed with the app's shared secret, the
//! user's session key obtained once through the desktop auth flow
//! (getToken -> the user approves in a browser -> getSession).

use super::SendError;
use crate::domain::scrobble::{lastfm_signature, lastfm_track_params, Listen};
use serde_json::Value;

pub const API: &str = "https://ws.audioscrobbler.com";

/// The application's credentials, from last.fm/api/account/create.
#[derive(Clone)]
pub struct AppKeys {
    pub api_key: String,
    pub secret: String,
}

/// Where the user approves access for `token`.
pub fn auth_url(api_key: &str, token: &str) -> String {
    format!("https://www.last.fm/api/auth/?api_key={api_key}&token={token}")
}

/// Last.fm's error codes that mean the request itself is wrong and will
/// never succeed as it is (invalid parameters) - everything else, from a bad
/// session to the service being down, is worth retrying later.
const INVALID_PARAMETERS: i64 = 6;

async fn call(
    http: &reqwest::Client,
    base: &str,
    keys: &AppKeys,
    mut params: Vec<(String, String)>,
    post: bool,
) -> Result<Value, SendError> {
    params.push(("api_key".into(), keys.api_key.clone()));
    let signature = lastfm_signature(&params, &keys.secret);
    params.push(("api_sig".into(), signature));
    params.push(("format".into(), "json".into()));

    let url = format!("{base}/2.0/");
    let request = if post {
        http.post(url).form(&params)
    } else {
        http.get(url).query(&params)
    };
    let response = request
        .send()
        .await
        .map_err(|e| SendError::Retry(format!("нет связи с Last.fm: {e}")))?;
    let body: Value = response
        .json()
        .await
        .map_err(|e| SendError::Retry(format!("Last.fm ответил непонятно: {e}")))?;
    if let Some(code) = body["error"].as_i64() {
        let message = format!(
            "Last.fm: {} (код {code})",
            body["message"].as_str().unwrap_or("ошибка")
        );
        return Err(if code == INVALID_PARAMETERS {
            SendError::Drop(message)
        } else {
            SendError::Retry(message)
        });
    }
    Ok(body)
}

fn method(name: &str) -> (String, String) {
    ("method".into(), name.into())
}

/// First step of the desktop auth flow: a token for the user to approve.
pub async fn get_token(
    http: &reqwest::Client,
    base: &str,
    keys: &AppKeys,
) -> anyhow::Result<String> {
    let body = call(http, base, keys, vec![method("auth.getToken")], false)
        .await
        .map_err(|e| anyhow::anyhow!(e.message().to_string()))?;
    body["token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("Last.fm не выдал токен"))
}

/// Last step: once the user has approved `token`, the session key (which
/// never expires) and the account name.
pub async fn get_session(
    http: &reqwest::Client,
    base: &str,
    keys: &AppKeys,
    token: &str,
) -> anyhow::Result<(String, String)> {
    let params = vec![method("auth.getSession"), ("token".into(), token.into())];
    let body = call(http, base, keys, params, false).await.map_err(|e| {
        anyhow::anyhow!(
            "{} - подтвердите доступ в браузере и нажмите ещё раз",
            e.message()
        )
    })?;
    let session = &body["session"];
    match (session["name"].as_str(), session["key"].as_str()) {
        (Some(name), Some(key)) => Ok((name.to_string(), key.to_string())),
        _ => anyhow::bail!("Last.fm не выдал сессию"),
    }
}

pub async fn scrobble(
    http: &reqwest::Client,
    base: &str,
    keys: &AppKeys,
    session: &str,
    listens: &[Listen],
) -> Result<(), SendError> {
    let mut params = vec![method("track.scrobble"), ("sk".into(), session.into())];
    for (i, listen) in listens.iter().enumerate() {
        params.extend(lastfm_track_params(listen, Some(i)));
    }
    // Scrobbles Last.fm "ignores" (a filtered artist name, a timestamp too
    // far back) come back as a success: resending them would change nothing
    call(http, base, keys, params, true).await.map(|_| ())
}

pub async fn now_playing(
    http: &reqwest::Client,
    base: &str,
    keys: &AppKeys,
    session: &str,
    listen: &Listen,
) -> Result<(), SendError> {
    let mut params = vec![
        method("track.updateNowPlaying"),
        ("sk".into(), session.into()),
    ];
    params.extend(lastfm_track_params(listen, None));
    call(http, base, keys, params, true).await.map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fake_http_server;

    fn keys() -> AppKeys {
        AppKeys {
            api_key: "KEY".into(),
            secret: "SECRET".into(),
        }
    }

    fn form(body: &str) -> Vec<(String, String)> {
        url::form_urlencoded::parse(body.as_bytes())
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[tokio::test]
    async fn the_auth_flow_yields_the_user_and_session() {
        let (base, seen) = fake_http_server(vec![
            (200, r#"{"token":"T"}"#.into()),
            (
                200,
                r#"{"session":{"name":"eno","key":"SK","subscriber":0}}"#.into(),
            ),
        ]);
        let http = reqwest::Client::new();
        let token = get_token(&http, &base, &keys()).await.unwrap();
        assert_eq!(token, "T");
        assert!(seen.recv().unwrap().target.contains("method=auth.getToken"));

        let (user, session) = get_session(&http, &base, &keys(), &token).await.unwrap();
        assert_eq!((user.as_str(), session.as_str()), ("eno", "SK"));
    }

    #[tokio::test]
    async fn a_scrobble_batch_is_signed_over_everything_it_sends() {
        let (base, seen) = fake_http_server(vec![(
            200,
            r#"{"scrobbles":{"@attr":{"accepted":2,"ignored":0}}}"#.into(),
        )]);
        let listen = Listen {
            artist: "Brian Eno".into(),
            track: "Ascent".into(),
            album: Some("Apollo".into()),
            duration_secs: Some(254.0),
            track_no: None,
            listened_at: 1_700_000_000,
        };
        scrobble(
            &reqwest::Client::new(),
            &base,
            &keys(),
            "SK",
            &[listen.clone(), listen],
        )
        .await
        .unwrap();

        let params = form(&seen.recv().unwrap().body);
        let get = |k: &str| {
            params
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
        };
        assert_eq!(get("artist[1]").as_deref(), Some("Brian Eno"));
        assert_eq!(get("sk").as_deref(), Some("SK"));
        // What the server would check: the signature over the rest
        let unsigned: Vec<(String, String)> = params
            .iter()
            .filter(|(k, _)| k != "api_sig")
            .cloned()
            .collect();
        assert_eq!(get("api_sig"), Some(lastfm_signature(&unsigned, "SECRET")));
    }

    #[tokio::test]
    async fn a_bad_session_is_retried_and_bad_parameters_are_dropped() {
        let (base, _seen) = fake_http_server(vec![
            (403, r#"{"error":9,"message":"Invalid session key"}"#.into()),
            (400, r#"{"error":6,"message":"Invalid parameters"}"#.into()),
        ]);
        let http = reqwest::Client::new();
        let listen = Listen {
            artist: "A".into(),
            track: "T".into(),
            album: None,
            duration_secs: None,
            track_no: None,
            listened_at: 1,
        };
        assert!(matches!(
            scrobble(&http, &base, &keys(), "SK", &[listen.clone()]).await,
            Err(SendError::Retry(_))
        ));
        assert!(matches!(
            scrobble(&http, &base, &keys(), "SK", &[listen]).await,
            Err(SendError::Drop(_))
        ));
    }
}
