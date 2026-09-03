//! 草稿工作副本补开：驳回回草稿后没有 `Editing` 副本时，按本次草稿新建一份。

use database::{AccessControlExt, NoTransaction, SalesOrderExt, Transactional};
use entities::ids::{SalesOrderId, SalesOrderLineId};
use entities::sales_order::{
    SalesOrder, SalesOrderLine, SalesOrderLineData, SalesOrderWorkingCopy, SalesOrderWorkingCopyLine,
    WorkingPurpose,
};
use id_generator::next_id;

use super::dto::{SalesOrderDraftLineRequest, SalesOrderDraftRequest};
use super::mapper::build_working_copy;
use super::SalesOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

/// 草稿保存用的稳定明细：已有行 + 本次需要新建的行。
pub(super) struct DraftStableLines {
    /// 行号对齐后的完整稳定明细（含尚未落库的新行）。
    pub all: Vec<SalesOrderLine>,
    /// 本次新建、需在同一事务内写入的稳定明细。
    pub created: Vec<SalesOrderLine>,
}

impl SalesOrderService {
    /// 按草稿行号补齐稳定明细：已有行复用，缺失行号在内存中新建。
    ///
    /// # 参数
    /// * `order_id` - 所属销售单
    /// * `draft_lines` - 本次保存的草稿行
    ///
    /// # 返回
    /// 返回完整稳定明细与尚未落库的新建行。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 约束
    /// 不在本方法内写库；新建行必须与工作副本写入同一事务。
    pub(super) async fn collect_stable_lines_for_draft(
        &self,
        order_id: &SalesOrderId,
        draft_lines: &[SalesOrderDraftLineRequest],
    ) -> Result<DraftStableLines> {
        let existing = self
            .db
            .sales_order_lines()
            .list_lines_by_order(order_id, &mut NoTransaction)
            .await?;
        let mut all = existing;
        let mut created = Vec::new();
        for line in draft_lines {
            if all.iter().any(|stable| stable.line_no == line.line_no) {
                continue;
            }
            let new_line = SalesOrderLine::new(
                SalesOrderLineId::new(next_id()),
                order_id.clone(),
                SalesOrderLineData {
                    line_no: line.line_no,
                },
            )?;
            created.push(new_line.clone());
            all.push(new_line);
        }
        Ok(DraftStableLines { all, created })
    }

    /// 为已回草稿、但没有有效首次提交工作副本的销售单新开编辑中副本。
    ///
    /// # 参数
    /// * `order` - 已确认处于草稿的销售单
    /// * `stable_lines` - 行号已对齐的稳定明细
    /// * `draft` - 本次保存的表头与明细
    /// * `actor` - 当前编辑人
    ///
    /// # 返回
    /// 返回新建的工作副本实体及明细行（尚未落库）。
    ///
    /// # 错误
    /// 草稿字段组、金额或行清单校验失败时返回错误。
    ///
    /// # 约束
    /// 旧的 `Submitted` 副本保持历史，不回写；新副本 `working_purpose` 仍是首次提交。
    pub(super) fn build_reopened_first_submission_working_copy(
        order: &SalesOrder,
        stable_lines: &[SalesOrderLine],
        draft: &SalesOrderDraftRequest,
        actor: &AuditActor,
    ) -> Result<(SalesOrderWorkingCopy, Vec<SalesOrderWorkingCopyLine>)> {
        build_working_copy(order, stable_lines, draft, 1, actor)
    }

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
        let stable = self
            .collect_stable_lines_for_draft(&order_id, &draft.lines)
            .await?;
        if let Some(working_copy) = self
            .db
            .sales_order_working_copies()
            .find_active_by_order_and_purpose(&order_id, WorkingPurpose::FirstSubmission, &mut NoTransaction)
            .await?
        {
            if !working_copy.matches_version(req_version) {
                return Err(Error::ConflictError(
                    "数据已被其他请求修改，请刷新后重试".to_string(),
                ));
            }
            return Ok((working_copy, stable, false));
        }

        let (working_copy, working_copy_lines) =
            Self::build_reopened_first_submission_working_copy(order, &stable.all, draft, actor)?;
        let working_copy = self
            .persist_reopened_first_submission_working_copy(
                order.base.id.as_str(),
                stable.created,
                working_copy,
                working_copy_lines,
                actor,
            )
            .await?;
        Ok((
            working_copy,
            DraftStableLines {
                all: Vec::new(),
                created: Vec::new(),
            },
            true,
        ))
    }

    /// 把新开的首次提交工作副本、明细和补建的稳定行写入同一事务。
    ///
    /// # 参数
    /// * `order_id` - 销售单主键（审计资源）
    /// * `created_stable_lines` - 本次新建的稳定明细
    /// * `working_copy` - 新开工作副本
    /// * `working_copy_lines` - 新开工作副本行
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回已落库的工作副本。
    ///
    /// # 错误
    /// 事务或仓储写入失败时返回错误。
    ///
    /// # 约束
    /// 必须与旧的 `Submitted` 副本并存；部分唯一索引只约束 `Editing`/`Conflict`。
    async fn persist_reopened_first_submission_working_copy(
        &self,
        order_id: &str,
        created_stable_lines: Vec<SalesOrderLine>,
        working_copy: SalesOrderWorkingCopy,
        working_copy_lines: Vec<SalesOrderWorkingCopyLine>,
        actor: &AuditActor,
    ) -> Result<SalesOrderWorkingCopy> {
        let audit =
            actor
                .clone()
                .resource_log("sales_order.save_draft", "sales_order", order_id.to_string())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let sellable_refs = Self::sellable_working_copy_refs(&working_copy_lines)?;
        let persisted = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    SalesOrderService::new(db.clone())
                        .ensure_sellable_refs(&sellable_refs, session)
                        .await?;
                    for line in &created_stable_lines {
                        db.sales_order_lines().create(line, session).await?;
                    }
                    db.sales_order_working_copies()
                        .create(&working_copy, session)
                        .await?;
                    for line in &working_copy_lines {
                        db.sales_order_working_copy_lines().create(line, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SalesOrderWorkingCopy, crate::errors::Error>(working_copy)
                })
            })
            .await?;
        Ok(persisted)
    }
}
