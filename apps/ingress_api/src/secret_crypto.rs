use aes_gcm::{
    aead::{rand_core::RngCore, Aead, OsRng},
    Aes256Gcm, KeyInit, Nonce,
};
use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use sha2::{Digest, Sha256};

const SECRET_PREFIX: &str = "enc:v1:";
const NONCE_LEN: usize = 12;

#[derive(Clone)]
pub struct SecretCipher {
    key: [u8; 32],
}

impl SecretCipher {
    pub fn from_passphrase(passphrase: &str) -> Result<Self> {
        let trimmed = passphrase.trim();
        if trimmed.is_empty() {
            return Err(anyhow!("INGRESS_CONNECTOR_SECRETS_KEY cannot be empty"));
        }

        let mut hasher = Sha256::new();
        hasher.update(trimmed.as_bytes());
        let digest = hasher.finalize();

        let mut key = [0u8; 32];
        key.copy_from_slice(&digest);

        Ok(Self { key })
    }

    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);

        let cipher = Aes256Gcm::new_from_slice(&self.key)?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = cipher
            .encrypt(nonce, plaintext.as_bytes())
            .map_err(|_| anyhow!("failed to encrypt secret"))?;

        let nonce_b64 = URL_SAFE_NO_PAD.encode(nonce_bytes);
        let ciphertext_b64 = URL_SAFE_NO_PAD.encode(ciphertext);
        Ok(format!("{SECRET_PREFIX}{nonce_b64}.{ciphertext_b64}"))
    }

    pub fn decrypt(&self, encoded: &str) -> Result<String> {
        let raw = encoded
            .strip_prefix(SECRET_PREFIX)
            .ok_or_else(|| anyhow!("invalid secret encoding prefix"))?;

        let (nonce_b64, ciphertext_b64) = raw
            .split_once('.')
            .ok_or_else(|| anyhow!("invalid secret encoding format"))?;

        let nonce_bytes = URL_SAFE_NO_PAD.decode(nonce_b64)?;
        if nonce_bytes.len() != NONCE_LEN {
            return Err(anyhow!("invalid nonce length"));
        }

        let ciphertext = URL_SAFE_NO_PAD.decode(ciphertext_b64)?;
        let cipher = Aes256Gcm::new_from_slice(&self.key)?;
        let nonce = Nonce::from_slice(&nonce_bytes);
        let plaintext = cipher
            .decrypt(nonce, ciphertext.as_ref())
            .map_err(|_| anyhow!("failed to decrypt secret"))?;
        Ok(String::from_utf8(plaintext)?)
    }
}
