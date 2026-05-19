use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::router_parser::{Param, Route};

use super::route_list::method_fg;
use super::route_request::{RequestOutcome, RequestState};

const MAX_BODY_DISPLAY: usize = 50_000;

pub fn detail_lines(route: Option<&Route>, request: &RequestState) -> Vec<Line<'static>> {
    match route {
        None => vec![Line::from(vec![Span::styled(
            "(no route selected)",
            Style::default().fg(Color::DarkGray),
        )])],
        Some(r) => {
            let mut lines = build_lines(r);
            append_request_lines(&mut lines, request);
            lines
        }
    }
}

pub fn detail_paragraph(lines: Vec<Line<'static>>, scroll_y: u16) -> Paragraph<'static> {
    Paragraph::new(lines)
        .wrap(Wrap { trim: false })
        .scroll((0, scroll_y))
}

pub fn line_count(lines: &[Line], _width: u16) -> u16 {
    lines.len() as u16
}

fn build_lines(route: &Route) -> Vec<Line<'static>> {
    let mut lines = Vec::new();
    let m_fg = method_fg(route.method());
    lines.push(Line::from(vec![
        Span::raw(route.summary().to_string()),
        Span::raw(" | "),
        Span::styled(
            route.method().to_string(),
            Style::default().fg(m_fg).add_modifier(Modifier::BOLD),
        ),
        Span::raw(" "),
        Span::raw(route.path().to_string()),
    ]));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Query parameters",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    append_param_lines(&mut lines, route.query_params());
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Body parameters",
        Style::default().add_modifier(Modifier::BOLD),
    )));
    append_param_lines(&mut lines, route.body_params());
    lines
}

fn append_request_lines(out: &mut Vec<Line<'static>>, request: &RequestState) {
    out.push(Line::from(""));
    out.push(Line::from(Span::styled(
        "Response",
        Style::default().add_modifier(Modifier::BOLD),
    )));

    match request {
        RequestState::Idle => {
            out.push(Line::from(vec![Span::styled(
                "  Press Enter to send a request to localhost:8080",
                Style::default().fg(Color::DarkGray),
            )]));
        }
        RequestState::Loading => {
            out.push(Line::from(vec![Span::styled(
                "  Sending request…",
                Style::default().fg(Color::Yellow),
            )]));
        }
        RequestState::Done(outcome) => match outcome {
            RequestOutcome::Success {
                status,
                body,
                elapsed_ms,
            } => {
                let status_color = if (200..300).contains(status) {
                    Color::Green
                } else {
                    Color::Red
                };
                out.push(Line::from(vec![
                    Span::raw("  Status: "),
                    Span::styled(
                        status.to_string(),
                        Style::default()
                            .fg(status_color)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::raw(format!("  ({elapsed_ms} ms)")),
                ]));
                out.push(Line::from(""));
                let display = truncate_body(&format_response_body(body));
                for line in display.lines() {
                    out.push(Line::from(Span::raw(line.to_string())));
                }
            }
            RequestOutcome::Error(msg) => {
                out.push(Line::from(vec![Span::styled(
                    format!("  Error: {msg}"),
                    Style::default().fg(Color::Red),
                )]));
            }
        },
    }
}

fn format_response_body(body: &str) -> String {
    let trimmed = body.trim();
    if trimmed.is_empty() {
        return String::new();
    }
    match serde_json::from_str::<serde_json::Value>(trimmed) {
        Ok(value) => serde_json::to_string_pretty(&value).unwrap_or_else(|_| body.to_string()),
        Err(_) => body.to_string(),
    }
}

fn truncate_body(body: &str) -> String {
    if body.len() <= MAX_BODY_DISPLAY {
        return body.to_string();
    }
    format!(
        "{}…\n\n(truncated, {} bytes total)",
        &body[..MAX_BODY_DISPLAY],
        body.len()
    )
}

fn append_param_lines(out: &mut Vec<Line<'static>>, params: &[Param]) {
    if params.is_empty() {
        out.push(Line::from(vec![Span::styled(
            "  (none)",
            Style::default().fg(Color::DarkGray),
        )]));
        return;
    }
    for p in params {
        out.push(param_line(p));
    }
}

fn param_line(p: &Param) -> Line<'static> {
    let req = if p.required() { " · required" } else { "" };
    Line::from(vec![
        Span::raw("  • ".to_string()),
        Span::styled(p.name().to_string(), Style::default().fg(Color::White)),
        Span::raw(format!(" ({}){}", p.ty(), req)),
    ])
}
