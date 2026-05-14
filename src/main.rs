use clap::Parser;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs::{File};
use std::fmt;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

#[derive(Parser)]
#[command(name = "api_benchmarker", about = "Analyze JSONL request logs")]
struct CliOptions {
    file: PathBuf,
    #[arg(long, help = "Sort output by: count | mean | p50 | p95 | p99 | max")]
    sort_by: Option<String>,
    #[arg(long, help = "Only show routes matching this substring")]
    filter_route: Option<String>,
    #[arg(long, help = "Hide routes with fewer than N requests")]
    min_count: Option<usize>,
    #[arg(long, help = "Show only the top N routes")]
    top_n: Option<usize>,
}

#[derive(Debug, Deserialize)]
struct LogEntry {
    route: String,
    method: String,
    latency_ms: f64,
    #[allow(dead_code)]
    timestamp: String,
}

impl fmt::Display for LogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {} {}", self.route, self.method, self.latency_ms, self.timestamp)
    }
}

#[derive(Debug)]
struct RouteStats {
    count: usize,
    min: f64,
    max: f64,
    mean: f64,
    p50: f64,
    p95: f64,
    p99: f64,
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

/// Reads only bytes after `*offset`, parses complete newline-terminated lines, advances `*offset`
/// past consumed bytes. If the file shrinks (rotate/truncate), resets `*offset` to 0.
fn read_new_log_entries(path: &Path, offset: &mut u64) -> std::io::Result<Vec<LogEntry>> {
    let mut file = File::open(path)?;
    let len = file.metadata()?.len();
    if len < *offset {
        *offset = 0;
    }
    if *offset > len {
        *offset = len;
    }
    file.seek(SeekFrom::Start(*offset))?;
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
        entries = parse_jsonl_chunk(text, 0);
    }
    let tail = &buf[complete_len..];
    if !tail.is_empty() {
        if let Ok(tail_str) = std::str::from_utf8(tail) {
            let t = tail_str.trim();
            if !t.is_empty() {
                if let Ok(entry) = serde_json::from_str::<LogEntry>(t) {
                    entries.push(entry);
                    *offset += buf.len() as u64;
                    return Ok(entries);
                }
            }
        }
    }
    *offset += complete_len as u64;
    Ok(entries)
}

fn merge_entries(grouped: &mut HashMap<String, Vec<f64>>, entries: &[LogEntry]) {
    for entry in entries {
        let key = format!("{} {}", entry.method, entry.route);
        grouped.entry(key).or_default().push(entry.latency_ms);
    }
}

fn compute_stats(latencies: &mut Vec<f64>) -> RouteStats {
    if latencies.is_empty() {
        return RouteStats {
            count: 0,
            min: 0.0,
            max: 0.0,
            mean: 0.0,
            p50: 0.0,
            p95: 0.0,
            p99: 0.0,
        };
    }
    latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let count = latencies.len();
    let min = latencies[0];
    let max = latencies[count - 1];
    let sum: f64 = latencies.iter().sum();
    let mean = sum / count as f64;
    let pct_idx = |p: f64| -> usize {
        let i = ((p / 100.0) * count as f64).ceil() as usize;
        i.saturating_sub(1).min(count - 1)
    };
    RouteStats {
        count,
        min,
        max,
        mean,
        p50: latencies[pct_idx(50.0)],
        p95: latencies[pct_idx(95.0)],
        p99: latencies[pct_idx(99.0)],
    }
}

fn filter_and_sort(
    stats: HashMap<String, RouteStats>,
    cli: &CliOptions,
) -> Vec<(String, RouteStats)> {
    let mut rows: Vec<(String, RouteStats)> = stats.into_iter().collect();
    rows.retain(|(key, s)| {
        if let Some(sub) = &cli.filter_route {
            if !key.contains(sub) {
                return false;
            }
        }
        if let Some(mc) = cli.min_count {
            if s.count < mc {
                return false;
            }
        }
        true
    });
    let key = cli.sort_by.as_deref().unwrap_or("route");
    rows.sort_by(|(ka, sa), (kb, sb)| {
        let cmp = match key {
            "count" => sb.count.cmp(&sa.count),
            "mean" => sb.mean.partial_cmp(&sa.mean).unwrap_or(std::cmp::Ordering::Equal),
            "p50" => sb.p50.partial_cmp(&sa.p50).unwrap_or(std::cmp::Ordering::Equal),
            "p95" => sb.p95.partial_cmp(&sa.p95).unwrap_or(std::cmp::Ordering::Equal),
            "p99" => sb.p99.partial_cmp(&sa.p99).unwrap_or(std::cmp::Ordering::Equal),
            "max" => sb.max.partial_cmp(&sa.max).unwrap_or(std::cmp::Ordering::Equal),
            _ => std::cmp::Ordering::Equal,
        };
        cmp.then_with(|| ka.cmp(kb))
    });
    if let Some(n) = cli.top_n {
        rows.truncate(n);
    }
    rows
}

fn print_table(results: &[(String, RouteStats)]) {
    println!(
        "{:<48} {:>6} {:>10} {:>10} {:>10} {:>10} {:>10} {:>10}",
        "ROUTE", "COUNT", "MIN", "MAX", "MEAN", "P50", "P95", "P99"
    );
    for (route, s) in results {
        println!(
            "{:<48} {:>6} {:>10.2} {:>10.2} {:>10.2} {:>10.2} {:>10.2} {:>10.2}",
            route, s.count, s.min, s.max, s.mean, s.p50, s.p95, s.p99
        );
    }
}

fn render_stats(grouped: &mut HashMap<String, Vec<f64>>, cli: &CliOptions) {
    let stats: HashMap<String, RouteStats> = grouped
        .iter_mut()
        .map(|(route, lats)| (route.clone(), compute_stats(lats)))
        .collect();
    let results = filter_and_sort(stats, cli);
    print_table(&results);
}

fn main() {
    let cli = CliOptions::parse();

    let mut offset = 0u64;
    let mut grouped = HashMap::new();

    loop {
        match read_new_log_entries(&cli.file, &mut offset) {
            Ok(new) => {
                if !new.is_empty() {
                    merge_entries(&mut grouped, &new);
                    render_stats(&mut grouped, &cli);
                }
            }
            Err(e) => eprintln!("watch: {}", e),
        }
        thread::sleep(Duration::from_millis(1000));
    }
}
