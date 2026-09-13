use std::sync::OnceLock;
use std::time::Duration;

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// A process-wide HTTP client so requests share connection pooling and TLS
/// sessions. Individual requests may override the timeout via
/// `RequestBuilder::timeout`.
pub fn client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .timeout(Duration::from_secs(120))
            .build()
            .expect("failed to build shared HTTP client")
    })
}
