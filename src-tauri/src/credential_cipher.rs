//! Authenticated encryption for model credentials persisted in the application database.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use zeroize::{Zeroize, Zeroizing};

use crate::error::{AppError, AppResult};

pub const ALGORITHM: &str = "AES-256-GCM-file-v1";
pub const MASTER_KEY_FILE: &str = "model-credential.key";
const AAD: &[u8] = b"topic-desk-studio:model-api-key:file-v1";

/// Opaque encrypted payload stored in SQLite; it never contains reusable key material.
#[derive(Debug, Clone)]
pub struct EncryptedCredential {
    pub ciphertext: Vec<u8>,
    pub nonce: Vec<u8>,
}

/// Process-local cipher whose master key is erased when the short-lived value is dropped.
pub struct CredentialCipher {
    key: Zeroizing<[u8; 32]>,
}

impl CredentialCipher {
    /// Load the app-private key file without contacting the operating-system credential vault.
    pub fn load_existing(path: &Path) -> AppResult<Option<Self>> {
        match fs::read(path) {
            Ok(bytes) => decode_master_key(bytes).map(Some),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    /// Load or create a random app-private key file with owner-only permissions where supported.
    pub fn load_or_create(path: &Path) -> AppResult<Self> {
        if let Some(cipher) = Self::load_existing(path)? {
            return Ok(cipher);
        }
        let mut key = [0_u8; 32];
        use aes_gcm::aead::rand_core::RngCore;
        OsRng.fill_bytes(&mut key);
        if !write_new_key(path, &key)? {
            key.zeroize();
            return Self::load_existing(path)?
                .ok_or_else(|| AppError::Credential("模型凭据密钥文件创建后不可读取".into()));
        }
        let cipher = Self {
            key: Zeroizing::new(key),
        };
        key.zeroize();
        Ok(cipher)
    }

    /// Encrypt a non-empty API key with a fresh nonce and bind it to this application purpose.
    pub fn encrypt(&self, plaintext: &str) -> AppResult<EncryptedCredential> {
        let plaintext = plaintext.trim();
        if plaintext.is_empty() {
            return Err(AppError::InvalidInput("API Key 不能为空".into()));
        }
        let mut nonce = [0_u8; 12];
        use aes_gcm::aead::rand_core::RngCore;
        OsRng.fill_bytes(&mut nonce);
        let ciphertext = self
            .cipher()
            .encrypt(
                Nonce::from_slice(&nonce),
                aes_gcm::aead::Payload {
                    msg: plaintext.as_bytes(),
                    aad: AAD,
                },
            )
            .map_err(|_| AppError::Credential("模型 API Key 加密失败".into()))?;
        Ok(EncryptedCredential {
            ciphertext,
            nonce: nonce.to_vec(),
        })
    }

    /// Decrypt and authenticate a database payload into memory that zeroes itself on drop.
    pub fn decrypt(&self, encrypted: &EncryptedCredential) -> AppResult<Zeroizing<String>> {
        if encrypted.nonce.len() != 12 {
            return Err(AppError::Credential("模型 API Key 的随机数长度无效".into()));
        }
        let plaintext = self
            .cipher()
            .decrypt(
                Nonce::from_slice(&encrypted.nonce),
                aes_gcm::aead::Payload {
                    msg: &encrypted.ciphertext,
                    aad: AAD,
                },
            )
            .map_err(|_| AppError::Credential("模型 API Key 解密或完整性校验失败".into()))?;
        String::from_utf8(plaintext)
            .map(Zeroizing::new)
            .map_err(|_| AppError::Credential("模型 API Key 解密结果不是 UTF-8".into()))
    }

    fn cipher(&self) -> Aes256Gcm {
        Aes256Gcm::new_from_slice(self.key.as_ref()).expect("AES-256 key length is fixed")
    }
}

fn write_new_key(path: &Path, key: &[u8; 32]) -> AppResult<bool> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(path) {
        Ok(mut file) => {
            file.write_all(key)?;
            file.sync_all()?;
            Ok(true)
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(false),
        Err(error) => Err(error.into()),
    }
}

fn decode_master_key(mut bytes: Vec<u8>) -> AppResult<CredentialCipher> {
    let mut key: [u8; 32] = bytes
        .as_slice()
        .try_into()
        .map_err(|_| AppError::Credential("模型凭据主密钥长度无效".into()))?;
    bytes.zeroize();
    let cipher = CredentialCipher {
        key: Zeroizing::new(key),
    };
    key.zeroize();
    Ok(cipher)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Authenticated encryption must round-trip while producing no plaintext bytes in storage.
    #[test]
    fn encrypts_and_decrypts_api_key() {
        let cipher = CredentialCipher {
            key: Zeroizing::new([7_u8; 32]),
        };
        let encrypted = cipher.encrypt("test-secret").expect("key should encrypt");
        assert!(!encrypted
            .ciphertext
            .windows(b"test-secret".len())
            .any(|window| window == b"test-secret"));
        assert_eq!(
            cipher
                .decrypt(&encrypted)
                .expect("key should decrypt")
                .as_str(),
            "test-secret"
        );
    }

    /// Changing authenticated ciphertext must make decryption fail rather than return garbage.
    #[test]
    fn rejects_tampered_ciphertext() {
        let cipher = CredentialCipher {
            key: Zeroizing::new([9_u8; 32]),
        };
        let mut encrypted = cipher.encrypt("test-secret").expect("key should encrypt");
        encrypted.ciphertext[0] ^= 1;
        assert!(cipher.decrypt(&encrypted).is_err());
    }
}
