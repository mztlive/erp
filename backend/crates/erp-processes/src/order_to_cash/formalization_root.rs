//! 销售首次生效的订单、应收、财务任务与审计组合流程。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_identity::SharedRbacService;
use erp_read_models::sales_center::order::dto::SalesOrderDetailView;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::{FormalizedSubmissionWrite, SalesOrderCommandProcess};
use crate::Result;

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
        fields(layer = "service", domain = "sales_order", operation = "formalize_approved_submission")
    )]
    pub async fn formalize_approved_submission(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        let service = SalesOrderCommandProcess::with_rbac(self.db.clone(), self.rbac.clone());
        if let Some(write) = service.prepare_approved_submission(id, actor, &mut NoTransaction).await? {
            let audit = actor.clone().resource_log(
                "sales_order.formalize",
                "sales_order",
                write.order_id().to_string(),
            )?;
            let policy_revision = write.policy_revision();
            let db = self.db.clone();
            if let Some(policy_revision) = policy_revision {
                self.rbac
                    .run_authorized_policy_transaction(policy_revision, move |executor| {
                        Box::pin(
                            async move { persist_formalized_submission(&db, write, &audit, executor).await },
                        )
                    })
                    .await?;
            } else {
                self.db
                    .client()
                    .with_transaction(move |executor| {
                        Box::pin(
                            async move { persist_formalized_submission(&db, write, &audit, executor).await },
                        )
                    })
                    .await?;
            }
        }
        service.read_model().sales_order_detail(id, None).await.map_err(crate::Error::from)
    }

    /// 在审批运行时持有的事务内形式化最终通过的销售单。
    ///
    /// # 参数
    /// * `id` - 销售单主键
    /// * `actor` - 已认证操作人
    /// * `executor` - 审批运行时持有的执行器
    ///
    /// # 返回
    /// 正式版本、应收、供给任务和成功审计全部写入时返回 `Ok(())`。
    ///
    /// # 错误
    /// 单据状态、提交、采购责任或持久化不变量失败时返回错误。
    pub async fn formalize_approved_submission_apply(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let service = SalesOrderCommandProcess::with_rbac(self.db.clone(), self.rbac.clone());
        if let Some(write) = service.prepare_approved_submission(id, actor, executor).await? {
            let audit = actor.clone().resource_log(
                "sales_order.formalize",
                "sales_order",
                write.order_id().to_string(),
            )?;
            persist_formalized_submission(&self.db, write, &audit, executor).await?;
        }
        Ok(())
    }
}

/// 销售版本、首次应收、卡券复核、开票任务与成功审计严格复用原写入顺序。
async fn persist_formalized_submission(
    _db: &Database,
    write: FormalizedSubmissionWrite,
    audit: &erp_audit::AuditLog,
    executor: &mut dyn Executor,
) -> Result<()> {
    super::formalization_posting::post(write, audit, executor).await
}
