mod error;
mod log_parser;
mod router_parser;
mod ui;

use std::path::PathBuf;
use std::process::ExitCode;

use rusqlite::Connection;

use error::AppError;

const DATABASE_PATH: &str = "amon.db";
const API_PROJECT_ROOT_ENV: &str = "API_PROJECT_ROOT";

fn main() -> ExitCode {
    let _ = dotenvy::dotenv();

    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), AppError> {
    let project_root = read_project_root()?;
    let conn = open_database(DATABASE_PATH)?;
    ensure_schema(&conn)?;

    let routes = router_parser::find_all_routes(&project_root, &conn);
    ui::run(routes)?;
    Ok(())
}

fn read_project_root() -> Result<PathBuf, AppError> {
    let raw = std::env::var(API_PROJECT_ROOT_ENV).map_err(|_| {
        AppError::Config(format!(
            "{API_PROJECT_ROOT_ENV} is not set. Add it to .env or your shell environment."
        ))
    })?;
    let path = PathBuf::from(raw);
    if !path.is_dir() {
        return Err(AppError::Config(format!(
            "{API_PROJECT_ROOT_ENV} does not point to a directory: {}",
            path.display()
        )));
    }
    Ok(path)
}

fn open_database(path: &str) -> Result<Connection, AppError> {
    let conn = Connection::open(path)?;
    Ok(conn)
}

fn ensure_schema(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS routes (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            summary TEXT,
            path TEXT,
            method TEXT,
            query_params TEXT,
            body_params TEXT
         );
         CREATE TABLE IF NOT EXISTS controllers (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            name TEXT
         );",
    )?;
    Ok(())
}
