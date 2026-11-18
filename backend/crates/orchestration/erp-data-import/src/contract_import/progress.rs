//! 进度随任务版本持久化；后台等待写入成功后才能开始下一阶段。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_contract::ContractAccess;
use erp_contract::entity::recognition::{ContractImport, ImportStage};
use mongodb::Database;

use crate::Result;

#[async_trait]
pub(super) trait RecognitionProgress: Send {
    // 在实际处理前保存阶段。
    //
    // `stage`：即将执行的阶段。
    //
    // 进度已确认保存。
    //
    // 阶段非法、版本冲突或持久化失败。
    async fn advance(&mut self, stage: ImportStage) -> Result<()>;
}

pub(super) struct TaskProgress<'a> {
    pub db: &'a Database,
    pub access: &'a ContractAccess,
    pub actor: &'a AuditActor,
    pub task: &'a mut ContractImport,
}

#[async_trait]
impl RecognitionProgress for TaskProgress<'_> {
    async fn advance(&mut self, stage: ImportStage) -> Result<()> {
        self.task.advance(stage)?;
        *self.task = super::save_task(self.db, self.access, self.actor, self.task.clone()).await?;
        Ok(())
    }
}
