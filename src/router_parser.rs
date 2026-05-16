use std::{fmt, fs::File, io::{BufRead, BufReader}};
enum Method {
    GET,
    POST,
    PUT,
    DELETE,
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

    pub fn string_to_route_method(method: &str) -> Option<Method> {
        let method = method.replace("[", "").replace("]", "").to_uppercase();

        match method.as_str() {
            "GET" => Some(Method::GET),
            "POST" => Some(Method::POST),
            "PUT" => Some(Method::PUT),
            "DELETE" => Some(Method::DELETE),
            _ => None,
        }
    }
}


impl fmt::Display for Route {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.method, self.path)
    }
}

pub fn find_all_routes(file: &str) -> Vec<Route> {
    match File::open(file) {
        Ok(file) => {
            let reader = BufReader::new(file);
            let mut routes = Vec::new();

            let mut current_route: Vec<String> = Vec::new();

            for line_res in reader.split(b'\n') {
                match line_res {
                    Ok(bytes) => {
                        let line = String::from_utf8_lossy(&bytes)
                            .trim_end_matches('\r')
                            .to_string();
                        if line.is_empty() {
                            if current_route.is_empty() {
                                continue;
                            }

                            routes.push(parse_route(&current_route));
                            current_route.clear();
                            continue;
                        }
                        if line.starts_with("//") {
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
            routes
        },
        Err(e) => {
            eprintln!("Error opening file");
            Vec::new()
        },
    }
}

fn parse_route(lines: &[String]) -> Route {
    let mut route = Route::new();
    println!("Base route? {}", route);

    for line in lines {
        let parts = line.split_whitespace().collect::<Vec<_>>();

        let Some(tag) = parts.get(1) else {
            continue;
        };

        match *tag {
            "@Summary" => {
                if let Some(summary) = parts.get(2) {
                    route.summary = summary.to_string();
                }
            }
            "@Route" => {
                if let Some(path) = parts.get(2) {
                    route.path = path.to_string();
                }
            }
            "@Method" => {
                if let Some(method) = parts.get(2) {
                    if let Some(method) = Route::string_to_route_method(method) {
                        route.method = method;
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