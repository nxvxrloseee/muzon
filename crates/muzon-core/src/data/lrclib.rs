use serde::Deserialize;
use std::sync::OnceLock;
use std::time::Duration;

const API: &str = "https://lrclib.net/api";
/// LRCLIB asks clients to identify themselves so it can tell traffic apart.
const USER_AGENT: &str = concat!(
    "Muzon/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/nxvxrloseee/muzon)"
);
const TIMEOUT: Duration = Duration::from_secs(10);

/// One result. Field names are LRCLIB's, in camelCase.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Candidate {
    pub duration: Option<f64>,
    pub synced_lyrics: Option<String>,
    pub plain_lyrics: Option<String>,
}

impl Candidate {
    /// Timed lyrics if there are any, otherwise plain text. Empty strings count
    /// as absent: the API returns them for instrumentals.
    pub fn text(&self) -> Option<&str> {
        let non_empty = |s: &Option<String>| {
            s.as_deref()
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(|_| ())
        };
        non_empty(&self.synced_lyrics)
            .and(self.synced_lyrics.as_deref())
            .or_else(|| non_empty(&self.plain_lyrics).and(self.plain_lyrics.as_deref()))
    }

    pub fn has_synced(&self) -> bool {
        self.synced_lyrics
            .as_deref()
            .is_some_and(|s| !s.trim().is_empty())
    }
}

pub struct Query {
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_secs: Option<f64>,
}

/// Shared HTTP client; no request is made until an explicit `find` call.
pub struct Client {
    http: reqwest::Client,
    base: String,
    gate: tokio::sync::Mutex<Rate>,
}
struct Rate {
    next: std::time::Instant,
    blocked: Option<std::time::Instant>,
}
impl Client {
    pub fn new(base: &str) -> anyhow::Result<Self> {
        let now = std::time::Instant::now();
        Ok(Self {
            http: reqwest::Client::builder()
                .user_agent(USER_AGENT)
                .timeout(TIMEOUT)
                .redirect(reqwest::redirect::Policy::limited(3))
                .build()?,
            base: base.trim_end_matches('/').to_owned(),
            gate: tokio::sync::Mutex::new(Rate {
                next: now,
                blocked: Some(now),
            }),
        })
    }
    async fn get<T: serde::de::DeserializeOwned>(
        &self,
        path: &str,
        params: &[(&str, String)],
    ) -> anyhow::Result<Option<T>> {
        let mut rate = self.gate.lock().await;
        let now = std::time::Instant::now();
        anyhow::ensure!(
            rate.blocked.is_some_and(|until| now >= until),
            "LRCLIB просит подождать {} с",
            rate.blocked
                .map(|until| until.saturating_duration_since(now))
                .unwrap_or(Duration::MAX)
                .as_secs()
                .saturating_add(1)
        );
        if rate.next > now {
            tokio::time::sleep(rate.next - now).await;
        }
        // Set the throttle before awaiting too: cancellation cannot erase it.
        rate.next = std::time::Instant::now() + Duration::from_millis(250);
        let mut response = self
            .http
            .get(format!("{}{path}", self.base))
            .query(params)
            .send()
            .await?;
        rate.next = std::time::Instant::now() + Duration::from_millis(250);
        let status = response.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS
            || status == reqwest::StatusCode::SERVICE_UNAVAILABLE
        {
            let wait = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .map(retry_delay)
                .unwrap_or(Duration::from_secs(5));
            rate.blocked = std::time::Instant::now()
                .checked_add(wait.saturating_add(Duration::from_millis(250)));
            anyhow::bail!(
                "LRCLIB временно недоступен (HTTP {status}); повторите через {} с",
                wait.as_secs().max(1)
            );
        }
        if status == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        response.error_for_status_ref()?;
        anyhow::ensure!(status.is_success(), "LRCLIB вернул HTTP {status}");
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");
        anyhow::ensure!(
            content_type
                .split(';')
                .next()
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("application/json")),
            "LRCLIB вернул ответ не в формате JSON"
        );
        const LIMIT: usize = 2 * 1024 * 1024;
        anyhow::ensure!(
            response.content_length().is_none_or(|n| n <= LIMIT as u64),
            "Ответ LRCLIB превышает 2 МиБ"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(
                bytes.len().saturating_add(chunk.len()) <= LIMIT,
                "Ответ LRCLIB превышает 2 МиБ"
            );
            bytes.extend_from_slice(&chunk);
        }
        Ok(Some(serde_json::from_slice(&bytes)?))
    }
    pub async fn find(&self, query: &Query) -> anyhow::Result<Vec<Candidate>> {
        anyhow::ensure!(
            !query.title.trim().is_empty(),
            "Укажите название на вкладке тегов"
        );
        anyhow::ensure!(
            query.title.len() <= 4096
                && query.artist.as_ref().is_none_or(|s| s.len() <= 4096)
                && query.album.as_ref().is_none_or(|s| s.len() <= 4096),
            "Слишком длинные метаданные для поиска"
        );
        let mut params = vec![("track_name", query.title.trim().to_owned())];
        let artist = query
            .artist
            .as_deref()
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if let Some(artist) = artist {
            params.push(("artist_name", artist.to_owned()));
        }
        if artist.is_some() {
            let mut exact = params.clone();
            if let Some(album) = query
                .album
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                exact.push(("album_name", album.to_owned()));
            }
            if let Some(seconds) = query
                .duration_secs
                .filter(|s| s.is_finite() && (1.0..=3600.0).contains(s))
            {
                exact.push(("duration", seconds.round().to_string()));
            }
            if let Some(candidate) = self.get::<Candidate>("/get", &exact).await? {
                if candidate.text().is_some() {
                    return Ok(vec![candidate]);
                }
                // Instrumental/exact empty results do not authorize unrelated text.
                return Ok(Vec::new());
            }
        }
        Ok(self
            .get::<Vec<Candidate>>("/search", &params)
            .await?
            .unwrap_or_default())
    }
}
fn retry_delay(value: &str) -> Duration {
    value
        .parse::<u64>()
        .map(Duration::from_secs)
        .ok()
        .or_else(|| {
            httpdate::parse_http_date(value).ok().map(|date| {
                date.duration_since(std::time::SystemTime::now())
                    .unwrap_or_default()
            })
        })
        .unwrap_or(Duration::from_secs(5))
}
fn client() -> anyhow::Result<&'static Client> {
    static CLIENT: OnceLock<Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }
    let built = Client::new(API)?;
    Ok(CLIENT.get_or_init(|| built))
}
pub async fn find(query: &Query) -> anyhow::Result<Vec<Candidate>> {
    client()?.find(query).await
}
#[derive(Debug, Clone)]
pub struct Suggestion {
    pub text: String,
    pub synced: bool,
}
pub fn select(candidates: &[Candidate], duration: Option<f64>) -> Option<Suggestion> {
    let usable: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| c.text().is_some_and(|s| s.len() <= 1024 * 1024))
        .collect();
    let matches: Vec<crate::domain::lyrics::Match> = usable
        .iter()
        .map(|c| crate::domain::lyrics::Match {
            has_synced: c.has_synced(),
            duration_secs: c.duration.filter(|v| v.is_finite() && *v > 0.0),
        })
        .collect();
    let index = crate::domain::lyrics::pick_best_match(
        &matches,
        duration.filter(|v| v.is_finite() && *v > 0.0),
    )?;
    let candidate = usable[index];
    Some(Suggestion {
        text: candidate.text()?.to_owned(),
        synced: candidate.has_synced(),
    })
}
pub async fn suggest(query: &Query) -> anyhow::Result<Option<Suggestion>> {
    let candidates = find(query).await?;
    Ok(select(&candidates, query.duration_secs))
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::fake_http_server;
    fn query() -> Query {
        Query {
            title: "Song / &".into(),
            artist: Some("Artist".into()),
            album: Some("Album".into()),
            duration_secs: Some(180.0),
        }
    }
    fn candidate(seconds: f64, synced: Option<&str>, plain: Option<&str>) -> Candidate {
        Candidate {
            duration: Some(seconds),
            synced_lyrics: synced.map(str::to_owned),
            plain_lyrics: plain.map(str::to_owned),
        }
    }
    fn raw_server(headers: &str, body: &str, delay: Duration) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let response = format!("HTTP/1.1 {headers}\r\nConnection: close\r\n\r\n{body}");
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(1)))
                .unwrap();
            let mut bytes = [0; 4096];
            let _ = stream.read(&mut bytes);
            std::thread::sleep(delay);
            let _ = stream.write_all(response.as_bytes());
        });
        url
    }
    #[tokio::test]
    async fn server_cooldown_headers_and_non_json_errors_are_respected() {
        let url = raw_server(
            "429 Too Many Requests\r\nRetry-After: 120\r\nContent-Length: 0",
            "",
            Duration::ZERO,
        );
        let client = Client::new(&url).unwrap();
        assert!(client
            .find(&query())
            .await
            .unwrap_err()
            .to_string()
            .contains("120"));
        assert!(
            client
                .gate
                .lock()
                .await
                .blocked
                .unwrap()
                .saturating_duration_since(std::time::Instant::now())
                >= Duration::from_secs(119)
        );
        assert!(client
            .find(&query())
            .await
            .unwrap_err()
            .to_string()
            .contains("подождать"));
        let url = raw_server(
            "200 OK\r\nContent-Type: text/html\r\nContent-Length: 4",
            "oops",
            Duration::ZERO,
        );
        assert!(Client::new(&url)
            .unwrap()
            .find(&query())
            .await
            .unwrap_err()
            .to_string()
            .contains("JSON"));
    }
    #[tokio::test]
    async fn large_replies_and_slow_servers_do_not_hold_the_ui_task() {
        let url = raw_server(
            "200 OK\r\nContent-Type: application/json\r\nContent-Length: 3000000",
            "{}",
            Duration::ZERO,
        );
        assert!(Client::new(&url)
            .unwrap()
            .find(&query())
            .await
            .unwrap_err()
            .to_string()
            .contains("2 МиБ"));
        let url = raw_server(
            "200 OK\r\nContent-Type: application/json\r\nContent-Length: 2",
            "{}",
            Duration::from_millis(200),
        );
        let mut client = Client::new(&url).unwrap();
        client.http = reqwest::Client::builder()
            .timeout(Duration::from_millis(40))
            .build()
            .unwrap();
        assert!(client
            .find(&query())
            .await
            .unwrap_err()
            .downcast_ref::<reqwest::Error>()
            .unwrap()
            .is_timeout());
    }
    #[tokio::test]
    async fn exact_miss_falls_back_and_encodes_metadata() {
        let (url, seen) = fake_http_server(vec![
            (404, "{}".into()),
            (
                200,
                r#"[{"duration":180,"syncedLyrics":"[00:00]Found","plainLyrics":null}]"#.into(),
            ),
        ]);
        let client = Client::new(&url).unwrap();
        let found = client.find(&query()).await.unwrap();
        assert_eq!(select(&found, Some(180.0)).unwrap().text, "[00:00]Found");
        let first = seen.recv().unwrap();
        assert!(first.target.starts_with("/get?"));
        assert!(first.target.contains("track_name=Song+%2F+%26"));
        assert!(first.header("user-agent").unwrap().contains("Muzon"));
        assert!(seen.recv().unwrap().target.starts_with("/search?"));
    }
    #[tokio::test]
    async fn missing_artist_skips_exact_and_empty_title_never_connects() {
        let (url, seen) = fake_http_server(vec![(200, "[]".into())]);
        let client = Client::new(&url).unwrap();
        let mut q = query();
        q.artist = None;
        assert!(client.find(&q).await.unwrap().is_empty());
        assert!(seen.recv().unwrap().target.starts_with("/search?"));
        q.title = " ".into();
        assert!(client.find(&q).await.is_err());
    }
    #[tokio::test]
    async fn rate_limit_prevents_followup_requests() {
        let (url, seen) = fake_http_server(vec![(429, "not json".into())]);
        let client = Client::new(&url).unwrap();
        assert!(client.find(&query()).await.is_err());
        assert!(client
            .find(&query())
            .await
            .unwrap_err()
            .to_string()
            .contains("подождать"));
        assert!(seen.try_recv().is_ok());
        assert!(seen.try_recv().is_err());
    }
    #[test]
    fn matching_rejects_wrong_recording_and_prefers_synced() {
        let candidates = vec![
            candidate(500., Some("wrong"), None),
            candidate(180., None, Some("plain")),
            candidate(181., Some("[00:00]timed"), None),
        ];
        assert_eq!(
            select(&candidates, Some(180.)).unwrap().text,
            "[00:00]timed"
        );
        assert!(select(&candidates, Some(60.)).is_none());
        assert_eq!(retry_delay("99999999"), Duration::from_secs(99999999));
    }
    #[tokio::test]
    #[ignore = "makes an explicit request with public sample metadata"]
    async fn live_lrclib() {
        let found = find(&Query {
            title: "After Dark".into(),
            artist: Some("Mr.Kitty".into()),
            album: None,
            duration_secs: Some(259.),
        })
        .await
        .unwrap();
        assert!(select(&found, Some(259.)).is_some());
    }
}
