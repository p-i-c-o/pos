//! Security-critical POS authentication primitives. This crate never logs
//! credentials and does not expose any unauthenticated HTTP operations.
//!
//! Passwords use per-user salted Argon2id. Session secrets are 256-bit
//! cryptographically random bearer tokens, stored server-side only as SHA-256
//! digests. Session/device/user revocation is enforced by storage.

use std::{error::Error, fmt};

use argon2::{
    Algorithm, Argon2, Params, PasswordHash, PasswordHasher, PasswordVerifier, Version,
    password_hash::SaltString,
};
use rand_core::{OsRng, RngCore};
use sha2::{Digest, Sha256};
use zeroize::Zeroize;

const MIN_PASSWORD_BYTES: usize = 12;
const MAX_PASSWORD_BYTES: usize = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuthError {
    InvalidEmail,
    InvalidPassword,
    InvalidDeviceLabel,
    HashingFailed,
}

impl fmt::Display for AuthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::InvalidEmail => "invalid login email",
            Self::InvalidPassword => "password must be between 12 and 1024 bytes",
            Self::InvalidDeviceLabel => "device label must be between 1 and 120 characters",
            Self::HashingFailed => "password hashing failed",
        };
        f.write_str(message)
    }
}
impl Error for AuthError {}

/// Minimal canonical identity for this foundation. No non-ASCII email
/// addresses or Unicode confusable normalization until it can be defined
/// and tested alongside account verification.
pub fn normalize_email(input: &str) -> Result<String, AuthError> {
    let email = input.trim();
    if email.len() < 5
        || email.len() > 254
        || !email.is_ascii()
        || email
            .chars()
            .any(|c| c.is_ascii_whitespace() || c.is_ascii_control())
    {
        return Err(AuthError::InvalidEmail);
    }
    let Some((local, domain)) = email.split_once('@') else {
        return Err(AuthError::InvalidEmail);
    };
    if local.is_empty()
        || local.len() > 64
        || domain.len() < 3
        || domain.starts_with('.')
        || domain.ends_with('.')
        || !domain.contains('.')
        || domain.contains('@')
        || local.contains("..")
        || domain.contains("..")
    {
        return Err(AuthError::InvalidEmail);
    }
    Ok(email.to_ascii_lowercase())
}

pub fn validate_device_label(input: &str) -> Result<String, AuthError> {
    let label = input.trim();
    if label.is_empty() || label.chars().count() > 120 || label.chars().any(char::is_control) {
        return Err(AuthError::InvalidDeviceLabel);
    }
    Ok(label.to_owned())
}

/// OWASP-aligned Argon2id parameters (19 MiB, 2 iterations, p=1).
fn hasher() -> Result<Argon2<'static>, AuthError> {
    let params = Params::new(19 * 1024, 2, 1, None).map_err(|_| AuthError::HashingFailed)?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
}

pub fn hash_password(password: &str) -> Result<String, AuthError> {
    if !(MIN_PASSWORD_BYTES..=MAX_PASSWORD_BYTES).contains(&password.len()) {
        return Err(AuthError::InvalidPassword);
    }
    let salt = SaltString::generate(&mut OsRng);
    hasher()?
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AuthError::HashingFailed)
}

/// Verifies the stored PHC parameters, enabling future cost upgrades.
/// Malformed hashes are rejected without revealing any secret.
pub fn verify_password(password: &str, encoded: &str) -> bool {
    if password.len() > MAX_PASSWORD_BYTES {
        return false;
    }
    let Ok(hash) = PasswordHash::new(encoded) else {
        return false;
    };
    hasher().is_ok_and(|argon| argon.verify_password(password.as_bytes(), &hash).is_ok())
}

/// A bearer credential whose Debug output and Drop behavior avoid leaking
/// secret material in logs or long-lived memory. Handle with care when it
/// must be serialized or transported.
pub struct SessionToken(String);

impl SessionToken {
    pub fn generate() -> Self {
        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);
        let token = Self(hex::encode(secret));
        secret.zeroize();
        token
    }

    /// Explicit secret access for the credential recipient only.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Store only the digest, never the plaintext token.
    pub fn digest(&self) -> [u8; 32] {
        token_digest(self.expose()).expect("generated tokens are always valid")
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionToken([REDACTED])")
    }
}
impl Drop for SessionToken {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

/// Invalid bearer token strings are rejected before querying storage.
pub fn token_digest(token: &str) -> Option<[u8; 32]> {
    if token.len() != 64
        || !token
            .bytes()
            .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    {
        return None;
    }
    let digest = Sha256::digest(token.as_bytes());
    Some(digest.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hashes_are_unique_and_strong() {
        let a = hash_password("a good long password").unwrap();
        let b = hash_password("a good long password").unwrap();
        assert_ne!(a, b);
        assert!(a.starts_with("$argon2id$v=19$m=19456,t=2,p=1$"));
        assert!(verify_password("a good long password", &a));
        assert!(!verify_password("different password", &a));
        assert!(!verify_password("anything here", "invalid stored hash"));
        assert!(!verify_password(&"x".repeat(1025), &a));
        assert_eq!(
            hash_password("too short").unwrap_err(),
            AuthError::InvalidPassword
        );
    }

    #[test]
    fn tokens_are_random_hashed_and_redacted() {
        let a = SessionToken::generate();
        let b = SessionToken::generate();
        assert_ne!(a.expose(), b.expose());
        assert_eq!(a.expose().len(), 64);
        assert_ne!(a.digest(), b.digest());
        assert_eq!(a.digest(), token_digest(a.expose()).unwrap());
        assert_eq!(format!("{a:?}"), "SessionToken([REDACTED])");
        for bad in [
            "",
            "a".repeat(63).as_str(),
            &"Z".repeat(64),
            &"A".repeat(64),
        ] {
            assert!(token_digest(bad).is_none());
        }
    }

    #[test]
    fn canonical_identity_and_device_validation() {
        assert_eq!(
            normalize_email("  Elie@Example.COM ").unwrap(),
            "elie@example.com"
        );
        assert_eq!(normalize_email("a@example.test").unwrap(), "a@example.test");
        for bad in [
            "",
            "bad",
            "a@@example.com",
            "a@localhost",
            "a@.example.com",
            "a b@example.com",
            "é@example.com",
        ] {
            assert_eq!(normalize_email(bad), Err(AuthError::InvalidEmail));
        }
        assert_eq!(validate_device_label(" ThinkPad ").unwrap(), "ThinkPad");
        assert_eq!(
            validate_device_label(" \t"),
            Err(AuthError::InvalidDeviceLabel)
        );
        assert_eq!(
            validate_device_label(&"x".repeat(121)),
            Err(AuthError::InvalidDeviceLabel)
        );
    }
}
