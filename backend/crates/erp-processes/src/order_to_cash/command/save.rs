use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditLog};
use erp_sales::dto::sales_order::{SaveWorkingCopyRequest, WorkingCopyView};
use erp_sales::entity::sales_order::{
    SalesOrder, SalesOrderLine, SalesOrderWorkingCopy, SalesOrderWorkingCopyLine,
};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_sales::service::sales_order::SalesOrderService;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::super::SalesOrderCommandProcess;
use super::super::adapters::catalog::CatalogQualificationAdapter;
use super::super::authorization::SalesCommandAccess;
use crate::Result;
use crate::audit::persist_log;

impl SalesOrderCommandProcess {
    /// 按准备阶段的副本身份读取原活跃行，保留原独立展示执行器和查询顺序。
    async fn saved_draft_lines(
        &self,
        copy: &SalesOrderWorkingCopy,
    ) -> Result<Vec<SalesOrderWorkingCopyLine>> {
        Ok(self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&copy.base.id.clone().into(), &mut NoTransaction)
            .await?)
    }

    /// 保存草稿（整表头覆盖 + 明细整批替换，乐观锁语义）。
    ///
    /// 采购/销售驳回后订单回到草稿，但首次提交工作副本已是 `Submitted` 终态时，
    /// 会新开一份 `Editing` 副本，而不是返回「有效工作副本不存在」。
    /// 已有有效副本时，`req.version` 必须与当前工作副本版本一致；新开副本不校验
    /// 该版本（前端在无副本时会把销售单版本误当成草稿版本）。
    /// 行替换在事务内「软删旧行 + 写入新行」原子完成。
    ///
    /// # 参数
    /// * `id` - 销售单 ID
    /// * `req` - 保存请求（含期望版本与草稿内容）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回保存后的工作副本视图。
    ///
    /// # 错误
    /// * `NotFound` - 销售单不存在
    /// * `ConflictError` - 非草稿，或已有副本但期望版本不一致
    #[tracing::instrument(
        name = "sales_order.save_working_copy",
        skip_all,
        fields(layer = "service", domain = "sales_order", operation = "save_working_copy")
    )]
    pub async fn save_working_copy(
        &self,
        id: &str,
        req: SaveWorkingCopyRequest,
        actor: &AuditActor,
    ) -> Result<WorkingCopyView> {
        req.validate()?;
        let access = self.command_access(actor, "update")?;
        let authorized_order = access.current(id, &mut NoTransaction).await?;
        let (order, draft) = self
            .prepare_sales_edit(
                &access,
                authorized_order,
                (req.contract_id, req.customer_id),
                req.draft,
                actor,
                false,
            )
            .await?;
        let expected_order_version = order.base.version;
        self.sales().ensure_sellable_draft_lines(&draft.lines, &self.catalog()).await?;
        let (mut working_copy, stable, opened_new) =
            self.load_or_reopen_first_submission_working_copy(&order, req.version, &draft, actor).await?;
        if opened_new {
            return Ok(self.sales().working_copy_view(&working_copy).await?);
        }

        let lines =
            SalesOrderService::prepare_saved_working_copy(&order, &mut working_copy, &stable, &draft, actor)?;
        let created_stable_lines = stable.created;

        let old_lines = self.saved_draft_lines(&working_copy).await?;
        let audit = actor.clone().resource_log("sales_order.save_draft", "sales_order", id.to_string())?;
        let write = SavedWorkingCopyWrite {
            db: self.db.clone(),
            access,
            order,
            expected_order_version,
            working_copy,
            created_stable_lines,
            old_lines,
            lines: lines.clone(),
            sellable_refs: SalesOrderService::sellable_working_copy_refs(&lines)?,
            audit,
        };
        let client = write.db.client().clone();
        let working_copy = client.with_transaction(move |executor| Box::pin(write.persist(executor))).await?;

        Ok(self.sales().working_copy_view(&working_copy).await?)
    }
}

/// 草稿保存的原子写集合；仅由保存入口建立事务。
struct SavedWorkingCopyWrite {
    db: Database,
    access: SalesCommandAccess,
    order: SalesOrder,
    expected_order_version: u64,
    working_copy: SalesOrderWorkingCopy,
    created_stable_lines: Vec<SalesOrderLine>,
    old_lines: Vec<SalesOrderWorkingCopyLine>,
    lines: Vec<SalesOrderWorkingCopyLine>,
    sellable_refs: Vec<(String, String)>,
    audit: AuditLog,
}

impl SavedWorkingCopyWrite {
    async fn persist(mut self, executor: &mut dyn Executor) -> Result<SalesOrderWorkingCopy> {
        self.access.related_order(&self.order, executor).await?;
        self.access.revalidate(&self.order.base.id, self.expected_order_version, executor).await?;
        SalesOrderCommandProcess::persist_first_contract_binding(
            &self.db,
            &self.access,
            &self.order,
            self.working_copy.contract_revision_id.as_ref(),
            executor,
        )
        .await?;
        SalesOrderService::new(self.db.clone())
            .ensure_sellable_refs(
                &self.sellable_refs,
                &CatalogQualificationAdapter::new(self.db.clone()),
                executor,
            )
            .await?;
        SalesOrderService::new(self.db.clone())
            .persist_saved_working_copy(
                &self.created_stable_lines,
                self.old_lines,
                &self.lines,
                &mut self.working_copy,
                executor,
            )
            .await?;
        persist_log(&self.db, &self.audit, executor).await?;
        Ok(self.working_copy)
    }
}
