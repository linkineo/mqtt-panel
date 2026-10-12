//! Authentication: anonymous gate, per-listener login, shared users (argon2 hashes at rest).

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use std::collections::HashMap;

/// A stored password: an argon2 PHC string, or plain text (listener logins from the keychain, TOML files).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Credential {
    Argon2(String),
    Plain(String),
}

impl Credential {
    /// Checks a password. Argon2 verification takes tens of milliseconds: call from a blocking context.
    pub fn verify(&self, password: &str) -> bool {
        match self {
            Credential::Plain(p) => constant_time_eq(p.as_bytes(), password.as_bytes()),
            Credential::Argon2(h) => verify_password(h, password),
        }
    }
}

fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Hashes a password with argon2id (PHC string).
pub fn hash_password(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| e.to_string())
}

/// Verifies a password against an argon2 PHC string.
pub fn verify_password(hash: &str, password: &str) -> bool {
    match PasswordHash::new(hash) {
        Ok(h) => Argon2::default()
            .verify_password(password.as_bytes(), &h)
            .is_ok(),
        Err(_) => false,
    }
}

/// Live authentication settings (can be replaced while the broker runs).
#[derive(Debug, Clone, Default)]
pub struct AuthConfig {
    /// Anyone can connect: users, listener logins and client certificates are ignored.
    pub anonymous: bool,
    /// Shared users, accepted on every listener.
    pub users: HashMap<String, Credential>,
    /// Per-listener login: listener id -> (username, password).
    pub listener_logins: HashMap<String, (String, Credential)>,
}

/// Outcome of the authentication step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AuthOutcome {
    Accepted,
    BadLogin,
}

/// Decides whether a client may connect (order documented in the crate root).
pub(crate) fn authenticate(
    auth: &AuthConfig,
    listener_id: &str,
    require_client_cert: bool,
    cert_verified: bool,
    username: Option<&str>,
    password: Option<&str>,
) -> AuthOutcome {
    if auth.anonymous {
        return AuthOutcome::Accepted;
    }
    if require_client_cert && cert_verified {
        return AuthOutcome::Accepted;
    }
    let (Some(user), Some(pass)) = (username, password) else {
        return AuthOutcome::BadLogin;
    };
    if let Some((lu, lp)) = auth.listener_logins.get(listener_id) {
        if lu == user && lp.verify(pass) {
            return AuthOutcome::Accepted;
        }
    }
    if let Some(c) = auth.users.get(user) {
        if c.verify(pass) {
            return AuthOutcome::Accepted;
        }
    }
    AuthOutcome::BadLogin
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn argon2_round_trip_and_order() {
        let h = hash_password("s3cret").unwrap();
        assert!(h.starts_with("$argon2id$"));
        assert!(verify_password(&h, "s3cret"));
        assert!(!verify_password(&h, "wrong"));
        let mut a = AuthConfig::default();
        a.users.insert("alice".into(), Credential::Argon2(h));
        a.listener_logins
            .insert("tls".into(), ("dev".into(), Credential::Plain("pw".into())));
        assert_eq!(
            authenticate(&a, "tcp", false, false, Some("alice"), Some("s3cret")),
            AuthOutcome::Accepted
        );
        assert_eq!(
            authenticate(&a, "tcp", false, false, Some("dev"), Some("pw")),
            AuthOutcome::BadLogin
        );
        assert_eq!(
            authenticate(&a, "tls", false, false, Some("dev"), Some("pw")),
            AuthOutcome::Accepted
        );
        assert_eq!(
            authenticate(&a, "tls", true, true, None, None),
            AuthOutcome::Accepted
        );
        assert_eq!(
            authenticate(&a, "tls", false, true, None, None),
            AuthOutcome::BadLogin
        );
        a.anonymous = true;
        assert_eq!(
            authenticate(&a, "tcp", false, false, None, None),
            AuthOutcome::Accepted
        );
    }
}
