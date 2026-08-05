//! `mall_refund`、`mall_refund_line` 与 `mall_refund_allocation`：退款成功事实头、
//! 商品退款行与原支付来源分配（数据模型 §6.18）。
//!
//! 退款头、行、初始 `APPLY` 分配和消费冲减在同一事务写入（§8.4 第 3 条）；
//! 过账后均不可更新或删除，错误分配只能追加 `REVERSE` 及等额正确 `APPLY`，
//! 不得改变商城成功退款总额。本文件实体均只提供 `new()`。
//!
//! 跨行不变式（§6.18，依赖聚合查询，由 P3 落实）：
//! - 退款行金额合计等于头金额；每行净 `APPLY − REVERSE` 分配合计等于该行退款金额；
//! - `REVERSE` 必须等额引用同退款行、同原消费和同支付来源的一个 `APPLY`；
//! - 同一原消费累计成功退款金额不得超过原消费金额；任一时点净
//!   `APPLY − REVERSE` 非负且不超过净可退余额。
//!
//! 本实体做单行不变式：分配金额非负、动作与引用字段完整性。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    MallAfterSalesRequestId, MallConsumptionEntryId, MallOrderFactId, MallOrderId, MallOrderItemId,
    MallPaymentSourceId, MallRefundAllocationId, MallRefundId, MallRefundLineId,
};
use crate::mall_after_sales::types::AllocationAction;
use crate::money::{Amount, Quantity};
use crate::validation::normalize_required_text;

/// 目标商城代码最大长度。
const MALL_ID_MAX_LEN: usize = 64;
/// 商城退款单号最大长度。
const REFUND_NO_MAX_LEN: usize = 128;
/// 退款版本最大长度。
const REFUND_VERSION_MAX_LEN: usize = 64;

/// 退款头创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallRefundData {
    /// `REFUND_SUCCEEDED` 事实。
    pub mall_order_fact_id: MallOrderFactId,
    /// 同一售后案件。
    pub after_sales_request_id: MallAfterSalesRequestId,
    /// 来源商城。
    pub mall_id: String,
    /// 商城退款身份。
    pub external_refund_no: String,
    /// 商城退款版本。
    pub external_refund_version: String,
    /// 原订单。
    pub mall_order_id: MallOrderId,
    /// 实际成功退款金额。
    pub refund_amount: Amount,
    /// 实际退款时间。
    pub refunded_at: Instant,
}

/// 退款成功事实头实体（数据模型 §6.18）。
///
/// 事实类型必须为 `REFUND_SUCCEEDED`（与所引 `mall_order_fact.fact_type` 一致，
/// 跨实体校验由 P3 落实）；`mall_order_fact_id` 非空且唯一由 P2 唯一索引落实。
/// 不可变，只提供 `new()`。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallRefund {
    #[serde(flatten)]
    pub base: BaseModel,
    /// `REFUND_SUCCEEDED` 事实。
    pub mall_order_fact_id: MallOrderFactId,
    /// 同一售后案件。
    pub after_sales_request_id: MallAfterSalesRequestId,
    /// 来源商城。
    pub mall_id: String,
    /// 商城退款身份。
    pub external_refund_no: String,
    /// 商城退款版本。
    pub external_refund_version: String,
    /// 原订单。
    pub mall_order_id: MallOrderId,
    /// 实际成功退款金额。
    pub refund_amount: Amount,
    /// 实际退款时间。
    pub refunded_at: Instant,
}

impl MallRefund {
    /// 创建退款成功事实头。
    ///
    /// 完成文本校验与规范化；`refund_amount` 必须大于零（实际成功退款金额）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallRefundId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的退款头实体。
    ///
    /// # 错误
    /// 当文本为空/超长或退款金额非正时返回错误。
    pub fn new(id: MallRefundId, data: MallRefundData) -> Result<Self> {
        let mall_id = normalize_required_text(
            data.mall_id,
            "来源商城不能为空",
            MALL_ID_MAX_LEN,
            "来源商城代码过长",
        )?;
        let external_refund_no = normalize_required_text(
            data.external_refund_no,
            "商城退款单号不能为空",
            REFUND_NO_MAX_LEN,
            "商城退款单号过长",
        )?;
        let external_refund_version = normalize_required_text(
            data.external_refund_version,
            "退款版本不能为空",
            REFUND_VERSION_MAX_LEN,
            "退款版本过长",
        )?;
        if data.refund_amount.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("退款金额必须大于零"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_order_fact_id: data.mall_order_fact_id,
            after_sales_request_id: data.after_sales_request_id,
            mall_id,
            external_refund_no,
            external_refund_version,
            mall_order_id: data.mall_order_id,
            refund_amount: data.refund_amount,
            refunded_at: data.refunded_at,
        })
    }
}

/// 退款行创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallRefundLineData {
    /// 退款头。
    pub mall_refund_id: MallRefundId,
    /// 稳定行号（从 1 起）。
    pub line_no: u32,
    /// 原商品明细。
    pub mall_order_item_id: MallOrderItemId,
    /// 本商品实际退款的基本单位数量。
    pub refunded_quantity: Quantity,
    /// 本商品实际退款金额。
    pub line_refund_amount: Amount,
}

/// 商品退款行实体（数据模型 §6.18）。
///
/// 保存不重复计量的商品退款数量与金额；行金额合计等于头金额由 P3 落实
/// （P3 条目：§6.18 退款行合计守恒）。不可变，只提供 `new()`。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallRefundLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 退款头。
    pub mall_refund_id: MallRefundId,
    /// 稳定行号。
    pub line_no: u32,
    /// 原商品明细。
    pub mall_order_item_id: MallOrderItemId,
    /// 本商品实际退款的基本单位数量。
    pub refunded_quantity: Quantity,
    /// 本商品实际退款金额。
    pub line_refund_amount: Amount,
}

impl MallRefundLine {
    /// 创建退款行。
    ///
    /// `line_no` 从 1 起；退款数量与金额必须大于零。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallRefundLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的退款行实体。
    ///
    /// # 错误
    /// 当行号为 0 或退款数量/金额非正时返回错误。
    pub fn new(id: MallRefundLineId, data: MallRefundLineData) -> Result<Self> {
        if data.line_no == 0 {
            return Err(Error::from("行号必须从 1 开始"));
        }
        if data.refunded_quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("退款数量必须大于零"));
        }
        if data.line_refund_amount.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("退款金额必须大于零"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_refund_id: data.mall_refund_id,
            line_no: data.line_no,
            mall_order_item_id: data.mall_order_item_id,
            refunded_quantity: data.refunded_quantity,
            line_refund_amount: data.line_refund_amount,
        })
    }
}

/// 退款分配创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallRefundAllocationData {
    /// 退款行。
    pub mall_refund_line_id: MallRefundLineId,
    /// 稳定分配序号（从 1 起）。
    pub allocation_no: u32,
    /// 原商品 × 原支付来源消费事实。
    pub original_consumption_entry_id: MallConsumptionEntryId,
    /// 原卡券或微信来源。
    pub original_payment_source_id: MallPaymentSourceId,
    /// 实际冲减金额。
    pub allocated_refund_amount: Amount,
    /// `APPLY` 或 `REVERSE`。
    pub allocation_action: AllocationAction,
    /// `REVERSE` 必填的原 `APPLY` 分配。
    pub reverses_allocation_id: Option<MallRefundAllocationId>,
    /// 与本分配同事务追加的消费反向或反向纠错事实。
    pub reversal_consumption_entry_id: Option<MallConsumptionEntryId>,
}

/// 退款分配实体（数据模型 §6.18）。
///
/// `(mall_refund_line_id, allocation_no)` 唯一与 `REVERSE` 引用唯一由 P2 唯一索引
/// 落实；等额引用、净额上限依赖聚合查询，由 P3 落实（P3 条目：§6.18 分配净额）。
/// 不可变，只提供 `new()`。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallRefundAllocation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 退款行。
    pub mall_refund_line_id: MallRefundLineId,
    /// 稳定分配序号。
    pub allocation_no: u32,
    /// 原商品 × 原支付来源消费事实。
    pub original_consumption_entry_id: MallConsumptionEntryId,
    /// 原卡券或微信来源。
    pub original_payment_source_id: MallPaymentSourceId,
    /// 实际冲减金额。
    pub allocated_refund_amount: Amount,
    /// `APPLY` 或 `REVERSE`。
    pub allocation_action: AllocationAction,
    /// `REVERSE` 必填的原 `APPLY` 分配。
    pub reverses_allocation_id: Option<MallRefundAllocationId>,
    /// 与本分配同事务追加的消费反向或反向纠错事实。
    pub reversal_consumption_entry_id: Option<MallConsumptionEntryId>,
}

impl MallRefundAllocation {
    /// 创建退款分配。
    ///
    /// 强制单行不变式（§6.18）：`allocation_no` 从 1 起；分配金额非负；
    /// `APPLY` 不得携带反向引用，`REVERSE` 必须同时携带原 `APPLY` 分配与同事务
    /// 消费反向事实。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallRefundAllocationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的退款分配实体。
    ///
    /// # 错误
    /// 当序号为 0、金额为负或动作与引用字段不一致时返回错误。
    pub fn new(id: MallRefundAllocationId, data: MallRefundAllocationData) -> Result<Self> {
        if data.allocation_no == 0 {
            return Err(Error::from("分配序号必须从 1 开始"));
        }
        if data.allocated_refund_amount.to_decimal().is_sign_negative() {
            return Err(Error::from("分配金额不能为负"));
        }
        validate_allocation_action(
            data.allocation_action,
            data.reverses_allocation_id.clone(),
            data.reversal_consumption_entry_id.clone(),
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_refund_line_id: data.mall_refund_line_id,
            allocation_no: data.allocation_no,
            original_consumption_entry_id: data.original_consumption_entry_id,
            original_payment_source_id: data.original_payment_source_id,
            allocated_refund_amount: data.allocated_refund_amount,
            allocation_action: data.allocation_action,
            reverses_allocation_id: data.reverses_allocation_id,
            reversal_consumption_entry_id: data.reversal_consumption_entry_id,
        })
    }
}

/// 校验分配动作与引用字段一致性。
///
/// # 参数
/// * `action` - 分配动作
/// * `reverses_allocation_id` - 反向引用
/// * `reversal_consumption_entry_id` - 消费反向事实
///
/// # 返回
/// 动作与引用一致返回 `Ok(())`。
///
/// # 错误
/// `APPLY` 携带反向引用，或 `REVERSE` 缺失反向引用/消费反向事实时返回错误。
fn validate_allocation_action(
    action: AllocationAction,
    reverses_allocation_id: Option<MallRefundAllocationId>,
    reversal_consumption_entry_id: Option<MallConsumptionEntryId>,
) -> Result<()> {
    if action.is_reverse() != reverses_allocation_id.is_some() {
        return Err(Error::from(
            "REVERSE 分配必须引用原 APPLY 分配，APPLY 分配不得携带反向引用",
        ));
    }
    if action.is_reverse() != reversal_consumption_entry_id.is_some() {
        return Err(Error::from(
            "REVERSE 分配必须携带同事务消费反向事实，APPLY 分配不得携带",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        MallRefund, MallRefundAllocation, MallRefundAllocationData, MallRefundData, MallRefundLine,
        MallRefundLineData,
    };
    use crate::common::time::Instant;
    use crate::ids::{
        MallAfterSalesRequestId, MallConsumptionEntryId, MallOrderFactId, MallOrderId, MallOrderItemId,
        MallPaymentSourceId, MallRefundAllocationId, MallRefundId, MallRefundLineId,
    };
    use crate::mall_after_sales::types::AllocationAction;
    use crate::money::{Amount, Quantity};
    use std::str::FromStr;

    fn refund_data() -> MallRefundData {
        MallRefundData {
            mall_order_fact_id: MallOrderFactId::new("fact-2"),
            after_sales_request_id: MallAfterSalesRequestId::new("asr-1"),
            mall_id: " mall-a ".to_string(),
            external_refund_no: " rn-100 ".to_string(),
            external_refund_version: " v1 ".to_string(),
            mall_order_id: MallOrderId::new("order-1"),
            refund_amount: Amount::from_str("49.00").unwrap(),
            refunded_at: Instant::from_unix_secs(1_700_000_200),
        }
    }

    fn line_data() -> MallRefundLineData {
        MallRefundLineData {
            mall_refund_id: MallRefundId::new("refund-1"),
            line_no: 1,
            mall_order_item_id: MallOrderItemId::new("item-1"),
            refunded_quantity: Quantity::from_str("1.000000").unwrap(),
            line_refund_amount: Amount::from_str("49.00").unwrap(),
        }
    }

    fn allocation_data(action: AllocationAction) -> MallRefundAllocationData {
        MallRefundAllocationData {
            mall_refund_line_id: MallRefundLineId::new("rl-1"),
            allocation_no: 1,
            original_consumption_entry_id: MallConsumptionEntryId::new("ce-1"),
            original_payment_source_id: MallPaymentSourceId::new("ps-1"),
            allocated_refund_amount: Amount::from_str("49.00").unwrap(),
            allocation_action: action,
            reverses_allocation_id: if action == AllocationAction::Reverse {
                Some(MallRefundAllocationId::new("ra-1"))
            } else {
                None
            },
            reversal_consumption_entry_id: if action == AllocationAction::Reverse {
                Some(MallConsumptionEntryId::new("ce-2"))
            } else {
                None
            },
        }
    }

    /// happy path：退款头文本规范化、金额与事实引用落库。
    #[test]
    fn refund_new_trims_fields_and_keeps_fact_link() {
        let refund = MallRefund::new(MallRefundId::new("refund-1"), refund_data()).unwrap();

        assert_eq!(refund.mall_id, "mall-a");
        assert_eq!(refund.external_refund_no, "rn-100");
        assert_eq!(refund.external_refund_version, "v1");
        assert_eq!(refund.refund_amount, Amount::from_str("49.00").unwrap());
        assert_eq!(refund.mall_order_fact_id, MallOrderFactId::new("fact-2"));
        assert_eq!(
            refund.after_sales_request_id,
            MallAfterSalesRequestId::new("asr-1")
        );
    }

    /// 失败路径：必填空、超长、退款金额非正。
    #[test]
    fn refund_new_rejects_blank_overlong_and_non_positive_amount() {
        let blank = MallRefundData {
            external_refund_no: "  ".to_string(),
            ..refund_data()
        };
        assert!(MallRefund::new(MallRefundId::new("r2"), blank).is_err());

        let overlong = MallRefundData {
            external_refund_no: "n".repeat(129),
            ..refund_data()
        };
        assert!(MallRefund::new(MallRefundId::new("r3"), overlong).is_err());

        let zero = MallRefundData {
            refund_amount: Amount::from_str("0.00").unwrap(),
            ..refund_data()
        };
        assert!(MallRefund::new(MallRefundId::new("r4"), zero).is_err());
    }

    /// 退款行：happy path 与行号/数量/金额越界拒绝。
    #[test]
    fn refund_line_keeps_fields_and_rejects_invalid_scope() {
        let line = MallRefundLine::new(MallRefundLineId::new("rl-1"), line_data()).unwrap();
        assert_eq!(line.line_no, 1);
        assert_eq!(line.refunded_quantity, Quantity::from_str("1.000000").unwrap());
        assert_eq!(line.line_refund_amount, Amount::from_str("49.00").unwrap());

        let zero_no = MallRefundLineData {
            line_no: 0,
            ..line_data()
        };
        assert!(MallRefundLine::new(MallRefundLineId::new("rl-2"), zero_no).is_err());

        let zero_quantity = MallRefundLineData {
            refunded_quantity: Quantity::from_str("0.000000").unwrap(),
            ..line_data()
        };
        assert!(MallRefundLine::new(MallRefundLineId::new("rl-3"), zero_quantity).is_err());

        let zero_amount = MallRefundLineData {
            line_refund_amount: Amount::from_str("0.00").unwrap(),
            ..line_data()
        };
        assert!(MallRefundLine::new(MallRefundLineId::new("rl-4"), zero_amount).is_err());
    }

    /// 分配：APPLY 与 REVERSE 各自身份与引用完整性；happy path。
    #[test]
    fn allocation_apply_and_reverse_keep_action_consistency() {
        let apply = MallRefundAllocation::new(
            MallRefundAllocationId::new("ra-1"),
            allocation_data(AllocationAction::Apply),
        )
        .unwrap();
        assert_eq!(apply.allocation_action, AllocationAction::Apply);
        assert!(apply.reverses_allocation_id.is_none());
        assert!(apply.reversal_consumption_entry_id.is_none());

        let reverse = MallRefundAllocation::new(
            MallRefundAllocationId::new("ra-2"),
            allocation_data(AllocationAction::Reverse),
        )
        .unwrap();
        assert_eq!(reverse.allocation_action, AllocationAction::Reverse);
        assert_eq!(
            reverse.reverses_allocation_id,
            Some(MallRefundAllocationId::new("ra-1"))
        );
        assert_eq!(
            reverse.reversal_consumption_entry_id,
            Some(MallConsumptionEntryId::new("ce-2"))
        );
    }

    /// 失败路径 + 金额：分配序号 0、负金额、动作与引用不一致。
    #[test]
    fn allocation_rejects_zero_no_negative_amount_and_action_mismatch() {
        let zero_no = MallRefundAllocationData {
            allocation_no: 0,
            ..allocation_data(AllocationAction::Apply)
        };
        assert!(MallRefundAllocation::new(MallRefundAllocationId::new("ra-3"), zero_no).is_err());

        let negative = MallRefundAllocationData {
            allocated_refund_amount: Amount::from_str("-0.01").unwrap(),
            ..allocation_data(AllocationAction::Apply)
        };
        assert!(MallRefundAllocation::new(MallRefundAllocationId::new("ra-4"), negative).is_err());

        let apply_with_reverse = MallRefundAllocationData {
            reverses_allocation_id: Some(MallRefundAllocationId::new("ra-1")),
            ..allocation_data(AllocationAction::Apply)
        };
        assert!(MallRefundAllocation::new(MallRefundAllocationId::new("ra-5"), apply_with_reverse).is_err());

        let reverse_without_ref = MallRefundAllocationData {
            reverses_allocation_id: None,
            ..allocation_data(AllocationAction::Reverse)
        };
        assert!(MallRefundAllocation::new(MallRefundAllocationId::new("ra-6"), reverse_without_ref).is_err());

        let reverse_without_reversal_entry = MallRefundAllocationData {
            reversal_consumption_entry_id: None,
            ..allocation_data(AllocationAction::Reverse)
        };
        assert!(MallRefundAllocation::new(
            MallRefundAllocationId::new("ra-7"),
            reverse_without_reversal_entry,
        )
        .is_err());
    }

    /// 金额：分配金额非负（APPLY/REVERSE 均可为零金额校验通过，负值拒绝）。
    #[test]
    fn allocation_amount_is_non_negative() {
        let zero = MallRefundAllocationData {
            allocated_refund_amount: Amount::from_str("0.00").unwrap(),
            ..allocation_data(AllocationAction::Apply)
        };
        assert!(MallRefundAllocation::new(MallRefundAllocationId::new("ra-8"), zero).is_ok());
    }
}
