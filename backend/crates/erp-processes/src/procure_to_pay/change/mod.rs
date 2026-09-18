//! 采购变更发起、提交、撤回、生效与查询。

mod effect;
mod mapping;
mod posting;
mod submit;

#[cfg(test)]
use erp_procurement::service::purchase_order::change::state::start_purchase_change_approval;

pub(super) use super::change_adapter;
#[cfg(test)]
use super::change_adapter::execute_purchase_change_domain_action;

#[cfg(test)]
mod tests {
    use erp_core::ids::{
        PurchaseChangeOrderId, PurchaseChangeSubmissionId, PurchaseOrderId, PurchaseOrderRevisionId,
    };
    use erp_procurement::entity::purchase_order::{
        PurchaseChangeOrder, PurchaseChangeOrderData, PurchaseChangeOrderStatus,
    };
    use erp_workflow::service::approval::policy::ApprovalDomainAction;

    use super::{execute_purchase_change_domain_action, start_purchase_change_approval};

    /// 客户端直接生效必须失败关闭。
    #[test]
    fn client_effect_fails_closed() {
        let error = erp_procurement::service::purchase_order::PurchaseOrderService::reject_client_effect()
            .unwrap_err();
        assert!(error.to_string().contains("客户端不得直接生效"));
    }

    /// 销售 guard 乐观锁与事务冲突必须映射为稳定 HTTP 409 服务错误。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无；任一并发错误未映射为稳定冲突类型时测试失败。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 采购变更生效不得把数据库 CAS 或 MongoDB 瞬态事务冲突泄露为 500。
    #[test]
    fn effect_concurrency_errors_map_to_stable_conflicts() {
        let optimistic = crate::Error::from(persistence_core::Error::OptimisticLockingError);
        assert!(matches!(
            optimistic,
            crate::Error::ConflictError(message)
                if message == "数据已被其他请求修改，请刷新后重试"
        ));

        let transient = crate::Error::from(persistence_core::Error::TransientTransactionConflict(
            mongodb::error::Error::custom("write conflict"),
        ));
        assert!(matches!(
            transient,
            crate::Error::TransientTransaction(persistence_core::Error::TransientTransactionConflict(_))
        ));
    }

    /// 生效只接受当前冻结提交；错误提交或缺失提交失败关闭。
    #[test]
    fn effect_rejects_mismatched_or_missing_submission() {
        let mut change = PurchaseChangeOrder::new(
            PurchaseChangeOrderId::new("pco-1"),
            PurchaseChangeOrderData {
                purchase_order_id: PurchaseOrderId::new("po-1"),
                base_revision_id: PurchaseOrderRevisionId::new("por-1"),
                reason: "成本上涨".into(),
            },
            "user-1",
        )
        .unwrap();
        assert!(change.submission_id_for_effect(Some("pcs-current")).is_err());
        change.start_approval(PurchaseChangeSubmissionId::new("pcs-current"), "hash-1", "user-1").unwrap();
        assert_eq!(change.submission_id_for_effect(Some("pcs-current")).unwrap().as_ref(), "pcs-current");
        assert!(change.submission_id_for_effect(Some("pcs-old")).is_err());
    }

    /// 非审批中不得走最终通过动作；撤回不回退 subject_version。
    #[test]
    fn cancel_keeps_subject_version_and_effect_requires_in_approval() {
        let mut change = PurchaseChangeOrder::new(
            PurchaseChangeOrderId::new("pco-1"),
            PurchaseChangeOrderData {
                purchase_order_id: PurchaseOrderId::new("po-1"),
                base_revision_id: PurchaseOrderRevisionId::new("por-1"),
                reason: "成本上涨".into(),
            },
            "user-1",
        )
        .expect("草稿必须可构造");
        assert!(
            execute_purchase_change_domain_action(
                &mut change,
                ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange,
                "user-1",
            )
            .is_err()
        );
        start_purchase_change_approval(
            &mut change,
            PurchaseChangeSubmissionId::new("pcs-1"),
            "hash-1",
            "user-1",
        )
        .unwrap();
        assert_eq!(change.approval_subject_version, 1);
        execute_purchase_change_domain_action(
            &mut change,
            ApprovalDomainAction::PurchaseChangeOrderCancelApproval,
            "user-1",
        )
        .unwrap();
        assert_eq!(change.stable.status, PurchaseChangeOrderStatus::Draft);
        assert_eq!(change.approval_subject_version, 1);
    }
}
