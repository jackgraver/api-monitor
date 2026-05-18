use std::io::{self, stdout, Stdout};

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::style::{Color, Style};
use ratatui::widgets::{Block, Borders, List, ListState};
use ratatui::Terminal;

use crate::router_parser::Route;

use super::route_list;

pub fn run(routes: Vec<Route>) -> io::Result<()> {
    enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let app_result = run_loop(&mut terminal, routes);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;

    app_result
}

fn run_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    routes: Vec<Route>,
) -> io::Result<()> {
    let n = routes.len().saturating_sub(1);
    let mut list_state = ListState::default();
    if !routes.is_empty() {
        list_state.select(Some(0));
    }

    loop {
        terminal.draw(|f| {
            let area = f.area();
            let items = route_list::list_items(&routes);
            let list = List::new(items)
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title(" Routes (q or Esc to quit) "),
                )
                .highlight_style(Style::default().bg(Color::DarkGray));
            f.render_stateful_widget(list, area, &mut list_state);
        })?;

        if event::poll(std::time::Duration::from_millis(250))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        KeyCode::Down | KeyCode::Char('j') => {
                            if !routes.is_empty() {
                                let i = list_state.selected().unwrap_or(0);
                                list_state.select(Some((i + 1).min(n)));
                            }
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if !routes.is_empty() {
                                let i = list_state.selected().unwrap_or(0);
                                list_state.select(Some(i.saturating_sub(1)));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
    }
}
