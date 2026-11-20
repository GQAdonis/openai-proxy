//! Stable conversation ownership across restarts, with invalidation after observed logout.
//! The record contains an owner fingerprint and a random public binding, never a bearer.
use std::{
    fs,
    io::{self, Read, Seek, Write},
    path::Path,
};

use fs2::FileExt;
use sha2::{Digest, Sha256};

pub fn update(source: &Path, owner: Option<&str>) -> io::Result<Option<String>> {
    let directory = std::env::var_os("PROXY_CONSOLE_DATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| crate::config::data_dir().join("console"))
        .join("credential-bindings");
    let source = format!(
        "{:x}",
        Sha256::digest(source.as_os_str().as_encoded_bytes())
    );
    let path = directory.join(source);
    if owner.is_none() && !path.exists() {
        return Ok(None);
    }
    let mut directory_options = fs::DirBuilder::new();
    directory_options.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        directory_options.mode(0o700);
    }
    directory_options.create(directory)?;
    let mut options = fs::OpenOptions::new();
    options.read(true).write(true).create(true).truncate(false);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.lock_exclusive()?;
    let mut existing = String::new();
    file.read_to_string(&mut existing)?;
    let fingerprint = owner.map(|s| format!("{:x}", Sha256::digest(s.as_bytes())));
    let binding = match fingerprint {
        None => None,
        Some(key) => {
            if let Some((previous, binding)) = existing.trim().split_once(' ')
                && previous == key
                && uuid::Uuid::parse_str(binding).is_ok()
            {
                return Ok(Some(binding.to_owned()));
            }
            Some((key, uuid::Uuid::new_v4().to_string()))
        }
    };
    file.rewind()?;
    file.set_len(0)?;
    if let Some((key, binding)) = &binding {
        writeln!(file, "{key} {binding}")?;
    }
    file.sync_all()?;
    Ok(binding.map(|(_, id)| id))
}
