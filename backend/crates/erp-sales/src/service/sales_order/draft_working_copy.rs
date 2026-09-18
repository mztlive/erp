//! Sales-owned first-submission working-copy identities and reconstruction.
use application_core::AuditActor;
use erp_core::ids::{SalesOrderId, SalesOrderLineId};
use id_generator::next_id;
use persistence_core::NoTransaction;

use super::SalesOrderService;
use super::mapper::build_working_copy;
use crate::Result;
use crate::dto::sales_order::{SalesOrderDraftLineRequest, SalesOrderDraftRequest};
use crate::entity::sales_order::{
    SalesOrder, SalesOrderLine, SalesOrderLineData, SalesOrderWorkingCopy, SalesOrderWorkingCopyLine,
};
use crate::repository::SalesOrderExt;
use crate::repository::prelude::*;
/// Existing and newly allocated stable line identities for one draft.
#[derive(Debug, Clone, Default)]
pub struct DraftStableLines {
    /// All aligned identities.
    pub all: Vec<SalesOrderLine>,
    /// Identities not yet persisted.
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
    pub async fn collect_stable_lines_for_draft(
        &self,
        order_id: &SalesOrderId,
        draft_lines: &[SalesOrderDraftLineRequest],
    ) -> Result<DraftStableLines> {
        let existing = self.db.sales_order_lines().list_lines_by_order(order_id, &mut NoTransaction).await?;
        let mut all = existing;
        let mut created = Vec::new();
        for line in draft_lines {
            if all.iter().any(|stable| stable.line_no == line.line_no) {
                continue;
            }
            let new_line = SalesOrderLine::new(
                SalesOrderLineId::new(next_id()),
                order_id.clone(),
                SalesOrderLineData { line_no: line.line_no },
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
    pub fn build_reopened_first_submission_working_copy(
        order: &SalesOrder,
        stable_lines: &[SalesOrderLine],
        draft: &SalesOrderDraftRequest,
        actor: &AuditActor,
    ) -> Result<(SalesOrderWorkingCopy, Vec<SalesOrderWorkingCopyLine>)> {
        build_working_copy(order, stable_lines, draft, 1, actor)
    }
}
