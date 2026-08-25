//! Server-sent-events framing.
//!
//! Transport-free on purpose: bytes in, complete events out. The HTTP client
//! feeds it whatever the socket happened to deliver, which is why every rule
//! here is about *incomplete* input.

use crate::LlmError;

/// One `data:` payload lifted out of an SSE stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SseEvent {
    /// A `data:` payload. JSON, for every provider this crate targets.
    Data(String),
    /// The `data: [DONE]` sentinel that ends an OpenAI-style stream.
    Done,
}

/// Reassembles SSE events from arbitrarily chunked network reads.
///
/// Buffers **bytes**, not text, and decodes UTF-8 only once a whole event has
/// arrived. A chunk boundary can fall inside a multi-byte character -- lyrics
/// are written in 50+ languages, so this is a matter of course, not an edge
/// case -- and decoding each read on its own would corrupt exactly those
/// characters.
#[derive(Debug, Default)]
pub struct SseDecoder {
    buf: Vec<u8>,
}

impl SseDecoder {
    /// A decoder with an empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feeds one network chunk and returns every event completed by it.
    ///
    /// A partial event stays buffered for the next call. Comment lines
    /// (`: keep-alive`) and non-`data:` fields (`event:`, `id:`, `retry:`) are
    /// dropped, per the SSE spec -- OpenRouter sends comment heartbeats while
    /// a slow model warms up.
    pub fn push(&mut self, chunk: &[u8]) -> Result<Vec<SseEvent>, LlmError> {
        self.buf.extend_from_slice(chunk);
        let mut events = Vec::new();
        while let Some((end, delimiter)) = find_blank_line(&self.buf) {
            let frame = self.buf[..end].to_vec();
            self.buf.drain(..end + delimiter);
            let text = std::str::from_utf8(&frame)
                .map_err(|e| LlmError::Decode(format!("event is not valid UTF-8: {e}")))?;
            if let Some(event) = parse_event(text) {
                events.push(event);
            }
        }
        Ok(events)
    }
}

/// Offset and length of the first blank line, which terminates an event.
///
/// Both `\n\n` and `\r\n\r\n` occur in the wild; proxies rewrite line endings.
fn find_blank_line(buf: &[u8]) -> Option<(usize, usize)> {
    for i in 0..buf.len().saturating_sub(1) {
        if buf[i..].starts_with(b"\r\n\r\n") {
            return Some((i, 4));
        }
        if buf[i] == b'\n' && buf[i + 1] == b'\n' {
            return Some((i, 2));
        }
    }
    None
}

/// Turns one complete frame into an event, or `None` when it carries no data.
fn parse_event(frame: &str) -> Option<SseEvent> {
    let mut data = String::new();
    let mut seen_data = false;
    for line in frame.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() || line.starts_with(':') {
            continue;
        }
        let Some(value) = line.strip_prefix("data:") else {
            continue;
        };
        if seen_data {
            data.push('\n');
        }
        data.push_str(value.strip_prefix(' ').unwrap_or(value));
        seen_data = true;
    }
    if !seen_data {
        return None;
    }
    if data.trim() == "[DONE]" {
        return Some(SseEvent::Done);
    }
    Some(SseEvent::Data(data))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Protects: an event split across two reads is reassembled, not lost or
    /// half-parsed. The split falls inside the JSON body, which is what a
    /// short TCP read looks like.
    #[test]
    fn test_event_split_across_reads_is_reassembled() {
        let mut decoder = SseDecoder::new();
        assert_eq!(decoder.push(b"data: {\"choices\":[{\"de").unwrap(), vec![]);
        let events = decoder.push(b"lta\":{\"content\":\"hi\"}}]}\n\n").unwrap();
        assert_eq!(
            events,
            vec![SseEvent::Data(
                "{\"choices\":[{\"delta\":{\"content\":\"hi\"}}]}".to_string()
            )]
        );
    }

    /// Protects: the reason this buffers bytes rather than text. The split
    /// falls between the two bytes of `e` + combining acute, and a decoder
    /// that called `from_utf8` per read would produce a replacement character
    /// in the middle of someone's lyrics.
    #[test]
    fn test_multibyte_character_split_across_reads_survives() {
        let payload = "data: {\"content\":\"caf\u{00e9}\"}\n\n".as_bytes();
        let split = payload
            .iter()
            .position(|b| *b == 0xc3)
            .expect("the two-byte e-acute");
        let mut decoder = SseDecoder::new();
        assert_eq!(decoder.push(&payload[..split + 1]).unwrap(), vec![]);
        let events = decoder.push(&payload[split + 1..]).unwrap();
        assert_eq!(
            events,
            vec![SseEvent::Data("{\"content\":\"caf\u{00e9}\"}".to_string())]
        );
    }

    /// Protects: several events in one read all come out, in order. Ollama
    /// delivers bursts of chunks in a single body read.
    #[test]
    fn test_several_events_in_one_read_come_out_in_order() {
        let mut decoder = SseDecoder::new();
        let events = decoder
            .push(b"data: one\n\ndata: two\n\ndata: [DONE]\n\n")
            .unwrap();
        assert_eq!(
            events,
            vec![
                SseEvent::Data("one".to_string()),
                SseEvent::Data("two".to_string()),
                SseEvent::Done,
            ]
        );
    }

    /// Protects: heartbeats and non-data fields never reach the caller as
    /// content. OpenRouter sends `: OPENROUTER PROCESSING` comments while a
    /// model warms up; parsing one as JSON would fail the whole stream.
    #[test]
    fn test_comments_and_other_fields_are_dropped() {
        let mut decoder = SseDecoder::new();
        let events = decoder
            .push(b": OPENROUTER PROCESSING\n\nevent: message\nid: 7\ndata: kept\n\n")
            .unwrap();
        assert_eq!(events, vec![SseEvent::Data("kept".to_string())]);
    }

    /// Protects: CRLF framing is recognised. Some proxies rewrite line
    /// endings, and a decoder that only knows `\n\n` buffers the whole
    /// response and emits nothing at all.
    #[test]
    fn test_crlf_framing_is_recognised() {
        let mut decoder = SseDecoder::new();
        let events = decoder
            .push(b"data: one\r\n\r\ndata: [DONE]\r\n\r\n")
            .unwrap();
        assert_eq!(
            events,
            vec![SseEvent::Data("one".to_string()), SseEvent::Done]
        );
    }

    /// Protects: the multi-line `data:` rule. The SSE spec joins repeated
    /// `data:` lines with a newline; taking only the last would truncate.
    #[test]
    fn test_repeated_data_lines_join_with_a_newline() {
        let mut decoder = SseDecoder::new();
        let events = decoder.push(b"data: first\ndata: second\n\n").unwrap();
        assert_eq!(events, vec![SseEvent::Data("first\nsecond".to_string())]);
    }
}
