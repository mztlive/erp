//! `CustomerAcceptanceLine` 批量工厂（FUL-E02）。
//!
//! 验收行输入顺序、连续行号、默认无凭证与数量约束的唯一规则源。

use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::fulfillment::{CustomerAcceptanceLine, CustomerAcceptanceLineData};
use crate::ids::{CustomerAcceptanceId, CustomerAcceptanceLineId, SalesOrderLineId};
use crate::money::Quantity;

/// 单行领域输入（已验证形态，不含行号/表头/凭证）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerAcceptanceLineSpec {
    /// 行主键（Service 注入系统 ID）。
    pub line_id: CustomerAcceptanceLineId,
    /// 验收明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 通过数量。
    pub accepted_quantity: Quantity,
    /// 短少数量。
    pub short_quantity: Quantity,
    /// 拒收数量。
    pub rejected_quantity: Quantity,
    /// 依据说明。
    pub reason: Option<String>,
}

/// 验收行批量构造上限（DTO 校验 `1-200` 的领域侧等价）。
pub const ACCEPTANCE_LINE_BATCH_MAX: usize = 200;

/// `CustomerAcceptanceLine` 批量构造入口。
pub struct CustomerAcceptanceLineBatch;

impl CustomerAcceptanceLineBatch {
    /// 按输入顺序从 1 连续编号批量构造验收行。
    ///
    /// # 参数
    /// * `acceptance_id` - 所属验收单主键
    /// * `specs` - 与输入顺序一致的行规格（含 Service 注入的行 ID）
    ///
    /// # 返回
    /// 返回与输入顺序一致的行实体集合（凭证统一为 `None`）。
    ///
    /// # 错误
    /// 空输入、超过 200 行或行级数量/说明违规时返回错误。
    ///
    /// # 约束
    /// 无 I/O、无时钟、无 ID 生成；空行在此失败，调用方不得先建表头。
    pub fn build(
        acceptance_id: CustomerAcceptanceId,
        specs: Vec<CustomerAcceptanceLineSpec>,
    ) -> Result<Vec<CustomerAcceptanceLine>> {
        if specs.is_empty() {
            return Err(Error::from("客户验收单至少需要一行"));
        }
        if specs.len() > ACCEPTANCE_LINE_BATCH_MAX {
            return Err(Error::from("验收行数必须在1-200之间"));
        }
        let mut lines = Vec::with_capacity(specs.len());
        for (index, spec) in specs.into_iter().enumerate() {
            let line_no = u32::try_from(index + 1).map_err(|_| Error::from("验收行号溢出"))?;
            lines.push(CustomerAcceptanceLine::new(
                spec.line_id,
                CustomerAcceptanceLineData {
                    customer_acceptance_id: acceptance_id.clone(),
                    line_no,
                    sales_order_line_id: spec.sales_order_line_id,
                    accepted_quantity: spec.accepted_quantity,
                    short_quantity: spec.short_quantity,
                    rejected_quantity: spec.rejected_quantity,
                    reason: spec.reason,
                    evidence_attachment_id: None,
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

    fn spec(id: &str) -> CustomerAcceptanceLineSpec {
        CustomerAcceptanceLineSpec {
            line_id: CustomerAcceptanceLineId::new(id),
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            accepted_quantity: Quantity::from_str("9").unwrap(),
            short_quantity: Quantity::from_str("1").unwrap(),
            rejected_quantity: Quantity::from_str("0").unwrap(),
            reason: Some("短少".to_string()),
        }
    }

    /// 正常路径：输入顺序与行号一一对应，凭证默认为空。
    #[test]
    fn build_preserves_order_and_defaults_no_evidence() {
        let lines = CustomerAcceptanceLineBatch::build(
            CustomerAcceptanceId::new("a-1"),
            vec![spec("l-1"), spec("l-2")],
        )
        .unwrap();
        assert_eq!(lines[0].line_no, 1);
        assert_eq!(lines[1].line_no, 2);
        assert!(lines[0].evidence_attachment_id.is_none());
    }

    /// 数量约束由实体直接覆盖：负数量失败。
    #[test]
    fn quantity_violations_fail() {
        assert!(CustomerAcceptanceLineBatch::build(
            CustomerAcceptanceId::new("a-1"),
            vec![CustomerAcceptanceLineSpec {
                accepted_quantity: Quantity::from_str("-1").unwrap(),
                ..spec("l-1")
            }],
        )
        .is_err());
    }

    /// 原因超长失败。
    #[test]
    fn overlong_reason_fails() {
        assert!(CustomerAcceptanceLineBatch::build(
            CustomerAcceptanceId::new("a-1"),
            vec![CustomerAcceptanceLineSpec {
                reason: Some("x".repeat(513)),
                ..spec("l-1")
            }],
        )
        .is_err());
    }

    /// 空行失败（调用方不得先建表头）；超 200 行失败。
    #[test]
    fn empty_and_oversized_batches_fail() {
        assert!(CustomerAcceptanceLineBatch::build(CustomerAcceptanceId::new("a-1"), vec![]).is_err());
        let many = (0..201).map(|i| spec(&format!("l-{i}"))).collect::<Vec<_>>();
        assert!(CustomerAcceptanceLineBatch::build(CustomerAcceptanceId::new("a-1"), many).is_err());
    }
}
