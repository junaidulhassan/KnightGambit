use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::{Connection, OptionalExtension, Row, params};

use super::model::{
    GameRecord, GameReviewSummary, MoveReviewRow, Outcome, StoredReview, color_as_db, color_from_db,
};

/// Schema migrations. Entry `n` upgrades a database from `user_version == n`
/// to `n + 1`. Existing entries must never be edited; append new ones.
const MIGRATIONS: &[&str] = &[
    // v1: games, engine reviews and metadata.
    r#"
    CREATE TABLE games (
        id             INTEGER PRIMARY KEY AUTOINCREMENT,
        uid            TEXT    NOT NULL UNIQUE,
        played_at      INTEGER NOT NULL,
        duration_secs  INTEGER NOT NULL DEFAULT 0,
        user_color     TEXT    NOT NULL CHECK (user_color IN ('white', 'black')),
        opponent_name  TEXT    NOT NULL,
        opponent_level TEXT    NOT NULL,
        opponent_elo   INTEGER,
        result         TEXT    NOT NULL CHECK (result IN ('win', 'loss', 'draw')),
        score          TEXT    NOT NULL,
        termination    TEXT    NOT NULL,
        opening_name   TEXT,
        eco            TEXT,
        start_fen      TEXT    NOT NULL DEFAULT '',
        pgn            TEXT    NOT NULL,
        moves_uci      TEXT    NOT NULL,
        ply_count      INTEGER NOT NULL,
        time_control   TEXT,
        endgame_ply    INTEGER,
        white_name     TEXT    NOT NULL,
        black_name     TEXT    NOT NULL,
        created_at     INTEGER NOT NULL DEFAULT (strftime('%s', 'now'))
    );
    CREATE UNIQUE INDEX idx_games_content ON games (played_at, user_color, start_fen, moves_uci);
    CREATE INDEX idx_games_played_at ON games (played_at);
    CREATE INDEX idx_games_opening ON games (opening_name);

    CREATE TABLE game_reviews (
        game_id        INTEGER PRIMARY KEY REFERENCES games (id) ON DELETE CASCADE,
        engine         TEXT    NOT NULL,
        reviewed_at    INTEGER NOT NULL,
        movetime_ms    INTEGER NOT NULL DEFAULT 0,
        white_accuracy REAL    NOT NULL,
        black_accuracy REAL    NOT NULL
    );

    CREATE TABLE move_reviews (
        game_id  INTEGER NOT NULL REFERENCES games (id) ON DELETE CASCADE,
        ply      INTEGER NOT NULL,
        mover    TEXT    NOT NULL CHECK (mover IN ('white', 'black')),
        class    TEXT    NOT NULL,
        loss     REAL    NOT NULL,
        accuracy REAL    NOT NULL,
        san      TEXT    NOT NULL,
        best_san TEXT,
        PRIMARY KEY (game_id, ply)
    ) WITHOUT ROWID;

    CREATE TABLE meta (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    ) WITHOUT ROWID;
    "#,
];

pub const SCHEMA_VERSION: i64 = MIGRATIONS.len() as i64;

#[derive(Debug)]
pub enum HistoryError {
    Sqlite(rusqlite::Error),
    Io(std::io::Error),
    NewerSchema(i64),
    Poisoned,
}

impl fmt::Display for HistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HistoryError::Sqlite(error) => write!(f, "database error: {error}"),
            HistoryError::Io(error) => write!(f, "file error: {error}"),
            HistoryError::NewerSchema(version) => write!(
                f,
                "the game history was written by a newer KnightGambit (schema {version}); update the application"
            ),
            HistoryError::Poisoned => write!(f, "the game history is temporarily unavailable"),
        }
    }
}

impl std::error::Error for HistoryError {}

impl From<rusqlite::Error> for HistoryError {
    fn from(error: rusqlite::Error) -> Self {
        HistoryError::Sqlite(error)
    }
}

impl From<std::io::Error> for HistoryError {
    fn from(error: std::io::Error) -> Self {
        HistoryError::Io(error)
    }
}

pub type Result<T> = std::result::Result<T, HistoryError>;

/// What happened when the database was opened.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OpenReport {
    /// The database file did not exist and was created.
    pub created: bool,
    /// The file failed its integrity check and was moved here (never deleted).
    pub quarantined: Option<PathBuf>,
}

struct Inner {
    conn: Mutex<Connection>,
    path: Option<PathBuf>,
}

/// Thread-safe handle to the local game-history database.
#[derive(Clone)]
pub struct HistoryStore {
    inner: Arc<Inner>,
}

fn configure(conn: &Connection) -> Result<()> {
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "foreign_keys", true)?;
    let _ = conn.pragma_update(None, "journal_mode", "WAL");
    conn.pragma_update(None, "synchronous", "NORMAL")?;
    Ok(())
}

fn migrate(conn: &mut Connection) -> Result<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version > SCHEMA_VERSION {
        return Err(HistoryError::NewerSchema(version));
    }
    for (index, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
        let transaction = conn.transaction()?;
        transaction.execute_batch(sql)?;
        transaction.pragma_update(None, "user_version", index as i64 + 1)?;
        transaction.commit()?;
    }
    conn.execute(
        "INSERT OR IGNORE INTO meta (key, value) VALUES ('created_at', strftime('%s', 'now'))",
        [],
    )?;
    Ok(())
}

fn integrity_ok(conn: &Connection) -> bool {
    conn.query_row("PRAGMA quick_check", [], |row| row.get::<_, String>(0))
        .map(|answer| answer == "ok")
        .unwrap_or(false)
}

fn quarantine(path: &Path) -> Result<PathBuf> {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0);
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let target = path.with_file_name(format!("{name}.corrupt-{stamp}"));
    std::fs::rename(path, &target)?;
    for suffix in ["-wal", "-shm"] {
        let side = path.with_file_name(format!("{name}{suffix}"));
        if side.exists() {
            let _ = std::fs::rename(&side, path.with_file_name(format!("{name}.corrupt-{stamp}{suffix}")));
        }
    }
    Ok(target)
}

impl HistoryStore {
    /// Opens (creating on first launch) the database at `path`.
    pub fn open(path: &Path) -> Result<(Self, OpenReport)> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut report = OpenReport {
            created: !path.exists(),
            quarantined: None,
        };
        let mut conn = match Connection::open(path) {
            Ok(conn) if path.metadata().map(|m| m.len() == 0).unwrap_or(true) || integrity_ok(&conn) => conn,
            _ => {
                report.quarantined = Some(quarantine(path)?);
                report.created = true;
                Connection::open(path)?
            }
        };
        configure(&conn)?;
        migrate(&mut conn)?;
        Ok((
            Self {
                inner: Arc::new(Inner {
                    conn: Mutex::new(conn),
                    path: Some(path.to_owned()),
                }),
            },
            report,
        ))
    }

    /// A non-persistent store, used as a fallback when the file cannot be opened.
    pub fn in_memory() -> Result<Self> {
        let mut conn = Connection::open_in_memory()?;
        configure(&conn)?;
        migrate(&mut conn)?;
        Ok(Self {
            inner: Arc::new(Inner {
                conn: Mutex::new(conn),
                path: None,
            }),
        })
    }

    pub fn path(&self) -> Option<&Path> {
        self.inner.path.as_deref()
    }

    pub fn is_persistent(&self) -> bool {
        self.inner.path.is_some()
    }

    fn lock(&self) -> Result<MutexGuard<'_, Connection>> {
        self.inner.conn.lock().map_err(|_| HistoryError::Poisoned)
    }

    pub fn schema_version(&self) -> Result<i64> {
        Ok(self.lock()?.query_row("PRAGMA user_version", [], |row| row.get(0))?)
    }

    /// Stores a completed game. Returns `None` when an identical game (same
    /// session or same content) is already stored; stored games are never
    /// overwritten.
    pub fn insert_game(&self, game: &GameRecord) -> Result<Option<i64>> {
        let conn = self.lock()?;
        let changed = conn.execute(
            "INSERT OR IGNORE INTO games (
                uid, played_at, duration_secs, user_color, opponent_name, opponent_level,
                opponent_elo, result, score, termination, opening_name, eco, start_fen, pgn,
                moves_uci, ply_count, time_control, endgame_ply, white_name, black_name
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20)",
            params![
                game.uid,
                game.played_at,
                game.duration_secs,
                color_as_db(game.user_color),
                game.opponent_name,
                game.opponent_level,
                game.opponent_elo,
                game.outcome.as_db(),
                game.score,
                game.termination,
                game.opening_name,
                game.eco,
                game.start_fen,
                game.pgn,
                game.moves_uci,
                game.ply_count,
                game.time_control,
                game.endgame_ply,
                game.white_name,
                game.black_name,
            ],
        )?;
        Ok((changed > 0).then(|| conn.last_insert_rowid()))
    }

    /// Withdraws the record of a play session whose game was resumed by a
    /// take-back after it had ended, so it no longer counts as completed.
    pub fn retract_session(&self, uid: &str) -> Result<bool> {
        let conn = self.lock()?;
        Ok(conn.execute("DELETE FROM games WHERE uid = ?1", params![uid])? > 0)
    }

    fn row_to_game(row: &Row<'_>, with_moves: bool) -> rusqlite::Result<GameRecord> {
        let outcome: String = row.get("result")?;
        let color: String = row.get("user_color")?;
        Ok(GameRecord {
            id: row.get("id")?,
            uid: row.get("uid")?,
            played_at: row.get("played_at")?,
            duration_secs: row.get("duration_secs")?,
            user_color: color_from_db(&color),
            opponent_name: row.get("opponent_name")?,
            opponent_level: row.get("opponent_level")?,
            opponent_elo: row.get("opponent_elo")?,
            outcome: Outcome::from_db(&outcome).unwrap_or(Outcome::Draw),
            score: row.get("score")?,
            termination: row.get("termination")?,
            opening_name: row.get("opening_name")?,
            eco: row.get("eco")?,
            start_fen: row.get("start_fen")?,
            pgn: if with_moves { row.get("pgn")? } else { String::new() },
            moves_uci: if with_moves { row.get("moves_uci")? } else { String::new() },
            ply_count: row.get("ply_count")?,
            time_control: row.get("time_control")?,
            endgame_ply: row.get("endgame_ply")?,
            white_name: row.get("white_name")?,
            black_name: row.get("black_name")?,
            analyzed: row.get::<_, i64>("analyzed")? != 0,
        })
    }

    /// All games, newest first, without the PGN and move text.
    pub fn list_games(&self) -> Result<Vec<GameRecord>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT g.id, g.uid, g.played_at, g.duration_secs, g.user_color, g.opponent_name,
                    g.opponent_level, g.opponent_elo, g.result, g.score, g.termination,
                    g.opening_name, g.eco, g.start_fen, g.ply_count, g.time_control,
                    g.endgame_ply, g.white_name, g.black_name,
                    EXISTS (SELECT 1 FROM game_reviews r WHERE r.game_id = g.id) AS analyzed
             FROM games g ORDER BY g.played_at DESC, g.id DESC",
        )?;
        let rows = statement.query_map([], |row| Self::row_to_game(row, false))?;
        Ok(rows.collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn get_game(&self, id: i64) -> Result<Option<GameRecord>> {
        let conn = self.lock()?;
        Ok(conn
            .query_row(
                "SELECT g.*, EXISTS (SELECT 1 FROM game_reviews r WHERE r.game_id = g.id) AS analyzed
                 FROM games g WHERE g.id = ?1",
                params![id],
                |row| Self::row_to_game(row, true),
            )
            .optional()?)
    }

    pub fn game_id_for_uid(&self, uid: &str) -> Result<Option<i64>> {
        let conn = self.lock()?;
        Ok(conn
            .query_row("SELECT id FROM games WHERE uid = ?1", params![uid], |row| row.get(0))
            .optional()?)
    }

    pub fn game_count(&self) -> Result<i64> {
        Ok(self.lock()?.query_row("SELECT COUNT(*) FROM games", [], |row| row.get(0))?)
    }

    /// Stores (or refreshes) the engine review of a game. Only review data is
    /// replaced; the game record itself is never modified.
    pub fn save_review(&self, game_id: i64, summary: &GameReviewSummary, moves: &[MoveReviewRow]) -> Result<()> {
        let mut conn = self.lock()?;
        let transaction = conn.transaction()?;
        let exists: bool = transaction
            .query_row("SELECT 1 FROM games WHERE id = ?1", params![game_id], |_| Ok(true))
            .optional()?
            .unwrap_or(false);
        if !exists {
            return Ok(());
        }
        transaction.execute("DELETE FROM move_reviews WHERE game_id = ?1", params![game_id])?;
        transaction.execute(
            "INSERT INTO game_reviews (game_id, engine, reviewed_at, movetime_ms, white_accuracy, black_accuracy)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT(game_id) DO UPDATE SET engine = excluded.engine, reviewed_at = excluded.reviewed_at,
                movetime_ms = excluded.movetime_ms, white_accuracy = excluded.white_accuracy,
                black_accuracy = excluded.black_accuracy",
            params![
                game_id,
                summary.engine,
                summary.reviewed_at,
                summary.movetime_ms,
                summary.white_accuracy,
                summary.black_accuracy
            ],
        )?;
        {
            let mut insert = transaction.prepare(
                "INSERT INTO move_reviews (game_id, ply, mover, class, loss, accuracy, san, best_san)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            )?;
            for row in moves {
                insert.execute(params![
                    game_id,
                    row.ply,
                    color_as_db(row.mover),
                    row.class,
                    row.loss,
                    row.accuracy,
                    row.san,
                    row.best_san
                ])?;
            }
        }
        transaction.commit()?;
        Ok(())
    }

    fn row_to_move(row: &Row<'_>) -> rusqlite::Result<MoveReviewRow> {
        let mover: String = row.get(2)?;
        Ok(MoveReviewRow {
            game_id: row.get(0)?,
            ply: row.get(1)?,
            mover: color_from_db(&mover),
            class: row.get(3)?,
            loss: row.get(4)?,
            accuracy: row.get(5)?,
            san: row.get(6)?,
            best_san: row.get(7)?,
        })
    }

    pub fn load_review(&self, game_id: i64) -> Result<Option<StoredReview>> {
        let conn = self.lock()?;
        let summary = conn
            .query_row(
                "SELECT engine, reviewed_at, movetime_ms, white_accuracy, black_accuracy
                 FROM game_reviews WHERE game_id = ?1",
                params![game_id],
                |row| {
                    Ok(GameReviewSummary {
                        engine: row.get(0)?,
                        reviewed_at: row.get(1)?,
                        movetime_ms: row.get(2)?,
                        white_accuracy: row.get(3)?,
                        black_accuracy: row.get(4)?,
                    })
                },
            )
            .optional()?;
        let Some(summary) = summary else { return Ok(None) };
        let mut statement = conn.prepare(
            "SELECT game_id, ply, mover, class, loss, accuracy, san, best_san
             FROM move_reviews WHERE game_id = ?1 ORDER BY ply",
        )?;
        let moves = statement
            .query_map(params![game_id], Self::row_to_move)?
            .collect::<rusqlite::Result<Vec<_>>>()?;
        Ok(Some(StoredReview { summary, moves }))
    }

    pub fn load_all_reviews(&self) -> Result<Vec<MoveReviewRow>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT game_id, ply, mover, class, loss, accuracy, san, best_san
             FROM move_reviews ORDER BY game_id, ply",
        )?;
        Ok(statement
            .query_map([], Self::row_to_move)?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    pub fn list_review_summaries(&self) -> Result<Vec<(i64, GameReviewSummary)>> {
        let conn = self.lock()?;
        let mut statement = conn.prepare(
            "SELECT game_id, engine, reviewed_at, movetime_ms, white_accuracy, black_accuracy FROM game_reviews",
        )?;
        Ok(statement
            .query_map([], |row| {
                Ok((
                    row.get(0)?,
                    GameReviewSummary {
                        engine: row.get(1)?,
                        reviewed_at: row.get(2)?,
                        movetime_ms: row.get(3)?,
                        white_accuracy: row.get(4)?,
                        black_accuracy: row.get(5)?,
                    },
                ))
            })?
            .collect::<rusqlite::Result<Vec<_>>>()?)
    }

    /// Concatenated PGN of the requested games (all games when `ids` is `None`),
    /// oldest first.
    pub fn export_pgn(&self, ids: Option<&[i64]>) -> Result<String> {
        let conn = self.lock()?;
        let mut statement = conn.prepare("SELECT id, pgn FROM games ORDER BY played_at, id")?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))?;
        let mut output = String::new();
        for row in rows {
            let (id, pgn) = row?;
            if ids.is_none_or(|ids| ids.contains(&id)) {
                output.push_str(pgn.trim_end());
                output.push_str("\n\n");
            }
        }
        Ok(output)
    }

    /// Writes a consistent, self-contained copy of the database to `destination`.
    pub fn backup_to(&self, destination: &Path) -> Result<()> {
        let temporary = destination.with_extension("kgbackup-partial");
        let _ = std::fs::remove_file(&temporary);
        {
            let conn = self.lock()?;
            conn.execute("VACUUM INTO ?1", params![temporary.to_string_lossy()])?;
        }
        std::fs::rename(&temporary, destination)?;
        Ok(())
    }
}
