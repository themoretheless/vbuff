//! Sharded content-addressable storage for large flavor bodies.

use std::collections::HashSet;
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::PathBuf;

use vbuff_types::{Body, ClipId, ContentKind, Flavor};

use crate::{Result, StoreError};

#[derive(Clone, Debug)]
pub(crate) struct CasStore {
    root: PathBuf,
}

impl CasStore {
    pub(crate) fn new(root: PathBuf) -> Result<Self> {
        std::fs::create_dir_all(&root).map_err(StoreError::Io)?;
        harden_directory(&root)?;
        Ok(Self { root })
    }

    pub(crate) fn spill_flavors(&self, flavors: &mut [Flavor], kind: ContentKind) -> Result<()> {
        let threshold = threshold_for(kind);
        for flavor in flavors {
            let Body::Inline(bytes) = &flavor.body else {
                continue;
            };
            if bytes.len() <= threshold {
                continue;
            }
            let byte_size = bytes.len() as u64;
            let blob_ref = self.put(kind, bytes)?;
            flavor.body = Body::Spilled {
                blob_ref,
                byte_size,
            };
        }
        Ok(())
    }

    pub(crate) fn hydrate_flavors(&self, flavors: &mut [Flavor], kind: ContentKind) -> Result<()> {
        for flavor in flavors {
            let Body::Spilled {
                blob_ref,
                byte_size,
            } = &flavor.body
            else {
                continue;
            };
            let bytes = self.read(kind, blob_ref, *byte_size)?;
            flavor.body = Body::Inline(bytes);
        }
        Ok(())
    }

    pub(crate) fn remove(&self, kind: ContentKind, blob_ref: &str) -> Result<()> {
        let path = self.path_for(kind, blob_ref)?;
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(StoreError::Io(error)),
        }
    }

    /// Remove complete but unreferenced blobs and abandoned temporary writes.
    /// The application enforces one store owner, so startup/idle GC cannot race
    /// another vbuff writer installing a blob.
    pub(crate) fn remove_orphans(&self, live: &HashSet<(ContentKind, String)>) -> Result<usize> {
        let mut removed = 0;
        for (kind, slug) in ALL_KINDS {
            let kind_root = self.root.join(slug);
            if !kind_root.exists() {
                continue;
            }
            for first in read_dir(&kind_root)? {
                if !is_regular_dir(&first)? {
                    continue;
                }
                for second in read_dir(&first)? {
                    if !is_regular_dir(&second)? {
                        continue;
                    }
                    for path in read_dir(&second)? {
                        if !is_regular_file(&path)? {
                            continue;
                        }
                        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                            continue;
                        };
                        let abandoned_temporary = name.contains(".tmp-");
                        let valid_blob = valid_blob_ref(name);
                        if abandoned_temporary
                            || (valid_blob && !live.contains(&(kind, name.to_owned())))
                        {
                            std::fs::remove_file(&path).map_err(StoreError::Io)?;
                            removed += 1;
                        }
                    }
                }
            }
        }
        Ok(removed)
    }

    fn put(&self, kind: ContentKind, bytes: &[u8]) -> Result<String> {
        let digest = blake3::hash(bytes);
        let blob_ref = digest.to_hex().to_string();
        let path = self.path_for(kind, &blob_ref)?;
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.file_type().is_file() => {
                if metadata.len() == bytes.len() as u64 && file_hash(&path)? == *digest.as_bytes() {
                    return Ok(blob_ref);
                }
                return Err(StoreError::Corrupt(format!(
                    "existing CAS blob {blob_ref} does not match its hash"
                )));
            }
            Ok(_) => {
                return Err(StoreError::Corrupt(format!(
                    "CAS destination for {blob_ref} is not a regular file"
                )));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(StoreError::Io(error)),
        }
        let parent = path
            .parent()
            .ok_or_else(|| StoreError::Corrupt("CAS path has no parent".into()))?;
        std::fs::create_dir_all(parent).map_err(StoreError::Io)?;
        self.harden_tree(parent)?;
        let temporary = path.with_extension(format!("tmp-{}", ClipId::new()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary).map_err(StoreError::Io)?;
        file.write_all(bytes).map_err(StoreError::Io)?;
        file.sync_all().map_err(StoreError::Io)?;
        match std::fs::rename(&temporary, &path) {
            Ok(()) => Ok(blob_ref),
            Err(_error) if path.exists() => {
                let _ = std::fs::remove_file(temporary);
                let metadata = std::fs::symlink_metadata(&path).map_err(StoreError::Io)?;
                if metadata.file_type().is_file()
                    && metadata.len() == bytes.len() as u64
                    && file_hash(&path)? == *digest.as_bytes()
                {
                    Ok(blob_ref)
                } else {
                    Err(StoreError::Corrupt(format!(
                        "racing CAS blob {blob_ref} failed integrity verification"
                    )))
                }
            }
            Err(error) => {
                let _ = std::fs::remove_file(temporary);
                Err(StoreError::Io(error))
            }
        }
    }

    fn read(&self, kind: ContentKind, blob_ref: &str, expected_size: u64) -> Result<Vec<u8>> {
        let path = self.path_for(kind, blob_ref)?;
        let file = std::fs::File::open(path).map_err(StoreError::Io)?;
        let metadata = file.metadata().map_err(StoreError::Io)?;
        if !metadata.file_type().is_file() || metadata.len() != expected_size {
            return Err(StoreError::Corrupt(format!(
                "CAS blob {blob_ref} size or type mismatch"
            )));
        }
        let capacity = usize::try_from(expected_size)
            .map_err(|_| StoreError::Corrupt(format!("CAS blob {blob_ref} is too large")))?;
        let mut bytes = Vec::with_capacity(capacity);
        file.take(expected_size.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(StoreError::Io)?;
        if bytes.len() != capacity {
            return Err(StoreError::Corrupt(format!(
                "CAS blob {blob_ref} changed while reading"
            )));
        }
        if blake3::hash(&bytes).to_hex().as_str() != blob_ref {
            return Err(StoreError::Corrupt(format!(
                "CAS blob {blob_ref} failed integrity verification"
            )));
        }
        Ok(bytes)
    }

    fn path_for(&self, kind: ContentKind, blob_ref: &str) -> Result<PathBuf> {
        if !valid_blob_ref(blob_ref) {
            return Err(StoreError::Corrupt("invalid CAS blob reference".into()));
        }
        Ok(self
            .root
            .join(kind_slug(kind))
            .join(&blob_ref[0..2])
            .join(&blob_ref[2..4])
            .join(blob_ref))
    }

    fn harden_tree(&self, leaf: &std::path::Path) -> Result<()> {
        let mut current = Some(leaf);
        while let Some(path) = current {
            if !path.starts_with(&self.root) {
                break;
            }
            harden_directory(path)?;
            if path == self.root {
                break;
            }
            current = path.parent();
        }
        Ok(())
    }
}

#[cfg(unix)]
fn harden_directory(path: &std::path::Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;

    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700)).map_err(StoreError::Io)
}

#[cfg(not(unix))]
fn harden_directory(_path: &std::path::Path) -> Result<()> {
    Ok(())
}

fn file_hash(path: &std::path::Path) -> Result<[u8; 32]> {
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
    Ok(*hasher.finalize().as_bytes())
}

fn read_dir(path: &std::path::Path) -> Result<Vec<PathBuf>> {
    std::fs::read_dir(path)
        .map_err(StoreError::Io)?
        .map(|entry| entry.map(|entry| entry.path()).map_err(StoreError::Io))
        .collect()
}

fn is_regular_dir(path: &std::path::Path) -> Result<bool> {
    Ok(std::fs::symlink_metadata(path)
        .map_err(StoreError::Io)?
        .file_type()
        .is_dir())
}

fn is_regular_file(path: &std::path::Path) -> Result<bool> {
    Ok(std::fs::symlink_metadata(path)
        .map_err(StoreError::Io)?
        .file_type()
        .is_file())
}

fn valid_blob_ref(blob_ref: &str) -> bool {
    blob_ref.len() == 64
        && blob_ref
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
}

fn threshold_for(kind: ContentKind) -> usize {
    match kind {
        ContentKind::Image | ContentKind::File => 256 * 1024,
        ContentKind::Html | ContentKind::Rtf | ContentKind::Other => 512 * 1024,
        ContentKind::Text | ContentKind::Url | ContentKind::Color | ContentKind::Code => {
            1024 * 1024
        }
    }
}

fn kind_slug(kind: ContentKind) -> &'static str {
    match kind {
        ContentKind::Text => "text",
        ContentKind::Url => "url",
        ContentKind::Color => "color",
        ContentKind::Code => "code",
        ContentKind::Image => "image",
        ContentKind::File => "file",
        ContentKind::Rtf => "rtf",
        ContentKind::Html => "html",
        ContentKind::Other => "other",
    }
}

const ALL_KINDS: [(ContentKind, &str); 9] = [
    (ContentKind::Text, "text"),
    (ContentKind::Url, "url"),
    (ContentKind::Color, "color"),
    (ContentKind::Code, "code"),
    (ContentKind::Image, "image"),
    (ContentKind::File, "file"),
    (ContentKind::Rtf, "rtf"),
    (ContentKind::Html, "html"),
    (ContentKind::Other, "other"),
];
