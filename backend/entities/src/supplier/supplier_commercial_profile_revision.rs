//! `supplier_commercial_profile_revision`：供应商商务结算版本（§6.2）。
//!
//! 不可变修订：新版本保存即成为当前版本，没有生效窗口概念；付款条件按
//! §2.2 / §4.4 内联为结构化快照字段。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::revision::RevisionBase;
use crate::errors::{Error, Result};
use crate::money::Rate;
use crate::validation::normalize_required_text;

pub use crate::ids::{PartyId, SupplierAccountId, SupplierCommercialProfileRevisionId};

/// 付款条件快照最大长度。
const PAYMENT_TERM_SNAPSHOT_MAX_LEN: usize = 64;
/// 变更原因最大长度。
const CHANGE_REASON_MAX_LEN: usize = 500;

/// 结算方式（§6.2：预付款、先用后付、现结等受控代码；固定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SettlementMode {
    /// 预付款。
    Prepayment,
    /// 先用后付。
    PayAfterUse,
    /// 现结。
    CashSettlement,
}

impl SettlementMode {
    /// 返回方式的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Prepayment => "预付款",
            Self::PayAfterUse => "先用后付",
            Self::CashSettlement => "现结",
        }
    }

    /// 返回方式的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Prepayment => "prepayment",
            Self::PayAfterUse => "pay_after_use",
            Self::CashSettlement => "cash_settlement",
        }
    }
}

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
    /// 无需周期对账。
    None,
}

impl ReconciliationCycle {
    /// 返回周期的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Daily => "日",
            Self::Weekly => "周",
            Self::Monthly => "月",
            Self::Quarterly => "季",
            Self::Yearly => "年",
            Self::None => "无需周期对账",
        }
    }

    /// 返回周期的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Daily => "daily",
            Self::Weekly => "weekly",
            Self::Monthly => "monthly",
            Self::Quarterly => "quarterly",
            Self::Yearly => "yearly",
            Self::None => "none",
        }
    }
}

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
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::VatSpecial => "增值税专用发票",
            Self::VatNormal => "增值税普通发票",
            Self::Electronic => "电子发票",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::VatSpecial => "vat_special",
            Self::VatNormal => "vat_normal",
            Self::Electronic => "electronic",
        }
    }
}

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
    /// 发票类型。
    pub invoice_type: InvoiceType,
    /// 发票税点（如 `0.13` 表示 13%；定点类型，§4.2）。
    pub invoice_tax_rate: Rate,
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
    /// 发票类型。
    pub invoice_type: InvoiceType,
    /// 发票税点。
    pub invoice_tax_rate: Rate,
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
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierCommercialProfileRevisionId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的版本实体。
    ///
    /// # 错误
    /// 当快照/原因为空或超长、税点越界时返回错误。
    pub fn new(
        id: SupplierCommercialProfileRevisionId,
        data: SupplierCommercialProfileRevisionData,
    ) -> Result<Self> {
        let payment_term_snapshot = normalize_required_text(
            data.payment_term_snapshot,
            "付款条件快照不能为空",
            PAYMENT_TERM_SNAPSHOT_MAX_LEN,
            "付款条件快照过长",
        )?;
        let change_reason = normalize_required_text(
            data.change_reason,
            "变更原因不能为空",
            CHANGE_REASON_MAX_LEN,
            "变更原因过长",
        )?;
        ensure_tax_rate_valid(data.invoice_tax_rate)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            revision: RevisionBase::new(data.revision_no),
            supplier_id: data.supplier_id,
            settlement_mode: data.settlement_mode,
            reconciliation_cycle: data.reconciliation_cycle,
            payment_term_snapshot,
            invoice_type: data.invoice_type,
            invoice_tax_rate: data.invoice_tax_rate,
            signing_entity_party_id: data.signing_entity_party_id,
            payment_entity_party_id: data.payment_entity_party_id,
            change_reason,
        })
    }
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

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{
        InvoiceType, ReconciliationCycle, SettlementMode, SupplierCommercialProfileRevision,
        SupplierCommercialProfileRevisionData,
    };
    use crate::ids::{PartyId, SupplierAccountId, SupplierCommercialProfileRevisionId};
    use crate::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};

    fn profile_data() -> SupplierCommercialProfileRevisionData {
        SupplierCommercialProfileRevisionData {
            supplier_id: SupplierAccountId::new("supplier-1"),
            revision_no: 1,
            settlement_mode: SettlementMode::Prepayment,
            reconciliation_cycle: ReconciliationCycle::Monthly,
            payment_term_snapshot: " PREPAY_30 ".to_string(),
            invoice_type: InvoiceType::VatSpecial,
            invoice_tax_rate: Rate::from_str("0.13").unwrap(),
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
        assert_eq!(profile.change_reason, "首次建档");
        assert_eq!(profile.settlement_mode, SettlementMode::Prepayment);
        assert_eq!(profile.reconciliation_cycle, ReconciliationCycle::Monthly);
        assert_eq!(profile.invoice_type, InvoiceType::VatSpecial);
        assert_eq!(profile.revision.revision_no, 1);
    }

    /// 失败路径：快照/原因为空或超长、税点越界。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_snapshot = SupplierCommercialProfileRevisionData {
            payment_term_snapshot: "   ".to_string(),
            ..profile_data()
        };
        assert!(SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new("p"),
            blank_snapshot,
        )
        .is_err());

        let overlong_reason = SupplierCommercialProfileRevisionData {
            change_reason: "x".repeat(501),
            ..profile_data()
        };
        assert!(SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new("p"),
            overlong_reason,
        )
        .is_err());

        let bad_rate = SupplierCommercialProfileRevisionData {
            invoice_tax_rate: Rate::from_str("1.05").unwrap(),
            ..profile_data()
        };
        assert!(SupplierCommercialProfileRevision::new(
            SupplierCommercialProfileRevisionId::new("p"),
            bad_rate,
        )
        .is_err());
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
            profile.invoice_tax_rate,
        );
        assert_eq!(
            gross.to_decimal(),
            net.to_decimal() + tax.to_decimal(),
            "gross = net + tax 对税点 {} 不成立",
            profile.invoice_tax_rate
        );
        assert_eq!(tax.to_decimal(), Amount::from_str("39.00").unwrap().to_decimal());
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
            bson::from_document(bson::to_document(&profile).unwrap()).unwrap();
        assert_eq!(roundtrip, profile);
    }

    /// 受控代码的稳定序列化形态与中文标签。
    #[test]
    fn enums_serialize_with_stable_codes() {
        assert_eq!(
            serde_json::to_string(&SettlementMode::PayAfterUse).unwrap(),
            "\"pay_after_use\""
        );
        assert_eq!(
            serde_json::to_string(&ReconciliationCycle::Yearly).unwrap(),
            "\"yearly\""
        );
        assert_eq!(
            serde_json::to_string(&InvoiceType::Electronic).unwrap(),
            "\"electronic\""
        );
        assert_eq!(SettlementMode::CashSettlement.label(), "现结");
        assert_eq!(ReconciliationCycle::None.label(), "无需周期对账");
        assert_eq!(InvoiceType::VatSpecial.label(), "增值税专用发票");
    }
}
