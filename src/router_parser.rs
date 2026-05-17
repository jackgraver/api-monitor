use std::{fmt, fs::File, io::{BufRead, BufReader}};
use std::str::FromStr;
use rusqlite::Connection;

struct Line {
    content: String,
    line_number: usize,
}

impl Line {
    pub fn new(content: String, line_number: usize) -> Self {
        Self { content, line_number }
    }
}

enum Method {
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

struct Param {
    name: String,
    ty: String,
    required: bool,
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
                conn.execute("INSERT INTO routes (summary, path, method) VALUES (?, ?, ?)", (&route.summary, &route.path, &route.method.to_string()));
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
    println!("Base route? {}", route);

    for line in lines {
        let parts = line.content.split_whitespace().collect::<Vec<_>>();

        let Some(tag) = parts.get(1) else {
            continue;
        };

        match *tag {
            "@Summary" => {
                match parts.get(2) {
                    Some(summary) => route.summary = summary.to_string(),
                    None => {
                        eprintln!("Summary not found on line {}", line.line_number);
                        continue;
                    }
                }
            }
            "@Route" => {
                match parts.get(2) {
                    Some(path) => route.path = path.to_string(),
                    None => {
                        eprintln!("Path not found on line {}", line.line_number);
                        continue;
                    }
                }
            }
            "@Method" => {

                match parts.get(2) {
                    Some(method) => {
                        let method = method.replace("[", "").replace("]", "").to_uppercase();
                        match Method::from_str(&method) {
                            Ok(method) => route.method = method,
                            Err(_) => {
                                eprintln!("Unknown HTTP method on line {}", line.line_number);
                                continue;
                            }
                        }
                    }
                    None => {
                        eprintln!("Method not found on line {}", line.line_number);
                        continue;
                    }
                }
            }
            "@Param" => {
            }
            "@Body" => {
            }
            _ => {
                continue;
            }
        }
    }

    route
}

fn load_database_routes(conn: &Connection) -> Vec<Route> {
    let mut stmt = conn.prepare("SELECT * FROM routes").unwrap();
    let routes = stmt.query_map([], |row| {
        Ok(Route {
            summary: row.get(0)?,
            path: row.get(1)?,
            method: row.get(2)?,
            query_params: row.get(3)?,
            body_params: row.get(4)?,
        })
    }).unwrap().collect();
    routes
}