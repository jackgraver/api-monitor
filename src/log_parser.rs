use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::types::LogEntry;

pub struct LogParser {
    path: PathBuf,
    offset: u64,
}

impl LogParser {
    pub fn open(path: impl AsRef<Path>) -> std::io::Result<Self> {
        Ok(Self {
            path: path.as_ref().to_path_buf(),
            offset: 0,
        })
    }

    fn parse_jsonl_chunk(text: &str, line_offset: usize) -> Vec<LogEntry> {
        let mut out = Vec::new();
        for (i, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let lineno = line_offset + i + 1;
            match serde_json::from_str::<LogEntry>(line) {
                Ok(entry) => out.push(entry),
                Err(e) => eprintln!("line {}: skip ({}): {}", lineno, e, line),
            }
        }
        out
    }

    /// Reads only bytes after [`Self::offset`], parses complete newline-terminated lines,
    /// advances [`Self::offset`] past consumed bytes. If the file shrinks (rotate/truncate),
    /// resets [`Self::offset`] to 0.
    pub fn read_new_log_entries(&mut self) -> std::io::Result<Vec<LogEntry>> {
        let mut file = File::open(&self.path)?;
        let len = file.metadata()?.len();
        if len < self.offset {
            self.offset = 0;
        }
        if self.offset > len {
            self.offset = len;
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut buf = Vec::new();
        file.read_to_end(&mut buf)?;
        if buf.is_empty() {
            return Ok(Vec::new());
        }
        let complete_len = match buf.last() {
            Some(b'\n') => buf.len(),
            _ => buf
                .iter()
                .rposition(|&b| b == b'\n')
                .map(|i| i + 1)
                .unwrap_or(0),
        };
        let mut entries = Vec::new();
        if complete_len > 0 {
            let text = std::str::from_utf8(&buf[..complete_len]).map_err(|e| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, format!("utf-8: {e}"))
            })?;
            entries = Self::parse_jsonl_chunk(text, 0);
        }
        let tail = &buf[complete_len..];
        if !tail.is_empty() {
            if let Ok(tail_str) = std::str::from_utf8(tail) {
                let t = tail_str.trim();
                if !t.is_empty() {
                    if let Ok(entry) = serde_json::from_str::<LogEntry>(t) {
                        entries.push(entry);
                        self.offset += buf.len() as u64;
                        return Ok(entries);
                    }
                }
            }
        }
        self.offset += complete_len as u64;
        Ok(entries)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::OpenOptions;
    use std::io::Write;

    fn line(route: &str, method: &str, latency: f64) -> String {
        format!(
            r#"{{"route":"{}","method":"{}","latency_ms":{},"timestamp":"t"}}"#,
            route, method, latency
        )
    }

    #[test]
    fn parse_chunk_skips_blank_and_collects_entries() {
        let text = format!("{}\n\n{}", line("/a", "GET", 1.5), line("/b", "POST", 2.25));
        let got = LogParser::parse_jsonl_chunk(&text, 0);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].route, "/a");
        assert_eq!(got[0].latency_ms, 1.5);
        assert_eq!(got[1].method, "POST");
    }

    #[test]
    fn read_reads_full_lines_advances_offset() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("log.jsonl");
        std::fs::write(&path, format!("{}\n", line("/x", "GET", 1.0)))?;
        let mut p = LogParser::open(&path)?;
        assert_eq!(p.offset, 0);
        let rows = p.read_new_log_entries()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].route, "/x");
        assert!(p.offset > 0);
        Ok(())
    }

    #[test]
    fn read_buffers_until_line_finished() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("log.jsonl");
        std::fs::write(&path, r#"{"route"#)?;
        let mut p = LogParser::open(&path)?;
        assert!(p.read_new_log_entries()?.is_empty());
        let mut f = OpenOptions::new().append(true).open(&path)?;
        f.write_all(br#"":"/y","method":"GET","latency_ms":1.0,"timestamp":"t"}"#)?;
        f.write_all(b"\n")?;
        f.flush()?;
        let rows = p.read_new_log_entries()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].route, "/y");
        Ok(())
    }

    #[test]
    fn read_resets_when_file_truncated() -> std::io::Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("log.jsonl");
        std::fs::write(
            &path,
            format!("{}\n{}\n", line("/aaaaaaaa", "GET", 1.0), line("/bbbbbbbb", "GET", 2.0)),
        )?;
        let mut p = LogParser::open(&path)?;
        assert_eq!(p.read_new_log_entries()?.len(), 2);
        assert_eq!(
            path.metadata()?.len(),
            p.offset,
            "fixture: read should consume whole file before rotate"
        );
        std::fs::write(&path, format!("{}\n", line("/z", "POST", 3.0)))?;
        let rows = p.read_new_log_entries()?;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].route, "/z");
        Ok(())
    }
}
