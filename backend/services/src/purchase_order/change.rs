//! 采购变更发起、提交、撤回、生效与查询。

mod effect;
mod mapping;
mod query;
mod submit;

#[allow(unused_imports)]
pub(super) use super::{change_adapter, dto};

#[cfg(test)]
use super::change_adapter::{execute_purchase_change_domain_action, start_purchase_change_approval};
#[cfg(test)]
use super::PurchaseOrderService;

#[cfg(test)]
mod tests {
    use super::{
        execute_purchase_change_domain_action, start_purchase_change_approval, PurchaseOrderService,
    };
    use crate::approval::policy::ApprovalDomainAction;
    use entities::ids::{
        PurchaseChangeOrderId, PurchaseChangeSubmissionId, PurchaseOrderId, PurchaseOrderRevisionId,
    };
    use entities::purchase_order::{PurchaseChangeOrder, PurchaseChangeOrderData, PurchaseChangeOrderStatus};

    fn change_source() -> String {
        [
            include_str!("change.rs"),
            include_str!("change/submit.rs"),
            include_str!("change/effect.rs"),
            include_str!("change/query.rs"),
            include_str!("change/mapping.rs"),
        ]
        .concat()
    }

    /// 创建必须注册 BusinessDocument 并独立绑定发布定义。
    #[test]
    fn create_registers_document_and_binds_published_definition() {
        let source = change_source();
        assert!(source.contains("bind_published_definition_on_document_create"));
        assert!(source.contains("new_registered_document"));
        assert!(source.contains("DocumentType::PurchaseChangeOrder"));
        assert!(source.contains("新变更单独立绑定"));
    }

    /// 提交必须锁定单据、递增 approval_subject_version 并调用 start_approval。
    #[test]
    fn submit_calls_start_approval_with_subject_version() {
        let source = change_source();
        assert!(source.contains("start_change_approval"));
        assert!(source.contains("purchase_change_start_command"));
        assert!(source.contains("change.approval_subject_version"));
        assert!(source.contains("prepare_start"));
    }

    /// 最终动作唯一为 apply_effective_change，且绑定当前冻结提交。
    #[test]
    fn final_action_is_apply_effective_change() {
        let source = change_source();
        assert!(source.contains("pub async fn apply_effective_change"));
        assert!(source.contains("change.apply_effective"));
        assert!(source.contains("PurchaseChangeOrderApplyEffectiveChange"));
        assert!(source.contains("submission_id_for_effect"));
    }

    /// 撤回必须调用统一 cancel 并回到草稿。
    #[test]
    fn cancel_uses_unified_port() {
        let source = change_source();
        assert!(source.contains("pub async fn cancel_change_approval"));
        assert!(source.contains("prepare_cancel"));
        assert!(source.contains("adapter.cancel_action"));
    }

    /// 详情必须返回统一审批结构。
    #[test]
    fn detail_returns_unified_approval() {
        let source = change_source();
        assert!(source.contains("document_approval_view"));
        assert!(source.contains("load_change_binding"));
    }

    /// 客户端直接生效必须失败关闭。
    #[test]
    fn client_effect_fails_closed() {
        let error = PurchaseOrderService::reject_client_effect().unwrap_err();
        assert!(error.to_string().contains("客户端不得直接生效"));
    }

    /// 生效写事务必须先推进销售 guard，再重建分配、切换采购版本并同步任务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无；写入顺序和最新采购锁版本返回链路不满足时测试失败。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// guard CAS 必须早于 allocation 重建，任务同步必须晚于采购当前版本更新。
    #[test]
    fn effect_write_serializes_and_rebuilds_procurement_coverage() {
        let source = change_source();
        let body = source
            .split_once("async fn persist_effective_writes")
            .expect("必须存在采购变更生效事务写方法")
            .1;
        let guard = body
            .find("advance_source_sales_procurement_guard")
            .expect("必须推进来源销售 guard");
        let prepare = body
            .find("prepare_current_sales_allocations")
            .expect("必须重建当前销售分配");
        let persist = body
            .find("persist_current_sales_allocations")
            .expect("必须持久化当前销售分配");
        let pointer = body
            .find("order.apply_change_revision")
            .expect("必须通过实体切换采购当前版本");
        let order_update = body
            .find("db.purchase_orders().update")
            .expect("必须 CAS 更新采购单");
        let task_sync = body
            .find("sync_procurement_tasks_for_sales_order")
            .expect("必须同步采购任务");

        assert!(guard < prepare);
        assert!(prepare < persist);
        assert!(persist < pointer);
        assert!(pointer < order_update);
        assert!(order_update < task_sync);
        assert!(body.contains("Ok(order.base.version)"));
        assert!(source.contains("let purchase_order_lock_version = write_effective_change"));
    }

    /// 来源销售 guard helper 必须在同一事务中加载销售单并执行版本 CAS。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无；事务执行器或 guard CAS 缺失时测试失败。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 不得在事务外预读销售单后仅写采购修订，否则有效增减量可能并发越界。
    #[test]
    fn effect_guard_load_and_cas_share_transaction_session() {
        let source = change_source();
        let helper = source
            .split_once("async fn advance_source_sales_procurement_guard")
            .expect("必须存在来源销售 guard helper")
            .1;
        let load = helper
            .find(".find_by_id(&order.sales_order_id, session)")
            .expect("必须在事务内加载来源销售单");
        let advance = helper
            .find("sales_order.advance_procurement_guard(actor_id)")
            .expect("必须推进 procurement guard");
        let update = helper
            .find("db.sales_orders().update(&mut sales_order, session)")
            .expect("必须通过同一事务执行销售单 CAS");

        assert!(load < advance);
        assert!(advance < update);
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
        let optimistic = crate::errors::Error::from(database::Error::OptimisticLockingError);
        assert!(matches!(
            optimistic,
            crate::errors::Error::ConflictError(message)
                if message == "数据已被其他请求修改，请刷新后重试"
        ));

        let transient = crate::errors::Error::from(database::Error::TransientTransactionConflict(
            mongodb::error::Error::custom("write conflict"),
        ));
        assert!(matches!(
            transient,
            crate::errors::Error::TransientTransaction(database::Error::TransientTransactionConflict(_))
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
        change
            .start_approval(PurchaseChangeSubmissionId::new("pcs-current"), "hash-1", "user-1")
            .unwrap();
        assert_eq!(
            change
                .submission_id_for_effect(Some("pcs-current"))
                .unwrap()
                .as_ref(),
            "pcs-current"
        );
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
        assert!(execute_purchase_change_domain_action(
            &mut change,
            ApprovalDomainAction::PurchaseChangeOrderApplyEffectiveChange,
            "user-1",
        )
        .is_err());
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
