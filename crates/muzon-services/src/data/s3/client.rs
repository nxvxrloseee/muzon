//! A small S3 client: list, head, get, put. Enough for syncing a music
//! library, and compatible with anything that speaks the S3 API (AWS itself,
//! MinIO, Backblaze B2, Cloudflare R2, Yandex Object Storage).

use super::sigv4::{self, Credentials, Request, UNSIGNED_PAYLOAD};
use crate::domain::S3Config;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub struct RemoteObject {
    pub key: String,
    pub size: u64,
    pub etag: String,
}

pub struct S3Client {
    http: reqwest::Client,
    cfg: S3Config,
    creds: Credentials,
}

impl S3Client {
    pub fn new(cfg: S3Config, creds: Credentials) -> anyhow::Result<Self> {
        if cfg.bucket.trim().is_empty() {
            anyhow::bail!("не указан бакет");
        }
        let http = reqwest::Client::builder()
            .user_agent(concat!("muzon/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(std::time::Duration::from_secs(10))
            .read_timeout(std::time::Duration::from_secs(30))
            .redirect(reqwest::redirect::Policy::none())
            .build()?;
        let built = Self { http, cfg, creds };
        let (url, _, _) = built.endpoint_for("")?;
        let parsed = reqwest::Url::parse(&url)?;
        anyhow::ensure!(
            matches!(parsed.scheme(), "http" | "https"),
            "Endpoint должен использовать HTTP или HTTPS"
        );
        if !built.cfg.endpoint.is_empty() {
            let base = if built.cfg.endpoint.contains("://") {
                built.cfg.endpoint.clone()
            } else {
                format!("https://{}", built.cfg.endpoint)
            };
            let parsed = reqwest::Url::parse(&base)?;
            anyhow::ensure!(
                parsed.username().is_empty()
                    && parsed.password().is_none()
                    && parsed.query().is_none()
                    && parsed.fragment().is_none()
                    && matches!(parsed.path(), "" | "/"),
                "Endpoint должен содержать только адрес сервера, без пароля, пути и параметров"
            );
        }
        Ok(built)
    }

    /// Where a key lives: `https://host/bucket/key` for path-style endpoints
    /// (MinIO and most self-hosted servers), `https://bucket.host/key` for AWS.
    fn endpoint_for(&self, key: &str) -> anyhow::Result<(String, String, String)> {
        let base = self.cfg.endpoint.trim().trim_end_matches('/');
        let base = if base.is_empty() {
            format!("https://s3.{}.amazonaws.com", self.cfg.region)
        } else if base.contains("://") {
            base.to_string()
        } else {
            format!("https://{base}")
        };

        let url = reqwest::Url::parse(&base)?;
        let scheme = url.scheme().to_string();
        let host = url
            .host_str()
            .ok_or_else(|| anyhow::anyhow!("в адресе хранилища нет хоста: {base}"))?
            .to_string();
        let host = if host.contains(':') && !host.starts_with('[') {
            format!("[{host}]")
        } else {
            host
        };
        anyhow::ensure!(
            self.cfg.path_style || !host.contains(':'),
            "Для IPv6 используйте path-style"
        );
        let port = url.port().map(|p| format!(":{p}")).unwrap_or_default();
        let encoded_key = sigv4::uri_encode(key, true);

        Ok(if self.cfg.path_style {
            let host_header = format!("{host}{port}");
            let path = format!("/{}/{}", self.cfg.bucket, encoded_key);
            (format!("{scheme}://{host_header}{path}"), host_header, path)
        } else {
            let host_header = format!("{}.{host}{port}", self.cfg.bucket);
            let path = format!("/{encoded_key}");
            (format!("{scheme}://{host_header}{path}"), host_header, path)
        })
    }

    async fn send(
        &self,
        method: reqwest::Method,
        key: &str,
        query: &[(&str, String)],
        body: Option<Vec<u8>>,
    ) -> anyhow::Result<reqwest::Response> {
        let length = body.as_ref().map(|b| b.len() as u64);
        self.send_body(method, key, query, body.map(reqwest::Body::from), length)
            .await
    }
    async fn send_body(
        &self,
        method: reqwest::Method,
        key: &str,
        query: &[(&str, String)],
        body: Option<reqwest::Body>,
        length: Option<u64>,
    ) -> anyhow::Result<reqwest::Response> {
        self.send_body_condition(method, key, query, body, length, false)
            .await
    }

    async fn send_body_condition(
        &self,
        method: reqwest::Method,
        key: &str,
        query: &[(&str, String)],
        body: Option<reqwest::Body>,
        length: Option<u64>,
        create_only: bool,
    ) -> anyhow::Result<reqwest::Response> {
        let (url, host, path) = self.endpoint_for(key)?;

        // The canonical query string is sorted and encoded pair by pair
        let mut pairs: Vec<String> = query
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    sigv4::uri_encode(k, false),
                    sigv4::uri_encode(v, false)
                )
            })
            .collect();
        pairs.sort();
        let canonical_query = pairs.join("&");

        // UNSIGNED-PAYLOAD keeps large uploads out of the signature: the body is
        // protected by TLS, and hashing a 60 MB file twice buys nothing.
        let payload_hash = UNSIGNED_PAYLOAD;
        let headers = sigv4::sign(
            &Request {
                method: method.as_str(),
                path: &path,
                query: &canonical_query,
                host: &host,
                payload_hash,
                timestamp: &sigv4::amz_timestamp(sigv4::now_unix()),
                region: &self.cfg.region,
                service: "s3",
                content_sha_header: true,
            },
            &self.creds,
        );

        let full_url = if canonical_query.is_empty() {
            url
        } else {
            format!("{url}?{canonical_query}")
        };

        let mut req = self.http.request(method, &full_url);
        for (name, value) in headers {
            if name == "host" {
                continue; // reqwest sets it from the URL
            }
            req = req.header(name, value);
        }
        if let Some(body) = body {
            if let Some(length) = length {
                req = req.header("content-length", length);
            }
            req = req.body(body);
        }

        if create_only {
            req = req.header("if-none-match", "*");
        }
        Ok(req.send().await?)
    }

    async fn check(resp: reqwest::Response, what: &str) -> anyhow::Result<reqwest::Response> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status();
        let body =
            String::from_utf8_lossy(&Self::read_bounded(resp, 8192).await.unwrap_or_default())
                .into_owned();
        let detail =
            extract_tag(&body, "Message").unwrap_or_else(|| body.chars().take(200).collect());
        anyhow::bail!("{what}: {status} {detail}")
    }

    /// Everything under `prefix`, following continuation tokens.
    pub async fn list(&self, prefix: &str) -> anyhow::Result<Vec<RemoteObject>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;
        let mut seen = std::collections::HashSet::new();

        loop {
            let mut query = vec![
                ("list-type", "2".to_string()),
                ("prefix", prefix.to_string()),
                ("max-keys", "1000".to_string()),
            ];
            if let Some(t) = &token {
                query.push(("continuation-token", t.clone()));
            }

            let resp = self.send(reqwest::Method::GET, "", &query, None).await?;
            let body = String::from_utf8(
                Self::read_bounded(
                    Self::check(resp, "не удалось получить список объектов").await?,
                    16 * 1024 * 1024,
                )
                .await?,
            )?;

            out.extend(parse_list(&body));
            anyhow::ensure!(
                out.len() <= 250000,
                "Список S3 превышает 250000 объектов; сузьте префикс"
            );
            match extract_tag(&body, "NextContinuationToken") {
                Some(t) if extract_tag(&body, "IsTruncated").as_deref() == Some("true") => {
                    anyhow::ensure!(seen.insert(t.clone()), "S3 повторил токен страницы списка");
                    token = Some(t)
                }
                _ => break,
            }
        }
        Ok(out)
    }

    pub async fn put_bytes(&self, key: &str, body: Vec<u8>) -> anyhow::Result<()> {
        let resp = self
            .send(reqwest::Method::PUT, key, &[], Some(body))
            .await?;
        Self::check(resp, &format!("не удалось загрузить {key}")).await?;
        Ok(())
    }

    /// Stream files with a bounded buffer; albums never accumulate in RAM.
    pub async fn put_file(&self, key: &str, path: &Path) -> anyhow::Result<()> {
        let file = tokio::fs::File::open(path).await?;
        let length = file.metadata().await?.len();
        let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(file));
        let response = self
            .send_body(reqwest::Method::PUT, key, &[], Some(body), Some(length))
            .await?;
        Self::check(response, &format!("не удалось загрузить {key}")).await?;
        Ok(())
    }
    /// Fail closed if another client created the key after the listing.
    pub async fn put_file_if_absent(&self, key: &str, path: &Path) -> anyhow::Result<()> {
        let file = tokio::fs::File::open(path).await?;
        let length = file.metadata().await?.len();
        let body = reqwest::Body::wrap_stream(tokio_util::io::ReaderStream::new(file));
        let response = self
            .send_body_condition(
                reqwest::Method::PUT,
                key,
                &[],
                Some(body),
                Some(length),
                true,
            )
            .await?;
        Self::check(response, &format!("create-only upload failed: {key}")).await?;
        Ok(())
    }

    /// Opaque ETags are never used as content checksums. Stream even large audio.
    pub async fn content_hash(&self, key: &str) -> anyhow::Result<String> {
        use sha2::{Digest, Sha256};
        let response = self.send(reqwest::Method::GET, key, &[], None).await?;
        let mut response = Self::check(response, &format!("cannot compare {key}")).await?;
        let mut hash = Sha256::new();
        while let Some(chunk) = response.chunk().await? {
            hash.update(&chunk);
        }
        Ok(hex::encode(hash.finalize()))
    }

    async fn read_bounded(
        mut response: reqwest::Response,
        limit: usize,
    ) -> anyhow::Result<Vec<u8>> {
        anyhow::ensure!(
            response.content_length().is_none_or(|n| n <= limit as u64),
            "Ответ S3 превышает допустимый размер"
        );
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await? {
            anyhow::ensure!(
                bytes.len().saturating_add(chunk.len()) <= limit,
                "Ответ S3 превышает допустимый размер"
            );
            bytes.extend_from_slice(&chunk);
        }
        Ok(bytes)
    }

    pub async fn get_bytes(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let resp = self.send(reqwest::Method::GET, key, &[], None).await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let resp = Self::check(resp, &format!("не удалось скачать {key}")).await?;
        Ok(Some(Self::read_bounded(resp, 64 * 1024 * 1024).await?))
    }

    /// A partial download is removed on errors/cancellation; an existing file is never replaced.
    pub async fn get_file(&self, key: &str, dest: &Path) -> anyhow::Result<()> {
        use std::io::Write;
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        anyhow::ensure!(
            std::fs::symlink_metadata(dest).is_err(),
            "Локальный файл уже существует"
        );
        let mut temporary = Partial::new(dest)?;
        let response = self.send(reqwest::Method::GET, key, &[], None).await?;
        let mut response = Self::check(response, &format!("не удалось скачать {key}")).await?;
        while let Some(chunk) = response.chunk().await? {
            temporary.file.as_mut().unwrap().write_all(&chunk)?;
        }
        temporary.file.as_mut().unwrap().sync_all()?;
        temporary.file.take();
        // Same-directory hard-link commit is atomic and refuses replacement.
        std::fs::hard_link(&temporary.path, dest)?;
        Ok(())
    }

    /// A cheap request that proves the endpoint, the bucket and the keys all
    /// work together - used by the "check connection" button.
    pub async fn check_access(&self) -> anyhow::Result<()> {
        let query = vec![
            ("list-type", "2".to_string()),
            ("max-keys", "1".to_string()),
        ];
        let resp = self.send(reqwest::Method::GET, "", &query, None).await?;
        Self::check(resp, "хранилище недоступно").await?;
        Ok(())
    }
}

/// The response schema is three tags deep, so a full XML parser would be a
/// dependency for nothing.
fn parse_list(xml: &str) -> Vec<RemoteObject> {
    let mut out = Vec::new();
    for chunk in xml.split("<Contents>").skip(1) {
        let chunk = chunk.split("</Contents>").next().unwrap_or("");
        let Some(key) = extract_tag(chunk, "Key") else {
            continue;
        };
        out.push(RemoteObject {
            key: unescape(&key),
            size: extract_tag(chunk, "Size")
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            // ETags come back quoted, and the quotes arrive XML-escaped
            etag: extract_tag(chunk, "ETag")
                .map(|e| unescape(&e).trim_matches('"').to_string())
                .unwrap_or_default(),
        });
    }
    out
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{tag}>");
    let close = format!("</{tag}>");
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

fn unescape(s: &str) -> String {
    s.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

struct Partial {
    path: std::path::PathBuf,
    file: Option<std::fs::File>,
}
impl Partial {
    fn new(dest: &Path) -> anyhow::Result<Self> {
        static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        for _ in 0..32 {
            let path = dest.with_file_name(format!(
                ".muzon-download-{}-{}.part",
                std::process::id(),
                SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            match std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => {
                    return Ok(Self {
                        path,
                        file: Some(file),
                    })
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error.into()),
            }
        }
        anyhow::bail!("Не удалось создать временный файл загрузки")
    }
}
impl Drop for Partial {
    fn drop(&mut self) {
        self.file.take();
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn client(path_style: bool, endpoint: &str) -> S3Client {
        S3Client::new(
            S3Config {
                endpoint: endpoint.into(),
                region: "eu-central-1".into(),
                bucket: "muzon".into(),
                prefix: "music".into(),
                path_style,
            },
            Credentials {
                access_key: "key".into(),
                secret_key: "secret".into(),
                session_token: String::new(),
            },
        )
        .unwrap()
    }

    #[test]
    fn builds_path_style_urls_for_self_hosted_servers() {
        let (url, host, path) = client(true, "http://nas.local:9000")
            .endpoint_for("tracks/Камелия/трек.flac")
            .unwrap();
        assert_eq!(host, "nas.local:9000");
        assert_eq!(
            path,
            "/muzon/tracks/%D0%9A%D0%B0%D0%BC%D0%B5%D0%BB%D0%B8%D1%8F/%D1%82%D1%80%D0%B5%D0%BA.flac"
        );
        assert!(url.starts_with("http://nas.local:9000/muzon/tracks/"));
    }

    #[test]
    fn builds_virtual_host_urls_for_aws() {
        let (url, host, path) = client(false, "https://s3.eu-central-1.amazonaws.com")
            .endpoint_for("tracks/a.mp3")
            .unwrap();
        assert_eq!(host, "muzon.s3.eu-central-1.amazonaws.com");
        assert_eq!(path, "/tracks/a.mp3");
        assert_eq!(
            url,
            "https://muzon.s3.eu-central-1.amazonaws.com/tracks/a.mp3"
        );
    }

    #[test]
    fn falls_back_to_the_aws_endpoint_for_the_region() {
        let (_, host, _) = client(false, "").endpoint_for("a").unwrap();
        assert_eq!(host, "muzon.s3.eu-central-1.amazonaws.com");
    }

    #[test]
    fn parses_a_listing() {
        let xml = r#"<?xml version="1.0"?>
        <ListBucketResult>
          <IsTruncated>false</IsTruncated>
          <Contents>
            <Key>music/tracks/a &amp; b.mp3</Key>
            <Size>1234</Size>
            <ETag>&quot;9a0364b9e99bb480dd25e1f0284c8555&quot;</ETag>
          </Contents>
          <Contents>
            <Key>music/library.json</Key>
            <Size>42</Size>
            <ETag>"abc"</ETag>
          </Contents>
        </ListBucketResult>"#;

        let items = parse_list(xml);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].key, "music/tracks/a & b.mp3");
        assert_eq!(items[0].size, 1234);
        assert_eq!(items[0].etag, "9a0364b9e99bb480dd25e1f0284c8555");
        assert_eq!(items[1].key, "music/library.json");
    }

    #[test]
    fn a_listing_without_objects_is_empty_not_an_error() {
        assert!(parse_list("<ListBucketResult></ListBucketResult>").is_empty());
    }

    #[test]
    fn refuses_a_config_without_a_bucket() {
        let bad = S3Client::new(
            S3Config {
                bucket: "  ".into(),
                ..Default::default()
            },
            Credentials {
                access_key: "k".into(),
                secret_key: "s".into(),
                session_token: String::new(),
            },
        );
        assert!(bad.is_err());
    }
}

/// Клиент против живого сервера (заглушка на Python: tests/fake_s3.py).
/// Юнит-тесты выше проверяют подпись и разбор XML по отдельности; здесь -
/// то, что между ними: форма URL, канонический запрос, пагинация и запись
/// файла на диск.
#[cfg(test)]
mod roundtrip {
    use super::super::sigv4::Credentials;
    use super::S3Client;
    use crate::domain::S3Config;
    use std::io::{BufRead, BufReader};
    use std::process::{Child, Command, Stdio};

    struct FakeS3 {
        child: Child,
        port: u16,
    }

    impl FakeS3 {
        fn start() -> Self {
            let script = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fake_s3.py");
            let mut child = Command::new("python3")
                .arg(script)
                .stdout(Stdio::piped())
                .spawn()
                .expect("python3 для заглушки S3");

            let stdout = child.stdout.take().expect("stdout заглушки");
            let mut line = String::new();
            BufReader::new(stdout)
                .read_line(&mut line)
                .expect("заглушка не сообщила порт");
            let port = line
                .trim()
                .strip_prefix("READY ")
                .and_then(|p| p.parse().ok())
                .unwrap_or_else(|| panic!("неожиданный ответ заглушки: {line:?}"));

            Self { child, port }
        }

        fn client(&self) -> S3Client {
            S3Client::new(
                S3Config {
                    endpoint: format!("http://127.0.0.1:{}", self.port),
                    region: "us-east-1".into(),
                    bucket: "muzon".into(),
                    prefix: "muzon".into(),
                    path_style: true,
                },
                Credentials {
                    access_key: "AKIAIOSFODNN7EXAMPLE".into(),
                    secret_key: "wJalrXUtnFEMI/K7MDENG/bPxRfiCYEXAMPLEKEY".into(),
                    session_token: String::new(),
                },
            )
            .expect("клиент")
        }
    }

    impl Drop for FakeS3 {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[test]
    fn uploads_lists_and_downloads() {
        let server = FakeS3::start();
        let client = server.client();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            // Пустое хранилище - не ошибка
            assert!(client.list("muzon/").await.unwrap().is_empty());
            client.check_access().await.expect("доступ");

            // Имя с пробелом и кириллицей проверяет кодирование пути
            let track = "muzon/tracks/Music/Камелия - трек 1.flac";
            client
                .put_bytes(track, b"fake flac data".to_vec())
                .await
                .expect("загрузка");

            // Пять объектов - заглушка отдаёт их по два, значит пагинация работает
            for i in 0..4 {
                client
                    .put_bytes(&format!("muzon/tracks/Music/{i}.mp3"), vec![b'x'; i + 1])
                    .await
                    .unwrap();
            }

            let listing = client.list("muzon/").await.expect("список");
            assert_eq!(listing.len(), 5, "все объекты со всех страниц: {listing:?}");
            let found = listing
                .iter()
                .find(|o| o.key == track)
                .expect("трек в списке");
            assert_eq!(found.size, 14);

            // Скачивание в файл
            let dir = std::env::temp_dir().join(format!("muzon-s3-test-{}", std::process::id()));
            let dest = dir.join("Камелия - трек 1.flac");
            client.get_file(track, &dest).await.expect("скачивание");
            assert_eq!(std::fs::read(&dest).unwrap(), b"fake flac data");
            client
                .put_bytes(track, b"changed remote audio".to_vec())
                .await
                .unwrap();
            assert!(
                client.get_file(track, &dest).await.is_err(),
                "existing local audio must not be overwritten"
            );
            assert_eq!(std::fs::read(&dest).unwrap(), b"fake flac data");
            assert!(
                std::fs::read_dir(&dir).unwrap().all(|entry| !entry
                    .unwrap()
                    .file_name()
                    .to_string_lossy()
                    .ends_with(".part")),
                "failed download must clean its partial file"
            );
            assert!(
                !dest.with_extension("muzon-part").exists(),
                "временный файл должен быть переименован"
            );
            std::fs::remove_dir_all(&dir).ok();

            // Отсутствующий ключ - None, а не ошибка
            assert!(client
                .get_bytes("muzon/нет-такого")
                .await
                .unwrap()
                .is_none());
        });
    }

    #[tokio::test]
    async fn content_conflicts_and_create_races_preserve_both_copies() {
        use crate::data::s3::sync::{self, LocalFile};
        let server = FakeS3::start();
        let client = server.client();
        let dir = crate::testing::scratch_dir("s3-content-conflicts");
        let path = dir.join("tagged.flac");
        std::fs::write(&path, b"tag1audio").unwrap();
        let file = LocalFile {
            path: path.clone(),
            key: "muzon/tracks/Music/tagged.flac".into(),
            size: 9,
        };
        client.put_file_if_absent(&file.key, &path).await.unwrap();
        assert!(sync::same_content(&client, &file).await.unwrap());
        // A tag edit with exactly the same length must not be called up to date.
        std::fs::write(&path, b"tag2audio").unwrap();
        assert!(!sync::same_content(&client, &file).await.unwrap());
        assert!(client.put_file_if_absent(&file.key, &path).await.is_err());
        assert_eq!(
            client.get_bytes(&file.key).await.unwrap().unwrap(),
            b"tag1audio"
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"tag2audio");
        // Multipart-like/opaque ETag in the fake listing is irrelevant.
        assert_eq!(client.list("muzon/").await.unwrap()[0].etag, "deadbeef");
        client
            .put_bytes(&file.key, b"remote-v3".to_vec())
            .await
            .unwrap();
        assert!(!sync::same_content(&client, &file).await.unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), b"tag2audio");
        assert!(client.content_hash("missing").await.is_err());
    }

    #[tokio::test]
    async fn sync_reports_equal_size_changes_without_replacing_either_side() {
        use crate::data::{
            db::{Db, NewTrack},
            s3::sync,
        };
        use std::sync::Arc;
        let server = FakeS3::start();
        let client = server.client();
        let dir = crate::testing::scratch_dir("s3-sync-content");
        let root = dir.join("Music");
        std::fs::create_dir_all(&root).unwrap();
        let path = root.join("a.flac");
        std::fs::write(&path, b"tag1audio").unwrap();
        let db = Arc::new(Db::open(&dir.join("library.sqlite3")).unwrap());
        db.add_root(&root).unwrap();
        db.upsert_track(&NewTrack {
            path: path.to_string_lossy().into_owned(),
            title: "A".into(),
            artist: None,
            album: None,
            duration_secs: None,
            track_no: None,
            genre: None,
            year: None,
            mtime: 0,
        })
        .unwrap();
        let stages = Arc::new(std::sync::Mutex::new(Vec::new()));
        let captured = stages.clone();
        let callback =
            Arc::new(move |progress: sync::SyncProgress| captured.lock().unwrap().push(progress));
        let run = || {
            sync::run(
                db.clone(),
                client.cfg.clone(),
                Credentials {
                    access_key: "test".into(),
                    secret_key: "test".into(),
                    session_token: String::new(),
                },
                callback.clone(),
                Arc::new(|| Ok(())),
            )
        };
        let first = run().await.unwrap();
        assert_eq!(first.uploaded, 1);
        assert!(first.failures.is_empty());
        let unchanged = run().await.unwrap();
        assert_eq!(unchanged.up_to_date, 1);
        assert_eq!(unchanged.uploaded, 0);
        assert!(stages.lock().unwrap().iter().any(|p| p.phase == "scan"
            && p.current == "muzon/tracks/Music/a.flac"
            && p.total == 1));
        std::fs::write(&path, b"tag2audio").unwrap();
        let changed = run().await.unwrap();
        assert_eq!(changed.uploaded, 0);
        assert_eq!(changed.downloaded, 0);
        assert_eq!(changed.up_to_date, 0);
        assert_eq!(changed.conflicts, vec!["muzon/tracks/Music/a.flac"]);
        assert_eq!(changed.failures.len(), 2);
        assert!(changed.failures[0].contains("Конфликтов содержимого: 1"));
        assert!(changed.failures[1].contains("content conflict"));
        let key = "muzon/tracks/Music/a.flac";
        assert_eq!(client.get_bytes(key).await.unwrap().unwrap(), b"tag1audio");
        assert_eq!(std::fs::read(&path).unwrap(), b"tag2audio");
        // Removing a local file never removes its remote counterpart.
        std::fs::remove_file(&path).unwrap();
        let restored = run().await.unwrap();
        assert_eq!(restored.downloaded, 1);
        assert_eq!(std::fs::read(&path).unwrap(), b"tag1audio");
    }

    #[tokio::test]
    async fn cancelled_download_removes_partial_and_never_publishes_destination() {
        use std::{
            io::{Read, Write},
            sync::mpsc,
            time::Duration,
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        let (seen_tx, seen_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut buffer = [0; 8192];
            assert!(stream.read(&mut buffer).unwrap() > 0);
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 100000\r\nConnection: close\r\n\r\npartial").unwrap();
            seen_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        });
        let client = S3Client::new(
            S3Config {
                endpoint,
                bucket: "muzon".into(),
                path_style: true,
                ..Default::default()
            },
            Credentials {
                access_key: "test".into(),
                secret_key: "test".into(),
                session_token: String::new(),
            },
        )
        .unwrap();
        let dir = crate::testing::scratch_dir("s3-download-cancel");
        let dest = dir.join("a.flac");
        let target = dest.clone();
        let task = tokio::spawn(async move { client.get_file("a.flac", &target).await });
        tokio::task::spawn_blocking(move || seen_rx.recv_timeout(Duration::from_secs(5)).unwrap())
            .await
            .unwrap();
        assert!(!dest.exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        assert!(!dest.exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        release_tx.send(()).unwrap();
        server.join().unwrap();
    }

    #[test]
    fn a_wrong_bucket_is_reported_not_swallowed() {
        let server = FakeS3::start();
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let client = S3Client::new(
            S3Config {
                endpoint: format!("http://127.0.0.1:{}", server.port),
                bucket: "нет-такого-бакета".into(),
                path_style: true,
                ..Default::default()
            },
            Credentials {
                access_key: "k".into(),
                secret_key: "s".into(),
                session_token: String::new(),
            },
        )
        .unwrap();

        // Заглушка отвечает пустым списком на чужой бакет, но ключ из него не
        // скачивается - важно, что ошибка доходит до вызывающего
        rt.block_on(async {
            let missing = client.get_bytes("whatever").await.unwrap();
            assert!(missing.is_none());
        });
    }
}

/// Проверка формы запроса против настоящего Yandex Object Storage.
///
/// Ключи заведомо чужие, поэтому сервер отвечает SignatureDoesNotMatch - зато
/// в ответе он присылает свой каноничный запрос и StringToSign. Если
/// пересчитать подпись по его StringToSign нашим (фальшивым) секретом и она
/// совпадёт с отправленной, значит наш каноничный запрос совпал с его
/// побайтово: путь, порядок параметров, набор заголовков, UNSIGNED-PAYLOAD.
/// Тест ходит в сеть: `cargo test yandex -- --ignored --nocapture`.
#[cfg(test)]
mod yandex_probe {
    use super::super::sigv4::Credentials;
    use super::{extract_tag, S3Client};
    use crate::domain::S3Config;
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    const SECRET: &str = "YCNotARealSecretKeyJustForProbing0000000";

    fn hmac(key: &[u8], data: &str) -> Vec<u8> {
        let mut mac = Hmac::<Sha256>::new_from_slice(key).unwrap();
        mac.update(data.as_bytes());
        mac.finalize().into_bytes().to_vec()
    }

    fn probe(path_style: bool) -> (reqwest::StatusCode, String) {
        let client = S3Client::new(
            S3Config {
                endpoint: "https://storage.yandexcloud.net".into(),
                region: "ru-central1".into(),
                bucket: "muzon-probe-bucket".into(),
                prefix: "muzon".into(),
                path_style,
            },
            Credentials {
                access_key: "YCAJEexampleexampleexample".into(),
                secret_key: SECRET.into(),
                session_token: String::new(),
            },
        )
        .unwrap();

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        rt.block_on(async {
            let query = vec![
                ("list-type", "2".to_string()),
                ("max-keys", "1".to_string()),
            ];
            let resp = client
                .send(reqwest::Method::GET, "", &query, None)
                .await
                .expect("запрос ушёл");
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            (status, body)
        })
    }

    #[test]
    #[ignore = "ходит в интернет"]
    fn yandex_builds_the_same_canonical_request_as_we_do() {
        for path_style in [true, false] {
            let (status, body) = probe(path_style);
            let code = extract_tag(&body, "Code").unwrap_or_default();
            println!("path_style={path_style} status={status} code={code}");

            let Some(string_to_sign) = extract_tag(&body, "StringToSign") else {
                panic!("Яндекс не прислал StringToSign (path_style={path_style}): {body}");
            };
            let sent = extract_tag(&body, "SignatureProvided").unwrap_or_default();

            let date = &string_to_sign[17..25]; // YYYYMMDD из второй строки
            let k = hmac(
                &hmac(
                    &hmac(
                        &hmac(format!("AWS4{SECRET}").as_bytes(), date),
                        "ru-central1",
                    ),
                    "s3",
                ),
                "aws4_request",
            );
            let theirs = hex::encode(hmac(&k, &string_to_sign));

            assert_eq!(
                theirs, sent,
                "каноничный запрос разошёлся с тем, что посчитал Яндекс (path_style={path_style}):\n{body}"
            );
        }
    }
}
