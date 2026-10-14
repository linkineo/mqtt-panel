//! Authentication: anonymous gate and shared users (argon2 hashes at rest). There is no per-listener login.

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;
use std::collections::HashMap;

/// A stored password: an argon2 PHC string, or plain text (TOML files).
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
    /// Anyone can connect: users and client certificates are ignored.
    pub anonymous: bool,
    /// Shared users, accepted on every listener.
    pub users: HashMap<String, Credential>,
}

/// How a session logged in (session registry, change set 7): decides which sessions a later change invalidates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Login {
    /// Accepted because Anonymous is on, without a valid login.
    Anonymous,
    /// A shared user; the credential in force at login, so a password change or removal is detected.
    Password {
        username: String,
        credential: Credential,
    },
    /// A client certificate on an encrypted listener that required one.
    Certificate,
}

/// Outcome of the authentication step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuthOutcome {
    Accepted(Login),
    BadLogin,
}

/// Decides whether a client may connect (order documented in the crate root): a required and verified client
/// certificate, then a valid shared user, then Anonymous.
pub(crate) fn authenticate(
    auth: &AuthConfig,
    require_client_cert: bool,
    cert_verified: bool,
    username: Option<&str>,
    password: Option<&str>,
) -> AuthOutcome {
    if !auth.anonymous && require_client_cert && cert_verified {
        return AuthOutcome::Accepted(Login::Certificate);
    }
    if let (Some(user), Some(pass)) = (username, password) {
        if let Some(c) = auth.users.get(user) {
            if c.verify(pass) {
                return AuthOutcome::Accepted(Login::Password {
                    username: user.to_string(),
                    credential: c.clone(),
                });
            }
        }
    }
    if auth.anonymous {
        return AuthOutcome::Accepted(Login::Anonymous);
    }
    AuthOutcome::BadLogin
}

/// True when a session that logged in with `login` would still be accepted under `auth` (users and Anonymous only;
/// certificates are checked by the caller).
pub(crate) fn login_still_valid(auth: &AuthConfig, login: &Login) -> bool {
    if auth.anonymous {
        return true;
    }
    match login {
        Login::Anonymous => false,
        Login::Certificate => true,
        Login::Password {
            username,
            credential,
        } => auth.users.get(username) == Some(credential),
    }
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
        a.users.insert("dev".into(), Credential::Plain("pw".into()));
        assert!(matches!(
            authenticate(&a, false, false, Some("alice"), Some("s3cret")),
            AuthOutcome::Accepted(Login::Password { .. })
        ));
        assert_eq!(
            authenticate(&a, false, false, Some("alice"), Some("wrong")),
            AuthOutcome::BadLogin
        );
        assert!(matches!(
            authenticate(&a, false, false, Some("dev"), Some("pw")),
            AuthOutcome::Accepted(Login::Password { .. })
        ));
        assert_eq!(
            authenticate(&a, true, true, None, None),
            AuthOutcome::Accepted(Login::Certificate)
        );
        assert_eq!(
            authenticate(&a, false, true, None, None),
            AuthOutcome::BadLogin
        );
        // Session registry: a password change or removal invalidates the login, Anonymous off invalidates
        // anonymous sessions.
        let AuthOutcome::Accepted(login) = authenticate(&a, false, false, Some("dev"), Some("pw"))
        else {
            panic!()
        };
        assert!(login_still_valid(&a, &login));
        a.users
            .insert("dev".into(), Credential::Plain("pw2".into()));
        assert!(!login_still_valid(&a, &login));
        a.anonymous = true;
        assert_eq!(
            authenticate(&a, false, false, None, None),
            AuthOutcome::Accepted(Login::Anonymous)
        );
        assert!(login_still_valid(&a, &Login::Anonymous));
        a.anonymous = false;
        assert!(!login_still_valid(&a, &Login::Anonymous));
    }
}
