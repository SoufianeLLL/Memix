use crate::brain::schema::MemoryEntry;
use crate::sync::team::PullResult;
use anyhow::{anyhow, Result};
use async_trait::async_trait;

pub struct NoopTeamSync {
    reason: &'static str,
}

impl NoopTeamSync {
    pub fn new(reason: &'static str) -> Self {
        Self { reason }
    }
}

#[async_trait]
pub trait TeamSyncBackend: Send + Sync {
    async fn publish(&self, team_id: &str, project_id: &str, entries: &[MemoryEntry]) -> Result<u64>;
    async fn pull(&self, team_id: &str, project_id: &str) -> Result<PullResult>;
    fn is_available(&self) -> bool;
    fn denial_reason(&self) -> Option<&str>;
}

#[async_trait]
impl TeamSyncBackend for NoopTeamSync {
    async fn publish(&self, _: &str, _: &str, _: &[MemoryEntry]) -> Result<u64> {
        Err(anyhow!(self.reason))
    }

    async fn pull(&self, _: &str, _: &str) -> Result<PullResult> {
        Err(anyhow!(self.reason))
    }

    fn is_available(&self) -> bool {
        false
    }

    fn denial_reason(&self) -> Option<&str> {
        Some(self.reason)
    }
}
