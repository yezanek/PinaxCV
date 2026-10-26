//! The HTTP side of the providers: one JSON POST, bounded retries, and
//! cancellation. Bodies are never logged or put into errors.

use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio_util::sync::CancellationToken;

use crate::LlmError;

/// Longest a provider may take to connect.
pub const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
/// Longest one request may take in total by default (spec section 11.2).
/// Local models can be slower; see [`UreqTransport::with_timeout`].
pub const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
/// Largest response body read, in bytes.
pub const MAX_RESPONSE_BYTES: u64 = 8 * 1024 * 1024;
/// Waits before the second and third attempt of a retryable failure.
pub const RETRY_DELAYS: [Duration; 2] = [Duration::from_secs(1), Duration::from_secs(4)];

/// One JSON POST.
#[derive(Clone, Debug, PartialEq)]
pub struct HttpRequest {
    pub url: String,
    /// Header names and values. Values may hold API keys: never log them.
    pub headers: Vec<(&'static str, String)>,
    pub body: Value,
}

/// The status and the body, parsed as JSON (`Value::Null` when it is not).
#[derive(Clone, Debug, PartialEq)]
pub struct HttpResponse {
    pub status: u16,
    pub body: Value,
}

/// Why no response arrived.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransportError {
    Timeout,
    /// Connection or TLS failure; the text names the kind, not the body.
    Network(String),
}

/// Sends requests. Tests replace it with a recording fake.
pub trait Transport: Send + Sync {
    /// Blocks until the response arrives or the request fails.
    fn post_json(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError>;
}

/// The real transport: `ureq` with rustls and the timeouts above.
pub struct UreqTransport {
    agent: ureq::Agent,
}

impl Default for UreqTransport {
    fn default() -> Self {
        Self::with_timeout(REQUEST_TIMEOUT)
    }
}

impl UreqTransport {
    /// A transport whose requests may take up to `timeout` in total.
    pub fn with_timeout(timeout: Duration) -> Self {
        let agent = ureq::Agent::config_builder()
            .timeout_connect(Some(CONNECT_TIMEOUT.min(timeout)))
            .timeout_global(Some(timeout))
            // A redirect could carry the API-key header to another host, so a
            // 3xx is returned as it is and fails like any other status.
            .max_redirects(0)
            // Statuses are handled here, so error bodies are read and dropped.
            .http_status_as_error(false)
            .build()
            .into();
        Self { agent }
    }
}

impl Transport for UreqTransport {
    fn post_json(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
        let mut builder = self.agent.post(&request.url);
        for (name, value) in &request.headers {
            builder = builder.header(*name, value);
        }
        let mut response = builder.send_json(&request.body).map_err(transport_error)?;
        let status = response.status().as_u16();
        let text = response
            .body_mut()
            .with_config()
            .limit(MAX_RESPONSE_BYTES)
            .read_to_string()
            .map_err(transport_error)?;
        Ok(HttpResponse {
            status,
            body: serde_json::from_str(&text).unwrap_or(Value::Null),
        })
    }
}

fn transport_error(e: ureq::Error) -> TransportError {
    match e {
        ureq::Error::Timeout(_) => TransportError::Timeout,
        // The kind only: some errors quote the URL, which can hold a host the
        // user typed but nothing secret, and never the body.
        other => TransportError::Network(error_kind(&other)),
    }
}

fn error_kind(e: &ureq::Error) -> String {
    let debug = format!("{e:?}");
    debug
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .unwrap_or("network")
        .to_owned()
}

/// Statuses worth another try: rate limits and overloaded or failing servers.
pub fn retryable(status: u16) -> bool {
    matches!(status, 408 | 429 | 500 | 502 | 503 | 504 | 529)
}

/// Sends `request` on a blocking thread, retrying network failures and
/// [`retryable`] statuses up to [`RETRY_DELAYS`]`.len()` times. Returns the
/// body of a 2xx response. Cancelling returns at once; a request already
/// sent finishes in the background and is dropped.
pub async fn send(
    transport: Arc<dyn Transport>,
    request: HttpRequest,
    cancel: CancellationToken,
) -> Result<Value, LlmError> {
    if cancel.is_cancelled() {
        return Err(LlmError::Cancelled);
    }
    let token = cancel.clone();
    let work = tokio::task::spawn_blocking(move || {
        attempts(transport.as_ref(), &request, &token, &RETRY_DELAYS)
    });
    tokio::select! {
        () = cancel.cancelled() => Err(LlmError::Cancelled),
        joined = work => joined.unwrap_or_else(|_| Err(LlmError::Provider("the request thread failed".into()))),
    }
}

fn attempts(
    transport: &dyn Transport,
    request: &HttpRequest,
    cancel: &CancellationToken,
    delays: &[Duration],
) -> Result<Value, LlmError> {
    let mut delays = delays.iter();
    loop {
        let failure = match transport.post_json(request) {
            Ok(r) if (200..300).contains(&r.status) => return Ok(r.body),
            Ok(r) if retryable(r.status) => LlmError::Http { status: r.status },
            Ok(r) => return Err(LlmError::Http { status: r.status }),
            Err(TransportError::Timeout) => return Err(LlmError::Timeout),
            Err(TransportError::Network(kind)) => {
                LlmError::Provider(format!("could not reach the provider ({kind})"))
            }
        };
        let Some(delay) = delays.next() else {
            return Err(failure);
        };
        if wait(*delay, cancel) {
            return Err(LlmError::Cancelled);
        }
    }
}

/// Sleeps `delay` in short steps; returns true if cancelled meanwhile.
fn wait(delay: Duration, cancel: &CancellationToken) -> bool {
    const STEP: Duration = Duration::from_millis(50);
    let mut left = delay;
    while !left.is_zero() {
        if cancel.is_cancelled() {
            return true;
        }
        let step = left.min(STEP);
        std::thread::sleep(step);
        left -= step;
    }
    cancel.is_cancelled()
}

/// The host of `url`, for names shown to the user. Any user name or
/// password in the URL is left out.
pub fn host(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
}

#[cfg(test)]
pub(crate) mod testing {
    use std::collections::VecDeque;
    use std::sync::Mutex;

    use super::*;

    /// Answers with queued responses and records each request.
    #[derive(Default)]
    pub struct Recorder {
        pub replies: Mutex<VecDeque<Result<HttpResponse, TransportError>>>,
        pub sent: Mutex<Vec<HttpRequest>>,
    }

    impl Recorder {
        pub fn replying(replies: Vec<Result<HttpResponse, TransportError>>) -> Arc<Self> {
            Arc::new(Self {
                replies: Mutex::new(replies.into()),
                sent: Mutex::default(),
            })
        }

        pub fn ok(body: Value) -> Result<HttpResponse, TransportError> {
            Ok(HttpResponse { status: 200, body })
        }

        pub fn sent(&self) -> Vec<HttpRequest> {
            self.sent.lock().expect("lock").clone()
        }
    }

    impl Transport for Recorder {
        fn post_json(&self, request: &HttpRequest) -> Result<HttpResponse, TransportError> {
            self.sent.lock().expect("lock").push(request.clone());
            self.replies
                .lock()
                .expect("lock")
                .pop_front()
                .unwrap_or(Err(TransportError::Network("empty".into())))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::testing::Recorder;
    use super::*;

    const NO_WAIT: [Duration; 2] = [Duration::ZERO; 2];

    fn request() -> HttpRequest {
        HttpRequest {
            url: "https://example.test/v1".into(),
            headers: Vec::new(),
            body: serde_json::json!({}),
        }
    }

    fn status(status: u16) -> Result<HttpResponse, TransportError> {
        Ok(HttpResponse {
            status,
            body: Value::Null,
        })
    }

    /// Serves one connection on localhost with `reply` (`None`: never answer).
    fn serve_once(reply: Option<&'static str>) -> String {
        use std::io::{Read, Write};
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let url = format!("http://{}/v1", listener.local_addr().expect("addr"));
        std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept");
            // Read the whole request first: closing a socket with unread
            // bytes resets the connection on Windows before the reply lands.
            let mut req = Vec::new();
            let mut buf = [0u8; 4096];
            while let Ok(n) = stream.read(&mut buf) {
                if n == 0 {
                    break;
                }
                req.extend_from_slice(&buf[..n]);
                let text = String::from_utf8_lossy(&req);
                if let Some(end) = text.find("\r\n\r\n") {
                    let len = text[..end]
                        .lines()
                        .find_map(|l| {
                            let (k, v) = l.split_once(':')?;
                            k.eq_ignore_ascii_case("content-length")
                                .then(|| v.trim().parse::<usize>().ok())?
                        })
                        .unwrap_or(0);
                    if req.len() >= end + 4 + len {
                        break;
                    }
                }
            }
            match reply {
                Some(r) => {
                    let _ = stream.write_all(r.as_bytes());
                    let _ = stream.flush();
                    let _ = stream.shutdown(std::net::Shutdown::Write);
                }
                None => std::thread::sleep(Duration::from_secs(2)),
            }
        });
        url
    }

    #[test]
    fn ureq_transport_reads_status_and_json() {
        let url = serve_once(Some(
            "HTTP/1.1 429 Too Many Requests\r\ncontent-type: application/json\r\ncontent-length: 11\r\nconnection: close\r\n\r\n{\"a\": true}",
        ));
        let r = UreqTransport::default()
            .post_json(&HttpRequest { url, ..request() })
            .expect("response");
        assert_eq!(r.status, 429);
        assert_eq!(r.body, serde_json::json!({ "a": true }));
        assert!(retryable(r.status) && !retryable(400) && !retryable(200));
    }

    #[test]
    fn ureq_transport_times_out() {
        let url = serve_once(None);
        let r = UreqTransport::with_timeout(Duration::from_millis(200))
            .post_json(&HttpRequest { url, ..request() });
        assert_eq!(r, Err(TransportError::Timeout));
    }

    #[test]
    fn hosts_are_taken_from_urls() {
        assert_eq!(
            host("https://api.anthropic.com/v1/messages"),
            "api.anthropic.com"
        );
        assert_eq!(host("http://localhost:11434/v1"), "localhost:11434");
        assert_eq!(host("example.org"), "example.org");
        assert_eq!(host("https://u:p@example.org/v1"), "example.org");
        assert_eq!(host("http://token@localhost:8080"), "localhost:8080");
    }

    #[test]
    fn retryable_failures_are_retried_then_reported() {
        let t = Recorder::replying(vec![status(529), Recorder::ok(serde_json::json!({"a": 1}))]);
        let out = attempts(t.as_ref(), &request(), &CancellationToken::new(), &NO_WAIT);
        assert_eq!(out.expect("ok"), serde_json::json!({"a": 1}));
        assert_eq!(t.sent().len(), 2);

        let t = Recorder::replying(vec![status(503), status(503), status(503), status(503)]);
        let out = attempts(t.as_ref(), &request(), &CancellationToken::new(), &NO_WAIT);
        assert!(matches!(out, Err(LlmError::Http { status: 503 })));
        assert_eq!(t.sent().len(), 3);
    }

    #[test]
    fn client_errors_and_timeouts_are_not_retried() {
        let t = Recorder::replying(vec![status(401)]);
        let out = attempts(t.as_ref(), &request(), &CancellationToken::new(), &NO_WAIT);
        assert!(matches!(out, Err(LlmError::Http { status: 401 })));
        let t = Recorder::replying(vec![Err(TransportError::Timeout)]);
        let out = attempts(t.as_ref(), &request(), &CancellationToken::new(), &NO_WAIT);
        assert!(matches!(out, Err(LlmError::Timeout)));
        assert_eq!(t.sent().len(), 1);
    }

    #[test]
    fn cancelling_stops_the_retry_wait() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let t = Recorder::replying(vec![status(429), status(200)]);
        assert!(matches!(
            attempts(t.as_ref(), &request(), &cancel, &NO_WAIT),
            Err(LlmError::Cancelled)
        ));
        assert_eq!(t.sent().len(), 1);
    }

    #[tokio::test]
    async fn a_cancelled_request_is_not_sent() {
        let cancel = CancellationToken::new();
        cancel.cancel();
        let t = Recorder::replying(vec![status(200)]);
        let out = send(t.clone(), request(), cancel).await;
        assert!(matches!(out, Err(LlmError::Cancelled)));
        assert!(t.sent().is_empty());
    }
}
