use std::io::{self, stdout, Stdout};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::{Constraint, Direction, Layout};
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, List, ListState};
use ratatui::Terminal;

use crate::router_parser::Route;

use super::route_detail;
use super::route_list;
use super::route_request::{self, RequestOutcome, RequestState};

struct App {
    routes: Vec<Route>,
    list_state: ListState,
    request: RequestState,
    request_rx: Option<Receiver<RequestOutcome>>,
    last_selected: Option<usize>,
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
        let mut list_state = ListState::default();
        let last_selected = if routes.is_empty() {
            None
        } else {
            list_state.select(Some(0));
            Some(0)
        };
        Self {
            routes,
            list_state,
            request: RequestState::Idle,
            request_rx: None,
            last_selected,
        }
    }

    fn selected_route(&self) -> Option<&Route> {
        self.list_state
            .selected()
            .and_then(|i| self.routes.get(i))
    }

    fn on_selection_change(&mut self) {
        let selected = self.list_state.selected();
        if selected != self.last_selected {
            self.last_selected = selected;
            self.request = RequestState::Idle;
            self.request_rx = None;
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
}

fn run_loop(terminal: &mut Terminal<CrosstermBackend<Stdout>>, app: &mut App) -> io::Result<()> {
    let n = app.routes.len().saturating_sub(1);

    loop {
        app.poll_request();

        terminal.draw(|f| {
            let area = f.area();
            let chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([Constraint::Percentage(38), Constraint::Percentage(62)])
                .split(area);

            let items = route_list::list_items(&app.routes);
            let list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Routes (Enter send, q quit) "),
                )
                .highlight_style(Style::default().bg(Color::DarkGray));
            f.render_stateful_widget(list, chunks[0], &mut app.list_state);

            let selected = app.selected_route();
            let detail = route_detail::detail_paragraph(selected, &app.request);
            f.render_widget(detail, chunks[1]);
        })?;

        if event::poll(std::time::Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Enter => app.send_request(),
                        KeyCode::Down | KeyCode::Char('j') => {
                            if !app.routes.is_empty() {
                                let i = app.list_state.selected().unwrap_or(0);
                                app.list_state.select(Some((i + 1).min(n)));
                                app.on_selection_change();
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if !app.routes.is_empty() {
                                let i = app.list_state.selected().unwrap_or(0);
                                app.list_state.select(Some(i.saturating_sub(1)));
                                app.on_selection_change();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
