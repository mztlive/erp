//! `cost_entry` 成本事实（数据模型 §6.10）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{CostEntryId, FileAssetId, SupplierAccountId};
use crate::money::{Amount, Rate};
use crate::validation::normalize_required_text;

/// 来源事实类型最大长度。
const FACT_TYPE_MAX_LEN: usize = 64;
/// 来源单据 ID 最大长度。
const DOCUMENT_ID_MAX_LEN: usize = 128;
/// 来源行 ID 最大长度。
const LINE_ID_MAX_LEN: usize = 128;
/// 来源版本最大长度。
const VERSION_MAX_LEN: usize = 64;

/// 成本类型（数据模型 §6.10：商品、物流、印刷、仓储、配送、平台技术、线下服务、
/// 返点、其他）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostType {
    /// 商品成本。
    Product,
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
    /// 返点。
    Rebate,
    /// 其他。
    Other,
}

impl CostType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Product => "商品",
            Self::Logistics => "物流",
            Self::Printing => "印刷",
            Self::Storage => "仓储",
            Self::Delivery => "配送",
            Self::PlatformTech => "平台技术",
            Self::OfflineService => "线下服务",
            Self::Rebate => "返点",
            Self::Other => "其他",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Product => "product",
            Self::Logistics => "logistics",
            Self::Printing => "printing",
            Self::Storage => "storage",
            Self::Delivery => "delivery",
            Self::PlatformTech => "platform_tech",
            Self::OfflineService => "offline_service",
            Self::Rebate => "rebate",
            Self::Other => "other",
        }
    }
}

/// 成本阶段（数据模型 §6.10：`EXPECTED`、`CONFIRMED`、`ACTUAL`、`REDUCTION`）。
///
/// 同一采购成本从预计、确认到实际是不同阶段事实，不覆盖前阶段；后续更权威
/// 成本只追加相对当前累计成本的差额。实际利润只使用「实际发生」和「冲减」。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostStage {
    /// 预计成本（实物及服务销售提交冻结时）。
    Expected,
    /// 确认成本（采购财务审核通过时）。
    Confirmed,
    /// 实际成本（合格入库/直发/电子交付/线下服务确认或财务登记有凭证费用时）。
    Actual,
    /// 冲减（采购退货、供应商退款或经复核成本调整实际发生时）。
    Reduction,
}

impl CostStage {
    /// 返回阶段的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Expected => "预计",
            Self::Confirmed => "确认",
            Self::Actual => "实际",
            Self::Reduction => "冲减",
        }
    }

    /// 返回阶段的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Expected => "expected",
            Self::Confirmed => "confirmed",
            Self::Actual => "actual",
            Self::Reduction => "reduction",
        }
    }

    /// 判断是否进入一期实际盈亏。
    ///
    /// # 返回
    /// 阶段为 `Actual` 或 `Reduction` 时返回 `true`。
    pub fn is_profit_relevant(&self) -> bool {
        matches!(self, Self::Actual | Self::Reduction)
    }
}

/// 成本归属范围（数据模型 §6.10）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostScope {
    /// 非卡券履约。
    NonVoucherFulfillment,
    /// 卡券直接履约费用（一期只留费用台账，不进卡券实际盈亏）。
    CardDirectFulfillment,
    /// 二期商城消费。
    MallConsumption,
    /// 微信支付成本。
    WechatCost,
}

impl CostScope {
    /// 返回范围的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::NonVoucherFulfillment => "非卡券履约",
            Self::CardDirectFulfillment => "卡券直接履约",
            Self::MallConsumption => "商城消费",
            Self::WechatCost => "微信支付",
        }
    }

    /// 返回范围的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::NonVoucherFulfillment => "non_voucher_fulfillment",
            Self::CardDirectFulfillment => "card_direct_fulfillment",
            Self::MallConsumption => "mall_consumption",
            Self::WechatCost => "wechat_cost",
        }
    }
}

/// 二期消费成本取值基础（数据模型 §6.10：`ACTUAL`、`STANDARD`、`NONE`）。
///
/// `NONE` 不保存伪造的零成本金额，只在消费归集对象上标记无成本，因此
/// `cost_entry` 不允许 `Some(None)` 取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostBasis {
    /// 实际（商城订单快照含完整含税标识与进项税率的供货成本）。
    Actual,
    /// 标准（按消费发生时间精确匹配有效 `supplier_offering_revision`）。
    Standard,
    /// 无成本（不形成成本事实，仅在消费对象上标记）。
    None,
}

impl CostBasis {
    /// 返回基础的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Actual => "实际",
            Self::Standard => "标准",
            Self::None => "无成本",
        }
    }

    /// 返回基础的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Actual => "actual",
            Self::Standard => "standard",
            Self::None => "none",
        }
    }
}

/// 成本事实创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CostEntryData {
    /// 成本类型。
    pub cost_type: CostType,
    /// 成本阶段。
    pub cost_stage: CostStage,
    /// 成本归属范围。
    pub cost_scope: CostScope,
    /// 成本取值基础（二期商城消费必填；其他成本可空；`NONE` 不得入表）。
    pub cost_basis: Option<CostBasis>,
    /// 成本供应商（可空）。
    pub supplier_id: Option<SupplierAccountId>,
    /// 含税成本金额。
    pub gross_amount: Amount,
    /// 不含税成本金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 含税标识。
    pub tax_inclusion: bool,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 成本发生时间。
    pub occurred_at: Instant,
    /// 唯一来源：来源事实类型。
    pub source_fact_type: String,
    /// 唯一来源：来源单据 ID。
    pub source_document_id: String,
    /// 唯一来源：来源行 ID。
    pub source_line_id: String,
    /// 唯一来源：来源版本。
    pub source_version: String,
    /// 后续差额或冲减所调整的原成本。
    pub adjusts_cost_entry_id: Option<CostEntryId>,
    /// 凭证附件。
    pub evidence_attachment_id: Option<FileAssetId>,
}

/// 成本事实实体（正式事实，数据模型 §6.10）。
///
/// 金额三元组满足 gross = net + tax 恒等且均不得为负；
/// `(source_fact_type, source_document_id, source_line_id, source_version,
/// cost_stage, cost_type)` 业务幂等唯一由唯一索引保证；`NONE` 成本不保存伪造的
/// 零成本金额，只在消费对象上标记；`adjusts_cost_entry_id` 保证同一经济冲减
/// 只产生一次净冲减（§8.4 由 P3 事务校验）。过账后不可更新或删除。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct CostEntry {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 成本类型。
    pub cost_type: CostType,
    /// 成本阶段。
    pub cost_stage: CostStage,
    /// 成本归属范围。
    pub cost_scope: CostScope,
    /// 成本取值基础。
    pub cost_basis: Option<CostBasis>,
    /// 成本供应商。
    pub supplier_id: Option<SupplierAccountId>,
    /// 含税成本金额。
    pub gross_amount: Amount,
    /// 不含税成本金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 含税标识。
    pub tax_inclusion: bool,
    /// 进项税率。
    pub input_tax_rate: Rate,
    /// 成本发生时间。
    pub occurred_at: Instant,
    /// 来源事实类型。
    pub source_fact_type: String,
    /// 来源单据 ID。
    pub source_document_id: String,
    /// 来源行 ID。
    pub source_line_id: String,
    /// 来源版本。
    pub source_version: String,
    /// 所调整的原成本。
    pub adjusts_cost_entry_id: Option<CostEntryId>,
    /// 凭证附件。
    pub evidence_attachment_id: Option<FileAssetId>,
}

impl CostEntry {
    /// 创建成本事实。
    ///
    /// 完成来源文本的 trim/非空/长度校验、金额恒等（gross = net + tax）与非负
    /// 校验、进项税率非负校验，并执行二期取值规则：商城消费范围必填
    /// `cost_basis`，且 `NONE` 不得作为成本事实入表。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::CostEntryId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的成本事实实体。
    ///
    /// # 错误
    /// 当来源字段为空/超长、金额恒等不成立或为负、税率为负、商城消费缺
    /// `cost_basis` 或使用 `NONE` 取值时返回错误。
    #[allow(clippy::too_many_arguments)]
    pub fn new(id: CostEntryId, data: CostEntryData) -> Result<Self> {
        validate_amounts(data.gross_amount, data.net_amount, data.tax_amount)?;
        if data.input_tax_rate.to_decimal().is_sign_negative() {
            return Err(Error::from("进项税率不得为负"));
        }
        validate_basis(data.cost_scope, data.cost_basis)?;
        let source_fact_type = normalize_required_text(
            data.source_fact_type,
            "来源事实类型不能为空",
            FACT_TYPE_MAX_LEN,
            "来源事实类型过长",
        )?;
        let source_document_id = normalize_required_text(
            data.source_document_id,
            "来源单据ID不能为空",
            DOCUMENT_ID_MAX_LEN,
            "来源单据ID过长",
        )?;
        let source_line_id = normalize_required_text(
            data.source_line_id,
            "来源行ID不能为空",
            LINE_ID_MAX_LEN,
            "来源行ID过长",
        )?;
        let source_version = normalize_required_text(
            data.source_version,
            "来源版本不能为空",
            VERSION_MAX_LEN,
            "来源版本过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            cost_type: data.cost_type,
            cost_stage: data.cost_stage,
            cost_scope: data.cost_scope,
            cost_basis: data.cost_basis,
            supplier_id: data.supplier_id,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
            tax_inclusion: data.tax_inclusion,
            input_tax_rate: data.input_tax_rate,
            occurred_at: data.occurred_at,
            source_fact_type,
            source_document_id,
            source_line_id,
            source_version,
            adjusts_cost_entry_id: data.adjusts_cost_entry_id,
            evidence_attachment_id: data.evidence_attachment_id,
        })
    }

    /// 更新成本事实。
    ///
    /// 正式事实过账后不可更新（数据模型 §4.5），任何字段的修改都被拒绝，
    /// 差额或冲减必须追加新阶段成本事实。
    ///
    /// # 参数
    /// * `update` - 更新数据（恒被拒绝）
    /// * `updated_by` - 更新执行人（恒被拒绝）
    ///
    /// # 返回
    /// 恒返回错误。
    pub fn update(&mut self, update: CostEntryData, updated_by: impl Into<String>) -> Result<()> {
        let _ = (update, updated_by);
        Err(Error::from("正式事实过账后不可更新，请追加差额成本事实"))
    }

    /// 判断成本事实是否进入一期实际盈亏。
    ///
    /// 规则（数据模型 §6.10）：一期「订单实际经营盈亏」只汇总
    /// `cost_scope = NON_VOUCHER_FULFILLMENT` 且 `cost_stage IN (ACTUAL,
    /// REDUCTION)` 的不含税净额。
    ///
    /// # 返回
    /// 满足一期实际盈亏口径时返回 `true`。
    pub fn is_phase1_profit_relevant(&self) -> bool {
        self.cost_scope == CostScope::NonVoucherFulfillment && self.cost_stage.is_profit_relevant()
    }
}

/// 校验成本金额三元组恒等且非负。
///
/// 规则（数据模型 §4.2）：含税金额必须精确等于不含税金额加税额。
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
        return Err(Error::from("成本金额不得为负"));
    }
    if gross != net.checked_add(tax) {
        return Err(Error::from("含税金额必须等于不含税金额加税额"));
    }
    Ok(())
}

/// 校验二期取值规则。
///
/// 规则（数据模型 §6.10）：商城消费范围必填 `cost_basis`；`NONE` 不保存伪造的
/// 零成本金额，只在消费归集对象上标记。
///
/// # 参数
/// * `scope` - 成本归属范围
/// * `basis` - 成本取值基础
///
/// # 返回
/// 合法返回 `Ok(())`。
///
/// # 错误
/// 商城消费缺 `cost_basis` 或 `cost_basis` 为 `NONE` 时返回错误。
fn validate_basis(scope: CostScope, basis: Option<CostBasis>) -> Result<()> {
    match basis {
        Some(CostBasis::None) => Err(Error::from("NONE 不保存伪造的零成本金额，只在消费归集对象上标记")),
        None if scope == CostScope::MallConsumption => Err(Error::from("商城消费成本必填取值基础")),
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::money::Amount;
    use std::str::FromStr;

    fn data() -> CostEntryData {
        CostEntryData {
            cost_type: CostType::Product,
            cost_stage: CostStage::Actual,
            cost_scope: CostScope::NonVoucherFulfillment,
            cost_basis: None,
            supplier_id: Some(SupplierAccountId::new("sup-1")),
            gross_amount: Amount::from_str("113.00").unwrap(),
            net_amount: Amount::from_str("100.00").unwrap(),
            tax_amount: Amount::from_str("13.00").unwrap(),
            tax_inclusion: true,
            input_tax_rate: Rate::from_str("0.130000").unwrap(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            source_fact_type: " PURCHASE_RECEIPT ".to_string(),
            source_document_id: " PR-1 ".to_string(),
            source_line_id: " PR-1-L1 ".to_string(),
            source_version: " v1 ".to_string(),
            adjusts_cost_entry_id: None,
            evidence_attachment_id: None,
        }
    }

    #[test]
    fn new_trims_and_normalizes_source_fields() {
        let entry = CostEntry::new(CostEntryId::new("ce-1"), data()).unwrap();

        assert_eq!(entry.source_fact_type, "PURCHASE_RECEIPT");
        assert_eq!(entry.source_document_id, "PR-1");
        assert_eq!(entry.source_line_id, "PR-1-L1");
        assert_eq!(entry.source_version, "v1");
        assert!(entry.is_phase1_profit_relevant());
    }

    #[test]
    fn new_rejects_blank_overlong_and_negative_amounts() {
        let blank = CostEntryData {
            source_line_id: "   ".to_string(),
            ..data()
        };
        assert!(CostEntry::new(CostEntryId::new("ce-2"), blank).is_err());

        let overlong = CostEntryData {
            source_version: "x".repeat(65),
            ..data()
        };
        assert!(CostEntry::new(CostEntryId::new("ce-3"), overlong).is_err());

        let mismatch = CostEntryData {
            net_amount: Amount::from_str("99.00").unwrap(),
            ..data()
        };
        assert!(CostEntry::new(CostEntryId::new("ce-4"), mismatch).is_err());

        let negative_tax = CostEntryData {
            input_tax_rate: Rate::from_str("-0.01").unwrap(),
            ..data()
        };
        assert!(CostEntry::new(CostEntryId::new("ce-5"), negative_tax).is_err());
    }

    #[test]
    fn new_enforces_phase2_basis_rules() {
        let none_basis = CostEntryData {
            cost_scope: CostScope::MallConsumption,
            cost_basis: Some(CostBasis::None),
            ..data()
        };
        assert!(
            CostEntry::new(CostEntryId::new("ce-6"), none_basis).is_err(),
            "NONE 不得形成成本事实"
        );

        let missing_basis = CostEntryData {
            cost_scope: CostScope::MallConsumption,
            cost_basis: None,
            ..data()
        };
        assert!(
            CostEntry::new(CostEntryId::new("ce-7"), missing_basis).is_err(),
            "商城消费必填取值基础"
        );

        let actual_basis = CostEntryData {
            cost_scope: CostScope::MallConsumption,
            cost_basis: Some(CostBasis::Actual),
            ..data()
        };
        assert!(CostEntry::new(CostEntryId::new("ce-8"), actual_basis).is_ok());
    }

    #[test]
    fn phase1_profit_relevance_only_for_non_voucher_actual_reduction() {
        let not_actual = CostEntryData {
            cost_stage: CostStage::Confirmed,
            ..data()
        };
        let entry = CostEntry::new(CostEntryId::new("ce-9"), not_actual).unwrap();
        assert!(!entry.is_phase1_profit_relevant(), "CONFIRMED 不进入一期实际盈亏");

        let card_scope = CostEntryData {
            cost_scope: CostScope::CardDirectFulfillment,
            ..data()
        };
        let entry = CostEntry::new(CostEntryId::new("ce-10"), card_scope).unwrap();
        assert!(
            !entry.is_phase1_profit_relevant(),
            "卡券直接履约费用不进卡券实际盈亏"
        );

        assert!(CostStage::Actual.is_profit_relevant());
        assert!(CostStage::Reduction.is_profit_relevant());
        assert!(!CostStage::Expected.is_profit_relevant());
    }

    #[test]
    fn formal_fact_rejects_update() {
        let mut entry = CostEntry::new(CostEntryId::new("ce-1"), data()).unwrap();
        assert!(entry.update(data(), "admin-2").is_err());
    }

    #[test]
    fn enums_serialize_with_stable_codes_and_labels() {
        assert_eq!(
            serde_json::to_string(&CostType::PlatformTech).unwrap(),
            "\"platform_tech\""
        );
        assert_eq!(
            serde_json::to_string(&CostStage::Reduction).unwrap(),
            "\"reduction\""
        );
        assert_eq!(
            serde_json::to_string(&CostScope::WechatCost).unwrap(),
            "\"wechat_cost\""
        );
        assert_eq!(
            serde_json::to_string(&CostBasis::Standard).unwrap(),
            "\"standard\""
        );
        assert_eq!(CostType::OfflineService.label(), "线下服务");
        assert_eq!(CostStage::Confirmed.label(), "确认");
        assert_eq!(CostScope::NonVoucherFulfillment.label(), "非卡券履约");
        assert_eq!(CostBasis::None.as_str(), "none");
    }
}
