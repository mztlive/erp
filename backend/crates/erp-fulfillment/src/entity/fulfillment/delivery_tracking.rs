//! 包裹物流与销售明细的关联、规范化和实际发货行资格。

use erp_core::ids::{SalesOrderId, SalesOrderLineId};
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::DeliveryLine;

/// 一次发货最多登记的包裹明细关联数量。
pub const TRACKING_ENTRIES_MAX: usize = 100;
/// 每个物流单号的字符数上限。
pub const TRACKING_NO_MAX_LEN: usize = 128;
/// 每个包裹承运商的字符数上限。
pub const TRACKING_CARRIER_MAX_LEN: usize = 64;
/// 销售稳定明细身份的字符数上限。
const SALES_LINE_ID_MAX_LEN: usize = 128;

/// 一个包裹与一条销售明细的关联；同包裹可分别关联多条明细。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DeliveryTrackingEntry {
    /// 本次发货实际包含的销售稳定明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 包裹物流单号。
    pub tracking_no: String,
    /// 该包裹承运商；缺省为空，不从表头推断。
    pub carrier: Option<String>,
}

impl DeliveryTrackingEntry {
    /// 校验物流关联明细的真实销售单归属。
    ///
    /// # 参数
    /// * `expected` - 本次发货表头的销售单。
    /// * `actual` - 同执行器读取的真实销售明细归属。
    /// # 返回
    /// 明细属于本销售单时成功。
    /// # 错误
    /// 明细属于其他销售单时拒绝。
    pub fn ensure_sales_order(&self, expected: &SalesOrderId, actual: &SalesOrderId) -> Result<()> {
        if expected != actual {
            return Err(Error::from("包裹关联的销售明细不属于本销售单"));
        }
        Ok(())
    }

    /// 规范化包裹明细关联中的身份、物流单号和可选承运商。
    ///
    /// # 参数
    /// 无；消费当前原始关联。
    /// # 返回
    /// 返回规范化且通过字符长度校验的关联。
    /// # 错误
    /// 明细身份或物流号为空、任一字段超长时拒绝。
    pub fn normalized(self) -> Result<Self> {
        let sales_order_line_id = normalize_required_text(
            self.sales_order_line_id.to_string(),
            "销售明细不能为空",
            SALES_LINE_ID_MAX_LEN,
            "销售明细标识过长",
        )?;
        let tracking_no = normalize_required_text(
            self.tracking_no,
            "物流单号不能为空",
            TRACKING_NO_MAX_LEN,
            "物流单号过长",
        )?;
        let carrier = normalize_optional_text(self.carrier, "物流承运方", TRACKING_CARRIER_MAX_LEN)?;
        Ok(Self { sales_order_line_id: SalesOrderLineId::new(sales_order_line_id), tracking_no, carrier })
    }
}

/// 已规范化、按关联元组去重并保留输入顺序的包裹集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryTrackingEntries(Vec<DeliveryTrackingEntry>);

impl DeliveryTrackingEntries {
    /// 校验和规范化包裹关联；相同编号关联不同明细时均保留。
    ///
    /// # 参数
    /// * `entries` - 原始包裹明细关联。
    /// # 返回
    /// 返回按明细、物流号、承运商三元组去重的完整集合。
    /// # 错误
    /// 超过100条、空身份/单号或字段超长时拒绝。
    pub fn new(entries: Vec<DeliveryTrackingEntry>) -> Result<Self> {
        if entries.len() > TRACKING_ENTRIES_MAX {
            return Err(Error::from("包裹明细关联最多允许100条"));
        }
        let mut normalized = Vec::with_capacity(entries.len());
        for entry in entries {
            let entry = entry.normalized()?;
            if !normalized.contains(&entry) {
                normalized.push(entry);
            }
        }
        Ok(Self(normalized))
    }

    /// 转移完整关联集合供实体持久化。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回全部规范化关联；空集合表示清空包裹。
    /// # 错误
    /// 无。
    pub fn into_entries(self) -> Vec<DeliveryTrackingEntry> {
        self.0
    }

    /// 验证所有包裹只能绑定本发货单实际包含的销售明细。
    ///
    /// # 参数
    /// * `entries` - 当前完整包裹关联。
    /// * `delivery_id` - 本次发货单稳定身份。
    /// * `lines` - 从同执行器读取或创建的真实发货行。
    /// # 返回
    /// 所有关联均命中本发货行时成功。
    /// # 错误
    /// 未包含的销售明细或误用另一发货单明细时拒绝。
    pub fn ensure_lines(
        entries: &[DeliveryTrackingEntry],
        delivery_id: &str,
        lines: &[DeliveryLine],
    ) -> Result<()> {
        for entry in entries {
            if !lines.iter().any(|line| {
                line.delivery_id.as_ref() == delivery_id
                    && line.sales_order_line_id == entry.sales_order_line_id
            }) {
                return Err(Error::from("包裹关联的销售明细不属于本次发货"));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{DeliveryId, DeliveryLineId, StockReservationId};
    use erp_core::money::Quantity;

    use super::*;
    use crate::entity::fulfillment::{DeliveryLineData, DeliveryType};

    /// 构造一个原始关联。
    fn entry(line: &str, number: &str, carrier: Option<&str>) -> DeliveryTrackingEntry {
        DeliveryTrackingEntry {
            sales_order_line_id: SalesOrderLineId::new(line),
            tracking_no: number.into(),
            carrier: carrier.map(str::to_owned),
        }
    }

    /// 构造实际发货行，不能用客户端传入的物流明细身份替代。
    fn delivery_line(delivery: &str, sale_line: &str) -> DeliveryLine {
        DeliveryLine::new(
            DeliveryLineId::new(format!("{delivery}-{sale_line}")),
            DeliveryLineData {
                delivery_id: DeliveryId::new(delivery),
                line_no: 1,
                sales_order_line_id: SalesOrderLineId::new(sale_line),
                quantity: Quantity::from_str("1").unwrap(),
                stock_reservation_id: Some(StockReservationId::new("reservation")),
                purchase_line_sales_allocation_id: None,
            },
            DeliveryType::WarehouseShip,
        )
        .unwrap()
    }

    /// 同销售明细可有多个包裹；未知明细与其他发货单的明细均拒绝。
    #[test]
    fn validates_tracking_against_actual_delivery_lines() {
        let valid = [entry("line-1", "A", None), entry("line-1", "B", Some("货拉拉"))];
        let lines = [delivery_line("delivery-1", "line-1"), delivery_line("delivery-1", "line-2")];
        assert!(DeliveryTrackingEntries::ensure_lines(&valid, "delivery-1", &lines).is_ok());
        assert!(
            DeliveryTrackingEntries::ensure_lines(&[entry("missing-line", "A", None)], "delivery-1", &lines)
                .is_err()
        );
        assert!(DeliveryTrackingEntries::ensure_lines(&valid, "other-delivery", &lines).is_err());
        assert!(DeliveryTrackingEntries::ensure_lines(&valid, "delivery-1", &[]).is_err());
        assert!(DeliveryTrackingEntries::ensure_lines(&[], "delivery-1", &[]).is_ok());
    }

    /// 即使错误销售明细出现在发货集合，也须依据真实销售归属拒绝。
    #[test]
    fn rejects_sales_line_from_another_sales_order() {
        let entry = entry("line-1", "A", None);
        assert!(entry.ensure_sales_order(&SalesOrderId::new("sale-1"), &SalesOrderId::new("sale-1")).is_ok());
        assert!(
            entry.ensure_sales_order(&SalesOrderId::new("sale-1"), &SalesOrderId::new("sale-2")).is_err()
        );
    }

    /// 相同包裹关联多个销售明细不能因全局编号去重丢失映射。
    #[test]
    fn deduplicates_association_tuple_without_losing_shared_package_lines() {
        let entries = DeliveryTrackingEntries::new(vec![
            entry(" line-1 ", " SF-1 ", Some(" 顺丰 ")),
            entry("line-1", "SF-2", Some("货拉拉")),
            entry("line-2", "SF-1", Some("顺丰")),
            entry("line-1", "SF-1", Some("顺丰")),
            entry("line-1", "SF-1", Some("京东")),
        ])
        .unwrap()
        .into_entries();
        assert_eq!(entries.len(), 4);
        assert_eq!(entries[0], entry("line-1", "SF-1", Some("顺丰")));
        assert_eq!(entries[1].carrier.as_deref(), Some("货拉拉"));
        assert_eq!(entries[2].sales_order_line_id.as_ref(), "line-2");
        assert_eq!(entries[3].carrier.as_deref(), Some("京东"));
    }

    /// 空白身份和单号失败；可选承运商为空白时明确为空。
    #[test]
    fn rejects_empty_required_fields_and_normalizes_optional_carrier() {
        assert!(DeliveryTrackingEntries::new(vec![entry(" ", "A", None)]).is_err());
        assert!(DeliveryTrackingEntries::new(vec![entry("line", " ", None)]).is_err());
        let normalized =
            DeliveryTrackingEntries::new(vec![entry("line", "A", Some(" "))]).unwrap().into_entries();
        assert_eq!(normalized[0].carrier, None);
        assert!(DeliveryTrackingEntries::new(Vec::new()).unwrap().into_entries().is_empty());
    }

    /// 字符长度与原始关联条数严格限制，不静默截断或先去重规避条数。
    #[test]
    fn enforces_field_lengths_and_input_count() {
        assert!(entry("line", &"货".repeat(128), Some(&"货".repeat(64))).normalized().is_ok());
        assert!(entry("line", &"货".repeat(129), None).normalized().is_err());
        assert!(entry("line", "A", Some(&"货".repeat(65))).normalized().is_err());
        assert!(entry(&"x".repeat(129), "A", None).normalized().is_err());
        assert!(DeliveryTrackingEntries::new(vec![entry("line", "A", None); 100]).is_ok());
        assert!(DeliveryTrackingEntries::new(vec![entry("line", "A", None); 101]).is_err());
    }

    /// 关联的未知字段失败关闭。
    #[test]
    fn rejects_unknown_tracking_entry_fields() {
        assert!(
            serde_json::from_value::<DeliveryTrackingEntry>(serde_json::json!({
                "sales_order_line_id":"line", "tracking_no":"A", "carrier_code":"SF",
            }))
            .is_err()
        );
    }
}
