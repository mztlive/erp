//! `invoice` 发票（销项/进项统一表，数据模型 §6.8、§6.9）。
//!
//! 合同 §4.3 签署为 `NO_APPROVAL`：实体只保留业务状态，不得新增审批绑定字段
//! 或审批状态机。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::ids::{InvoiceId, PartyId};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 发票代码最大长度。
const INVOICE_CODE_MAX_LEN: usize = 32;
/// 发票号码最大长度。
const INVOICE_NO_MAX_LEN: usize = 32;
/// 尾差原因最大长度。
const ROUNDING_REASON_MAX_LEN: usize = 256;

/// 发票方向（数据模型 §6.8：销项或进项）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceDirection {
    /// 销项发票。
    Sales,
    /// 进项发票。
    Purchase,
}

impl InvoiceDirection {
    /// 返回方向的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Sales => "销项",
            Self::Purchase => "进项",
        }
    }

    /// 返回方向的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Sales => "sales",
            Self::Purchase => "purchase",
        }
    }
}

/// 发票蓝红类型（数据模型 §6.8：`BLUE` 或 `RED`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceKind {
    /// 蓝票。
    Blue,
    /// 红票（冲销原蓝票）。
    Red,
}

impl InvoiceKind {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Blue => "蓝票",
            Self::Red => "红票",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Blue => "blue",
            Self::Red => "red",
        }
    }
}

/// 记账方向（数据模型 §6.8：由发票方向与蓝红类型确定）。
///
/// 销项蓝票/进项蓝票增加净已开/净收票金额，红票减少。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AccountingDirection {
    /// 增加。
    Increase,
    /// 减少。
    Decrease,
}

impl AccountingDirection {
    /// 返回方向的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Increase => "增加",
            Self::Decrease => "减少",
        }
    }

    /// 返回方向的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Increase => "increase",
            Self::Decrease => "decrease",
        }
    }
}

/// 发票状态（数据模型 §6.8：草稿、已登记、已红冲；第 7 章未定义发票状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceStatus {
    /// 草稿。
    #[default]
    Draft,
    /// 已登记。
    Registered,
    /// 已红冲（红票过账后原蓝票置此状态）。
    RedInvoiced,
}

impl InvoiceStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::Registered => "已登记",
            Self::RedInvoiced => "已红冲",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Registered => "registered",
            Self::RedInvoiced => "red_invoiced",
        }
    }
}

/// 发票创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InvoiceData {
    /// 发票方向。
    pub invoice_direction: InvoiceDirection,
    /// 蓝红类型。
    pub invoice_kind: InvoiceKind,
    /// 客户或供应商。
    pub party_id: PartyId,
    /// 发票代码（无代码数电票为空）。
    pub invoice_code: Option<String>,
    /// 发票号码。
    pub invoice_no: String,
    /// 开票日期。
    pub invoice_date: BusinessDate,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 发票尾差（可正可负，含原因）。
    pub rounding_adjustment_amount: Amount,
    /// 尾差原因。
    pub rounding_reason: Option<String>,
    /// 红票必填的原蓝票。
    pub original_invoice_id: Option<InvoiceId>,
}

/// 发票更新数据（仅草稿可编辑）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct InvoiceUpdate {
    /// 开票日期；`None` 表示不修改。
    pub invoice_date: Option<BusinessDate>,
    /// 含税金额；`None` 表示不修改。
    pub gross_amount: Option<Amount>,
    /// 不含税金额；`None` 表示不修改。
    pub net_amount: Option<Amount>,
    /// 税额；`None` 表示不修改。
    pub tax_amount: Option<Amount>,
    /// 发票尾差；`None` 表示不修改。
    pub rounding_adjustment_amount: Option<Amount>,
    /// 尾差原因；`None` 表示不修改，`Some("")` 清除。
    pub rounding_reason: Option<String>,
    /// 发票代码；`None` 表示不修改，`Some("")` 清除。
    pub invoice_code: Option<String>,
    /// 发票号码；`None` 表示不修改。
    pub invoice_no: Option<String>,
}

/// 发票实体（主表类，数据模型 §6.8）。
///
/// 金额一律存正数；`accounting_direction` 由方向与蓝红类型派生；有代码发票按
/// `(invoice_direction, normalized_code, normalized_no)` 唯一、无代码数电票按
/// `(invoice_direction, normalized_no)` 唯一，由唯一索引保证（P3 登记事务先做
/// 规范化号码去重，§8.3）。红票新建独立发票并关联原蓝票，不覆盖原票。
/// 第 7 章未定义发票状态机，`mark_registered` / `mark_red_invoiced` 是受控状态
/// 变迁（§13.3 不发明状态机）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct Invoice {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<InvoiceStatus>,
    /// 发票方向。
    pub invoice_direction: InvoiceDirection,
    /// 蓝红类型。
    pub invoice_kind: InvoiceKind,
    /// 记账方向（派生）。
    pub accounting_direction: AccountingDirection,
    /// 客户或供应商。
    pub party_id: PartyId,
    /// 发票代码。
    pub invoice_code: Option<String>,
    /// 发票号码。
    pub invoice_no: String,
    /// 规范化发票代码（去空白转大写，去重键）。
    pub normalized_code: Option<String>,
    /// 规范化发票号码（去空白转大写，去重键）。
    pub normalized_no: String,
    /// 开票日期。
    pub invoice_date: BusinessDate,
    /// 含税金额。
    pub gross_amount: Amount,
    /// 不含税金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 发票尾差。
    pub rounding_adjustment_amount: Amount,
    /// 尾差原因。
    pub rounding_reason: Option<String>,
    /// 红票原蓝票。
    pub original_invoice_id: Option<InvoiceId>,
}

impl PartialEq for Invoice {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.invoice_direction == other.invoice_direction
            && self.invoice_kind == other.invoice_kind
            && self.accounting_direction == other.accounting_direction
            && self.party_id == other.party_id
            && self.invoice_code == other.invoice_code
            && self.invoice_no == other.invoice_no
            && self.normalized_code == other.normalized_code
            && self.normalized_no == other.normalized_no
            && self.invoice_date == other.invoice_date
            && self.gross_amount == other.gross_amount
            && self.net_amount == other.net_amount
            && self.tax_amount == other.tax_amount
            && self.rounding_adjustment_amount == other.rounding_adjustment_amount
            && self.rounding_reason == other.rounding_reason
            && self.original_invoice_id == other.original_invoice_id
    }
}

impl Eq for Invoice {}

impl Invoice {
    /// 创建发票。
    ///
    /// 完成金额恒等（gross = net + tax）、蓝红与 `original_invoice_id` 一致性
    /// （红票必填、蓝票禁填）、发票代码/号码的 trim/非空/长度校验，并派生
    /// `accounting_direction` 与规范化号码。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::InvoiceId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的发票实体（状态为草稿）。
    ///
    /// # 错误
    /// 当金额三元组不一致、蓝红与引用关系矛盾或代码/号码为空/超长时返回错误。
    #[allow(clippy::too_many_arguments)]
    pub fn new(id: InvoiceId, data: InvoiceData, created_by: impl Into<String>) -> Result<Self> {
        validate_amounts(data.gross_amount, data.net_amount, data.tax_amount)?;
        match data.invoice_kind {
            InvoiceKind::Red if data.original_invoice_id.is_none() => {
                return Err(Error::from("红票必须引用原蓝票"));
            }
            InvoiceKind::Blue if data.original_invoice_id.is_some() => {
                return Err(Error::from("蓝票不得引用原发票"));
            }
            _ => {}
        }
        let invoice_code = normalize_optional_text(data.invoice_code, "发票代码", INVOICE_CODE_MAX_LEN)?;
        let invoice_no = normalize_required_text(
            data.invoice_no,
            "发票号码不能为空",
            INVOICE_NO_MAX_LEN,
            "发票号码过长",
        )?;
        let normalized_code = invoice_code.as_ref().map(|code| code.to_uppercase());
        let rounding_reason =
            normalize_optional_text(data.rounding_reason, "尾差原因", ROUNDING_REASON_MAX_LEN)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(InvoiceStatus::Draft, created_by),
            invoice_direction: data.invoice_direction,
            invoice_kind: data.invoice_kind,
            accounting_direction: derive_accounting_direction(data.invoice_direction, data.invoice_kind),
            party_id: data.party_id,
            invoice_code: invoice_code.clone(),
            invoice_no: invoice_no.clone(),
            normalized_code,
            normalized_no: invoice_no.to_uppercase(),
            invoice_date: data.invoice_date,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
            rounding_adjustment_amount: data.rounding_adjustment_amount,
            rounding_reason,
            original_invoice_id: data.original_invoice_id,
        })
    }

    /// 更新发票草稿。
    ///
    /// 复用 `new` 的校验规则并重新派生规范化号码；发票方向、蓝红类型、往来
    /// 主体与红票原票引用是固定字段，不允许修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当状态非草稿或金额三元组不一致/字段超长时返回错误。
    pub fn update(&mut self, update: InvoiceUpdate, updated_by: impl Into<String>) -> Result<()> {
        if self.stable.status() != InvoiceStatus::Draft {
            return Err(Error::from("已登记或已红冲的发票不可编辑"));
        }
        let gross = update.gross_amount.unwrap_or(self.gross_amount);
        let net = update.net_amount.unwrap_or(self.net_amount);
        let tax = update.tax_amount.unwrap_or(self.tax_amount);
        validate_amounts(gross, net, tax)?;
        if let Some(gross) = update.gross_amount {
            self.gross_amount = gross;
        }
        if let Some(net) = update.net_amount {
            self.net_amount = net;
        }
        if let Some(tax) = update.tax_amount {
            self.tax_amount = tax;
        }
        if let Some(date) = update.invoice_date {
            self.invoice_date = date;
        }
        if let Some(adjustment) = update.rounding_adjustment_amount {
            self.rounding_adjustment_amount = adjustment;
        }
        if let Some(reason) = update.rounding_reason {
            self.rounding_reason =
                normalize_optional_text(Some(reason), "尾差原因", ROUNDING_REASON_MAX_LEN)?;
        }
        if let Some(code) = update.invoice_code {
            self.invoice_code = normalize_optional_text(Some(code), "发票代码", INVOICE_CODE_MAX_LEN)?;
            self.normalized_code = self.invoice_code.as_ref().map(|code| code.to_uppercase());
        }
        if let Some(no) = update.invoice_no {
            self.invoice_no =
                normalize_required_text(no, "发票号码不能为空", INVOICE_NO_MAX_LEN, "发票号码过长")?;
            self.normalized_no = self.invoice_no.to_uppercase();
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 登记发票（草稿 → 已登记）。
    ///
    /// # 参数
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 登记成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非草稿时返回错误。
    pub fn mark_registered(&mut self, updated_by: impl Into<String>) -> Result<()> {
        if self.stable.status() != InvoiceStatus::Draft {
            return Err(Error::from("只有草稿发票可以登记"));
        }
        self.stable.status = InvoiceStatus::Registered;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 标记红票已红冲（已登记 → 已红冲，仅蓝票）。
    ///
    /// # 参数
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 标记成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态非已登记或发票不是蓝票时返回错误。
    pub fn mark_red_invoiced(&mut self, updated_by: impl Into<String>) -> Result<()> {
        if self.stable.status() != InvoiceStatus::Registered {
            return Err(Error::from("只有已登记的发票可以被红冲"));
        }
        if self.invoice_kind != InvoiceKind::Blue {
            return Err(Error::from("红票本身不被再次红冲"));
        }
        self.stable.status = InvoiceStatus::RedInvoiced;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断发票是否已登记。
    ///
    /// # 返回
    /// 状态为 `Registered` 时返回 `true`。
    pub fn is_registered(&self) -> bool {
        self.stable.status() == InvoiceStatus::Registered
    }
}

/// 校验发票金额三元组恒等。
///
/// 规则（数据模型 §4.2）：含税金额必须精确等于不含税金额加税额，各分量非负。
///
/// # 参数
/// * `gross` - 含税金额
/// * `net` - 不含税金额
/// * `tax` - 税额
///
/// # 返回
/// 恒等成立返回 `Ok(())`。
///
/// # 错误
/// 分量非负或恒等不成立时返回错误。
fn validate_amounts(gross: Amount, net: Amount, tax: Amount) -> Result<()> {
    if gross.to_decimal().is_sign_negative()
        || net.to_decimal().is_sign_negative()
        || tax.to_decimal().is_sign_negative()
    {
        return Err(Error::from("发票金额不得为负"));
    }
    if gross != net.checked_add(tax) {
        return Err(Error::from("含税金额必须等于不含税金额加税额"));
    }
    Ok(())
}

/// 由发票方向与蓝红类型派生记账方向。
///
/// # 参数
/// * `direction` - 发票方向
/// * `kind` - 蓝红类型
///
/// # 返回
/// 蓝票为增加，红票为减少。
fn derive_accounting_direction(direction: InvoiceDirection, kind: InvoiceKind) -> AccountingDirection {
    match (direction, kind) {
        (InvoiceDirection::Sales, InvoiceKind::Blue) | (InvoiceDirection::Purchase, InvoiceKind::Blue) => {
            AccountingDirection::Increase
        }
        (InvoiceDirection::Sales, InvoiceKind::Red) | (InvoiceDirection::Purchase, InvoiceKind::Red) => {
            AccountingDirection::Decrease
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn data() -> InvoiceData {
        InvoiceData {
            invoice_direction: InvoiceDirection::Sales,
            invoice_kind: InvoiceKind::Blue,
            party_id: PartyId::new("party-1"),
            invoice_code: Some(" 1100199999 ".to_string()),
            invoice_no: " 01234567 ".to_string(),
            invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
            gross_amount: Amount::from_str("1000.00").unwrap(),
            net_amount: Amount::from_str("884.96").unwrap(),
            tax_amount: Amount::from_str("115.04").unwrap(),
            rounding_adjustment_amount: Amount::from_str("0.00").unwrap(),
            rounding_reason: None,
            original_invoice_id: None,
        }
    }

    #[test]
    fn new_normalizes_and_derives_accounting_direction() {
        let invoice = Invoice::new(InvoiceId::new("inv-1"), data(), "admin-1").unwrap();

        assert_eq!(invoice.invoice_no, "01234567");
        assert_eq!(invoice.invoice_code.as_deref(), Some("1100199999"));
        assert_eq!(invoice.normalized_no, "01234567");
        assert_eq!(invoice.normalized_code.as_deref(), Some("1100199999"));
        assert_eq!(invoice.accounting_direction, AccountingDirection::Increase);
        assert_eq!(invoice.stable.status(), InvoiceStatus::Draft);
        assert!(!invoice.is_registered());
    }

    #[test]
    fn new_rejects_amount_mismatch_and_red_blue_relation() {
        let mismatch = InvoiceData {
            net_amount: Amount::from_str("800.00").unwrap(),
            ..data()
        };
        assert!(Invoice::new(InvoiceId::new("inv-2"), mismatch, "admin").is_err());

        let red_without_original = InvoiceData {
            invoice_kind: InvoiceKind::Red,
            original_invoice_id: None,
            ..data()
        };
        assert!(Invoice::new(InvoiceId::new("inv-3"), red_without_original, "admin").is_err());

        let blue_with_original = InvoiceData {
            invoice_kind: InvoiceKind::Blue,
            original_invoice_id: Some(InvoiceId::new("inv-1")),
            ..data()
        };
        assert!(Invoice::new(InvoiceId::new("inv-4"), blue_with_original, "admin").is_err());

        let red_with_original = InvoiceData {
            invoice_kind: InvoiceKind::Red,
            original_invoice_id: Some(InvoiceId::new("inv-1")),
            ..data()
        };
        let red = Invoice::new(InvoiceId::new("inv-5"), red_with_original, "admin").unwrap();
        assert_eq!(red.accounting_direction, AccountingDirection::Decrease);
    }

    #[test]
    fn update_edits_draft_and_rejects_registered() {
        let mut invoice = Invoice::new(InvoiceId::new("inv-1"), data(), "admin-1").unwrap();

        invoice
            .update(
                InvoiceUpdate {
                    gross_amount: Some(Amount::from_str("2000.00").unwrap()),
                    net_amount: Some(Amount::from_str("1769.91").unwrap()),
                    tax_amount: Some(Amount::from_str("230.09").unwrap()),
                    invoice_no: Some(" 99887766 ".to_string()),
                    ..Default::default()
                },
                "admin-2",
            )
            .unwrap();
        assert_eq!(invoice.gross_amount, Amount::from_str("2000.00").unwrap());
        assert_eq!(invoice.invoice_no, "99887766");
        assert_eq!(invoice.normalized_no, "99887766");
        assert_eq!(invoice.stable.updated_by, "admin-2");

        invoice.mark_registered("admin-2").unwrap();
        assert!(invoice.is_registered());
        assert!(invoice
            .update(
                InvoiceUpdate {
                    invoice_date: Some(BusinessDate::from_ymd(2026, 8, 7).unwrap()),
                    ..Default::default()
                },
                "admin-3",
            )
            .is_err());
    }

    #[test]
    fn status_markers_are_guarded() {
        let mut blue = Invoice::new(InvoiceId::new("inv-1"), data(), "admin-1").unwrap();
        assert!(blue.mark_red_invoiced("admin").is_err(), "草稿不可直接红冲");
        blue.mark_registered("admin").unwrap();
        assert!(blue.mark_registered("admin").is_err(), "重复登记被拒");
        blue.mark_red_invoiced("admin").unwrap();
        assert_eq!(blue.stable.status(), InvoiceStatus::RedInvoiced);

        let mut red = Invoice::new(
            InvoiceId::new("inv-2"),
            InvoiceData {
                invoice_kind: InvoiceKind::Red,
                original_invoice_id: Some(InvoiceId::new("inv-1")),
                ..data()
            },
            "admin-1",
        )
        .unwrap();
        red.mark_registered("admin").unwrap();
        assert!(red.mark_red_invoiced("admin").is_err(), "红票不被再次红冲");
    }

    #[test]
    fn invoice_bson_roundtrip_preserves_fields() {
        let invoice = Invoice::new(InvoiceId::new("inv-1"), data(), "admin-1").unwrap();
        let back: Invoice = bson::from_document(bson::to_document(&invoice).unwrap()).unwrap();
        assert_eq!(back, invoice);
    }

    /// 发票无审批约束：不得出现绑定字段或审批状态机。
    #[test]
    fn invoice_has_no_approval_binding_or_state_machine() {
        let invoice = Invoice::new(InvoiceId::new("inv-1"), data(), "admin-1").unwrap();
        let value = serde_json::to_value(&invoice).unwrap();
        let object = value.as_object().expect("发票序列化为对象");
        assert!(!object.contains_key("approval_binding"));
        assert!(!object.contains_key("approval_subject_version"));
        assert!(!object.contains_key("pending_allocations"));
        assert_eq!(invoice.stable.status(), InvoiceStatus::Draft);
        assert_eq!(InvoiceStatus::Draft.as_str(), "draft");
        assert_eq!(InvoiceStatus::Registered.as_str(), "registered");
        assert_eq!(InvoiceStatus::RedInvoiced.as_str(), "red_invoiced");

        let production = include_str!("invoice.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("IN_APPROVAL"));
        assert!(!production.contains("fn start_approval"));
        assert!(!production.contains("approval_subject_version"));
        assert!(!production.contains("ApprovalDefinitionBinding"));
        assert!(!production.contains("PENDING_REVIEW"));
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&InvoiceDirection::Purchase).unwrap(),
            "\"purchase\""
        );
        assert_eq!(serde_json::to_string(&InvoiceKind::Red).unwrap(), "\"red\"");
        assert_eq!(
            serde_json::to_string(&InvoiceStatus::RedInvoiced).unwrap(),
            "\"red_invoiced\""
        );
        assert_eq!(InvoiceDirection::Sales.label(), "销项");
        assert_eq!(InvoiceKind::Blue.label(), "蓝票");
        assert_eq!(InvoiceStatus::Registered.label(), "已登记");
        assert_eq!(AccountingDirection::Decrease.as_str(), "decrease");
    }
}
