//! [`ComfyHttp`]: one typed method per verified route.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use serde::de::DeserializeOwned;
use serde_json::{json, Map, Value};

use crate::error::{ComfyError, Rejection};
use crate::session_log::SessionLog;
use crate::transport::{HttpRequest, HttpResponse, Method, ReqwestTransport, Transport};
use crate::types::{HistoryRecord, NodeSchema, OutputRef, QueueState, SystemStats};

/// Where to look for ComfyUI when the user has not set an address, in order.
/// 8188 is every install observed (Desktop, portable, `python main.py`); 8000 is
/// older Desktop builds' default, kept as a fallback probe (COMFY-HTTP-SURFACE 2).
pub const DEFAULT_URLS: [&str; 2] = ["http://127.0.0.1:8188", "http://127.0.0.1:8000"];

/// How long a probe waits before calling an address dead.
const PROBE_TIMEOUT: Duration = Duration::from_secs(3);

/// A client for one ComfyUI.
#[derive(Clone)]
pub struct ComfyHttp {
    base: String,
    transport: Arc<dyn Transport>,
    log: Option<SessionLog>,
    client_id: String,
}

impl std::fmt::Debug for ComfyHttp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ComfyHttp")
            .field("base", &self.base)
            .finish()
    }
}

impl ComfyHttp {
    /// A production client for `url`, logging to `log` when given.
    pub fn new(url: &str, log: Option<SessionLog>) -> Self {
        Self::with_transport(url, Arc::new(ReqwestTransport::new()), log)
    }

    /// A client over any transport -- the seam tests use.
    pub fn with_transport(
        url: &str,
        transport: Arc<dyn Transport>,
        log: Option<SessionLog>,
    ) -> Self {
        Self {
            base: normalize_url(url),
            transport,
            log,
            client_id: mint_client_id(),
        }
    }

    /// The normalized base URL (`http://127.0.0.1:8188`, no trailing slash).
    pub fn base_url(&self) -> &str {
        &self.base
    }

    /// Whether this ComfyUI is on this machine, judged by its address. Model
    /// downloads need that (ARCHITECTURE 3): the directories ComfyUI reports
    /// must exist here for a file written here to be one ComfyUI can read.
    pub fn is_loopback(&self) -> bool {
        let host = host_of(&self.base);
        host == "localhost" || host == "::1" || host == "[::1]" || host.starts_with("127.")
    }

    async fn send(&self, request: HttpRequest) -> Result<HttpResponse, ComfyError> {
        let route = format!("{} {}", request.method, request.path);
        if let Some(log) = &self.log {
            let mut args = Map::new();
            args.insert("url".into(), Value::String(self.base.clone()));
            if let Some(body) = &request.body {
                // The workflow is the record of what ran; it is logged in full
                // (redacted), as the comfy-mcp era logged its tool arguments.
                if let Ok(v) = serde_json::from_slice::<Value>(body) {
                    args.insert("body".into(), v);
                }
            }
            log.log_call(&route, &args);
        }
        let result = self.transport.send(&self.base, request).await;
        match result {
            Ok(response) => {
                if let Some(log) = &self.log {
                    let text = String::from_utf8_lossy(&response.body);
                    // `/view` returns megabytes of audio; a log entry needs
                    // to say that it happened, not carry the file.
                    let text = if text.len() > 4096 {
                        format!("<{} bytes>", response.body.len())
                    } else {
                        text.into_owned()
                    };
                    log.log_result(&route, (200..300).contains(&response.status), &text);
                }
                Ok(response)
            }
            Err(e) => {
                if let Some(log) = &self.log {
                    log.log_result(&route, false, &e.to_string());
                }
                Err(ComfyError::from_transport(&self.base, e))
            }
        }
    }

    async fn get(&self, path: String) -> Result<HttpResponse, ComfyError> {
        self.send(HttpRequest {
            method: Method::Get,
            path,
            body: None,
            timeout: None,
        })
        .await
    }

    async fn post(&self, path: &str, body: &Value) -> Result<HttpResponse, ComfyError> {
        self.send(HttpRequest {
            method: Method::Post,
            path: path.to_string(),
            body: Some(body.to_string().into_bytes()),
            timeout: None,
        })
        .await
    }

    async fn get_json<T: DeserializeOwned>(&self, path: String) -> Result<T, ComfyError> {
        let route = format!("GET {path}");
        let response = self.get(path).await?;
        expect_ok(&route, &response)?;
        decode(&route, &response.body)
    }

    /// `GET /system_stats`, with the short probe timeout. The health check, and
    /// the only response that proves the thing answering is ComfyUI.
    pub async fn system_stats(&self) -> Result<SystemStats, ComfyError> {
        let route = "GET /system_stats".to_string();
        let response = self
            .send(HttpRequest {
                method: Method::Get,
                path: "/system_stats".into(),
                body: None,
                timeout: Some(PROBE_TIMEOUT),
            })
            .await?;
        expect_ok(&route, &response)?;
        let value: Value = decode(&route, &response.body)?;
        // Something answered with JSON; it is ComfyUI only if it has the shape.
        if value.get("system").is_none() {
            return Err(ComfyError::Decode {
                route,
                detail: "no `system` block -- not a ComfyUI".into(),
            });
        }
        serde_json::from_value(value).map_err(|e| ComfyError::Decode {
            route,
            detail: e.to_string(),
        })
    }

    /// `GET /object_info/<class>`. An unknown class answers `{}`, which becomes
    /// [`ComfyError::UnknownNode`].
    pub async fn object_info(&self, class: &str) -> Result<NodeSchema, ComfyError> {
        let path = format!("/object_info/{}", encode_segment(class));
        let route = format!("GET {path}");
        let value: Value = self.get_json(path).await?;
        match value.get(class) {
            Some(entry) => NodeSchema::from_object_info(class, entry).ok_or(ComfyError::Decode {
                route,
                detail: format!("{class} has no `input` block"),
            }),
            None => Err(ComfyError::UnknownNode(class.to_string())),
        }
    }

    /// `GET /models/<folder>`: the files ComfyUI can load from that folder,
    /// across every directory it searches. Refreshes live (COMFY-HTTP-SURFACE 3).
    pub async fn models_in(&self, folder: &str) -> Result<Vec<String>, ComfyError> {
        self.get_json(format!("/models/{}", encode_segment(folder)))
            .await
    }

    /// `GET /models`: the folder names ComfyUI knows.
    pub async fn model_folders(&self) -> Result<Vec<String>, ComfyError> {
        self.get_json("/models".into()).await
    }

    /// `GET /internal/folder_paths`: every folder's directories, in ComfyUI's
    /// search order. `/internal/` is not a stable API; callers degrade, never
    /// guess, when this fails.
    pub async fn folder_paths(&self) -> Result<BTreeMap<String, Vec<PathBuf>>, ComfyError> {
        let raw: BTreeMap<String, Value> = self.get_json("/internal/folder_paths".into()).await?;
        // Each value is a list of paths; tolerate ComfyUI's older
        // `[[paths], [extensions]]` pair shape by taking the first list.
        Ok(raw
            .into_iter()
            .map(|(folder, value)| {
                let list = match value.get(0) {
                    Some(Value::Array(inner)) => inner.clone(),
                    _ => value.as_array().cloned().unwrap_or_default(),
                };
                let dirs = list
                    .iter()
                    .filter_map(Value::as_str)
                    .map(PathBuf::from)
                    .collect();
                (folder, dirs)
            })
            .collect())
    }

    /// `POST /prompt`: queue an API-format workflow and return its prompt id.
    /// A `400` is ComfyUI's validation refusal, parsed into
    /// [`ComfyError::Rejected`].
    pub async fn submit(&self, graph: &Value) -> Result<String, ComfyError> {
        let route = "POST /prompt".to_string();
        let response = self
            .post(
                "/prompt",
                &json!({"prompt": graph, "client_id": self.client_id}),
            )
            .await?;
        if response.status == 400 {
            let body: Value = decode(&route, &response.body)?;
            return Err(match Rejection::parse(&body) {
                Some(rejection) => ComfyError::Rejected(rejection),
                None => ComfyError::Http {
                    route,
                    status: 400,
                    body: body.to_string(),
                },
            });
        }
        expect_ok(&route, &response)?;
        let body: Value = decode(&route, &response.body)?;
        body.get("prompt_id")
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or(ComfyError::Decode {
                route,
                detail: "no prompt_id".into(),
            })
    }

    /// `GET /queue`.
    pub async fn queue(&self) -> Result<QueueState, ComfyError> {
        let route = "GET /queue".to_string();
        let value: Value = self.get_json("/queue".into()).await?;
        QueueState::from_value(&value).ok_or(ComfyError::Decode {
            route,
            detail: "missing queue_running / queue_pending".into(),
        })
    }

    /// `GET /history/<id>`. `None` until the job finishes -- and for an id
    /// ComfyUI has never heard of, which answers the same `{}`.
    pub async fn history(&self, prompt_id: &str) -> Result<Option<HistoryRecord>, ComfyError> {
        let path = format!("/history/{}", encode_segment(prompt_id));
        let route = format!("GET {path}");
        let value: Value = self.get_json(path).await?;
        match value.get(prompt_id) {
            Some(record) => serde_json::from_value(record.clone())
                .map(Some)
                .map_err(|e| ComfyError::Decode {
                    route,
                    detail: e.to_string(),
                }),
            None => Ok(None),
        }
    }

    /// `POST /interrupt` for this prompt: stops it if it is the one running.
    pub async fn interrupt(&self, prompt_id: &str) -> Result<(), ComfyError> {
        let response = self
            .post("/interrupt", &json!({"prompt_id": prompt_id}))
            .await?;
        expect_ok("POST /interrupt", &response)
    }

    /// `POST /queue {"delete": [id]}`: removes a pending job. It will never
    /// reach history.
    pub async fn delete_pending(&self, prompt_id: &str) -> Result<(), ComfyError> {
        let response = self.post("/queue", &json!({"delete": [prompt_id]})).await?;
        expect_ok("POST /queue", &response)
    }

    /// `GET /view`: the bytes of one output file.
    pub async fn view(&self, output: &OutputRef) -> Result<Bytes, ComfyError> {
        let path = format!(
            "/view?filename={}&subfolder={}&type={}",
            encode_segment(&output.filename),
            encode_segment(&output.subfolder),
            encode_segment(&output.file_type)
        );
        let route = format!("GET {path}");
        let response = self.get(path).await?;
        expect_ok(&route, &response)?;
        Ok(response.body)
    }
}

/// Probe candidate addresses in order and return the first ComfyUI that
/// answers, with its stats. When none does, every address tried is returned
/// with why, so the UI can say exactly where it looked.
pub async fn discover(
    candidates: &[String],
    make: impl Fn(&str) -> ComfyHttp,
) -> Result<(ComfyHttp, SystemStats), Vec<(String, ComfyError)>> {
    let mut tried = Vec::new();
    for url in candidates {
        let client = make(url);
        match client.system_stats().await {
            Ok(stats) => return Ok((client, stats)),
            Err(e) => tried.push((client.base_url().to_string(), e)),
        }
    }
    Err(tried)
}

fn expect_ok(route: &str, response: &HttpResponse) -> Result<(), ComfyError> {
    if (200..300).contains(&response.status) {
        Ok(())
    } else {
        Err(ComfyError::Http {
            route: route.to_string(),
            status: response.status,
            body: String::from_utf8_lossy(&response.body).into_owned(),
        })
    }
}

fn decode<T: DeserializeOwned>(route: &str, body: &[u8]) -> Result<T, ComfyError> {
    serde_json::from_slice(body).map_err(|e| ComfyError::Decode {
        route: route.to_string(),
        detail: e.to_string(),
    })
}

/// Trim, default the scheme to `http://`, and drop trailing slashes, so
/// `127.0.0.1:8188/` and `http://127.0.0.1:8188` are the same address.
pub fn normalize_url(url: &str) -> String {
    let url = url.trim();
    let with_scheme = if url.starts_with("http://") || url.starts_with("https://") {
        url.to_string()
    } else {
        format!("http://{url}")
    };
    with_scheme.trim_end_matches('/').to_string()
}

/// The host part of a normalized URL, lowercased.
fn host_of(base: &str) -> String {
    let rest = base.split_once("://").map(|(_, r)| r).unwrap_or(base);
    let authority = rest.split('/').next().unwrap_or(rest);
    let host = if authority.starts_with('[') {
        authority
            .split(']')
            .next()
            .map(|h| format!("{h}]"))
            .unwrap_or_default()
    } else {
        authority
            .rsplit_once(':')
            .map(|(h, _)| h.to_string())
            .unwrap_or_else(|| authority.to_string())
    };
    host.to_ascii_lowercase()
}

/// Percent-encode one path segment or query value (RFC 3986 unreserved set
/// passes through). Hand-rolled: the app needs exactly this and no URL parser.
pub fn encode_segment(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// A random-enough client id for ComfyUI's per-client websocket routing, which
/// this app does not use but ComfyUI expects to be present.
fn mint_client_id() -> String {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("latentcreate-{nanos:x}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transport::{FakeTransport, TransportError};
    use crate::types::Outcome;

    fn fixture(name: &str) -> String {
        let path = format!(
            "{}/../../testdata/comfy-http/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        std::fs::read_to_string(path).expect("fixture")
    }

    fn client(fake: &Arc<FakeTransport>) -> ComfyHttp {
        ComfyHttp::with_transport("127.0.0.1:8188/", fake.clone(), None)
    }

    #[test]
    fn test_normalize_url_defaults_scheme_and_trims_slashes() {
        assert_eq!(normalize_url(" 127.0.0.1:8188/ "), "http://127.0.0.1:8188");
        assert_eq!(
            normalize_url("https://gpu.lan:8443//"),
            "https://gpu.lan:8443"
        );
    }

    #[test]
    fn test_is_loopback_only_for_this_machine() {
        let fake = Arc::new(FakeTransport::new());
        let at = |u: &str| ComfyHttp::with_transport(u, fake.clone(), None).is_loopback();
        assert!(at("127.0.0.1:8188"));
        assert!(at("http://localhost:8000"));
        assert!(at("http://[::1]:8188"));
        assert!(!at("http://192.168.1.20:8188"));
        assert!(!at("http://gpu-box:8188"));
    }

    #[test]
    fn test_encode_segment_escapes_reserved_bytes() {
        assert_eq!(
            encode_segment("TextEncodeAceStepAudio1.5"),
            "TextEncodeAceStepAudio1.5"
        );
        assert_eq!(encode_segment("my file&x=1.png"), "my%20file%26x%3D1.png");
        assert_eq!(encode_segment("sub/dir"), "sub%2Fdir");
    }

    #[tokio::test]
    async fn test_system_stats_refuses_json_that_is_not_comfyui() {
        let fake = Arc::new(FakeTransport::new());
        fake.on(Method::Get, "/system_stats", 200, r#"{"ok": true}"#);
        let err = client(&fake).system_stats().await.expect_err("not comfy");
        assert!(matches!(err, ComfyError::Decode { .. }), "{err:?}");
    }

    /// Protects: a refused connection is Unreachable with the address in the
    /// sentence and a next step -- the state the Setup card renders.
    #[tokio::test]
    async fn test_refused_connection_is_unreachable_with_a_next_step() {
        let fake = Arc::new(FakeTransport::new());
        fake.fail(
            Method::Get,
            "/system_stats",
            TransportError::Connect("refused".into()),
        );
        let err = client(&fake).system_stats().await.expect_err("down");
        assert!(err.is_unreachable());
        assert_eq!(
            err.to_string(),
            "No ComfyUI answered at http://127.0.0.1:8188. Start ComfyUI, then Retry."
        );
    }

    /// Protects: discovery stops at the first address that is really ComfyUI,
    /// and reports every address it tried when none is.
    #[tokio::test]
    async fn test_discover_returns_the_first_answering_comfyui() {
        let fake = Arc::new(FakeTransport::new());
        fake.on(
            Method::Get,
            "/system_stats",
            200,
            fixture("system_stats.json"),
        );
        let candidates: Vec<String> = DEFAULT_URLS.iter().map(|s| s.to_string()).collect();
        let (found, stats) = discover(&candidates, |u| {
            ComfyHttp::with_transport(u, fake.clone(), None)
        })
        .await
        .expect("found");
        assert_eq!(found.base_url(), "http://127.0.0.1:8188");
        assert_eq!(stats.system.comfyui_version.as_deref(), Some("0.45.2"));
        assert_eq!(fake.calls().len(), 1);

        let dead = Arc::new(FakeTransport::new());
        dead.fail(
            Method::Get,
            "/system_stats",
            TransportError::Connect("refused".into()),
        );
        let tried = discover(&candidates, |u| {
            ComfyHttp::with_transport(u, dead.clone(), None)
        })
        .await
        .expect_err("none");
        assert_eq!(tried.len(), 2);
        assert_eq!(tried[1].0, "http://127.0.0.1:8000");
    }

    #[tokio::test]
    async fn test_submit_returns_the_prompt_id_and_sends_the_graph() {
        let fake = Arc::new(FakeTransport::new());
        let body: Value = serde_json::from_str(&fixture("prompt_200.json")).expect("json");
        fake.on_json(Method::Post, "/prompt", &body["body"]);
        let graph = json!({"9": {"class_type": "SaveImage", "inputs": {}}});
        let id = client(&fake).submit(&graph).await.expect("queued");
        assert_eq!(id, body["body"]["prompt_id"].as_str().expect("id"));
        let sent = fake.body_of(0);
        assert_eq!(sent["prompt"], graph);
        assert!(sent["client_id"]
            .as_str()
            .expect("client id")
            .starts_with("latentcreate-"));
    }

    #[tokio::test]
    async fn test_submit_turns_a_400_into_a_rejection() {
        let fake = Arc::new(FakeTransport::new());
        let body: Value =
            serde_json::from_str(&fixture("prompt_400_node_errors.json")).expect("json");
        fake.on(Method::Post, "/prompt", 400, body["body"].to_string());
        let err = client(&fake)
            .submit(&json!({}))
            .await
            .expect_err("rejected");
        match err {
            ComfyError::Rejected(r) => assert_eq!(r.nodes.len(), 2),
            other => panic!("expected Rejected, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_object_info_of_an_unknown_class_is_unknown_node() {
        let fake = Arc::new(FakeTransport::new());
        fake.on(Method::Get, "/object_info/NoSuchNode", 200, "{}");
        let err = client(&fake)
            .object_info("NoSuchNode")
            .await
            .expect_err("unknown");
        assert!(matches!(err, ComfyError::UnknownNode(ref c) if c == "NoSuchNode"));
    }

    #[tokio::test]
    async fn test_history_is_none_until_the_job_finishes() {
        let fake = Arc::new(FakeTransport::new());
        fake.on(Method::Get, "/history/abc", 200, "{}");
        assert!(client(&fake).history("abc").await.expect("ok").is_none());

        let value: Value =
            serde_json::from_str(&fixture("history_interrupted.json")).expect("json");
        let id = value
            .as_object()
            .expect("obj")
            .keys()
            .next()
            .expect("id")
            .clone();
        fake.on_json(Method::Get, &format!("/history/{id}"), &value);
        let rec = client(&fake)
            .history(&id)
            .await
            .expect("ok")
            .expect("finished");
        assert_eq!(rec.outcome(), Outcome::Interrupted);
    }

    #[tokio::test]
    async fn test_cancel_routes_send_the_documented_bodies() {
        let fake = Arc::new(FakeTransport::new());
        fake.on(Method::Post, "/interrupt", 200, "");
        fake.on(Method::Post, "/queue", 200, "");
        let c = client(&fake);
        c.interrupt("run-1").await.expect("ok");
        c.delete_pending("wait-2").await.expect("ok");
        assert_eq!(fake.body_of(0), json!({"prompt_id": "run-1"}));
        assert_eq!(fake.body_of(1), json!({"delete": ["wait-2"]}));
    }

    #[tokio::test]
    async fn test_view_encodes_the_output_address() {
        let fake = Arc::new(FakeTransport::new());
        let path = "/view?filename=ace_00001.flac&subfolder=latentcreate%20probe&type=output";
        fake.on(Method::Get, path, 200, "fLaC....");
        let out = OutputRef {
            node_id: "111".into(),
            kind: "audio".into(),
            filename: "ace_00001.flac".into(),
            subfolder: "latentcreate probe".into(),
            file_type: "output".into(),
        };
        let bytes = client(&fake).view(&out).await.expect("bytes");
        assert!(bytes.starts_with(b"fLaC"));
    }

    #[tokio::test]
    async fn test_folder_paths_reads_the_desktop_shape() {
        let fake = Arc::new(FakeTransport::new());
        fake.on(
            Method::Get,
            "/internal/folder_paths",
            200,
            fixture("folder_paths_desktop.json"),
        );
        let paths = client(&fake).folder_paths().await.expect("ok");
        assert!(paths["diffusion_models"][1].ends_with("diffusion_models"));
    }

    #[tokio::test]
    async fn test_a_session_log_records_the_route_and_outcome() {
        let dir = tempfile::tempdir().expect("tempdir");
        let log = SessionLog::open(dir.path().join("session.log")).expect("log");
        let fake = Arc::new(FakeTransport::new());
        fake.on(Method::Get, "/queue", 200, fixture("queue_running.json"));
        ComfyHttp::with_transport("127.0.0.1:8188", fake, Some(log))
            .queue()
            .await
            .expect("ok");
        let text = std::fs::read_to_string(dir.path().join("session.log")).expect("read");
        assert!(text.contains("\"tool\":\"GET /queue\""), "{text}");
        assert!(text.contains("\"ok\":true"), "{text}");
    }
}
