//! 销售本域首次提交工作副本的稳定行身份与重建。
use std::collections::HashSet;

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

impl DraftStableLines {
    /// 按请求顺序组装稳定行身份；重复行号复用已有身份，不重复分配 ID。
    ///
    /// # 参数
    /// * `order_id` - 所属销售单
    /// * `existing` - 已加载的稳定行，保留其顺序与身份
    /// * `draft_lines` - 本次草稿行请求
    ///
    /// # 返回
    /// 返回全部稳定行及按首次缺失顺序创建的行。
    ///
    /// # 错误
    /// 新建行的行号非法时传递实体构造错误。
    fn from_draft(
        order_id: &SalesOrderId,
        existing: Vec<SalesOrderLine>,
        draft_lines: &[SalesOrderDraftLineRequest],
    ) -> Result<Self> {
        let mut line_numbers = existing.iter().map(|line| line.line_no).collect::<HashSet<_>>();
        let mut all = existing;
        let mut created = Vec::new();
        for line in draft_lines {
            if !line_numbers.insert(line.line_no) {
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
        Ok(Self { all, created })
    }
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
    /// 数据库读取失败时返回仓储错误；新建行号非法时传递实体错误。
    ///
    /// # 约束
    /// 不在本方法内写库；新建行必须与工作副本写入同一事务。
    pub async fn collect_stable_lines_for_draft(
        &self,
        order_id: &SalesOrderId,
        draft_lines: &[SalesOrderDraftLineRequest],
    ) -> Result<DraftStableLines> {
        let existing = self.db.sales_order_lines().list_lines_by_order(order_id, &mut NoTransaction).await?;
        DraftStableLines::from_draft(order_id, existing, draft_lines)
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::money::Rate;

    use super::*;
    use crate::entity::sales_order::LineType;

    /// 构造只用于稳定身份对齐的草稿行。
    fn draft_line(line_no: u32) -> SalesOrderDraftLineRequest {
        SalesOrderDraftLineRequest {
            line_no,
            line_type: LineType::GoodsService,
            sales_tax_rate: Rate::from_str("0.13").unwrap(),
            item_name_snapshot: format!("商品-{line_no}"),
            spec_snapshot: None,
            unit_snapshot: None,
            goods: None,
            voucher: None,
        }
    }

    /// 既有重复行保留；请求复用既有行号，新身份按首次缺失顺序追加。
    #[test]
    fn draft_identities_preserve_existing_rows_and_create_missing_numbers_once() {
        let order_id = SalesOrderId::new("order-1");
        let existing = ["first", "duplicate"]
            .into_iter()
            .map(|id| {
                SalesOrderLine::new(
                    SalesOrderLineId::new(id),
                    order_id.clone(),
                    SalesOrderLineData { line_no: 2 },
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        let requests = [draft_line(2), draft_line(4), draft_line(1), draft_line(4)];
        let result = DraftStableLines::from_draft(&order_id, existing.clone(), &requests).unwrap();
        assert_eq!(&result.all[..2], existing.as_slice());
        assert_eq!(result.all.iter().map(|line| line.line_no).collect::<Vec<_>>(), [2, 2, 4, 1]);
        assert_eq!(result.created, result.all[2..]);
        assert_ne!(result.created[0].base.id, result.created[1].base.id);
        assert!(result.created.iter().all(|line| line.sales_order_id == order_id));
        let empty = DraftStableLines::from_draft(&order_id, existing.clone(), &[]).unwrap();
        assert_eq!(empty.all, existing);
        assert!(empty.created.is_empty());
    }

    /// 非法新行号沿用实体错误，不创建后续行。
    #[test]
    fn draft_identities_reject_invalid_new_line_number() {
        let error = DraftStableLines::from_draft(&SalesOrderId::new("order-1"), vec![], &[draft_line(0)])
            .unwrap_err();
        let expected = SalesOrderLine::new(
            SalesOrderLineId::new("invalid"),
            SalesOrderId::new("order-1"),
            SalesOrderLineData { line_no: 0 },
        )
        .unwrap_err();
        assert_eq!(error.to_string(), expected.to_string());
    }
}
