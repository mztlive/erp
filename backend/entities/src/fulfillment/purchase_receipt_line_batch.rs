//! `PurchaseReceiptLine` 批量工厂（FUL-E03）。
//!
//! 入库行连续编号与请求到领域数据转换的唯一规则源；质量规则不复制。

use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::fulfillment::{PurchaseReceiptLine, PurchaseReceiptLineData, QualityResult};
use crate::ids::{PurchaseOrderRevisionLineId, PurchaseReceiptId, PurchaseReceiptLineId};
use crate::money::Quantity;

/// 单行领域输入（不含行号/质量结果派生）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseReceiptLineSpec {
    /// 行主键（Service 注入系统 ID）。
    pub line_id: PurchaseReceiptLineId,
    /// 采购明细。
    pub purchase_order_revision_line_id: PurchaseOrderRevisionLineId,
    /// 到货数量。
    pub received_quantity: Quantity,
    /// 合格数量。
    pub qualified_quantity: Quantity,
    /// 不合格数量。
    pub rejected_quantity: Quantity,
}

/// `PurchaseReceiptLine` 批量构造入口。
pub struct PurchaseReceiptLineBatch;

impl PurchaseReceiptLineBatch {
    /// 按输入顺序从 1 连续编号批量构造入库行。
    ///
    /// # 参数
    /// * `receipt_id` - 所属入库单主键
    /// * `specs` - 与输入顺序一致的行规格（含 Service 注入的行 ID）
    ///
    /// # 返回
    /// 返回与输入顺序一致的行实体集合。
    ///
    /// # 错误
    /// 空输入、行号溢出或数量/守恒违规时返回错误。
    ///
    /// # 约束
    /// 质量结果一律经 `QualityResult::from_quantities` 派生，不得接受调用方传入值。
    pub fn build(
        receipt_id: PurchaseReceiptId,
        specs: Vec<PurchaseReceiptLineSpec>,
    ) -> Result<Vec<PurchaseReceiptLine>> {
        if specs.is_empty() {
            return Err(Error::from("采购入库单至少需要一行"));
        }
        let mut lines = Vec::with_capacity(specs.len());
        for (index, spec) in specs.into_iter().enumerate() {
            let line_no = u32::try_from(index + 1).map_err(|_| Error::from("入库行号溢出"))?;
            let quality_result =
                QualityResult::from_quantities(spec.qualified_quantity, spec.rejected_quantity);
            lines.push(PurchaseReceiptLine::new(
                spec.line_id,
                PurchaseReceiptLineData {
                    purchase_receipt_id: receipt_id.clone(),
                    line_no,
                    purchase_order_revision_line_id: spec.purchase_order_revision_line_id,
                    received_quantity: spec.received_quantity,
                    qualified_quantity: spec.qualified_quantity,
                    rejected_quantity: spec.rejected_quantity,
                    quality_result,
                },
            )?);
        }
        Ok(lines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn spec(id: &str) -> PurchaseReceiptLineSpec {
        PurchaseReceiptLineSpec {
            line_id: PurchaseReceiptLineId::new(id),
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("po-line-1"),
            received_quantity: Quantity::from_str("10").unwrap(),
            qualified_quantity: Quantity::from_str("9").unwrap(),
            rejected_quantity: Quantity::from_str("1").unwrap(),
        }
    }

    /// 正常路径：输入顺序与行号一一对应，质量按数量派生。
    #[test]
    fn build_preserves_order_and_derives_quality() {
        let lines =
            PurchaseReceiptLineBatch::build(PurchaseReceiptId::new("r-1"), vec![spec("l-1")]).unwrap();
        assert_eq!(lines[0].line_no, 1);
        assert_eq!(lines[0].quality_result, QualityResult::Partial);
    }

    /// 全合格与全拒收均由实体规则派生。
    #[test]
    fn full_pass_and_full_reject_derive() {
        let pass = PurchaseReceiptLineBatch::build(
            PurchaseReceiptId::new("r-1"),
            vec![PurchaseReceiptLineSpec {
                qualified_quantity: Quantity::from_str("10").unwrap(),
                rejected_quantity: Quantity::from_str("0").unwrap(),
                ..spec("l-1")
            }],
        )
        .unwrap();
        assert_eq!(pass[0].quality_result, QualityResult::Passed);
        let reject = PurchaseReceiptLineBatch::build(
            PurchaseReceiptId::new("r-1"),
            vec![PurchaseReceiptLineSpec {
                qualified_quantity: Quantity::from_str("0").unwrap(),
                rejected_quantity: Quantity::from_str("10").unwrap(),
                ..spec("l-2")
            }],
        )
        .unwrap();
        assert_eq!(reject[0].quality_result, QualityResult::Rejected);
    }

    /// 数量守恒违规失败；空输入失败。
    #[test]
    fn quantity_conservation_and_empty_fail() {
        assert!(PurchaseReceiptLineBatch::build(
            PurchaseReceiptId::new("r-1"),
            vec![PurchaseReceiptLineSpec {
                received_quantity: Quantity::from_str("5").unwrap(),
                ..spec("l-1")
            }],
        )
        .is_err());
        assert!(PurchaseReceiptLineBatch::build(PurchaseReceiptId::new("r-1"), vec![]).is_err());
    }
}
