//! 资金命令范围守卫。

use application_core::AuditActor;
use erp_finance::repository::ReceivableExt;
use erp_sales::repository::SalesOrderExt;
use persistence_core::Executor;

use super::authorization::*;
use crate::{Error, Result};

impl FundsAccess {
    /// 应收子账命令守卫：详情动作重验关联销售当前负责人与登记经办。
    pub(super) async fn guard_receivable_account(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (access, _) = self.resolve(actor, "receivable_account", "detail", executor).await?;
        let account = self
            .db
            .receivable_accounts()
            .find_by_id(id, executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".into()))?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(account.sales_order_id.as_ref(), executor)
            .await
            .map_err(Error::from)?
            .ok_or_else(|| Error::NotFound("应收往来子账不存在".into()))?;
        let facts = FundsLinkedFacts {
            owner_user_id: Some(order.sales_owner_user_id.clone()),
            business_org_unit_id: Some(order.business_org_unit_id.clone()),
            operator_user_ids: vec![account.stable.created_by.clone()],
            secondary_operator_user_ids: Vec::new(),
            linked_document_id: order.base.id.clone(),
            linked_document_version: order.base.version,
        };
        if !Self::allows(&access, &facts)? {
            return Err(Error::NotFound("应收往来子账不存在".into()));
        }
        Ok(())
    }
}
