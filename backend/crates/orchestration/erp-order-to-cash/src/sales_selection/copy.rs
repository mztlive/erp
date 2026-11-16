//! 复制选品册的跨域授权与事务入口。

use application_core::AuditActor;
use erp_customer::{CustomerAccountStatus, CustomerExt};
use erp_party::PartyExt;
use erp_permissions::sales_selection;
use erp_sales::dto::sales_selection::{CopySalesSelectionBookletRequest, SalesSelectionBookletView};
use erp_sales::entity::sales_selection::IdempotencyOperation;
use erp_sales::ports::sales_selection::SelectionCustomerFact;
use erp_sales::service::sales_selection::SalesSelectionService;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::{SalesSelectionProcess, check_create_scope};
use crate::{Error, Result};

impl SalesSelectionProcess {
    /// 复制原册内容，仅替换客户；保留责任归属并重新证明创建范围。
    ///
    /// # 参数
    /// * `id` - 原册身份
    /// * `req` - 新客户、原册版本及幂等键
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 返回独立新册，已有陈列时进入待发布。
    ///
    /// # 错误
    /// 无读取或创建权限、客户停用、版本冲突或事务失败时拒绝。
    pub async fn copy(
        &self,
        id: String,
        req: CopySalesSelectionBookletRequest,
        actor: AuditActor,
    ) -> Result<SalesSelectionBookletView> {
        let fingerprint = self.crypto.request_fingerprint(IdempotencyOperation::Copy, &(&id, &req))?;
        let access = self.services.selections.clone();
        let customer_access = self.services.customer_access.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let source = access
                        .require_booklet(&actor, &sales_selection::BOOKLET_DETAIL, &id, executor)
                        .await?;
                    customer_access.require_current(&actor, &req.customer_id, executor).await?;
                    check_create_scope(
                        &access,
                        &actor,
                        &req.customer_id,
                        &source.sales_owner_user_id,
                        &source.business_org_unit_id,
                        executor,
                    )
                    .await?;
                    let customer = customer_fact(&db, &req.customer_id, executor).await?;
                    SalesSelectionService::new(db, access.authorization())
                        .copy(&source, &req, customer, actor.id(), &fingerprint, executor)
                        .await
                        .map_err(Error::from)
                })
            })
            .await
    }
}

async fn customer_fact(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<SelectionCustomerFact> {
    let account = db
        .customer_accounts()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("客户不存在或无权查看".into()))?;
    let party = db.party().find_with_current_revision(&account.party_id, executor).await?;
    let name = party
        .and_then(|(_, revision)| revision)
        .map(|revision| revision.legal_name)
        .unwrap_or_else(|| account.customer_no.clone());
    Ok(SelectionCustomerFact {
        id: account.base.id,
        customer_no: account.customer_no,
        display_name: name,
        active: account.stable.status == CustomerAccountStatus::Active,
    })
}
