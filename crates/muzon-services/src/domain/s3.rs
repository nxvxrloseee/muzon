use serde::{Deserialize, Serialize};

/// Where the library is mirrored. Keys are never stored here - they live in the
/// system keyring (see data/s3/creds.rs).
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct S3Config {
    /// Empty means AWS in the given region
    pub endpoint: String,
    pub region: String,
    pub bucket: String,
    /// Everything Muzon writes lives under this prefix
    pub prefix: String,
    /// MinIO and most self-hosted servers need path-style addressing
    pub path_style: bool,
}

impl Default for S3Config {
    fn default() -> Self {
        Self {
            endpoint: String::new(),
            region: "us-east-1".into(),
            bucket: String::new(),
            prefix: "muzon".into(),
            path_style: true,
        }
    }
}

impl S3Config {
    pub fn is_configured(&self) -> bool {
        !self.bucket.trim().is_empty()
    }

    /// The prefix with exactly one trailing slash, or nothing at all
    pub fn normalised_prefix(&self) -> String {
        let p = self.prefix.trim().trim_matches('/');
        if p.is_empty() {
            String::new()
        } else {
            format!("{p}/")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prefix_always_ends_with_one_slash() {
        let mut cfg = S3Config::default();
        for raw in ["muzon", "/muzon/", "muzon//"] {
            cfg.prefix = raw.into();
            assert_eq!(cfg.normalised_prefix(), "muzon/", "для {raw}");
        }
        cfg.prefix = "  ".into();
        assert_eq!(cfg.normalised_prefix(), "");
    }
}
