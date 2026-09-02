//! 商城关键事实载荷 exact-one-of 校验（INT-E01）。
//!
//! `ReceiveMallOrderFactRequest` 外层 `Validate` 只覆盖信封字段与递归子对象；
//! 本模块额外强制 `payment`/`cancel`/`completion` 三者精确择一，且与
//! `fact_type` 匹配，拒绝缺失、多个或额外载荷。

use entities::mall_order::FactType;
use validator::Validate;

use super::dto::{CancelFactData, CompletionFactData, PaymentFactData, ReceiveMallOrderFactRequest};
use crate::errors::{Error, Result};

/// 已通过 exact-one-of 与递归校验的商城订单事实载荷。
#[derive(Debug, Clone)]
pub enum ValidatedMallOrderFactPayload {
    /// `PAYMENT_SUCCEEDED` 付款载荷。
    Payment(PaymentFactData),
    /// `ORDER_CANCELED` 取消载荷。
    Cancel(CancelFactData),
    /// `ORDER_COMPLETED` 完成载荷。
    Completion(CompletionFactData),
}

impl ValidatedMallOrderFactPayload {
    /// 从接收请求构造已校验载荷。
    ///
    /// 先执行信封与嵌套 `Validate`（含 items/sources/allocations 递归），再强制
    /// `payment`/`cancel`/`completion` exact-one-of 且与 `fact_type` 对齐。
    ///
    /// # 参数
    /// * `req` - 原始接收请求
    ///
    /// # 返回
    /// 返回与事实类型匹配的唯一载荷。
    ///
    /// # 错误
    /// 字段/嵌套校验失败返回 `ValidationError`；载荷缺失、多个、额外或不匹配
    /// 事实类型时返回 `BusinessLogicError`；售后类事实类型拒绝接收。
    ///
    /// # 约束
    /// 不访问数据库；售后退款/余额恢复仍由售后域接口接收。
    pub fn try_from_request(req: &ReceiveMallOrderFactRequest) -> Result<Self> {
        req.validate()?;
        if req.fact_type.is_after_sales_result() {
            return Err(Error::BusinessLogicError(
                "退款与余额恢复事实由售后域接口接收".to_string(),
            ));
        }
        let present = payload_presence(req);
        if present.count() == 0 {
            return Err(Error::BusinessLogicError(missing_payload_message(req.fact_type)));
        }
        if present.count() > 1 {
            return Err(Error::BusinessLogicError(
                "关键事实载荷必须精确择一，不得同时携带多个或额外载荷".to_string(),
            ));
        }
        match (req.fact_type, present) {
            (FactType::PaymentSucceeded, PayloadPresence { payment: true, .. }) => Ok(Self::Payment(
                req.payment.clone().expect("presence 已确认 payment 存在"),
            )),
            (FactType::OrderCanceled, PayloadPresence { cancel: true, .. }) => Ok(Self::Cancel(
                req.cancel.clone().expect("presence 已确认 cancel 存在"),
            )),
            (FactType::OrderCompleted, PayloadPresence { completion: true, .. }) => Ok(Self::Completion(
                req.completion.clone().expect("presence 已确认 completion 存在"),
            )),
            _ => Err(Error::BusinessLogicError(mismatch_payload_message(req.fact_type))),
        }
    }
}

/// 三个可选载荷的存在性快照。
#[derive(Debug, Clone, Copy)]
struct PayloadPresence {
    payment: bool,
    cancel: bool,
    completion: bool,
}

impl PayloadPresence {
    /// 返回已出现的载荷数量。
    fn count(self) -> usize {
        usize::from(self.payment) + usize::from(self.cancel) + usize::from(self.completion)
    }
}

/// 统计请求中出现的可选载荷。
fn payload_presence(req: &ReceiveMallOrderFactRequest) -> PayloadPresence {
    PayloadPresence {
        payment: req.payment.is_some(),
        cancel: req.cancel.is_some(),
        completion: req.completion.is_some(),
    }
}

/// 按事实类型返回缺失载荷错误文案（保持既有中文语义）。
fn missing_payload_message(fact_type: FactType) -> String {
    match fact_type {
        FactType::PaymentSucceeded => "支付事实必须携带付款载荷".to_string(),
        FactType::OrderCanceled => "取消事实必须携带取消载荷".to_string(),
        FactType::OrderCompleted => "完成事实必须携带完成载荷".to_string(),
        FactType::RefundSucceeded | FactType::CardBalanceRestored => {
            "退款与余额恢复事实由售后域接口接收".to_string()
        }
    }
}

/// 事实类型与唯一载荷不匹配时的错误文案。
fn mismatch_payload_message(fact_type: FactType) -> String {
    match fact_type {
        FactType::PaymentSucceeded => "支付事实不得携带取消或完成载荷".to_string(),
        FactType::OrderCanceled => "取消事实不得携带付款或完成载荷".to_string(),
        FactType::OrderCompleted => "完成事实不得携带付款或取消载荷".to_string(),
        FactType::RefundSucceeded | FactType::CardBalanceRestored => {
            "退款与余额恢复事实由售后域接口接收".to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use entities::mall_order::{CancelScope, DataSource, FactType, PaymentSourceType};
    use validator::Validate;

    use super::ValidatedMallOrderFactPayload;
    use crate::errors::Error;
    use crate::mall_order::dto::{
        CancelFactData, CompletionFactData, FundingAllocationData, PaymentFactData, PaymentItemData,
        PaymentSourceData, ReceiveMallOrderFactRequest,
    };

    fn payment_item() -> PaymentItemData {
        PaymentItemData {
            external_item_id: "item-1".to_string(),
            sku_id: None,
            product_publication_revision_id: None,
            supplier_offering_revision_id: None,
            name_snapshot: "商品".to_string(),
            spec_snapshot: None,
            quantity: "1.000000".to_string(),
            unit_price_gross: "10.0000".to_string(),
            allocated_discount_amount: "0.00".to_string(),
            allocated_freight_amount: "0.00".to_string(),
            sales_tax_rate: "0.130000".to_string(),
            unit_cost_snapshot: None,
            cost_snapshot_total: None,
            cost_tax_inclusion: None,
            cost_input_tax_rate: None,
        }
    }

    fn payment_payload() -> PaymentFactData {
        PaymentFactData {
            mall_user_ref: "user-1".to_string(),
            source_customer_ref: None,
            customer_id: None,
            ordered_at: 1_700_000_000,
            gross_amount: "10.00".to_string(),
            discount_amount: "0.00".to_string(),
            freight_amount: "0.00".to_string(),
            paid_amount: "10.00".to_string(),
            address_snapshot_encrypted: None,
            items: vec![payment_item()],
            payment_sources: vec![PaymentSourceData {
                source_no: 1,
                source_type: PaymentSourceType::Wechat,
                amount: "10.00".to_string(),
                source_card_instance_ref: None,
                wechat_payment_ref: Some("wx-1".to_string()),
            }],
            funding_allocations: vec![FundingAllocationData {
                external_item_id: "item-1".to_string(),
                source_no: 1,
                allocated_payment_amount: "10.00".to_string(),
            }],
        }
    }

    fn cancel_payload() -> CancelFactData {
        CancelFactData {
            cancel_version: "v1".to_string(),
            cancel_scope: CancelScope::WholeOrder,
            actual_canceled_quantity: "1.000000".to_string(),
            actual_canceled_amount: "10.00".to_string(),
            reason: "用户取消".to_string(),
        }
    }

    fn completion_payload() -> CompletionFactData {
        CompletionFactData {
            completion_version: "v1".to_string(),
            completed_at: 1_700_000_100,
        }
    }

    fn base_request(fact_type: FactType) -> ReceiveMallOrderFactRequest {
        ReceiveMallOrderFactRequest {
            mall_id: "mall-a".to_string(),
            source_event_id: "evt-1".to_string(),
            inbox_message_id: "inbox-1".to_string(),
            business_fact_key: "fact-1".to_string(),
            fact_type,
            external_order_no: "SO-1".to_string(),
            external_order_version: "1".to_string(),
            after_sales_request_id: None,
            original_payment_fact_id: None,
            occurred_at: 1_700_000_000,
            received_at: 1_700_000_001,
            data_source: DataSource::Realtime,
            raw_payload_reference: None,
            payment: None,
            cancel: None,
            completion: None,
        }
    }

    #[test]
    fn accepts_three_legal_fact_payloads() {
        let mut payment = base_request(FactType::PaymentSucceeded);
        payment.payment = Some(payment_payload());
        assert!(matches!(
            ValidatedMallOrderFactPayload::try_from_request(&payment).unwrap(),
            ValidatedMallOrderFactPayload::Payment(_)
        ));

        let mut cancel = base_request(FactType::OrderCanceled);
        cancel.original_payment_fact_id = Some(entities::ids::MallOrderFactId::new("pay-1"));
        cancel.after_sales_request_id = Some(entities::ids::MallAfterSalesRequestId::new("asr-1"));
        cancel.cancel = Some(cancel_payload());
        assert!(matches!(
            ValidatedMallOrderFactPayload::try_from_request(&cancel).unwrap(),
            ValidatedMallOrderFactPayload::Cancel(_)
        ));

        let mut completion = base_request(FactType::OrderCompleted);
        completion.original_payment_fact_id = Some(entities::ids::MallOrderFactId::new("pay-1"));
        completion.completion = Some(completion_payload());
        assert!(matches!(
            ValidatedMallOrderFactPayload::try_from_request(&completion).unwrap(),
            ValidatedMallOrderFactPayload::Completion(_)
        ));
    }

    #[test]
    fn rejects_missing_extra_and_multiple_payloads() {
        let missing = base_request(FactType::PaymentSucceeded);
        match ValidatedMallOrderFactPayload::try_from_request(&missing) {
            Err(Error::BusinessLogicError(message)) => {
                assert!(message.contains("付款载荷"));
            }
            other => panic!("unexpected: {other:?}"),
        }

        let mut extra = base_request(FactType::PaymentSucceeded);
        extra.payment = Some(payment_payload());
        extra.cancel = Some(cancel_payload());
        match ValidatedMallOrderFactPayload::try_from_request(&extra) {
            Err(Error::BusinessLogicError(message)) => {
                assert!(message.contains("精确择一"));
            }
            other => panic!("unexpected: {other:?}"),
        }

        let mut mismatched = base_request(FactType::PaymentSucceeded);
        mismatched.cancel = Some(cancel_payload());
        match ValidatedMallOrderFactPayload::try_from_request(&mismatched) {
            Err(Error::BusinessLogicError(message)) => {
                assert!(message.contains("取消或完成") || message.contains("付款载荷"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn nested_validation_rejects_illegal_amounts_and_blank_refs() {
        let mut req = base_request(FactType::PaymentSucceeded);
        let mut payment = payment_payload();
        payment.items[0].allocated_discount_amount = "-1.00".to_string();
        payment.items[0].external_item_id = " ".to_string();
        payment.payment_sources[0].amount = "abc".to_string();
        req.payment = Some(payment);
        assert!(req.validate().is_err());
        assert!(ValidatedMallOrderFactPayload::try_from_request(&req).is_err());
    }

    #[test]
    fn nested_validation_rejects_illegal_quantity_price_and_rate() {
        let mut req = base_request(FactType::PaymentSucceeded);
        let mut payment = payment_payload();
        payment.items[0].quantity = "1.2345678".to_string();
        payment.items[0].unit_price_gross = "10.00001".to_string();
        payment.items[0].sales_tax_rate = "1.5".to_string();
        req.payment = Some(payment);
        assert!(ValidatedMallOrderFactPayload::try_from_request(&req).is_err());
    }
}
