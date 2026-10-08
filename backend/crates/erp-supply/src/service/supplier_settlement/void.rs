use persistence_core::Executor;
use validator::Validate;

use super::SupplierSettlementService;
use crate::dto::supplier_settlement::*;
use crate::entity::supplier_settlement::*;
use crate::{Error, Result};
impl SupplierSettlementService {
    /// 结算本域prepare_void，保持原校验、构造和执行器顺序。
    ///
    /// # 参数
    /// * `id` - 结算单主键。
    /// * `req` - 作废请求，含期望版本。
    /// * `actor_id` - 操作人。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 返回结算单和是否已经作废。`true` 表示进入函数时已作废，本次不再改状态；`false` 表示已在内存中作废，本函数不写回。
    ///
    /// # 错误
    /// 结算单不存在时返回 `NotFound`。操作人不是经办人或草稿不可编辑时返回 `BusinessLogicError`。版本不一致时返回 `ConflictError`。参数校验、状态迁移或仓储读取失败时返回对应错误。
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
            return Err(Error::BusinessLogicError("只有经办人可以作废尚未提交复核的结算草稿".to_string()));
        }
        statement
            .ensure_version(req.version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        statement.void_draft()?;

        Ok((statement, false))
    }
}
