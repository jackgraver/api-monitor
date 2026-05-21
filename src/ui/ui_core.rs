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
    response_inner_width: u16,
    response_inner_height: u16,
}

struct App {
    routes: Vec<Route>,
    search: String,
    filtered: Vec<usize>,
    list_state: ListState,
    focus: Focus,
    response_scroll: u16,
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
            response_scroll: 0,
            request: RequestState::Idle,
            request_rx: None,
            last_selected_route: None,
        }
    }

    fn right_column_layout(area: Rect) -> [Rect; 5] {
        let chunks = Layout::default()
            .direction(Direction::Vertical)
            .constraints([
                Constraint::Percentage(22),
                Constraint::Length(1),
                Constraint::Percentage(38),
                Constraint::Length(1),
                Constraint::Min(0),
            ])
            .split(area);
        [
            chunks[0], chunks[1], chunks[2], chunks[3], chunks[4],
        ]
    }

    fn layout(area: Rect) -> LayoutRects {
        let columns = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
            .split(area);

        let right = Self::right_column_layout(columns[1]);
        let response_chunks = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Min(0), Constraint::Length(1)])
            .split(right[4]);

        LayoutRects {
            response_inner_width: response_chunks[0].width.saturating_sub(2),
            response_inner_height: response_chunks[0].height.saturating_sub(2),
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
            self.response_scroll = 0;
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
        self.response_scroll = 0;
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
                self.response_scroll = 0;
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

    fn clamp_scroll(
        scroll: &mut u16,
        lines: &[Line],
        inner_width: u16,
        inner_height: u16,
    ) {
        let total = route_detail::line_count(lines, inner_width);
        let max_scroll = total.saturating_sub(inner_height);
        *scroll = (*scroll).min(max_scroll);
    }

    fn scroll_section(
        scroll: &mut u16,
        delta: i16,
        lines: &[Line],
        inner_width: u16,
        inner_height: u16,
    ) {
        let total = route_detail::line_count(lines, inner_width);
        let max_scroll = total.saturating_sub(inner_height);

        if delta < 0 {
            *scroll = (*scroll).saturating_sub((-delta) as u16);
        } else {
            *scroll = ((*scroll) + delta as u16).min(max_scroll);
        }
    }

    fn scroll_section_page(
        scroll: &mut u16,
        up: bool,
        lines: &[Line],
        inner_width: u16,
        inner_height: u16,
    ) {
        let page = inner_height.saturating_sub(1).max(1) as i16;
        Self::scroll_section(
            scroll,
            if up { -page } else { page },
            lines,
            inner_width,
            inner_height,
        );
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

fn detail_border_style(focus: Focus) -> Style {
    if focus == Focus::Detail {
        Style::default().fg(Color::Yellow)
    } else {
        Style::default()
    }
}

fn horizontal_divider(width: u16) -> Paragraph<'static> {
    let n = width.max(1) as usize;
    let line = "─".repeat(n);
    Paragraph::new(Line::from(Span::styled(
        line,
        Style::default().fg(Color::DarkGray),
    )))
}

fn render_scrollbar(
    f: &mut ratatui::Frame,
    area: Rect,
    lines: &[Line],
    inner_width: u16,
    inner_height: u16,
    scroll: u16,
) {
    let total_lines = route_detail::line_count(lines, inner_width) as usize;
    let visible = inner_height as usize;
    if total_lines > visible {
        let mut scrollbar_state =
            ScrollbarState::new(total_lines).position(scroll as usize);
        f.render_stateful_widget(
            Scrollbar::new(ScrollbarOrientation::VerticalRight)
                .thumb_symbol("█")
                .track_symbol(Some("│")),
            area,
            &mut scrollbar_state,
        );
    }
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    loop {
        app.poll_request();

        let area = terminal_area(terminal)?;
        let layout = App::layout(area);
        let response_lines = route_detail::response_lines(&app.request);
        App::clamp_scroll(
            &mut app.response_scroll,
            &response_lines,
            layout.response_inner_width,
            layout.response_inner_height,
        );

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

            let route = app.selected_route();
            let summary_lines = route_detail::summary_lines(route);
            let params_lines = route_detail::params_lines(route);
            let response_lines = route_detail::response_lines(&app.request);
            let detail_style = detail_border_style(app.focus);

            let right = App::right_column_layout(columns[1]);

            let summary = route_detail::section_paragraph(summary_lines, 0).block(
                Block::default()
                    .borders(Borders::ALL)
                    .title(" Summary ")
                    .border_style(detail_style),
            );
            f.render_widget(summary, right[0]);
            f.render_widget(horizontal_divider(right[1].width), right[1]);

            let params = route_detail::section_paragraph(params_lines.clone(), 0)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Parameters ")
                        .border_style(detail_style),
                );
            f.render_widget(params, right[2]);
            f.render_widget(horizontal_divider(right[3].width), right[3]);

            let response_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Min(0), Constraint::Length(1)])
                .split(right[4]);

            let response =
                route_detail::section_paragraph(response_lines.clone(), app.response_scroll)
                    .block(
                        Block::default()
                            .borders(Borders::ALL)
                            .title(" Response (Enter send, PgUp/Dn scroll) ")
                            .border_style(detail_style),
                    );
            f.render_widget(response, response_chunks[0]);
            render_scrollbar(
                f,
                response_chunks[1],
                &response_lines,
                layout.response_inner_width,
                layout.response_inner_height,
                app.response_scroll,
            );
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    let layout = App::layout(terminal_area(terminal)?);
                    let response_lines = route_detail::response_lines(&app.request);

                    if handle_key(app, key.code, &layout, &response_lines)? {
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
    response_lines: &[Line],
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
            Focus::Detail => App::scroll_section(
                &mut app.response_scroll,
                1,
                response_lines,
                layout.response_inner_width,
                layout.response_inner_height,
            ),
        },
        KeyCode::Up | KeyCode::Char('k') => match app.focus {
            Focus::Routes => {
                if !app.filtered.is_empty() {
                    let i = app.list_state.selected().unwrap_or(0);
                    app.list_state.select(Some(i.saturating_sub(1)));
                    app.on_selection_change();
                }
            }
            Focus::Detail => App::scroll_section(
                &mut app.response_scroll,
                -1,
                response_lines,
                layout.response_inner_width,
                layout.response_inner_height,
            ),
            Focus::Search => {}
        },
        KeyCode::PageDown => {
            if app.focus == Focus::Detail {
                App::scroll_section_page(
                    &mut app.response_scroll,
                    false,
                    response_lines,
                    layout.response_inner_width,
                    layout.response_inner_height,
                );
            }
        }
        KeyCode::PageUp => {
            if app.focus == Focus::Detail {
                App::scroll_section_page(
                    &mut app.response_scroll,
                    true,
                    response_lines,
                    layout.response_inner_width,
                    layout.response_inner_height,
                );
            }
        }
        KeyCode::Home | KeyCode::Char('g') => {
            if app.focus == Focus::Detail {
                app.response_scroll = 0;
            }
        }
        KeyCode::End | KeyCode::Char('G') => {
            if app.focus == Focus::Detail {
                let total =
                    route_detail::line_count(response_lines, layout.response_inner_width);
                let max_scroll = total.saturating_sub(layout.response_inner_height);
                app.response_scroll = max_scroll;
            }
        }
        _ => {}
    }
    Ok(false)
}
