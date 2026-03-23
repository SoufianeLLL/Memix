use anyhow::{anyhow, Result};
use base64::{engine::general_purpose::STANDARD, Engine as _};
use chrono::Utc;
use ring::hmac;
use ring::signature::{UnparsedPublicKey, ED25519};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LicenseTier {
    Solo,
    Pro,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePayload {
    pub email: String,
    pub tier: LicenseTier,
    pub seats: u32,
    pub issued_at: i64,
    pub expires_at: i64,
    pub subscription_id: String,
    pub nonce: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseBundle {
    pub payload: String,
    pub signature: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseCache {
    pub raw_key: String,
    pub payload: LicensePayload,
    pub server_checked_at: i64,
    pub hmac: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseRuntimeStatus {
    pub available: bool,
    pub active: bool,
    pub tier: Option<LicenseTier>,
    pub email: Option<String>,
    pub seats: Option<u32>,
    pub expires_at: Option<i64>,
    pub mode: Option<String>,
    pub message: Option<String>,
    pub grace_until: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LicenseServerStatusResponse {
    pub active: bool,
    #[serde(default)]
    pub tier: Option<LicenseTier>,
    #[serde(default)]
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseInitiateResult {
    pub license_exists: bool,
    pub token: Option<String>,
    pub checkout_url: Option<String>,
    pub key: Option<String>,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicensePendingResult {
    pub ready: bool,
    pub key: Option<String>,
    pub message: Option<String>,
}

pub struct LicenseValidator {
    public_key_bytes: Option<Vec<u8>>,
    cache_path: PathBuf,
    license_server_url: Option<String>,
    http_client: reqwest::Client,
}

impl LicenseValidator {
    pub const CACHE_FRESH_SECONDS: i64 = 60 * 60 * 24;
    pub const GRACE_PERIOD_SECONDS: i64 = 60 * 60 * 24 * 7;

    pub fn new(public_key_b64: Option<String>, license_server_url: Option<String>, cache_path: PathBuf) -> Result<Self> {
        let public_key_bytes = match public_key_b64 {
            Some(value) if !value.trim().is_empty() => Some(
                STANDARD
                    .decode(value.trim())
                    .map_err(|e| anyhow!("invalid license_public_key: {}", e))?,
            ),
            _ => None,
        };
        let license_server_url = license_server_url
            .map(|value| value.trim().trim_end_matches('/').to_string())
            .filter(|value| !value.is_empty());

        if cache_path.components().any(|c| c == std::path::Component::ParentDir) {
            return Err(anyhow!("Invalid cache path: path traversal is not permitted"));
        }

        Ok(Self {
            public_key_bytes,
            cache_path,
            license_server_url,
            http_client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(10))
                .build()?,
        })
    }

    pub fn is_available(&self) -> bool {
        self.public_key_bytes.is_some()
    }

    pub fn server_base_url(&self) -> Option<&str> {
        self.license_server_url.as_deref()
    }

    pub fn http_client(&self) -> &reqwest::Client {
        &self.http_client
    }

    pub fn verify_signature(&self, key_str: &str) -> Result<LicensePayload> {
        let public_key = self
            .public_key_bytes
            .as_ref()
            .ok_or_else(|| anyhow!("license validation unavailable: MEMIX_LICENSE_PUBLIC_KEY is not configured"))?;
        let raw = Self::strip_key_formatting(key_str);
        let bundle: LicenseBundle = serde_json::from_slice(
            &STANDARD
                .decode(raw)
                .map_err(|e| anyhow!("invalid license encoding: {}", e))?,
        )?;
        let payload_bytes = STANDARD
            .decode(&bundle.payload)
            .map_err(|e| anyhow!("invalid license payload encoding: {}", e))?;
        let signature_bytes = STANDARD
            .decode(&bundle.signature)
            .map_err(|e| anyhow!("invalid license signature encoding: {}", e))?;

        UnparsedPublicKey::new(&ED25519, public_key)
            .verify(&payload_bytes, &signature_bytes)
            .map_err(|_| anyhow!("invalid license signature"))?;

        let payload: LicensePayload = serde_json::from_slice(&payload_bytes)?;
        if Utc::now().timestamp() > payload.expires_at {
            return Err(anyhow!("license key has expired"));
        }
        Ok(payload)
    }

    pub async fn activate(&self, key_str: &str, device_id: Option<&str>) -> Result<LicenseRuntimeStatus> {
        let payload = self.verify_signature(key_str)?;
        let online = self.check_server_status(&payload, device_id).await;
        match online {
            Ok(server_status) => {
                if !server_status.active {
                    return Err(anyhow!(server_status.message.unwrap_or_else(|| "license has been revoked".to_string())));
                }
                self.write_cache(key_str, &payload).await?;
                Ok(Self::status_from_payload(
                    payload,
                    "server_verified".to_string(),
                    Some("license activated".to_string()),
                    None,
                ))
            }
            Err(err) => {
                let cache = self.load_and_verify_cache(key_str).await;
                let grace_until = cache
                    .as_ref()
                    .map(|entry| entry.server_checked_at + Self::GRACE_PERIOD_SECONDS)
                    .unwrap_or_else(|| Utc::now().timestamp() + Self::GRACE_PERIOD_SECONDS);
                Ok(Self::status_from_payload(
                    payload,
                    "grace_period".to_string(),
                    Some(format!("license server unavailable, using grace period: {}", err)),
                    Some(grace_until),
                ))
            }
        }
    }

    pub async fn status_for_key(&self, key: Option<&str>, device_id: Option<&str>) -> LicenseRuntimeStatus {
        if !self.is_available() {
            return LicenseRuntimeStatus {
                available: false,
                active: false,
                tier: None,
                email: None,
                seats: None,
                expires_at: None,
                mode: None,
                message: Some("license validation unavailable: MEMIX_LICENSE_PUBLIC_KEY is not configured".to_string()),
                grace_until: None,
            };
        }

        let Some(raw_key) = key.filter(|value| !value.trim().is_empty()) else {
            return LicenseRuntimeStatus {
                available: true,
                active: false,
                tier: None,
                email: None,
                seats: None,
                expires_at: None,
                mode: None,
                message: Some("No license key configured".to_string()),
                grace_until: None,
            };
        };

        match self.verify_signature(raw_key) {
            Ok(payload) => {
                let cache = self.load_and_verify_cache(raw_key).await;
                let now = Utc::now().timestamp();
                if let Some(cache_entry) = cache.clone() {
                    let age = now - cache_entry.server_checked_at;
                    if age <= Self::CACHE_FRESH_SECONDS {
                        return Self::status_from_payload(payload, "cached".to_string(), None, None);
                    }
                }

                match self.check_server_status(&payload, device_id).await {
                    Ok(server_status) => {
                        if !server_status.active {
                            return LicenseRuntimeStatus {
                                available: true,
                                active: false,
                                tier: None,
                                email: None,
                                seats: None,
                                expires_at: None,
                                mode: Some("server_revoked".to_string()),
                                message: Some(server_status.message.unwrap_or_else(|| "license has been revoked".to_string())),
                                grace_until: None,
                            };
                        }
                        let _ = self.write_cache(raw_key, &payload).await;
                        Self::status_from_payload(payload, "server_verified".to_string(), server_status.message, None)
                    }
                    Err(err) => {
                        if let Some(cache_entry) = cache {
                            let grace_until = cache_entry.server_checked_at + Self::GRACE_PERIOD_SECONDS;
                            if now <= grace_until {
                                return Self::status_from_payload(
                                    payload,
                                    "grace_period".to_string(),
                                    Some(format!("license server unavailable, using grace period: {}", err)),
                                    Some(grace_until),
                                );
                            }
                        }
                        LicenseRuntimeStatus {
                            available: true,
                            active: false,
                            tier: None,
                            email: None,
                            seats: None,
                            expires_at: None,
                            mode: Some("verification_failed".to_string()),
                            message: Some(format!("license server unreachable and grace period expired: {}", err)),
                            grace_until: None,
                        }
                    }
                }
            }
            Err(err) => LicenseRuntimeStatus {
                available: true,
                active: false,
                tier: None,
                email: None,
                seats: None,
                expires_at: None,
                mode: None,
                message: Some(err.to_string()),
                grace_until: None,
            },
        }
    }

    async fn check_server_status(&self, payload: &LicensePayload, device_id: Option<&str>) -> Result<LicenseServerStatusResponse> {
        let Some(base_url) = &self.license_server_url else {
            return Ok(LicenseServerStatusResponse {
                active: true,
                tier: Some(payload.tier.clone()),
                message: Some("license server not configured; using offline verification".to_string()),
            });
        };

        let url = format!("{}/v1/license/status/{}", base_url, payload.subscription_id);
        let mut request = self.http_client.get(url);
        if let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) {
            request = request.query(&[("device_id", device_id)]);
        }
        let response = request.send().await?;
        if !response.status().is_success() {
            return Err(anyhow!("license status server returned HTTP {}", response.status()));
        }
        Self::parse_json(response).await
    }

    async fn parse_json<T: DeserializeOwned>(response: reqwest::Response) -> Result<T> {
        Ok(response.json::<T>().await?)
    }

    async fn load_and_verify_cache(&self, raw_key: &str) -> Option<LicenseCache> {
        let metadata = tokio::fs::metadata(&self.cache_path).await.ok()?;
        if metadata.len() > 1024 * 1024 {
            return None;
        }
        let bytes = tokio::fs::read(&self.cache_path).await.ok()?;
        let cache: LicenseCache = serde_json::from_slice(&bytes).ok()?;
        if cache.raw_key != raw_key {
            return None;
        }
        let expected = Self::cache_hmac(&cache.raw_key, &cache.payload, cache.server_checked_at).ok()?;
        if expected != cache.hmac {
            return None;
        }
        Some(cache)
    }

    async fn write_cache(&self, raw_key: &str, payload: &LicensePayload) -> Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let server_checked_at = Utc::now().timestamp();
        let cache = LicenseCache {
            raw_key: raw_key.to_string(),
            payload: payload.clone(),
            server_checked_at,
            hmac: Self::cache_hmac(raw_key, payload, server_checked_at)?,
        };
        tokio::fs::write(&self.cache_path, serde_json::to_vec_pretty(&cache)?).await?;
        Ok(())
    }

    fn cache_hmac(raw_key: &str, payload: &LicensePayload, server_checked_at: i64) -> Result<String> {
        let key = hmac::Key::new(hmac::HMAC_SHA256, raw_key.as_bytes());
        let data = serde_json::to_vec(&(raw_key, payload, server_checked_at))?;
        Ok(hex::encode(hmac::sign(&key, &data).as_ref()))
    }

    fn strip_key_formatting(key: &str) -> String {
        let trimmed = key.trim();
        let without_prefix = trimmed.strip_prefix("MEMIX-").unwrap_or(trimmed);
        without_prefix
            .chars()
            .filter(|ch| !ch.is_whitespace() && *ch != '-')
            .collect()
    }

    fn status_from_payload(payload: LicensePayload, mode: String, message: Option<String>, grace_until: Option<i64>) -> LicenseRuntimeStatus {
        LicenseRuntimeStatus {
            available: true,
            active: true,
            tier: Some(payload.tier.clone()),
            email: Some(payload.email.clone()),
            seats: Some(payload.seats),
            expires_at: Some(payload.expires_at),
            mode: Some(mode),
            message,
            grace_until,
        }
    }
}
