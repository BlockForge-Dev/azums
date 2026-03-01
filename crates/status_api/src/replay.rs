use async_trait::async_trait;
use execution_core::{CoreError, ReplayCommand, ReplayResult};

#[async_trait]
pub trait ReplayGateway: Send + Sync {
    async fn request_replay(&self, command: ReplayCommand) -> Result<ReplayResult, CoreError>;
}

#[async_trait]
impl ReplayGateway for execution_core::ExecutionCore {
    async fn request_replay(&self, command: ReplayCommand) -> Result<ReplayResult, CoreError> {
        self.request_replay(command).await
    }
}
