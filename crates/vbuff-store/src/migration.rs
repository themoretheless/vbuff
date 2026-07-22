//! On-disk migration preflight, manifest, verification, and rollback.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use rusqlite::{Connection, MAIN_DB};
use serde::{Deserialize, Serialize};

use crate::{Result, StoreError};

#[derive(Debug)]
pub(crate) struct MigrationGuard {
    live_path: PathBuf,
    backup_path: PathBuf,
    dry_run_path: PathBuf,
    manifest_path: PathBuf,
    pre_version: i64,
    target_version: i64,
    pre_rows: i64,
    pre_schema_hash: String,
    backup_checksum: String,
}

#[derive(Deserialize, Serialize)]
struct MigrationManifest {
    pre_version: i64,
    target_version: i64,
    pre_rows: i64,
    post_rows: i64,
    pre_schema_hash: String,
    post_schema_hash: String,
    backup_path: String,
    backup_blake3: String,
    verified_at_ms: i64,
}

impl MigrationGuard {
    pub(crate) fn prepare(path: &Path, target_version: i64) -> Result<Option<Self>> {
        if !path.exists() || path.metadata().map_err(StoreError::Io)?.len() == 0 {
            return Ok(None);
        }
        let source = Connection::open(path)?;
        let version: i64 = source.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version > target_version {
            return Err(StoreError::Migration(format!(
                "database version {version} is newer than supported {target_version}"
            )));
        }
        if version == target_version {
            return Ok(None);
        }
        if !table_exists(&source, "clips")? {
            return Ok(None);
        }

        let pre_rows = row_count(&source)?;
        let pre_schema_hash = schema_hash(&source)?;
        let backup_path = path.with_extension(format!("migration-v{version}.bak"));
        let dry_run_path = path.with_extension("migration-dry-run.db");
        let manifest_path = path.with_extension("migration.json");
        remove_if_exists(&backup_path)?;
        remove_if_exists(&dry_run_path)?;
        source.backup(MAIN_DB, &backup_path, None)?;
        harden_file(&backup_path)?;
        std::fs::copy(&backup_path, &dry_run_path).map_err(StoreError::Io)?;
        harden_file(&dry_run_path)?;
        let backup_checksum = file_checksum(&backup_path)?;

        Ok(Some(Self {
            live_path: path.to_path_buf(),
            backup_path,
            dry_run_path,
            manifest_path,
            pre_version: version,
            target_version,
            pre_rows,
            pre_schema_hash,
            backup_checksum,
        }))
    }

    pub(crate) fn dry_run_path(&self) -> &Path {
        &self.dry_run_path
    }

    pub(crate) fn verify_dry_run(&self, connection: &Connection) -> Result<()> {
        self.verify(connection)?;
        let post_rows = row_count(connection)?;
        let manifest = MigrationManifest {
            pre_version: self.pre_version,
            target_version: self.target_version,
            pre_rows: self.pre_rows,
            post_rows,
            pre_schema_hash: self.pre_schema_hash.clone(),
            post_schema_hash: schema_hash(connection)?,
            backup_path: self.backup_path.to_string_lossy().into_owned(),
            backup_blake3: self.backup_checksum.clone(),
            verified_at_ms: chrono::Utc::now().timestamp_millis(),
        };
        let bytes = serde_json::to_vec_pretty(&manifest)?;
        atomic_write(&self.manifest_path, &bytes)
    }

    pub(crate) fn finish_dry_run(&self) -> Result<()> {
        remove_database_files(&self.dry_run_path)
    }

    pub(crate) fn verify_live(&self, connection: &Connection) -> Result<()> {
        self.verify(connection)?;
        let manifest: MigrationManifest =
            serde_json::from_slice(&std::fs::read(&self.manifest_path).map_err(StoreError::Io)?)?;
        let live_schema_hash = schema_hash(connection)?;
        if live_schema_hash != manifest.post_schema_hash {
            return Err(StoreError::Migration(format!(
                "live schema hash {live_schema_hash} differs from dry-run hash {}",
                manifest.post_schema_hash
            )));
        }
        Ok(())
    }

    pub(crate) fn rollback(&self) -> Result<()> {
        let checksum = file_checksum(&self.backup_path)?;
        if checksum != self.backup_checksum {
            return Err(StoreError::Migration(format!(
                "migration backup checksum changed: expected {}, found {checksum}",
                self.backup_checksum
            )));
        }
        remove_if_exists(&sidecar(&self.live_path, "-wal"))?;
        remove_if_exists(&sidecar(&self.live_path, "-shm"))?;
        atomic_copy(&self.backup_path, &self.live_path)
    }

    /// Remove plaintext rollback artifacts once the live migration has been
    /// verified. The backup exists only for the transactional migration window.
    pub(crate) fn commit(&self) -> Result<()> {
        remove_database_files(&self.backup_path)?;
        remove_if_exists(&self.manifest_path)
    }

    fn verify(&self, connection: &Connection) -> Result<()> {
        let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version != self.target_version {
            return Err(StoreError::Migration(format!(
                "expected schema {}, found {version}",
                self.target_version
            )));
        }
        let post_rows = row_count(connection)?;
        if post_rows != self.pre_rows {
            return Err(StoreError::Migration(format!(
                "row count changed from {} to {post_rows}",
                self.pre_rows
            )));
        }
        let quick_check: String =
            connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
        if quick_check != "ok" {
            return Err(StoreError::Migration(format!(
                "SQLite quick_check failed: {quick_check}"
            )));
        }
        Ok(())
    }
}

impl Drop for MigrationGuard {
    fn drop(&mut self) {
        let _ = remove_database_files(&self.dry_run_path);
    }
}

fn table_exists(connection: &Connection, table: &str) -> Result<bool> {
    Ok(connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name=?1)",
        [table],
        |row| row.get(0),
    )?)
}

fn row_count(connection: &Connection) -> Result<i64> {
    Ok(connection.query_row("SELECT COUNT(*) FROM clips", [], |row| row.get(0))?)
}

fn schema_hash(connection: &Connection) -> Result<String> {
    let mut statement = connection
        .prepare("SELECT type, name, COALESCE(sql, '') FROM sqlite_master ORDER BY type, name")?;
    let rows = statement.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
        ))
    })?;
    let mut hasher = blake3::Hasher::new();
    for row in rows {
        let (kind, name, sql) = row?;
        hasher.update(kind.as_bytes());
        hasher.update(&[0]);
        hasher.update(name.as_bytes());
        hasher.update(&[0]);
        hasher.update(sql.as_bytes());
        hasher.update(&[0xff]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn file_checksum(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(StoreError::Io)?;
    let mut buffer = [0_u8; 64 * 1024];
    let mut hasher = blake3::Hasher::new();
    loop {
        let read = file.read(&mut buffer).map_err(StoreError::Io)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_hex().to_string())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<()> {
    let mut file = atomic_write_file::AtomicWriteFile::open(path).map_err(StoreError::Io)?;
    file.write_all(bytes).map_err(StoreError::Io)?;
    file.as_file().sync_all().map_err(StoreError::Io)?;
    file.commit().map_err(StoreError::Io)?;
    harden_file(path)
}

fn atomic_copy(source: &Path, destination: &Path) -> Result<()> {
    let mut source = std::fs::File::open(source).map_err(StoreError::Io)?;
    let mut output =
        atomic_write_file::AtomicWriteFile::open(destination).map_err(StoreError::Io)?;
    std::io::copy(&mut source, &mut output).map_err(StoreError::Io)?;
    output.as_file().sync_all().map_err(StoreError::Io)?;
    output.commit().map_err(StoreError::Io)?;
    harden_file(destination)
}

#[cfg(unix)]
fn harden_file(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600)).map_err(StoreError::Io)
}

#[cfg(not(unix))]
fn harden_file(_path: &Path) -> Result<()> {
    Ok(())
}

fn remove_database_files(path: &Path) -> Result<()> {
    remove_if_exists(path)?;
    remove_if_exists(&sidecar(path, "-wal"))?;
    remove_if_exists(&sidecar(path, "-shm"))
}

pub(crate) fn cleanup_stale_after_verified_open(
    path: &Path,
    target_version: i64,
    connection: &Connection,
) -> Result<()> {
    let has_stale_artifact = (0..target_version).any(|version| {
        database_files_exist(&path.with_extension(format!("migration-v{version}.bak")))
    }) || database_files_exist(
        &path.with_extension("migration-dry-run.db"),
    ) || path.with_extension("migration.json").exists();
    if !has_stale_artifact {
        return Ok(());
    }
    let version: i64 = connection.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != target_version {
        return Err(StoreError::Migration(format!(
            "refusing stale migration cleanup at schema {version}; expected {target_version}"
        )));
    }
    let quick_check: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if quick_check != "ok" {
        return Err(StoreError::Migration(format!(
            "refusing stale migration cleanup after SQLite quick_check: {quick_check}"
        )));
    }
    for version in 0..target_version {
        remove_database_files(&path.with_extension(format!("migration-v{version}.bak")))?;
    }
    remove_database_files(&path.with_extension("migration-dry-run.db"))?;
    remove_if_exists(&path.with_extension("migration.json"))
}

fn database_files_exist(path: &Path) -> bool {
    path.exists() || sidecar(path, "-wal").exists() || sidecar(path, "-shm").exists()
}

fn remove_if_exists(path: &Path) -> Result<()> {
    match std::fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(StoreError::Io(error)),
    }
}

fn sidecar(path: &Path, suffix: &str) -> PathBuf {
    PathBuf::from(format!("{}{suffix}", path.to_string_lossy()))
}
