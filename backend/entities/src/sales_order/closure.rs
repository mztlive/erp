//! 销售单结案资格：单聚合状态事实与应收金额兜底边界。
//!
//! 仓储事实（进行中变更单、应收摘要读取）不得进入本模块；中文 View 文案留在 Service。

use crate::money::Amount;

use super::entity::{
    CloseStatus, CollectionProgress, CommercialStatus, FulfillmentProgress, InvoiceProgress, SalesOrder,
};
use super::types::BusinessType;

/// 销售单结案判定所需的单聚合状态事实。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SalesOrderClosureFacts {
    /// 业务性质。
    pub business_type: BusinessType,
    /// 商业主状态。
    pub commercial_status: CommercialStatus,
    /// 结案状态。
    pub close_status: CloseStatus,
    /// 履约进度。
    pub fulfillment_progress: FulfillmentProgress,
    /// 回款进度。
    pub collection_progress: CollectionProgress,
    /// 开票进度（只展示，不参与是否可结案）。
    pub invoice_progress: InvoiceProgress,
}

/// 终态结案分支。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalesOrderClosureTerminal {
    /// 已关闭。
    Closed,
    /// 已作废。
    Voided,
    /// 草稿。
    Draft,
}

/// 履约未完成时的阻断类型（文案由 Service 映射）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SalesOrderFulfillmentBlocker {
    /// 实物及服务尚未完成客户验收。
    CustomerAcceptance,
    /// 卡券尚未到履约期限。
    VoucherExpiry,
}

/// 结案资格纯事实结果（不含中文文案）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SalesOrderClosureAssessment {
    /// 交付是否已完成。
    pub fulfillment_complete: bool,
    /// 应收是否已结清。
    pub receivable_settled: bool,
    /// 开票是否已完成（不影响 `eligible_to_close`）。
    pub invoice_complete: bool,
    /// 是否具备结案资格。
    pub eligible_to_close: bool,
    /// 终态分支；非终态为 `None`。
    pub terminal: Option<SalesOrderClosureTerminal>,
    /// 履约未完成时的阻断类型。
    pub fulfillment_blocker: Option<SalesOrderFulfillmentBlocker>,
    /// 回款未结清。
    pub collection_unsettled: bool,
}

impl SalesOrder {
    /// 抽取结案判定所需的单聚合状态事实。
    ///
    /// # 返回
    /// 返回业务性质、商业/结案状态与履约/回款/开票进度。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不含应收摘要、进行中变更单等仓储事实。
    pub fn closure_facts(&self) -> SalesOrderClosureFacts {
        SalesOrderClosureFacts {
            business_type: self.business_type,
            commercial_status: self.commercial_status,
            close_status: self.close_status,
            fulfillment_progress: self.fulfillment_progress,
            collection_progress: self.collection_progress,
            invoice_progress: self.invoice_progress,
        }
    }
}

impl SalesOrderClosureFacts {
    /// 结合应收子账摘要评估结案资格。
    ///
    /// # 参数
    /// * `has_receivable_accounts` - 是否存在未删除应收子账
    /// * `settled_total` - 子账已核销含税合计；无子账时为精确零
    /// * `gross_total` - 子账含税应收合计；无子账时为精确零
    ///
    /// # 返回
    /// 返回结案资格纯事实；中文阻断文案由 Service 映射。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 无子账不得因 `0 >= 0` 视为结清；合法零额子账允许金额兜底；开票进度永不阻断。
    pub fn assess(
        &self,
        has_receivable_accounts: bool,
        settled_total: Amount,
        gross_total: Amount,
    ) -> SalesOrderClosureAssessment {
        let invoice_complete = self.invoice_progress == InvoiceProgress::Completed;
        let fulfillment_done = self.fulfillment_progress == FulfillmentProgress::Completed;
        let collection_settled = self.collection_progress == CollectionProgress::Settled;
        let amount_fallback_settled = has_receivable_accounts && settled_total >= gross_total;

        if let Some(terminal) = self.terminal_kind() {
            let closed = terminal == SalesOrderClosureTerminal::Closed;
            return SalesOrderClosureAssessment {
                fulfillment_complete: closed || fulfillment_done,
                receivable_settled: closed || collection_settled,
                invoice_complete,
                eligible_to_close: closed,
                terminal: Some(terminal),
                fulfillment_blocker: None,
                collection_unsettled: !closed && !collection_settled,
            };
        }

        let fulfillment_complete = fulfillment_done;
        let receivable_settled = collection_settled || amount_fallback_settled;
        let fulfillment_blocker = if fulfillment_complete {
            None
        } else if self.business_type == BusinessType::Voucher {
            Some(SalesOrderFulfillmentBlocker::VoucherExpiry)
        } else {
            Some(SalesOrderFulfillmentBlocker::CustomerAcceptance)
        };
        SalesOrderClosureAssessment {
            fulfillment_complete,
            receivable_settled,
            invoice_complete,
            eligible_to_close: fulfillment_complete && receivable_settled,
            terminal: None,
            fulfillment_blocker,
            collection_unsettled: !receivable_settled,
        }
    }

    /// 返回结案终态分支。
    ///
    /// # 返回
    /// 已关闭、已作废或草稿时返回对应终态；其余返回 `None`。
    fn terminal_kind(&self) -> Option<SalesOrderClosureTerminal> {
        if self.close_status == CloseStatus::Closed {
            Some(SalesOrderClosureTerminal::Closed)
        } else if self.commercial_status == CommercialStatus::Voided {
            Some(SalesOrderClosureTerminal::Voided)
        } else if self.commercial_status == CommercialStatus::Draft {
            Some(SalesOrderClosureTerminal::Draft)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::super::entity::{
        CloseStatus, CollectionProgress, CommercialStatus, FulfillmentProgress, InvoiceProgress, SalesOrder,
        SalesOrderData,
    };
    use super::super::types::{BusinessType, OriginSystem};
    use super::{SalesOrderClosureFacts, SalesOrderClosureTerminal, SalesOrderFulfillmentBlocker};
    use crate::ids::{CustomerAccountId, PartyId, SalesOrderId};
    use crate::money::Amount;

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn order(business_type: BusinessType) -> SalesOrder {
        SalesOrder::new(
            SalesOrderId::new("o-1"),
            SalesOrderData {
                order_no: "SO-1".to_string(),
                business_type,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("cust-1"),
                contract_id: None,
                settlement_party_id: PartyId::new("party-1"),
                source_status_code: None,
            },
            "admin-1",
        )
        .unwrap()
    }

    fn facts(order: &SalesOrder) -> SalesOrderClosureFacts {
        order.closure_facts()
    }

    #[test]
    fn closed_is_always_eligible_even_if_progress_lags() {
        let mut order = order(BusinessType::GoodsService);
        order.commercial_status = CommercialStatus::Effective;
        order.close_status = CloseStatus::Closed;
        order.fulfillment_progress = FulfillmentProgress::NotStarted;
        order.collection_progress = CollectionProgress::NotCollected;
        let assessment = facts(&order).assess(true, amt("0.00"), amt("100.00"));
        assert_eq!(assessment.terminal, Some(SalesOrderClosureTerminal::Closed));
        assert!(assessment.eligible_to_close);
        assert!(assessment.fulfillment_complete);
        assert!(assessment.receivable_settled);
        assert!(!assessment.invoice_complete);
        assert!(assessment.fulfillment_blocker.is_none());
    }

    #[test]
    fn voided_and_draft_are_not_eligible() {
        let mut voided = order(BusinessType::GoodsService);
        voided.commercial_status = CommercialStatus::Voided;
        let voided_assessment = facts(&voided).assess(false, amt("0.00"), amt("0.00"));
        assert_eq!(
            voided_assessment.terminal,
            Some(SalesOrderClosureTerminal::Voided)
        );
        assert!(!voided_assessment.eligible_to_close);

        let draft = order(BusinessType::GoodsService);
        let draft_assessment = facts(&draft).assess(false, amt("0.00"), amt("0.00"));
        assert_eq!(draft_assessment.terminal, Some(SalesOrderClosureTerminal::Draft));
        assert!(!draft_assessment.eligible_to_close);
    }

    #[test]
    fn effective_goods_and_voucher_use_distinct_fulfillment_blockers() {
        let mut goods = order(BusinessType::GoodsService);
        goods.commercial_status = CommercialStatus::Effective;
        let goods_assessment = facts(&goods).assess(true, amt("0.00"), amt("100.00"));
        assert_eq!(
            goods_assessment.fulfillment_blocker,
            Some(SalesOrderFulfillmentBlocker::CustomerAcceptance)
        );
        assert!(!goods_assessment.eligible_to_close);

        let mut voucher = order(BusinessType::Voucher);
        voucher.commercial_status = CommercialStatus::Effective;
        let voucher_assessment = facts(&voucher).assess(true, amt("0.00"), amt("100.00"));
        assert_eq!(
            voucher_assessment.fulfillment_blocker,
            Some(SalesOrderFulfillmentBlocker::VoucherExpiry)
        );
    }

    #[test]
    fn missing_accounts_are_not_settled_by_zero_comparison() {
        let mut order = order(BusinessType::GoodsService);
        order.commercial_status = CommercialStatus::Effective;
        order.fulfillment_progress = FulfillmentProgress::Completed;
        order.collection_progress = CollectionProgress::PartiallyCollected;
        let assessment = facts(&order).assess(false, amt("0.00"), amt("0.00"));
        assert!(!assessment.receivable_settled);
        assert!(assessment.collection_unsettled);
        assert!(!assessment.eligible_to_close);
    }

    #[test]
    fn legal_zero_accounts_allow_amount_fallback() {
        let mut order = order(BusinessType::GoodsService);
        order.commercial_status = CommercialStatus::Effective;
        order.fulfillment_progress = FulfillmentProgress::Completed;
        order.collection_progress = CollectionProgress::PartiallyCollected;
        let assessment = facts(&order).assess(true, amt("0.00"), amt("0.00"));
        assert!(assessment.receivable_settled);
        assert!(assessment.eligible_to_close);
    }

    #[test]
    fn amount_fallback_settles_when_totals_match() {
        let mut order = order(BusinessType::GoodsService);
        order.commercial_status = CommercialStatus::Effective;
        order.fulfillment_progress = FulfillmentProgress::Completed;
        order.collection_progress = CollectionProgress::PartiallyCollected;
        let assessment = facts(&order).assess(true, amt("100.00"), amt("100.00"));
        assert!(assessment.receivable_settled);
        assert!(assessment.eligible_to_close);
    }

    #[test]
    fn present_accounts_with_settled_below_gross_are_not_settled() {
        let mut order = order(BusinessType::GoodsService);
        order.commercial_status = CommercialStatus::Effective;
        order.fulfillment_progress = FulfillmentProgress::Completed;
        order.collection_progress = CollectionProgress::PartiallyCollected;
        let assessment = facts(&order).assess(true, amt("50.00"), amt("100.00"));
        assert!(assessment.fulfillment_complete);
        assert!(!assessment.receivable_settled);
        assert!(assessment.collection_unsettled);
        assert!(!assessment.eligible_to_close);
        assert!(assessment.fulfillment_blocker.is_none());
    }

    #[test]
    fn incomplete_fulfillment_blocks_even_when_collection_is_settled() {
        let mut order = order(BusinessType::GoodsService);
        order.commercial_status = CommercialStatus::Effective;
        order.fulfillment_progress = FulfillmentProgress::PartiallyFulfilled;
        order.collection_progress = CollectionProgress::Settled;
        let assessment = facts(&order).assess(true, amt("10.00"), amt("10.00"));
        assert!(assessment.receivable_settled);
        assert!(!assessment.fulfillment_complete);
        assert!(!assessment.eligible_to_close);
    }

    #[test]
    fn invoice_progress_never_blocks_close() {
        let mut order = order(BusinessType::GoodsService);
        order.commercial_status = CommercialStatus::Effective;
        order.fulfillment_progress = FulfillmentProgress::Completed;
        order.collection_progress = CollectionProgress::Settled;
        order.invoice_progress = InvoiceProgress::NotInvoiced;
        let assessment = facts(&order).assess(true, amt("100.00"), amt("100.00"));
        assert!(assessment.eligible_to_close);
        assert!(!assessment.invoice_complete);
    }
}
