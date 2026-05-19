use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::router_parser::{Method, Route};

pub fn filter_indices(routes: &[Route], query: &str) -> Vec<usize> {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return (0..routes.len()).collect();
    }
    routes
        .iter()
        .enumerate()
        .filter(|(_, r)| route_matches(&q, r))
        .map(|(i, _)| i)
        .collect()
}

fn route_matches(q: &str, route: &Route) -> bool {
    route.path().to_lowercase().contains(q)
        || route.summary().to_lowercase().contains(q)
        || route.method().to_string().to_lowercase().contains(q)
}

pub fn list_items(routes: &[Route], indices: &[usize]) -> Vec<ListItem<'static>> {
    indices
        .iter()
        .filter_map(|&i| routes.get(i))
        .map(|r| {
            let label = format!(" {:<7}", r.method());
            let fg = method_fg(r.method());
            let line = Line::from(vec![
                Span::styled(
                    label,
                    Style::default().fg(fg).add_modifier(Modifier::BOLD),
                ),
                Span::raw(" "),
                Span::raw(r.path().to_string()),
            ]);
            ListItem::new(line)
        })
        .collect()
}

pub(crate) fn method_fg(method: &Method) -> ratatui::style::Color {
    use ratatui::style::Color;
    match method {
        Method::GET => Color::Green,
        Method::POST => Color::Cyan,
        Method::PUT => Color::Yellow,
        Method::DELETE => Color::Red,
    }
}
