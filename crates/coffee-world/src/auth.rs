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
    #[error("invalid username or recovery code")]
    InvalidRecovery,
    #[error("invalid email address")]
    InvalidEmail,
    #[error("that email is already registered")]
    EmailTaken,
    #[error("this guest account cannot drop its recovery code")]
    RecoveryRequired,
    #[error("this account is already permanent")]
    AlreadyPermanent,
    #[error("upgrade this guest account to set a password")]
    UpgradeRequired,
    #[error("authentication failed")]
    Internal,
}

/// Guest accounts have no password yet. Permanent accounts do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccountKind {
    Guest,
    Permanent,
}

impl AccountKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Guest => "guest",
            Self::Permanent => "permanent",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw {
            "guest" => Some(Self::Guest),
            "permanent" => Some(Self::Permanent),
            _ => None,
        }
    }
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

/// Empty means "no email". A present address is stored in lowercase and is not an account key.
pub fn validate_email(raw: &str) -> Result<Option<String>, AuthError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    let email = trimmed.to_ascii_lowercase();
    let Some((local, domain)) = email.split_once('@') else {
        return Err(AuthError::InvalidEmail);
    };
    let domain_ok = domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains("..")
        && domain.chars().all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-');
    let local_ok = !local.is_empty()
        && local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && local.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'));
    if !domain_ok || !local_ok || email.len() > 254 || email.chars().any(|c| c.is_control()) {
        return Err(AuthError::InvalidEmail);
    }
    Ok(Some(email))
}

const RECOVERY_ALPHABET: &[u8] = b"23456789ABCDEFGHJKLMNPQRSTUVWXYZ";
const RECOVERY_GROUPS: usize = 6;
const RECOVERY_GROUP_LEN: usize = 4;

/// 120 unbiased bits, shown as six groups of four. The caller must display this once and drop it.
pub fn generate_recovery_code() -> Result<String, AuthError> {
    let symbols = RECOVERY_GROUPS * RECOVERY_GROUP_LEN;
    let mut raw = vec![0_u8; symbols];
    let mut rng = rand::rngs::SysRng;
    rand::TryRng::try_fill_bytes(&mut rng, &mut raw).map_err(|_| AuthError::Internal)?;
    let mut out = String::with_capacity(symbols + RECOVERY_GROUPS - 1);
    for (index, byte) in raw.iter().enumerate() {
        if index > 0 && index % RECOVERY_GROUP_LEN == 0 {
            out.push('-');
        }
        let pick = usize::from(*byte) % RECOVERY_ALPHABET.len();
        out.push(RECOVERY_ALPHABET[pick] as char);
    }
    Ok(out)
}

/// Accepts the hyphenated code or the same symbols with spaces and hyphens removed.
pub fn canonical_recovery_code(raw: &str) -> Result<String, AuthError> {
    let compact: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| c.to_ascii_uppercase())
        .collect();
    let expect = RECOVERY_GROUPS * RECOVERY_GROUP_LEN;
    if compact.len() != expect {
        return Err(AuthError::InvalidRecovery);
    }
    if !compact.bytes().all(|byte| RECOVERY_ALPHABET.contains(&byte)) {
        return Err(AuthError::InvalidRecovery);
    }
    let mut out = String::with_capacity(expect + RECOVERY_GROUPS - 1);
    for (index, ch) in compact.chars().enumerate() {
        if index > 0 && index % RECOVERY_GROUP_LEN == 0 {
            out.push('-');
        }
        out.push(ch);
    }
    Ok(out)
}

pub fn hash_recovery_code(code: &str) -> Result<String, AuthError> {
    let canonical = canonical_recovery_code(code)?;
    hash_password(&canonical)
}

pub fn verify_recovery_code(code: &str, encoded: &str) -> bool {
    let Ok(canonical) = canonical_recovery_code(code) else {
        return false;
    };
    verify_password(&canonical, encoded)
}

/// Password verifier for a guest. The plaintext is discarded by the caller.
pub fn hash_unusable_password() -> Result<String, AuthError> {
    let secret = generate_recovery_code()?;
    hash_password(&secret)
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
        assert!(!verify_password("correct horse", "not-a-hash"));
    }

    #[test]
    fn email_is_optional_and_not_an_account_key() {
        assert_eq!(validate_email("  ").unwrap(), None);
        assert_eq!(validate_email("Ada@Example.COM").unwrap().as_deref(), Some("ada@example.com"));
        assert!(validate_email("not-an-email").is_err());
        assert!(validate_email("a@b").is_err());
    }

    #[test]
    fn recovery_codes_are_high_entropy_and_not_stored_as_plaintext() {
        let first = generate_recovery_code().unwrap();
        let second = generate_recovery_code().unwrap();
        assert_ne!(first, second);
        assert_eq!(first.chars().filter(|c| *c != '-').count(), 24);
        assert!(first.split('-').all(|group| group.len() == 4));
        assert!(canonical_recovery_code(&first.replace('-', " ")).is_ok());
        let hash = hash_recovery_code(&first).unwrap();
        assert!(!hash.contains(&first));
        assert!(!hash.contains(&first.replace('-', "")));
        assert!(verify_recovery_code(&first, &hash));
        assert!(verify_recovery_code(&first.replace('-', ""), &hash));
        assert!(!verify_recovery_code(&second, &hash));
        assert!(canonical_recovery_code("1234").is_err());
        assert!(!verify_recovery_code("nope", &hash));
    }
}
