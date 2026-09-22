use serde::Deserialize;
use std::sync::OnceLock;
use std::time::Duration;

const API: &str = "https://lrclib.net/api";
/// LRCLIB asks clients to identify themselves so it can tell traffic apart.
const USER_AGENT: &str = concat!("Muzon/", env!("CARGO_PKG_VERSION"));
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

fn client() -> anyhow::Result<&'static reqwest::Client> {
    static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();
    if let Some(client) = CLIENT.get() {
        return Ok(client);
    }
    let built = reqwest::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(TIMEOUT)
        .build()?;
    Ok(CLIENT.get_or_init(|| built))
}

/// Looks the track up, exact match first and a looser search as the fallback.
///
/// The exact endpoint matches artist, title, album and duration together and
/// answers 404 if any of them is off, so a miss there is the normal case rather
/// than a failure - plenty of libraries disagree with LRCLIB about album names.
pub async fn find(query: &Query) -> anyhow::Result<Vec<Candidate>> {
    let client = client()?;
    if let Some(exact) = get_exact(client, query).await? {
        return Ok(vec![exact]);
    }
    search(client, query).await
}

async fn get_exact(
    client: &reqwest::Client,
    query: &Query,
) -> anyhow::Result<Option<Candidate>> {
    let mut params: Vec<(&str, String)> = vec![("track_name", query.title.clone())];
    if let Some(artist) = &query.artist {
        params.push(("artist_name", artist.clone()));
    }
    if let Some(album) = &query.album {
        params.push(("album_name", album.clone()));
    }
    if let Some(duration) = query.duration_secs {
        params.push(("duration", (duration.round() as i64).to_string()));
    }

    let response = client
        .get(format!("{API}/get"))
        .query(&params)
        .send()
        .await?;
    if response.status() == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    Ok(Some(response.error_for_status()?.json().await?))
}

async fn search(client: &reqwest::Client, query: &Query) -> anyhow::Result<Vec<Candidate>> {
    let mut params: Vec<(&str, String)> = vec![("track_name", query.title.clone())];
    if let Some(artist) = &query.artist {
        params.push(("artist_name", artist.clone()));
    }

    Ok(client
        .get(format!("{API}/search"))
        .query(&params)
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hits the real service, so it is `#[ignore]`d: an offline machine must not
    /// fail the suite over it. Run it deliberately with
    /// `cargo test -- --ignored lrclib` when the request or parsing changes.
    #[tokio::test]
    #[ignore = "makes a network request"]
    async fn finds_timed_lyrics_for_a_well_known_track() {
        let found = find(&Query {
            title: "After Dark".to_string(),
            artist: Some("Mr.Kitty".to_string()),
            album: None,
            duration_secs: Some(259.0),
        })
        .await
        .expect("the request itself has to succeed");

        let candidate = found.first().expect("this track is definitely in LRCLIB");
        assert!(candidate.text().is_some());
        assert!(
            candidate.duration.is_some(),
            "duration is what the match is judged on, so it has to survive parsing"
        );
    }
}
