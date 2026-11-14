//! ListenBrainz: a user token in a header, JSON in, JSON out.

use super::SendError;
use crate::domain::scrobble::{listenbrainz_payload, Listen};
use serde_json::Value;

pub const API: &str = "https://api.listenbrainz.org";

/// Checks the token and returns the account it belongs to.
pub async fn validate(http: &reqwest::Client, base: &str, token: &str) -> anyhow::Result<String> {
    let response = http
        .get(format!("{base}/1/validate-token"))
        .header("Authorization", format!("Token {token}"))
        .send()
        .await?;
    let body: Value = response.json().await?;
    if body["valid"].as_bool() != Some(true) {
        anyhow::bail!("ListenBrainz не принял токен");
    }
    body["user_name"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| anyhow::anyhow!("ListenBrainz не назвал пользователя"))
}

/// Submits listens (or one "playing now"). Up to 1000 per call is allowed;
/// the queue sends far fewer.
pub async fn submit(
    http: &reqwest::Client,
    base: &str,
    token: &str,
    listens: &[Listen],
    now_playing: bool,
) -> Result<(), SendError> {
    let response = http
        .post(format!("{base}/1/submit-listens"))
        .header("Authorization", format!("Token {token}"))
        .json(&listenbrainz_payload(listens, now_playing))
        .send()
        .await
        .map_err(|e| SendError::Retry(format!("нет связи с ListenBrainz: {e}")))?;
    let status = response.status();
    if status.is_success() {
        return Ok(());
    }
    let message = response
        .json::<Value>()
        .await
        .ok()
        .and_then(|b| b["error"].as_str().map(str::to_string))
        .unwrap_or_else(|| status.to_string());
    Err(match status.as_u16() {
        // The listens themselves were refused: sending them again would be
        // refused again, and they'd block everything queued behind them
        400 => SendError::Drop(format!("ListenBrainz отклонил прослушивания: {message}")),
        401 => SendError::Retry(format!("токен ListenBrainz недействителен: {message}")),
        _ => SendError::Retry(format!("ListenBrainz: {message}")),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fake_http_server;

    fn listen() -> Listen {
        Listen {
            artist: "Miles Davis".into(),
            track: "So What".into(),
            album: None,
            duration_secs: None,
            track_no: None,
            listened_at: 1_700_000_000,
        }
    }

    #[tokio::test]
    async fn validate_returns_the_user_and_sends_the_token() {
        let (base, seen) =
            fake_http_server(vec![(200, r#"{"valid":true,"user_name":"miles"}"#.into())]);
        let user = validate(&reqwest::Client::new(), &base, "tok")
            .await
            .unwrap();
        assert_eq!(user, "miles");
        assert_eq!(
            seen.recv().unwrap().header("authorization"),
            Some("Token tok")
        );
    }

    #[tokio::test]
    async fn an_invalid_token_is_an_error() {
        let (base, _seen) = fake_http_server(vec![(200, r#"{"valid":false}"#.into())]);
        assert!(validate(&reqwest::Client::new(), &base, "bad")
            .await
            .is_err());
    }

    #[tokio::test]
    async fn submit_posts_the_listen() {
        let (base, seen) = fake_http_server(vec![(200, r#"{"status":"ok"}"#.into())]);
        submit(&reqwest::Client::new(), &base, "tok", &[listen()], false)
            .await
            .unwrap();
        let request = seen.recv().unwrap();
        assert_eq!(request.method, "POST");
        assert_eq!(request.target, "/1/submit-listens");
        let body: Value = serde_json::from_str(&request.body).unwrap();
        assert_eq!(
            body["payload"][0]["track_metadata"]["track_name"],
            "So What"
        );
    }

    #[tokio::test]
    async fn refused_listens_are_dropped_but_a_server_error_is_retried() {
        let (base, _seen) = fake_http_server(vec![
            (400, r#"{"error":"bad"}"#.into()),
            (503, r#"{"error":"later"}"#.into()),
        ]);
        let http = reqwest::Client::new();
        assert!(matches!(
            submit(&http, &base, "tok", &[listen()], false).await,
            Err(SendError::Drop(_))
        ));
        assert!(matches!(
            submit(&http, &base, "tok", &[listen()], false).await,
            Err(SendError::Retry(_))
        ));
    }
}
