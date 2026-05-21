use std::fmt;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::OnceLock;

use regex::Regex;
use rusqlite::Connection;

use crate::db::default_app_id;

fn annotation_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*//\s*(?P<annotation>@\w+)\s+(?P<content>.*)$")
            .expect("annotation regex is a static literal and must compile")
    })
}

#[derive(Clone)]
struct Line {
    content: String,
    line_number: usize,
}

impl Line {
    fn new(content: String, line_number: usize) -> Self {
        Self { content, line_number }
    }
}

#[derive(Clone, Copy)]
pub enum Method {
    GET,
    POST,
    PUT,
    DELETE,
}

impl FromStr for Method {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_uppercase().as_str() {
            "GET" => Ok(Method::GET),
            "POST" => Ok(Method::POST),
            "PUT" => Ok(Method::PUT),
            "DELETE" => Ok(Method::DELETE),
            _ => Err(()),
        }
    }
}

impl fmt::Display for Method {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Method::GET => "GET",
            Method::POST => "POST",
            Method::PUT => "PUT",
            Method::DELETE => "DELETE",
        })
    }
}

pub struct Param {
    name: String,
    ty: String,
    required: bool,
}

impl Param {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ty(&self) -> &str {
        &self.ty
    }

    pub fn required(&self) -> bool {
        self.required
    }
}

pub struct Route {
    summary: String,
    path: String,
    method: Method,
    query_params: Vec<Param>,
    body_params: Vec<Param>,
}

impl Route {
    pub fn new() -> Self {
        Self {
            summary: String::new(),
            path: String::new(),
            method: Method::GET,
            query_params: Vec::new(),
            body_params: Vec::new(),
        }
    }

    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn method(&self) -> &Method {
        &self.method
    }

    pub fn summary(&self) -> &str {
        &self.summary
    }

    pub fn query_params(&self) -> &[Param] {
        &self.query_params
    }

    pub fn body_params(&self) -> &[Param] {
        &self.body_params
    }
}

impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.method, self.path)
    }
}

pub fn find_all_routes(root_directory: &Path, conn: &Connection) -> Vec<Route> {
    let app_id = match default_app_id(conn) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("default app: {e}");
            return Vec::new();
        }
    };

    let mut go_files = Vec::new();
    collect_go_files(root_directory, &mut go_files);

    for path in &go_files {
        if let Err(e) = scan_go_file(path, conn, app_id) {
            eprintln!("scan {}: {e}", path.display());
        }
    }

    match load_database_routes(conn, app_id) {
        Ok(routes) => routes,
        Err(e) => {
            eprintln!("load routes: {e}");
            Vec::new()
        }
    }
}

fn collect_go_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            eprintln!("Error reading directory {}: {}", dir.display(), e);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_go_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "go") {
            out.push(path);
        }
    }
}

fn scan_go_file(path: &Path, conn: &Connection, app_id: i64) -> std::io::Result<()> {
    let file = File::open(path)?;
    let reader = BufReader::new(file);
    let mut line_count = 0_usize;
    let mut current_route: Vec<Line> = Vec::new();

    for line_res in reader.split(b'\n') {
        line_count += 1;
        let bytes = match line_res {
            Ok(b) => b,
            Err(e) => {
                eprintln!("read {} line {line_count}: {e}", path.display());
                continue;
            }
        };

        let content = String::from_utf8_lossy(&bytes)
            .trim_end_matches('\r')
            .to_string();

        let line = Line::new(content, line_count);

        if line.content.is_empty() {
            flush_route_block(&mut current_route, conn, app_id);
            continue;
        }

        if line.content.starts_with("//") {
            handle_comment_line(&line, conn, &mut current_route);
            continue;
        }

        flush_route_block(&mut current_route, conn, app_id);
    }

    flush_route_block(&mut current_route, conn, app_id);
    Ok(())
}

fn handle_comment_line(line: &Line, conn: &Connection, current_route: &mut Vec<Line>) {
    if let Some(controller_name) = parse_controller_annotation(&line.content) {
        if let Err(e) = save_controller_if_new(conn, &controller_name) {
            eprintln!("save controller `{controller_name}`: {e}");
        }
        return;
    }

    if is_route_annotation(&line.content) || !current_route.is_empty() {
        current_route.push(line.clone());
    }
}

fn parse_controller_annotation(content: &str) -> Option<String> {
    let caps = annotation_line_re().captures(content)?;
    let annotation = caps.name("annotation")?.as_str();
    if annotation != "@Controller" {
        return None;
    }
    let name = caps.name("content")?.as_str().trim();
    if name.is_empty() {
        return None;
    }
    Some(name.to_string())
}

fn is_route_annotation(content: &str) -> bool {
    let Some(caps) = annotation_line_re().captures(content) else {
        return false;
    };
    let Some(annotation) = caps.name("annotation").map(|m| m.as_str()) else {
        return false;
    };
    matches!(
        annotation,
        "@Summary" | "@Route" | "@Method" | "@Param" | "@Body"
    )
}

fn flush_route_block(current_route: &mut Vec<Line>, conn: &Connection, app_id: i64) {
    if current_route.is_empty() {
        return;
    }

    let route = parse_route(current_route);
    current_route.clear();

    if route.path().is_empty() {
        return;
    }

    if let Err(e) = save_route_if_new(conn, &route, app_id) {
        eprintln!("save route `{} {}`: {e}", route.method(), route.path());
    }
}

fn save_controller_if_new(conn: &Connection, name: &str) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT INTO controllers (name)
         SELECT ?1
         WHERE NOT EXISTS (SELECT 1 FROM controllers WHERE name = ?1)",
        [name],
    )?;
    Ok(())
}

fn save_route_if_new(conn: &Connection, route: &Route, app_id: i64) -> rusqlite::Result<()> {
    let method_str = route.method().to_string();
    conn.execute(
        "INSERT INTO routes (app_id, summary, path, method)
         SELECT ?1, ?2, ?3, ?4
         WHERE NOT EXISTS (
             SELECT 1 FROM routes
             WHERE app_id = ?1 AND path = ?3 AND method = ?4
         )",
        (
            app_id,
            route.summary(),
            route.path(),
            method_str.as_str(),
        ),
    )?;
    Ok(())
}

fn parse_route(lines: &[Line]) -> Route {
    let mut route = Route::new();
    let re = annotation_line_re();

    for line in lines {
        let Some(caps) = re.captures(&line.content) else {
            continue;
        };
        let Some(annotation) = caps.name("annotation").map(|m| m.as_str()) else {
            continue;
        };
        let content = caps
            .name("content")
            .map(|m| m.as_str().trim())
            .unwrap_or_default();

        match annotation {
            "@Summary" if !content.is_empty() => route.summary = content.to_string(),
            "@Summary" => eprintln!("Summary missing on line {}", line.line_number),
            "@Route" if !content.is_empty() => route.path = content.to_string(),
            "@Route" => eprintln!("Path missing on line {}", line.line_number),
            "@Method" => {
                if content.is_empty() {
                    eprintln!("Method not found on line {}", line.line_number);
                    continue;
                }
                let method = content
                    .replace(['[', ']'], "")
                    .trim()
                    .to_uppercase();
                match Method::from_str(&method) {
                    Ok(m) => route.method = m,
                    Err(_) => eprintln!(
                        "Unknown HTTP method `{method}` on line {}",
                        line.line_number
                    ),
                }
            }
            "@Param" | "@Body" => {}
            _ => {}
        }
    }

    route
}

fn load_database_routes(conn: &Connection, app_id: i64) -> rusqlite::Result<Vec<Route>> {
    let mut stmt =
        conn.prepare("SELECT summary, path, method FROM routes WHERE app_id = ?1")?;
    let rows = stmt.query_map([app_id], |row| {
        let summary: String = row.get(0)?;
        let path: String = row.get(1)?;
        let method_str: String = row.get(2)?;
        let method = Method::from_str(&method_str).unwrap_or(Method::GET);
        Ok(Route {
            summary,
            path,
            method,
            query_params: Vec::new(),
            body_params: Vec::new(),
        })
    })?;
    let mut out = Vec::new();
    for row in rows {
        match row {
            Ok(route) => out.push(route),
            Err(e) => eprintln!("row decode: {e}"),
        }
    }
    Ok(out)
}
