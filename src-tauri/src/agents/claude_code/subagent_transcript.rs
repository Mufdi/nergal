use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const TAIL_BYTES: u64 = 256 * 1024;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SubagentSummary {
    pub last_text: Option<String>,
    /// Context carried by the agent's latest request (input + cache creation + cache
    /// read), the figure closest to what Claude Code's TUI shows beside an agent.
    pub tokens: Option<u64>,
}

pub fn read_subagent_summary(path: &Path) -> SubagentSummary {
    let Some(tail) = read_tail(path) else {
        return SubagentSummary::default();
    };
    let mut summary = SubagentSummary::default();
    for line in tail.lines().rev() {
        let Ok(record) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if record.get("type").and_then(|t| t.as_str()) != Some("assistant") {
            continue;
        }
        let message = &record["message"];
        if summary.tokens.is_none() {
            summary.tokens = context_tokens(&message["usage"]);
        }
        if summary.last_text.is_none() {
            summary.last_text = assistant_text(&message["content"]);
        }
        if summary.tokens.is_some() && summary.last_text.is_some() {
            break;
        }
    }
    summary
}

fn read_tail(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let start = len.saturating_sub(TAIL_BYTES);
    file.seek(SeekFrom::Start(start)).ok()?;
    let mut buf = Vec::with_capacity((len - start) as usize);
    file.read_to_end(&mut buf).ok()?;
    let text = String::from_utf8_lossy(&buf).into_owned();
    // The cut lands mid-line, so the first fragment is never a whole record.
    if start > 0 {
        return text.split_once('\n').map(|(_, rest)| rest.to_string());
    }
    Some(text)
}

fn context_tokens(usage: &serde_json::Value) -> Option<u64> {
    if !usage.is_object() {
        return None;
    }
    let field = |k: &str| usage.get(k).and_then(|v| v.as_u64()).unwrap_or(0);
    Some(
        field("input_tokens")
            + field("cache_creation_input_tokens")
            + field("cache_read_input_tokens"),
    )
}

fn assistant_text(content: &serde_json::Value) -> Option<String> {
    let text = content
        .as_array()?
        .iter()
        .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
        .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
        .collect::<Vec<_>>()
        .join("\n");
    let trimmed = text.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_transcript(lines: &[&str]) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
        f
    }

    fn assistant(text: &str, input: u64, cc: u64, cr: u64, out: u64) -> String {
        format!(
            r#"{{"type":"assistant","message":{{"id":"m","content":[{{"type":"text","text":"{text}"}}],"usage":{{"input_tokens":{input},"cache_creation_input_tokens":{cc},"cache_read_input_tokens":{cr},"output_tokens":{out}}}}}}}"#
        )
    }

    #[test]
    fn last_assistant_text_and_context_tokens_come_from_the_latest_record() {
        let a = assistant("first", 2, 100, 1000, 10);
        let b = assistant("final report", 2, 238, 49509, 112);
        let f = write_transcript(&[&a, r#"{"type":"user","message":{"content":"x"}}"#, &b]);
        let s = read_subagent_summary(f.path());
        assert_eq!(s.last_text.as_deref(), Some("final report"));
        assert_eq!(s.tokens, Some(2 + 238 + 49509));
    }

    #[test]
    fn tool_use_only_tail_falls_back_to_the_previous_text() {
        let a = assistant("earlier text", 1, 1, 1, 1);
        let tool = r#"{"type":"assistant","message":{"id":"t","content":[{"type":"tool_use","name":"Read"}],"usage":{"input_tokens":2,"cache_creation_input_tokens":3,"cache_read_input_tokens":4,"output_tokens":5}}}"#;
        let f = write_transcript(&[&a, tool]);
        let s = read_subagent_summary(f.path());
        assert_eq!(s.last_text.as_deref(), Some("earlier text"));
        assert_eq!(s.tokens, Some(9));
    }

    #[test]
    fn missing_file_yields_empty_summary() {
        let s = read_subagent_summary(Path::new("/nonexistent/agent-x.jsonl"));
        assert_eq!(s, SubagentSummary::default());
    }

    #[test]
    fn malformed_and_partial_lines_are_skipped() {
        let a = assistant("ok", 1, 2, 3, 4);
        let f = write_transcript(&["not json", &a, r#"{"type":"assistant","message":{"con"#]);
        let s = read_subagent_summary(f.path());
        assert_eq!(s.last_text.as_deref(), Some("ok"));
        assert_eq!(s.tokens, Some(6));
    }

    #[test]
    fn only_the_tail_is_read_and_the_cut_first_line_is_ignored() {
        let old = assistant("old", 1, 1, 1, 1);
        let pad = "x".repeat(300 * 1024);
        let filler = format!(r#"{{"type":"user","message":{{"content":"{pad}"}}}}"#);
        let new = assistant("new", 5, 6, 7, 8);
        let f = write_transcript(&[&old, &filler, &new]);
        let s = read_subagent_summary(f.path());
        assert_eq!(s.last_text.as_deref(), Some("new"));
        assert_eq!(s.tokens, Some(18));
    }

    #[test]
    fn empty_transcript_yields_empty_summary() {
        let f = write_transcript(&[]);
        assert_eq!(read_subagent_summary(f.path()), SubagentSummary::default());
    }
}
