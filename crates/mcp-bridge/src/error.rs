//! The one error type every mcp-bridge call returns.

use rmcp::service::ServiceError;

/// Everything that can go wrong talking to a local ComfyUI through `comfy-mcp`.
#[derive(Debug, thiserror::Error)]
pub enum ComfyError {
    /// The `comfy-mcp` executable is not installed or not on PATH.
    #[error("comfy-mcp is not installed. Install it with `pip install comfy-mcp`, then Retry.")]
    NotInstalled,
    /// The child process could not be spawned for some other reason.
    #[error("could not start comfy-mcp: {0}")]
    Spawn(#[source] std::io::Error),
    /// The MCP session failed at the transport or protocol level.
    #[error("comfy-mcp connection failed: {0}")]
    Transport(String),
    /// The tool ran and reported a failure. `code` is comfy-mcp's bracketed
    /// error slug when it emitted one (e.g. `workflow_not_found`).
    #[error("{tool} failed{}: {message}", .code.as_ref().map(|c| format!(" [{c}]")).unwrap_or_default())]
    Tool {
        /// Tool name as sent on the wire.
        tool: String,
        /// Machine-readable slug parsed out of the message, when present.
        code: Option<String>,
        /// Full human-readable text comfy-mcp returned.
        message: String,
    },
    /// A tool succeeded but its payload was not the JSON we expected.
    #[error("{tool} returned an unreadable payload: {detail}")]
    Payload {
        /// Tool name as sent on the wire.
        tool: String,
        /// What failed to parse.
        detail: String,
    },
}

impl From<ServiceError> for ComfyError {
    fn from(e: ServiceError) -> Self {
        ComfyError::Transport(e.to_string())
    }
}

/// Pull comfy-mcp's bracketed error slug out of a failure message.
///
/// Its failures read `... failed [workflow_not_found]: Workflow file not found`.
/// Pydantic argument errors bracket something else entirely
/// (`[type=missing, input_value={}]`) and must yield `None`.
///
/// Anchored on the literal ` failed [` rather than the first `[`, because the
/// message embeds the workflow PATH ahead of the slug: a file under
/// `my [demo] songs/` would otherwise parse as the code `demo`. Returning
/// `None` for an unrecognised phrasing is the safe direction -- a wrong slug
/// would route the user to the wrong remedy.
pub(crate) fn parse_error_code(text: &str) -> Option<String> {
    const ANCHOR: &str = " failed [";
    let start = text.rfind(ANCHOR)? + ANCHOR.len() - 1;
    let end = text[start..].find(']')? + start;
    let code = &text[start + 1..end];
    let ok = !code.is_empty()
        && code
            .chars()
            .all(|c| c.is_ascii_lowercase() || c == '_' || c.is_ascii_digit());
    if ok {
        Some(code.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_error_code_reads_the_slug() {
        let text = "comfy workflow slots x failed [workflow_not_found]: Workflow file not found";
        assert_eq!(
            parse_error_code(text),
            Some("workflow_not_found".to_string())
        );
    }

    #[test]
    fn test_parse_error_code_rejects_a_non_slug_bracket() {
        let text = "Field required [type=missing, input_value={'path': 'x'}]";
        assert_eq!(parse_error_code(text), None);
    }

    #[test]
    fn test_parse_error_code_is_none_without_brackets() {
        let text = "something went wrong";
        assert_eq!(parse_error_code(text), None);
    }

    /// Protects: the slug must come from comfy-mcp's own `failed [...]` marker,
    /// not from the workflow path the message quotes ahead of it. Bracketed
    /// folder names are ordinary in a music library, and a first-bracket scan
    /// reports `demo` here -- routing the user to the wrong remedy.
    #[test]
    fn test_parse_error_code_ignores_brackets_in_the_workflow_path() {
        let text = "Error executing tool list_workflow_slots: comfy workflow slots \
                    C:/Users/x/my [demo] songs/wf.json failed [workflow_not_found]: \
                    Workflow file not found";
        assert_eq!(
            parse_error_code(text),
            Some("workflow_not_found".to_string())
        );
    }

    #[test]
    fn test_tool_error_displays_the_code_when_present() {
        let with_code = ComfyError::Tool {
            tool: "list_workflow_slots".to_string(),
            code: Some("workflow_not_found".to_string()),
            message: "Workflow file not found".to_string(),
        };
        let displayed = with_code.to_string();
        assert!(displayed.contains("list_workflow_slots"));
        assert!(displayed.contains("[workflow_not_found]"));

        let without_code = ComfyError::Tool {
            tool: "server_info".to_string(),
            code: None,
            message: "ComfyUI is not running".to_string(),
        };
        let displayed = without_code.to_string();
        assert!(displayed.contains("server_info"));
        assert!(!displayed.contains("[]"));
    }
}
