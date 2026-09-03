//! 库存调整审批快照（FUL-E11）。
//!
//! 快照责任组织、对手方、数量合计、行数与空行拒绝的唯一规则源。

use crate::approval_integration::{ApprovalSubjectCounterparty, ApprovalSubjectSnapshotPayload};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::WarehouseId;
use crate::inventory::{StockAdjustment, StockAdjustmentLine};
use crate::money::Quantity;

/// 库存调整审批快照构造入口。
pub struct StockAdjustmentApprovalSnapshot;

impl StockAdjustmentApprovalSnapshot {
    /// 冻结库存调整审批快照。
    ///
    /// # 参数
    /// * `adjustment` - 已冻结提交版本的调整单
    /// * `lines` - 当前已加载的调整明细
    /// * `submitted_by` - 提交人
    /// * `submitted_at` - 提交时间（Service 注入时钟事实）
    ///
    /// # 返回
    /// 返回复用 `ApprovalSubjectSnapshotPayload` 的冻结快照。
    ///
    /// # 错误
    /// 空行、数量合计溢出、行数溢出或组织/提交人空白时返回错误。
    ///
    /// # 约束
    /// 无 I/O、无时钟、无 ID 生成；不接受客户端定义 ID 或审批人。
    pub fn build(
        adjustment: &StockAdjustment,
        lines: &[StockAdjustmentLine],
        submitted_by: &str,
        submitted_at: Instant,
    ) -> Result<ApprovalSubjectSnapshotPayload> {
        if lines.is_empty() {
            return Err(Error::from("库存调整单没有明细，无法启动审批"));
        }
        if adjustment.warehouse_id.as_ref().trim().is_empty() {
            return Err(Error::from("库存调整单缺少仓库，无法冻结快照"));
        }
        if submitted_by.trim().is_empty() {
            return Err(Error::from("提交人不能为空"));
        }
        Ok(ApprovalSubjectSnapshotPayload {
            document_no: adjustment.adjustment_no.clone(),
            responsible_org_id: adjustment.warehouse_id.to_string(),
            submitted_by: submitted_by.to_string(),
            submitted_at,
            counterparty: Some(ApprovalSubjectCounterparty::Warehouse {
                warehouse_id: WarehouseId::new(adjustment.warehouse_id.to_string()),
            }),
            total_amount: None,
            total_quantity: Some(sum_line_quantity(lines)?),
            line_count: u32::try_from(lines.len()).map_err(|_| Error::from("调整明细行数溢出"))?,
        })
    }
}

/// 受检数量合计（十进制累加后一次性转 `Quantity`）。
///
/// # 参数
/// * `lines` - 调整明细
///
/// # 返回
/// 返回合计数量。
///
/// # 错误
/// 空行或合计超出数量标度时返回错误。
fn sum_line_quantity(lines: &[StockAdjustmentLine]) -> Result<Quantity> {
    let Some(first) = lines.first() else {
        return Err(Error::from("库存调整单没有明细，无法启动审批"));
    };
    let mut total = first.quantity.to_decimal();
    for line in &lines[1..] {
        total = total
            .checked_add(line.quantity.to_decimal())
            .ok_or_else(|| Error::from("库存调整数量合计溢出"))?;
    }
    Quantity::try_from(total).map_err(|error| Error::from(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SkuId, StockAdjustmentId, StockAdjustmentLineId};
    use crate::inventory::{MovementDirection, StockAdjustmentData, StockAdjustmentLineData};
    use std::str::FromStr;

    fn adjustment() -> StockAdjustment {
        StockAdjustment::new(
            StockAdjustmentId::new("adj-1"),
            StockAdjustmentData {
                adjustment_no: "ADJ-1".to_string(),
                warehouse_id: WarehouseId::new("wh-1"),
                reason_type: crate::inventory::AdjustmentReasonType::StockGain,
                prepared_by: "u-1".to_string(),
                note: None,
                occurred_at: None,
            },
            "creator-1",
        )
        .unwrap()
    }

    fn line(id: &str, quantity: &str) -> StockAdjustmentLine {
        StockAdjustmentLine::new(
            StockAdjustmentLineId::new(id),
            StockAdjustmentLineData {
                stock_adjustment_id: StockAdjustmentId::new("adj-1"),
                sku_id: SkuId::new("sku-1"),
                quantity: Quantity::from_str(quantity).unwrap(),
                direction: MovementDirection::Increase,
            },
        )
        .unwrap()
    }

    /// 仓库责任组织与 counterparty 精确冻结，合计/行数/编号/提交人正确。
    #[test]
    fn snapshot_freezes_org_counterparty_and_totals() {
        let payload = StockAdjustmentApprovalSnapshot::build(
            &adjustment(),
            &[line("l-1", "1"), line("l-2", "2")],
            "user-1",
            Instant::from_unix_secs(10),
        )
        .unwrap();
        assert_eq!(payload.document_no, "ADJ-1");
        assert_eq!(payload.responsible_org_id, "wh-1");
        assert_eq!(payload.total_quantity.unwrap().to_string(), "3");
        assert_eq!(payload.line_count, 2);
        assert_eq!(payload.submitted_by, "user-1");
        assert!(matches!(
            payload.counterparty,
            Some(ApprovalSubjectCounterparty::Warehouse { .. })
        ));
    }

    /// 空行失败；数量合计溢出失败。
    #[test]
    fn empty_and_overflow_fail() {
        assert!(StockAdjustmentApprovalSnapshot::build(
            &adjustment(),
            &[],
            "user-1",
            Instant::from_unix_secs(10),
        )
        .is_err());
        let huge = Quantity::try_from(rust_decimal::Decimal::MAX).unwrap();
        let overflow = StockAdjustmentLine::new(
            StockAdjustmentLineId::new("huge-2"),
            StockAdjustmentLineData {
                stock_adjustment_id: StockAdjustmentId::new("adj-1"),
                sku_id: SkuId::new("sku-1"),
                quantity: huge,
                direction: MovementDirection::Increase,
            },
        )
        .unwrap();
        assert!(StockAdjustmentApprovalSnapshot::build(
            &adjustment(),
            &[overflow.clone(), overflow],
            "user-1",
            Instant::from_unix_secs(10),
        )
        .is_err());
    }
}
