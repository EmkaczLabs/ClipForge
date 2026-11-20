use std::num::NonZeroU32;

use ring::digest::{SHA256, digest};
use ring::pbkdf2;
use ring::rand::{SecureRandom, SystemRandom};
use subtle::ConstantTimeEq;

use crate::error::ServerError;

const PBKDF2_ITERATIONS: u32 = 100_000;
const SALT_LEN: usize = 16;
const HASH_LEN: usize = 32;

/// Hashes a plaintext password using PBKDF2-HMAC-SHA256 with a random 16-byte salt.
pub fn hash_password(password: &str) -> Result<String, ServerError> {
    if password.len() < 8 {
        return Err(ServerError::BadRequest(
            "password must be at least 8 characters".to_string(),
        ));
    }

    let rng = SystemRandom::new();
    let mut salt = [0u8; SALT_LEN];
    rng.fill(&mut salt)
        .map_err(|_| ServerError::Internal("failed to generate password salt".to_string()))?;

    let mut out = [0u8; HASH_LEN];
    let iterations = NonZeroU32::new(PBKDF2_ITERATIONS).expect("non-zero iterations");
    pbkdf2::derive(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        &salt,
        password.as_bytes(),
        &mut out,
    );

    Ok(format!(
        "pbkdf2$sha256${PBKDF2_ITERATIONS}${}${}",
        hex::encode(salt),
        hex::encode(out)
    ))
}

/// Verifies a plaintext password against a stored PBKDF2 hash string in constant time.
pub fn verify_password(password: &str, stored: &str) -> bool {
    let parts: Vec<&str> = stored.split('$').collect();
    if parts.len() != 5 || parts[0] != "pbkdf2" || parts[1] != "sha256" {
        return false;
    }

    let Ok(iters) = parts[2].parse::<u32>() else {
        return false;
    };
    let Some(iterations) = NonZeroU32::new(iters) else {
        return false;
    };
    let Ok(salt) = hex::decode(parts[3]) else {
        return false;
    };
    let Ok(expected) = hex::decode(parts[4]) else {
        return false;
    };

    pbkdf2::verify(
        pbkdf2::PBKDF2_HMAC_SHA256,
        iterations,
        &salt,
        password.as_bytes(),
        &expected,
    )
    .is_ok()
}

/// Generates a cryptographically random token or API key with the given prefix.
pub fn generate_secret_token(prefix: &str) -> Result<String, ServerError> {
    let rng = SystemRandom::new();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes)
        .map_err(|_| ServerError::Internal("failed to generate random token".to_string()))?;
    Ok(format!("{prefix}{}", hex::encode(bytes)))
}

/// Hashes a bearer token or API key using SHA-256 for safe database storage.
pub fn hash_token(token: &str) -> String {
    let d = digest(&SHA256, token.as_bytes());
    hex::encode(d.as_ref())
}

/// Constant-time string equality helper.
pub fn constant_time_eq(a: &str, b: &str) -> bool {
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

/// Extracts a bearer token or API key from HTTP headers (or optional query token).
pub fn extract_raw_credential(
    auth_header: Option<&str>,
    api_key_header: Option<&str>,
    query_token: Option<&str>,
) -> Option<String> {
    if let Some(auth) = auth_header {
        let trimmed = auth.trim();
        if let Some(rest) = trimmed
            .strip_prefix("Bearer ")
            .or_else(|| trimmed.strip_prefix("bearer "))
        {
            let token = rest.trim();
            if !token.is_empty() {
                return Some(token.to_string());
            }
        }
    }

    if let Some(key) = api_key_header {
        let trimmed = key.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    if let Some(tok) = query_token {
        let trimmed = tok.trim();
        if !trimmed.is_empty() {
            return Some(trimmed.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn password_hash_and_verify() {
        let hash = hash_password("correct-horse-battery-staple").expect("hash");
        assert!(verify_password("correct-horse-battery-staple", &hash));
        assert!(!verify_password("wrong-password", &hash));
    }

    #[test]
    fn token_hashing_and_constant_time() {
        let tok = generate_secret_token("cf_tok_").expect("token");
        assert!(tok.starts_with("cf_tok_"));
        let h1 = hash_token(&tok);
        let h2 = hash_token(&tok);
        assert!(constant_time_eq(&h1, &h2));
        assert!(!constant_time_eq(&h1, "0000"));
    }
}
