//! The seam between [`crate::ComfyHttp`] and the network.
//!
//! Every route the client uses is one request and one buffered response, so the
//! seam is a single method. Production sends it with reqwest; tests send it to a
//! [`FakeTransport`] that replays responses captured from a live ComfyUI
//! (`testdata/comfy-http/`) and records what was asked -- so a call sequence can
//! be asserted without a server (WORKFLOW section 5).
//!
//! Model downloads are the exception: gigabytes cannot be buffered, so
//! [`crate::download`] streams with reqwest directly and is tested through its
//! pure parts.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use bytes::Bytes;

/// The two HTTP methods ComfyUI's routes use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Method {
    /// `GET`
    Get,
    /// `POST`, always with a JSON body here.
    Post,
}

impl Method {
    fn as_str(self) -> &'static str {
        match self {
            Method::Get => "GET",
            Method::Post => "POST",
        }
    }
}

impl std::fmt::Display for Method {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One request: a method, a path (with any query string) relative to the base
/// URL, and an optional JSON body.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpRequest {
    /// `GET` or `POST`.
    pub method: Method,
    /// Starts with `/`, already percent-encoded.
    pub path: String,
    /// The serialised JSON body, for `POST`.
    pub body: Option<Vec<u8>>,
    /// Overrides the transport's default timeout. The address probe uses a short
    /// one so a dead port does not hold the Setup card for half a minute.
    pub timeout: Option<Duration>,
}

/// One buffered response.
#[derive(Debug, Clone, PartialEq)]
pub struct HttpResponse {
    /// The HTTP status code. Non-2xx is **not** an error at this layer; the
    /// client decides, because a `400` from `/prompt` carries the most useful
    /// body ComfyUI ever sends.
    pub status: u16,
    /// The raw body.
    pub body: Bytes,
}

/// Nothing usable came back at all.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum TransportError {
    /// Nobody accepted the connection: ComfyUI is not running at that address.
    #[error("connection refused or host unreachable: {0}")]
    Connect(String),
    /// Connected, then nothing arrived in time.
    #[error("timed out")]
    Timeout,
    /// Anything else the HTTP stack reported (TLS, a malformed URL, a reset).
    #[error("{0}")]
    Other(String),
}

/// A boxed future, so the trait stays object-safe and `ComfyHttp` can hold an
/// `Arc<dyn Transport>`.
pub type SendFuture<'a> =
    Pin<Box<dyn Future<Output = Result<HttpResponse, TransportError>> + Send + 'a>>;

/// Sends one request to `base` + `request.path`.
pub trait Transport: Send + Sync {
    /// Send the request and buffer the whole response.
    fn send<'a>(&'a self, base: &'a str, request: HttpRequest) -> SendFuture<'a>;
}

/// The default timeout for an API call. Generous, because `/object_info` on a
/// large install and `/view` of a long FLAC both take a moment.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

/// The production transport.
#[derive(Debug, Clone)]
pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    /// A client with a short connect timeout -- a ComfyUI on this machine either
    /// accepts at once or is not running.
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(3))
            .timeout(DEFAULT_TIMEOUT)
            .build()
            .unwrap_or_default();
        Self { client }
    }
}

impl Default for ReqwestTransport {
    fn default() -> Self {
        Self::new()
    }
}

impl Transport for ReqwestTransport {
    fn send<'a>(&'a self, base: &'a str, request: HttpRequest) -> SendFuture<'a> {
        Box::pin(async move {
            let url = format!("{base}{}", request.path);
            let mut builder = match request.method {
                Method::Get => self.client.get(&url),
                Method::Post => self
                    .client
                    .post(&url)
                    .header("Content-Type", "application/json")
                    .body(request.body.unwrap_or_default()),
            };
            if let Some(timeout) = request.timeout {
                builder = builder.timeout(timeout);
            }
            let response = builder.send().await.map_err(classify)?;
            let status = response.status().as_u16();
            let body = response.bytes().await.map_err(classify)?;
            Ok(HttpResponse { status, body })
        })
    }
}

/// Sort a reqwest failure into the three cases the UI distinguishes.
pub(crate) fn classify(error: reqwest::Error) -> TransportError {
    if error.is_timeout() {
        TransportError::Timeout
    } else if error.is_connect() {
        TransportError::Connect(error.to_string())
    } else {
        TransportError::Other(error.to_string())
    }
}

#[cfg(any(test, feature = "test-support"))]
pub use fake::FakeTransport;

#[cfg(any(test, feature = "test-support"))]
mod fake {
    use std::collections::{HashMap, VecDeque};
    use std::sync::Mutex;

    use bytes::Bytes;

    use super::{HttpRequest, HttpResponse, Method, SendFuture, Transport, TransportError};

    type Reply = Result<HttpResponse, TransportError>;

    /// A scripted ComfyUI.
    ///
    /// Replies are queued per `(method, path)`; each request takes the next one,
    /// and the **last reply repeats** so a poll loop can be scripted as
    /// "pending, pending, done" without counting polls. A request nobody
    /// scripted gets a `404` with a body naming it, which fails the test loudly
    /// rather than hanging it.
    #[derive(Default)]
    pub struct FakeTransport {
        replies: Mutex<HashMap<(Method, String), VecDeque<Reply>>>,
        calls: Mutex<Vec<HttpRequest>>,
    }

    impl FakeTransport {
        /// An empty script.
        pub fn new() -> Self {
            Self::default()
        }

        /// Queue a reply with this status and body.
        pub fn on(&self, method: Method, path: &str, status: u16, body: impl Into<Bytes>) {
            self.push(
                method,
                path,
                Ok(HttpResponse {
                    status,
                    body: body.into(),
                }),
            );
        }

        /// Queue a `200` whose body is `json`.
        pub fn on_json(&self, method: Method, path: &str, json: &serde_json::Value) {
            self.on(method, path, 200, json.to_string());
        }

        /// Queue a transport failure.
        pub fn fail(&self, method: Method, path: &str, error: TransportError) {
            self.push(method, path, Err(error));
        }

        fn push(&self, method: Method, path: &str, reply: Reply) {
            self.replies
                .lock()
                .expect("fake lock")
                .entry((method, path.to_string()))
                .or_default()
                .push_back(reply);
        }

        /// Every request sent, in order.
        pub fn calls(&self) -> Vec<HttpRequest> {
            self.calls.lock().expect("fake lock").clone()
        }

        /// `"METHOD /path"` for every request sent, in order -- the readable
        /// form a call-sequence assertion wants.
        pub fn call_lines(&self) -> Vec<String> {
            self.calls()
                .iter()
                .map(|c| format!("{} {}", c.method, c.path))
                .collect()
        }

        /// The JSON body of the `n`th request (0-based).
        pub fn body_of(&self, n: usize) -> serde_json::Value {
            let calls = self.calls();
            let body = calls[n].body.as_deref().unwrap_or(b"null");
            serde_json::from_slice(body).expect("request body is JSON")
        }
    }

    impl Transport for FakeTransport {
        fn send<'a>(&'a self, _base: &'a str, request: HttpRequest) -> SendFuture<'a> {
            let key = (request.method, request.path.clone());
            self.calls.lock().expect("fake lock").push(request);
            let reply = {
                let mut replies = self.replies.lock().expect("fake lock");
                match replies.get_mut(&key) {
                    Some(queue) if queue.len() > 1 => queue.pop_front(),
                    Some(queue) => queue.front().cloned(),
                    None => None,
                }
            };
            let reply = reply.unwrap_or_else(|| {
                Ok(HttpResponse {
                    status: 404,
                    body: Bytes::from(format!("unscripted request: {} {}", key.0, key.1)),
                })
            });
            Box::pin(async move { reply })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_fake_repeats_the_last_reply_and_records_calls() {
        let fake = FakeTransport::new();
        fake.on(Method::Get, "/queue", 200, "a");
        fake.on(Method::Get, "/queue", 200, "b");
        let req = || HttpRequest {
            method: Method::Get,
            path: "/queue".into(),
            body: None,
            timeout: None,
        };
        let bodies: Vec<Bytes> = vec![
            fake.send("x", req()).await.expect("ok").body,
            fake.send("x", req()).await.expect("ok").body,
            fake.send("x", req()).await.expect("ok").body,
        ];
        assert_eq!(bodies, vec!["a", "b", "b"]);
        assert_eq!(fake.call_lines(), vec!["GET /queue"; 3]);
    }

    #[tokio::test]
    async fn test_fake_answers_an_unscripted_request_with_a_404() {
        let fake = FakeTransport::new();
        let reply = fake
            .send(
                "x",
                HttpRequest {
                    method: Method::Post,
                    path: "/prompt".into(),
                    body: None,
                    timeout: None,
                },
            )
            .await
            .expect("ok");
        assert_eq!(reply.status, 404);
    }
}
