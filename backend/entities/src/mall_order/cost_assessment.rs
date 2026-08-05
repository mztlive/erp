//! `mall_consumption_cost_assessment`：消费成本评估链（数据模型 §6.17）。
//!
//! 消费金额事实与成本判断分离：每次取得更可靠成本来源时追加一个评估，不修改原消费，
//! 也不把成本字段塞回支付分摊矩阵。评估链不变式（§6.17）：`assessment_no = 1` 时
//! `supersedes_assessment_id` 必须为空，后续评估在锁定当前链尾后递增一号并引用链尾，
//! 禁止多根或分叉（链尾锁定与同消费唯一由 P2/P3 落实）。
//!
//! `delta_cost_entry_id` 非空必须对应「相对上一评估差额非零」且差额为零不得制造零金额
//! 成本——差额计算依赖上一评估链尾金额（跨行聚合），由 P3 落实（P3 条目：§6.17
//! 评估差额成本事实）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{CostEntryId, MallConsumptionCostAssessmentId, MallConsumptionEntryId};
use crate::mall_order::types::CostBasis;
use crate::money::{Amount, Rate};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 评估依据来源类型（数据模型 §6.17：商城成本快照、供给修订、供应商履约、结算或人工复核）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CostBasisSourceType {
    /// 商城成本快照。
    MallCostSnapshot,
    /// 供应商供给修订。
    SupplierOfferingRevision,
    /// 供应商履约。
    SupplierFulfillment,
    /// 供应商结算。
    Settlement,
    /// 人工复核依据。
    ManualReview,
}

impl CostBasisSourceType {
    /// 返回依据来源类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::MallCostSnapshot => "商城成本快照",
            Self::SupplierOfferingRevision => "供给修订",
            Self::SupplierFulfillment => "供应商履约",
            Self::Settlement => "供应商结算",
            Self::ManualReview => "人工复核",
        }
    }

    /// 返回依据来源类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::MallCostSnapshot => "mall_cost_snapshot",
            Self::SupplierOfferingRevision => "supplier_offering_revision",
            Self::SupplierFulfillment => "supplier_fulfillment",
            Self::Settlement => "settlement",
            Self::ManualReview => "manual_review",
        }
    }
}

/// 评估来源引用最大长度。
const BASIS_SOURCE_REF_MAX_LEN: usize = 256;
/// 来源内容指纹最大长度。
const SOURCE_HASH_MAX_LEN: usize = 128;
/// 评估人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 成本评估创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallConsumptionCostAssessmentData {
    /// 消费来源明细。
    pub mall_consumption_entry_id: MallConsumptionEntryId,
    /// 同消费递增评估号（从 1 起）。
    pub assessment_no: u32,
    /// 成本口径。
    pub cost_basis: CostBasis,
    /// 评估依据来源类型。
    pub basis_source_type: Option<CostBasisSourceType>,
    /// 评估依据来源对象 ID。
    pub basis_source_id: Option<String>,
    /// 评估依据来源行 ID。
    pub basis_source_line_id: Option<String>,
    /// 评估依据来源版本。
    pub basis_source_version: Option<String>,
    /// 本次成本依据的不可变内容指纹。
    pub source_snapshot_hash: Option<String>,
    /// 本次评估得到的累计成本金额。
    pub gross_amount: Option<Amount>,
    /// 累计成本净额。
    pub net_amount: Option<Amount>,
    /// 累计成本税额。
    pub tax_amount: Option<Amount>,
    /// 含税口径。
    pub tax_inclusion: Option<bool>,
    /// 进项税率。
    pub input_tax_rate: Option<Rate>,
    /// 相对上一评估形成的差额成本事实；`NONE` 时为空。
    pub delta_cost_entry_id: Option<CostEntryId>,
    /// 被本次更权威评估替代的上一评估，可空。
    pub supersedes_assessment_id: Option<MallConsumptionCostAssessmentId>,
    /// 评估时间。
    pub assessed_at: Instant,
    /// 评估的系统或复核人。
    pub assessed_by: String,
}

/// 消费成本评估实体（数据模型 §6.17）。
///
/// 评估不可变，只提供 `new()`。当前评估由「未被后续评估引用」的链尾派生，
/// 不维护可覆盖的 `current_flag`。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallConsumptionCostAssessment {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 消费来源明细。
    pub mall_consumption_entry_id: MallConsumptionEntryId,
    /// 同消费递增评估号。
    pub assessment_no: u32,
    /// 成本口径。
    pub cost_basis: CostBasis,
    /// 评估依据来源类型。
    pub basis_source_type: Option<CostBasisSourceType>,
    /// 评估依据来源对象 ID。
    pub basis_source_id: Option<String>,
    /// 评估依据来源行 ID。
    pub basis_source_line_id: Option<String>,
    /// 评估依据来源版本。
    pub basis_source_version: Option<String>,
    /// 本次成本依据的不可变内容指纹。
    pub source_snapshot_hash: Option<String>,
    /// 本次评估得到的累计成本金额。
    pub gross_amount: Option<Amount>,
    /// 累计成本净额。
    pub net_amount: Option<Amount>,
    /// 累计成本税额。
    pub tax_amount: Option<Amount>,
    /// 含税口径。
    pub tax_inclusion: Option<bool>,
    /// 进项税率。
    pub input_tax_rate: Option<Rate>,
    /// 相对上一评估形成的差额成本事实。
    pub delta_cost_entry_id: Option<CostEntryId>,
    /// 被本次更权威评估替代的上一评估。
    pub supersedes_assessment_id: Option<MallConsumptionCostAssessmentId>,
    /// 评估时间。
    pub assessed_at: Instant,
    /// 评估的系统或复核人。
    pub assessed_by: String,
}

impl MallConsumptionCostAssessment {
    /// 创建成本评估。
    ///
    /// 强制链与内容不变式（§6.17）：
    /// - `assessment_no` 从 1 起；为 1 时 `supersedes_assessment_id` 必须为空，
    ///   大于 1 时必须引用上一评估；
    /// - `ACTUAL`/`STANDARD` 必须有完整来源指纹、金额与税口径，且
    ///   `gross_amount = net_amount + tax_amount` 精确成立；含税时进项税率必填，
    ///   不含税时不得携带进项税率；
    /// - `NONE` 的金额、税字段、来源指纹与 `delta_cost_entry_id` 均为空。
    ///
    /// `delta_cost_entry_id` 的差额非零规则依赖上一评估链尾，由 P3 落实。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallConsumptionCostAssessmentId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的成本评估实体。
    ///
    /// # 错误
    /// 当评估号非法、前后驱不一致、`ACTUAL`/`STANDARD` 内容不完整或金额恒等
    /// 不成立、`NONE` 携带金额/税字段时返回错误。
    pub fn new(id: MallConsumptionCostAssessmentId, data: MallConsumptionCostAssessmentData) -> Result<Self> {
        if data.assessment_no == 0 {
            return Err(Error::from("评估号必须从 1 开始"));
        }
        let has_predecessor = data.supersedes_assessment_id.is_some();
        if (data.assessment_no == 1) == has_predecessor {
            return Err(Error::from(
                "评估号为 1 时不得引用前驱，大于 1 时必须引用链尾前驱",
            ));
        }
        let content = validate_assessment_content(&data)?;
        let assessed_by = normalize_required_text(
            data.assessed_by,
            "评估人不能为空",
            ACTOR_MAX_LEN,
            "评估人标识过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_consumption_entry_id: data.mall_consumption_entry_id,
            assessment_no: data.assessment_no,
            cost_basis: data.cost_basis,
            basis_source_type: content.basis_source_type,
            basis_source_id: content.basis_source_id,
            basis_source_line_id: data.basis_source_line_id,
            basis_source_version: data.basis_source_version,
            source_snapshot_hash: content.source_snapshot_hash,
            gross_amount: content.gross_amount,
            net_amount: content.net_amount,
            tax_amount: content.tax_amount,
            tax_inclusion: data.tax_inclusion,
            input_tax_rate: data.input_tax_rate,
            delta_cost_entry_id: data.delta_cost_entry_id,
            supersedes_assessment_id: data.supersedes_assessment_id,
            assessed_at: data.assessed_at,
            assessed_by,
        })
    }
}

/// 按成本口径校验后的评估内容（来源依据、指纹与金额三元组）。
struct AssessmentContent {
    /// 依据来源类型。
    basis_source_type: Option<CostBasisSourceType>,
    /// 依据来源对象 ID。
    basis_source_id: Option<String>,
    /// 来源内容指纹。
    source_snapshot_hash: Option<String>,
    /// 累计成本金额。
    gross_amount: Option<Amount>,
    /// 累计成本净额。
    net_amount: Option<Amount>,
    /// 累计成本税额。
    tax_amount: Option<Amount>,
}

/// 按成本口径校验并规范化评估内容字段。
///
/// # 参数
/// * `data` - 评估创建数据
///
/// # 返回
/// 返回规范化后的评估内容（依据来源、指纹与金额三元组）。
///
/// # 错误
/// `ACTUAL`/`STANDARD` 内容不完整或恒等不成立、`NONE` 携带金额/税/依据字段时
/// 返回错误。
fn validate_assessment_content(data: &MallConsumptionCostAssessmentData) -> Result<AssessmentContent> {
    let basis_source_id = normalize_optional_text(
        data.basis_source_id.clone(),
        "评估依据来源对象ID",
        BASIS_SOURCE_REF_MAX_LEN,
    )?;
    let source_snapshot_hash = normalize_optional_text(
        data.source_snapshot_hash.clone(),
        "来源内容指纹",
        SOURCE_HASH_MAX_LEN,
    )?;

    match data.cost_basis {
        CostBasis::Actual | CostBasis::Standard => {
            let (Some(source_type), Some(source_id), Some(hash), Some(gross), Some(net), Some(tax)) = (
                data.basis_source_type,
                basis_source_id.clone(),
                source_snapshot_hash.clone(),
                data.gross_amount,
                data.net_amount,
                data.tax_amount,
            ) else {
                return Err(Error::from("ACTUAL/STANDARD 必须有完整来源指纹、金额和税口径"));
            };
            if gross.to_decimal() != net.to_decimal() + tax.to_decimal() {
                return Err(Error::from("成本金额必须满足 gross = net + tax"));
            }
            validate_tax_inclusion(data.tax_inclusion, data.input_tax_rate)?;
            Ok(AssessmentContent {
                basis_source_type: Some(source_type),
                basis_source_id: Some(source_id),
                source_snapshot_hash: Some(hash),
                gross_amount: Some(gross),
                net_amount: Some(net),
                tax_amount: Some(tax),
            })
        }
        CostBasis::None => {
            if data.basis_source_type.is_some()
                || basis_source_id.is_some()
                || data.basis_source_line_id.is_some()
                || data.basis_source_version.is_some()
                || source_snapshot_hash.is_some()
                || data.gross_amount.is_some()
                || data.net_amount.is_some()
                || data.tax_amount.is_some()
                || data.tax_inclusion.is_some()
                || data.input_tax_rate.is_some()
                || data.delta_cost_entry_id.is_some()
            {
                return Err(Error::from("NONE 评估不得携带金额、税字段、依据来源或差额成本"));
            }
            Ok(AssessmentContent {
                basis_source_type: None,
                basis_source_id: None,
                source_snapshot_hash: None,
                gross_amount: None,
                net_amount: None,
                tax_amount: None,
            })
        }
    }
}

/// 校验含税口径与进项税率一致性。
///
/// # 参数
/// * `tax_inclusion` - 含税口径
/// * `input_tax_rate` - 进项税率
///
/// # 返回
/// 含税时必有进项税率、不含税时不得携带时返回 `Ok(())`。
///
/// # 错误
/// 口径与税率不一致时返回错误。
fn validate_tax_inclusion(tax_inclusion: Option<bool>, input_tax_rate: Option<Rate>) -> Result<()> {
    match (tax_inclusion, input_tax_rate) {
        (Some(true), Some(_)) | (Some(false), None) => Ok(()),
        (Some(true), None) => Err(Error::from("成本含税时进项税率必填")),
        (Some(false), Some(_)) => Err(Error::from("成本不含税时不得携带进项税率")),
        (None, _) => Err(Error::from("ACTUAL/STANDARD 评估必须有含税口径")),
    }
}

#[cfg(test)]
mod tests {
    use super::{CostBasisSourceType, MallConsumptionCostAssessment, MallConsumptionCostAssessmentData};
    use crate::common::time::Instant;
    use crate::ids::{CostEntryId, MallConsumptionCostAssessmentId, MallConsumptionEntryId};
    use crate::mall_order::types::CostBasis;
    use crate::money::{Amount, Rate};
    use std::str::FromStr;

    fn actual_data() -> MallConsumptionCostAssessmentData {
        MallConsumptionCostAssessmentData {
            mall_consumption_entry_id: MallConsumptionEntryId::new("ce-1"),
            assessment_no: 1,
            cost_basis: CostBasis::Actual,
            basis_source_type: Some(CostBasisSourceType::MallCostSnapshot),
            basis_source_id: Some(" so-1 ".to_string()),
            basis_source_line_id: None,
            basis_source_version: Some(" v1 ".to_string()),
            source_snapshot_hash: Some(" 9f86d081 ".to_string()),
            gross_amount: Some(Amount::from_str("12.00").unwrap()),
            net_amount: Some(Amount::from_str("11.32").unwrap()),
            tax_amount: Some(Amount::from_str("0.68").unwrap()),
            tax_inclusion: Some(true),
            input_tax_rate: Some(Rate::from_str("0.060000").unwrap()),
            delta_cost_entry_id: None,
            supersedes_assessment_id: None,
            assessed_at: Instant::from_unix_secs(1_700_000_100),
            assessed_by: " cost-team ".to_string(),
        }
    }

    fn none_data() -> MallConsumptionCostAssessmentData {
        MallConsumptionCostAssessmentData {
            cost_basis: CostBasis::None,
            basis_source_type: None,
            basis_source_id: None,
            basis_source_line_id: None,
            basis_source_version: None,
            source_snapshot_hash: None,
            gross_amount: None,
            net_amount: None,
            tax_amount: None,
            tax_inclusion: None,
            input_tax_rate: None,
            delta_cost_entry_id: None,
            ..actual_data()
        }
    }

    /// happy path：ACTUAL 评估内容规范化、金额恒等、来源与税口径落库。
    #[test]
    fn actual_new_trims_and_keeps_tax_consistent_amounts() {
        let assessment =
            MallConsumptionCostAssessment::new(MallConsumptionCostAssessmentId::new("ca-1"), actual_data())
                .unwrap();

        assert_eq!(assessment.assessment_no, 1);
        assert_eq!(assessment.basis_source_id.as_deref(), Some("so-1"));
        assert_eq!(assessment.source_snapshot_hash.as_deref(), Some("9f86d081"));
        assert_eq!(assessment.gross_amount, Some(Amount::from_str("12.00").unwrap()));
        assert_eq!(assessment.net_amount, Some(Amount::from_str("11.32").unwrap()));
        assert_eq!(assessment.tax_amount, Some(Amount::from_str("0.68").unwrap()));
        assert_eq!(
            assessment.input_tax_rate,
            Some(Rate::from_str("0.060000").unwrap())
        );
    }

    /// 失败路径：评估号 0、前后驱不一致、ACTUAL 内容不完整、恒等不成立。
    #[test]
    fn actual_new_rejects_invalid_no_predecessor_and_incomplete_content() {
        let zero_no = MallConsumptionCostAssessmentData {
            assessment_no: 0,
            ..actual_data()
        };
        assert!(
            MallConsumptionCostAssessment::new(MallConsumptionCostAssessmentId::new("ca-2"), zero_no)
                .is_err()
        );

        let no1_with_predecessor = MallConsumptionCostAssessmentData {
            supersedes_assessment_id: Some(MallConsumptionCostAssessmentId::new("ca-0")),
            ..actual_data()
        };
        assert!(MallConsumptionCostAssessment::new(
            MallConsumptionCostAssessmentId::new("ca-3"),
            no1_with_predecessor,
        )
        .is_err());

        let no2_without_predecessor = MallConsumptionCostAssessmentData {
            assessment_no: 2,
            supersedes_assessment_id: None,
            ..actual_data()
        };
        assert!(MallConsumptionCostAssessment::new(
            MallConsumptionCostAssessmentId::new("ca-4"),
            no2_without_predecessor,
        )
        .is_err());

        let missing_hash = MallConsumptionCostAssessmentData {
            source_snapshot_hash: None,
            ..actual_data()
        };
        assert!(MallConsumptionCostAssessment::new(
            MallConsumptionCostAssessmentId::new("ca-5"),
            missing_hash,
        )
        .is_err());

        let broken_identity = MallConsumptionCostAssessmentData {
            net_amount: Some(Amount::from_str("11.31").unwrap()),
            ..actual_data()
        };
        assert!(MallConsumptionCostAssessment::new(
            MallConsumptionCostAssessmentId::new("ca-6"),
            broken_identity,
        )
        .is_err());
    }

    /// 金额与税口径：gross = net + tax 恒等；含税/不含税与税率互斥一致。
    #[test]
    fn amount_identity_and_tax_inclusion_consistency() {
        let exclusive = MallConsumptionCostAssessmentData {
            tax_inclusion: Some(false),
            input_tax_rate: None,
            ..actual_data()
        };
        assert!(
            MallConsumptionCostAssessment::new(MallConsumptionCostAssessmentId::new("ca-7"), exclusive,)
                .is_ok()
        );

        let exclusive_with_rate = MallConsumptionCostAssessmentData {
            tax_inclusion: Some(false),
            input_tax_rate: Some(Rate::from_str("0.060000").unwrap()),
            ..actual_data()
        };
        assert!(MallConsumptionCostAssessment::new(
            MallConsumptionCostAssessmentId::new("ca-8"),
            exclusive_with_rate,
        )
        .is_err());

        let inclusive_without_rate = MallConsumptionCostAssessmentData {
            tax_inclusion: Some(true),
            input_tax_rate: None,
            ..actual_data()
        };
        assert!(MallConsumptionCostAssessment::new(
            MallConsumptionCostAssessmentId::new("ca-9"),
            inclusive_without_rate,
        )
        .is_err());
    }

    /// NONE 评估：全部金额、税、依据与差额字段为空；携带任一字段即拒绝。
    #[test]
    fn none_assessment_rejects_any_content_field() {
        let assessment =
            MallConsumptionCostAssessment::new(MallConsumptionCostAssessmentId::new("ca-10"), none_data())
                .unwrap();
        assert_eq!(assessment.cost_basis, CostBasis::None);
        assert!(assessment.gross_amount.is_none());
        assert!(assessment.delta_cost_entry_id.is_none());

        let none_with_amount = MallConsumptionCostAssessmentData {
            gross_amount: Some(Amount::from_str("1.00").unwrap()),
            ..none_data()
        };
        assert!(MallConsumptionCostAssessment::new(
            MallConsumptionCostAssessmentId::new("ca-11"),
            none_with_amount,
        )
        .is_err());

        let none_with_delta = MallConsumptionCostAssessmentData {
            delta_cost_entry_id: Some(CostEntryId::new("cost-1")),
            ..none_data()
        };
        assert!(MallConsumptionCostAssessment::new(
            MallConsumptionCostAssessmentId::new("ca-12"),
            none_with_delta,
        )
        .is_err());
    }

    /// 失败路径：评估人必填/超长。
    #[test]
    fn new_rejects_blank_or_overlong_assessed_by() {
        let blank = MallConsumptionCostAssessmentData {
            assessed_by: "  ".to_string(),
            ..actual_data()
        };
        assert!(
            MallConsumptionCostAssessment::new(MallConsumptionCostAssessmentId::new("ca-13"), blank).is_err()
        );

        let overlong = MallConsumptionCostAssessmentData {
            assessed_by: "a".repeat(129),
            ..actual_data()
        };
        assert!(
            MallConsumptionCostAssessment::new(MallConsumptionCostAssessmentId::new("ca-14"), overlong)
                .is_err()
        );
    }
}
