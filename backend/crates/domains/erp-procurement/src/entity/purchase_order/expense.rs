//! 采购费用行：稳定身份、类别、说明与归属，以及确认发生事实（§7.5.1）。
//!
//! 费用行不带商品、数量和单价，不计入销售明细供给覆盖。费用行身份在首次创建时
//! 生成，随草稿冻结、正式版本与采购变更原样延续，确认发生按该身份记录且只能一次。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    FileAssetId, PurchaseOrderId, PurchaseOrderRevisionId, SalesOrderId, SalesOrderLineId, SupplierAccountId,
};
use erp_core::money::{Amount, Rate};
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::InquiryExpenseSource;
use super::types::status_display;

/// 费用说明最大长度。
pub const EXPENSE_DESCRIPTION_MAX_LEN: usize = 200;
/// 费用行身份最大长度。
const EXPENSE_LINE_ID_MAX_LEN: usize = 128;

/// 采购费用类别；稳定代码与成本类型一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurchaseExpenseCategory {
    /// 物流。
    Logistics,
    /// 印刷。
    Printing,
    /// 仓储。
    Storage,
    /// 配送。
    Delivery,
    /// 平台技术。
    PlatformTech,
    /// 线下服务。
    OfflineService,
    /// 其他。
    Other,
}

status_display!(PurchaseExpenseCategory, {
    Logistics => ("物流", "logistics"),
    Printing => ("印刷", "printing"),
    Storage => ("仓储", "storage"),
    Delivery => ("配送", "delivery"),
    PlatformTech => ("平台技术", "platform_tech"),
    OfflineService => ("线下服务", "offline_service"),
    Other => ("其他", "other"),
});

/// 费用行的业务内容；只出现在费用行上。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseExpenseDetail {
    /// 稳定费用行身份，跨草稿、版本与变更不变。
    pub expense_line_id: String,
    /// 费用类别。
    pub category: PurchaseExpenseCategory,
    /// 费用说明。
    pub description: String,
    /// 归属销售明细；空表示整张销售单。
    #[serde(default)]
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 经服务端确认的询价费用来源；历史与直接费用采购为空。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inquiry_source: Option<InquiryExpenseSource>,
}

impl PurchaseExpenseDetail {
    /// 构造并规范化费用内容。
    ///
    /// # 参数
    /// * `expense_line_id` - 稳定费用行身份
    /// * `category` - 费用类别
    /// * `description` - 费用说明
    /// * `sales_order_line_id` - 归属销售明细
    ///
    /// # 返回
    /// 返回说明已去除首尾空白的费用内容。
    ///
    /// # 错误
    /// 身份或说明为空、超长时返回错误。
    pub fn new(
        expense_line_id: String,
        category: PurchaseExpenseCategory,
        description: String,
        sales_order_line_id: Option<SalesOrderLineId>,
    ) -> Result<Self> {
        let detail =
            Self { expense_line_id, category, description, sales_order_line_id, inquiry_source: None };
        detail.normalized()
    }

    /// 校验并返回规范化的费用内容。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回规范化后的副本。
    ///
    /// # 错误
    /// 身份或说明为空、超长时返回错误。
    pub fn normalized(&self) -> Result<Self> {
        Ok(Self {
            expense_line_id: normalize_required_text(
                self.expense_line_id.clone(),
                "费用行身份不能为空",
                EXPENSE_LINE_ID_MAX_LEN,
                "费用行身份过长",
            )?,
            category: self.category,
            description: normalize_required_text(
                self.description.clone(),
                "费用说明不能为空",
                EXPENSE_DESCRIPTION_MAX_LEN,
                "费用说明不能超过 200 字",
            )?,
            sales_order_line_id: self.sales_order_line_id.clone(),
            inquiry_source: self.inquiry_source.as_ref().map(InquiryExpenseSource::normalized).transpose()?,
        })
    }
}

/// 费用行确认发生事实创建数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PurchaseExpenseOccurrenceData {
    /// 所属采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 确认时的采购生效版本。
    pub purchase_order_revision_id: PurchaseOrderRevisionId,
    /// 来源销售单。
    pub sales_order_id: SalesOrderId,
    /// 收款供应商。
    pub supplier_id: SupplierAccountId,
    /// 费用内容。
    pub expense: PurchaseExpenseDetail,
    /// 确认时的含税金额。
    pub gross_amount: Amount,
    /// 确认时的不含税金额。
    pub net_amount: Amount,
    /// 确认时的税额。
    pub tax_amount: Amount,
    /// 确认时的进项税率。
    pub input_tax_rate: Rate,
    /// 费用发生日期。
    pub occurred_on: BusinessDate,
    /// 凭证文件。
    pub evidence_attachment_id: FileAssetId,
    /// 确认人。
    pub confirmed_by: String,
}

/// 费用行确认发生事实；每个费用行身份只能记录一次，记录后不可修改。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct PurchaseExpenseOccurrence {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属采购单。
    pub purchase_order_id: PurchaseOrderId,
    /// 确认时的采购生效版本。
    pub purchase_order_revision_id: PurchaseOrderRevisionId,
    /// 来源销售单。
    pub sales_order_id: SalesOrderId,
    /// 收款供应商。
    pub supplier_id: SupplierAccountId,
    /// 稳定费用行身份（与 `purchase_order_id` 唯一）。
    pub expense_line_id: String,
    /// 费用内容快照。
    pub expense: PurchaseExpenseDetail,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 费用发生日期。
    pub occurred_on: BusinessDate,
    /// 凭证文件。
    pub evidence_attachment_id: FileAssetId,
    /// 确认人。
    pub confirmed_by: String,
}

impl PurchaseExpenseOccurrence {
    /// 记录费用行确认发生。
    ///
    /// # 参数
    /// * `id` - 事实主键
    /// * `data` - 确认内容
    /// * `today` - 业务当日
    ///
    /// # 返回
    /// 返回确认发生事实。
    ///
    /// # 错误
    /// 金额不为正、金额不守恒、发生日期晚于当日或费用内容非法时返回错误。
    pub fn new(id: String, data: PurchaseExpenseOccurrenceData, today: BusinessDate) -> Result<Self> {
        if data.gross_amount.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("金额为 0 的费用行不能确认发生，请先移除"));
        }
        if data.gross_amount != data.net_amount.checked_add(data.tax_amount) {
            return Err(Error::from("含税金额必须等于不含税金额加税额"));
        }
        if data.occurred_on > today {
            return Err(Error::from("发生日期不能晚于今天"));
        }
        if data.confirmed_by.trim().is_empty() {
            return Err(Error::from("确认人不能为空"));
        }
        let expense = data.expense.normalized()?;
        Ok(Self {
            base: BaseModel::new(id),
            purchase_order_id: data.purchase_order_id,
            purchase_order_revision_id: data.purchase_order_revision_id,
            sales_order_id: data.sales_order_id,
            supplier_id: data.supplier_id,
            expense_line_id: expense.expense_line_id.clone(),
            expense,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
            input_tax_rate: data.input_tax_rate,
            occurred_on: data.occurred_on,
            evidence_attachment_id: data.evidence_attachment_id,
            confirmed_by: data.confirmed_by,
        })
    }
}

/// 一条费用行在改单比对中的业务内容。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ExpenseLineState<'a> {
    /// 费用内容；历史费用行可为空。
    pub expense: Option<&'a PurchaseExpenseDetail>,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 进项税率。
    pub input_tax_rate: Option<Rate>,
}

impl ExpenseLineState<'_> {
    /// 改单是否改变了本行的金额、税率、说明或归属。
    fn differs_from(&self, other: &Self) -> bool {
        let (Some(left), Some(right)) = (self.expense, other.expense) else {
            return true;
        };
        self.gross_amount != other.gross_amount
            || self.input_tax_rate != other.input_tax_rate
            || left.description != right.description
            || left.sales_order_line_id != right.sales_order_line_id
            || left.inquiry_source != right.inquiry_source
    }
}

/// 校验改单目标中的费用行：不能新增、不能遗漏、类别不变，已确认发生的行不可修改。
///
/// # 参数
/// * `base` - 当前生效版本的费用行
/// * `requested` - 改单目标中的费用行
/// * `occurred` - 已确认发生的费用行身份
///
/// # 返回
/// 费用行合规时返回 `Ok(())`。历史费用行（无费用内容）不参与比对。
///
/// # 错误
/// 新增或重复费用行、遗漏原费用行、改变类别或修改已确认发生的费用行时返回错误。
pub fn validate_change_expenses(
    base: &[ExpenseLineState<'_>],
    requested: &[ExpenseLineState<'_>],
    occurred: &std::collections::HashSet<String>,
) -> Result<()> {
    let by_id = base
        .iter()
        .filter_map(|line| Some((line.expense?.expense_line_id.as_str(), line)))
        .collect::<std::collections::HashMap<_, _>>();
    let mut seen = std::collections::HashSet::new();
    for line in requested {
        let Some(expense) = line.expense else { continue };
        let id = expense.expense_line_id.as_str();
        let original = by_id.get(id).ok_or("采购改单不能新增费用行；新的费用请另建费用采购单")?;
        if !seen.insert(id) {
            return Err(Error::from("同一费用行在改单中重复出现，请刷新后重试"));
        }
        if original.expense.map(|value| value.category) != Some(expense.category) {
            return Err(Error::from("费用类别不能修改；需要其他类别请移除本行后另建费用采购单"));
        }
        let original_source = original.expense.and_then(|value| value.inquiry_source.as_ref());
        if !same_inquiry_source(original_source, expense.inquiry_source.as_ref()) {
            return Err(Error::from("采购改单不能更换询价费用来源"));
        }
        if occurred.contains(id) && line.differs_from(original) {
            return Err(Error::from("已确认发生的费用行不能修改"));
        }
    }
    if seen.len() != by_id.len() {
        return Err(Error::from("原费用行须保留记录；不再需要的费用请把金额改为 0"));
    }
    Ok(())
}

fn same_inquiry_source(left: Option<&InquiryExpenseSource>, right: Option<&InquiryExpenseSource>) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => left.same_identity(right),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn detail() -> PurchaseExpenseDetail {
        PurchaseExpenseDetail::new(
            " exp-1 ".into(),
            PurchaseExpenseCategory::Logistics,
            "  第三方物流  ".into(),
            None,
        )
        .unwrap()
    }

    fn data(gross: &str, net: &str, tax: &str) -> PurchaseExpenseOccurrenceData {
        PurchaseExpenseOccurrenceData {
            purchase_order_id: PurchaseOrderId::new("po-1"),
            purchase_order_revision_id: PurchaseOrderRevisionId::new("por-1"),
            sales_order_id: SalesOrderId::new("so-1"),
            supplier_id: SupplierAccountId::new("sup-1"),
            expense: detail(),
            gross_amount: Amount::from_str(gross).unwrap(),
            net_amount: Amount::from_str(net).unwrap(),
            tax_amount: Amount::from_str(tax).unwrap(),
            input_tax_rate: Rate::from_str("0.06").unwrap(),
            occurred_on: BusinessDate::from_ymd(2026, 10, 13).unwrap(),
            evidence_attachment_id: FileAssetId::new("file-1"),
            confirmed_by: "u-1".into(),
        }
    }

    #[test]
    fn detail_normalizes_identity_and_description() {
        let value = detail();
        assert_eq!(value.expense_line_id, "exp-1");
        assert_eq!(value.description, "第三方物流");
        assert_eq!(PurchaseExpenseCategory::PlatformTech.as_str(), "platform_tech");
        assert_eq!(PurchaseExpenseCategory::Other.label(), "其他");
        let blank = PurchaseExpenseDetail::new("e".into(), PurchaseExpenseCategory::Other, " ".into(), None);
        assert!(blank.is_err());
        let long =
            PurchaseExpenseDetail::new("e".into(), PurchaseExpenseCategory::Other, "费".repeat(201), None);
        assert!(long.is_err());
    }

    fn state<'a>(expense: &'a PurchaseExpenseDetail, gross: &str) -> ExpenseLineState<'a> {
        ExpenseLineState {
            expense: Some(expense),
            gross_amount: Amount::from_str(gross).unwrap(),
            input_tax_rate: Some(Rate::from_str("0.06").unwrap()),
        }
    }

    #[test]
    fn change_keeps_every_fee_line_and_freezes_occurred_lines() {
        let first = detail();
        let second =
            PurchaseExpenseDetail::new("exp-2".into(), PurchaseExpenseCategory::Other, "包装".into(), None)
                .unwrap();
        let base = [state(&first, "100.00"), state(&second, "20.00")];
        let none = std::collections::HashSet::new();
        assert!(
            validate_change_expenses(&base, &[state(&first, "80.00"), state(&second, "0.00")], &none).is_ok()
        );
        assert!(validate_change_expenses(&base, &[state(&first, "80.00")], &none).is_err());
        let extra =
            PurchaseExpenseDetail::new("exp-3".into(), PurchaseExpenseCategory::Other, "新增".into(), None)
                .unwrap();
        let added = [state(&first, "100.00"), state(&second, "20.00"), state(&extra, "1.00")];
        assert!(validate_change_expenses(&base, &added, &none).is_err());
        let duplicated = [state(&first, "100.00"), state(&first, "100.00")];
        assert!(validate_change_expenses(&base, &duplicated, &none).is_err());
        let recategorized =
            PurchaseExpenseDetail { category: PurchaseExpenseCategory::Storage, ..first.clone() };
        assert!(
            validate_change_expenses(
                &base,
                &[state(&recategorized, "100.00"), state(&second, "20.00")],
                &none
            )
            .is_err()
        );
        let occurred = std::collections::HashSet::from(["exp-1".to_string()]);
        assert!(
            validate_change_expenses(&base, &[state(&first, "100.00"), state(&second, "0.00")], &occurred)
                .is_ok()
        );
        assert!(
            validate_change_expenses(&base, &[state(&first, "90.00"), state(&second, "20.00")], &occurred)
                .is_err()
        );
        let renamed = PurchaseExpenseDetail { description: "改名".into(), ..first.clone() };
        assert!(
            validate_change_expenses(&base, &[state(&renamed, "100.00"), state(&second, "20.00")], &occurred)
                .is_err()
        );
    }

    #[test]
    fn occurrence_requires_positive_balanced_amount_and_past_date() {
        let today = BusinessDate::from_ymd(2026, 10, 14).unwrap();
        let ok =
            PurchaseExpenseOccurrence::new("occ-1".into(), data("106.00", "99.64", "6.36"), today).unwrap();
        assert_eq!(ok.expense_line_id, "exp-1");
        assert!(PurchaseExpenseOccurrence::new("o".into(), data("0.00", "0.00", "0.00"), today).is_err());
        assert!(PurchaseExpenseOccurrence::new("o".into(), data("106.00", "99.00", "6.36"), today).is_err());
        let future = BusinessDate::from_ymd(2026, 10, 13).unwrap();
        let early_today = BusinessDate::from_ymd(2026, 10, 12).unwrap();
        assert!(future > early_today);
        assert!(
            PurchaseExpenseOccurrence::new("o".into(), data("106.00", "99.64", "6.36"), early_today).is_err()
        );
    }

    #[test]
    fn change_can_explicitly_adopt_a_new_fee_round_but_keeps_stable_identity_and_occurred_history() {
        let original = PurchaseExpenseDetail {
            inquiry_source: Some(InquiryExpenseSource {
                inquiry_id: "inquiry".into(),
                feedback_round: 1,
                fee_id: "fee".into(),
                allocation_id: "allocation".into(),
            }),
            ..detail()
        };
        let mut revised = original.clone();
        revised.inquiry_source.as_mut().unwrap().feedback_round = 2;
        let base = [state(&original, "100.00")];
        assert!(validate_change_expenses(&base, &[state(&revised, "110.00")], &Default::default()).is_ok());
        let occurred = std::collections::HashSet::from([original.expense_line_id.clone()]);
        assert!(validate_change_expenses(&base, &[state(&revised, "100.00")], &occurred).is_err());
        revised.inquiry_source.as_mut().unwrap().allocation_id = "other".into();
        assert!(validate_change_expenses(&base, &[state(&revised, "100.00")], &Default::default()).is_err());
        revised.inquiry_source = None;
        assert!(validate_change_expenses(&base, &[state(&revised, "100.00")], &Default::default()).is_err());
    }
}
