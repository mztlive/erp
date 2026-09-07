//! 销售变更正式版本、应收差额、复核/开票任务及审计的同事务组合。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::SharedRbacService;
use mongodb::{ClientSession, Database};
use persistence_core::{NoTransaction, Transactional};
use services::sales_review::{SalesChangeOrderDetailView, SalesReviewService};
use services::Result;

mod posting;
use posting::persist_effective_writes;

/// 销售变更最终通过流程，所有写入共享审批会话或本流程唯一根事务。
pub struct SalesChangeProcess {
    db: Database,
}
impl SalesChangeProcess {
    /// 使用组合根数据库创建流程；授权仍由审批运行时在调用前完成。
    pub fn new(db: Database, _rbac: SharedRbacService) -> Self {
        Self { db }
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
        let write = service
            .prepare_effective_change(id, actor, &mut NoTransaction)
            .await?;
        let audit = actor.clone().resource_log(
            "sales_change_order.effective",
            "sales_change_order",
            write.change_id().to_string(),
        )?;
        let db = self.db.clone();
        self.db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move { persist_effective_writes(&db, write, &audit, session).await })
            })
            .await?;
        service.sales_change_order_detail(id).await
    }

    /// 在审批运行时持有的事务内生效销售变更。
    ///
    /// # 错误
    /// 状态、基准版本、应收差额或持久化不变量失败时返回错误。
    pub async fn apply_effective_change_in_transaction(
        &self,
        id: &str,
        actor: &AuditActor,
        session: &mut ClientSession,
    ) -> Result<()> {
        let write = SalesReviewService::new(self.db.clone())
            .prepare_effective_change(id, actor, session)
            .await?;
        let audit = actor.clone().resource_log(
            "sales_change_order.effective",
            "sales_change_order",
            write.change_id().to_string(),
        )?;
        persist_effective_writes(&self.db, write, &audit, session).await
    }
}
