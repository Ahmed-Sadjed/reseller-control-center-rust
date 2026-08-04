use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use argon2::password_hash::{
    rand_core::{OsRng, RngCore},
    PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;

pub fn hash_password(password: &str) -> Result<String, argon2::password_hash::Error> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
}

pub fn verify_password(password: &str, hash: &str) -> Result<bool, argon2::password_hash::Error> {
    let parsed = PasswordHash::new(hash)?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

/// AES-256-GCM encrypt. Output: 12-byte nonce || ciphertext.
pub fn encrypt(plaintext: &[u8], key: &[u8]) -> Result<Vec<u8>, aes_gcm::Error> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| aes_gcm::Error)?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher.encrypt(nonce, plaintext)?;
    let mut out = Vec::with_capacity(nonce_bytes.len() + ciphertext.len());
    out.extend_from_slice(&nonce_bytes);
    out.extend_from_slice(&ciphertext);
    Ok(out)
}

/// AES-256-GCM decrypt of `nonce || ciphertext` produced by `encrypt`.
pub fn decrypt(ciphertext: &[u8], key: &[u8]) -> Result<Vec<u8>, aes_gcm::Error> {
    let cipher = Aes256Gcm::new_from_slice(key).map_err(|_| aes_gcm::Error)?;
    if ciphertext.len() < 12 {
        return Err(aes_gcm::Error);
    }
    let (nonce_bytes, payload) = ciphertext.split_at(12);
    let nonce = Nonce::from_slice(nonce_bytes);
    cipher.decrypt(nonce, payload)
}

/// Decrypt a provider `api_token` stored in BYTEA. New rows are `encrypt()`
/// output; legacy rows hold plaintext UTF-8. Falls back to plaintext when the
/// stored bytes are not valid encrypted output.
pub fn decrypt_api_token(stored: &[u8], key: &[u8]) -> Option<String> {
    if stored.is_empty() {
        return None;
    }
    if let Ok(plain) = decrypt(stored, key) {
        if let Ok(text) = String::from_utf8(plain) {
            return Some(text);
        }
    }
    String::from_utf8(stored.to_vec()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8; 32] = b"0123456789abcdef0123456789abcdef";

    #[test]
    fn password_hash_verify_roundtrip() {
        let hash = hash_password("hunter2").unwrap();
        assert!(verify_password("hunter2", &hash).unwrap());
        assert!(!verify_password("wrong", &hash).unwrap());
        assert!(!verify_password("", &hash).unwrap());
    }

    #[test]
    fn hash_outputs_are_salted() {
        let a = hash_password("same-pass").unwrap();
        let b = hash_password("same-pass").unwrap();
        assert_ne!(a, b, "argon2 must use a random salt per hash");
    }

    #[test]
    fn encrypt_decrypt_roundtrip() {
        let cipher = encrypt(b"credential-password", KEY).unwrap();
        assert_ne!(cipher, b"credential-password");
        assert!(cipher.len() > 12, "nonce || ciphertext layout");
        let plain = decrypt(&cipher, KEY).unwrap();
        assert_eq!(plain, b"credential-password");
    }

    #[test]
    fn decrypt_with_wrong_key_fails() {
        let cipher = encrypt(b"secret", KEY).unwrap();
        let other: &[u8; 32] = b"fedcba9876543210fedcba9876543210";
        assert!(decrypt(&cipher, other).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let mut cipher = encrypt(b"secret", KEY).unwrap();
        let last = cipher.len() - 1;
        cipher[last] ^= 0xFF;
        assert!(decrypt(&cipher, KEY).is_err());
    }

    #[test]
    fn short_input_fails_cleanly() {
        assert!(decrypt(b"too-short", KEY).is_err());
    }

    #[test]
    fn decrypt_api_token_roundtrip_and_fallback() {
        let token = "some-reseller-token";
        let encrypted = encrypt(token.as_bytes(), KEY).unwrap();
        assert_eq!(decrypt_api_token(&encrypted, KEY).as_deref(), Some(token));
        assert_eq!(
            decrypt_api_token(token.as_bytes(), KEY).as_deref(),
            Some(token),
            "legacy plaintext rows still work"
        );
        assert_eq!(decrypt_api_token(b"", KEY), None);
        let other: &[u8; 32] = b"fedcba9876543210fedcba9876543210";
        assert_eq!(
            decrypt_api_token(&encrypted, other),
            None,
            "encrypted bytes are not valid UTF-8, fallback must not return garbage"
        );
    }
}
