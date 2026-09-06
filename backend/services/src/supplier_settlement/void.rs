use database::SupplierSettlementExt;
use entities::supplier_settlement::SupplierSettlementStatement;
use erp_audit::AuditExt;
use persistence_core::Transactional;
use validator::Validate;

use super::{SupplierSettlementService, SupplierSettlementStatementView, VoidSettlementRequest};
use crate::errors::{Error, Result};
use application_core::AuditActor;
use erp_audit::AuditActorLogs;

impl SupplierSettlementService {
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
        req.validate()?;
        let mut statement = self.load_statement(id).await?;
        if statement.is_voided() {
            return Ok(statement.into());
        }
        if !statement.is_prepared_by(actor.id()) || !statement.is_editable() {
            return Err(Error::BusinessLogicError(
                "只有经办人可以作废尚未提交复核的结算草稿".to_string(),
            ));
        }
        statement
            .ensure_version(req.version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        statement.void_draft()?;
        let audit = actor.clone().resource_log(
            "supplier_settlement.void",
            "supplier_settlement_statement",
            id.to_string(),
        )?;
        self.update_statement_with_audit(&mut statement, &audit).await?;
        Ok(statement.into())
    }

    /// 在同一事务更新结算单并写审计。
    ///
    /// # 参数
    /// * `statement` - 结算单实体（就地更新）
    /// * `audit` - 审计日志
    ///
    /// # 错误
    /// 乐观锁冲突透出 `ConflictError`，提交结果未知透出 `OutcomeUnknown`。
    async fn update_statement_with_audit(
        &self,
        statement: &mut SupplierSettlementStatement,
        audit: &erp_audit::AuditLog,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let mut statement_for_tx = statement.clone();
        let audit_for_tx = audit.clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_settlement_statements()
                        .update(&mut statement_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<SupplierSettlementStatement, crate::errors::Error>(statement_for_tx)
                })
            })
            .await?;
        *statement = updated;
        Ok(())
    }
}
