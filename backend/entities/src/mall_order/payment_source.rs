//! `mall_payment_source`：商城订单支付来源（数据模型 §6.17）。
//!
//! 支付来源类型只允许 `CARD` 或 `WECHAT`，不允许第三种「福利账户」来源（§6.17、
//! `erp-phase-2.md` §9.1）。`CARD` 必须携带稳定卡实例引用（`source_card_instance_ref`，
//! 不可反推卡号、卡密的稳定引用），成功归集后必须补充 `mall_card_instance_id`；
//! `WECHAT` 只能有微信支付引用。支付来源金额合计等于订单实付金额依赖聚合查询，
//! 由 P3 落实（P3 条目：§6.17 支付来源合计守恒）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::ensure_transition;
use crate::errors::{Error, Result};
use crate::ids::{MallCardInstanceId, MallOrderId, MallPaymentSourceId};
use crate::mall_order::types::AttributionStatus;
use crate::money::Amount;
use crate::validation::normalize_required_text;

/// 卡实例稳定引用最大长度。
const CARD_REF_MAX_LEN: usize = 256;
/// 微信支付引用最大长度。
const WECHAT_REF_MAX_LEN: usize = 256;

/// 支付来源类型（数据模型 §6.17：仅 `CARD` 或 `WECHAT`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PaymentSourceType {
    /// 卡券支付来源。
    Card,
    /// 微信支付来源。
    Wechat,
}

impl PaymentSourceType {
    /// 返回来源类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Card => "卡券",
            Self::Wechat => "微信支付",
        }
    }

    /// 返回来源类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Card => "CARD",
            Self::Wechat => "WECHAT",
        }
    }
}

/// 支付来源创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallPaymentSourceData {
    /// 商城订单。
    pub mall_order_id: MallOrderId,
    /// 单内支付来源序号（从 1 起）。
    pub source_no: u32,
    /// 来源类型。
    pub source_type: PaymentSourceType,
    /// 实际支付金额。
    pub amount: Amount,
    /// 卡券支付必填的来源稳定引用。
    pub source_card_instance_ref: Option<String>,
    /// 映射后的卡实例；事实先落库而基线暂缺时可空。
    pub mall_card_instance_id: Option<MallCardInstanceId>,
    /// 微信支付引用，卡券支付为空。
    pub wechat_payment_ref: Option<String>,
    /// 归集进度状态。
    pub attribution_status: AttributionStatus,
}

/// 支付来源实体（数据模型 §6.17）。
///
/// 来源引用字段按来源类型强制互斥（§6.17）：`CARD` 必须有
/// `source_card_instance_ref`，`WECHAT` 只能有 `wechat_payment_ref`；
/// `WECHAT` 不得携带卡实例。实体字段不含卡号、卡密（§4.5.6），只用稳定引用。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallPaymentSource {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 商城订单。
    pub mall_order_id: MallOrderId,
    /// 单内支付来源序号。
    pub source_no: u32,
    /// 来源类型。
    pub source_type: PaymentSourceType,
    /// 实际支付金额。
    pub amount: Amount,
    /// 卡券支付来源的稳定引用。
    pub source_card_instance_ref: Option<String>,
    /// 映射后的卡实例。
    pub mall_card_instance_id: Option<MallCardInstanceId>,
    /// 微信支付引用。
    pub wechat_payment_ref: Option<String>,
    /// 归集进度状态。
    pub attribution_status: AttributionStatus,
}

impl MallPaymentSource {
    /// 创建支付来源。
    ///
    /// 完成引用字段校验与规范化，并按来源类型强制引用完整性（§6.17）：
    /// `CARD` 必填 `source_card_instance_ref` 且不得携带微信引用；
    /// `WECHAT` 必填 `wechat_payment_ref` 且不得携带卡引用与卡实例。
    /// `source_no` 从 1 起；支付金额必须大于零。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallPaymentSourceId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的支付来源实体。
    ///
    /// # 错误
    /// 当序号为 0、金额非正、引用与来源类型不一致或必填引用为空/超长时返回错误。
    pub fn new(id: MallPaymentSourceId, data: MallPaymentSourceData) -> Result<Self> {
        if data.source_no == 0 {
            return Err(Error::from("支付来源序号必须从 1 开始"));
        }
        if data.amount.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("支付来源金额必须大于零"));
        }
        validate_source_refs_exclusivity(data.source_type, &data)?;
        let source_card_instance_ref = match data.source_type {
            PaymentSourceType::Card => Some(normalize_required_text(
                data.source_card_instance_ref.unwrap_or_default(),
                "卡券来源稳定引用不能为空",
                CARD_REF_MAX_LEN,
                "卡券来源稳定引用过长",
            )?),
            PaymentSourceType::Wechat => None,
        };
        let wechat_payment_ref = match data.source_type {
            PaymentSourceType::Card => None,
            PaymentSourceType::Wechat => Some(normalize_required_text(
                data.wechat_payment_ref.unwrap_or_default(),
                "微信支付引用不能为空",
                WECHAT_REF_MAX_LEN,
                "微信支付引用过长",
            )?),
        };

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_order_id: data.mall_order_id,
            source_no: data.source_no,
            source_type: data.source_type,
            amount: data.amount,
            source_card_instance_ref,
            mall_card_instance_id: data.mall_card_instance_id,
            wechat_payment_ref,
            attribution_status: data.attribution_status,
        })
    }

    /// 推进归集进度状态。
    ///
    /// 固定邻接（§6.17）：待归集 → 已归集 | 差异；差异 → 待归集；已归集为终态。
    ///
    /// # 参数
    /// * `to` - 目标归集状态
    ///
    /// # 返回
    /// 迁移合法返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标不在后继列表中且与当前状态不同时返回 `InvalidStateTransition`。
    pub fn update_attribution_status(&mut self, to: AttributionStatus) -> Result<()> {
        ensure_transition(self.attribution_status, to)?;
        self.attribution_status = to;
        Ok(())
    }

    /// 绑定归集后的卡实例。
    ///
    /// 卡券来源成功归集后必须有 `mall_card_instance_id`（§6.17）；卡实例基线暂缺
    /// 时保留来源引用并生成差异，补齐后使用原事实归集。仅 `CARD` 来源允许绑定。
    ///
    /// # 参数
    /// * `mall_card_instance_id` - 映射后的卡实例
    ///
    /// # 返回
    /// 绑定成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 微信支付来源不允许绑定卡实例时返回错误。
    pub fn assign_card_instance(&mut self, mall_card_instance_id: MallCardInstanceId) -> Result<()> {
        if self.source_type != PaymentSourceType::Card {
            return Err(Error::from("微信支付来源不得绑定卡实例"));
        }
        self.mall_card_instance_id = Some(mall_card_instance_id);
        Ok(())
    }
}

/// 校验来源类型与引用字段的互斥关系（§6.17）。
///
/// # 参数
/// * `source_type` - 来源类型
/// * `data` - 创建数据
///
/// # 返回
/// 互斥关系成立返回 `Ok(())`。
///
/// # 错误
/// `CARD` 携带微信引用，或 `WECHAT` 携带卡券引用/卡实例时返回错误。
fn validate_source_refs_exclusivity(
    source_type: PaymentSourceType,
    data: &MallPaymentSourceData,
) -> Result<()> {
    match source_type {
        PaymentSourceType::Card => {
            if data.wechat_payment_ref.is_some() {
                return Err(Error::from("卡券支付来源不得携带微信支付引用"));
            }
        }
        PaymentSourceType::Wechat => {
            if data.source_card_instance_ref.is_some() {
                return Err(Error::from("微信支付来源不得携带卡券稳定引用"));
            }
            if data.mall_card_instance_id.is_some() {
                return Err(Error::from("微信支付来源不得携带卡实例"));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{MallPaymentSource, MallPaymentSourceData, PaymentSourceType};
    use crate::common::state::ensure_transition;
    use crate::ids::{MallCardInstanceId, MallOrderId, MallPaymentSourceId};
    use crate::mall_order::types::AttributionStatus;
    use crate::money::Amount;
    use std::str::FromStr;

    fn card_data() -> MallPaymentSourceData {
        MallPaymentSourceData {
            mall_order_id: MallOrderId::new("order-1"),
            source_no: 1,
            source_type: PaymentSourceType::Card,
            amount: Amount::from_str("80.00").unwrap(),
            source_card_instance_ref: Some(" ref-001 ".to_string()),
            mall_card_instance_id: None,
            wechat_payment_ref: None,
            attribution_status: AttributionStatus::PendingAttribution,
        }
    }

    fn wechat_data() -> MallPaymentSourceData {
        MallPaymentSourceData {
            source_type: PaymentSourceType::Wechat,
            source_card_instance_ref: None,
            mall_card_instance_id: None,
            wechat_payment_ref: Some(" wx-001 ".to_string()),
            ..card_data()
        }
    }

    /// happy path：引用规范化，类型与引用互斥关系正确。
    #[test]
    fn new_trims_refs_and_enforces_type_specific_refs() {
        let card = MallPaymentSource::new(MallPaymentSourceId::new("ps-1"), card_data()).unwrap();
        assert_eq!(card.source_card_instance_ref.as_deref(), Some("ref-001"));
        assert!(card.wechat_payment_ref.is_none());
        assert_eq!(card.source_type, PaymentSourceType::Card);

        let wechat = MallPaymentSource::new(MallPaymentSourceId::new("ps-2"), wechat_data()).unwrap();
        assert_eq!(wechat.wechat_payment_ref.as_deref(), Some("wx-001"));
        assert!(wechat.source_card_instance_ref.is_none());
        assert!(wechat.mall_card_instance_id.is_none());
    }

    /// 失败路径：序号越界、金额越界、引用缺失或互斥关系被破坏。
    #[test]
    fn new_rejects_zero_no_negative_amount_and_ref_mismatch() {
        let zero_no = MallPaymentSourceData {
            source_no: 0,
            ..card_data()
        };
        assert!(MallPaymentSource::new(MallPaymentSourceId::new("ps-3"), zero_no).is_err());

        let zero_amount = MallPaymentSourceData {
            amount: Amount::from_str("0.00").unwrap(),
            ..card_data()
        };
        assert!(MallPaymentSource::new(MallPaymentSourceId::new("ps-4"), zero_amount).is_err());

        let card_without_ref = MallPaymentSourceData {
            source_card_instance_ref: None,
            ..card_data()
        };
        assert!(MallPaymentSource::new(MallPaymentSourceId::new("ps-5"), card_without_ref).is_err());

        let wechat_with_card_ref = MallPaymentSourceData {
            source_card_instance_ref: Some("ref-001".to_string()),
            ..wechat_data()
        };
        assert!(MallPaymentSource::new(MallPaymentSourceId::new("ps-6"), wechat_with_card_ref).is_err());

        let card_with_wechat_ref = MallPaymentSourceData {
            wechat_payment_ref: Some("wx-001".to_string()),
            ..card_data()
        };
        assert!(MallPaymentSource::new(MallPaymentSourceId::new("ps-7"), card_with_wechat_ref).is_err());

        let wechat_with_instance = MallPaymentSourceData {
            mall_card_instance_id: Some(MallCardInstanceId::new("card-1")),
            ..wechat_data()
        };
        assert!(MallPaymentSource::new(MallPaymentSourceId::new("ps-8"), wechat_with_instance).is_err());
    }

    /// 归集推进与卡实例绑定：已归集可绑定；微信来源拒绝绑定。
    #[test]
    fn attribution_advances_and_card_instance_binding() {
        let mut card = MallPaymentSource::new(MallPaymentSourceId::new("ps-9"), card_data()).unwrap();
        card.update_attribution_status(AttributionStatus::Attributed)
            .unwrap();
        assert!(card
            .update_attribution_status(AttributionStatus::Difference)
            .is_err());

        card.assign_card_instance(MallCardInstanceId::new("card-1"))
            .unwrap();
        assert_eq!(
            card.mall_card_instance_id,
            Some(MallCardInstanceId::new("card-1"))
        );

        let mut wechat = MallPaymentSource::new(MallPaymentSourceId::new("ps-10"), wechat_data()).unwrap();
        assert!(wechat
            .assign_card_instance(MallCardInstanceId::new("card-1"))
            .is_err());
        assert!(wechat.mall_card_instance_id.is_none());
    }

    /// 归集状态状态机：合法/非法迁移（复用 §6.17 固定邻接）。
    #[test]
    fn attribution_machine_directed_edges() {
        assert!(ensure_transition(
            AttributionStatus::PendingAttribution,
            AttributionStatus::Attributed
        )
        .is_ok());
        assert!(ensure_transition(
            AttributionStatus::PendingAttribution,
            AttributionStatus::Difference
        )
        .is_ok());
        assert!(ensure_transition(
            AttributionStatus::Difference,
            AttributionStatus::PendingAttribution
        )
        .is_ok());
        assert!(ensure_transition(AttributionStatus::Difference, AttributionStatus::Attributed).is_err());
        assert!(ensure_transition(AttributionStatus::Attributed, AttributionStatus::Difference).is_err());
    }

    /// 敏感字段（§4.5.6）：支付来源不含卡号、卡密、手机号字段，只保留稳定引用。
    #[test]
    fn entity_does_not_hold_forbidden_card_fields() {
        let card = MallPaymentSource::new(MallPaymentSourceId::new("ps-1"), card_data()).unwrap();
        let value = serde_json::to_value(&card).unwrap();
        let keys: Vec<&str> = value.as_object().unwrap().keys().map(String::as_str).collect();
        assert!(
            keys.contains(&"source_card_instance_ref"),
            "保留文档定义的稳定引用字段"
        );
        let forbidden = [
            "card_no",
            "card_number",
            "card_secret",
            "card_password",
            "phone",
            "mobile",
            "bound_phone",
        ];
        for key in forbidden {
            assert!(!keys.contains(&key), "支付来源不得包含字段 {key}");
        }
    }
}
