//! 销售单附加费用请求与视图合同（§7.5.1）。

use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{FileAssetId, SalesOrderLineId, SupplierAccountId};
use erp_core::money::{Amount, Rate};
use serde::{Deserialize, Serialize};
use validator::Validate;

use super::inquiry_fee::InquiryFeeReference;
use crate::entity::cost::{
    CostAllocation, CostEntry, ExtraCostCategory, ExtraCostInput, ExtraCostReductionInput, ReducedAmounts,
    is_registered_extra_cost,
};

/// 财务登记附加费用请求；凭证与命令同一 multipart 提交。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct RegisterExtraCostRequest {
    /// 费用类别。
    pub category: ExtraCostCategory,
    /// 费用说明。
    #[validate(length(min = 1, max = 200, message = "费用说明须为 1–200 字"))]
    pub description: String,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 发生日期。
    pub occurred_on: BusinessDate,
    /// 收款供应商（可空）。
    #[serde(default)]
    pub supplier_id: Option<SupplierAccountId>,
    /// 归属销售明细（可空，空表示整张销售单）。
    #[serde(default)]
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// ERP 外办理的询价费用；登记与来源额度在同一事务消费。
    #[serde(default)]
    pub inquiry_source: Option<InquiryFeeReference>,
    /// 凭证文件；本次上传时为 `pending-file:` 临时引用。
    pub evidence_attachment_id: FileAssetId,
    /// 幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

impl RegisterExtraCostRequest {
    /// 转换为领域登记输入。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回未校验的领域输入，由 [`crate::entity::cost::ExtraCostRegistration`] 校验。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn input(&self) -> ExtraCostInput {
        ExtraCostInput {
            category: self.category,
            description: self.description.clone(),
            gross_amount: self.gross_amount,
            input_tax_rate: self.input_tax_rate,
            occurred_on: self.occurred_on,
            supplier_id: self.supplier_id.clone(),
            sales_order_line_id: self.sales_order_line_id.clone(),
            evidence_attachment_id: self.evidence_attachment_id.clone(),
        }
    }
}

/// 冲减附加费用请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
#[serde(deny_unknown_fields)]
pub struct ReduceExtraCostRequest {
    /// 冲减原因。
    #[validate(length(min = 1, max = 200, message = "冲减原因须为 1–200 字"))]
    pub description: String,
    /// 冲减含税金额。
    pub gross_amount: Amount,
    /// 冲减日期。
    pub occurred_on: BusinessDate,
    /// 凭证文件；本次上传时为 `pending-file:` 临时引用。
    pub evidence_attachment_id: FileAssetId,
    /// 幂等键。
    #[validate(length(min = 1, max = 128, message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

impl ReduceExtraCostRequest {
    /// 转换为领域冲减输入。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回未校验的领域输入。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn input(&self) -> ExtraCostReductionInput {
        ExtraCostReductionInput {
            description: self.description.clone(),
            gross_amount: self.gross_amount,
            occurred_on: self.occurred_on,
            evidence_attachment_id: self.evidence_attachment_id.clone(),
        }
    }
}

/// 附加费用登记或冲减的命令结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtraCostCommandResult {
    /// 本次写入（或重放命中）的成本事实。
    pub cost_entry_id: String,
    /// 是否为同一幂等键的重放。
    pub replayed: bool,
}

/// 附加费用来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtraCostSource {
    /// 财务凭证登记。
    Registration,
    /// 采购单费用行。
    PurchaseExpense,
}

/// 附加费用发生状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtraCostStatus {
    /// 采购费用行已生效、尚未确认发生。
    Pending,
    /// 已发生，计入实际经营盈亏。
    Occurred,
}

/// 附加费用冲减记录。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExtraCostReductionView {
    /// 冲减成本事实。
    pub cost_entry_id: String,
    /// 冲减原因。
    pub description: Option<String>,
    /// 冲减含税金额。
    pub gross_amount: Amount,
    /// 冲减日期。
    pub occurred_at: Instant,
    /// 是否附凭证。
    pub evidence_attachment_id: Option<String>,
}

/// 销售单上的一笔附加费用。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SalesExtraCostView {
    /// 来源。
    pub source: ExtraCostSource,
    /// 稳定标识：财务登记为成本事实主键，采购费用为费用行身份。
    pub id: String,
    /// 费用类别。
    pub category: ExtraCostCategory,
    /// 费用说明。
    pub description: String,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税率。
    pub input_tax_rate: Rate,
    /// 发生状态。
    pub status: ExtraCostStatus,
    /// 发生时间；待发生时为空。
    pub occurred_at: Option<Instant>,
    /// 归属销售明细；空表示整张销售单。
    pub sales_order_line_id: Option<String>,
    /// 收款供应商。
    pub supplier_id: Option<String>,
    /// 来源采购单（采购费用）。
    pub purchase_order_id: Option<String>,
    /// 来源采购单号（采购费用）。
    pub purchase_no: Option<String>,
    /// 凭证文件。
    pub evidence_attachment_id: Option<String>,
    /// 已冲减含税金额。
    pub reduced_gross_amount: Amount,
    /// 冲减后剩余含税金额。
    pub remaining_gross_amount: Amount,
    /// 冲减记录。
    pub reductions: Vec<ExtraCostReductionView>,
}

/// 销售单附加费用列表。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SalesExtraCostListView {
    /// 逐笔费用，按发生时间排序，待发生在后。
    pub items: Vec<SalesExtraCostView>,
    /// 待发生含税合计。
    pub pending_gross_total: Amount,
    /// 已发生含税合计（扣除冲减）。
    pub occurred_gross_total: Amount,
}

impl SalesExtraCostView {
    /// 由财务登记的附加费用及其冲减生成视图。
    ///
    /// # 参数
    /// * `entries` - 归属同一销售单的成本事实；非财务登记费用被忽略
    /// * `allocations` - 这些成本事实的分配
    ///
    /// # 返回
    /// 返回每笔财务登记费用一条视图，冲减挂在原费用下。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn registrations(entries: &[CostEntry], allocations: &[CostAllocation]) -> Vec<Self> {
        entries
            .iter()
            .filter(|entry| is_registered_extra_cost(entry))
            .filter_map(|entry| {
                let category = ExtraCostCategory::from_cost_type(entry.cost_type)?;
                let reduced = ReducedAmounts::of(entry, entries.iter());
                let line = allocations
                    .iter()
                    .find(|allocation| allocation.cost_entry_id.as_ref() == entry.base.id)
                    .and_then(|allocation| allocation.sales_order_line_id.as_ref())
                    .map(ToString::to_string);
                Some(Self {
                    source: ExtraCostSource::Registration,
                    id: entry.base.id.clone(),
                    category,
                    description: entry.description.clone().unwrap_or_default(),
                    gross_amount: entry.gross_amount,
                    net_amount: entry.net_amount,
                    input_tax_rate: entry.input_tax_rate,
                    status: ExtraCostStatus::Occurred,
                    occurred_at: Some(entry.occurred_at),
                    sales_order_line_id: line,
                    supplier_id: entry.supplier_id.as_ref().map(ToString::to_string),
                    purchase_order_id: None,
                    purchase_no: None,
                    evidence_attachment_id: entry.evidence_attachment_id.as_ref().map(ToString::to_string),
                    reduced_gross_amount: reduced.gross,
                    remaining_gross_amount: entry.gross_amount.checked_sub(reduced.gross),
                    reductions: reductions_of(entry, entries),
                })
            })
            .collect()
    }
}

fn reductions_of(original: &CostEntry, entries: &[CostEntry]) -> Vec<ExtraCostReductionView> {
    let mut reductions = entries
        .iter()
        .filter(|entry| {
            entry.adjusts_cost_entry_id.as_ref().is_some_and(|id| id.as_ref() == original.base.id)
        })
        .map(|entry| ExtraCostReductionView {
            cost_entry_id: entry.base.id.clone(),
            description: entry.description.clone(),
            gross_amount: entry.gross_amount,
            occurred_at: entry.occurred_at,
            evidence_attachment_id: entry.evidence_attachment_id.as_ref().map(ToString::to_string),
        })
        .collect::<Vec<_>>();
    reductions.sort_by(|left, right| {
        (left.occurred_at.unix_secs(), &left.cost_entry_id)
            .cmp(&(right.occurred_at.unix_secs(), &right.cost_entry_id))
    });
    reductions
}

impl SalesExtraCostListView {
    /// 排序并汇总附加费用。
    ///
    /// # 参数
    /// * `items` - 全部附加费用
    ///
    /// # 返回
    /// 已发生在前按发生时间升序，待发生在后；返回待发生合计与扣除冲减后的已发生合计。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn from_items(mut items: Vec<SalesExtraCostView>) -> Self {
        items.sort_by(|left, right| {
            let key = |item: &SalesExtraCostView| {
                (
                    item.status == ExtraCostStatus::Pending,
                    item.occurred_at.map(Instant::unix_secs),
                    item.id.clone(),
                )
            };
            key(left).cmp(&key(right))
        });
        let mut pending = Amount::zero();
        let mut occurred = Amount::zero();
        for item in &items {
            match item.status {
                ExtraCostStatus::Pending => pending = pending.checked_add(item.gross_amount),
                ExtraCostStatus::Occurred => occurred = occurred.checked_add(item.remaining_gross_amount),
            }
        }
        Self { items, pending_gross_total: pending, occurred_gross_total: occurred }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn register_request_parses_wire_shape_and_rejects_unknown_fields() {
        let body = json!({
            "category": "logistics", "description": "第三方物流", "gross_amount": "106.00",
            "input_tax_rate": "0.06", "occurred_on": "2026-10-13",
            "evidence_attachment_id": "pending-file:evidence", "idempotency_key": "k-1"
        });
        let request: RegisterExtraCostRequest = serde_json::from_value(body.clone()).unwrap();
        assert!(request.validate().is_ok());
        assert_eq!(request.input().category, ExtraCostCategory::Logistics);
        assert!(request.input().sales_order_line_id.is_none());
        let mut unknown = body.clone();
        unknown["cost_stage"] = json!("reduction");
        assert!(serde_json::from_value::<RegisterExtraCostRequest>(unknown).is_err());
        let mut product = body;
        product["category"] = json!("product");
        assert!(serde_json::from_value::<RegisterExtraCostRequest>(product).is_err());
    }

    fn registered_pair(key: &str, gross: &str, day: u32) -> (CostEntry, CostAllocation) {
        use std::str::FromStr;

        use erp_core::ids::{CostEntryId, SalesOrderId};

        use crate::entity::cost::ExtraCostRegistration;
        let input = ExtraCostInput {
            category: ExtraCostCategory::Storage,
            description: "临时仓储".into(),
            gross_amount: Amount::from_str(gross).unwrap(),
            input_tax_rate: Rate::from_str("0").unwrap(),
            occurred_on: BusinessDate::from_ymd(2026, 10, day).unwrap(),
            supplier_id: None,
            sales_order_line_id: None,
            evidence_attachment_id: FileAssetId::new("file-1"),
        };
        ExtraCostRegistration::new(input, BusinessDate::from_ymd(2026, 10, 14).unwrap())
            .unwrap()
            .into_facts(CostEntryId::new(key), SalesOrderId::new("so-1"))
            .unwrap()
    }

    #[test]
    fn registrations_attach_reductions_and_totals_net_of_reductions() {
        use std::str::FromStr;

        use erp_core::ids::CostEntryId;

        use crate::entity::cost::{ExtraCostReduction, ReducedAmounts};
        let (late, late_allocation) = registered_pair("xc-b", "80.00", 12);
        let (early, early_allocation) = registered_pair("xc-a", "20.00", 10);
        let (reduction, reduction_allocation) = ExtraCostReduction::new(
            ExtraCostReductionInput {
                description: "退回".into(),
                gross_amount: Amount::from_str("30.00").unwrap(),
                occurred_on: BusinessDate::from_ymd(2026, 10, 13).unwrap(),
                evidence_attachment_id: FileAssetId::new("file-2"),
            },
            BusinessDate::from_ymd(2026, 10, 14).unwrap(),
        )
        .unwrap()
        .into_facts(
            CostEntryId::new("xc-c"),
            &late,
            &late_allocation,
            ReducedAmounts::of(&late, std::iter::empty()),
        )
        .unwrap();
        let entries = vec![late, reduction, early];
        let allocations = vec![late_allocation, reduction_allocation, early_allocation];

        let list =
            SalesExtraCostListView::from_items(SalesExtraCostView::registrations(&entries, &allocations));

        assert_eq!(list.items.iter().map(|item| item.id.as_str()).collect::<Vec<_>>(), ["xc-a", "xc-b"]);
        let late = &list.items[1];
        assert_eq!(late.reductions.len(), 1);
        assert_eq!(late.remaining_gross_amount, Amount::from_str("50.00").unwrap());
        assert_eq!(list.occurred_gross_total, Amount::from_str("70.00").unwrap());
        assert_eq!(list.pending_gross_total, Amount::zero());
    }

    #[test]
    fn reduce_request_requires_reason_and_key() {
        let request: ReduceExtraCostRequest = serde_json::from_value(json!({
            "description": "", "gross_amount": "1.00", "occurred_on": "2026-10-13",
            "evidence_attachment_id": "pending-file:evidence", "idempotency_key": ""
        }))
        .unwrap();
        assert!(request.validate().is_err());
    }
}
