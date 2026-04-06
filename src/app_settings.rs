use std::collections::HashSet;
use std::env;

/// Permission level associated with a Bearer token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenPermission {
    ReadOnly,
    ReadWrite,
}

/// Resolved token store: maps tokens to their permission level.
#[derive(Debug, Clone)]
pub struct TokenStore {
    read_only: HashSet<String>,
    read_write: HashSet<String>,
}

impl TokenStore {
    pub fn new(read_only: HashSet<String>, read_write: HashSet<String>) -> Self {
        Self {
            read_only,
            read_write,
        }
    }

    /// Look up a token and return its permission level, or None if not found.
    pub fn resolve(&self, token: &str) -> Option<TokenPermission> {
        if self.read_write.contains(token) {
            Some(TokenPermission::ReadWrite)
        } else if self.read_only.contains(token) {
            Some(TokenPermission::ReadOnly)
        } else {
            None
        }
    }
}

/// Application configuration loaded from environment variables.
#[derive(Debug, Clone)]
pub struct AppSettings {
    pub host: String,
    pub port: u16,
    pub s3_region: String,
    pub s3_bucket: String,
    pub s3_prefix: String,
    pub s3_endpoint: Option<String>,
    pub s3_access_key: Option<String>,
    pub s3_secret_key: Option<String>,
    pub s3_use_path_style: bool,
    pub max_body_size: usize,
    pub log_level: String,
    pub logs_directory: Option<String>,
    pub token_store: TokenStore,
}

impl AppSettings {
    /// Load settings from environment variables.
    /// Panics if required variables are missing.
    pub fn from_env() -> Self {
        let s3_bucket = env::var("S3_BUCKET").expect("S3_BUCKET environment variable is required");
        let s3_region = env::var("S3_REGION").expect("S3_REGION environment variable is required");

        let read_only_tokens =
            parse_comma_separated(&env::var("CACHE_TOKENS_READ_ONLY").unwrap_or_default());
        let read_write_tokens =
            parse_comma_separated(&env::var("CACHE_TOKENS_READ_WRITE").unwrap_or_default());

        let token_store = TokenStore::new(read_only_tokens, read_write_tokens);

        Self {
            host: env::var("HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
            port: env::var("PORT")
                .unwrap_or_else(|_| "8080".to_string())
                .parse()
                .expect("PORT must be a valid u16"),
            s3_region,
            s3_bucket,
            s3_prefix: env::var("S3_PREFIX").unwrap_or_else(|_| "rush-cache".to_string()),
            s3_endpoint: env::var("S3_ENDPOINT").ok(),
            s3_access_key: env::var("S3_ACCESS_KEY").ok(),
            s3_secret_key: env::var("S3_SECRET_KEY").ok(),
            s3_use_path_style: env::var("S3_USE_PATH_STYLE")
                .unwrap_or_else(|_| "false".to_string())
                .parse()
                .unwrap_or(false),
            max_body_size: env::var("MAX_BODY_SIZE")
                .unwrap_or_else(|_| "524288000".to_string())
                .parse()
                .expect("MAX_BODY_SIZE must be a valid usize"),
            log_level: env::var("LOG_LEVEL").unwrap_or_else(|_| "info".to_string()),
            logs_directory: env::var("LOGS_DIRECTORY").ok(),
            token_store,
        }
    }
}

/// Parse a comma-separated string into a HashSet, trimming whitespace and ignoring empty entries.
fn parse_comma_separated(input: &str) -> HashSet<String> {
    input
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_comma_separated() {
        let result = parse_comma_separated("tok_a, tok_b , tok_c");
        assert_eq!(result.len(), 3);
        assert!(result.contains("tok_a"));
        assert!(result.contains("tok_b"));
        assert!(result.contains("tok_c"));
    }

    #[test]
    fn test_parse_comma_separated_empty() {
        let result = parse_comma_separated("");
        assert!(result.is_empty());
    }

    #[test]
    fn test_parse_comma_separated_single() {
        let result = parse_comma_separated("tok_only");
        assert_eq!(result.len(), 1);
        assert!(result.contains("tok_only"));
    }

    #[test]
    fn test_token_store_resolve_read_write() {
        let store = TokenStore::new(
            HashSet::from(["ro_token".to_string()]),
            HashSet::from(["rw_token".to_string()]),
        );
        assert_eq!(store.resolve("rw_token"), Some(TokenPermission::ReadWrite));
    }

    #[test]
    fn test_token_store_resolve_read_only() {
        let store = TokenStore::new(
            HashSet::from(["ro_token".to_string()]),
            HashSet::from(["rw_token".to_string()]),
        );
        assert_eq!(store.resolve("ro_token"), Some(TokenPermission::ReadOnly));
    }

    #[test]
    fn test_token_store_resolve_unknown() {
        let store = TokenStore::new(
            HashSet::from(["ro_token".to_string()]),
            HashSet::from(["rw_token".to_string()]),
        );
        assert_eq!(store.resolve("unknown"), None);
    }

    #[test]
    fn test_token_store_read_write_takes_precedence() {
        // If a token appears in both sets, read-write wins
        let store = TokenStore::new(
            HashSet::from(["shared".to_string()]),
            HashSet::from(["shared".to_string()]),
        );
        assert_eq!(store.resolve("shared"), Some(TokenPermission::ReadWrite));
    }
}
