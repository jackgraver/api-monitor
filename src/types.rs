use clap::Parser;
use serde::Deserialize;
use std::fmt;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "api_benchmarker", about = "Analyze JSONL request logs")]
pub struct CliOptions {
    pub file: PathBuf,
    #[arg(long, help = "Sort output by: count | mean | p50 | p95 | p99 | max")]
    pub sort_by: Option<String>,
    #[arg(long, help = "Only show routes matching this substring")]
    pub filter_route: Option<String>,
    #[arg(long, help = "Hide routes with fewer than N requests")]
    pub min_count: Option<usize>,
    #[arg(long, help = "Show only the top N routes")]
    pub top_n: Option<usize>,
}

#[derive(Debug, Deserialize)]
pub struct LogEntry {
    pub route: String,
    pub method: String,
    pub latency_ms: f64,
    #[allow(dead_code)]
    pub timestamp: String,
}

impl fmt::Display for LogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {} {} {}", self.route, self.method, self.latency_ms, self.timestamp)
    }
}

#[derive(Debug)]
pub struct RouteStats {
    pub count: usize,
    pub min: f64,
    pub max: f64,
    pub mean: f64,
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
}