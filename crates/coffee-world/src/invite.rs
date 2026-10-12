//! Single-use account invite codes.
//!
//! Codes gate registration when the server runs invite-only. Redeeming a code
//! creates the account and mints a small batch of fresh codes for that operator
//! to share. Settlement invites are a separate feature.

use terminus_core::PlayerId;
use uuid::Uuid;

/// Characters used in generated codes. Ambiguous glyphs (0/O, 1/I/L) are omitted.
const ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
/// Canonical key length before display formatting.
pub const CODE_KEY_LEN: usize = 8;
/// How many friend codes a new account receives after a successful redeem.
pub const DEFAULT_INVITES_PER_PLAYER: u32 = 2;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AccountInvite {
    pub id: Uuid,
    pub code_key: String,
    pub display_code: String,
    pub created_by: Option<PlayerId>,
    pub created_at: u64,
    pub note: String,
    pub redeemed_by: Option<PlayerId>,
    pub redeemed_at: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InviteBook {
    pub codes: Vec<AccountInvite>,
}

impl InviteBook {
    pub fn unused_for(&self, owner: PlayerId) -> Vec<&AccountInvite> {
        let mut rows: Vec<&AccountInvite> = self
            .codes
            .iter()
            .filter(|row| row.created_by == Some(owner) && row.redeemed_by.is_none())
            .collect();
        rows.sort_by(|left, right| {
            left.created_at.cmp(&right.created_at).then(left.display_code.cmp(&right.display_code))
        });
        rows
    }

    pub fn redeemed_count_for(&self, owner: PlayerId) -> usize {
        self.codes
            .iter()
            .filter(|row| row.created_by == Some(owner) && row.redeemed_by.is_some())
            .count()
    }

    pub fn push_many(&mut self, rows: impl IntoIterator<Item = AccountInvite>) {
        self.codes.extend(rows);
    }

    pub fn mark_redeemed(&mut self, code_key: &str, redeemed_by: PlayerId, redeemed_at: u64) {
        if let Some(row) = self.codes.iter_mut().find(|row| row.code_key == code_key) {
            row.redeemed_by = Some(redeemed_by);
            row.redeemed_at = Some(redeemed_at);
        }
    }
}

/// Normalize typed input to the canonical lookup key (`ABCD1234`).
pub fn normalize_invite_code(raw: &str) -> Result<String, &'static str> {
    let key: String = raw
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_uppercase())
        .collect();
    if key.len() != CODE_KEY_LEN {
        return Err("Invite codes look like ABCD-EFGH.");
    }
    if !key.bytes().all(|byte| ALPHABET.contains(&byte)) {
        return Err("Invite codes look like ABCD-EFGH.");
    }
    Ok(key)
}

/// Format a canonical key for players: `ABCD-EFGH`.
pub fn format_invite_code(code_key: &str) -> String {
    if code_key.len() == CODE_KEY_LEN {
        format!("{}-{}", &code_key[..4], &code_key[4..])
    } else {
        code_key.to_string()
    }
}

/// Mint one unused code from random UUID bytes.
pub fn generate_invite_code() -> (String, String) {
    let bytes = Uuid::new_v4().into_bytes();
    let mut key = String::with_capacity(CODE_KEY_LEN);
    for byte in bytes.iter().take(CODE_KEY_LEN) {
        key.push(ALPHABET[usize::from(*byte) % ALPHABET.len()] as char);
    }
    let display = format_invite_code(&key);
    (key, display)
}

pub fn new_invite(
    created_by: Option<PlayerId>,
    created_at: u64,
    note: impl Into<String>,
) -> AccountInvite {
    let (code_key, display_code) = generate_invite_code();
    AccountInvite {
        id: Uuid::new_v4(),
        code_key,
        display_code,
        created_by,
        created_at,
        note: note.into(),
        redeemed_by: None,
        redeemed_at: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_accepts_hyphens_and_case() {
        let (key, display) = generate_invite_code();
        assert_eq!(key.len(), CODE_KEY_LEN);
        assert_eq!(normalize_invite_code(&display).unwrap(), key);
        assert_eq!(normalize_invite_code(&key.to_ascii_lowercase()).unwrap(), key);
        assert_eq!(normalize_invite_code(&format!("  {display}  ")).unwrap(), key);
        assert!(normalize_invite_code("short").is_err());
        assert!(normalize_invite_code("OOOO-OOOO").is_err());
    }

    #[test]
    fn unused_list_is_owner_scoped() {
        let ada = PlayerId::new();
        let bob = PlayerId::new();
        let mut book = InviteBook::default();
        let mut ada_code = new_invite(Some(ada), 1, "");
        let bob_code = new_invite(Some(bob), 2, "");
        ada_code.redeemed_by = Some(bob);
        ada_code.redeemed_at = Some(3);
        book.push_many([ada_code.clone(), bob_code.clone(), new_invite(Some(ada), 4, "")]);
        assert_eq!(book.unused_for(ada).len(), 1);
        assert_eq!(book.redeemed_count_for(ada), 1);
        assert_eq!(book.unused_for(bob).len(), 1);
    }
}
