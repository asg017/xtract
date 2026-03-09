use std::path::Path;

use rusqlite::Connection;
use sha2::{Digest, Sha256};

use crate::commands::extract::ExtractResult;

pub struct InsertOpts<'a> {
    pub input_file: &'a Path,
    pub page: Option<u32>,
    pub page_count: Option<u32>,
    pub model: &'a str,
    pub classifier_data: Option<&'a str>,
}

fn sha256_hex(path: &Path) -> anyhow::Result<String> {
    let bytes = std::fs::read(path)?;
    let hash = Sha256::digest(&bytes);
    Ok(format!("{hash:x}"))
}

/// Open (or create) the database with WAL mode and create tables.
pub fn open_db(db_path: &Path) -> anyhow::Result<Connection> {
    let conn = Connection::open(db_path)?;
    conn.execute_batch("PRAGMA journal_mode=WAL;")?;
    create_tables(&conn)?;
    Ok(conn)
}

fn upsert_document(conn: &Connection, opts: &InsertOpts) -> anyhow::Result<i64> {
    let sha256 = sha256_hex(opts.input_file)?;
    let filename = opts
        .input_file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    conn.execute(
        "INSERT INTO documents (sha256, filename, page_count) VALUES (?1, ?2, ?3)
         ON CONFLICT(sha256) DO UPDATE SET filename = ?2, page_count = COALESCE(?3, page_count)",
        rusqlite::params![sha256, filename, opts.page_count],
    )?;
    let doc_id: i64 = conn.query_row(
        "SELECT id FROM documents WHERE sha256 = ?1",
        [&sha256],
        |row| row.get(0),
    )?;
    Ok(doc_id)
}

/// Insert a single extraction result. Each call is its own transaction
/// (SQLite autocommit), so the row is visible immediately.
pub fn insert(conn: &Connection, result: &ExtractResult, opts: &InsertOpts) -> anyhow::Result<()> {
    let document_id = upsert_document(conn, opts)?;

    let schema_json = serde_json::to_string(&result.json_schema)?;
    let schema_name = result
        .json_schema
        .get("title")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string());
    conn.execute(
        "INSERT OR IGNORE INTO schemas (name, schema) VALUES (?1, ?2)",
        rusqlite::params![schema_name, schema_json],
    )?;
    let schema_id: i64 = conn.query_row(
        "SELECT id FROM schemas WHERE schema = ?1",
        [&schema_json],
        |row| row.get(0),
    )?;

    conn.execute(
        "INSERT OR IGNORE INTO prompts (prompt) VALUES (?1)",
        [&result.prompt],
    )?;
    let prompt_id: i64 = conn.query_row(
        "SELECT id FROM prompts WHERE prompt = ?1",
        [&result.prompt],
        |row| row.get(0),
    )?;

    let image_sha256 = {
        let hash = Sha256::digest(&result.image_bytes);
        format!("{hash:x}")
    };
    conn.execute(
        "INSERT INTO images (sha256, mime_type, data) VALUES (?1, ?2, ?3)",
        rusqlite::params![image_sha256, result.image_mime, result.image_bytes],
    )?;
    let image_id = conn.last_insert_rowid();

    let page = opts.page.unwrap_or(0) as i64;
    conn.execute(
        "INSERT INTO extractions (document_id, page, model, schema_id, prompt_id, image_id, created_at, classifier_data, data, started_at, finished_at, elapsed_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'), ?7, ?8, ?9, ?10, ?11)",
        rusqlite::params![
            document_id,
            page,
            opts.model,
            schema_id,
            prompt_id,
            image_id,
            opts.classifier_data,
            result.data,
            result.timing.started_at,
            result.timing.finished_at,
            result.timing.elapsed_ms,
        ],
    )?;

    Ok(())
}

pub struct ErrorOpts<'a> {
    pub input_file: &'a Path,
    pub page: Option<u32>,
    pub model: &'a str,
    pub error: &'a str,
}

/// Insert an extraction error into the errors table.
pub fn insert_error(conn: &Connection, opts: &ErrorOpts) -> anyhow::Result<()> {
    let filename = opts
        .input_file
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");

    conn.execute(
        "INSERT INTO errors (filename, page, model, error, created_at)
         VALUES (?1, ?2, ?3, ?4, datetime('now'))",
        rusqlite::params![filename, opts.page, opts.model, opts.error],
    )?;

    Ok(())
}

pub struct ExistsCheck<'a> {
    pub input_file: &'a Path,
    pub page: Option<u32>,
    pub page_count: Option<u32>,
    pub model: &'a str,
    pub schema_json: &'a str,
    pub prompt: &'a str,
}

/// Returns true if an extraction already exists for this exact combination.
pub fn extraction_exists(conn: &Connection, check: &ExistsCheck) -> anyhow::Result<bool> {
    let document_id = upsert_document(
        conn,
        &InsertOpts {
            input_file: check.input_file,
            page: check.page,
            page_count: check.page_count,
            model: check.model,
            classifier_data: None,
        },
    )?;

    let schema_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM schemas WHERE schema = ?1",
            [check.schema_json],
            |row| row.get(0),
        )
        .ok();

    let prompt_id: Option<i64> = conn
        .query_row(
            "SELECT id FROM prompts WHERE prompt = ?1",
            [check.prompt],
            |row| row.get(0),
        )
        .ok();

    let (Some(schema_id), Some(prompt_id)) = (schema_id, prompt_id) else {
        return Ok(false);
    };

    let page = check.page.unwrap_or(0) as i64;
    let exists: bool = conn.query_row(
        "SELECT 1 FROM extractions WHERE document_id = ?1 AND page = ?2 AND model = ?3 AND schema_id = ?4 AND prompt_id = ?5 LIMIT 1",
        rusqlite::params![document_id, page, check.model, schema_id, prompt_id],
        |_| Ok(true),
    ).unwrap_or(false);

    Ok(exists)
}

/// Returns true if any extraction exists for this document + page + model,
/// regardless of schema/prompt. Used in classifier mode to skip both
/// classification and extraction when a result already exists.
pub fn extraction_exists_any(conn: &Connection, input_file: &Path, page: Option<u32>, page_count: Option<u32>, model: &str) -> anyhow::Result<bool> {
    let document_id = upsert_document(
        conn,
        &InsertOpts {
            input_file,
            page,
            page_count,
            model,
            classifier_data: None,
        },
    )?;

    let page = page.unwrap_or(0) as i64;
    let exists: bool = conn.query_row(
        "SELECT 1 FROM extractions WHERE document_id = ?1 AND page = ?2 AND model = ?3 LIMIT 1",
        rusqlite::params![document_id, page, model],
        |_| Ok(true),
    ).unwrap_or(false);

    Ok(exists)
}

fn create_tables(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS documents (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            sha256      TEXT NOT NULL UNIQUE,
            filename    TEXT NOT NULL,
            page_count  INTEGER,
            created_at  TEXT NOT NULL DEFAULT (datetime('now'))
        );

        CREATE TABLE IF NOT EXISTS schemas (
            id     INTEGER PRIMARY KEY AUTOINCREMENT,
            name   TEXT,
            schema TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS prompts (
            id     INTEGER PRIMARY KEY AUTOINCREMENT,
            prompt TEXT NOT NULL UNIQUE
        );

        CREATE TABLE IF NOT EXISTS images (
            id        INTEGER PRIMARY KEY AUTOINCREMENT,
            sha256    TEXT NOT NULL,
            mime_type TEXT NOT NULL,
            data      BLOB NOT NULL
        );

        CREATE TABLE IF NOT EXISTS extractions (
            id              INTEGER PRIMARY KEY AUTOINCREMENT,
            document_id     INTEGER NOT NULL REFERENCES documents(id),
            page            INTEGER,
            model           TEXT NOT NULL,
            schema_id       INTEGER REFERENCES schemas(id),
            prompt_id       INTEGER REFERENCES prompts(id),
            image_id        INTEGER REFERENCES images(id),
            created_at      TEXT NOT NULL DEFAULT (datetime('now')),
            classifier_data TEXT,
            data            TEXT NOT NULL,
            started_at      TEXT,
            finished_at     TEXT,
            elapsed_ms      INTEGER
        );

        CREATE UNIQUE INDEX IF NOT EXISTS idx_extractions_dedup
            ON extractions(document_id, page, model, schema_id, prompt_id);

        CREATE TABLE IF NOT EXISTS errors (
            id         INTEGER PRIMARY KEY AUTOINCREMENT,
            filename   TEXT NOT NULL,
            page       INTEGER,
            model      TEXT NOT NULL,
            error      TEXT NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;
    Ok(())
}
