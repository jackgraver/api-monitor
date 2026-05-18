use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Paragraph, Wrap};

use crate::router_parser::{Param, Route};

use super::route_list::method_fg;

pub fn detail_paragraph(route: Option<&Route>) -> Paragraph<'static> {
    let lines: Vec<Line<'static>> = match route {
        None => vec![Line::from(vec![Span::styled(
            "(no route selected)",
            Style::default().fg(Color::DarkGray),
        )])],
        Some(r) => build_lines(r),
    };
    Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Detail "),
        )
        .wrap(Wrap { trim: true })
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
