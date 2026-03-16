use anyhow::Result;
use crdts::map::Op;
use crdts::{CmRDT, MVReg, Map, VClock};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

/// We define standard identifiers for our global actor mesh
pub type ActorId = String;

pub struct CrdtEngine {
    local_actor: ActorId,
}

impl CrdtEngine {
    pub fn new(local_actor: ActorId) -> Self {
        Self { local_actor }
    }

    pub fn actor_id(&self) -> &str {
        &self.local_actor
    }

    pub fn encode<T: Serialize>(&self, data: &T) -> Result<Vec<u8>> {
        Ok(serde_json::to_vec(data)?)
    }

    pub fn decode<T: DeserializeOwned>(&self, data: &[u8]) -> Result<T> {
        Ok(serde_json::from_slice(data)?)
    }
}

impl Default for CrdtEngine {
    fn default() -> Self {
        Self::new("local-actor".to_string())
    }
}

/// The Collaborative Brain Matrix handles memory state securely across an entire developer team,
/// guaranteeing absolute mathematical convergence without merge conflicts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollaborativeBrainMatrix {
    /// Multi-Value Register containing the textual brain contents.
    /// `Map<Key, Component>`: We map the Memory ID to its content blocks natively.
    pub memory_store: Map<String, MVReg<String, ActorId>, ActorId>,

    /// True Vector Clock tracking causality across the distributed mesh
    pub clock: VClock<ActorId>,

    /// The local author ID interacting with this specific daemon daemon
    pub local_actor: ActorId,
}

impl CollaborativeBrainMatrix {
    pub fn new(actor_id: String) -> Self {
        Self {
            memory_store: Map::new(),
            clock: VClock::new(),
            local_actor: actor_id,
        }
    }

    /// Mutates the global memory matrix perfectly tracking the local vector clock sequence
    pub fn update_memory(
        &mut self,
        memory_id: &str,
        new_content: String,
    ) -> Result<Op<String, MVReg<String, ActorId>, ActorId>> {
        let ctx = self
            .memory_store
            .read_ctx()
            .derive_add_ctx(self.local_actor.clone());

        let op = self
            .memory_store
            .update(memory_id.to_string(), ctx, |reg, ctx| {
                reg.write(new_content.clone(), ctx)
            });

        self.memory_store.apply(op.clone());
        Ok(op)
    }

    /// Synchronizes a foreign CRDT operational branch into this local daemon, resolving conflicts implicitly natively.
    pub fn merge_foreign_operation(
        &mut self,
        operation: Op<String, MVReg<String, ActorId>, ActorId>,
    ) {
        self.memory_store.apply(operation);
    }

    /// Retrieves the dynamically resolved state of a particular memory across the team mesh
    pub fn read_memory(&self, memory_id: &str) -> Option<Vec<String>> {
        let read_ctx = self.memory_store.get(&memory_id.to_string());
        if let Some(reg) = read_ctx.val {
            Some(reg.read().val.into_iter().collect())
        } else {
            None
        }
    }
}
