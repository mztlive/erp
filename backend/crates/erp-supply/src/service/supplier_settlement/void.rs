use super::SupplierSettlementService;
use crate::dto::supplier_settlement::*;
use crate::entity::supplier_settlement::*;
use crate::{Error, Result};
use persistence_core::Executor;
use validator::Validate;
impl SupplierSettlementService {
    /// 结算本域prepare_void，保持原校验、构造和执行器顺序。
    pub async fn prepare_void(
        &self,
        id: &str,
        req: &VoidSettlementRequest,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierSettlementStatement, bool)> {
        req.validate()?;
        let mut statement = self.load_statement(id, executor).await?;
        if statement.is_voided() {
            return Ok((statement, true));
        }
        if !statement.is_prepared_by(actor_id) || !statement.is_editable() {
            return Err(Error::BusinessLogicError(
                "只有经办人可以作废尚未提交复核的结算草稿".to_string(),
            ));
        }
        statement
            .ensure_version(req.version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        statement.void_draft()?;

        Ok((statement, false))
    }
}
