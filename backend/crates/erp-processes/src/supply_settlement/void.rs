use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use persistence_core::{NoTransaction, Transactional};

use super::{SupplierSettlementProcess, SupplierSettlementStatementView, VoidSettlementRequest};
use crate::audit::persist_log;
use crate::{Error, Result};

impl SupplierSettlementProcess {
    /// 作废尚未提交复核的结算草稿。
    ///
    /// # 参数
    /// * `id` - 结算单 ID
    /// * `req` - 作废请求（含期望版本与原因）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回作废后结算单的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 结算单不存在
    /// * `ConflictError` - 版本冲突
    pub async fn void_statement(
        &self,
        id: &str,
        req: VoidSettlementRequest,
        actor: &AuditActor,
    ) -> Result<SupplierSettlementStatementView> {
        self.domain().access().require_statement(actor, "update", id, &mut NoTransaction).await?;
        let (mut statement, replayed) =
            self.domain().prepare_void(id, &req, actor.id(), &mut NoTransaction).await?;
        if replayed {
            return Ok(statement.into());
        }
        let audit = actor.clone().resource_log(
            "supplier_settlement.void",
            "supplier_settlement_statement",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let statement = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    SupplierSettlementService::new(db.clone())
                        .persist_statement(&mut statement, executor)
                        .await?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<_, Error>(statement)
                })
            })
            .await?;
        Ok(statement.into())
    }
}
