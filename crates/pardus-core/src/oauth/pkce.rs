//! PKCE (Proof Key for Code Exchange) generation for OAuth 2.0.
//!
//! Implements S256 code challenge method per RFC 7636:
//! code_challenge = BASE64URL(SHA256(code_verifier))

use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use sha2::{Digest, Sha256};

/// PKCE parameters generated at the start of each authorization flow.
#[derive(Debug, Clone)]
pub struct PkcePair {
    /// Cryptographically random 43-char string (unreserved ASCII chars).
    pub code_verifier: String,
    /// BASE64URL(SHA256(code_verifier)).
    pub code_challenge: String,
}

/// Unreserved characters per RFC 3986 §2.3, used for code_verifier.
const VERIFIER_CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";

impl PkcePair {
    /// Generate a new PKCE pair with a 43-char random code_verifier
    /// and the corresponding S256 code_challenge.
    pub fn generate() -> Self {
        let verifier = Self::generate_verifier(43);
        let challenge = Self::compute_challenge(&verifier);
        Self {
            code_verifier: verifier,
            code_challenge: challenge,
        }
    }

    /// Generate a cryptographically random code_verifier of the given length.
    fn generate_verifier(len: usize) -> String {
        let mut buf = vec![0u8; len];
        getrandom::fill(&mut buf).expect("failed to generate random bytes for PKCE verifier");
        buf.iter()
            .map(|b| VERIFIER_CHARSET[*b as usize % VERIFIER_CHARSET.len()] as char)
            .collect()
    }

    /// Compute BASE64URL(SHA256(input)) per RFC 7636 §4.2.
    pub fn compute_challenge(verifier: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(verifier.as_bytes());
        let hash = hasher.finalize();
        URL_SAFE_NO_PAD.encode(hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_verifier_length() {
        let pair = PkcePair::generate();
        assert_eq!(pair.code_verifier.len(), 43);
    }

    #[test]
    fn pkce_verifier_contains_only_unreserved_chars() {
        let pair = PkcePair::generate();
        for ch in pair.code_verifier.chars() {
            assert!(
                VERIFIER_CHARSET.contains(&(ch as u8)),
                "invalid char in verifier: {ch}"
            );
        }
    }

    #[test]
    fn pkce_challenge_is_base64url_sha256() {
        let pair = PkcePair::generate();
        let expected = PkcePair::compute_challenge(&pair.code_verifier);
        assert_eq!(pair.code_challenge, expected);
    }

    #[test]
    fn pkce_uniqueness() {
        let a = PkcePair::generate();
        let b = PkcePair::generate();
        assert_ne!(a.code_verifier, b.code_verifier);
        assert_ne!(a.code_challenge, b.code_challenge);
    }
}
