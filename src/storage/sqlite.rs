//! SQLite adapter implementation managing internal connection lifecycle, migrations, and transactions.

use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::SystemTime;
use rusqlite::{params, Connection, OptionalExtension};

use crate::domain::manifest::{InitializationManifest, InitializationReport};
use crate::domain::types::{CollectionName, IndexVersion, JsonPointer, SchemaVersion};
use crate::error::Error;
use crate::storage::{StorageEngine, StorageTransaction};

/// Current internal storage schema version for Praxium.
pub const CURRENT_STORAGE_VERSION: u32 = 1;

/// Formats a [`SystemTime`] into a standard RFC 3339 UTC string.
pub fn system_time_to_rfc3339(time: SystemTime) -> String {
    let duration = time.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_default();
    let total_secs = duration.as_secs();

    let total_days = (total_secs / 86400) as i64;
    let day_secs = (total_secs % 86400) as u32;

    let hour = day_secs / 3600;
    let minute = (day_secs % 3600) / 60;
    let second = day_secs % 60;

    let z = total_days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}T{hour:02}:{minute:02}:{second:02}Z")
}

/// Storage representation of an existing collection definition.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct StoredCollection {
    pub name: CollectionName,
    pub created_at: String,
}

/// Storage representation of an existing schema definition.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct StoredSchema {
    pub collection_name: CollectionName,
    pub version: SchemaVersion,
    pub schema_json: String,
    pub created_at: String,
    pub is_active: bool,
}

/// Storage representation of an existing index definition.
#[allow(dead_code)]
#[derive(Debug, Clone)]
pub struct StoredIndex {
    pub collection_name: CollectionName,
    pub pointer: JsonPointer,
    pub version: IndexVersion,
    pub created_at: String,
}

/// Thread-safe SQLite storage handle encapsulating all raw database operations.
#[derive(Debug)]
pub struct SqliteStorage {
    conn: Arc<Mutex<Connection>>,
}

impl SqliteStorage {
    /// Opens or creates the SQLite database at `path` and runs internal migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let path_ref = path.as_ref();
        let mut conn = Connection::open(path_ref).map_err(|e| {
            Error::storage("SqliteStorage::open", format!("failed to open database at {:?}", path_ref), Some(Box::new(e)))
        })?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA foreign_keys = ON;
             PRAGMA busy_timeout = 5000;"
        ).map_err(|e| {
            Error::storage("SqliteStorage::open", "failed to set pragmas", Some(Box::new(e)))
        })?;

        Self::check_and_migrate(&mut conn)?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    fn check_and_migrate(conn: &mut Connection) -> Result<(), Error> {
        let table_exists: bool = conn
            .query_row(
                "SELECT 1 FROM sqlite_master WHERE type='table' AND name='_praxium_metadata'",
                [],
                |_| Ok(true),
            )
            .optional()
            .map_err(|e| Error::storage("SqliteStorage::check_and_migrate", "failed checking metadata table", Some(Box::new(e))))?
            .unwrap_or(false);

        let current_ver: Option<u32> = if table_exists {
            let ver_str: Option<String> = conn
                .query_row(
                    "SELECT value FROM _praxium_metadata WHERE key = 'storage_version'",
                    [],
                    |row| row.get(0),
                )
                .optional()
                .map_err(|e| Error::storage("SqliteStorage::check_and_migrate", "failed reading storage version", Some(Box::new(e))))?;

            if let Some(s) = ver_str {
                let parsed = s.parse::<u32>().map_err(|e| {
                    Error::storage("SqliteStorage::check_and_migrate", format!("corrupted storage version '{s}'"), Some(Box::new(e)))
                })?;
                Some(parsed)
            } else {
                None
            }
        } else {
            None
        };

        if let Some(found) = current_ver
            && found > CURRENT_STORAGE_VERSION
        {
            return Err(Error::UnsupportedVersion {
                found_version: found,
                supported_version: CURRENT_STORAGE_VERSION,
            });
        }

        let from_ver = current_ver.unwrap_or(0);
        if from_ver < CURRENT_STORAGE_VERSION {
            let tx = conn.transaction().map_err(|e| {
                Error::storage("SqliteStorage::check_and_migrate", "failed to begin migration transaction", Some(Box::new(e)))
            })?;

            if from_ver == 0 {
                tx.execute_batch(
                    "CREATE TABLE IF NOT EXISTS _praxium_metadata (
                         key TEXT PRIMARY KEY,
                         value TEXT NOT NULL
                     );
                     CREATE TABLE IF NOT EXISTS collections (
                         name TEXT PRIMARY KEY,
                         created_at TEXT NOT NULL
                     );
                     CREATE TABLE IF NOT EXISTS schemas (
                         collection_name TEXT NOT NULL,
                         version INTEGER NOT NULL,
                         schema_json TEXT NOT NULL,
                         created_at TEXT NOT NULL,
                         is_active INTEGER NOT NULL,
                         PRIMARY KEY (collection_name, version),
                         FOREIGN KEY (collection_name) REFERENCES collections(name) ON DELETE CASCADE
                     );
                     CREATE TABLE IF NOT EXISTS indexes (
                         collection_name TEXT NOT NULL,
                         pointer TEXT NOT NULL,
                         version INTEGER NOT NULL,
                         created_at TEXT NOT NULL,
                         PRIMARY KEY (collection_name, pointer),
                         FOREIGN KEY (collection_name) REFERENCES collections(name) ON DELETE CASCADE
                     );
                     CREATE TABLE IF NOT EXISTS objects (
                         id TEXT PRIMARY KEY,
                         collection_name TEXT NOT NULL,
                         revision INTEGER NOT NULL,
                         data TEXT NOT NULL,
                         created_at TEXT NOT NULL,
                         updated_at TEXT NOT NULL,
                         deleted_at TEXT,
                         FOREIGN KEY (collection_name) REFERENCES collections(name) ON DELETE RESTRICT
                     );
                     CREATE TABLE IF NOT EXISTS history (
                         id INTEGER PRIMARY KEY AUTOINCREMENT,
                         object_id TEXT NOT NULL,
                         revision INTEGER NOT NULL,
                         operation TEXT NOT NULL,
                         snapshot TEXT,
                         source TEXT,
                         tx_id TEXT,
                         created_at TEXT NOT NULL
                     );
                     INSERT OR REPLACE INTO _praxium_metadata (key, value) VALUES ('storage_version', '1');"
                ).map_err(|e| {
                    Error::storage("SqliteStorage::check_and_migrate", "failed executing v1 migration schema", Some(Box::new(e)))
                })?;
            }

            tx.commit().map_err(|e| {
                Error::storage("SqliteStorage::check_and_migrate", "failed to commit migration transaction", Some(Box::new(e)))
            })?;
        }

        Ok(())
    }

    /// Reads an active schema for a collection, if one exists.
    #[allow(dead_code)]
    pub fn get_active_schema(&self, collection: &CollectionName) -> Result<Option<StoredSchema>, Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT collection_name, version, schema_json, created_at, is_active
             FROM schemas WHERE collection_name = ?1 AND is_active = 1"
        ).map_err(|e| Error::storage("SqliteStorage::get_active_schema", "prepare failed", Some(Box::new(e))))?;

        let res = stmt.query_row(params![collection.as_str()], |row| {
            let col_name: String = row.get(0)?;
            let ver: u32 = row.get(1)?;
            let json: String = row.get(2)?;
            let created_at: String = row.get(3)?;
            let is_active: bool = row.get(4)?;
            Ok((col_name, ver, json, created_at, is_active))
        }).optional().map_err(|e| Error::storage("SqliteStorage::get_active_schema", "query error", Some(Box::new(e))))?;

        match res {
            Some((c, v, j, cat, act)) => Ok(Some(StoredSchema {
                collection_name: CollectionName::new(c)?,
                version: SchemaVersion::new(v)?,
                schema_json: j,
                created_at: cat,
                is_active: act,
            })),
            None => Ok(None),
        }
    }

    /// Reads an index for a collection and pointer, if one exists.
    #[allow(dead_code)]
    pub fn get_index(&self, collection: &CollectionName, pointer: &JsonPointer) -> Result<Option<StoredIndex>, Error> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT collection_name, pointer, version, created_at
             FROM indexes WHERE collection_name = ?1 AND pointer = ?2"
        ).map_err(|e| Error::storage("SqliteStorage::get_index", "prepare failed", Some(Box::new(e))))?;

        let res = stmt.query_row(params![collection.as_str(), pointer.as_str()], |row| {
            let col_name: String = row.get(0)?;
            let ptr: String = row.get(1)?;
            let ver: u32 = row.get(2)?;
            let created_at: String = row.get(3)?;
            Ok((col_name, ptr, ver, created_at))
        }).optional().map_err(|e| Error::storage("SqliteStorage::get_index", "query error", Some(Box::new(e))))?;

        match res {
            Some((c, p, v, cat)) => Ok(Some(StoredIndex {
                collection_name: CollectionName::new(c)?,
                pointer: JsonPointer::new(p)?,
                version: IndexVersion::new(v)?,
                created_at: cat,
            })),
            None => Ok(None),
        }
    }
}

/// SQLite transactional storage handle implementing [`StorageTransaction`].
pub struct SqliteTransaction<'a> {
    tx: rusqlite::Transaction<'a>,
}

impl<'a> StorageTransaction for SqliteTransaction<'a> {
    fn list_collections(&mut self) -> Result<Vec<CollectionName>, Error> {
        let mut stmt = self.tx.prepare("SELECT name FROM collections ORDER BY name ASC").map_err(|e| {
            Error::storage("SqliteTransaction::list_collections", "prepare failed", Some(Box::new(e)))
        })?;

        let rows = stmt.query_map([], |row| {
            let name_str: String = row.get(0)?;
            Ok(name_str)
        }).map_err(|e| {
            Error::storage("SqliteTransaction::list_collections", "query failed", Some(Box::new(e)))
        })?;

        let mut names = Vec::new();
        for r in rows {
            let name_str = r.map_err(|e| {
                Error::storage("SqliteTransaction::list_collections", "row error", Some(Box::new(e)))
            })?;
            names.push(CollectionName::new(name_str)?);
        }
        Ok(names)
    }
}

impl StorageEngine for SqliteStorage {

    fn seed_manifest(&self, manifest: &InitializationManifest) -> Result<InitializationReport, Error> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| {
            Error::storage("SqliteStorage::seed_manifest", "failed to begin transaction", Some(Box::new(e)))
        })?;

        let now = system_time_to_rfc3339(SystemTime::now());

        // 1. Query all pre-existing definitions to guarantee comprehensive preservation reporting
        let mut existing_collections = HashSet::new();
        {
            let mut stmt = tx.prepare("SELECT name FROM collections").map_err(|e| {
                Error::storage("SqliteStorage::seed_manifest", "query collections prepare failed", Some(Box::new(e)))
            })?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0)).map_err(|e| {
                Error::storage("SqliteStorage::seed_manifest", "query collections failed", Some(Box::new(e)))
            })?;
            for r in rows {
                let name = r.map_err(|e| Error::storage("SqliteStorage::seed_manifest", "row error", Some(Box::new(e))))?;
                existing_collections.insert(CollectionName::new(name)?);
            }
        }

        let mut existing_schemas: HashMap<CollectionName, SchemaVersion> = HashMap::new();
        {
            let mut stmt = tx.prepare("SELECT collection_name, version FROM schemas WHERE is_active = 1").map_err(|e| {
                Error::storage("SqliteStorage::seed_manifest", "query schemas prepare failed", Some(Box::new(e)))
            })?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, u32>(1)?))).map_err(|e| {
                Error::storage("SqliteStorage::seed_manifest", "query schemas failed", Some(Box::new(e)))
            })?;
            for r in rows {
                let (col, ver) = r.map_err(|e| Error::storage("SqliteStorage::seed_manifest", "row error", Some(Box::new(e))))?;
                existing_schemas.insert(CollectionName::new(col)?, SchemaVersion::new(ver)?);
            }
        }

        let mut existing_indexes: HashMap<(CollectionName, JsonPointer), IndexVersion> = HashMap::new();
        {
            let mut stmt = tx.prepare("SELECT collection_name, pointer, version FROM indexes").map_err(|e| {
                Error::storage("SqliteStorage::seed_manifest", "query indexes prepare failed", Some(Box::new(e)))
            })?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, u32>(2)?))).map_err(|e| {
                Error::storage("SqliteStorage::seed_manifest", "query indexes failed", Some(Box::new(e)))
            })?;
            for r in rows {
                let (col, ptr, ver) = r.map_err(|e| Error::storage("SqliteStorage::seed_manifest", "row error", Some(Box::new(e))))?;
                existing_indexes.insert(
                    (CollectionName::new(col)?, JsonPointer::new(ptr)?),
                    IndexVersion::new(ver)?,
                );
            }
        }

        let mut seeded_collections = Vec::new();
        let mut seeded_schemas = Vec::new();
        let mut seeded_indexes = Vec::new();

        for col in manifest.collections() {
            let col_name = col.name();

            if !existing_collections.contains(col_name) {
                tx.execute(
                    "INSERT INTO collections (name, created_at) VALUES (?1, ?2)",
                    params![col_name.as_str(), &now],
                ).map_err(|e| Error::storage("SqliteStorage::seed_manifest", "insert collection failed", Some(Box::new(e))))?;
                seeded_collections.push(col_name.clone());
            }

            if let Some(schema) = col.schema()
                && !existing_schemas.contains_key(col_name)
            {
                let schema_str = serde_json::to_string(schema).map_err(|e| {
                    Error::invalid_input("SqliteStorage::seed_manifest", "schema", format!("schema serialization error: {e}"))
                })?;
                tx.execute(
                    "INSERT INTO schemas (collection_name, version, schema_json, created_at, is_active)
                     VALUES (?1, 1, ?2, ?3, 1)",
                    params![col_name.as_str(), schema_str, &now],
                ).map_err(|e| Error::storage("SqliteStorage::seed_manifest", "insert schema failed", Some(Box::new(e))))?;
                seeded_schemas.push((col_name.clone(), SchemaVersion::INITIAL));
            }

            for idx in col.indexes() {
                let ptr = idx.pointer();
                let key = (col_name.clone(), ptr.clone());
                if !existing_indexes.contains_key(&key) {
                    tx.execute(
                        "INSERT INTO indexes (collection_name, pointer, version, created_at)
                         VALUES (?1, ?2, 1, ?3)",
                        params![col_name.as_str(), ptr.as_str(), &now],
                    ).map_err(|e| Error::storage("SqliteStorage::seed_manifest", "insert index failed", Some(Box::new(e))))?;
                    seeded_indexes.push((col_name.clone(), ptr.clone(), IndexVersion::INITIAL));
                }
            }
        }

        tx.commit().map_err(|e| {
            Error::storage("SqliteStorage::seed_manifest", "failed to commit transaction", Some(Box::new(e)))
        })?;

        // Build full preserved lists
        let mut preserved_collections: Vec<_> = existing_collections.into_iter().collect();
        preserved_collections.sort();

        let mut preserved_schemas: Vec<_> = existing_schemas.into_iter().collect();
        preserved_schemas.sort();

        let mut preserved_indexes: Vec<_> = existing_indexes.into_iter().map(|((c, p), v)| (c, p, v)).collect();
        preserved_indexes.sort();

        seeded_collections.sort();
        seeded_schemas.sort();
        seeded_indexes.sort();

        Ok(InitializationReport {
            seeded_collections,
            preserved_collections,
            seeded_schemas,
            preserved_schemas,
            seeded_indexes,
            preserved_indexes,
        })
    }

    fn run_transaction(&self, f: &mut dyn FnMut(&mut dyn StorageTransaction) -> Result<(), Error>) -> Result<(), Error> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| {
            Error::transaction("SqliteStorage::run_transaction", "failed to start transaction", Some(Box::new(e)))
        })?;

        let mut sqlite_tx = SqliteTransaction { tx };

        // If f() returns Err, sqlite_tx.tx drops and rolls back.
        // If f() panics, stack unwinding drops sqlite_tx.tx and rolls back; panic propagates unchanged.
        f(&mut sqlite_tx)?;

        sqlite_tx.tx.commit().map_err(|e| {
            Error::transaction("SqliteStorage::run_transaction", "failed to commit transaction", Some(Box::new(e)))
        })?;

        Ok(())
    }
}
