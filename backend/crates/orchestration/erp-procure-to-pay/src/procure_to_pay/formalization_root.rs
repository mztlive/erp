//! 采购正式版本、应付、付款任务、成本与履约的同事务组合。

use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::AuthorizationService;
use erp_procurement::dto::purchase_order::PurchaseReviewResult;
use erp_procurement::repository::PurchaseOrderExt;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::review::FormalizedOrderPersist;
use super::source_recheck::ensure_source_unaffected;
use super::{PurchaseOrderPorts, PurchaseOrderProcess};
use crate::{Error, Result};

/// 来源销售已变更并影响本采购时的最终审批提示。
const FORMALIZE_SOURCE_AFFECTED: &str = "来源销售已变更并影响本采购，请撤回后修改原单再提交";

/// 采购最终通过流程，财务与履约步骤严格复用原 Executor 和写入顺序。
pub struct PurchaseOrderFormalizationProcess {
    db: Database,
    rbac: Arc<AuthorizationService>,
    ports: Arc<PurchaseOrderPorts>,
}
impl PurchaseOrderFormalizationProcess {
    /// 使用组合根数据库与授权源构造采购形式化流程。
    ///
    /// `rbac` 为形式化时绑定采购单服务的授权源；`ports` 为与普通采购命令共用的已装配应用能力。返回形式化流程。
    pub fn new(db: Database, rbac: Arc<AuthorizationService>, ports: Arc<PurchaseOrderPorts>) -> Self {
        Self { db, rbac, ports }
    }

    /// 采购审批最终通过时，由 §4.4.4 的 `on_final_approve` 调用以形成采购正式版本、应付及成本并返回生效结果，
    /// 不可作为人工财务审核的旁路。非审批中、冻结提交缺失、来源复验或仓储失败时返回错误。
    pub async fn formalize_approved_order(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<PurchaseReviewResult> {
        let service = PurchaseOrderProcess::new(self.db.clone(), self.rbac.clone(), self.ports.clone());
        let (persist, result) = service.prepare_formalized_order(id, actor).await?.into_parts();

        let db = self.db.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { persist_formalized_order_write(&db, persist, &actor, executor).await })
            })
            .await?;
        Ok(result)
    }

    /// 审批运行时已持有事务时，复用其执行器写入最终通过的采购正式版本、应付、成本和履约草稿。
    /// 冻结提交准备、来源复验或任一写入失败时返回错误，不另开写事务。
    pub async fn formalize_approved_order_apply(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let service = PurchaseOrderProcess::new(self.db.clone(), self.rbac.clone(), self.ports.clone());
        let (persist, _) = service.prepare_formalized_order(id, actor).await?.into_parts();
        persist_formalized_order_write(&self.db, persist, actor, executor).await
    }
}

// 在同一事务内写入生效版本、采购单、提交结论、应付、任务、成本、履约与审计。
async fn persist_formalized_order_write(
    db: &Database,
    persist: FormalizedOrderPersist,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let order = db
        .purchase_orders()
        .find_by_id(persist.order_id(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
    ensure_source_unaffected(db, &order, FORMALIZE_SOURCE_AFFECTED, executor).await?;

    let audit = actor.clone().resource_log(
        "purchase_order.formalize",
        "purchase_order",
        persist.order_id().to_string(),
    )?;
    super::formalization_posting::post(db, persist, actor, &audit, executor).await
}
