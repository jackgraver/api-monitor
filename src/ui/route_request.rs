use std::time::{Duration, Instant};

use reqwest::Method as HttpMethod;
use reqwest::blocking::{Client, RequestBuilder};

use crate::error::RequestError;
use crate::router_parser::Method;

const DEFAULT_API_BASE: &str = "http://localhost:8080";
const DEFAULT_HEALTH_PATH: &str = "/health";
const AUTH_COOKIE_NAME: &str = "auth_token";
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(3);

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

pub enum HealthOutcome {
    Up {
        status: u16,
        elapsed_ms: u128,
    },
    Down(String),
}

#[derive(Clone, PartialEq, Eq)]
pub enum HealthState {
    Unknown,
    Checking,
    Up {
        status: u16,
        elapsed_ms: u128,
    },
    Down(String),
}

impl HealthState {
    pub fn is_up(&self) -> bool {
        matches!(self, HealthState::Up { .. })
    }

    pub fn apply_outcome(&mut self, outcome: HealthOutcome) {
        *self = match outcome {
            HealthOutcome::Up {
                status,
                elapsed_ms,
            } => HealthState::Up {
                status,
                elapsed_ms,
            },
            HealthOutcome::Down(msg) => HealthState::Down(msg),
        };
    }
}

pub fn api_base() -> String {
    std::env::var("API_BASE").unwrap_or_else(|_| DEFAULT_API_BASE.to_string())
}

pub fn health_url() -> String {
    let base = api_base();
    let path = health_path();
    format!("{}{}", base.trim_end_matches('/'), path)
}

fn health_path() -> String {
    let raw = std::env::var("API_HEALTH_PATH").unwrap_or_else(|_| DEFAULT_HEALTH_PATH.to_string());
    if raw.starts_with('/') {
        raw
    } else {
        format!("/{raw}")
    }
}

pub fn check_health() -> HealthOutcome {
    let client = match Client::builder().timeout(HEALTH_TIMEOUT).build() {
        Ok(c) => c,
        Err(e) => return HealthOutcome::Down(format!("client: {e}")),
    };

    let url = health_url();
    let started = Instant::now();
    match client.get(&url).send() {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let elapsed_ms = started.elapsed().as_millis();
            if resp.status().is_success() {
                HealthOutcome::Up {
                    status,
                    elapsed_ms,
                }
            } else {
                HealthOutcome::Down(format!("HTTP {status}"))
            }
        }
        Err(e) => HealthOutcome::Down(e.to_string()),
    }
}

pub fn send(path: &str, method: &Method) -> RequestOutcome {
    match try_send(path, method) {
        Ok(outcome) => outcome,
        Err(e) => RequestOutcome::Error(e.to_string()),
    }
}

fn try_send(path: &str, method: &Method) -> Result<RequestOutcome, RequestError> {
    let cookie = auth_cookie_header()?;
    let client = Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .build()
        .map_err(RequestError::ClientBuild)?;

    let url = format!("{}{}", api_base().trim_end_matches('/'), path);
    let req = build_request(&client, method, &url, &cookie);

    let started = Instant::now();
    let resp = req.send().map_err(RequestError::Send)?;
    let status = resp.status().as_u16();
    let body = resp.text().map_err(RequestError::ReadBody)?;
    Ok(RequestOutcome::Success {
        status,
        body,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

fn build_request(client: &Client, method: &Method, url: &str, cookie: &str) -> RequestBuilder {
    let mut req = client
        .request(to_http_method(method), url)
        .header("Cookie", cookie);
    if matches!(method, Method::POST | Method::PUT) {
        req = req.header("Content-Type", "application/json").body("{}");
    }
    req
}

fn auth_cookie_header() -> Result<String, RequestError> {
    let token = std::env::var("AUTH_TOKEN").map_err(|_| RequestError::MissingAuthToken)?;
    Ok(format!("{AUTH_COOKIE_NAME}={token}"))
}

fn to_http_method(method: &Method) -> HttpMethod {
    match method {
        Method::GET => HttpMethod::GET,
        Method::POST => HttpMethod::POST,
        Method::PUT => HttpMethod::PUT,
        Method::DELETE => HttpMethod::DELETE,
    }
}
