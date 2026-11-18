//! 每个后台写入事务都重新验证当前处理标记，旧处理者不得继续写入。

use application_core::AuditActor;
use erp_supply::catalog_adoption::CatalogAdoptionBatch;
use persistence_core::Executor;

use super::SupplierCatalogAdoptionProcess;
use crate::Result;

impl SupplierCatalogAdoptionProcess {
    // 在逐项事务中重验本次草稿操作仍持有写入资格。
    //
    // 连接、批次、本次标记、认证用户及调用方执行器。
    //
    // 当前已授权批次。
    //
    // 范围不足或操作已经被接管时拒绝。
    pub(super) async fn check_batch_draft(
        &self,
        connection: &str,
        id: &str,
        token: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CatalogAdoptionBatch> {
        let batch = self.load_batch(connection, id, actor, executor).await?;
        batch.ensure_draft_operation(token)?;
        Ok(batch)
    }
}
