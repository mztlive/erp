//! Receivable account creation coordinating finance, workflow tasks and audit.

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_core::ids::{ReceivableAccountId, ReceivableEntryId, SalesOrderRevisionId};
use erp_finance::dto::receivable::CreateReceivableAccountRequest;
use erp_finance::entity::receivable::{
    AccountReviewStatus, EntryDirection, ReceivableAccount, ReceivableAccountData, ReceivableEntry,
    ReceivableEntryData, ReceivableEntryType, SalesBusinessTypeFact,
};
use erp_finance::repository::ReceivableExt;
use erp_finance::service::receivable::mapping::zero_amount;
use erp_identity::entity::authorization::{ObjectFacts, RelatedObjects, RelationKey};
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::receivable;
use erp_read_models::finance::dto::ReceivableAccountView;
use erp_read_models::finance::receivable::ReceivableReadService;
use erp_sales::entity::sales_order::{BusinessType, SalesOrder};
use erp_sales::repository::SalesOrderExt;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::ReceivableProcess;
use crate::{Error, FinancePostingPorts, Result};

impl ReceivableProcess {
    // -----------------------------------------------------------------------
    // 应收往来子账
    // -----------------------------------------------------------------------

    /// 建立应收往来子账与原始应收分录（跨集合事务写入）。
    ///
    /// 校验来源销售单存在（D13 Repository），同事务写入子账与分录，
    /// 保证「子账 + 原始应收」原子可见（数据模型 §6.8）。业务幂等唯一
    /// `(receivable_account_id, source_fact_type, source_document_id,
    /// source_revision_id, entry_type, source_sequence)` 由唯一索引保证，
    /// 重复提交落入 409。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建子账的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 来源销售单不存在
    /// * `ConflictError` - 业务唯一键重复（唯一索引透出）
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_receivable_account(
        &self,
        req: CreateReceivableAccountRequest,
        actor: &AuditActor,
    ) -> Result<ReceivableAccountView> {
        req.validate()?;

        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let sales_order = authorized_source(&db, &rbac, &req, &actor, executor).await?;
                    let review_status = initial_review_status(&req, &sales_order)?;

                    let account_id = ReceivableAccountId::new(next_id());
                    let entry_id = ReceivableEntryId::new(next_id());
                    let posted_at = Instant::now();
                    let account = new_account(&req, account_id.clone(), review_status, actor.id())?;
                    let entry = new_entry(&req, entry_id, account_id.clone(), &account, posted_at)?;
                    let audit = actor.clone().resource_log(
                        "receivable_account.create",
                        "receivable_account",
                        account_id.to_string(),
                    )?;

                    db.receivable().create_receivable_with_entry(&account, &entry, executor).await?;
                    persist_log(&db, &audit, executor).await?;

                    created_account_view(
                        &ReceivableReadService::new(db.clone()),
                        &rbac.authorization,
                        &actor,
                        &account.base.id,
                        account.sales_order_id.as_ref(),
                        executor,
                    )
                    .await
                })
            })
            .await
    }
}

/// 写入已推进阶段，按原创建入口和实际销售来源重新授权后读取当前事务结果。
async fn created_account_view(
    read: &ReceivableReadService,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    id: &str,
    sales_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<ReceivableAccountView> {
    let candidate = ObjectFacts {
        resource: "receivable_account".into(),
        relations: [(
            RelationKey::parse("sales_source")?,
            RelatedObjects { expected_ids: [sales_order_id.to_owned()].into(), objects: vec![] },
        )]
        .into(),
        ..Default::default()
    };
    authorization
        .require(actor, &receivable::ACCOUNT_CREATE, AccessTarget::Candidate(candidate), executor)
        .await?;

    Ok(read.receivable_account_view(id.to_owned(), executor).await?)
}

/// 把已持久化的销售业务类型显式映射为财务消费的稳定事实。
///
/// 每个来源变体都有对应项，不用缺省分支改变复核政策。
fn sales_business_type_fact(value: BusinessType) -> SalesBusinessTypeFact {
    match value {
        BusinessType::GoodsService => SalesBusinessTypeFact::GoodsService,
        BusinessType::Voucher => SalesBusinessTypeFact::Voucher,
    }
}

fn new_account(
    req: &CreateReceivableAccountRequest,
    account_id: ReceivableAccountId,
    review_status: AccountReviewStatus,
    actor_id: &str,
) -> Result<ReceivableAccount> {
    Ok(ReceivableAccount::new(
        account_id,
        ReceivableAccountData {
            sales_order_id: req.sales_order_id.clone().into(),
            account_seq: req.account_seq,
            customer_id: req.customer_id.clone(),
            counterparty_party_id: req.counterparty_party_id.clone(),
            source_sales_order_revision_id: SalesOrderRevisionId::new(&req.source_sales_order_revision_id),
            review_status,
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: req.gross_total,
            settled_total: zero_amount(),
            invoiceable_total: req.invoiceable_total.unwrap_or(req.gross_total),
            invoiced_total: zero_amount(),
        },
        actor_id,
    )?)
}

fn new_entry(
    req: &CreateReceivableAccountRequest,
    entry_id: ReceivableEntryId,
    account_id: ReceivableAccountId,
    account: &ReceivableAccount,
    posted_at: Instant,
) -> Result<ReceivableEntry> {
    Ok(ReceivableEntry::new(
        entry_id,
        ReceivableEntryData {
            receivable_account_id: account_id,
            entry_type: ReceivableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: account.gross_total,
            due_date: req.due_date,
            source_fact_type: "sales_order".to_string(),
            source_document_id: req.sales_order_id.clone(),
            source_revision_id: req.source_sales_order_revision_id.clone(),
            source_sequence: req.source_sequence,
            posted_at,
        },
    )?)
}

/// 校验来源版本并确定新建子账复核状态。
fn initial_review_status(
    req: &CreateReceivableAccountRequest,
    sales_order: &SalesOrder,
) -> Result<AccountReviewStatus> {
    let review_status = AccountReviewStatus::resolve_initial(
        req.review_status,
        sales_business_type_fact(sales_order.business_type),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    if sales_order.business_type == BusinessType::Voucher
        && sales_order.stable.current_revision_id.as_deref()
            != Some(req.source_sales_order_revision_id.as_str())
    {
        return Err(Error::ConflictError(
            "卡券应收必须按来源销售单当前生效的内容登记，请刷新后核对".to_string(),
        ));
    }
    Ok(review_status)
}

/// 同一事务校验真实销售单、绑定版本与客户，避免客户端拼接跨单来源。
async fn authorized_source(
    db: &Database,
    rbac: &FinancePostingPorts,
    req: &CreateReceivableAccountRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<SalesOrder> {
    let sales_order = db
        .sales_orders()
        .find_by_id(&req.sales_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;

    let candidate = ObjectFacts {
        resource: "receivable_account".into(),
        relations: [(
            RelationKey::parse("sales_source")?,
            RelatedObjects { expected_ids: [sales_order.base.id.clone()].into(), objects: vec![] },
        )]
        .into(),
        ..Default::default()
    };
    rbac.authorization
        .require(actor, &receivable::ACCOUNT_CREATE, AccessTarget::Candidate(candidate), executor)
        .await?;

    if sales_order.customer_id != req.customer_id {
        return Err(Error::ConflictError("应收客户与来源销售单不一致".into()));
    }
    db.sales_order_revisions()
        .find_by_id(&req.source_sales_order_revision_id, executor)
        .await?
        .filter(|revision| revision.sales_order_id.as_ref() == req.sales_order_id)
        .ok_or_else(|| Error::ConflictError("应收记录引用的销售版本不属于该销售单，请联系财务核对".into()))?;
    Ok(sales_order)
}

#[cfg(test)]
mod result_tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    /// 写后回包仍须原创建资格；错误必须先于台账及跨域事实读取。
    #[tokio::test]
    async fn created_account_reauthorizes_original_source_before_result_reads() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap().database("created_receivable");
        let read = ReceivableReadService::new(db);
        let authorization = unavailable_authorization();
        let actor = AuditActor::new("actor".into(), "operator".into(), AccountKind::Admin);

        for (executor, snapshot) in [
            (&mut ReadExecutor::default() as &mut dyn Executor, false),
            (&mut SnapshotExecutor::default() as &mut dyn Executor, true),
        ] {
            let error = created_account_view(&read, &authorization, &actor, "account", "sale", executor)
                .await
                .unwrap_err();
            if snapshot {
                assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
            } else {
                assert!(
                    matches!(error, Error::ValidationError(message) if message == "写入授权必须使用快照事务")
                );
            }
        }
    }
}
