//! Host key storage.
//!
//! The key is generated on first boot and written to disk mode `0600`.
//! It is never embedded in the binary.

use std::fs;
use std::path::Path;

use russh::keys::ssh_key::{Algorithm, LineEnding, PrivateKey};

use crate::SshError;

pub fn load_or_create_host_key(path: &Path) -> Result<PrivateKey, SshError> {
    if path.exists() {
        return russh::keys::load_secret_key(path, None)
            .map_err(|err| SshError::Message(format!("failed to load host key: {err}")));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let key = PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519)
        .map_err(|err| SshError::Message(format!("failed to generate host key: {err}")))?;
    let openssh = key
        .to_openssh(LineEnding::LF)
        .map_err(|err| SshError::Message(format!("failed to encode host key: {err}")))?;
    fs::write(path, openssh.as_bytes())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
    }
    let public = key
        .public_key()
        .to_openssh()
        .map_err(|err| SshError::Message(format!("failed to encode public key: {err}")))?;
    let pub_path = path.with_extension("pub");
    fs::write(&pub_path, format!("{public} coffee-shop\n"))?;
    tracing::info!(path = %path.display(), "generated new SSH host key");
    Ok(key)
}

pub fn public_key_line(key: &PrivateKey) -> Result<String, SshError> {
    key.public_key()
        .to_openssh()
        .map_err(|err| SshError::Message(format!("failed to encode public key: {err}")))
}
