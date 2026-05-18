#![allow(unused)]

use clap::Parser;
use rusqlite::Connection;
use std::collections::HashMap;
use std::thread;
use std::time::Duration;

mod types;
mod log_parser;
mod router_parser;
mod ui;

use log_parser::LogParser;
use types::{CliOptions, LogEntry, RouteStats};

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


fn initialize_app_database_connection() -> Option<Connection> {
    match Connection::open("amon.db") {
        Ok(conn) => {
            println!("Database opened successfully");
            Some(conn)
        }
        Err(e) => {
            eprintln!("Error opening database: {}", e);
            None
        }
    }
}

fn main() {
    // let conn = conn.unwrap();
    // let route_stmt = "CREATE TABLE IF NOT EXISTS routes (id INTEGER PRIMARY KEY AUTOINCREMENT, summary TEXT, path TEXT, method TEXT, query_params TEXT, body_params TEXT)";
    // let mut create_route_stmt = conn.prepare(route_stmt).unwrap();
    // create_route_stmt.execute([]).unwrap();

    let Some(conn) = initialize_app_database_connection() else {
        eprintln!("'Failed' to open database");
        return;
    };
    
    let routes = router_parser::find_all_routes("", &conn);
    if let Err(e) = ui::run(routes) {
        eprintln!("ui: {e}");
    }

    // let mut parser = LogParser::open("data/sample_logs.jsonl").unwrap();
    // let mut grouped = HashMap::new();
    // let cli = CliOptions::parse();

    // loop {
    //     match parser.read_new_log_entries() {
    //         Ok(new) => {
    //             if !new.is_empty() {
    //                 merge_entries(&mut grouped, &new);
    //                 render_stats(&mut grouped, &cli);
    //             }
    //         }
    //         Err(e) => eprintln!("watch: {}", e),
    //     }
    //     thread::sleep(Duration::from_millis(1000));
    // }
}
