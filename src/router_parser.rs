use std::{fmt, fs::File, io::{BufRead, BufReader}};
use std::str::FromStr;
use std::sync::OnceLock;

use regex::Regex;
use rusqlite::Connection;

fn annotation_line_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^\s*//\s*(?P<annotation>@\w+)\s+(?P<content>.*)$").unwrap()
    })
}

struct Line {
    content: String,
    line_number: usize,
}

impl Line {
    pub fn new(content: String, line_number: usize) -> Self {
        Self { content, line_number }
    }
}

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

pub fn find_all_routes(file: &str, conn: &Connection) -> Vec<Route> {
    let mut routes = Vec::new();

    let database_routes = load_database_routes(conn);
    routes.extend(database_routes);

    match File::open(file) {
        Ok(file) => {
            let reader = BufReader::new(file);

            let mut line_count = 0;

            let mut current_route: Vec<Line> = Vec::new();

            for line_res in reader.split(b'\n') {
                line_count += 1;
                match line_res {
                    Ok(bytes) => {
                        let line = String::from_utf8_lossy(&bytes)
                            .trim_end_matches('\r')
                            .to_string();

                        let line = Line::new(line, line_count);

                        if line.content.is_empty() {
                            if current_route.is_empty() {
                                continue;
                            }

                            routes.push(parse_route(&current_route));
                            current_route.clear();
                            continue;
                        }
                        if line.content.starts_with("//") {
                            current_route.push(line);
                            continue;
                        }
                    }
                    Err(e) => {
                        eprintln!("Error reading line");
                        continue;
                    }
                }
            }

            for route in routes.iter() {
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

            routes
        },
        Err(e) => {
            eprintln!("Error opening file");
            Vec::new()
        },
    }
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