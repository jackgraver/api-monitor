use std::io::{self, stdout, Stdout};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{
    Block, Borders, List, ListState, Paragraph, Scrollbar, ScrollbarOrientation, ScrollbarState,
};
use ratatui::Terminal;

use crate::router_parser::Route;

use super::route_detail;
use super::route_list;
use super::route_request::{self, RequestOutcome, RequestState};

#[derive(Clone, Copy, PartialEq, Eq)]
enum Focus {
    Search,
    Routes,
    Detail,
}

struct LayoutRects {
    detail_inner_width: u16,
    detail_inner_height: u16,
}

struct App {
    routes: Vec<Route>,
    search: String,
    filtered: Vec<usize>,
    list_state: ListState,
    focus: Focus,
    detail_scroll: u16,
    request: RequestState,
    request_rx: Option<Receiver<RequestOutcome>>,
    last_selected_route: Option<usize>,
}

pub fn run(routes: Vec<Route>) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(routes);
    let app_result = run_loop(&mut terminal, &mut app);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    app_result
}

impl App {
    fn new(routes: Vec<Route>) -> Self {
        let filtered = route_list::filter_indices(&routes, "");
        let mut list_state = ListState::default();
        if !filtered.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            routes,
            search: String::new(),
            filtered,
            list_state,
            focus: Focus::Routes,
            detail_scroll: 0,
            request: RequestState::Idle,
            request_rx: None,
            last_selected_route: None,
        }
    }

    fn layout(area: Rect) -> LayoutRects {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(area);

        let detail_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(columns[1]);

        let detail = detail_chunks[0];
        LayoutRects {
            detail_inner_width: detail.width.saturating_sub(2),
            detail_inner_height: detail.height.saturating_sub(2),
        }
    }

    fn apply_search(&mut self) {
        self.filtered = route_list::filter_indices(&self.routes, &self.search);
        let n = self.filtered.len().saturating_sub(1);
        let i = self.list_state.selected().unwrap_or(0).min(n);
        self.list_state.select(if self.filtered.is_empty() {
            None
        } else {
            Some(i)
        });
    }

    fn selected_route(&self) -> Option<&Route> {
        let list_i = self.list_state.selected()?;
        let route_i = *self.filtered.get(list_i)?;
        self.routes.get(route_i)
    }

    fn selected_route_index(&self) -> Option<usize> {
        let list_i = self.list_state.selected()?;
        self.filtered.get(list_i).copied()
    }

    fn on_selection_change(&mut self) {
        let route_i = self.selected_route_index();
        if route_i != self.last_selected_route {
            self.last_selected_route = route_i;
            self.request = RequestState::Idle;
            self.request_rx = None;
            self.detail_scroll = 0;
        }
    }

    fn send_request(&mut self) {
        if self.request_rx.is_some() {
            return;
        }
        let Some(route) = self.selected_route() else {
            return;
        };
        let path = route.path().to_string();
        let method = *route.method();

        self.request = RequestState::Loading;
        self.detail_scroll = 0;
        let (tx, rx) = mpsc::channel();
        self.request_rx = Some(rx);

        thread::spawn(move || {
            let outcome = route_request::send(&path, &method);
            let _ = tx.send(outcome);
        });
    }

    fn poll_request(&mut self) {
        let Some(rx) = self.request_rx.as_ref() else {
            return;
        };
        match rx.try_recv() {
            Ok(outcome) => {
                self.request = RequestState::Done(outcome);
                self.request_rx = None;
                self.detail_scroll = 0;
            }
            Err(mpsc::TryRecvError::Empty) => {}
            Err(mpsc::TryRecvError::Disconnected) => {
                self.request = RequestState::Done(RequestOutcome::Error(
                    "Request thread ended unexpectedly".to_string(),
                ));
                self.request_rx = None;
            }
        }
    }

    fn cycle_focus(&mut self) {
        self.focus = match self.focus {
            Focus::Search => Focus::Routes,
            Focus::Routes => Focus::Detail,
            Focus::Detail => Focus::Search,
        };
    }

    fn clamp_detail_scroll(&mut self, layout: &LayoutRects, lines: &[Line]) {
        let total = route_detail::line_count(lines, layout.detail_inner_width);
        let max_scroll = total.saturating_sub(layout.detail_inner_height);
        self.detail_scroll = self.detail_scroll.min(max_scroll);
    }

    fn scroll_detail(&mut self, delta: i16, layout: &LayoutRects, lines: &[Line]) {
        let total = route_detail::line_count(lines, layout.detail_inner_width);
        let max_scroll = total.saturating_sub(layout.detail_inner_height);

        if delta < 0 {
            self.detail_scroll = self.detail_scroll.saturating_sub((-delta) as u16);
        } else {
            self.detail_scroll = (self.detail_scroll + delta as u16).min(max_scroll);
        }
    }

    fn scroll_detail_page(&mut self, up: bool, layout: &LayoutRects, lines: &[Line]) {
        let page = layout.detail_inner_height.saturating_sub(1).max(1) as i16;
        self.scroll_detail(if up { -page } else { page }, layout, lines);
    }
}

fn terminal_area(terminal: &Terminal<CrosstermBackend<Stdout>>) -> io::Result<Rect> {
    let size = terminal.size()?;
    Ok(Rect::new(0, 0, size.width, size.height))
}

fn search_block_title(focus: Focus) -> &'static str {
    if focus == Focus::Search {
        " Search (/ focus, Esc clear) "
    } else {
        " Search (/) "
    }
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        app.poll_request();

        let area = terminal_area(terminal)?;
        let layout = App::layout(area);
        let detail_lines = route_detail::detail_lines(app.selected_route(), &app.request);
        app.clamp_detail_scroll(&layout, &detail_lines);

        terminal.draw(|f| {
            let area = f.area();
            let layout = App::layout(area);

            let columns = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
                .split(area);

            let left = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Length(3), Constraint::Min(0)])
                .split(columns[0]);

            let search_style = if app.focus == Focus::Search {
                Style::default().fg(Color::Yellow)
            } else {
                Style::default().fg(Color::DarkGray)
            };
            let search_line = if app.search.is_empty() {
                Line::from(vec![
                    Span::styled("Filter: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled("(type to filter routes)", search_style),
                ])
            } else {
                Line::from(vec![
                    Span::styled("Filter: ", Style::default().add_modifier(Modifier::BOLD)),
                    Span::styled(app.search.clone(), search_style),
                ])
            };
            let search = Paragraph::new(vec![search_line]).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(search_block_title(app.focus))
                    .border_style(if app.focus == Focus::Search {
                        Style::default().fg(Color::Yellow)
                    } else {
                        Style::default()
                    }),
            );
            f.render_widget(search, left[0]);

            let list_title = if app.focus == Focus::Routes {
                " Routes (Enter send, Tab focus) "
            } else {
                " Routes "
            };
            let items = route_list::list_items(&app.routes, &app.filtered);
            let list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(list_title)
                        .border_style(if app.focus == Focus::Routes {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        }),
                )
                .highlight_style(Style::default().bg(Color::DarkGray));
            f.render_stateful_widget(list, left[1], &mut app.list_state);

            let detail_lines =
                route_detail::detail_lines(app.selected_route(), &app.request);
            app.clamp_detail_scroll(&layout, &detail_lines);

            let detail = route_detail::detail_paragraph(detail_lines.clone(), app.detail_scroll)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Detail (Enter send, Tab focus, PgUp/Dn scroll) ")
                        .border_style(if app.focus == Focus::Detail {
                            Style::default().fg(Color::Yellow)
                        } else {
                            Style::default()
                        }),
                );

            let detail_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0), Constraint::Length(1)])
                .split(columns[1]);

            f.render_widget(detail, detail_chunks[0]);

            let total_lines =
                route_detail::line_count(&detail_lines, layout.detail_inner_width) as usize;
            let visible = layout.detail_inner_height as usize;
            if total_lines > visible {
                let mut scrollbar_state =
                    ScrollbarState::new(total_lines).position(app.detail_scroll as usize);
                f.render_stateful_widget(
                    Scrollbar::new(ScrollbarOrientation::VerticalRight)
                        .thumb_symbol("█")
                        .track_symbol(Some("│")),
                    detail_chunks[1],
                    &mut scrollbar_state,
                );
            }
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let layout = App::layout(terminal_area(terminal)?);
                    let detail_lines =
                        route_detail::detail_lines(app.selected_route(), &app.request);

                    if handle_key(app, key.code, &layout, &detail_lines)? {
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn handle_key(
    app: &mut App,
    code: KeyCode,
    layout: &LayoutRects,
    detail_lines: &[Line],
) -> io::Result<bool> {
    match code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Esc => match app.focus {
            Focus::Search => {
                app.search.clear();
                app.apply_search();
                app.focus = Focus::Routes;
            }
            _ => return Ok(true),
        },
        KeyCode::Tab => app.cycle_focus(),
        KeyCode::Char('/') => app.focus = Focus::Search,
        KeyCode::Enter => {
            if app.focus != Focus::Search {
                app.send_request();
            }
        }
        KeyCode::Backspace if app.focus == Focus::Search => {
            app.search.pop();
            app.apply_search();
        }
        KeyCode::Char(c) if app.focus == Focus::Search && !c.is_control() => {
            app.search.push(c);
            app.apply_search();
        }
        KeyCode::Down | KeyCode::Char('j') => match app.focus {
            Focus::Search => app.focus = Focus::Routes,
            Focus::Routes => {
                if !app.filtered.is_empty() {
                    let n = app.filtered.len().saturating_sub(1);
                    let i = app.list_state.selected().unwrap_or(0);
                    app.list_state.select(Some((i + 1).min(n)));
                    app.on_selection_change();
                }
            }
            Focus::Detail => app.scroll_detail(1, layout, detail_lines),
        },
        KeyCode::Up | KeyCode::Char('k') => match app.focus {
            Focus::Routes => {
                if !app.filtered.is_empty() {
                    let i = app.list_state.selected().unwrap_or(0);
                    app.list_state.select(Some(i.saturating_sub(1)));
                    app.on_selection_change();
                }
            }
            Focus::Detail => app.scroll_detail(-1, layout, detail_lines),
            Focus::Search => {}
        },
        KeyCode::PageDown => {
            if app.focus == Focus::Detail {
                app.scroll_detail_page(false, layout, detail_lines);
            }
        }
        KeyCode::PageUp => {
            if app.focus == Focus::Detail {
                app.scroll_detail_page(true, layout, detail_lines);
            }
        }
        KeyCode::Home | KeyCode::Char('g') => {
            if app.focus == Focus::Detail {
                app.detail_scroll = 0;
            }
        }
        KeyCode::End | KeyCode::Char('G') => {
            if app.focus == Focus::Detail {
                let total = route_detail::line_count(detail_lines, layout.detail_inner_width);
                let max_scroll = total.saturating_sub(layout.detail_inner_height);
                app.detail_scroll = max_scroll;
            }
        }
        _ => {}
    }
    Ok(false)
}
