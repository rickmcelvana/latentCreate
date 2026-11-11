//! What can go wrong talking to ComfyUI, sorted by what the user does next.

use serde_json::Value;

use crate::transport::TransportError;

/// Every failure of a ComfyUI call.
///
/// The variants are the distinctions the UI acts on (CONVENTIONS: every error
/// says what to do next). Display strings are written for a person; raw bodies
/// never appear in them and go to the session log instead.
#[derive(Debug, thiserror::Error)]
pub enum ComfyError {
    /// Nobody answered at this address -- ComfyUI is not running there.
    #[error("No ComfyUI answered at {url}. Start ComfyUI, then Retry.")]
    Unreachable {
        /// The base URL that was tried.
        url: String,
        /// The transport's own reason, for the session log.
        detail: String,
    },
    /// Connected, but the answer did not arrive in time.
    #[error("ComfyUI at {url} stopped responding. Check that it is not stuck, then Retry.")]
    Timeout {
        /// The base URL that was tried.
        url: String,
    },
    /// ComfyUI refused a workflow on submit -- before any GPU time.
    #[error("{0}")]
    Rejected(Rejection),
    /// The class asked for is not in this ComfyUI's node registry.
    #[error(
        "ComfyUI has no node named {0}. Install the custom node that provides it, then Retry."
    )]
    UnknownNode(String),
    /// A status code the route does not document.
    #[error("ComfyUI answered {status} to {route}. Check the ComfyUI console, then Retry.")]
    Http {
        /// `"GET /queue"`.
        route: String,
        /// The HTTP status.
        status: u16,
        /// The body, for the session log only.
        body: String,
    },
    /// A body that does not have the shape COMFY-HTTP-SURFACE records. Usually a
    /// ComfyUI version this app has not been checked against.
    #[error("ComfyUI sent an answer to {route} that latentCreate could not read. Update latentCreate or ComfyUI, then Retry.")]
    Decode {
        /// `"GET /history/<id>"`.
        route: String,
        /// The parser's message, for the session log.
        detail: String,
    },
    /// A transport failure other than refused or timed out.
    #[error("Could not talk to ComfyUI at {url}: {detail}")]
    Transport {
        /// The base URL that was tried.
        url: String,
        /// What the HTTP stack reported.
        detail: String,
    },
}

impl ComfyError {
    /// Map a transport failure against `url` to the variant the UI shows.
    pub(crate) fn from_transport(url: &str, error: TransportError) -> Self {
        match error {
            TransportError::Connect(detail) => ComfyError::Unreachable {
                url: url.to_string(),
                detail,
            },
            TransportError::Timeout => ComfyError::Timeout {
                url: url.to_string(),
            },
            TransportError::Other(detail) => ComfyError::Transport {
                url: url.to_string(),
                detail,
            },
        }
    }

    /// Whether this is "nothing is listening" -- the one failure where trying
    /// the next candidate address is the right response.
    pub fn is_unreachable(&self) -> bool {
        matches!(
            self,
            ComfyError::Unreachable { .. } | ComfyError::Timeout { .. }
        )
    }
}

/// One input ComfyUI refused, from a `400` on `POST /prompt`.
#[derive(Debug, Clone, PartialEq)]
pub struct InputProblem {
    /// ComfyUI's code: `value_not_in_list`, `value_smaller_than_min`, ...
    pub kind: String,
    /// The input it names, when it names one.
    pub input_name: Option<String>,
    /// ComfyUI's own sentence, e.g. `"Value -5 smaller than min of 16"`.
    pub message: String,
    /// The extra detail ComfyUI attaches, e.g. `"sampler_name: 'nope' not in
    /// (list of length 45)"`. Often more specific than `message`.
    pub details: String,
}

/// One node's problems.
#[derive(Debug, Clone, PartialEq)]
pub struct NodeProblem {
    /// The graph node id (`"31"`, `"76:3"`).
    pub node_id: String,
    /// The node's class.
    pub class_type: String,
    /// What was wrong with its inputs.
    pub problems: Vec<InputProblem>,
}

/// ComfyUI's refusal of a whole workflow (COMFY-HTTP-SURFACE 6).
#[derive(Debug, Clone, PartialEq)]
pub struct Rejection {
    /// `prompt_outputs_failed_validation`, `missing_node_type`, ...
    pub kind: String,
    /// The top-level message.
    pub message: String,
    /// For `missing_node_type`: the class ComfyUI does not have.
    pub missing_class: Option<String>,
    /// For `missing_node_type`: the node that uses it.
    pub missing_node_id: Option<String>,
    /// Per-node input problems, in node-id order.
    pub nodes: Vec<NodeProblem>,
}

impl Rejection {
    /// Parse a `400` body. `None` when it is not the documented shape.
    pub fn parse(body: &Value) -> Option<Self> {
        let error = body.get("error")?;
        let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).unwrap_or("").to_string();
        let extra = error.get("extra_info");
        let mut nodes: Vec<NodeProblem> = body
            .get("node_errors")
            .and_then(Value::as_object)
            .map(|map| {
                map.iter()
                    .map(|(id, node)| NodeProblem {
                        node_id: id.clone(),
                        class_type: text(node, "class_type"),
                        problems: node
                            .get("errors")
                            .and_then(Value::as_array)
                            .map(|errors| {
                                errors
                                    .iter()
                                    .map(|e| InputProblem {
                                        kind: text(e, "type"),
                                        input_name: e
                                            .get("extra_info")
                                            .and_then(|x| x.get("input_name"))
                                            .and_then(Value::as_str)
                                            .map(str::to_string),
                                        message: text(e, "message"),
                                        details: text(e, "details"),
                                    })
                                    .collect()
                            })
                            .unwrap_or_default(),
                    })
                    .collect()
            })
            .unwrap_or_default();
        nodes.sort_by_key(|n| node_order(&n.node_id));
        Some(Rejection {
            kind: text(error, "type"),
            message: text(error, "message"),
            missing_class: extra
                .and_then(|x| x.get("class_type"))
                .and_then(Value::as_str)
                .map(str::to_string),
            missing_node_id: extra
                .and_then(|x| x.get("node_id"))
                .and_then(Value::as_str)
                .map(str::to_string),
            nodes,
        })
    }
}

/// Numeric node ids sort numerically, so `#9` comes before `#31`.
fn node_order(id: &str) -> (u64, String) {
    let head = id.split(':').next().unwrap_or(id);
    (head.parse().unwrap_or(u64::MAX), id.to_string())
}

impl std::fmt::Display for Rejection {
    /// One sentence a person can act on. The node's class leads because it is
    /// the half that says where to look; ComfyUI's `details` is preferred to its
    /// `message` when present, since it names the input and the value.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.kind == "missing_node_type" {
            let class = self.missing_class.as_deref().unwrap_or("an unknown node");
            return match &self.missing_node_id {
                Some(id) => write!(
                    f,
                    "This workflow needs {class} (node #{id}), which this ComfyUI does not have. \
                     Install the custom node that provides it, restart ComfyUI, then try again."
                ),
                None => write!(
                    f,
                    "This workflow needs {class}, which this ComfyUI does not have. \
                     Install the custom node that provides it, restart ComfyUI, then try again."
                ),
            };
        }
        let parts: Vec<String> = self
            .nodes
            .iter()
            .flat_map(|node| {
                node.problems.iter().map(move |p| {
                    let what = if !p.details.is_empty() && p.details.contains(':') {
                        p.details.clone()
                    } else {
                        match &p.input_name {
                            Some(input) => format!("{input}: {}", p.message),
                            None => p.message.clone(),
                        }
                    };
                    format!("{} (#{}) {what}", node.class_type, node.node_id)
                })
            })
            .collect();
        if parts.is_empty() {
            let message = if self.message.is_empty() {
                &self.kind
            } else {
                &self.message
            };
            write!(f, "ComfyUI rejected the workflow: {message}")
        } else {
            write!(f, "ComfyUI rejected the workflow: {}", parts.join("; "))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> Value {
        let path = format!(
            "{}/../../testdata/comfy-http/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        let text = std::fs::read_to_string(&path).expect("fixture");
        serde_json::from_str(&text).expect("fixture JSON")
    }

    /// Protects: a validation refusal names each node, its class and the input,
    /// in node order -- the captured live body, not a hand-written one.
    #[test]
    fn test_rejection_summarises_every_node_problem() {
        let body = fixture("prompt_400_node_errors.json")["body"].clone();
        let rejection = Rejection::parse(&body).expect("parses");
        assert_eq!(rejection.kind, "prompt_outputs_failed_validation");
        assert_eq!(rejection.nodes.len(), 2);
        assert_eq!(rejection.nodes[0].node_id, "27");
        assert_eq!(
            rejection.nodes[0].problems[0].input_name.as_deref(),
            Some("width")
        );
        assert_eq!(
            rejection.to_string(),
            "ComfyUI rejected the workflow: EmptySD3LatentImage (#27) width: Value -5 smaller than \
             min of 16; KSampler (#31) sampler_name: 'nope' not in (list of length 45)"
        );
    }

    /// Protects: a missing custom node says which class and what to do, rather
    /// than ComfyUI's sentence, which names the node's *title* ("KSampler") and
    /// not the class it cannot find.
    #[test]
    fn test_rejection_names_the_missing_class_and_the_remedy() {
        let body = fixture("prompt_400_missing_node.json")["body"].clone();
        let rejection = Rejection::parse(&body).expect("parses");
        let text = rejection.to_string();
        assert!(text.contains("NoSuchNode (node #31)"), "{text}");
        assert!(text.contains("Install the custom node"), "{text}");
    }

    #[test]
    fn test_rejection_parse_is_none_without_an_error_object() {
        assert!(Rejection::parse(&serde_json::json!({"node_errors": {}})).is_none());
    }

    #[test]
    fn test_unreachable_and_timeout_are_the_retryable_kinds() {
        let url = "http://127.0.0.1:8188";
        assert!(
            ComfyError::from_transport(url, TransportError::Connect("x".into())).is_unreachable()
        );
        assert!(ComfyError::from_transport(url, TransportError::Timeout).is_unreachable());
        assert!(
            !ComfyError::from_transport(url, TransportError::Other("tls".into())).is_unreachable()
        );
    }
}
