//! 销售首次生效的订单、应收、财务任务与审计组合流程。

use application_core::AuditActor;
use entities::sales_order::{SalesOrder, SalesOrderRevisionAggregate};
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_finance::entity::receivable::SalesBusinessTypeFact;
use erp_finance::service::receivable::initial_account::{create_initial_receivable, InitialReceivableInput};
use erp_identity::SharedRbacService;
use mongodb::Database;
use persistence_core::{NoTransaction, Transactional};
use services::sales_order::{FormalizedSubmissionWrite, SalesOrderDetailView, SalesOrderService};
use services::Result;

/// 以同一事务完成销售形式化与首次应收，不改变授权栅栏和重复生效判断。
pub struct SalesOrderFormalizationProcess {
    db: Database,
    rbac: SharedRbacService,
}
impl SalesOrderFormalizationProcess {
    /// 使用组合根数据库与授权源创建销售形式化流程。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 最终通过并形式化已批准提交。
    ///
    /// 只包装既有 `repository formalize_submission`：先把销售单推进到
    /// `EFFECTIVE` / `APPROVED`，再写入正式修订。
    /// 不得 `$set` 绕过领域不变式，也不得按卡券运营节点写专用副作用分支。
    ///
    /// # 参数
    /// * `id` - 销售单主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回形式化后的销售单详情。
    ///
    /// # 错误
    /// 非审批中、缺少提交或仓储失败时返回错误。
    #[tracing::instrument(
        name = "sales_order.formalize_approved_submission",
        skip_all,
        fields(
            layer = "service",
            domain = "sales_order",
            operation = "formalize_approved_submission"
        )
    )]
    pub async fn formalize_approved_submission(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        let service = SalesOrderService::with_rbac(self.db.clone(), self.rbac.clone());
        if let Some(write) = service
            .prepare_approved_submission(id, actor, &mut NoTransaction)
            .await?
        {
            let audit = actor.clone().resource_log(
                "sales_order.formalize",
                "sales_order",
                write.order_id().to_string(),
            )?;
            let policy_revision = write.policy_revision();
            let db = self.db.clone();
            if let Some(policy_revision) = policy_revision {
                self.rbac
                    .run_authorized_policy_transaction(policy_revision, move |session| {
                        Box::pin(
                            async move { persist_formalized_submission(&db, write, &audit, session).await },
                        )
                    })
                    .await?;
            } else {
                self.db
                    .client()
                    .with_transaction(move |session| {
                        Box::pin(
                            async move { persist_formalized_submission(&db, write, &audit, session).await },
                        )
                    })
                    .await?;
            }
        }
        service.sales_order_detail(id, None).await
    }

    /// 在审批运行时持有的事务内形式化最终通过的销售单。
    ///
    /// # 参数
    /// * `id` - 销售单主键
    /// * `actor` - 已认证操作人
    /// * `session` - 审批运行时持有的唯一事务会话
    ///
    /// # 返回
    /// 正式版本、应收、供给任务和成功审计全部写入时返回 `Ok(())`。
    ///
    /// # 错误
    /// 单据状态、提交、采购责任或持久化不变量失败时返回错误。
    pub async fn formalize_approved_submission_in_transaction(
        &self,
        id: &str,
        actor: &AuditActor,
        session: &mut mongodb::ClientSession,
    ) -> Result<()> {
        let service = SalesOrderService::with_rbac(self.db.clone(), self.rbac.clone());
        if let Some(write) = service.prepare_approved_submission(id, actor, session).await? {
            let audit = actor.clone().resource_log(
                "sales_order.formalize",
                "sales_order",
                write.order_id().to_string(),
            )?;
            persist_formalized_submission(&self.db, write, &audit, session).await?;
        }
        Ok(())
    }
}

/// 销售版本、首次应收、卡券复核、开票任务与成功审计严格复用原写入顺序。
async fn persist_formalized_submission(
    db: &Database,
    write: FormalizedSubmissionWrite,
    audit: &erp_audit::AuditLog,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let (order, aggregate, now) = write.persist_in_transaction(session).await?;
    // 销售单生效即形成原始应收（§6.8/§8.1.1）：子账 + 原始应收分录原子写入。
    // 后续销售变更差额由 sales_review 生效路径另行入账，本路径只写首次生效。
    create_original_receivable(db, &order, &aggregate, now, session).await?;
    db.audit_logs().create(audit, session).await?;
    Ok(())
}
/// 映射已生效销售事实并形成首次应收，再按原顺序推进票款复核和开票任务。
///
/// 只应由首次生效（最终审批通过）路径调用：子账 `account_seq = 1`，分录类型
/// 为原始应收（增加方向，金额 = 版本含税合计）；后续销售变更差额由
/// `sales_review` 生效路径写入。写在同一事务内，重复生效由提交状态守卫拦截。
///
/// # 参数
/// * `db` - 数据库实例
/// * `order` - 已生效的销售单（事务内最新版本）
/// * `aggregate` - 生效版本聚合
/// * `posted_at` - 入账时间
/// * `session` - 事务会话执行器
///
/// # 返回
/// 无返回值；写入失败时返回错误。
async fn create_original_receivable(
    db: &Database,
    order: &SalesOrder,
    aggregate: &SalesOrderRevisionAggregate,
    posted_at: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let account = create_initial_receivable(
        db,
        InitialReceivableInput {
            business_type: match order.business_type {
                entities::sales_order::BusinessType::GoodsService => SalesBusinessTypeFact::GoodsService,
                entities::sales_order::BusinessType::Voucher => SalesBusinessTypeFact::Voucher,
            },
            sales_order_id: order.base.id.clone().into(),
            customer_id: order.customer_id.clone(),
            counterparty_party_id: order.settlement_party_id.clone(),
            source_sales_order_revision_id: aggregate.revision.base.id.clone().into(),
            gross_total: aggregate.revision.gross_amount,
            posted_at,
        },
        session,
    )
    .await?;
    crate::finance_posting::receivable::card_funds_task::ensure_initial_card_funds_review_task(
        db, &account, session,
    )
    .await?;
    crate::finance_posting::receivable::invoice_task::ensure_sales_invoice_task(db, &account, session)
        .await?;
    Ok(())
}
