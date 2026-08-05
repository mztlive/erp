//! `mall_order`：商城订单追溯对象（数据模型 §6.17）。
//!
//! 本表是关键事实形成的追溯对象，不是商城可变订单状态副本。`payment_fact_id`
//! 非空且唯一是「一单一份有效支付事实」的数据库落点（唯一索引由 P2 落实）；
//! 后到的不同支付版本保存为差异，不得创建第二份 `mall_order`（P3 条目：§6.17
//! 唯一支付事实归集）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::ensure_transition;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{CustomerAccountId, MallOrderFactId, MallOrderId};
use crate::mall_order::types::{AttributionStatus, FulfillmentChain};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 目标商城代码最大长度。
const MALL_ID_MAX_LEN: usize = 64;
/// 商城订单号最大长度。
const ORDER_NO_MAX_LEN: usize = 128;
/// 商城用户稳定标识最大长度。
const MALL_USER_REF_MAX_LEN: usize = 128;
/// 来源客户标识最大长度。
const CUSTOMER_REF_MAX_LEN: usize = 128;
/// 加密履约地址快照最大长度。
const ADDRESS_SNAPSHOT_MAX_LEN: usize = 8192;

/// 商城订单创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallOrderData {
    /// 商城订单身份。
    pub mall_id: String,
    /// 商城订单号。
    pub external_order_no: String,
    /// 原支付成功事实。
    pub payment_fact_id: MallOrderFactId,
    /// 商城用户稳定标识。
    pub mall_user_ref: String,
    /// 来源客户标识，可空。
    pub source_customer_ref: Option<String>,
    /// 映射后的企业客户；待归集时可为空。
    pub customer_id: Option<CustomerAccountId>,
    /// 下单时间。
    pub ordered_at: Instant,
    /// 支付成功时间。
    pub paid_at: Instant,
    /// 原价快照。
    pub gross_amount: Amount,
    /// 优惠快照。
    pub discount_amount: Amount,
    /// 运费快照。
    pub freight_amount: Amount,
    /// 实付快照。
    pub paid_amount: Amount,
    /// 履约链归属。
    pub fulfillment_chain: FulfillmentChain,
    /// 归集进度状态。
    pub attribution_status: AttributionStatus,
    /// 供应商履约所需地址快照（加密）。
    pub address_snapshot_encrypted: Option<String>,
}

/// 商城订单实体（数据模型 §6.17）。
///
/// 下单时商品、价格、供给和成本快照不可被后续基础资料变化覆盖；订单金额只随
/// 首份有效支付事实创建时落库，后续只允许推进归集进度或补充客户归属。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallOrder {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 商城订单身份。
    pub mall_id: String,
    /// 商城订单号。
    pub external_order_no: String,
    /// 原支付成功事实。
    pub payment_fact_id: MallOrderFactId,
    /// 商城用户稳定标识。
    pub mall_user_ref: String,
    /// 来源客户标识。
    pub source_customer_ref: Option<String>,
    /// 映射后的企业客户。
    pub customer_id: Option<CustomerAccountId>,
    /// 下单时间。
    pub ordered_at: Instant,
    /// 支付成功时间。
    pub paid_at: Instant,
    /// 原价快照。
    pub gross_amount: Amount,
    /// 优惠快照。
    pub discount_amount: Amount,
    /// 运费快照。
    pub freight_amount: Amount,
    /// 实付快照。
    pub paid_amount: Amount,
    /// 履约链归属。
    pub fulfillment_chain: FulfillmentChain,
    /// 归集进度状态。
    pub attribution_status: AttributionStatus,
    /// 供应商履约所需地址快照（加密）。
    pub address_snapshot_encrypted: Option<String>,
}

impl MallOrder {
    /// 创建商城订单。
    ///
    /// 完成文本字段校验与规范化，并强制金额恒等（§6.17）：
    /// `paid_amount = gross_amount - discount_amount + freight_amount`，
    /// 各金额均不得为负；`paid_at` 不得早于 `ordered_at`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallOrderId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的商城订单实体。
    ///
    /// # 错误
    /// 当文本为空/超长、金额恒等不成立、金额为负或支付时间早于下单时间时返回错误。
    pub fn new(id: MallOrderId, data: MallOrderData) -> Result<Self> {
        let mall_id = normalize_required_text(
            data.mall_id,
            "来源商城不能为空",
            MALL_ID_MAX_LEN,
            "来源商城代码过长",
        )?;
        let external_order_no = normalize_required_text(
            data.external_order_no,
            "商城订单号不能为空",
            ORDER_NO_MAX_LEN,
            "商城订单号过长",
        )?;
        let mall_user_ref = normalize_required_text(
            data.mall_user_ref,
            "商城用户标识不能为空",
            MALL_USER_REF_MAX_LEN,
            "商城用户标识过长",
        )?;
        let source_customer_ref =
            normalize_optional_text(data.source_customer_ref, "来源客户标识", CUSTOMER_REF_MAX_LEN)?;
        let address_snapshot_encrypted = normalize_optional_text(
            data.address_snapshot_encrypted,
            "加密履约地址快照",
            ADDRESS_SNAPSHOT_MAX_LEN,
        )?;
        validate_order_amounts(
            data.gross_amount,
            data.discount_amount,
            data.freight_amount,
            data.paid_amount,
        )?;
        if data.paid_at < data.ordered_at {
            return Err(Error::from("支付成功时间不得早于下单时间"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_id,
            external_order_no,
            payment_fact_id: data.payment_fact_id,
            mall_user_ref,
            source_customer_ref,
            customer_id: data.customer_id,
            ordered_at: data.ordered_at,
            paid_at: data.paid_at,
            gross_amount: data.gross_amount,
            discount_amount: data.discount_amount,
            freight_amount: data.freight_amount,
            paid_amount: data.paid_amount,
            fulfillment_chain: data.fulfillment_chain,
            attribution_status: data.attribution_status,
            address_snapshot_encrypted,
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

    /// 绑定映射后的企业客户。
    ///
    /// 归集完成时写入映射结果；待归集时保持空（§6.17）。允许重复赋值，
    /// 由上层保证客户归属一致性。
    ///
    /// # 参数
    /// * `customer_id` - 映射后的企业客户；`None` 表示清除
    ///
    /// # 返回
    /// 返回 `Ok(())`。
    pub fn assign_customer(&mut self, customer_id: Option<CustomerAccountId>) {
        self.customer_id = customer_id;
    }
}

/// 校验订单金额恒等与非负。
///
/// 恒等式（§6.17）：`paid_amount = gross_amount - discount_amount + freight_amount`。
///
/// # 参数
/// * `gross_amount` - 原价
/// * `discount_amount` - 优惠
/// * `freight_amount` - 运费
/// * `paid_amount` - 实付
///
/// # 返回
/// 恒等成立且各金额非负返回 `Ok(())`。
///
/// # 错误
/// 任一金额为负或恒等不成立时返回错误。
fn validate_order_amounts(
    gross_amount: Amount,
    discount_amount: Amount,
    freight_amount: Amount,
    paid_amount: Amount,
) -> Result<()> {
    for (amount, label) in [
        (gross_amount, "原价"),
        (discount_amount, "优惠"),
        (freight_amount, "运费"),
        (paid_amount, "实付"),
    ] {
        if amount.to_decimal().is_sign_negative() {
            return Err(Error::from(format!("{label}金额不能为负")));
        }
    }
    let expected_paid =
        gross_amount.to_decimal() - discount_amount.to_decimal() + freight_amount.to_decimal();
    if paid_amount.to_decimal() != expected_paid {
        return Err(Error::from("实付金额必须等于原价减优惠加运费"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{MallOrder, MallOrderData};
    use crate::common::state::{ensure_transition, DocumentState};
    use crate::common::time::Instant;
    use crate::ids::{CustomerAccountId, MallOrderFactId, MallOrderId};
    use crate::mall_order::types::{AttributionStatus, FulfillmentChain};
    use crate::money::Amount;
    use std::str::FromStr;

    fn data() -> MallOrderData {
        MallOrderData {
            mall_id: " mall-a ".to_string(),
            external_order_no: " SO-1 ".to_string(),
            payment_fact_id: MallOrderFactId::new("fact-1"),
            mall_user_ref: " user-9 ".to_string(),
            source_customer_ref: Some(" cust-9 ".to_string()),
            customer_id: None,
            ordered_at: Instant::from_unix_secs(1_699_999_900),
            paid_at: Instant::from_unix_secs(1_700_000_000),
            gross_amount: Amount::from_str("100.00").unwrap(),
            discount_amount: Amount::from_str("10.00").unwrap(),
            freight_amount: Amount::from_str("5.00").unwrap(),
            paid_amount: Amount::from_str("95.00").unwrap(),
            fulfillment_chain: FulfillmentChain::ErpAutomated,
            attribution_status: AttributionStatus::PendingAttribution,
            address_snapshot_encrypted: Some(" <encrypted> ".to_string()),
        }
    }

    /// happy path：文本规范化与金额快照落库。
    #[test]
    fn new_trims_fields_and_keeps_amount_snapshots() {
        let order = MallOrder::new(MallOrderId::new("order-1"), data()).unwrap();

        assert_eq!(order.mall_id, "mall-a");
        assert_eq!(order.external_order_no, "SO-1");
        assert_eq!(order.mall_user_ref, "user-9");
        assert_eq!(order.source_customer_ref.as_deref(), Some("cust-9"));
        assert_eq!(order.paid_amount, Amount::from_str("95.00").unwrap());
        assert_eq!(order.address_snapshot_encrypted.as_deref(), Some("<encrypted>"));
        assert_eq!(order.fulfillment_chain, FulfillmentChain::ErpAutomated);
    }

    /// 失败路径：必填空、超长、金额恒等不成立、负金额、时间倒挂。
    #[test]
    fn new_rejects_blank_overlong_broken_identity_and_inverted_time() {
        let blank = MallOrderData {
            external_order_no: "  ".to_string(),
            ..data()
        };
        assert!(MallOrder::new(MallOrderId::new("o2"), blank).is_err());

        let overlong = MallOrderData {
            mall_user_ref: "u".repeat(129),
            ..data()
        };
        assert!(MallOrder::new(MallOrderId::new("o3"), overlong).is_err());

        let broken = MallOrderData {
            paid_amount: Amount::from_str("94.99").unwrap(),
            ..data()
        };
        assert!(MallOrder::new(MallOrderId::new("o4"), broken).is_err());

        let negative_freight = MallOrderData {
            freight_amount: Amount::from_str("-1.00").unwrap(),
            ..data()
        };
        assert!(MallOrder::new(MallOrderId::new("o5"), negative_freight).is_err());

        let inverted = MallOrderData {
            ordered_at: Instant::from_unix_secs(1_700_000_100),
            ..data()
        };
        assert!(MallOrder::new(MallOrderId::new("o6"), inverted).is_err());
    }

    /// 金额：paid = gross - discount + freight 恒等用例（含零优惠/运费）。
    #[test]
    fn amount_identity_holds_across_cases() {
        let no_discount = MallOrderData {
            discount_amount: Amount::from_str("0.00").unwrap(),
            paid_amount: Amount::from_str("105.00").unwrap(),
            ..data()
        };
        assert!(MallOrder::new(MallOrderId::new("o7"), no_discount).is_ok());

        let zero_freight = MallOrderData {
            freight_amount: Amount::from_str("0.00").unwrap(),
            paid_amount: Amount::from_str("90.00").unwrap(),
            ..data()
        };
        assert!(MallOrder::new(MallOrderId::new("o8"), zero_freight).is_ok());
    }

    /// 归集状态状态机：合法/非法迁移与终态定向断言。
    #[test]
    fn attribution_status_machine_directed_edges() {
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
        assert!(ensure_transition(AttributionStatus::Attributed, AttributionStatus::Difference).is_err());
        assert!(ensure_transition(
            AttributionStatus::Attributed,
            AttributionStatus::PendingAttribution
        )
        .is_err());
        assert_eq!(
            AttributionStatus::Attributed.allowed_next(),
            &[] as &[AttributionStatus]
        );
    }

    /// 归集推进与客户绑定：沿固定邻接推进并写入映射客户。
    #[test]
    fn attribution_advances_and_customer_assignment_works() {
        let mut order = MallOrder::new(MallOrderId::new("order-1"), data()).unwrap();

        order
            .update_attribution_status(AttributionStatus::Difference)
            .unwrap();
        order
            .update_attribution_status(AttributionStatus::PendingAttribution)
            .unwrap();
        order
            .update_attribution_status(AttributionStatus::Attributed)
            .unwrap();
        assert!(order
            .update_attribution_status(AttributionStatus::Difference)
            .is_err());

        order.assign_customer(Some(CustomerAccountId::new("cust-erp-1")));
        assert_eq!(order.customer_id, Some(CustomerAccountId::new("cust-erp-1")));
    }
}
