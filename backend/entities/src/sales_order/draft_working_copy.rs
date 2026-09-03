//! 首次提交工作副本可编辑事实：只解释销售单聚合内状态。
//!
//! 有效副本是否存在属于仓储事实，不得进入本模块。

use crate::errors::{Error, Result};

use super::entity::{CloseStatus, CommercialStatus, SalesOrder};

impl SalesOrder {
    /// 判断当前销售单是否允许保存或补开首次提交工作副本。
    ///
    /// # 返回
    /// 商业状态为草稿且尚未结案时返回 `true`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 审核中、已生效、已作废或已结案均不可再写首次提交草稿；不查询有效副本。
    pub fn allows_first_submission_working_copy(&self) -> bool {
        self.commercial_status == CommercialStatus::Draft && self.close_status != CloseStatus::Closed
    }

    /// 校验当前销售单允许编辑首次提交工作副本。
    ///
    /// # 返回
    /// 允许时返回 `Ok(())`。
    ///
    /// # 错误
    /// 非草稿或已结案时返回领域错误。
    ///
    /// # 关键业务约束
    /// Service 负责把本错误映射为冲突语义；实体不依赖 Service Error。
    pub fn ensure_first_submission_working_copy_editable(&self) -> Result<()> {
        if self.allows_first_submission_working_copy() {
            Ok(())
        } else {
            Err(Error::from("当前销售单不是草稿，不能保存工作副本"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::entity::{CloseStatus, CommercialStatus, SalesOrder, SalesOrderData};
    use super::super::types::{BusinessType, OriginSystem};
    use super::super::working_copy::{SalesOrderWorkingCopy, SalesOrderWorkingCopyData, WorkingPurpose};
    use super::super::working_copy_test_support::{amt, line_data};
    use crate::common::time::Instant;
    use crate::ids::{CustomerAccountId, PartyId, SalesOrderId};

    fn order() -> SalesOrder {
        SalesOrder::new(
            SalesOrderId::new("o-1"),
            SalesOrderData {
                order_no: "SO-1".to_string(),
                business_type: BusinessType::GoodsService,
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

    fn working_copy() -> SalesOrderWorkingCopy {
        SalesOrderWorkingCopy::new(
            crate::ids::SalesOrderWorkingCopyId::new("wc-1"),
            SalesOrderWorkingCopyData {
                sales_order_id: SalesOrderId::new("o-1"),
                working_purpose: WorkingPurpose::FirstSubmission,
                sales_change_order_id: None,
                base_revision_id: None,
                draft_version: 1,
                content_hash: "draft:wc-1:1".to_string(),
                editor_user_id: "user-1".to_string(),
                business_type: BusinessType::GoodsService,
                customer_id: CustomerAccountId::new("cust-1"),
                contract_id: None,
                contract_revision_id: None,
                settlement_party_id: PartyId::new("party-1"),
                snapshot: crate::sales_order::snapshot::HeaderSnapshotData {
                    customer_name: "东方企业".to_string(),
                    contract_no: None,
                    settlement_party_name: Some("集团结算中心".to_string()),
                    payment_term_code: "NET30".to_string(),
                    payment_term_name: "月结 30 天".to_string(),
                    invoice_type: "增值税专用发票".to_string(),
                    tax_point: "6".to_string(),
                },
                project_name: None,
                business_remark: None,
                voucher_category_sku_id: None,
                voucher_expiry_at: None,
                target_mall_id: None,
                receivable_due_date: None,
                gross_amount: amt("29.97"),
                net_amount: amt("26.07"),
                tax_amount: amt("3.90"),
                lines: vec![line_data(1)],
            },
            "admin-1",
        )
        .unwrap()
    }

    #[test]
    fn draft_allows_first_submission_working_copy() {
        let order = order();
        assert!(order.allows_first_submission_working_copy());
        assert!(order.ensure_first_submission_working_copy_editable().is_ok());
    }

    #[test]
    fn pending_review_effective_voided_and_closed_reject_working_copy() {
        let mut pending = order();
        pending.start_approval_submission("admin-1").unwrap();
        assert_eq!(pending.commercial_status, CommercialStatus::PendingReview);
        assert!(!pending.allows_first_submission_working_copy());

        let mut effective = order();
        effective.start_approval_submission("admin-1").unwrap();
        effective
            .approve(Instant::from_unix_secs(1_800_000_000), "approver")
            .unwrap();
        assert_eq!(effective.commercial_status, CommercialStatus::Effective);
        assert!(!effective.allows_first_submission_working_copy());

        let mut voided = order();
        voided.void("admin-1").unwrap();
        assert_eq!(voided.commercial_status, CommercialStatus::Voided);
        assert!(!voided.allows_first_submission_working_copy());

        let mut closed = order();
        closed.close_status = CloseStatus::Closed;
        assert!(!closed.allows_first_submission_working_copy());
        assert_eq!(
            closed
                .ensure_first_submission_working_copy_editable()
                .unwrap_err()
                .to_string(),
            "当前销售单不是草稿，不能保存工作副本"
        );
    }

    #[test]
    fn working_copy_matches_version_covers_current_and_stale() {
        let copy = working_copy();
        assert!(copy.matches_version(copy.base.version));
        assert!(!copy.matches_version(copy.base.version.saturating_add(1)));
        assert!(!copy.matches_version(0));
    }
}
