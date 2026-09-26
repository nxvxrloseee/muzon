//! AWS Signature Version 4 for S3 requests.
//!
//! Written by hand rather than pulled in with the AWS SDK: Muzon needs four
//! operations (list, head, get, put), and the SDK would add dozens of crates to
//! a package that is already slow to build. The signature itself is a hash, an
//! HMAC chain and some string formatting.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

pub const UNSIGNED_PAYLOAD: &str = "UNSIGNED-PAYLOAD";

#[derive(Debug, Clone)]
pub struct Credentials {
    pub access_key: String,
    pub secret_key: String,
    /// For temporary credentials (STS); empty for ordinary keys
    pub session_token: String,
}

/// Everything the signature covers, in the order SigV4 wants it.
pub struct Request<'a> {
    pub method: &'a str,
    /// Already URL-encoded, starting with '/'
    pub path: &'a str,
    /// Sorted `name=value` pairs, already encoded
    pub query: &'a str,
    pub host: &'a str,
    pub payload_hash: &'a str,
    /// YYYYMMDDTHHMMSSZ
    pub timestamp: &'a str,
    pub region: &'a str,
    /// "s3" in practice; a parameter so the signer can be checked against the
    /// published SigV4 test vectors, which use a service called "service"
    pub service: &'a str,
    /// S3 wants x-amz-content-sha256 on every request; the generic test
    /// vectors don't have it
    pub content_sha_header: bool,
}

pub fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

fn hmac(key: &[u8], data: &str) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("HMAC ключ любой длины");
    mac.update(data.as_bytes());
    mac.finalize().into_bytes().to_vec()
}

/// Percent-encoding as SigV4 defines it: unreserved characters stay, and '/'
/// stays in paths but not in query values.
pub fn uri_encode(s: &str, keep_slash: bool) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char)
            }
            b'/' if keep_slash => out.push('/'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// The headers to send, including `Authorization`. The caller adds them to the
/// request as they are.
pub fn sign(req: &Request, creds: &Credentials) -> Vec<(String, String)> {
    let date = &req.timestamp[..8];
    let scope = format!("{date}/{}/{}/aws4_request", req.region, req.service);

    let mut headers = vec![
        ("host".to_string(), req.host.to_string()),
        ("x-amz-date".to_string(), req.timestamp.to_string()),
    ];
    if req.content_sha_header {
        headers.push((
            "x-amz-content-sha256".to_string(),
            req.payload_hash.to_string(),
        ));
    }
    if !creds.session_token.is_empty() {
        headers.push((
            "x-amz-security-token".to_string(),
            creds.session_token.clone(),
        ));
    }
    headers.sort_by(|a, b| a.0.cmp(&b.0));

    let canonical_headers: String = headers
        .iter()
        .map(|(k, v)| format!("{k}:{}\n", v.trim()))
        .collect();
    let signed_headers = headers
        .iter()
        .map(|(k, _)| k.as_str())
        .collect::<Vec<_>>()
        .join(";");

    let canonical_request = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        req.method, req.path, req.query, canonical_headers, signed_headers, req.payload_hash
    );

    let string_to_sign = format!(
        "AWS4-HMAC-SHA256\n{}\n{}\n{}",
        req.timestamp,
        scope,
        sha256_hex(canonical_request.as_bytes())
    );

    let k_date = hmac(format!("AWS4{}", creds.secret_key).as_bytes(), date);
    let k_region = hmac(&k_date, req.region);
    let k_service = hmac(&k_region, req.service);
    let k_signing = hmac(&k_service, "aws4_request");
    let signature = hex::encode(hmac(&k_signing, &string_to_sign));

    let authorization = format!(
        "AWS4-HMAC-SHA256 Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        creds.access_key
    );

    let mut out = headers;
    out.push(("authorization".to_string(), authorization));
    out
}

/// `YYYYMMDDTHHMMSSZ` from a unix timestamp - the only date handling S3 needs,
/// so no calendar crate.
pub fn amz_timestamp(unix_secs: u64) -> String {
    let days = (unix_secs / 86_400) as i64;
    let secs_of_day = unix_secs % 86_400;
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}{m:02}{d:02}T{:02}{:02}{:02}Z",
        secs_of_day / 3600,
        (secs_of_day % 3600) / 60,
        secs_of_day % 60
    )
}

/// Howard Hinnant's days-to-civil algorithm.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;


    /// "get-vanilla" from the published SigV4 test suite - the one case whose
    /// expected signature is a documented constant.
    #[test]
    fn matches_the_published_sigv4_vector() {
        let headers = sign(
            &Request {
                method: "GET",
                path: "/",
                query: "",
                host: "example.amazonaws.com",
                payload_hash: &sha256_hex(b""),
                timestamp: "20150830T123600Z",
                region: "us-east-1",
                service: "service",
                content_sha_header: false,
            },
            &Credentials {
                access_key: "AKIDEXAMPLE".into(),
                secret_key: "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY".into(),
                session_token: String::new(),
            },
        );

        let auth = headers
            .iter()
            .find(|(k, _)| k == "authorization")
            .map(|(_, v)| v.clone())
            .unwrap();

        assert!(
            auth.contains(
                "Signature=5fa00fa31553b73ebf1942676e86291e8372ff2a2260956d9b8aae1d763fbf31"
            ),
            "подпись не совпала с эталоном SigV4: {auth}"
        );
        assert!(auth.contains("SignedHeaders=host;x-amz-date"));
        assert!(auth.contains("Credential=AKIDEXAMPLE/20150830/us-east-1/service/aws4_request"));
    }

    #[test]
    fn timestamps_are_utc_and_zero_padded() {
        assert_eq!(amz_timestamp(1_369_353_600), "20130524T000000Z");
        assert_eq!(amz_timestamp(0), "19700101T000000Z");
        assert_eq!(amz_timestamp(1_763_000_045), "20251113T021405Z");
    }

    #[test]
    fn encodes_keys_the_way_s3_expects() {
        assert_eq!(uri_encode("a b", false), "a%20b");
        assert_eq!(uri_encode("tracks/a b.mp3", true), "tracks/a%20b.mp3");
        assert_eq!(uri_encode("tracks/a b.mp3", false), "tracks%2Fa%20b.mp3");
        assert_eq!(uri_encode("Ю.flac", true), "%D0%AE.flac");
        assert_eq!(uri_encode("~-_.", true), "~-_.");
    }
}
