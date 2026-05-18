use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::ListItem;

use crate::router_parser::{Method, Route};

pub fn list_items(routes: &[Route]) -> Vec<ListItem<'static>> {
    routes
        .iter()
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
fn method_fg(method: &Method) -> ratatui::style::Color {
    use ratatui::style::Color;
    match method {
        Method::GET => Color::Green,
        Method::POST => Color::Cyan,
        Method::PUT => Color::Yellow,
        Method::DELETE => Color::Red,
    }
}
