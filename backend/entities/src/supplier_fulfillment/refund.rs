//! `supplier_refund_fact` 与 `supplier_refund_allocation`（数据模型 §6.19 供应商退款事实与分配）。
//!
//! 供应商退款成功是冲减供应商成本和应付的唯一事实（§6.19）；退款头与分配行是不可变
//! 正式事实，只 `new` 不 `update`，不设业务软删除（§4.5.1）。实体层固化：退款头金额
//! 与 APPLY 分配合计恒等、分配行含税/不含税/税额三元组恒等、未付应付冲减与现金退款
//! 拆分恒等、REVERSE 必须引用原 APPLY 分配；原成本/应付/付款分配的锁定与冲减编排
//! （§8.4 第 5 条）留给 P3。

use std::str::FromStr;

use entity_core::BaseModel;
use entity_macros::Entity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    CostAllocationId, CostEntryId, InboxMessageId, PayableEntryId, PaymentAllocationId, SupplierAccountId,
    SupplierApiConnectionId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
    SupplierRefundAllocationId, SupplierRefundFactId, SupplierRefundId,
};
use crate::money::{Amount, Quantity};
use crate::validation::normalize_required_text;

/// 外部退款号最大长度。
const EXTERNAL_REFUND_NO_MAX_LEN: usize = 64;
/// 外部退款版本最大长度。
const EXTERNAL_REFUND_VERSION_MAX_LEN: usize = 64;
/// 来源事件 ID 最大长度。
const SOURCE_EVENT_ID_MAX_LEN: usize = 128;

/// 退款分配动作（数据模型 §6.19：`APPLY` 或 `REVERSE`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AllocationAction {
    /// 应用：冲减原成本、应付和适用付款分配。
    Apply,
    /// 反向：纠错追加引用原 APPLY 分配的反向事实。
    Reverse,
}

impl AllocationAction {
    /// 返回动作的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Apply => "应用",
            Self::Reverse => "反向",
        }
    }

    /// 返回动作的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Apply => "APPLY",
            Self::Reverse => "REVERSE",
        }
    }
}

/// 供应商退款事实创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierRefundFactData {
    /// 退款供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商连接。
    pub connection_id: SupplierApiConnectionId,
    /// 原供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 外部退款号。
    pub external_refund_no: String,
    /// 外部退款版本（与连接、退款号组成幂等键）。
    pub external_refund_version: String,
    /// 实际退款金额。
    pub refund_amount: Amount,
    /// 实际退款时间。
    pub refunded_at: Instant,
    /// 来源事件 ID。
    pub source_event_id: String,
    /// 共同消息信封（非空且唯一）。
    pub inbox_message_id: InboxMessageId,
}

/// 供应商退款事实实体（数据模型 §6.19，正式事实头，不可变）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierRefundFact {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 退款供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商连接。
    pub connection_id: SupplierApiConnectionId,
    /// 原供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 外部退款号。
    pub external_refund_no: String,
    /// 外部退款版本。
    pub external_refund_version: String,
    /// 实际退款金额。
    pub refund_amount: Amount,
    /// 实际退款时间。
    pub refunded_at: Instant,
    /// 来源事件 ID。
    pub source_event_id: String,
    /// 共同消息信封。
    pub inbox_message_id: InboxMessageId,
}

impl SupplierRefundFact {
    /// 创建供应商退款事实。
    ///
    /// 完成外部退款身份与来源事件的校验和规范化，并强制退款金额大于零。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierRefundFactId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的退款事实实体。
    ///
    /// # 错误
    /// 字段为空/超长或退款金额小于等于零时返回错误。
    pub fn new(id: SupplierRefundFactId, data: SupplierRefundFactData) -> Result<Self> {
        let external_refund_no = normalize_required_text(
            data.external_refund_no,
            "外部退款号不能为空",
            EXTERNAL_REFUND_NO_MAX_LEN,
            "外部退款号过长",
        )?;
        let external_refund_version = normalize_required_text(
            data.external_refund_version,
            "外部退款版本不能为空",
            EXTERNAL_REFUND_VERSION_MAX_LEN,
            "外部退款版本过长",
        )?;
        let source_event_id = normalize_required_text(
            data.source_event_id,
            "来源事件ID不能为空",
            SOURCE_EVENT_ID_MAX_LEN,
            "来源事件ID过长",
        )?;
        if data.refund_amount.to_decimal() <= Decimal::ZERO {
            return Err(Error::from("退款金额必须大于零"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            supplier_id: data.supplier_id,
            connection_id: data.connection_id,
            supplier_fulfillment_order_id: data.supplier_fulfillment_order_id,
            external_refund_no,
            external_refund_version,
            refund_amount: data.refund_amount,
            refunded_at: data.refunded_at,
            source_event_id,
            inbox_message_id: data.inbox_message_id,
        })
    }

    /// 校验退款分配与退款头的一致性（数据模型 §6.19：各行净含税金额合计等于退款头金额）。
    ///
    /// 逐行校验分配归属本退款头；只有 `APPLY` 分配参与合计（`REVERSE` 是纠错事实，
    /// 与原 `APPLY` 一对一由 P3 校验），累计净退款数量上限校验在 P3 锁定原明细与
    /// 成本后执行（§6.19、§8.4 第 5 条）。
    ///
    /// # 参数
    /// * `allocations` - 本退款头下的全部分配行
    ///
    /// # 返回
    /// 归属正确且合计恒等时返回 `Ok(())`。
    ///
    /// # 错误
    /// 分配归属其他退款头或 APPLY 合计不等于退款头金额时返回错误。
    pub fn validate_allocations(&self, allocations: &[SupplierRefundAllocation]) -> Result<()> {
        let mut total = Amount::from_str("0.00").expect("零是合法金额");
        for allocation in allocations {
            if allocation.supplier_refund_fact_id.as_ref() != self.base.id {
                return Err(Error::from("分配不属于该退款头"));
            }
            if allocation.allocation_action == AllocationAction::Apply {
                total = total.checked_add(allocation.gross_amount);
            }
        }
        if total != self.refund_amount {
            return Err(Error::from("各 APPLY 分配含税金额合计必须等于退款头金额"));
        }
        Ok(())
    }
}

/// 供应商退款分配创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierRefundAllocationData {
    /// 所属退款事实头。
    pub supplier_refund_fact_id: SupplierRefundFactId,
    /// 退款头内稳定分配序号。
    pub allocation_no: u32,
    /// 原供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 被冲减的原成本。
    pub original_cost_entry_id: CostEntryId,
    /// 被冲减的原成本归属。
    pub original_cost_allocation_id: CostAllocationId,
    /// 被冲减的原应付分录。
    pub original_payable_entry_id: PayableEntryId,
    /// 原应付已付款部分的付款分配，可空。
    pub original_payment_allocation_id: Option<PaymentAllocationId>,
    /// 实际供应商退款数量。
    pub refund_quantity: Quantity,
    /// 含税成本冲减金额。
    pub gross_amount: Amount,
    /// 不含税成本冲减金额。
    pub net_amount: Amount,
    /// 税额冲减金额。
    pub tax_amount: Amount,
    /// 未付应付冲减金额。
    pub payable_reduction_amount: Amount,
    /// 已付现金退回拆分金额。
    pub cash_refund_amount: Amount,
    /// 已退款现金形成的通用 `supplier_refund` 事实，可空。
    pub cash_supplier_refund_id: Option<SupplierRefundId>,
    /// 分配动作：`APPLY` 或 `REVERSE`。
    pub allocation_action: AllocationAction,
    /// `REVERSE` 必填的原 `APPLY` 分配。
    pub reverses_allocation_id: Option<SupplierRefundAllocationId>,
}

/// 供应商退款分配实体（数据模型 §6.19，正式事实行，不可变）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierRefundAllocation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属退款事实头。
    pub supplier_refund_fact_id: SupplierRefundFactId,
    /// 退款头内稳定分配序号。
    pub allocation_no: u32,
    /// 原供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 被冲减的原成本。
    pub original_cost_entry_id: CostEntryId,
    /// 被冲减的原成本归属。
    pub original_cost_allocation_id: CostAllocationId,
    /// 被冲减的原应付分录。
    pub original_payable_entry_id: PayableEntryId,
    /// 原应付已付款部分的付款分配，可空。
    pub original_payment_allocation_id: Option<PaymentAllocationId>,
    /// 实际供应商退款数量。
    pub refund_quantity: Quantity,
    /// 含税成本冲减金额。
    pub gross_amount: Amount,
    /// 不含税成本冲减金额。
    pub net_amount: Amount,
    /// 税额冲减金额。
    pub tax_amount: Amount,
    /// 未付应付冲减金额。
    pub payable_reduction_amount: Amount,
    /// 已付现金退回拆分金额。
    pub cash_refund_amount: Amount,
    /// 已退款现金形成的通用 `supplier_refund` 事实，可空。
    pub cash_supplier_refund_id: Option<SupplierRefundId>,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 原 `APPLY` 分配。
    pub reverses_allocation_id: Option<SupplierRefundAllocationId>,
}

impl SupplierRefundAllocation {
    /// 创建供应商退款分配。
    ///
    /// 校验退款数量大于零、全部金额非负，并强制两条恒等（§6.19）：
    /// `gross = net + tax`、`payable_reduction + cash_refund = gross`；
    /// 同时强制 `REVERSE` 必填原分配、`APPLY` 不得引用原分配。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierRefundAllocationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的退款分配实体。
    ///
    /// # 错误
    /// 数量或金额越界、金额恒等不成立或 REVERSE/APPLY 引用不一致时返回错误。
    pub fn new(id: SupplierRefundAllocationId, data: SupplierRefundAllocationData) -> Result<Self> {
        if data.refund_quantity.to_decimal() <= Decimal::ZERO {
            return Err(Error::from("退款数量必须大于零"));
        }
        ensure_non_negative(data.gross_amount, "含税冲减金额不得为负")?;
        ensure_non_negative(data.net_amount, "不含税冲减金额不得为负")?;
        ensure_non_negative(data.tax_amount, "税额冲减金额不得为负")?;
        ensure_non_negative(data.payable_reduction_amount, "未付应付冲减金额不得为负")?;
        ensure_non_negative(data.cash_refund_amount, "现金退款金额不得为负")?;
        if data.gross_amount.to_decimal() != data.net_amount.to_decimal() + data.tax_amount.to_decimal() {
            return Err(Error::from("含税金额必须等于不含税金额加税额"));
        }
        if data.payable_reduction_amount.checked_add(data.cash_refund_amount) != data.gross_amount {
            return Err(Error::from("未付应付冲减与现金退款拆分合计必须等于含税金额"));
        }
        let is_reverse = data.allocation_action == AllocationAction::Reverse;
        if is_reverse && data.reverses_allocation_id.is_none() {
            return Err(Error::from("REVERSE 分配必须引用原 APPLY 分配"));
        }
        if !is_reverse && data.reverses_allocation_id.is_some() {
            return Err(Error::from("APPLY 分配不得引用原分配"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            supplier_refund_fact_id: data.supplier_refund_fact_id,
            allocation_no: data.allocation_no,
            supplier_fulfillment_item_id: data.supplier_fulfillment_item_id,
            original_cost_entry_id: data.original_cost_entry_id,
            original_cost_allocation_id: data.original_cost_allocation_id,
            original_payable_entry_id: data.original_payable_entry_id,
            original_payment_allocation_id: data.original_payment_allocation_id,
            refund_quantity: data.refund_quantity,
            gross_amount: data.gross_amount,
            net_amount: data.net_amount,
            tax_amount: data.tax_amount,
            payable_reduction_amount: data.payable_reduction_amount,
            cash_refund_amount: data.cash_refund_amount,
            cash_supplier_refund_id: data.cash_supplier_refund_id,
            allocation_action: data.allocation_action,
            reverses_allocation_id: data.reverses_allocation_id,
        })
    }
}

/// 校验金额非负。
///
/// # 参数
/// * `value` - 金额
/// * `message` - 失败时的错误信息
///
/// # 错误
/// 金额为负时返回错误。
fn ensure_non_negative(value: Amount, message: &str) -> Result<()> {
    if value.to_decimal() < Decimal::ZERO {
        return Err(Error::from(message));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{
        CostAllocationId, CostEntryId, InboxMessageId, PayableEntryId, PaymentAllocationId,
        SupplierRefundAllocationId, SupplierRefundFactId, SupplierRefundId,
    };

    fn sample_fact_data() -> SupplierRefundFactData {
        SupplierRefundFactData {
            supplier_id: SupplierAccountId::new("supplier-1"),
            connection_id: SupplierApiConnectionId::new("connection-1"),
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
            external_refund_no: " REF-1001 ".to_string(),
            external_refund_version: "1".to_string(),
            refund_amount: Amount::from_str("19.98").unwrap(),
            refunded_at: Instant::from_unix_secs(1_700_000_000),
            source_event_id: " EVT-2001 ".to_string(),
            inbox_message_id: InboxMessageId::new("message-1"),
        }
    }

    fn sample_allocation_data() -> SupplierRefundAllocationData {
        SupplierRefundAllocationData {
            supplier_refund_fact_id: SupplierRefundFactId::new("fact-1"),
            allocation_no: 1,
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item-1"),
            original_cost_entry_id: CostEntryId::new("cost-entry-1"),
            original_cost_allocation_id: CostAllocationId::new("cost-allocation-1"),
            original_payable_entry_id: PayableEntryId::new("payable-entry-1"),
            original_payment_allocation_id: Some(PaymentAllocationId::new("payment-alloc-1")),
            refund_quantity: Quantity::from_str("2.000000").unwrap(),
            gross_amount: Amount::from_str("19.98").unwrap(),
            net_amount: Amount::from_str("17.38").unwrap(),
            tax_amount: Amount::from_str("2.60").unwrap(),
            payable_reduction_amount: Amount::from_str("9.99").unwrap(),
            cash_refund_amount: Amount::from_str("9.99").unwrap(),
            cash_supplier_refund_id: Some(SupplierRefundId::new("cash-refund-1")),
            allocation_action: AllocationAction::Apply,
            reverses_allocation_id: None,
        }
    }

    #[test]
    fn fact_new_accepts_valid_fact_and_normalizes_fields() {
        let fact = SupplierRefundFact::new(SupplierRefundFactId::new("fact-1"), sample_fact_data()).unwrap();

        assert_eq!(fact.external_refund_no, "REF-1001");
        assert_eq!(fact.source_event_id, "EVT-2001");
        assert_eq!(fact.refund_amount, Amount::from_str("19.98").unwrap());
        assert_eq!(fact.inbox_message_id, InboxMessageId::new("message-1"));
    }

    #[test]
    fn fact_new_rejects_non_positive_refund_amount() {
        for amount in [
            Amount::from_str("0.00").unwrap(),
            Amount::from_str("-1.00").unwrap(),
        ] {
            let data = SupplierRefundFactData {
                refund_amount: amount,
                ..sample_fact_data()
            };
            assert!(SupplierRefundFact::new(SupplierRefundFactId::new("fact-2"), data).is_err());
        }
    }

    #[test]
    fn fact_new_rejects_empty_or_overlong_fields() {
        let empty_refund_no = SupplierRefundFactData {
            external_refund_no: "  ".to_string(),
            ..sample_fact_data()
        };
        assert!(SupplierRefundFact::new(SupplierRefundFactId::new("fact-3"), empty_refund_no).is_err());

        let overlong_version = SupplierRefundFactData {
            external_refund_version: "v".repeat(65),
            ..sample_fact_data()
        };
        assert!(SupplierRefundFact::new(SupplierRefundFactId::new("fact-4"), overlong_version).is_err());

        let empty_event_id = SupplierRefundFactData {
            source_event_id: "  ".to_string(),
            ..sample_fact_data()
        };
        assert!(SupplierRefundFact::new(SupplierRefundFactId::new("fact-5"), empty_event_id).is_err());
    }

    #[test]
    fn allocation_new_accepts_apply_and_reverse() {
        let apply = SupplierRefundAllocation::new(
            SupplierRefundAllocationId::new("allocation-1"),
            sample_allocation_data(),
        )
        .unwrap();
        assert_eq!(apply.allocation_action, AllocationAction::Apply);
        assert!(apply.reverses_allocation_id.is_none());

        let reverse_data = SupplierRefundAllocationData {
            allocation_action: AllocationAction::Reverse,
            reverses_allocation_id: Some(SupplierRefundAllocationId::new("allocation-1")),
            original_payment_allocation_id: None,
            cash_supplier_refund_id: None,
            ..sample_allocation_data()
        };
        let reverse =
            SupplierRefundAllocation::new(SupplierRefundAllocationId::new("allocation-2"), reverse_data)
                .unwrap();
        assert_eq!(reverse.allocation_action, AllocationAction::Reverse);
        assert_eq!(
            reverse.reverses_allocation_id,
            Some(SupplierRefundAllocationId::new("allocation-1"))
        );
    }

    #[test]
    fn allocation_new_rejects_inconsistent_reverse_reference() {
        let reverse_without_original = SupplierRefundAllocationData {
            allocation_action: AllocationAction::Reverse,
            reverses_allocation_id: None,
            ..sample_allocation_data()
        };
        assert!(SupplierRefundAllocation::new(
            SupplierRefundAllocationId::new("allocation-3"),
            reverse_without_original
        )
        .is_err());

        let apply_with_reference = SupplierRefundAllocationData {
            allocation_action: AllocationAction::Apply,
            reverses_allocation_id: Some(SupplierRefundAllocationId::new("allocation-1")),
            ..sample_allocation_data()
        };
        assert!(SupplierRefundAllocation::new(
            SupplierRefundAllocationId::new("allocation-4"),
            apply_with_reference
        )
        .is_err());
    }

    #[test]
    fn allocation_new_rejects_negative_amounts() {
        let negative_gross = SupplierRefundAllocationData {
            gross_amount: Amount::from_str("-19.98").unwrap(),
            payable_reduction_amount: Amount::from_str("9.99").unwrap(),
            cash_refund_amount: Amount::from_str("-29.97").unwrap(),
            ..sample_allocation_data()
        };
        assert!(SupplierRefundAllocation::new(
            SupplierRefundAllocationId::new("allocation-5"),
            negative_gross
        )
        .is_err());
    }

    #[test]
    fn allocation_new_enforces_amount_identities() {
        let broken_tax_identity = SupplierRefundAllocationData {
            net_amount: Amount::from_str("17.00").unwrap(),
            ..sample_allocation_data()
        };
        assert!(
            SupplierRefundAllocation::new(
                SupplierRefundAllocationId::new("allocation-6"),
                broken_tax_identity
            )
            .is_err(),
            "gross = net + tax 必须恒等"
        );

        let broken_split_identity = SupplierRefundAllocationData {
            payable_reduction_amount: Amount::from_str("9.99").unwrap(),
            cash_refund_amount: Amount::from_str("10.00").unwrap(),
            ..sample_allocation_data()
        };
        assert!(
            SupplierRefundAllocation::new(
                SupplierRefundAllocationId::new("allocation-7"),
                broken_split_identity
            )
            .is_err(),
            "payable_reduction + cash_refund = gross 必须恒等"
        );
    }

    #[test]
    fn allocation_new_rejects_non_positive_refund_quantity() {
        let data = SupplierRefundAllocationData {
            refund_quantity: Quantity::from_str("0.000000").unwrap(),
            ..sample_allocation_data()
        };
        assert!(
            SupplierRefundAllocation::new(SupplierRefundAllocationId::new("allocation-8"), data).is_err()
        );
    }

    #[test]
    fn fact_validate_allocations_requires_exact_total() {
        let fact = SupplierRefundFact::new(SupplierRefundFactId::new("fact-1"), sample_fact_data()).unwrap();
        let allocation = SupplierRefundAllocation::new(
            SupplierRefundAllocationId::new("allocation-1"),
            sample_allocation_data(),
        )
        .unwrap();
        fact.validate_allocations(std::slice::from_ref(&allocation))
            .unwrap();

        let half = SupplierRefundAllocation::new(
            SupplierRefundAllocationId::new("allocation-2"),
            SupplierRefundAllocationData {
                allocation_no: 2,
                gross_amount: Amount::from_str("9.99").unwrap(),
                net_amount: Amount::from_str("8.69").unwrap(),
                tax_amount: Amount::from_str("1.30").unwrap(),
                payable_reduction_amount: Amount::from_str("0.00").unwrap(),
                cash_refund_amount: Amount::from_str("9.99").unwrap(),
                ..sample_allocation_data()
            },
        )
        .unwrap();
        assert!(
            fact.validate_allocations(&[allocation, half]).is_err(),
            "合计不等于退款头金额必须被拒绝"
        );
    }

    #[test]
    fn fact_validate_allocations_rejects_foreign_allocation() {
        let fact = SupplierRefundFact::new(SupplierRefundFactId::new("fact-1"), sample_fact_data()).unwrap();
        let foreign = SupplierRefundAllocation::new(
            SupplierRefundAllocationId::new("allocation-9"),
            SupplierRefundAllocationData {
                supplier_refund_fact_id: SupplierRefundFactId::new("fact-other"),
                ..sample_allocation_data()
            },
        )
        .unwrap();
        assert!(fact.validate_allocations(&[foreign]).is_err());
    }

    #[test]
    fn fact_validate_allocations_ignores_reverse_allocations() {
        let fact = SupplierRefundFact::new(SupplierRefundFactId::new("fact-1"), sample_fact_data()).unwrap();
        let apply = SupplierRefundAllocation::new(
            SupplierRefundAllocationId::new("allocation-1"),
            sample_allocation_data(),
        )
        .unwrap();
        let reverse = SupplierRefundAllocation::new(
            SupplierRefundAllocationId::new("allocation-2"),
            SupplierRefundAllocationData {
                allocation_action: AllocationAction::Reverse,
                reverses_allocation_id: Some(SupplierRefundAllocationId::new("allocation-1")),
                allocation_no: 2,
                original_payment_allocation_id: None,
                cash_supplier_refund_id: None,
                ..sample_allocation_data()
            },
        )
        .unwrap();
        fact.validate_allocations(&[apply, reverse]).unwrap();
    }
}
