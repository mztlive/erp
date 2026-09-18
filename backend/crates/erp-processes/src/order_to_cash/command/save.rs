use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_sales::dto::sales_order::{SaveWorkingCopyRequest, WorkingCopyView};
use erp_sales::entity::sales_order::SalesOrderWorkingCopy;
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::super::SalesOrderCommandProcess;
use crate::{Error, Result};

impl SalesOrderCommandProcess {
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
        let (customer_id, settlement_party_id, draft) = self
            .resolve_sales_command_draft(&access, &req.contract_id, req.draft, &mut NoTransaction)
            .await?;
        let order = authorized_order;
        let expected_order_version = order.base.version;
        if !order.matches_contract_context(&req.contract_id, &customer_id, &settlement_party_id) {
            return Err(Error::ConflictError("销售单合同归属已变化，请刷新后重试".to_string()));
        }
        self.sales().ensure_sellable_draft_lines(&draft.lines, &self.catalog()).await?;
        let (mut working_copy, stable, opened_new) =
            self.load_or_reopen_first_submission_working_copy(&order, req.version, &draft, actor).await?;
        if opened_new {
            return Ok(self.sales().working_copy_view(&working_copy).await?);
        }

        let lines = erp_sales::service::sales_order::SalesOrderService::prepare_saved_working_copy(
            &order,
            &mut working_copy,
            &stable,
            &draft,
            actor,
        )?;
        let created_stable_lines = stable.created;

        let old_lines = self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&working_copy.base.id.clone().into(), &mut NoTransaction)
            .await?;
        let audit = actor.clone().resource_log("sales_order.save_draft", "sales_order", id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let lines_for_tx = lines.clone();
        let created_stable_for_tx = created_stable_lines;
        let sellable_refs_for_tx =
            erp_sales::service::sales_order::SalesOrderService::sellable_working_copy_refs(&lines)?;
        let working_copy = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    access.related_order(&order, session).await?;
                    access.revalidate(&order.base.id, expected_order_version, session).await?;
                    erp_sales::service::sales_order::SalesOrderService::new(db.clone())
                        .ensure_sellable_refs(
                            &sellable_refs_for_tx,
                            &crate::order_to_cash::adapters::catalog::CatalogQualificationAdapter::new(
                                db.clone(),
                            ),
                            session,
                        )
                        .await?;
                    erp_sales::service::sales_order::SalesOrderService::new(db.clone())
                        .persist_saved_working_copy(
                            &created_stable_for_tx,
                            old_lines,
                            &lines_for_tx,
                            &mut working_copy,
                            session,
                        )
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SalesOrderWorkingCopy, crate::Error>(working_copy)
                })
            })
            .await?;

        Ok(self.sales().working_copy_view(&working_copy).await?)
    }
}
