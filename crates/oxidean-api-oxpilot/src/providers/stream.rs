//! Shared wire decoders — `SseDecoder` frames `\n\n`-delimited
//! server-sent events and surfaces each event's `data:` payload;
//! `NdjsonDecoder` frames `\n`-delimited JSON lines (Ollama). Both are
//! incremental: adapters feed `bytes_stream()` chunks and poll for
//! completed units, so a JSON event split across TCP segments is never
//! parsed half-formed.

/// Server-sent events per the HTML spec — events split on a blank line,
/// and each event's payload is its `data:` lines joined with `\n`.
/// Comment lines (`:`), `event:`/`id:`/`retry:` fields are ignored; the
/// adapters read event types off the JSON payload itself.
#[derive(Default)]
pub struct SseDecoder {
    buf: Vec<u8>,
}

impl SseDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk; return every event payload completed by it.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some((end, sep_len)) = find_frame_end(&self.buf) {
            let frame: Vec<u8> = self.buf.drain(..end + sep_len).take(end).collect();
            if let Some(payload) = parse_sse_frame(&frame) {
                out.push(payload);
            }
        }
        out
    }

    /// Flush at EOF — a trailing unterminated event still parses (some
    /// providers close the body right after the last `data:` line).
    pub fn finish(&mut self) -> Vec<String> {
        if self.buf.is_empty() {
            return Vec::new();
        }
        let rest = std::mem::take(&mut self.buf);
        parse_sse_frame(&rest).into_iter().collect()
    }
}

/// Find the end of the next `\n\n` or `\r\n\r\n` frame —
/// `(frame_end, delimiter_len)`.
fn find_frame_end(buf: &[u8]) -> Option<(usize, usize)> {
    let mut i = 0;
    while i < buf.len() {
        match buf[i] {
            b'\n' => {
                if buf.get(i + 1) == Some(&b'\n') {
                    return Some((i, 2));
                }
                // "\r\n\r\n" is handled byte-wise — a '\n' at i where
                // i>=1 and buf[i-1]=='\r' and buf[i+1]=='\r' and
                // buf[i+2]=='\n' ends the frame at i-1.
                if i >= 1
                    && buf[i - 1] == b'\r'
                    && buf.get(i + 1) == Some(&b'\r')
                    && buf.get(i + 2) == Some(&b'\n')
                {
                    return Some((i - 1, 4));
                }
                i += 1;
            }
            _ => i += 1,
        }
    }
    None
}

/// Extract the `data:` payload of one raw event frame — multi-line data
/// joins with `\n` per spec. `None` when the frame carries no data
/// (comments, lone field lines).
fn parse_sse_frame(frame: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(frame);
    let mut data = String::new();
    for line in text.lines() {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.starts_with(':') {
            continue;
        }
        if let Some(d) = line.strip_prefix("data:") {
            if !data.is_empty() {
                data.push('\n');
            }
            data.push_str(d.strip_prefix(' ').unwrap_or(d));
        }
    }
    (!data.is_empty()).then_some(data)
}

/// NDJSON — one JSON object per line (Ollama's `/api/chat` stream).
#[derive(Default)]
pub struct NdjsonDecoder {
    buf: Vec<u8>,
}

impl NdjsonDecoder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk; return every complete non-empty line.
    pub fn feed(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.extend_from_slice(chunk);
        let mut out = Vec::new();
        while let Some(pos) = self.buf.iter().position(|b| *b == b'\n') {
            let line: Vec<u8> = self.buf.drain(..=pos).collect();
            let text = String::from_utf8_lossy(&line);
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
        out
    }

    /// Flush at EOF — Ollama may close without a trailing newline.
    pub fn finish(&mut self) -> Vec<String> {
        if self.buf.is_empty() {
            return Vec::new();
        }
        let rest = std::mem::take(&mut self.buf);
        let text = String::from_utf8_lossy(&rest);
        let trimmed = text.trim();
        if trimmed.is_empty() {
            Vec::new()
        } else {
            vec![trimmed.to_string()]
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sse_frames_split_across_chunks() {
        let mut d = SseDecoder::new();
        assert!(d.feed(b"data: {\"a\":1}").is_empty()); // no terminator yet
        assert_eq!(d.feed(b"\n\ndata: {\"b\""), vec!["{\"a\":1}"]);
        // A lone '\n' is not a terminator — the tail flushes at finish().
        assert!(d.feed(b":2}\n").is_empty());
        assert_eq!(d.finish(), vec!["{\"b\":2}"]);
    }

    #[test]
    fn sse_crlf_and_multiline_data() {
        let mut d = SseDecoder::new();
        let events = d.feed(b"data: one\r\ndata: two\r\n\r\ndata: x\n\n");
        assert_eq!(events, vec!["one\ntwo", "x"]);
    }

    #[test]
    fn sse_ignores_comments_and_nondata_fields() {
        let mut d = SseDecoder::new();
        let events = d.feed(b": keepalive\n\nevent: ping\nid: 9\n\ndata: y\n\n");
        assert_eq!(events, vec!["y"]);
    }

    #[test]
    fn sse_done_marker_passes_through_verbatim() {
        let mut d = SseDecoder::new();
        let events = d.feed(b"data: [DONE]\n\n");
        assert_eq!(events, vec!["[DONE]"]);
    }

    #[test]
    fn ndjson_lines_split_and_blank_skip() {
        let mut d = NdjsonDecoder::new();
        assert_eq!(d.feed(b"{\"a\":1}\n\n{\"b\""), vec!["{\"a\":1}"]);
        assert_eq!(d.feed(b":2}\n"), vec!["{\"b\":2}"]);
        assert!(d.finish().is_empty());
    }

    #[test]
    fn ndjson_finish_flushes_unterminated_tail() {
        let mut d = NdjsonDecoder::new();
        assert!(d.feed(b"{\"done\":true}").is_empty());
        assert_eq!(d.finish(), vec!["{\"done\":true}"]);
    }
}
