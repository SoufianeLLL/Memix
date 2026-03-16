use anyhow::{anyhow, Result};
use ring::aead::{self, Aad, BoundKey, Nonce, NonceSequence, OpeningKey, SealingKey, UnboundKey};
use ring::rand::{SecureRandom, SystemRandom};
use std::env;

pub struct CryptoEngine {
    key_bytes: Vec<u8>,
}

#[allow(dead_code)]
struct RandomNonceSequence {
    rng: SystemRandom,
}

impl NonceSequence for RandomNonceSequence {
    fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
        let mut nonce_bytes = [0u8; 12];
        self.rng.fill(&mut nonce_bytes)?;
        Nonce::try_assume_unique_for_key(&nonce_bytes)
    }
}

struct OneTimeNonce {
    nonce: Option<Nonce>,
}

impl NonceSequence for OneTimeNonce {
    fn advance(&mut self) -> Result<Nonce, ring::error::Unspecified> {
        self.nonce.take().ok_or(ring::error::Unspecified)
    }
}

impl CryptoEngine {
    /// World-class AES-256-GCM instantiation.
    /// In production, `key_material` should be provided by a secure KMS or `.env` rotation.
    pub fn new(key_material: &[u8]) -> Result<Self> {
        if key_material.len() != 32 {
            return Err(anyhow!(
                "AES-256-GCM requires exactly 32 bytes of key material."
            ));
        }
        Ok(Self {
            key_bytes: key_material.to_vec(),
        })
    }

    /// Creates a new CryptoEngine from environment variable MEMIX_ENCRYPTION_KEY
    /// Falls back to deriving key from machine-specific secret if not set
    pub fn from_environment() -> Result<Self> {
        if let Ok(key) = env::var("MEMIX_ENCRYPTION_KEY") {
            let key_bytes = key.as_bytes();
            if key_bytes.len() == 32 {
                return Self::new(key_bytes);
            }
            let hash = ring::digest::digest(&ring::digest::SHA256, key_bytes);
            return Self::new(hash.as_ref());
        }

        Self::from_machine_secret()
    }

    /// Derives encryption key from machine-specific secret
    /// Uses machine ID + CPU info to create a unique but reproducible key
    fn from_machine_secret() -> Result<Self> {
        let machine_id = Self::get_machine_id()?;
        let key_material = ring::digest::digest(&ring::digest::SHA256, machine_id.as_bytes());
        Self::new(key_material.as_ref())
    }

    fn get_machine_id() -> Result<String> {
        let id_file = dirs::home_dir()
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join(".memix")
            .join("machine_id");
        if let Ok(id) = std::fs::read_to_string(&id_file) {
            return Ok(id.trim().to_string());
        }
        let new_id = uuid::Uuid::new_v4().to_string();
        if let Some(parent) = id_file.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(&id_file, &new_id);
        Ok(new_id)
    }

    /// Generates a new random encryption key (for first-time setup)
    pub fn generate_key() -> Result<[u8; 32]> {
        let rng = SystemRandom::new();
        let mut key = [0u8; 32];
        rng.fill(&mut key)
            .map_err(|_| anyhow!("Failed to generate random key"))?;
        Ok(key)
    }

    /// Derives a key from a user-provided password using PBKDF2
    pub fn derive_from_password(password: &str, salt: &[u8]) -> Result<[u8; 32]> {
        use ring::pbkdf2;

        let mut key = [0u8; 32];
        pbkdf2::derive(
            pbkdf2::PBKDF2_HMAC_SHA256,
            std::num::NonZeroU32::new(100_000).unwrap(),
            salt,
            password.as_bytes(),
            &mut key,
        );
        Ok(key)
    }

    /// Gets the current key (for debugging/backup purposes - returns hash not actual key)
    pub fn get_key_fingerprint(&self) -> String {
        let hash = ring::digest::digest(&ring::digest::SHA256, &self.key_bytes);
        hex::encode(hash)
    }    
    
    /// Safely seals the memory payload, generating a non-deterministic 12-byte nonce dynamically prepended to the ciphertext.
    pub fn encrypt(&self, plaintext: &[u8]) -> Result<Vec<u8>> {
        let rng = SystemRandom::new();
        let mut nonce_bytes = [0u8; 12];
        rng.fill(&mut nonce_bytes)
            .map_err(|_| anyhow::anyhow!("Failed to generate secure nonce"))?;
        let nonce = Nonce::try_assume_unique_for_key(&nonce_bytes)
            .map_err(|_| anyhow::anyhow!("Invalid Nonce generation"))?;

        let unbound_key = UnboundKey::new(&aead::AES_256_GCM, &self.key_bytes)
            .map_err(|_| anyhow::anyhow!("Failed to bind AES key"))?;

        let ot_nonce = OneTimeNonce { nonce: Some(nonce) };
        let mut sealing_key = SealingKey::new(unbound_key, ot_nonce);

        let mut in_out = plaintext.to_vec();
        sealing_key
            .seal_in_place_append_tag(Aad::empty(), &mut in_out)
            .map_err(|_| anyhow::anyhow!("Encryption failed during AES-GCM tag sealing"))?;

        // Prepend nonce to the securely sealed ciphertext block
        let mut final_payload = nonce_bytes.to_vec();
        final_payload.extend(in_out);

        Ok(final_payload)
    }

    /// Decrypts the resting memory payload by stripping the 12-byte nonce and verifying the GCM authentication tag.
    pub fn decrypt(&self, encrypted_payload: &[u8]) -> Result<Vec<u8>> {
        if encrypted_payload.len() < 12 {
            return Err(anyhow!("Payload too short to contain AES-GCM Nonce"));
        }

        let (nonce_bytes, ciphertext_with_tag) = encrypted_payload.split_at(12);

        let nonce = Nonce::try_assume_unique_for_key(nonce_bytes)
            .map_err(|_| anyhow::anyhow!("Stored Nonce byte structure invalid"))?;

        let unbound_key = UnboundKey::new(&aead::AES_256_GCM, &self.key_bytes)
            .map_err(|_| anyhow::anyhow!("Failed to bind AES key"))?;

        let ot_nonce = OneTimeNonce { nonce: Some(nonce) };
        let mut opening_key = OpeningKey::new(unbound_key, ot_nonce);

        let mut in_out = ciphertext_with_tag.to_vec();
        let decrypted_data = opening_key
            .open_in_place(Aad::empty(), &mut in_out)
            .map_err(|_| {
                anyhow::anyhow!(
                    "Decryption failed: cryptographic integrity check invalid or key mismatch"
                )
            })?;

        Ok(decrypted_data.to_vec())
    }
}
