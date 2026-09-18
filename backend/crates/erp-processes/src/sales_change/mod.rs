//! 销售变更正式版本、应收差额、复核/开票任务及审计的同事务组合。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::SharedRbacService;
use erp_read_models::sales_center::review::{SalesChangeOrderDetailView, SalesChangeReadService};
use erp_sales::service::sales_review::SalesReviewService;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use crate::Result;

mod adapter;
mod authorization;
mod cancel_approval;
mod commands;
mod posting;
mod start_approval;
pub use adapter::sales_change_order_object_readable;
use posting::persist_effective_writes;

/// 销售变更最终通过流程，所有写入共享审批会话或本流程唯一根事务。
pub struct SalesChangeProcess {
    db: Database,
    rbac: SharedRbacService,
    object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
}
impl SalesChangeProcess {
    /// 使用组合根数据库和授权源创建流程。
    ///
    /// # 参数
    /// * `db` - 数据库
    /// * `rbac` - 当前 RBAC 快照
    ///
    /// # 返回
    /// 返回未注入对象读取端口的流程。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 变更命令必须沿原销售单范围独立重验，不得退回路由级授权。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac, object_read: std::sync::Arc::new(erp_workflow::FailClosedObjectReadPort) }
    }

    /// 返回写命令使用的授权源。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回构造时注入的 RBAC。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得用空授权源补公司范围。
    fn require_rbac(&self) -> Result<SharedRbacService> {
        Ok(self.rbac.clone())
    }

    /// 创建时注入组合根的审批对象读取能力，未注入时失败关闭。
    pub fn with_object_read(
        mut self,
        object_read: std::sync::Arc<dyn erp_workflow::ApprovalObjectReadPort>,
    ) -> Self {
        self.object_read = object_read;
        self
    }

    /// 最终通过并生效：生成生效修订并改写销售单。
    ///
    /// 仅由合同 §4.4.4 `on_final_approve` 调用，不得再作为人工中间旁路。
    ///
    /// # 参数
    /// * `id` - 变更单主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回生效后的变更单详情。
    ///
    /// # 错误
    /// 非审批中、缺少提交、基准版本漂移或仓储失败时返回错误。
    pub async fn apply_effective_change(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SalesChangeOrderDetailView> {
        let service = SalesReviewService::new(self.db.clone());
        let write = service.prepare_effective_change(id, actor, &mut NoTransaction).await?;
        let delta = posting::prepare_receivable_delta(&self.db, &write, actor).await?;
        let audit = actor.clone().resource_log(
            "sales_change_order.effective",
            "sales_change_order",
            write.change_id().to_string(),
        )?;
        let db = self.db.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { persist_effective_writes(&db, write, delta, &audit, executor).await })
            })
            .await?;
        SalesChangeReadService::with_rbac(self.db.clone(), self.require_rbac()?)
            .sales_change_order_detail(id, actor)
            .await
            .map_err(crate::Error::from)
    }

    /// 在审批运行时持有的事务内生效销售变更。
    ///
    /// # 错误
    /// 状态、基准版本、应收差额或持久化不变量失败时返回错误。
    pub async fn apply_effective_change_apply(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let write =
            SalesReviewService::new(self.db.clone()).prepare_effective_change(id, actor, executor).await?;
        let delta = posting::prepare_receivable_delta(&self.db, &write, actor).await?;
        let audit = actor.clone().resource_log(
            "sales_change_order.effective",
            "sales_change_order",
            write.change_id().to_string(),
        )?;
        persist_effective_writes(&self.db, write, delta, &audit, executor).await
    }
}

/// 在审批运行时持有的事务内撤回销售变更审批。
///
/// # 错误
/// 变更单不存在、动作不属于本域、状态迁移或 CAS 失败时返回原错误。
pub async fn cancel_approval(
    db: &Database,
    id: &str,
    action: erp_workflow::service::approval::policy::ApprovalDomainAction,
    actor: &AuditActor,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    use erp_audit::AuditExt;
    use erp_sales::repository::SalesReviewExt;
    let mut change = db
        .sales_change_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| crate::Error::NotFound("销售变更单不存在".to_string()))?;
    adapter::execute_sales_change_domain_action(&mut change, action, actor.id())?;
    erp_sales::service::sales_review::persist_cancelled_change(db, &mut change, executor).await?;
    let audit = actor.clone().resource_log(
        "sales_change_order.cancel_approval",
        "sales_change_order",
        id.to_string(),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}
