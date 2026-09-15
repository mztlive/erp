//! 采购正式版本、应付、付款任务、成本与履约的同事务组合。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::SharedRbacService;
use erp_procurement::dto::purchase_order::PurchaseReviewResult;
use mongodb::{ClientSession, Database};
use persistence_core::Transactional;

use super::PurchaseOrderProcess;
use super::review::FormalizedOrderPersist;
use crate::Result;

/// 采购最终通过流程，财务与履约步骤严格复用原 Executor 和写入顺序。
pub struct PurchaseOrderFormalizationProcess {
    db: Database,
    rbac: SharedRbacService,
}
impl PurchaseOrderFormalizationProcess {
    /// 使用组合根数据库与授权源构造采购形式化流程。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 最终通过并生效：形成采购版本、应付与成本事实。
    ///
    /// 仅由合同 §4.4.4 `on_final_approve` 调用，不得再作为人工财务审核旁路。
    ///
    /// # 参数
    /// * `id` - 采购单主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回生效结果。
    ///
    /// # 错误
    /// 非审批中、缺少提交、来源复验失败或仓储失败时返回错误。
    pub async fn formalize_approved_order(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<PurchaseReviewResult> {
        let service = PurchaseOrderProcess::with_rbac(self.db.clone(), self.rbac.clone());
        let (persist, result) = service.prepare_formalized_order(id, actor).await?.into_parts();
        let db = self.db.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move { persist_formalized_order_write(&db, persist, &actor, session).await })
            })
            .await?;
        Ok(result)
    }

    /// 在审批运行时持有的事务内形式化最终通过的采购单。
    ///
    /// # 错误
    /// 提交、来源复验、应付/成本或履约草稿写入失败时返回错误。
    pub async fn formalize_approved_order_in_transaction(
        &self,
        id: &str,
        actor: &AuditActor,
        session: &mut ClientSession,
    ) -> Result<()> {
        let service = PurchaseOrderProcess::with_rbac(self.db.clone(), self.rbac.clone());
        let (persist, _) = service.prepare_formalized_order(id, actor).await?.into_parts();
        persist_formalized_order_write(&self.db, persist, actor, session).await
    }
}

/// 在同一事务内写入生效版本、采购单、提交结论、应付、任务、成本、履约与审计。
async fn persist_formalized_order_write(
    db: &Database,
    persist: FormalizedOrderPersist,
    actor: &AuditActor,
    session: &mut ClientSession,
) -> Result<()> {
    let audit = actor.clone().resource_log(
        "purchase_order.formalize",
        "purchase_order",
        persist.order_id().to_string(),
    )?;
    super::formalization_posting::post(db, persist, actor, &audit, session).await
}
