//! Password hashing and account-name rules.
//!
//! Hashes are Argon2id. Passwords are never logged. Callers should drop the
//! plaintext as soon as hashing or verification returns.

use argon2::{Algorithm, Argon2, Params, PasswordHasher, PasswordVerifier, Version};
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AuthError {
    #[error("invalid username or password")]
    InvalidCredentials,
    #[error("password must be 8 to 128 characters")]
    WeakPassword,
    #[error("invalid username")]
    InvalidUsername,
    #[error("invalid operator name")]
    InvalidOperatorName,
    #[error("that username is already registered")]
    NameTaken,
    #[error("registration is closed")]
    RegistrationClosed,
    #[error("too many attempts, wait a moment")]
    RateLimited,
    #[error("authentication failed")]
    Internal,
}

pub fn canonical_username(raw: &str) -> Result<String, AuthError> {
    let name = raw.trim().to_ascii_lowercase();
    if !valid_username(&name) {
        return Err(AuthError::InvalidUsername);
    }
    Ok(name)
}

fn valid_username(name: &str) -> bool {
    let bytes = name.as_bytes();
    (3..=16).contains(&bytes.len())
        && bytes[0].is_ascii_lowercase()
        && bytes.iter().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == b'_')
}

pub fn validate_operator_name(raw: &str) -> Result<String, AuthError> {
    let name = raw.trim();
    let chars: Vec<char> = name.chars().collect();
    if !(2..=24).contains(&chars.len()) {
        return Err(AuthError::InvalidOperatorName);
    }
    let ok = chars.iter().all(|c| c.is_ascii_alphanumeric() || *c == ' ' || *c == '_' || *c == '-')
        && chars[0].is_ascii_alphanumeric()
        && !chars.iter().any(|c| c.is_control());
    if !ok || name.contains("  ") {
        return Err(AuthError::InvalidOperatorName);
    }
    Ok(name.to_string())
}

pub fn validate_password(password: &str) -> Result<(), AuthError> {
    let len = password.chars().count();
    if (8..=128).contains(&len) && !password.chars().any(|c| c.is_control()) {
        Ok(())
    } else {
        Err(AuthError::WeakPassword)
    }
}

pub fn hash_password(password: &str) -> Result<String, AuthError> {
    validate_password(password)?;
    hasher()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(|_| AuthError::Internal)
}

pub fn verify_password(password: &str, encoded: &str) -> bool {
    hasher().verify_password(password.as_bytes(), encoded).is_ok()
}

/// Hash used when the account does not exist, so rejection takes similar time.
pub fn dummy_hash() -> String {
    hash_password(&format!("dummy-{}", uuid::Uuid::new_v4())).unwrap_or_else(|_| {
        // A syntactically valid PHC string is enough for PasswordHash::new to run.
        "$argon2id$v=19$m=8192,t=1,p=1$c2FsdHNhbHRzYWx0$aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
            .to_string()
    })
}

fn hasher() -> Argon2<'static> {
    // Tests use lighter parameters. Release builds keep the Argon2 crate defaults.
    if cfg!(test) {
        let params = Params::new(8 * 1024, 1, 1, None).unwrap_or_else(|_| Params::default());
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
    } else {
        Argon2::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn username_rules() {
        assert!(canonical_username("Ada").is_ok());
        assert_eq!(canonical_username("Ada").unwrap(), "ada");
        assert!(canonical_username("ab").is_err());
        assert!(canonical_username("1abc").is_err());
        assert!(canonical_username("has space").is_err());
    }

    #[test]
    fn password_round_trip() {
        let hash = hash_password("correct horse").unwrap();
        assert!(verify_password("correct horse", &hash));
        assert!(!verify_password("wrong horse", &hash));
        assert!(hash_password("short").is_err());
    }
}
