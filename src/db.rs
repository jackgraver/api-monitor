use rusqlite::Connection;

use crate::error::AppError;

pub const DEFAULT_APP_SLUG: &str = "simpletracker";

pub struct AppInfo {
    pub slug: String,
    pub display_name: String,
}

pub fn ensure_schema(conn: &Connection) -> Result<(), AppError> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS apps (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            slug TEXT NOT NULL UNIQUE,
            display_name TEXT
         );
         CREATE TABLE IF NOT EXISTS routes (
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

    conn.execute(
        "INSERT OR IGNORE INTO apps (slug, display_name) VALUES (?1, ?2)",
        (DEFAULT_APP_SLUG, "Simple Tracker"),
    )?;

    migrate_routes_app_id(conn)?;
    Ok(())
}

fn migrate_routes_app_id(conn: &Connection) -> Result<(), AppError> {
    let has_app_id = conn
        .prepare("SELECT 1 FROM pragma_table_info('routes') WHERE name = 'app_id'")?
        .exists([])?;

    if has_app_id {
        return Ok(());
    }

    conn.execute(
        "ALTER TABLE routes ADD COLUMN app_id INTEGER REFERENCES apps(id)",
        [],
    )?;

    let app_id = default_app_id(conn)?;
    conn.execute(
        "UPDATE routes SET app_id = ?1 WHERE app_id IS NULL",
        [app_id],
    )?;

    conn.execute_batch(
        "CREATE UNIQUE INDEX IF NOT EXISTS idx_routes_app_path_method
         ON routes(app_id, path, method);",
    )?;

    Ok(())
}

pub fn default_app_id(conn: &Connection) -> Result<i64, AppError> {
    conn.query_row(
        "SELECT id FROM apps WHERE slug = ?1",
        [DEFAULT_APP_SLUG],
        |row| row.get(0),
    )
    .map_err(Into::into)
}

pub fn load_default_app(conn: &Connection) -> Result<AppInfo, AppError> {
    conn.query_row(
        "SELECT slug, display_name FROM apps WHERE slug = ?1",
        [DEFAULT_APP_SLUG],
        |row| {
            Ok(AppInfo {
                slug: row.get(0)?,
                display_name: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            })
        },
    )
    .map_err(Into::into)
}
