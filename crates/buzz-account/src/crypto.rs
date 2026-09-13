//! Secrets handling: identity-key sealing under the master key, login-code and
//! session-token generation, and the hashes stored in place of both.

use aes_gcm::{
    aead::{rand_core::RngCore, Aead, KeyInit, OsRng},
    Aes256Gcm, Key, Nonce,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Bytes of a stored AES-GCM nonce.
pub const NONCE_LEN: usize = 12;

/// Sealing failure. Deliberately opaque: the only cause is a tampered row or
/// the wrong master key, and neither should leak detail to callers.
#[derive(Debug, Error)]
#[error("identity key could not be unsealed")]
pub struct UnsealError;

/// Encrypts identity keys at rest under `ACCOUNT_MASTER_KEY`.
#[derive(Clone)]
pub struct KeyVault {
    cipher: Aes256Gcm,
}

impl KeyVault {
    /// Build a vault from the 32-byte master key.
    pub fn new(master_key: &[u8; 32]) -> Self {
        Self {
            cipher: Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(master_key)),
        }
    }

    /// Seal a secret key. The account pubkey is bound as associated data so a
    /// ciphertext cannot be moved between account rows.
    pub fn seal(&self, secret_key: &[u8; 32], pubkey_hex: &str) -> (Vec<u8>, [u8; NONCE_LEN]) {
        let mut nonce = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = self
            .cipher
            .encrypt(
                Nonce::from_slice(&nonce),
                aes_gcm::aead::Payload {
                    msg: secret_key,
                    aad: pubkey_hex.as_bytes(),
                },
            )
            // AES-GCM encryption of 32 bytes cannot fail; the Result exists for
            // output-buffer errors that a Vec sink never produces.
            .unwrap_or_default();
        (ciphertext, nonce)
    }

    /// Unseal a secret key previously produced by [`KeyVault::seal`].
    pub fn open(
        &self,
        ciphertext: &[u8],
        nonce: &[u8],
        pubkey_hex: &str,
    ) -> Result<[u8; 32], UnsealError> {
        if nonce.len() != NONCE_LEN {
            return Err(UnsealError);
        }
        let plaintext = self
            .cipher
            .decrypt(
                Nonce::from_slice(nonce),
                aes_gcm::aead::Payload {
                    msg: ciphertext,
                    aad: pubkey_hex.as_bytes(),
                },
            )
            .map_err(|_| UnsealError)?;
        plaintext.try_into().map_err(|_| UnsealError)
    }
}

/// Fresh 6-digit login code, uniformly distributed (rejection sampling).
pub fn new_login_code() -> String {
    const LIMIT: u32 = u32::MAX - (u32::MAX % 1_000_000);
    loop {
        let n = OsRng.next_u32();
        if n < LIMIT {
            return format!("{:06}", n % 1_000_000);
        }
    }
}

/// Fresh opaque session token (32 random bytes, URL-safe base64).
pub fn new_session_token() -> String {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Stored form of a login code, bound to the email it was issued for.
pub fn hash_login_code(email: &str, code: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"buzz-account-login-code-v1:");
    h.update(email.as_bytes());
    h.update(b":");
    h.update(code.as_bytes());
    h.finalize().into()
}

/// Stored form of a session token.
pub fn hash_session_token(token: &str) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(b"buzz-account-session-v1:");
    h.update(token.as_bytes());
    h.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seal_then_open_round_trips_and_binds_pubkey() {
        let vault = KeyVault::new(&[9u8; 32]);
        let secret = [3u8; 32];
        let (ct, nonce) = vault.seal(&secret, "aa");
        assert_ne!(&ct[..32], &secret[..]);
        assert_eq!(vault.open(&ct, &nonce, "aa").expect("opens"), secret);
        assert!(vault.open(&ct, &nonce, "bb").is_err());
        assert!(KeyVault::new(&[8u8; 32]).open(&ct, &nonce, "aa").is_err());
    }

    #[test]
    fn login_codes_are_six_digits() {
        for _ in 0..1000 {
            let code = new_login_code();
            assert_eq!(code.len(), 6);
            assert!(code.bytes().all(|b| b.is_ascii_digit()));
        }
    }

    #[test]
    fn code_hash_depends_on_email() {
        assert_ne!(
            hash_login_code("a@x.com", "123456"),
            hash_login_code("b@x.com", "123456")
        );
    }
}
