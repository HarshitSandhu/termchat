use std::sync::OnceLock;
use std::time::Duration;

static CLIENT: OnceLock<reqwest::Client> = OnceLock::new();

/// A process-wide HTTP client so requests share connection pooling and TLS
/// sessions.
///
/// There is deliberately no total timeout here: it would also cover reading a
/// streamed body and cut off long responses. Instead we bound connecting and
/// the gap between reads; non-streaming requests should set their own total
/// limit via `RequestBuilder::timeout`.
pub fn client() -> &'static reqwest::Client {
    CLIENT.get_or_init(|| {
        reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(15))
            .read_timeout(Duration::from_secs(90))
            .build()
            .expect("failed to build shared HTTP client")
    })
}
