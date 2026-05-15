use std::{
    fs::File,
    io::{BufRead, BufReader},
};

enum Method {
    GET,
    POST,
    PUT,
    DELETE,
}

enum ConfigDelimiter {

}

struct Param {
    name: String,
    ty: String,
    required: bool,
}

struct Route {
    path: String,
    method: Method,
    params: Vec<Param>,
}

pub fn find_all_routes(file: &str) -> Vec<String> {
    let file = File::open(file).unwrap();
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
                    routes.push(current_route.join("\n"));
                    current_route.clear();
                    continue;
                }
                if line.starts_with("//") {
                    current_route.push(line);
                    continue;
                }
            }
            Err(e) => {
                eprintln!("Error reading line: {}", e);
                continue;
            }
        }
    }
    routes
}

// fn parse_route(lines: &[String]) -> Route {
    // let a = " da";
    // println!("test {}", a.trim_start().starts_with("da"));

// }