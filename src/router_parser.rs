use std::fmt;
use std::fs::{self, File};
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::OnceLock;

use regex::Regex;
use rusqlite::Connection;

const API_PROJECT_ROOT: &str = r"C:\Users\Jacks Desktop\Desktop\Coding\simpletracker";

fn annotation_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*//\s*(?P<annotation>@\w+)\s+(?P<content>.*)$").unwrap()
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

pub fn find_all_routes(_root_directory: &str, conn: &Connection) -> Vec<Route> {
    let root = Path::new(API_PROJECT_ROOT);
    let mut go_files = Vec::new();
    collect_go_files(root, &mut go_files);

    for path in &go_files {
        scan_go_file(path, conn);
    }

    load_database_routes(conn)
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

fn scan_go_file(path: &Path, conn: &Connection) {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("Error opening {}: {}", path.display(), e);
            return;
        }
    };

    let reader = BufReader::new(file);
    let mut line_count = 0;
    let mut current_route: Vec<Line> = Vec::new();

    for line_res in reader.split(b'\n') {
        line_count += 1;
        let Ok(bytes) = line_res else {
            eprintln!("Error reading line in {}", path.display());
            continue;
        };

        let content = String::from_utf8_lossy(&bytes)
            .trim_end_matches('\r')
            .to_string();

        let line = Line::new(content, line_count);

        if line.content.is_empty() {
            flush_route_block(&mut current_route, conn);
            continue;
        }

        if line.content.starts_with("//") {
            handle_comment_line(&line, conn, &mut current_route);
            continue;
        }

        flush_route_block(&mut current_route, conn);
    }

    flush_route_block(&mut current_route, conn);
}

fn handle_comment_line(line: &Line, conn: &Connection, current_route: &mut Vec<Line>) {
    if let Some(controller_name) = parse_controller_annotation(&line.content) {
        save_controller_if_new(conn, &controller_name);
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
    let annotation = caps.name("annotation").map(|m| m.as_str()).unwrap_or("");
    matches!(
        annotation,
        "@Summary" | "@Route" | "@Method" | "@Param" | "@Body"
    )
}

fn flush_route_block(current_route: &mut Vec<Line>, conn: &Connection) {
    if current_route.is_empty() {
        return;
    }

    let route = parse_route(current_route);
    current_route.clear();

    if route.path().is_empty() {
        return;
    }

    let method_str = route.method().to_string();
    if route_exists_in_db(conn, route.path(), &method_str) {
        return;
    }

    save_route_if_new(conn, &route);
}

fn route_exists_in_db(conn: &Connection, path: &str, method: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM routes WHERE path = ?1 AND method = ?2",
        [path, method],
        |_| Ok(()),
    )
    .is_ok()
}

fn controller_exists_in_db(conn: &Connection, name: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM controllers WHERE name = ?1",
        [name],
        |_| Ok(()),
    )
    .is_ok()
}

fn save_controller_if_new(conn: &Connection, name: &str) {
    if controller_exists_in_db(conn, name) {
        return;
    }

    let _ = conn.execute(
        "INSERT INTO controllers (name)
         SELECT ?1
         WHERE NOT EXISTS (SELECT 1 FROM controllers WHERE name = ?1)",
        [name],
    );
}

fn save_route_if_new(conn: &Connection, route: &Route) {
    let method_str = route.method().to_string();
    let _ = conn.execute(
        "INSERT INTO routes (summary, path, method)
         SELECT ?1, ?2, ?3
         WHERE NOT EXISTS (
             SELECT 1 FROM routes WHERE path = ?2 AND method = ?3
         )",
        (route.summary(), route.path(), method_str.as_str()),
    );
}

fn parse_route(lines: &[Line]) -> Route {
    let mut route = Route::new();
    let re = annotation_line_re();

    for line in lines {
        let Some(caps) = re.captures(&line.content) else {
            continue;
        };
        let annotation = caps.name("annotation").map(|m| m.as_str()).unwrap_or("");
        let content = caps
            .name("content")
            .map(|m| m.as_str().trim())
            .unwrap_or("");

        match annotation {
            "@Summary" => {
                if content.is_empty() {
                    eprintln!("Summary missing on line {}", line.line_number);
                } else {
                    route.summary = content.to_string();
                }
            }
            "@Route" => {
                if content.is_empty() {
                    eprintln!("Path missing on line {}", line.line_number);
                } else {
                    route.path = content.to_string();
                }
            }
            "@Method" => {
                if content.is_empty() {
                    eprintln!("Method not found on line {}", line.line_number);
                    continue;
                }
                let method = content.replace('[', "").replace(']', "").trim().to_uppercase();
                match Method::from_str(&method) {
                    Ok(m) => route.method = m,
                    Err(_) => {
                        eprintln!("Unknown HTTP method on line {}", line.line_number);
                    }
                }
            }
            "@Param" => {}
            "@Body" => {}
            _ => continue,
        }
    }

    route
}

fn load_database_routes(conn: &Connection) -> Vec<Route> {
    let mut stmt = match conn.prepare("SELECT summary, path, method FROM routes") {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let mapped = stmt.query_map([], |row| {
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
    });
    match mapped {
        Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
        Err(_) => Vec::new(),
    }
}
