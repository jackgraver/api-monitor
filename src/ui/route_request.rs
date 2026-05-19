use std::time::{Duration, Instant};

use reqwest::blocking::Client;
use reqwest::Method as HttpMethod;

use crate::router_parser::Method;

const API_BASE: &str = "http://localhost:8080";
const AUTH_COOKIE_NAME: &str = "auth_token";

fn auth_cookie_header() -> Result<String, String> {
    let token = std::env::var("AUTH_TOKEN").map_err(|_| {
        "AUTH_TOKEN is not set. Add it to a .env file in the project root.".to_string()
    })?;
    Ok(format!("{AUTH_COOKIE_NAME}={token}"))
}

pub enum RequestOutcome {
    Success {
        status: u16,
        body: String,
        elapsed_ms: u128,
    },
    Error(String),
}

pub enum RequestState {
    Idle,
    Loading,
    Done(RequestOutcome),
}

pub fn send(path: &str, method: &Method) -> RequestOutcome {
    let cookie = match auth_cookie_header() {
        Ok(v) => v,
        Err(e) => return RequestOutcome::Error(e),
    };

    let url = format!("{API_BASE}{path}");
    let client = match Client::builder().timeout(Duration::from_secs(30)).build() {
        Ok(c) => c,
        Err(e) => return RequestOutcome::Error(e.to_string()),
    };

    let mut req = client
        .request(to_http_method(method), &url)
        .header("Cookie", cookie);
    if matches!(method, Method::POST | Method::PUT) {
        req = req
            .header("Content-Type", "application/json")
            .body("{}");
    }

    let started = Instant::now();
    match req.send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.text().unwrap_or_default();
            RequestOutcome::Success {
                status,
                body,
                elapsed_ms: started.elapsed().as_millis(),
            }
        }
        Err(e) => RequestOutcome::Error(e.to_string()),
    }
}

fn to_http_method(method: &Method) -> HttpMethod {
    match method {
        Method::GET => HttpMethod::GET,
        Method::POST => HttpMethod::POST,
        Method::PUT => HttpMethod::PUT,
        Method::DELETE => HttpMethod::DELETE,
    }
}
