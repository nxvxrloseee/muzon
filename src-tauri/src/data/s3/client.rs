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
            .build()?;
        Ok(Self { http, cfg, creds })
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
            req = req.header("content-length", body.len()).body(body);
        }

        Ok(req.send().await?)
    }

    async fn check(resp: reqwest::Response, what: &str) -> anyhow::Result<reqwest::Response> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        let detail = extract_tag(&body, "Message").unwrap_or_else(|| body.chars().take(200).collect());
        anyhow::bail!("{what}: {status} {detail}")
    }

    /// Everything under `prefix`, following continuation tokens.
    pub async fn list(&self, prefix: &str) -> anyhow::Result<Vec<RemoteObject>> {
        let mut out = Vec::new();
        let mut token: Option<String> = None;

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
            let body = Self::check(resp, "не удалось получить список объектов")
                .await?
                .text()
                .await?;

            out.extend(parse_list(&body));
            match extract_tag(&body, "NextContinuationToken") {
                Some(t) if extract_tag(&body, "IsTruncated").as_deref() == Some("true") => {
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

    /// Files are read into memory before sending: tracks are tens of megabytes,
    /// and streaming would mean another dependency for the request body.
    pub async fn put_file(&self, key: &str, path: &Path) -> anyhow::Result<()> {
        let body = std::fs::read(path)?;
        self.put_bytes(key, body).await
    }

    pub async fn get_bytes(&self, key: &str) -> anyhow::Result<Option<Vec<u8>>> {
        let resp = self.send(reqwest::Method::GET, key, &[], None).await?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let resp = Self::check(resp, &format!("не удалось скачать {key}")).await?;
        Ok(Some(resp.bytes().await?.to_vec()))
    }

    /// Downloads to a temporary file next to the target and renames it, so an
    /// interrupted download never leaves a half-written track in the library.
    pub async fn get_file(&self, key: &str, dest: &Path) -> anyhow::Result<()> {
        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = dest.with_extension("muzon-part");

        let resp = self.send(reqwest::Method::GET, key, &[], None).await?;
        let mut resp = Self::check(resp, &format!("не удалось скачать {key}")).await?;

        {
            use std::io::Write;
            let mut file = std::fs::File::create(&tmp)?;
            while let Some(chunk) = resp.chunk().await? {
                file.write_all(&chunk)?;
            }
            file.flush()?;
        }
        std::fs::rename(&tmp, dest)?;
        Ok(())
    }

    /// A cheap request that proves the endpoint, the bucket and the keys all
    /// work together - used by the "check connection" button.
    pub async fn check_access(&self) -> anyhow::Result<()> {
        let query = vec![("list-type", "2".to_string()), ("max-keys", "1".to_string())];
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
        assert_eq!(url, "https://muzon.s3.eu-central-1.amazonaws.com/tracks/a.mp3");
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
            let found = listing.iter().find(|o| o.key == track).expect("трек в списке");
            assert_eq!(found.size, 14);

            // Скачивание в файл
            let dir = std::env::temp_dir().join(format!("muzon-s3-test-{}", std::process::id()));
            let dest = dir.join("Камелия - трек 1.flac");
            client.get_file(track, &dest).await.expect("скачивание");
            assert_eq!(std::fs::read(&dest).unwrap(), b"fake flac data");
            assert!(
                !dest.with_extension("muzon-part").exists(),
                "временный файл должен быть переименован"
            );
            std::fs::remove_dir_all(&dir).ok();

            // Отсутствующий ключ - None, а не ошибка
            assert!(client.get_bytes("muzon/нет-такого").await.unwrap().is_none());
        });
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
            let query = vec![("list-type", "2".to_string()), ("max-keys", "1".to_string())];
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
                    &hmac(&hmac(format!("AWS4{SECRET}").as_bytes(), date), "ru-central1"),
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
