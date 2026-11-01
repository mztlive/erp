//! 进度随任务版本持久化；后台等待写入成功后才能开始下一阶段。
use async_trait::async_trait;
use erp_contract::entity::recognition::{ContractImport, ImportStage};
use erp_contract::repository::recognition::ContractImportExt;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::Result;

#[async_trait]
pub(super) trait RecognitionProgress: Send {
    /// 在实际处理前保存阶段。
    /// # 参数
    /// * `stage` - 即将执行的阶段。
    /// # 返回
    /// 进度已确认保存。
    /// # 错误
    /// 阶段非法、版本冲突或持久化失败。
    async fn advance(&mut self, stage: ImportStage) -> Result<()>;
}

pub(super) struct TaskProgress<'a> {
    pub db: &'a Database,
    pub task: &'a mut ContractImport,
}

#[async_trait]
impl RecognitionProgress for TaskProgress<'_> {
    async fn advance(&mut self, stage: ImportStage) -> Result<()> {
        self.task.advance(stage)?;
        self.db.contract_imports().update(self.task, &mut NoTransaction).await?;
        Ok(())
    }
}
