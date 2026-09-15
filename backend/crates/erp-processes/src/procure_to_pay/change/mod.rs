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

    fn change_source() -> String {
        [
            include_str!("mod.rs"),
            include_str!("submit.rs"),
            include_str!("effect.rs"),
            include_str!("../../../../erp-read-models/src/purchase_center/change/query.rs"),
            include_str!("mapping.rs"),
            include_str!("posting.rs"),
            include_str!("../../../../erp-procurement/src/service/purchase_order/change/effect.rs"),
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
        assert!(source.contains(".apply_effective(self.revision.base.id.clone().into(), actor_id)"));
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

    /// 变更写命令必须在状态、版本、进行中校验之前按来源采购单动作消费同一解析器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 越权请求必须得到统一 NotFound，不得先暴露存在性或状态冲突。
    #[test]
    fn change_write_commands_require_source_access_before_prechecks() {
        let source = include_str!("submit.rs");
        let start = source
            .split("pub async fn start_change(")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn submit_change(").next())
            .expect("必须存在 start_change");
        let submit = source
            .split("pub async fn submit_change(")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn submit_change_view(").next())
            .expect("必须存在 submit_change");
        let cancel = source
            .split("pub async fn cancel_change_approval(")
            .nth(1)
            .and_then(|rest| rest.split("async fn persist_started_change(").next())
            .expect("必须存在 cancel_change_approval");

        let start_access =
            start.find(r#"command_access(actor, "update")"#).expect("发起变更必须先解析 update 动作");
        let start_current = start.find(".current(").expect("发起变更必须先证明来源采购单");
        let start_load = start.find("load_changeable_order").expect("发起变更必须在授权后加载可变更版本");
        let start_in_progress = start.find("ensure_no_in_progress_change").expect("进行中校验必须在授权之后");
        assert!(start_access < start_current);
        assert!(start_current < start_load);
        assert!(start_load < start_in_progress);

        let submit_load = submit.find("load_change(").expect("提交必须先读取变更单以取得来源单");
        let submit_access =
            submit.find(r#"command_access(actor, "submit")"#).expect("提交必须按来源采购单 submit 动作解析");
        let submit_current = submit.find(".current(").expect("提交必须证明来源采购单可见");
        let submit_lock = submit.find("lock_draft_change(").expect("草稿与版本校验必须在授权之后");
        assert!(submit_load < submit_access);
        assert!(submit_access < submit_current);
        assert!(submit_current < submit_lock);

        let cancel_load = cancel.find("load_change(").expect("撤回必须先读取变更单");
        let cancel_access = cancel
            .find(r#"command_access(actor, "cancel_approval")"#)
            .expect("撤回必须按来源采购单 cancel_approval 动作解析");
        let cancel_current = cancel.find(".current(").expect("撤回必须证明来源采购单可见");
        let cancel_version = cancel.find("ensure_expected_version").expect("版本校验必须在授权之后");
        assert!(cancel_load < cancel_access);
        assert!(cancel_access < cancel_current);
        assert!(cancel_current < cancel_version);

        let load = include_str!("../../../../erp-procurement/src/service/purchase_order/change/load.rs");
        let changeable = load
            .split("pub async fn load_changeable_order")
            .nth(1)
            .and_then(|rest| rest.split("pub async fn ensure_no_in_progress_change").next())
            .expect("必须存在 load_changeable_order");
        assert!(!changeable.contains("purchase_orders()"), "可变更加载不得按主键重读未授权采购单");
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
        let error = erp_procurement::service::purchase_order::PurchaseOrderService::reject_client_effect()
            .unwrap_err();
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
        let execute = source.split_once("async fn execute").expect("必须存在真实生产步骤执行函数").1;
        let guard = execute.find("SalesGuard").expect("必须推进销售 guard");
        let prepare = execute.find("PrepareAllocations").expect("必须准备当前分配");
        let revision = execute.find("Revision").expect("必须持久化新版本");
        let current = execute.find("CurrentOrder").expect("必须切当前版本");
        let tasks = execute.find("ProcurementTasks").expect("必须同步任务");
        assert!(guard < prepare && prepare < revision && revision < current && current < tasks);
        assert!(source.contains("purchase.persist_current_order"));
        assert!(source.contains("sync_procurement_tasks_for_sales_order"));
        assert!(source.contains("Ok(steps.write.purchase.order.base.version)"));
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
        let load =
            helper.find(".find_by_id(&order.sales_order_id, session)").expect("必须在事务内加载来源销售单");
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
