//! 财务办理所需资料只从应收的真实生效版本读取。
use application_core::AuditActor;
use erp_core::ids::{SalesOrderRevisionId, SalesOrderRevisionLineId};
use erp_finance::entity::receivable::ReceivableAccount;
use erp_finance::repository::ReceivableExt;
use erp_identity::AccessControlExt;
use erp_permissions::receivable;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use persistence_core::{Executor, Transactional};
use serde::{Deserialize, Serialize};

use super::FundsAccess;
use crate::{Error, Result};

#[derive(Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AllocationSourceKind {
    ReceivableEntry,
    ReceivableAccount,
}

/// 仅返回财务核对所需的冻结商业资料。
#[derive(Serialize)]
pub struct AllocationSource {
    pub customer_name: String,
    pub project_name: Option<String>,
    pub contract_no: Option<String>,
    pub owner_name: Option<String>,
    pub products: Vec<AllocationProduct>,
}
#[derive(Serialize)]
pub struct AllocationProduct {
    pub name: String,
    pub spec: Option<String>,
    pub quantity: Option<String>,
    pub unit: Option<String>,
}
impl FundsAccess {
    /// 处理财务分配办理资料时，根据实际应收目标返回已授权的办理资料。
    ///
    /// # 参数
    /// actor 为当前账号；kind、id 为本次分配的应收目标。
    ///
    /// # 返回
    /// 应收绑定版本中的客户、合同、项目和商品资料。
    ///
    /// # 错误
    /// 功能失效、来源越界、目标或版本错配拒绝。
    pub async fn allocation_source(
        &self,
        actor: &AuditActor,
        kind: AllocationSourceKind,
        id: &str,
    ) -> Result<AllocationSource> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |ex| {
                Box::pin(async move { this.load_allocation_source(&actor, kind, &id, ex).await })
            })
            .await
    }
    async fn load_allocation_source(
        &self,
        actor: &AuditActor,
        kind: AllocationSourceKind,
        id: &str,
        ex: &mut dyn Executor,
    ) -> Result<AllocationSource> {
        let (_, authorization) = self.resolve(actor, &receivable::ACCOUNT_DETAIL, ex).await?;
        let account_id = self.allocation_account_id(kind, id, ex).await?;
        self.require_visible_root(&authorization, &account_id, ex).await?;

        let account = self
            .db
            .receivable_accounts()
            .find_by_id(&account_id, ex)
            .await?
            .ok_or_else(|| Error::NotFound("分配来源不存在".into()))?;
        let result = self.allocation_source_view(&account, ex).await?;

        authorization.deliver(ex).await?;
        Ok(result)
    }

    // 只在详情功能资格成立后，把精确分录转换为它的真实应收子账。
    async fn allocation_account_id(
        &self,
        kind: AllocationSourceKind,
        id: &str,
        ex: &mut dyn Executor,
    ) -> Result<String> {
        let account_id = match kind {
            AllocationSourceKind::ReceivableAccount => id.to_owned(),
            AllocationSourceKind::ReceivableEntry => self
                .db
                .receivable_entries()
                .find_by_id(id, ex)
                .await?
                .ok_or_else(|| Error::NotFound("分配来源不存在".into()))?
                .receivable_account_id
                .to_string(),
        };
        Ok(account_id)
    }

    // 生成财务分配办理资料视图时，根子账已经过原查询过滤后，只装配该来源的固定商业字段。
    async fn allocation_source_view(
        &self,
        account: &ReceivableAccount,
        ex: &mut dyn Executor,
    ) -> Result<AllocationSource> {
        let revision = self
            .db
            .sales_order_revisions()
            .find_by_id(account.source_sales_order_revision_id.as_ref(), ex)
            .await?
            .filter(|revision| revision.sales_order_id == account.sales_order_id)
            .ok_or_else(|| Error::NotFound("应收来源版本不存在或不匹配".into()))?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(account.sales_order_id.as_ref(), ex)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;
        let owner_name = self.db.accounts().find_by_id(&order.sales_owner_user_id, ex).await?.map(|a| a.name);
        Ok(AllocationSource {
            customer_name: revision.customer_snapshot.customer_name,
            project_name: revision.project_name,
            contract_no: revision.contract_snapshot.map(|v| v.contract_no),
            owner_name,
            products: self.allocation_products(&account.source_sales_order_revision_id, ex).await?,
        })
    }
    async fn allocation_products(
        &self,
        revision: &SalesOrderRevisionId,
        ex: &mut dyn Executor,
    ) -> Result<Vec<AllocationProduct>> {
        let lines = self.db.sales_order_revision_lines().list_lines_by_revision(revision, ex).await?;
        let ids = lines.iter().map(|line| SalesOrderRevisionLineId::new(&line.base.id)).collect::<Vec<_>>();
        let goods =
            self.db.sales_order_goods_service_line_revisions().list_by_revision_line_ids(&ids, ex).await?;
        let vouchers =
            self.db.sales_order_voucher_line_revisions().list_by_revision_line_ids(&ids, ex).await?;

        Ok(lines
            .into_iter()
            .map(|line| AllocationProduct {
                quantity: goods
                    .iter()
                    .find(|v| v.revision_line_id.as_ref() == line.base.id)
                    .map(|v| v.quantity.to_string())
                    .or_else(|| {
                        vouchers
                            .iter()
                            .find(|v| v.revision_line_id.as_ref() == line.base.id)
                            .map(|v| v.card_count.to_string())
                    }),
                name: line.item_name_snapshot,
                spec: line.spec_snapshot,
                unit: line.unit_snapshot,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_finance::ports::funds_scope::FailClosedFundsDataScopePort;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    // 验证财务分配办理资料：两类来源都必须先完成统一资格检查，拒绝时不读取分录或子账。
    #[tokio::test]
    async fn allocation_sources_require_qualification_before_business_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("allocation_source_authorization");
        let service =
            FundsAccess::new(db, unavailable_authorization(), FailClosedFundsDataScopePort::shared());
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);

        for kind in [AllocationSourceKind::ReceivableEntry, AllocationSourceKind::ReceivableAccount] {
            let result = service
                .load_allocation_source(&actor, kind, "source", &mut SnapshotExecutor::default())
                .await;
            assert!(matches!(
                result,
                Err(Error::Internal(reason)) if reason == "测试授权设施不可用"
            ));
        }
    }
}
