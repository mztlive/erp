//! 草稿工作副本补开：驳回回草稿后没有 `Editing` 副本时，按本次草稿新建一份。

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::ids::SalesOrderId;
use erp_sales::dto::sales_order::SalesOrderDraftRequest;
use erp_sales::entity::sales_order::{
    SalesOrder, SalesOrderLine, SalesOrderWorkingCopy, SalesOrderWorkingCopyLine, WorkingPurpose,
};
use erp_sales::repository::SalesOrderExt;
use erp_sales::service::sales_order::draft_working_copy::DraftStableLines;
use persistence_core::{NoTransaction, Transactional};

use super::SalesOrderCommandProcess;
use crate::{Error, Result};
impl SalesOrderCommandProcess {
    /// 查找有效首次提交工作副本；草稿且没有有效副本时新开一份并落库。
    ///
    /// # 参数
    /// * `order` - 当前销售单
    /// * `req_version` - 客户端乐观锁版本；仅已有有效副本时校验
    /// * `draft` - 本次草稿；仅补开新副本时使用
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// `Ok((working_copy, stable, opened_new))`：
    /// - 已有有效副本时 `opened_new=false`，`stable.created` 可能含补建行号；
    /// - 新开副本时 `opened_new=true`，调用方不得再走覆盖保存。
    ///
    /// # 错误
    /// * `ConflictError` - 非草稿，或已有副本但版本不一致
    /// * `ValidationError` - 新开副本的草稿内容非法
    /// * 仓储错误
    ///
    /// # 约束
    /// 新开副本时本方法已完成事务写入；调用方只需返回视图。
    pub(super) async fn load_or_reopen_first_submission_working_copy(
        &self,
        order: &SalesOrder,
        req_version: u64,
        draft: &SalesOrderDraftRequest,
        actor: &AuditActor,
    ) -> Result<(SalesOrderWorkingCopy, DraftStableLines, bool)> {
        order
            .ensure_first_submission_working_copy_editable()
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        let order_id = SalesOrderId::new(order.base.id.clone());
        let stable = self.sales().collect_stable_lines_for_draft(&order_id, &draft.lines).await?;
        if let Some(working_copy) = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, &mut NoTransaction)
            .await?
        {
            if !working_copy.matches_version(req_version) {
                return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
            }
            return Ok((working_copy, stable, false));
        }

        let (working_copy, working_copy_lines) =
            erp_sales::service::sales_order::SalesOrderService::build_reopened_first_submission_working_copy(
                order,
                &stable.all,
                draft,
                actor,
            )?;
        let working_copy = self
            .persist_reopened_first_submission_working_copy(
                order,
                stable.created,
                working_copy,
                working_copy_lines,
                actor,
            )
            .await?;
        Ok((working_copy, DraftStableLines { all: Vec::new(), created: Vec::new() }, true))
    }

    /// 把新开的首次提交工作副本、明细和补建的稳定行写入同一事务。
    ///
    /// # 参数
    /// * `order` - 当前销售单，用于版本重验及合同／客户 detail 重验
    /// * `created_stable_lines` - 本次新建的稳定明细
    /// * `working_copy` - 新开工作副本
    /// * `working_copy_lines` - 新开工作副本行
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回已落库的工作副本。
    ///
    /// # 错误
    /// 合同／客户越权、版本冲突或仓储写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 必须与旧的 `Submitted` 副本并存；handler 事前检查不能代替事务内 `related`。
    async fn persist_reopened_first_submission_working_copy(
        &self,
        order: &SalesOrder,
        created_stable_lines: Vec<SalesOrderLine>,
        working_copy: SalesOrderWorkingCopy,
        working_copy_lines: Vec<SalesOrderWorkingCopyLine>,
        actor: &AuditActor,
    ) -> Result<SalesOrderWorkingCopy> {
        let order_id = order.base.id.clone();
        let expected_order_version = order.base.version;
        let audit = actor.clone().resource_log("sales_order.save_draft", "sales_order", order_id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let sellable_refs = erp_sales::service::sales_order::SalesOrderService::sellable_working_copy_refs(
            &working_copy_lines,
        )?;
        let access = self.command_access(actor, "update")?;
        let related_order = order.clone();
        let persisted = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    access.related_order(&related_order, session).await?;
                    access.revalidate(&order_id, expected_order_version, session).await?;
                    erp_sales::service::sales_order::SalesOrderService::new(db.clone())
                        .ensure_sellable_refs(
                            &sellable_refs,
                            &crate::order_to_cash::adapters::catalog::CatalogQualificationAdapter::new(
                                db.clone(),
                            ),
                            session,
                        )
                        .await?;
                    erp_sales::service::sales_order::SalesOrderService::new(db.clone())
                        .create_working_copy(
                            &created_stable_lines,
                            &working_copy,
                            &working_copy_lines,
                            session,
                        )
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SalesOrderWorkingCopy, crate::Error>(working_copy)
                })
            })
            .await?;
        Ok(persisted)
    }
}
