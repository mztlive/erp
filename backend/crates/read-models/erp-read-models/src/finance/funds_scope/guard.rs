//! 资金命令范围守卫。

use application_core::AuditActor;
use erp_finance::repository::ReceivableExt;
use erp_permissions::receivable as receivable_permissions;
use persistence_core::Executor;

use super::authorization::*;
use crate::{Error, Result};

impl FundsAccess {
    /// 应收子账命令守卫：详情动作重验关联销售当前负责人与登记经办。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `id` - 应收往来子账主键。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 详情动作允许该子账时成功，无返回值。
    ///
    /// # 错误
    /// 授权解析失败时返回对应错误；子账或来源销售单不存在，或当前责任不允许时返回 `NotFound`。
    pub(super) async fn guard_receivable_account(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (_access, authorization) =
            self.resolve(actor, &receivable_permissions::ACCOUNT_DETAIL, executor).await?;
        self.require_visible_root(&authorization, id, executor).await?;
        let account = self
            .db
            .receivable_accounts()
            .find_by_id(id, executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".into()))?;
        if authorization
            .sales_scope()?
            .ids_in(&self.db, &[account.sales_order_id.to_string()], executor)
            .await?
            .is_empty()
        {
            return Err(Error::NotFound("应收往来子账不存在".into()));
        }
        authorization.deliver(executor).await?;
        Ok(())
    }
}
