//! `supplier_commercial_profile_revision`：供应商商务结算版本（§6.2）。
//!
//! 不可变修订：新版本保存即成为当前版本，没有生效窗口概念；付款条件按
//! §2.2 / §4.4 内联为结构化快照字段。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::revision::RevisionBase;
pub use erp_core::ids::{PartyId, SupplierAccountId, SupplierCommercialProfileRevisionId};
use erp_core::money::Rate;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::business_category::{normalize_business_category, split_encoded_payment_term_snapshot};
pub use super::payment_term::SettlementMode;
use super::payment_term::SupplierPaymentTerm;

/// 付款条件快照最大长度。
const PAYMENT_TERM_SNAPSHOT_MAX_LEN: usize = 64;
/// 变更原因最大长度。
const CHANGE_REASON_MAX_LEN: usize = 500;

/// 对账周期（§6.2：日、周、月、季、年或无需周期对账；固定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReconciliationCycle {
    /// 日。
    Daily,
    /// 周。
    Weekly,
    /// 月。
    Monthly,
    /// 季。
    Quarterly,
    /// 年。
    Yearly,
    /// 半年。
    HalfYearly,
    /// 无需周期对账。
    None,
}

impl ReconciliationCycle {
    /// 返回周期的中文展示名。
    ///
    /// 映射表见 [`RECONCILIATION_CYCLE_DISPLAY`]（erp-supplier-002）。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        super::display::label_of(*self, &RECONCILIATION_CYCLE_DISPLAY)
    }

    /// 返回周期的稳定代码。
    ///
    /// 映射表见 [`RECONCILIATION_CYCLE_DISPLAY`]（erp-supplier-002）。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        super::display::code_of(*self, &RECONCILIATION_CYCLE_DISPLAY)
    }
}

/// 对账周期展示映射表（erp-supplier-002）：变体→中文名/稳定代码。
const RECONCILIATION_CYCLE_DISPLAY: [super::display::DisplayEntry<ReconciliationCycle>; 7] = [
    super::display::DisplayEntry { variant: ReconciliationCycle::Daily, label: "日", code: "daily" },
    super::display::DisplayEntry { variant: ReconciliationCycle::Weekly, label: "周", code: "weekly" },
    super::display::DisplayEntry { variant: ReconciliationCycle::Monthly, label: "月", code: "monthly" },
    super::display::DisplayEntry { variant: ReconciliationCycle::Quarterly, label: "季", code: "quarterly" },
    super::display::DisplayEntry { variant: ReconciliationCycle::Yearly, label: "年", code: "yearly" },
    super::display::DisplayEntry {
        variant: ReconciliationCycle::HalfYearly,
        label: "半年",
        code: "half_yearly",
    },
    super::display::DisplayEntry {
        variant: ReconciliationCycle::None, label: "无需周期对账", code: "none"
    },
];

/// 发票类型（§6.2：增值税专用发票、增值税普通发票、电子发票等受控代码）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvoiceType {
    /// 增值税专用发票。
    VatSpecial,
    /// 增值税普通发票。
    VatNormal,
    /// 电子发票。
    Electronic,
}

impl InvoiceType {
    /// 返回类型的中文展示名。
    ///
    /// 映射表见 [`INVOICE_TYPE_DISPLAY`]（erp-supplier-002）。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        super::display::label_of(*self, &INVOICE_TYPE_DISPLAY)
    }

    /// 返回类型的稳定代码。
    ///
    /// 映射表见 [`INVOICE_TYPE_DISPLAY`]（erp-supplier-002）。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        super::display::code_of(*self, &INVOICE_TYPE_DISPLAY)
    }
}

/// 发票类型展示映射表（erp-supplier-002）：变体→中文名/稳定代码。
const INVOICE_TYPE_DISPLAY: [super::display::DisplayEntry<InvoiceType>; 3] = [
    super::display::DisplayEntry {
        variant: InvoiceType::VatSpecial,
        label: "增值税专用发票",
        code: "vat_special",
    },
    super::display::DisplayEntry {
        variant: InvoiceType::VatNormal,
        label: "增值税普通发票",
        code: "vat_normal",
    },
    super::display::DisplayEntry {
        variant: InvoiceType::Electronic, label: "电子发票", code: "electronic"
    },
];

/// 商务结算版本创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCommercialProfileRevisionData {
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 同一稳定对象内从 1 递增的修订序号。
    pub revision_no: u32,
    /// 结算方式。
    pub settlement_mode: SettlementMode,
    /// 对账周期。
    pub reconciliation_cycle: ReconciliationCycle,
    /// 结构化付款条件快照（受控码表稳定代码，§2.2 内联快照）。
    pub payment_term_snapshot: String,
    /// 经营类目；未登记时为空。
    pub business_category: Option<String>,
    /// 发票类型。
    pub invoice_type: InvoiceType,
    /// 发票税点（如 `0.13` 表示 13%；定点类型，§4.2）。
    pub invoice_tax_rate: Option<Rate>,
    /// 常用进项税率；None 读取旧单值，Some([]) 明确表示未登记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoice_tax_rates: Option<Vec<Rate>>,
    /// 与我司签约的公司主体（内部 `party` 引用）。
    pub signing_entity_party_id: PartyId,
    /// 付款时的公司主体（内部 `party` 引用）。
    pub payment_entity_party_id: PartyId,
    /// 变更原因。
    pub change_reason: String,
}

/// 商务结算版本实体（不可变修订，§6.2）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierCommercialProfileRevision {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub revision: RevisionBase,
    /// 供应商角色 ID。
    pub supplier_id: SupplierAccountId,
    /// 结算方式。
    pub settlement_mode: SettlementMode,
    /// 对账周期。
    pub reconciliation_cycle: ReconciliationCycle,
    /// 结构化付款条件快照。
    pub payment_term_snapshot: String,
    /// 经营类目；历史修订可能缺省，读取时从付款条件快照拆出。
    #[serde(default)]
    pub business_category: Option<String>,
    /// 发票类型。
    pub invoice_type: InvoiceType,
    /// 发票税点。
    pub invoice_tax_rate: Option<Rate>,
    /// 常用进项税率；None 读取旧单值，Some([]) 明确表示未登记。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invoice_tax_rates: Option<Vec<Rate>>,
    /// 签约主体。
    pub signing_entity_party_id: PartyId,
    /// 付款主体。
    pub payment_entity_party_id: PartyId,
    /// 变更原因。
    pub change_reason: String,
}

impl SupplierCommercialProfileRevision {
    /// 创建商务结算版本。
    ///
    /// 完成付款条件快照与变更原因的必填校验与规范化（去首尾空白、
    /// 长度上限）；发票税点必须是 `[0, 1)` 的定点小数（§4.2 税率约定）。
    /// 历史把经营类目编码进付款条件快照时，在此拆成独立字段。
    ///
    /// # 参数
    /// * `id` - 实体主键（`erp_core::ids::SupplierCommercialProfileRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的版本实体。
    ///
    /// # 错误
    /// 当付款条件缺少可计算规则、与结算方式不一致，或其他版本字段非法时返回错误。
    pub fn new(
        id: SupplierCommercialProfileRevisionId,
        data: SupplierCommercialProfileRevisionData,
    ) -> Result<Self> {
        let (payment_term_snapshot, business_category) =
            split_payment_term_fields(data.payment_term_snapshot, data.business_category)?;
        let payment_term = SupplierPaymentTerm::parse(&payment_term_snapshot)?;
        ensure_settlement_rules(payment_term, data.settlement_mode, data.reconciliation_cycle)?;
        let change_reason = normalize_required_text(
            data.change_reason,
            "变更原因不能为空",
            CHANGE_REASON_MAX_LEN,
            "变更原因过长",
        )?;
        let rates = normalize_invoice_tax_rates(data.invoice_tax_rates.as_deref(), data.invoice_tax_rate)?;
        let legacy_rate = (rates.len() == 1).then(|| rates[0]);

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            supplier_id: data.supplier_id,
            settlement_mode: data.settlement_mode,
            reconciliation_cycle: data.reconciliation_cycle,
            payment_term_snapshot: payment_term.code(),
            business_category,
            invoice_type: data.invoice_type,
            invoice_tax_rate: legacy_rate,
            invoice_tax_rates: Some(rates),
            signing_entity_party_id: data.signing_entity_party_id,
            payment_entity_party_id: data.payment_entity_party_id,
            change_reason,
        })
    }

    /// 返回新集合或旧单值，保留明确未登记的空集合。
    pub fn tax_rates(&self) -> Vec<Rate> {
        self.invoice_tax_rates.clone().unwrap_or_else(|| self.invoice_tax_rate.into_iter().collect())
    }

    /// 返回不含经营类目编码的付款条件代码。
    ///
    /// 历史修订可能仍把类目写在快照里；调用方落采购单或展示时必须用本方法，
    /// 不得直接使用 `payment_term_snapshot`。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 去编码后的付款条件；无标记时原样返回去空白快照。
    ///
    /// # 错误
    /// 无。
    pub fn effective_payment_term_code(&self) -> String {
        let code = split_encoded_payment_term_snapshot(&self.payment_term_snapshot).payment_term_code;
        SupplierPaymentTerm::parse(&code).map(|term| term.code().to_string()).unwrap_or(code)
    }

    /// 返回经营类目：独立字段优先，否则从历史付款条件快照拆出。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 未登记时返回 `None`。
    ///
    /// # 错误
    /// 无。
    pub fn effective_business_category(&self) -> Option<String> {
        self.business_category
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .or_else(|| split_encoded_payment_term_snapshot(&self.payment_term_snapshot).business_category)
    }
}

/// 收集商务版本引用的签约与付款主体 ID。
///
/// 排序去重后映射为 [`PartyId`]，供列表/详情批量读取法定名称；
/// Service 与仓储共用本实现，不再各自保留去重逻辑。
///
/// # 参数
/// * `profiles` - 商务资料版本
///
/// # 返回
/// 返回去重后的主体 ID，供批量读取法定名称。
///
/// # 错误
/// 无。
pub(crate) fn commercial_party_ids(profiles: &[SupplierCommercialProfileRevision]) -> Vec<PartyId> {
    let mut ids: Vec<String> = profiles
        .iter()
        .flat_map(|profile| {
            [profile.signing_entity_party_id.to_string(), profile.payment_entity_party_id.to_string()]
        })
        .collect();
    ids.sort();
    ids.dedup();
    ids.into_iter().map(|id| PartyId::new(&id)).collect()
}

/// 把付款条件快照与经营类目规范成两个独立字段。
///
/// 显式传入的经营类目优先于快照内历史编码。
///
/// # 参数
/// * `payment_term_snapshot` - 原始付款条件快照
/// * `business_category` - 显式经营类目
///
/// # 返回
/// 不含类目编码的付款条件快照，以及独立经营类目。
///
/// # 错误
/// 付款条件为空/超长，或经营类目超长时返回错误。
fn split_payment_term_fields(
    payment_term_snapshot: String,
    business_category: Option<String>,
) -> Result<(String, Option<String>)> {
    let parts = split_encoded_payment_term_snapshot(&payment_term_snapshot);
    let payment_term_snapshot = normalize_required_text(
        parts.payment_term_code,
        "付款条件快照不能为空",
        PAYMENT_TERM_SNAPSHOT_MAX_LEN,
        "付款条件快照过长",
    )?;
    let explicit = normalize_business_category(business_category)?;
    let encoded = normalize_business_category(parts.business_category)?;
    Ok((payment_term_snapshot, explicit.or(encoded)))
}

/// 周期结算的付款规则与对账周期须一致；存量非周期资料维持原规则。
fn ensure_settlement_rules(
    term: SupplierPaymentTerm,
    mode: SettlementMode,
    cycle: ReconciliationCycle,
) -> Result<()> {
    if term.settlement_mode() != mode {
        return Err(Error::from("结算方式与付款条件不一致，请重新选择"));
    }
    let Some((period, _)) = term.calendar_due() else {
        return Ok(());
    };
    use erp_core::common::calendar::CalendarPeriod;
    let expected = match period {
        CalendarPeriod::Week => ReconciliationCycle::Weekly,
        CalendarPeriod::Month => ReconciliationCycle::Monthly,
        CalendarPeriod::Quarter => ReconciliationCycle::Quarterly,
        CalendarPeriod::HalfYear => ReconciliationCycle::HalfYearly,
        CalendarPeriod::Year => ReconciliationCycle::Yearly,
    };
    if cycle != expected {
        return Err(Error::from("对账周期与自然结算周期不一致"));
    }
    Ok(())
}

/// 校验发票税点是否落在合法区间。
///
/// # 参数
/// * `rate` - 发票税点（如 `0.13`）
///
/// # 返回
/// 税点在 `[0, 1)` 内返回 `Ok(())`。
///
/// # 错误
/// 税点小于 0 或大于等于 1 时返回错误。
fn ensure_tax_rate_valid(rate: Rate) -> Result<()> {
    let decimal = rate.to_decimal();
    if decimal < rust_decimal::Decimal::ZERO || decimal >= rust_decimal::Decimal::ONE {
        return Err(Error::from("发票税点必须在 [0, 1) 区间内（如 0.13 表示 13%）"));
    }
    Ok(())
}

/// 校验并去重常用税率，明确空集合不回退旧值。
///
/// # Errors
/// 超过 32 项、税率非法或单双字段矛盾时返回错误。
pub fn normalize_invoice_tax_rates(rates: Option<&[Rate]>, legacy: Option<Rate>) -> Result<Vec<Rate>> {
    let mut values = rates.map(<[Rate]>::to_vec).unwrap_or_else(|| legacy.into_iter().collect());
    if values.len() > 32 {
        return Err(Error::from("常用进项税率不能超过 32 项"));
    }
    for rate in &values {
        ensure_tax_rate_valid(*rate)?;
    }
    values.sort_by_key(|rate| rate.to_decimal());
    values.dedup();
    if rates.is_some() && legacy.is_some_and(|rate| values != vec![rate]) {
        return Err(Error::from("常用进项税率与旧税率字段不一致"));
    }
    Ok(values)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{PartyId, SupplierAccountId, SupplierCommercialProfileRevisionId};
    use erp_core::money::{Amount, Quantity, Rate, UnitPrice, line_amounts};

    use super::{
        InvoiceType, ReconciliationCycle, SettlementMode, SupplierCommercialProfileRevision,
        SupplierCommercialProfileRevisionData,
    };

    fn profile_data() -> SupplierCommercialProfileRevisionData {
        SupplierCommercialProfileRevisionData {
            supplier_id: SupplierAccountId::new("supplier-1"),
            revision_no: 1,
            settlement_mode: SettlementMode::Prepayment,
            reconciliation_cycle: ReconciliationCycle::Monthly,
            payment_term_snapshot: " PREPAY_30 ".to_string(),
            business_category: None,
            invoice_type: InvoiceType::VatSpecial,
            invoice_tax_rate: Some(Rate::from_str("0.13").unwrap()),
            invoice_tax_rates: None,
            signing_entity_party_id: PartyId::new("party-internal-1"),
            payment_entity_party_id: PartyId::new("party-internal-2"),
            change_reason: " 首次建档 ".to_string(),
        }
    }

    /// happy path：快照与原因去空白，受控代码与税点落库。
    #[test]
    fn new_trims_and_normalizes() {
        let profile = SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new("profile-rev-1"),
            profile_data(),
        )
        .unwrap();
        assert_eq!(profile.payment_term_snapshot, "PREPAY_30");
        assert_eq!(profile.business_category, None);
        assert_eq!(profile.change_reason, "首次建档");
        assert_eq!(profile.settlement_mode, SettlementMode::Prepayment);
        assert_eq!(profile.reconciliation_cycle, ReconciliationCycle::Monthly);
        assert_eq!(profile.invoice_type, InvoiceType::VatSpecial);
        assert_eq!(profile.revision.revision_no, 1);
    }

    /// 失败路径：快照/原因为空或超长、付款条件不受控、结算方式不匹配或税点越界。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_snapshot = SupplierCommercialProfileRevisionData {
            payment_term_snapshot: "   ".to_string(),
            ..profile_data()
        };
        assert!(
            SupplierCommercialProfileRevision::new(
                SupplierCommercialProfileRevisionId::new("p"),
                blank_snapshot,
            )
            .is_err()
        );

        let overlong_reason =
            SupplierCommercialProfileRevisionData { change_reason: "x".repeat(501), ..profile_data() };
        assert!(
            SupplierCommercialProfileRevision::new(
                SupplierCommercialProfileRevisionId::new("p"),
                overlong_reason,
            )
            .is_err()
        );

        let bad_rate = SupplierCommercialProfileRevisionData {
            invoice_tax_rate: Some(Rate::from_str("1.05").unwrap()),
            invoice_tax_rates: None,
            ..profile_data()
        };
        assert!(
            SupplierCommercialProfileRevision::new(SupplierCommercialProfileRevisionId::new("p"), bad_rate,)
                .is_err()
        );

        let ambiguous_payment_term = SupplierCommercialProfileRevisionData {
            settlement_mode: SettlementMode::PayAfterUse,
            payment_term_snapshot: "先用后付".to_string(),
            ..profile_data()
        };
        assert!(
            SupplierCommercialProfileRevision::new(
                SupplierCommercialProfileRevisionId::new("p"),
                ambiguous_payment_term,
            )
            .is_err()
        );

        let mismatched_settlement = SupplierCommercialProfileRevisionData {
            settlement_mode: SettlementMode::PayAfterUse,
            payment_term_snapshot: "PREPAY_30".to_string(),
            ..profile_data()
        };
        assert!(
            SupplierCommercialProfileRevision::new(
                SupplierCommercialProfileRevisionId::new("p"),
                mismatched_settlement,
            )
            .is_err()
        );
    }

    /// 金额三元组：发票税点参与行金额计算时保持 gross = net + tax。
    #[test]
    fn invoice_tax_rate_produces_consistent_line_amounts() {
        let profile = SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new("profile-rev-2"),
            profile_data(),
        )
        .unwrap();

        let (gross, net, tax) = line_amounts(
            UnitPrice::from_str("100.0000").unwrap(),
            Quantity::from_str("3.000000").unwrap(),
            profile.invoice_tax_rate.unwrap(),
        );
        assert_eq!(
            gross.to_decimal(),
            net.to_decimal() + tax.to_decimal(),
            "gross = net + tax 对税点 {} 不成立",
            profile.invoice_tax_rate.unwrap()
        );
        assert_eq!(tax.to_decimal(), Amount::from_str("39.00").unwrap().to_decimal());
    }

    /// 历史编码快照在构造时拆成独立经营类目，显式类目优先于快照内编码。
    #[test]
    fn new_splits_encoded_snapshot_and_prefers_explicit_category() {
        let encoded = SupplierCommercialProfileRevisionData {
            settlement_mode: SettlementMode::CashSettlement,
            payment_term_snapshot: "现结｜经营类目：礼盒".to_string(),
            business_category: None,
            ..profile_data()
        };
        let profile = SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new("profile-rev-encoded"),
            encoded,
        )
        .unwrap();
        assert_eq!(profile.payment_term_snapshot, "CASH_ON_APPROVAL");
        assert_eq!(profile.business_category.as_deref(), Some("礼盒"));
        assert_eq!(profile.effective_payment_term_code(), "CASH_ON_APPROVAL");
        assert_eq!(profile.effective_business_category().as_deref(), Some("礼盒"));

        let explicit = SupplierCommercialProfileRevisionData {
            settlement_mode: SettlementMode::CashSettlement,
            payment_term_snapshot: "现结｜经营类目：礼盒".to_string(),
            business_category: Some(" 鲜花 ".to_string()),
            ..profile_data()
        };
        let profile = SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new("profile-rev-explicit"),
            explicit,
        )
        .unwrap();
        assert_eq!(profile.payment_term_snapshot, "CASH_ON_APPROVAL");
        assert_eq!(profile.business_category.as_deref(), Some("鲜花"));
    }

    /// 缺省 `business_category` 的历史文档仍可反序列化，并从快照拆出类目。
    #[test]
    fn legacy_document_without_category_field_splits_on_read() {
        let profile = SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new("profile-rev-legacy"),
            profile_data(),
        )
        .unwrap();
        let mut doc = serde_json::to_value(&profile).unwrap();
        let object = doc.as_object_mut().expect("商务资料必须是 JSON 对象");
        object.remove("business_category");
        object.insert(
            "payment_term_snapshot".to_string(),
            serde_json::Value::String("现结｜经营类目：礼盒".to_string()),
        );
        let loaded: SupplierCommercialProfileRevision = serde_json::from_value(doc).unwrap();
        assert_eq!(loaded.business_category, None);
        assert_eq!(loaded.effective_payment_term_code(), "CASH_ON_APPROVAL");
        assert_eq!(loaded.effective_business_category().as_deref(), Some("礼盒"));
    }

    /// 实体 BSON 往返（含 Rate 与 ID）。
    #[test]
    fn bson_roundtrip() {
        let profile = SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new("profile-rev-3"),
            profile_data(),
        )
        .unwrap();
        let roundtrip: SupplierCommercialProfileRevision =
            serde_json::from_value(serde_json::to_value(&profile).unwrap()).unwrap();
        assert_eq!(roundtrip, profile);
    }

    /// 受控代码的稳定序列化形态与中文标签。
    #[test]
    fn enums_serialize_with_stable_codes() {
        assert_eq!(serde_json::to_string(&SettlementMode::PayAfterUse).unwrap(), "\"pay_after_use\"");
        assert_eq!(serde_json::to_string(&ReconciliationCycle::Yearly).unwrap(), "\"yearly\"");
        assert_eq!(serde_json::to_string(&InvoiceType::Electronic).unwrap(), "\"electronic\"");
        assert_eq!(SettlementMode::CashSettlement.label(), "现结");
        assert_eq!(ReconciliationCycle::None.label(), "无需周期对账");
        assert_eq!(InvoiceType::VatSpecial.label(), "增值税专用发票");
    }
    #[test]
    fn multiple_tax_rates_do_not_select_a_default_and_old_documents_still_read() {
        let old = SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new("p"),
            profile_data(),
        )
        .unwrap();
        let mut json = serde_json::to_value(&old).unwrap();
        json.as_object_mut().unwrap().remove("invoice_tax_rates");
        let legacy: SupplierCommercialProfileRevision = serde_json::from_value(json).unwrap();
        assert_eq!(legacy.tax_rates(), vec![Rate::from_str("0.13").unwrap()]);
        let data = SupplierCommercialProfileRevisionData {
            invoice_tax_rate: None,
            invoice_tax_rates: Some(vec![
                Rate::from_str("0.13").unwrap(),
                Rate::from_str("0.09").unwrap(),
                Rate::from_str("0.13").unwrap(),
            ]),
            ..profile_data()
        };
        let profile =
            SupplierCommercialProfileRevision::new(SupplierCommercialProfileRevisionId::new("p"), data)
                .unwrap();
        assert!(profile.invoice_tax_rate.is_none());
        assert_eq!(
            profile.tax_rates().iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["0.09", "0.13"]
        );
        let empty = SupplierCommercialProfileRevisionData {
            invoice_tax_rate: None,
            invoice_tax_rates: Some(vec![]),
            ..profile_data()
        };
        let empty =
            SupplierCommercialProfileRevision::new(SupplierCommercialProfileRevisionId::new("p"), empty)
                .unwrap();
        assert!(empty.tax_rates().is_empty());
        let conflict =
            SupplierCommercialProfileRevisionData { invoice_tax_rates: Some(vec![]), ..profile_data() };
        assert!(
            SupplierCommercialProfileRevision::new(SupplierCommercialProfileRevisionId::new("p"), conflict)
                .is_err()
        );
    }
    #[test]
    fn periodic_settlement_requires_matching_reconciliation_cycle() {
        let valid = SupplierCommercialProfileRevisionData {
            settlement_mode: SettlementMode::Monthly,
            reconciliation_cycle: ReconciliationCycle::Monthly,
            payment_term_snapshot: "PERIOD_MONTH_15".into(),
            ..profile_data()
        };
        assert!(
            SupplierCommercialProfileRevision::new(
                SupplierCommercialProfileRevisionId::new("p"),
                valid.clone()
            )
            .is_ok()
        );
        let invalid = SupplierCommercialProfileRevisionData {
            reconciliation_cycle: ReconciliationCycle::Weekly,
            ..valid
        };
        assert!(
            SupplierCommercialProfileRevision::new(SupplierCommercialProfileRevisionId::new("p"), invalid)
                .is_err()
        );
    }
}
