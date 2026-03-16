use crate::brain::schema::MemoryEntry;
use crate::brain::validator::BrainValidator;
use crate::sync::crdt::{CollaborativeBrainMatrix, CrdtEngine};
use anyhow::Result;
use crdts::{CmRDT, CvRDT, MVReg, Map, Orswot, VClock};
use redis::AsyncCommands;
use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305};
use ring::digest::{digest, SHA256};
use ring::hmac;
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamBrain {
    pub team_id: String,
    pub identity: MVReg<String, String>,
    pub patterns: MVReg<String, String>,
    pub decisions: Orswot<String, String>,
    pub known_issues: Orswot<String, String>,
    pub file_map: Map<String, MVReg<String, String>, String>,
    pub session_counter: u32,
    pub clock: VClock<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct TeamBrainSnapshot {
    pub team_id: String,
    pub identity: Vec<String>,
    pub patterns: Vec<String>,
    pub decisions: Vec<String>,
    pub known_issues: Vec<String>,
    pub file_map: BTreeMap<String, Vec<String>>,
    pub session_counter: u32,
}

impl TeamBrain {
    pub fn new(team_id: String) -> Self {
        Self {
            team_id,
            identity: MVReg::new(),
            patterns: MVReg::new(),
            decisions: Orswot::new(),
            known_issues: Orswot::new(),
            file_map: Map::new(),
            session_counter: 0,
            clock: VClock::new(),
        }
    }

    pub fn merge(&mut self, other: &TeamBrain) {
        self.identity.merge(other.identity.clone());
        self.patterns.merge(other.patterns.clone());
        self.decisions.merge(other.decisions.clone());
        self.known_issues.merge(other.known_issues.clone());
        self.file_map.merge(other.file_map.clone());

        self.session_counter = self.session_counter.max(other.session_counter);
        self.clock.merge(other.clock.clone());
    }

    pub fn set_identity(&mut self, actor_id: &str, value: String) {
        let ctx = self.identity.read().derive_add_ctx(actor_id.to_string());
        let op = self.identity.write(value, ctx);
        self.identity.apply(op);
    }

    pub fn set_patterns(&mut self, actor_id: &str, value: String) {
        let ctx = self.patterns.read().derive_add_ctx(actor_id.to_string());
        let op = self.patterns.write(value, ctx);
        self.patterns.apply(op);
    }

    pub fn add_decision(&mut self, actor_id: &str, value: String) {
        let ctx = self.decisions.read().derive_add_ctx(actor_id.to_string());
        let op = self.decisions.add(value, ctx);
        self.decisions.apply(op);
    }

    pub fn add_known_issue(&mut self, actor_id: &str, value: String) {
        let ctx = self.known_issues.read().derive_add_ctx(actor_id.to_string());
        let op = self.known_issues.add(value, ctx);
        self.known_issues.apply(op);
    }

    pub fn set_file_map_entry(&mut self, actor_id: &str, key: String, value: String) {
        let ctx = self
            .file_map
            .read_ctx()
            .derive_add_ctx(actor_id.to_string());
        let op = self.file_map.update(key, ctx, |reg, reg_ctx| reg.write(value, reg_ctx));
        self.file_map.apply(op);
    }

    pub fn bump_session_counter(&mut self) {
        self.session_counter = self.session_counter.saturating_add(1);
    }

    pub fn snapshot(&self) -> TeamBrainSnapshot {
        let mut identity = self.identity.read().val.into_iter().collect::<Vec<_>>();
        identity.sort();
        identity.dedup();

        let mut patterns = self.patterns.read().val.into_iter().collect::<Vec<_>>();
        patterns.sort();
        patterns.dedup();

        let mut decisions = self.decisions.read().val.into_iter().collect::<Vec<_>>();
        decisions.sort();
        decisions.dedup();

        let mut known_issues = self.known_issues.read().val.into_iter().collect::<Vec<_>>();
        known_issues.sort();
        known_issues.dedup();

        let file_map = BTreeMap::new();

        TeamBrainSnapshot {
            team_id: self.team_id.clone(),
            identity,
            patterns,
            decisions,
            known_issues,
            file_map,
            session_counter: self.session_counter,
        }
    }
}

pub struct TeamManager {
    client: redis::Client,
    actor_id: String,
    shared_secret: String,
    crdt: CrdtEngine,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TeamOperationEnvelope {
    team_id: String,
    project_id: String,
    actor_id: String,
    sequence: i64,
    generated_at_unix: i64,
    operation: TeamOperation,
    signature: String,
}

impl TeamManager {
    const TRANSPORT_TTL_SECONDS: i64 = 60 * 60 * 24 * 30;
    const FINGERPRINT_SCHEMA_VERSION: &'static str = "crdt_v1";

    pub fn new(client: redis::Client, actor_id: String, shared_secret: String) -> Self {
        let crdt = CrdtEngine::new(actor_id.clone());
        Self {
            client,
            actor_id,
            shared_secret,
            crdt,
        }
    }

    fn namespace(team_id: &str, project_id: &str) -> String {
        format!("team:{}:project:{}", team_id, project_id)
    }

    fn ops_key(team_id: &str, project_id: &str) -> String {
        format!("{}:ops", Self::namespace(team_id, project_id))
    }

    fn seq_key(team_id: &str, project_id: &str) -> String {
        format!("{}:seq", Self::namespace(team_id, project_id))
    }

    fn cursor_key(team_id: &str, project_id: &str, actor_id: &str) -> String {
        format!("{}:cursor:{}", Self::namespace(team_id, project_id), actor_id)
    }

    fn fingerprint_key(team_id: &str, project_id: &str, actor_id: &str) -> String {
        format!("{}:fingerprints:{}", Self::namespace(team_id, project_id), actor_id)
    }

    fn team_brain_key(team_id: &str, project_id: &str) -> String {
        format!("{}:team_brain", Self::namespace(team_id, project_id))
    }

    fn team_brain_file_map_key(team_id: &str, project_id: &str) -> String {
        format!("{}:team_brain_file_map", Self::namespace(team_id, project_id))
    }

    fn entry_projection_key(team_id: &str, project_id: &str) -> String {
        format!("{}:entry_projection", Self::namespace(team_id, project_id))
    }

	fn crdt_state_key(team_id: &str, project_id: &str) -> String {
		format!("{}:crdt_state", Self::namespace(team_id, project_id))
	}

    fn sanitize_entry(entry: &MemoryEntry) -> MemoryEntry {
        let validator = BrainValidator::new();
        let mut sanitized = entry.clone();
        sanitized.content = validator.sanitize_content(&sanitized.content);
        sanitized.access_count = 0;
        sanitized.last_accessed_at = None;
        sanitized
    }

    fn sign_payload(&self, team_id: &str, project_id: &str, actor_id: &str, sequence: i64, operation: &TeamOperation) -> Result<String> {
        let canonical = serde_json::to_vec(&(team_id, project_id, actor_id, sequence, operation))?;
        let key = hmac::Key::new(hmac::HMAC_SHA256, self.shared_secret.as_bytes());
        Ok(hex::encode(hmac::sign(&key, &canonical).as_ref()))
    }

    fn encryption_key(&self) -> Result<LessSafeKey> {
        let digest = digest(&SHA256, self.shared_secret.as_bytes());
        let unbound = UnboundKey::new(&CHACHA20_POLY1305, digest.as_ref())
            .map_err(|_| anyhow::anyhow!("failed to derive team sync encryption key"))?;
        Ok(LessSafeKey::new(unbound))
    }

    fn encrypt_payload(&self, plaintext: &[u8]) -> Result<String> {
        let key = self.encryption_key()?;
        let rng = SystemRandom::new();
        let mut nonce_bytes = [0u8; 12];
        rng.fill(&mut nonce_bytes)
            .map_err(|_| anyhow::anyhow!("failed to generate team sync nonce"))?;
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut buffer = plaintext.to_vec();
        key.seal_in_place_append_tag(nonce, Aad::empty(), &mut buffer)
            .map_err(|_| anyhow::anyhow!("failed to encrypt team sync payload"))?;
        let envelope = EncryptedTransportEnvelope {
            version: 1,
            nonce_hex: hex::encode(nonce_bytes),
            ciphertext_hex: hex::encode(buffer),
        };
        Ok(serde_json::to_string(&envelope)?)
    }

    fn decrypt_payload(&self, payload: &str) -> Result<Vec<u8>> {
        let envelope: EncryptedTransportEnvelope = serde_json::from_str(payload)?;
        if envelope.version != 1 {
            return Err(anyhow::anyhow!("unsupported transport envelope version {}", envelope.version));
        }
        let nonce_vec = hex::decode(&envelope.nonce_hex)?;
        if nonce_vec.len() != 12 {
            return Err(anyhow::anyhow!("invalid transport nonce length"));
        }
        let mut nonce_bytes = [0u8; 12];
        nonce_bytes.copy_from_slice(&nonce_vec);
        let nonce = Nonce::assume_unique_for_key(nonce_bytes);
        let mut ciphertext = hex::decode(&envelope.ciphertext_hex)?;
        let key = self.encryption_key()?;
        let plaintext = key
            .open_in_place(nonce, Aad::empty(), &mut ciphertext)
            .map_err(|_| anyhow::anyhow!("failed to decrypt team sync payload"))?;
        Ok(plaintext.to_vec())
    }

    fn verify_envelope(&self, envelope: &TeamOperationEnvelope) -> bool {
        match self.sign_payload(
            &envelope.team_id,
            &envelope.project_id,
            &envelope.actor_id,
            envelope.sequence,
            &envelope.operation,
        ) {
            Ok(expected) => expected == envelope.signature,
            Err(_) => false,
        }
    }

    fn entry_fingerprint(entry: &MemoryEntry) -> Result<String> {
        let canonical = serde_json::to_vec(&(
            &entry.id,
            &entry.kind,
            &entry.content,
            &entry.tags,
            &entry.source,
            &entry.superseded_by,
            &entry.contradicts,
            &entry.parent_id,
            &entry.caused_by,
            &entry.enables,
            entry.updated_at,
        ))?;
        let digest = ring::digest::digest(&ring::digest::SHA256, &canonical);
        Ok(hex::encode(digest.as_ref()))
    }

    fn fingerprint_record(&self, fingerprint: &str) -> String {
        format!("{}:{}", Self::FINGERPRINT_SCHEMA_VERSION, fingerprint)
    }

    fn normalize_entry_projection(entry: &mut MemoryEntry) {
        entry.tags.sort();
        entry.tags.dedup();
        entry.contradicts.sort();
        entry.contradicts.dedup();
        entry.caused_by.sort();
        entry.caused_by.dedup();
        entry.enables.sort();
        entry.enables.dedup();

        entry.superseded_by = entry
            .superseded_by
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        entry.parent_id = entry
            .parent_id
            .as_ref()
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
    }

    fn build_content_crdt_op(&self, entry: &MemoryEntry) -> Result<Vec<u8>> {
        let mut matrix = CollaborativeBrainMatrix::new(self.actor_id.clone());
        let op = matrix.update_memory(&entry.id, entry.content.clone())?;
        self.crdt.encode(&op)
    }

    fn serialize_team_brain(&self, team_brain: &TeamBrain) -> Result<String> {
        Ok(String::from_utf8(self.crdt.encode(team_brain)?)?)
    }

    fn deserialize_team_brain(&self, payload: &str) -> Result<TeamBrain> {
        self.crdt.decode(payload.as_bytes())
    }

    fn extract_string_values(value: &serde_json::Value) -> Vec<String> {
        match value {
            serde_json::Value::Array(items) => items
                .iter()
                .filter_map(|item| match item {
                    serde_json::Value::String(text) => Some(text.clone()),
                    serde_json::Value::Object(object) => serde_json::to_string(object).ok(),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    fn parse_file_map_entry(entry: &MemoryEntry) -> BTreeMap<String, Vec<String>> {
        let mut file_map = BTreeMap::new();
        if entry.id != "file_map" {
            return file_map;
        }

        let Ok(serde_json::Value::Object(values)) = serde_json::from_str::<serde_json::Value>(&entry.content) else {
            return file_map;
        };

        for (key, value) in values {
            let mut summaries = match value {
                serde_json::Value::String(summary) => vec![summary],
                serde_json::Value::Array(items) => items
                    .into_iter()
                    .filter_map(|item| item.as_str().map(|text| text.to_string()))
                    .collect::<Vec<_>>(),
                _ => Vec::new(),
            };
            summaries.sort();
            summaries.dedup();
            if !summaries.is_empty() {
                file_map.insert(key, summaries);
            }
        }

        file_map
    }

    fn update_team_brain_from_entry(&self, team_brain: &mut TeamBrain, entry: &MemoryEntry) {
        if entry.id == "identity" || entry.tags.iter().any(|tag| tag == "identity") {
            team_brain.set_identity(&self.actor_id, entry.content.clone());
        }

        if entry.id == "patterns" || entry.kind == crate::brain::schema::MemoryKind::Pattern {
            team_brain.set_patterns(&self.actor_id, entry.content.clone());
        }

        if entry.kind == crate::brain::schema::MemoryKind::Decision {
            team_brain.add_decision(&self.actor_id, entry.id.clone());
        }

        if entry.id == "known_issues" {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&entry.content) {
                for issue in Self::extract_string_values(&value) {
                    team_brain.add_known_issue(&self.actor_id, issue);
                }
            } else {
                team_brain.add_known_issue(&self.actor_id, entry.content.clone());
            }
        } else if entry.kind == crate::brain::schema::MemoryKind::Warning {
            team_brain.add_known_issue(&self.actor_id, entry.id.clone());
        }

        if entry.id == "file_map" {
            if let Ok(serde_json::Value::Object(file_map)) = serde_json::from_str::<serde_json::Value>(&entry.content) {
                for (key, value) in file_map {
                    if let Some(summary) = value.as_str() {
                        team_brain.set_file_map_entry(&self.actor_id, key, summary.to_string());
                    }
                }
            }
        }

        team_brain.bump_session_counter();
    }

    fn pull_range_start(cursor: i64, current_seq: i64, ops_len: i64) -> (i64, bool) {
        if current_seq <= 0 || ops_len <= 0 {
            return (0, false);
        }

        let earliest_sequence = (current_seq - ops_len + 1).max(1);
        let has_gap = cursor.saturating_add(1) < earliest_sequence;
        let start = if has_gap {
            0
        } else {
            (cursor - earliest_sequence + 1).max(0)
        };

        (start, has_gap)
    }

    fn validate_content_crdt_op(&self, memory_id: &str, expected_content: &str, encoded_op: &[u8]) -> bool {
        let op = match self.crdt.decode(encoded_op) {
            Ok(op) => op,
            Err(_) => return false,
        };
        let mut matrix = CollaborativeBrainMatrix::new(self.actor_id.clone());
        matrix.merge_foreign_operation(op);
        match matrix.read_memory(memory_id) {
            Some(values) => values.iter().any(|value| value == expected_content),
            None => false,
        }
    }

	fn serialize_matrix(&self, matrix: &CollaborativeBrainMatrix) -> Result<String> {
		Ok(String::from_utf8(self.crdt.encode(matrix)?)?)
	}

	fn deserialize_matrix(&self, payload: &str) -> Result<CollaborativeBrainMatrix> {
		self.crdt.decode(payload.as_bytes())
	}

	async fn load_team_brain(
		&self,
		conn: &mut redis::aio::MultiplexedConnection,
		team_id: &str,
		project_id: &str,
	) -> Result<TeamBrain> {
		let team_brain_key = Self::team_brain_key(team_id, project_id);
		let stored: Option<String> = conn.get(&team_brain_key).await.ok();
		match stored {
			Some(payload) => self.deserialize_team_brain(&payload),
			None => Ok(TeamBrain::new(team_id.to_string())),
		}
	}

	async fn persist_team_brain(
		&self,
		conn: &mut redis::aio::MultiplexedConnection,
		team_id: &str,
		project_id: &str,
		team_brain: &TeamBrain,
	) -> Result<()> {
		let team_brain_key = Self::team_brain_key(team_id, project_id);
		let encoded = self.serialize_team_brain(team_brain)?;
		let _: () = conn.set(&team_brain_key, encoded).await?;
		let _: () = redis::cmd("EXPIRE")
			.arg(&team_brain_key)
			.arg(Self::TRANSPORT_TTL_SECONDS)
			.query_async(conn)
			.await?;
		Ok(())
	}

	async fn read_team_brain_snapshot(
		&self,
		conn: &mut redis::aio::MultiplexedConnection,
		team_id: &str,
		project_id: &str,
	) -> Result<TeamBrainSnapshot> {
		let mut snapshot = self.load_team_brain(conn, team_id, project_id).await?.snapshot();
		let file_map_key = Self::team_brain_file_map_key(team_id, project_id);
		let stored_file_map: HashMap<String, String> = conn.hgetall(&file_map_key).await.unwrap_or_default();
		let mut readable_file_map = BTreeMap::new();
		for (path, encoded_values) in stored_file_map {
			let mut values: Vec<String> = serde_json::from_str(&encoded_values).unwrap_or_default();
			values.sort();
			values.dedup();
			if !values.is_empty() {
				readable_file_map.insert(path, values);
			}
		}
		snapshot.file_map = readable_file_map;
		Ok(snapshot)
	}

	async fn persist_team_brain_file_map_projection(
		&self,
		conn: &mut redis::aio::MultiplexedConnection,
		team_id: &str,
		project_id: &str,
		entry: &MemoryEntry,
	) -> Result<()> {
		let file_map_updates = Self::parse_file_map_entry(entry);
		if file_map_updates.is_empty() {
			return Ok(());
		}

		let file_map_key = Self::team_brain_file_map_key(team_id, project_id);
		for (path, values) in file_map_updates {
			let encoded = serde_json::to_string(&values)?;
			let _: () = conn.hset(&file_map_key, path, encoded).await?;
		}
		let _: () = redis::cmd("EXPIRE")
			.arg(&file_map_key)
			.arg(Self::TRANSPORT_TTL_SECONDS)
			.query_async(conn)
			.await?;
		Ok(())
	}

	async fn persist_entry_projection(
		&self,
		conn: &mut redis::aio::MultiplexedConnection,
		team_id: &str,
		project_id: &str,
		entry: &MemoryEntry,
	) -> Result<()> {
		let projection_key = Self::entry_projection_key(team_id, project_id);
		let mut normalized = entry.clone();
		Self::normalize_entry_projection(&mut normalized);
		let encoded = String::from_utf8(self.crdt.encode(&normalized)?)?;
		let _: () = conn.hset(&projection_key, &normalized.id, encoded).await?;
		let _: () = redis::cmd("EXPIRE")
			.arg(&projection_key)
			.arg(Self::TRANSPORT_TTL_SECONDS)
			.query_async(conn)
			.await?;
		Ok(())
	}

	async fn recover_entries_from_shared_state(
		&self,
		conn: &mut redis::aio::MultiplexedConnection,
		team_id: &str,
		project_id: &str,
	) -> Result<Vec<MemoryEntry>> {
		let projection_key = Self::entry_projection_key(team_id, project_id);
		let state_key = Self::crdt_state_key(team_id, project_id);
		let projections: HashMap<String, String> = conn.hgetall(&projection_key).await.unwrap_or_default();
		let state_entries: HashMap<String, String> = conn.hgetall(&state_key).await.unwrap_or_default();
		let mut recovered = Vec::new();

		for (entry_id, encoded_entry) in projections {
			let Some(encoded_matrix) = state_entries.get(&entry_id) else {
				continue;
			};
			let mut entry: MemoryEntry = self.crdt.decode(encoded_entry.as_bytes())?;
			let matrix = self.deserialize_matrix(encoded_matrix)?;
			let resolved = matrix.read_memory(&entry_id).unwrap_or_default();
			if !self.project_entry_from_resolved_values(&mut entry, &resolved) {
				continue;
			}
			Self::normalize_entry_projection(&mut entry);
			recovered.push(entry);
		}

		recovered.sort_by(|left, right| left.id.cmp(&right.id));
		Ok(recovered)
	}

	async fn apply_entry_to_team_brain(
		&self,
		conn: &mut redis::aio::MultiplexedConnection,
		team_id: &str,
		project_id: &str,
		entry: &MemoryEntry,
	) -> Result<()> {
		let mut team_brain = self.load_team_brain(conn, team_id, project_id).await?;
		self.update_team_brain_from_entry(&mut team_brain, entry);
		self.persist_team_brain(conn, team_id, project_id, &team_brain).await?;
		self.persist_team_brain_file_map_projection(conn, team_id, project_id, entry).await?;
		self.persist_entry_projection(conn, team_id, project_id, entry).await
	}

	async fn load_shared_matrix(
		&self,
		conn: &mut redis::aio::MultiplexedConnection,
		team_id: &str,
		project_id: &str,
		memory_id: &str,
	) -> Result<CollaborativeBrainMatrix> {
		let state_key = Self::crdt_state_key(team_id, project_id);
		let stored: Option<String> = conn.hget(&state_key, memory_id).await.ok();
		match stored {
			Some(payload) => self.deserialize_matrix(&payload),
			None => Ok(CollaborativeBrainMatrix::new(self.actor_id.clone())),
		}
	}

	async fn persist_shared_matrix(
		&self,
		conn: &mut redis::aio::MultiplexedConnection,
		team_id: &str,
		project_id: &str,
		memory_id: &str,
		matrix: &CollaborativeBrainMatrix,
	) -> Result<()> {
		let state_key = Self::crdt_state_key(team_id, project_id);
		let encoded = self.serialize_matrix(matrix)?;
		let _: () = conn.hset(&state_key, memory_id, encoded).await?;
		let _: () = redis::cmd("EXPIRE")
			.arg(&state_key)
			.arg(Self::TRANSPORT_TTL_SECONDS)
			.query_async(conn)
			.await?;
		Ok(())
	}

	async fn apply_content_crdt_op_to_shared_state(
		&self,
		conn: &mut redis::aio::MultiplexedConnection,
		team_id: &str,
		project_id: &str,
		memory_id: &str,
		encoded_op: &[u8],
	) -> Result<Vec<String>> {
		let op = self.crdt.decode(encoded_op)?;
		let mut matrix = self.load_shared_matrix(conn, team_id, project_id, memory_id).await?;
		matrix.merge_foreign_operation(op);
		let resolved = matrix.read_memory(memory_id).unwrap_or_default();
		self.persist_shared_matrix(conn, team_id, project_id, memory_id, &matrix).await?;
		Ok(resolved)
	}

    fn project_entry_from_resolved_values(&self, entry: &mut MemoryEntry, resolved: &[String]) -> bool {
        if resolved.is_empty() {
            return false;
        }

        let mut values = resolved.to_vec();
        values.sort();
        values.dedup();

        entry.tags.retain(|tag| tag != "crdt_conflict");

        if values.len() == 1 {
            if let Some(content) = values.into_iter().next() {
                entry.content = content;
                return true;
            }
            return false;
        }

        let preferred = if values.iter().any(|value| value == &entry.content) {
            entry.content.clone()
        } else {
            values[0].clone()
        };
        entry.content = preferred;
        entry.tags.push("crdt_conflict".to_string());
        Self::normalize_entry_projection(entry);
        true
    }

    pub async fn publish_entries(&self, team_id: &str, project_id: &str, entries: &[MemoryEntry]) -> Result<u64> {
        let ops_key = Self::ops_key(team_id, project_id);
        let seq_key = Self::seq_key(team_id, project_id);
        let fingerprint_key = Self::fingerprint_key(team_id, project_id, &self.actor_id);
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let mut published = 0u64;

        for entry in entries {
            let sanitized = Self::sanitize_entry(entry);
            let fingerprint = Self::entry_fingerprint(&sanitized)?;
            let fingerprint_record = self.fingerprint_record(&fingerprint);
            let previous: Option<String> = conn.hget(&fingerprint_key, &sanitized.id).await.ok();
            if previous.as_deref() == Some(fingerprint_record.as_str()) {
                continue;
            }
            let sequence: i64 = conn.incr(&seq_key, 1).await?;
            let content_op = self.build_content_crdt_op(&sanitized)?;
            let operation = TeamOperation::UpsertMemoryCrdt {
                entry: sanitized.clone(),
                content_op: content_op.clone(),
            };
            let signature = self.sign_payload(team_id, project_id, &self.actor_id, sequence, &operation)?;
            let envelope = TeamOperationEnvelope {
                team_id: team_id.to_string(),
                project_id: project_id.to_string(),
                actor_id: self.actor_id.clone(),
                sequence,
                generated_at_unix: chrono::Utc::now().timestamp(),
                operation,
                signature,
            };
            let encoded = self.crdt.encode(&envelope)?;
            let encrypted = self.encrypt_payload(&encoded)?;
            let _: () = conn.rpush(&ops_key, encrypted).await?;
			let _: () = conn.hset(&fingerprint_key, &sanitized.id, fingerprint_record).await?;
			let _ = self
				.apply_content_crdt_op_to_shared_state(&mut conn, team_id, project_id, &entry.id, &content_op)
				.await?;
			self
				.apply_entry_to_team_brain(&mut conn, team_id, project_id, &sanitized)
				.await?;
            published = published.saturating_add(1);
        }

        if published > 0 {
            let _: () = redis::cmd("LTRIM")
                .arg(&ops_key)
                .arg(-5000)
                .arg(-1)
                .query_async(&mut conn)
                .await?;
            let _: () = redis::cmd("EXPIRE")
                .arg(&ops_key)
                .arg(Self::TRANSPORT_TTL_SECONDS)
                .query_async(&mut conn)
                .await?;
            let _: () = redis::cmd("EXPIRE")
                .arg(&seq_key)
                .arg(Self::TRANSPORT_TTL_SECONDS)
                .query_async(&mut conn)
                .await?;
            let _: () = redis::cmd("EXPIRE")
                .arg(&fingerprint_key)
                .arg(Self::TRANSPORT_TTL_SECONDS)
                .query_async(&mut conn)
                .await?;
        }

        Ok(published)
    }

    pub async fn pull_operations(&self, team_id: &str, project_id: &str) -> Result<PullResult> {
        let ops_key = Self::ops_key(team_id, project_id);
        let seq_key = Self::seq_key(team_id, project_id);
        let cursor_key = Self::cursor_key(team_id, project_id, &self.actor_id);
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let cursor: i64 = conn.get(&cursor_key).await.unwrap_or(0);
        let current_seq: i64 = conn.get(&seq_key).await.unwrap_or(0);
        let ops_len: i64 = conn.llen(&ops_key).await.unwrap_or(0);
        let (range_start, has_gap) = Self::pull_range_start(cursor, current_seq, ops_len);
        let raw_ops: Vec<String> = if ops_len > 0 {
            conn.lrange(&ops_key, range_start as isize, -1).await?
        } else {
            Vec::new()
        };

        if has_gap {
            let recovered_entries = self.recover_entries_from_shared_state(&mut conn, team_id, project_id).await?;
            let team_brain = self.read_team_brain_snapshot(&mut conn, team_id, project_id).await?;
            let _: () = conn.set(&cursor_key, current_seq).await?;
            let _: () = redis::cmd("EXPIRE")
                .arg(&cursor_key)
                .arg(Self::TRANSPORT_TTL_SECONDS)
                .query_async(&mut conn)
                .await?;

            return Ok(PullResult {
                recovered_from_gap: true,
                recovered_entries: recovered_entries.len() as u64,
                entries: recovered_entries,
                applied_operations: 0,
                conflict_entries: 0,
                cursor: current_seq,
                namespace: Self::namespace(team_id, project_id),
                team_brain,
            });
        }

        let mut next_cursor = cursor;
        let mut applied_operations = 0u64;
        let mut conflict_entries: u64 = 0;
        let mut remote_entries = Vec::new();

        for raw in raw_ops {
            let decrypted = match self.decrypt_payload(&raw) {
                Ok(bytes) => bytes,
                Err(_) => {
                    conflict_entries = conflict_entries.saturating_add(1);
                    continue;
                }
            };
            let envelope: TeamOperationEnvelope = match self.crdt.decode(&decrypted) {
                Ok(envelope) => envelope,
                Err(_) => {
                    conflict_entries = conflict_entries.saturating_add(1);
                    continue;
                }
            };
            if envelope.sequence <= cursor {
                continue;
            }
            next_cursor = next_cursor.max(envelope.sequence);
            if envelope.team_id != team_id || envelope.project_id != project_id {
                conflict_entries = conflict_entries.saturating_add(1);
                continue;
            }
            if !self.verify_envelope(&envelope) {
                conflict_entries = conflict_entries.saturating_add(1);
                continue;
            }
            if envelope.actor_id == self.actor_id {
                continue;
            }
            match envelope.operation {
                TeamOperation::UpsertMemory(_) => {
                    conflict_entries = conflict_entries.saturating_add(1);
                    continue;
                }
                TeamOperation::UpsertMemoryCrdt { mut entry, content_op } => {
                    if !self.validate_content_crdt_op(&entry.id, &entry.content, &content_op) {
                        conflict_entries = conflict_entries.saturating_add(1);
                        continue;
                    }
					let resolved = match self
						.apply_content_crdt_op_to_shared_state(&mut conn, team_id, project_id, &entry.id, &content_op)
						.await
					{
						Ok(resolved) => resolved,
						Err(_) => {
							conflict_entries = conflict_entries.saturating_add(1);
							continue;
						}
					};
					if !resolved.iter().any(|value| value == &entry.content) {
						conflict_entries = conflict_entries.saturating_add(1);
						continue;
					}
                    if !self.project_entry_from_resolved_values(&mut entry, &resolved) {
                        conflict_entries = conflict_entries.saturating_add(1);
                        continue;
                    }
                    Self::normalize_entry_projection(&mut entry);
                    self
						.apply_entry_to_team_brain(&mut conn, team_id, project_id, &entry)
						.await?;
                    remote_entries.push(entry);
                    applied_operations = applied_operations.saturating_add(1);
                }
            }
        }

        let _: () = conn.set(&cursor_key, next_cursor).await?;
        let _: () = redis::cmd("EXPIRE")
            .arg(&cursor_key)
            .arg(Self::TRANSPORT_TTL_SECONDS)
            .query_async(&mut conn)
            .await?;

        let team_brain = self.read_team_brain_snapshot(&mut conn, team_id, project_id).await?;

        Ok(PullResult {
            recovered_from_gap: false,
            recovered_entries: 0,
            entries: remote_entries,
            applied_operations,
            conflict_entries,
            cursor: next_cursor,
            namespace: Self::namespace(team_id, project_id),
            team_brain,
        })
    }

    pub fn merge_team_brain(&self, local: &mut TeamBrain, remote: &TeamBrain) {
        local.merge(remote);
    }
}

impl Default for TeamManager {
    fn default() -> Self {
        let client = redis::Client::open("redis://127.0.0.1/").expect("default redis client must parse");
        Self::new(client, "default-actor".to_string(), "insecure-default".to_string())
    }
}

pub struct PullResult {
    pub recovered_from_gap: bool,
    pub recovered_entries: u64,
    pub entries: Vec<MemoryEntry>,
    pub applied_operations: u64,
    pub conflict_entries: u64,
    pub cursor: i64,
    pub namespace: String,
    pub team_brain: TeamBrainSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TeamOperation {
    UpsertMemory(MemoryEntry),
    UpsertMemoryCrdt {
        entry: MemoryEntry,
        content_op: Vec<u8>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct EncryptedTransportEnvelope {
    version: u8,
    nonce_hex: String,
    ciphertext_hex: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brain::schema::{MemoryKind, MemorySource};

    fn sample_entry(content: &str) -> MemoryEntry {
        let now = chrono::Utc::now();
        MemoryEntry {
            id: "entry-1".to_string(),
            project_id: "project-1".to_string(),
            kind: MemoryKind::Fact,
            content: content.to_string(),
            tags: vec!["team".to_string()],
            source: MemorySource::UserManual,
            superseded_by: None,
            contradicts: vec![],
            parent_id: None,
            caused_by: vec![],
            enables: vec![],
            created_at: now,
            updated_at: now,
            access_count: 7,
            last_accessed_at: Some(now),
        }
    }

    #[test]
    fn sanitize_entry_redacts_secrets_and_usage_metadata() {
        let entry = sample_entry("api_key=ghp_abcdefghijklmnopqrstuvwxyz1234567890");
        let sanitized = TeamManager::sanitize_entry(&entry);
        assert!(sanitized.content.contains("[REDACTED]"));
        assert_eq!(sanitized.access_count, 0);
        assert!(sanitized.last_accessed_at.is_none());
    }

    #[test]
    fn verify_envelope_rejects_tampering() {
        let client = redis::Client::open("redis://127.0.0.1/").expect("redis URL parses");
        let manager = TeamManager::new(client, "actor-a".to_string(), "0123456789abcdef0123456789abcdef".to_string());
        let entry = TeamManager::sanitize_entry(&sample_entry("safe content"));
        let operation = TeamOperation::UpsertMemoryCrdt {
            content_op: manager.build_content_crdt_op(&entry).expect("crdt op should encode"),
            entry,
        };
        let signature = manager
            .sign_payload("team-a", "project-a", "actor-a", 1, &operation)
            .expect("signature should be generated");
        let mut envelope = TeamOperationEnvelope {
            team_id: "team-a".to_string(),
            project_id: "project-a".to_string(),
            actor_id: "actor-a".to_string(),
            sequence: 1,
            generated_at_unix: chrono::Utc::now().timestamp(),
            operation,
            signature,
        };
        assert!(manager.verify_envelope(&envelope));
        envelope.sequence = 2;
        assert!(!manager.verify_envelope(&envelope));
    }

    #[test]
    fn encrypt_payload_round_trip_and_tamper_detection() {
        let client = redis::Client::open("redis://127.0.0.1/").expect("redis URL parses");
        let manager = TeamManager::new(client, "actor-a".to_string(), "0123456789abcdef0123456789abcdef".to_string());
        let plaintext = b"team-sync-plaintext";
        let encrypted = manager.encrypt_payload(plaintext).expect("encryption should work");
        let decrypted = manager.decrypt_payload(&encrypted).expect("decryption should work");
        assert_eq!(decrypted, plaintext);

        let mut tampered: serde_json::Value = serde_json::from_str(&encrypted).expect("json envelope");
        let ciphertext = tampered["ciphertext_hex"].as_str().unwrap().to_string();
        tampered["ciphertext_hex"] = serde_json::Value::String(format!("00{}", &ciphertext[2..]));
        let tampered_raw = serde_json::to_string(&tampered).expect("serialize tampered envelope");
        assert!(manager.decrypt_payload(&tampered_raw).is_err());
    }

    #[test]
    fn crdt_content_operation_round_trip_matches_entry_content() {
        let client = redis::Client::open("redis://127.0.0.1/").expect("redis URL parses");
        let manager = TeamManager::new(client, "actor-a".to_string(), "0123456789abcdef0123456789abcdef".to_string());
        let entry = TeamManager::sanitize_entry(&sample_entry("shared content"));
        let encoded_op = manager.build_content_crdt_op(&entry).expect("crdt op should encode");
        assert!(manager.validate_content_crdt_op(&entry.id, &entry.content, &encoded_op));
        assert!(!manager.validate_content_crdt_op(&entry.id, "different content", &encoded_op));
    }

    #[test]
    fn collaborative_matrix_serialization_round_trip_preserves_content() {
        let client = redis::Client::open("redis://127.0.0.1/").expect("redis URL parses");
        let manager = TeamManager::new(client, "actor-a".to_string(), "0123456789abcdef0123456789abcdef".to_string());
        let mut matrix = CollaborativeBrainMatrix::new("actor-a".to_string());
        matrix.update_memory("entry-1", "shared content".to_string()).expect("update should work");
        let encoded = manager.serialize_matrix(&matrix).expect("matrix serializes");
        let decoded = manager.deserialize_matrix(&encoded).expect("matrix deserializes");
        assert_eq!(decoded.read_memory("entry-1").unwrap_or_default(), vec!["shared content".to_string()]);
    }

    #[test]
    fn project_entry_from_resolved_values_is_conflict_aware() {
        let client = redis::Client::open("redis://127.0.0.1/").expect("redis URL parses");
        let manager = TeamManager::new(client, "actor-a".to_string(), "0123456789abcdef0123456789abcdef".to_string());
        let mut conflict_entry = sample_entry("b");
        assert!(manager.project_entry_from_resolved_values(&mut conflict_entry, &["b".to_string(), "a".to_string(), "a".to_string()]));
        assert_eq!(conflict_entry.content, "b".to_string());
        assert!(conflict_entry.tags.iter().any(|tag| tag == "crdt_conflict"));

        let mut single_entry = sample_entry("solo");
        assert!(manager.project_entry_from_resolved_values(&mut single_entry, &["solo".to_string()]));
        assert_eq!(single_entry.content, "solo".to_string());
        assert!(!single_entry.tags.iter().any(|tag| tag == "crdt_conflict"));

        let mut empty_entry = sample_entry("noop");
        assert!(!manager.project_entry_from_resolved_values(&mut empty_entry, &[]));
    }

    #[test]
    fn pull_range_start_detects_trimmed_cursor_gap() {
        assert_eq!(TeamManager::pull_range_start(6, 12, 5), (0, true));
        assert_eq!(TeamManager::pull_range_start(7, 12, 5), (0, false));
        assert_eq!(TeamManager::pull_range_start(10, 12, 5), (3, false));
        assert_eq!(TeamManager::pull_range_start(0, 0, 0), (0, false));
    }

    #[test]
    fn update_team_brain_from_entry_projects_observer_state() {
        let client = redis::Client::open("redis://127.0.0.1/").expect("redis URL parses");
        let manager = TeamManager::new(client, "actor-a".to_string(), "0123456789abcdef0123456789abcdef".to_string());
        let mut team_brain = TeamBrain::new("team-a".to_string());

        let mut file_map_entry = sample_entry(r#"{"src/lib.rs":"exports=run | patterns=sync"}"#);
        file_map_entry.id = "file_map".to_string();

        let mut known_issues_entry = sample_entry(r#"["redis lag","cursor gap"]"#);
        known_issues_entry.id = "known_issues".to_string();
        known_issues_entry.kind = MemoryKind::Warning;

        let mut decision_entry = sample_entry("Ship CRDT-only sync");
        decision_entry.id = "decision-1".to_string();
        decision_entry.kind = MemoryKind::Decision;

        manager.update_team_brain_from_entry(&mut team_brain, &file_map_entry);
        manager.update_team_brain_from_entry(&mut team_brain, &known_issues_entry);
        manager.update_team_brain_from_entry(&mut team_brain, &decision_entry);

        let file_map_summary = team_brain
            .file_map
            .get(&"src/lib.rs".to_string())
            .val
            .expect("file map entry should exist")
            .read()
            .val
            .into_iter()
            .collect::<Vec<_>>();
        assert!(file_map_summary.iter().any(|value| value == "exports=run | patterns=sync"));
        assert!(team_brain.decisions.read().val.contains(&"decision-1".to_string()));
        assert!(team_brain.known_issues.read().val.contains(&"redis lag".to_string()));
        assert!(team_brain.known_issues.read().val.contains(&"cursor gap".to_string()));
        assert_eq!(team_brain.session_counter, 3);
    }

    #[test]
    fn normalize_entry_projection_sorts_and_deduplicates_structured_fields() {
        let mut entry = sample_entry("content");
        entry.tags = vec!["b".to_string(), "a".to_string(), "a".to_string()];
        entry.contradicts = vec!["x".to_string(), "x".to_string(), "a".to_string()];
        entry.caused_by = vec!["2".to_string(), "1".to_string(), "1".to_string()];
        entry.enables = vec!["z".to_string(), "z".to_string(), "y".to_string()];
        entry.parent_id = Some("  parent ".to_string());
        entry.superseded_by = Some("   ".to_string());

        TeamManager::normalize_entry_projection(&mut entry);

        assert_eq!(entry.tags, vec!["a".to_string(), "b".to_string()]);
        assert_eq!(entry.contradicts, vec!["a".to_string(), "x".to_string()]);
        assert_eq!(entry.caused_by, vec!["1".to_string(), "2".to_string()]);
        assert_eq!(entry.enables, vec!["y".to_string(), "z".to_string()]);
        assert_eq!(entry.parent_id, Some("parent".to_string()));
        assert_eq!(entry.superseded_by, None);
    }

    #[test]
    fn team_brain_snapshot_is_readable() {
        let mut team_brain = TeamBrain::new("team-a".to_string());
        team_brain.set_identity("actor-a", "Memix".to_string());
        team_brain.set_patterns("actor-a", "CRDT-first".to_string());
        team_brain.add_decision("actor-a", "decision-1".to_string());
        team_brain.add_known_issue("actor-a", "cursor gap".to_string());
        team_brain.bump_session_counter();

        let snapshot = team_brain.snapshot();
        assert_eq!(snapshot.team_id, "team-a".to_string());
        assert_eq!(snapshot.identity, vec!["Memix".to_string()]);
        assert_eq!(snapshot.patterns, vec!["CRDT-first".to_string()]);
        assert_eq!(snapshot.decisions, vec!["decision-1".to_string()]);
        assert_eq!(snapshot.known_issues, vec!["cursor gap".to_string()]);
        assert!(snapshot.file_map.is_empty());
        assert_eq!(snapshot.session_counter, 1);
    }

    #[test]
    fn parse_file_map_entry_extracts_readable_projection() {
        let mut entry = sample_entry(r#"{"src/lib.rs":"exports=run","src/sync.rs":["patterns=sync","patterns=sync","owner=team"]}"#);
        entry.id = "file_map".to_string();

        let file_map = TeamManager::parse_file_map_entry(&entry);
        assert_eq!(file_map.get("src/lib.rs"), Some(&vec!["exports=run".to_string()]));
        assert_eq!(
            file_map.get("src/sync.rs"),
            Some(&vec!["owner=team".to_string(), "patterns=sync".to_string()])
        );
    }

    #[test]
    fn sanitize_entry_preserves_file_map_shape_while_redacting_secrets() {
        let mut entry = sample_entry(r#"{"src/lib.rs":"token=ghp_abcdefghijklmnopqrstuvwxyz1234567890"}"#);
        entry.id = "file_map".to_string();

        let sanitized = TeamManager::sanitize_entry(&entry);
        let file_map = TeamManager::parse_file_map_entry(&sanitized);
        let values = file_map.get("src/lib.rs").expect("file_map projection should exist");
        assert_eq!(values.len(), 1);
        assert!(values[0].contains("[REDACTED]"));
    }
}
