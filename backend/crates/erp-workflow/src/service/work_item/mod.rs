//! D03 人工任务责任查询与责任动作编排。
//!
//! 查询范围由认证身份、RBAC 角色与数据范围形成；客户端不能提交任意责任人或
//! 组织扩大范围。正式业务决定继续由各任务类型的强类型命令与审批运行时完成。

pub mod access;
mod close;
mod finance_responsibility;
mod query_support;
mod reassign;
mod write;

use std::sync::Arc;

use crate::dto::work_item as dto;
use crate::ports::{FailClosedAuditPort, FailClosedObjectFactPort, ObjectFactPort, WorkflowAuditPort};
use crate::repository::WorkItemExt;
use mongodb::Database;

pub use dto::{
    CloseWorkItemRequest, ProcessingBlockerView, ProcessingState, ReassignWorkItemRequest,
    WorkItemAllowedAction, WorkItemApprovalContextView, WorkItemConflict, WorkItemConflictKind,
    WorkItemMutationOutcome, WorkItemPartyView, WorkItemReassignCandidateView, WorkItemScope,
};
pub use finance_responsibility::{
    CreateFinanceResponsibilityRuleRequest, FinanceResponsibilityOwnerOptionView,
    FinanceResponsibilityRuleView, ResolvedFinanceResponsibility, UpdateFinanceResponsibilityRuleRequest,
};
pub use write::{expected_task_version, AuthorizedWorkItem};

#[cfg(test)]
use crate::ports::{ObjectFact, ObjectFactMap, ObjectKind};
#[cfg(test)]
use access::{
    allowed_actions, authorized_fields, authorized_item_fields, detail_scope,
    ensure_generic_work_item_mutation, has_assignment_candidate_access, object_policy, ActorAccess,
    ViewAccess,
};
#[cfg(test)]
use query_support::{
    business_day_bounds_at, counts_as_processable_stat, family_counts_for_types,
    remove_approval_decision_actions, AuthorizedPage, AuthorizedPageCollector, AUTHORIZED_SCAN_BATCH_SIZE,
};
#[cfg(test)]
use reassign::{
    approval_assignment_separated, audited_fact_operator_actors, non_empty_assignment_actors,
    purchase_order_fulfillment_responsibility_id, AssignmentSeparationPolicy,
};

pub type WorkItemFilter = <mongodb::Database as WorkItemExt>::WorkItemFilter;

/// 人工任务责任服务。
#[derive(Clone)]
pub struct WorkItemService<A> {
    pub db: Database,
    pub auth: A,
    pub facts: Arc<dyn ObjectFactPort>,
    pub audit: Arc<dyn WorkflowAuditPort>,
}

impl<A: crate::ports::WorkflowAuthorizationPort + Send + Sync + 'static> WorkItemService<A> {
    /// 创建服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `auth` - 注入的授权 Port
    ///
    /// # 返回
    /// 返回绑定当前应用授权源的服务。
    pub fn new(db: Database, auth: A) -> Self {
        Self::with_ports(
            db,
            auth,
            Arc::new(FailClosedObjectFactPort),
            Arc::new(FailClosedAuditPort),
        )
    }

    /// Create a command service with composition-root ports.
    pub fn with_ports(
        db: Database,
        auth: A,
        facts: Arc<dyn ObjectFactPort>,
        audit: Arc<dyn WorkflowAuditPort>,
    ) -> Self {
        Self {
            db,
            auth,
            facts,
            audit,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        allowed_actions, approval_assignment_separated, audited_fact_operator_actors, authorized_fields,
        authorized_item_fields, business_day_bounds_at, counts_as_processable_stat, detail_scope,
        ensure_generic_work_item_mutation, expected_task_version, family_counts_for_types,
        has_assignment_candidate_access, non_empty_assignment_actors, object_policy,
        purchase_order_fulfillment_responsibility_id, remove_approval_decision_actions, ActorAccess,
        AssignmentSeparationPolicy, AuthorizedPage, AuthorizedPageCollector, ObjectFact, ObjectFactMap,
        ObjectKind, ViewAccess, AUTHORIZED_SCAN_BATCH_SIZE,
    };
    use super::{ProcessingBlockerView, WorkItemAllowedAction, WorkItemScope};
    use crate::entity::work_item::{
        AssignmentSource, DocumentApprovalWorkItemData, WorkItem, WorkItemData, WorkItemPriority,
        WorkItemStatus, WorkItemType,
    };
    use crate::error::{Error, ErrorCode};

    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;

    use std::collections::{HashMap, HashSet};

    /// 验证工作项管理员转交的授权提交栅栏。
    ///
    /// 转交必须以分派授权快照版本执行 policy CAS，不能退回仅在事务内读取比较。
    #[test]
    fn reassign_binds_assignment_authorization_to_commit() {
        let production = include_str!("reassign.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");

        assert!(production.contains("run_authorized_policy_transaction(policy_revision"));
        assert!(production.contains("item.work_item_type.requires_full_execution_permissions()"));
        assert!(!production.contains("ensure_policy_revision(&db, authorization.policy_revision"));
    }

    fn w13_access() -> ActorAccess {
        ActorAccess {
            actor_id: "finance-user".to_string(),
            permissions: vec!["receivable_account:detail".to_string()],
            participant_document_ids: HashSet::from(["sales-order-1".to_string()]),
            organization_ids: vec!["company".to_string()],
            responsibility_scopes: vec![("role-finance".to_string(), Some("company".to_string()))],
            can_manage: false,
        }
    }

    fn audit(
        resource_type: &str,
        resource_id: &str,
        action: &str,
        actor_id: &str,
    ) -> crate::ports::WorkflowAuditFact {
        crate::ports::WorkflowAuditFact::successful(actor_id, action, resource_type, resource_id)
    }

    fn w13_facts() -> ObjectFactMap {
        HashMap::from([(
            (ObjectKind::ReceivableAccount, "account-1".to_string()),
            ObjectFact::new("sales-order-1", "应收子账 2", "sales-user"),
        )])
    }

    fn w13_delta_row() -> crate::repository::WorkItemRow {
        crate::repository::WorkItemRow {
            id: "wi-w13-delta".to_string(),
            work_item_type: WorkItemType::CardFundsDeltaReview,
            approval_node_execution_id: None,
            business_object_type: "receivable_account".to_string(),
            business_object_id: "account-1".to_string(),
            subject_version: "revision-2".to_string(),
            status: WorkItemStatus::Open,

            owner_role: "role-finance".to_string(),
            owner_organization_id: "company".to_string(),
            owner_user_id: Some("alice".to_string()),
            responsibility_actor_ids: Vec::new(),
            assignment_source: AssignmentSource::SystemRule,
            assigned_at: None,
            started_at: None,
            current_assignment_at: None,
            last_activity_at: None,
            priority: WorkItemPriority::High,
            due_at: None,
            reason_code: Some("CARD_FUNDS_DELTA_REVIEW".to_string()),
            impact_summary: Some("同步差额待复核".to_string()),
            completed_at: None,
            completed_by: None,
            closed_at: None,
            closed_by: None,
            close_reason: None,
            version: 1,
            created_at: 100,
            updated_at: 100,
        }
    }

    fn w13_delta_item() -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("wi-w13-delta-detail"),
            WorkItemData {
                work_item_type: WorkItemType::CardFundsDeltaReview,
                business_object_type: "receivable_account".to_string(),
                business_object_id: "account-1".to_string(),
                subject_version: "revision-2".to_string(),

                owner_role: "role-finance".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "alice".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: Some("CARD_FUNDS_DELTA_REVIEW".to_string()),
                impact_summary: Some("同步差额待复核".to_string()),
            },
            Instant::from_unix_secs(100),
        )
        .unwrap()
    }

    fn procurement_access(actor_id: &str) -> ActorAccess {
        ActorAccess {
            actor_id: actor_id.to_string(),
            permissions: vec!["purchase_order:create".to_string()],
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        }
    }

    fn procurement_facts() -> ObjectFactMap {
        HashMap::from([(
            (ObjectKind::SalesOrder, "sales-order-1".to_string()),
            ObjectFact::new("sales-order-1", "销售单 SO-1", "sales-user"),
        )])
    }

    fn procurement_item(owner_user_id: &str) -> WorkItem {
        WorkItem::new_with_responsibility_scope(
            WorkItemId::new("wi-procurement"),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                business_object_type: "sales_order".to_string(),
                business_object_id: "sales-order-1".to_string(),
                subject_version: "submission-1".to_string(),
                owner_role: "role-procurement".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: owner_user_id.to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("SALES_ORDER_EFFECTIVE".to_string()),
                impact_summary: Some("1 行待分配供给".to_string()),
            },
            "sales-lines:digest".to_string(),
            vec!["sales-line-1".to_string()],
        )
        .unwrap()
    }

    fn fulfillment_item(
        business_object_type: &str,
        owner_role: &str,
        responsibility_key: &str,
        reason_code: &str,
    ) -> WorkItem {
        WorkItem::new_with_responsibility_key(
            WorkItemId::new(format!("wi-{business_object_type}")),
            WorkItemData {
                work_item_type: WorkItemType::FulfillmentOperation,
                business_object_type: business_object_type.to_string(),
                business_object_id: format!("{business_object_type}-1"),
                subject_version: "1".to_string(),
                owner_role: owner_role.to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "owner-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some(reason_code.to_string()),
                impact_summary: None,
            },
            responsibility_key,
        )
        .unwrap()
    }

    fn invoice_execution_item(owner_user_id: &str) -> WorkItem {
        WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-invoice-execution"),
            WorkItemData {
                work_item_type: WorkItemType::SalesInvoiceExecution,
                business_object_type: "receivable_account".to_string(),
                business_object_id: "account-invoice-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "role-finance".to_string(),
                owner_organization_id: "party-1".to_string(),
                owner_user_id: owner_user_id.to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("RECEIVABLE_INVOICE_REQUIRED".to_string()),
                impact_summary: Some("待开票金额 ¥100.00".to_string()),
            },
            "finance:SALES_INVOICE:rule-1",
        )
        .unwrap()
    }

    fn invoice_execution_facts() -> ObjectFactMap {
        HashMap::from([(
            (ObjectKind::ReceivableAccount, "account-invoice-1".to_string()),
            ObjectFact::new("sales-order-1", "应收子账 1", "sales-user"),
        )])
    }

    fn invoice_execution_access(actor_id: &str, full: bool) -> ActorAccess {
        let codes: &[&str] = if full {
            &[
                "receivable_account:list",
                "receivable_account:detail",
                "invoice:list",
                "invoice:detail",
                "invoice:create",
                "invoice:post",
            ]
        } else {
            &["receivable_account:detail"]
        };
        ActorAccess {
            actor_id: actor_id.to_string(),
            permissions: codes.iter().map(|code| (*code).to_string()).collect(),
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        }
    }

    fn managed_action_access() -> ActorAccess {
        ActorAccess {
            actor_id: "manager-1".to_string(),
            permissions: ["work_item:reassign", "work_item:close"]
                .into_iter()
                .map(|code| code.to_string())
                .collect(),
            participant_document_ids: HashSet::new(),
            organization_ids: vec!["company".to_string()],
            responsibility_scopes: Vec::new(),
            can_manage: true,
        }
    }

    fn managed_w29_item() -> WorkItem {
        WorkItem::new_at(
            WorkItemId::new("wi-managed-w29"),
            WorkItemData {
                work_item_type: WorkItemType::BusinessException,
                business_object_type: "integration_error_task".to_string(),
                business_object_id: "error-task-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "integration_error_handler".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "worker-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("INTEGRATION_RESULT_UNKNOWN".to_string()),
                impact_summary: None,
            },
            Instant::from_unix_secs(100),
        )
        .unwrap()
    }

    fn managed_w29_fields(has_approval_step: bool) -> super::dto::WorkItemFields {
        let item = managed_w29_item();
        let mut fields = super::dto::WorkItemFields::from(item);
        fields.approval_node_execution_id = has_approval_step.then(|| "approval-execution-1".to_string());
        fields
    }

    fn document_approval_fields_without_execution() -> super::dto::WorkItemFields {
        let item = WorkItem::new_document_approval(
            WorkItemId::new("wi-document-approval"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: bpm::ApprovalNodeExecutionId::new("approval-execution-1"),
                business_object_type: "stock_adjustment".to_string(),
                business_object_id: "adjustment-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "stock_adjustment_approver".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "worker-1".to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(100),
        )
        .unwrap();
        let mut fields = super::dto::WorkItemFields::from(item);
        fields.approval_node_execution_id = None;
        fields
    }

    #[test]
    fn w29_close_registry_excludes_other_business_exception_workspaces() {
        assert!(WorkItemType::IntegrationResultUnknown.is_w29_closable("integration_error_task", false,));
        assert!(WorkItemType::BusinessException.is_w29_closable("reconciliation_difference", false,));
        assert!(!WorkItemType::BusinessException.is_w29_closable("MASTER_MAPPING_TASK", false));
        assert!(WorkItemType::BusinessException
            .brief_relation("MASTER_MAPPING_TASK")
            .is_none());
        assert!(!WorkItemType::BusinessException.is_w29_closable("SUPPLIER_OFFERING", false));
        assert!(!WorkItemType::BusinessException.is_w29_closable("SUPPLIER_FULFILLMENT_ORDER", false,));
        assert!(!WorkItemType::BusinessException.is_w29_closable("integration_error_task", true,));
    }

    #[test]
    fn w29_business_exception_integration_error_object_policy_is_registered() {
        let policy = object_policy(WorkItemType::BusinessException, "integration_error_task").unwrap();

        assert_eq!(policy.object_kind, ObjectKind::IntegrationErrorTask);
        assert_eq!(policy.read_permission, "integration_error_task:detail");
    }

    #[test]
    fn current_owner_does_not_bypass_revoked_object_participation() {
        let mut item = w13_delta_item();
        item.reassign("finance-user", Instant::from_unix_secs(101))
            .unwrap();
        let access = ActorAccess {
            actor_id: "finance-user".to_string(),
            permissions: vec!["receivable_account:detail".to_string()],
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        };

        assert_eq!(item.owner_user_id.as_deref(), Some("finance-user"));
        assert!(authorized_item_fields(item, &access, &w13_facts()).is_none());
    }

    /// 冻结的当前审批人有对象读取权限即可承接节点，陌生人与撤权者仍被拒绝。
    #[test]
    fn assigned_approver_requires_object_read_but_not_creator_participation() {
        let mut item = w13_delta_item();
        item.work_item_type = WorkItemType::DocumentApproval;
        item.business_object_type = "purchase_change_order".into();
        item.business_object_id = "change-1".into();
        item.owner_role = "purchase_change_order_approver".into();
        let facts = HashMap::from([(
            (ObjectKind::PurchaseChangeOrder, "change-1".to_string()),
            ObjectFact::new("change-1", "采购变更单", "buyer"),
        )]);
        let mut access = procurement_access("alice");
        access.permissions = vec!["purchase_change_order:detail".to_string()];
        assert!(authorized_item_fields(item.clone(), &access, &facts).is_some());
        access.actor_id = "stranger".into();
        assert!(authorized_item_fields(item.clone(), &access, &facts).is_none());
        access.actor_id = "alice".into();
        access.permissions.clear();
        assert!(authorized_item_fields(item, &access, &facts).is_none());
    }

    #[test]
    fn procurement_owner_and_reassign_candidate_use_concrete_permission() {
        let owner_access = procurement_access("buyer-1");
        let facts = procurement_facts();
        let item = procurement_item("buyer-1");
        let policy = object_policy(WorkItemType::ProcurementOrderCreation, "sales_order").unwrap();

        assert_eq!(policy.object_kind, ObjectKind::SalesOrder);
        assert_eq!(policy.read_permission, "purchase_order:create");
        assert_eq!(
            WorkItemType::ProcurementOrderCreation.assignment_separation_policy(),
            AssignmentSeparationPolicy::RoleAndParticipation
        );
        let fields = authorized_item_fields(item.clone(), &owner_access, &facts).unwrap();
        assert!(
            allowed_actions(&fields, WorkItemScope::Mine, "buyer-1", &owner_access)
                .contains(&WorkItemAllowedAction::Process)
        );

        let candidate_access = procurement_access("buyer-2");
        assert!(has_assignment_candidate_access(&item, &candidate_access, &facts));
    }

    #[test]
    fn procurement_direct_assignment_still_requires_create_permission() {
        let item = procurement_item("buyer-1");
        let access = ActorAccess {
            actor_id: "buyer-1".to_string(),
            permissions: Vec::new(),
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        };

        assert!(authorized_item_fields(item.clone(), &access, &procurement_facts()).is_none());
        assert!(!has_assignment_candidate_access(
            &item,
            &access,
            &procurement_facts()
        ));
    }

    #[test]
    fn fulfillment_candidate_requires_complete_execution_permissions() {
        let item = fulfillment_item(
            "purchase_receipt",
            "warehouse_inbound_handler",
            "warehouse:wh-1:receipt",
            "PURCHASE_RECEIPT_READY",
        );
        let facts = HashMap::from([(
            (ObjectKind::PurchaseReceipt, "purchase_receipt-1".to_string()),
            ObjectFact::new("po-1", "采购入库单 GRN-1", "__system__"),
        )]);
        let access = |codes: &[&str]| ActorAccess {
            actor_id: "candidate-1".to_string(),
            permissions: codes.iter().map(|code| (*code).to_string()).collect(),
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        };
        assert!(!has_assignment_candidate_access(
            &item,
            &access(&["purchase_receipt:post"]),
            &facts,
        ));
        assert!(has_assignment_candidate_access(
            &item,
            &access(&[
                "purchase_receipt:list",
                "purchase_receipt:detail",
                "purchase_receipt:update",
                "purchase_receipt:post",
            ]),
            &facts,
        ));
    }

    #[test]
    fn customer_acceptance_candidate_requires_complete_execution_permissions() {
        let item = WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-customer-acceptance"),
            WorkItemData {
                work_item_type: WorkItemType::CustomerAcceptanceRegistration,
                business_object_type: "sales_order".to_string(),
                business_object_id: "sales-order-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "sales_order_owner".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "sales-1".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: Some("CUSTOMER_ACCEPTANCE_REQUIRED".to_string()),
                impact_summary: None,
            },
            "sales_order:sales-order-1:customer_acceptance",
        )
        .unwrap();
        let facts = HashMap::from([(
            (ObjectKind::SalesOrder, "sales-order-1".to_string()),
            ObjectFact::new("sales-order-1", "销售单 SO-1", "sales-1"),
        )]);
        let access = |codes: &[&str]| ActorAccess {
            actor_id: "candidate-1".to_string(),
            permissions: codes.iter().map(|code| (*code).to_string()).collect(),
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        };

        assert!(!has_assignment_candidate_access(
            &item,
            &access(&["sales_order:detail"]),
            &facts,
        ));
        assert!(has_assignment_candidate_access(
            &item,
            &access(&[
                "customer_acceptance:list",
                "customer_acceptance:detail",
                "customer_acceptance:create",
                "customer_acceptance:post",
                "sales_order:detail",
            ]),
            &facts,
        ));
    }

    #[test]
    fn invoice_owner_can_view_but_cannot_process_after_execution_permission_is_revoked() {
        let item = invoice_execution_item("finance-1");
        let facts = invoice_execution_facts();
        let revoked = invoice_execution_access("finance-1", false);
        let fields = authorized_item_fields(item.clone(), &revoked, &facts).unwrap();

        assert!(
            !allowed_actions(&fields, WorkItemScope::Mine, "finance-1", &revoked)
                .contains(&WorkItemAllowedAction::Process)
        );
        assert!(!has_assignment_candidate_access(&item, &revoked, &facts));

        let full = invoice_execution_access("finance-1", true);
        let fields = authorized_item_fields(item.clone(), &full, &facts).unwrap();
        assert!(allowed_actions(&fields, WorkItemScope::Mine, "finance-1", &full)
            .contains(&WorkItemAllowedAction::Process));
        assert!(has_assignment_candidate_access(&item, &full, &facts));
    }

    #[test]
    fn fulfillment_reassign_parses_only_registered_responsibility_keys() {
        let procurement = fulfillment_item(
            "delivery",
            "purchase_order_owner",
            "purchase_order:po-1",
            "SUPPLIER_DIRECT_DELIVERY_READY",
        );
        assert_eq!(
            purchase_order_fulfillment_responsibility_id(&procurement).unwrap(),
            Some("po-1".to_string())
        );

        let warehouse = fulfillment_item(
            "delivery",
            "warehouse_outbound_handler",
            "warehouse:wh-1:warehouse_ship",
            "WAREHOUSE_DELIVERY_READY",
        );
        assert_eq!(
            purchase_order_fulfillment_responsibility_id(&warehouse).unwrap(),
            None
        );

        let malformed = fulfillment_item(
            "delivery",
            "purchase_order_owner",
            "warehouse:wh-1:warehouse_ship",
            "SUPPLIER_DIRECT_DELIVERY_READY",
        );
        assert!(purchase_order_fulfillment_responsibility_id(&malformed).is_err());

        let mismatched_reason = fulfillment_item(
            "delivery",
            "purchase_order_owner",
            "purchase_order:po-1",
            "WAREHOUSE_DELIVERY_READY",
        );
        assert!(purchase_order_fulfillment_responsibility_id(&mismatched_reason).is_err());

        let mismatched_object = fulfillment_item(
            "purchase_receipt",
            "purchase_order_owner",
            "purchase_order:po-1",
            "SUPPLIER_DIRECT_DELIVERY_READY",
        );
        assert!(purchase_order_fulfillment_responsibility_id(&mismatched_object).is_err());
    }

    #[test]
    fn business_day_bounds_use_fixed_asia_shanghai_timezone() {
        let (previous_start, previous_end) = business_day_bounds_at(1_722_441_599).unwrap();
        assert_eq!(previous_start.unix_secs(), 1_722_355_200);
        assert_eq!(previous_end.unix_secs(), 1_722_441_600);

        let (start, end) = business_day_bounds_at(1_722_441_600).unwrap();
        assert_eq!(start.unix_secs(), 1_722_441_600);
        assert_eq!(end.unix_secs(), 1_722_528_000);
    }

    #[test]
    fn blocked_items_never_enter_processable_stats() {
        let blocked = ViewAccess::blocked(ProcessingBlockerView {
            code: "APPROVAL_BLOCKED".to_string(),
            message: "审批当前受阻".to_string(),
        });
        assert!(!counts_as_processable_stat(WorkItemScope::Mine, &blocked));

        let mine = ViewAccess::ready(vec![WorkItemAllowedAction::Process]);
        assert!(counts_as_processable_stat(WorkItemScope::Mine, &mine));
        assert!(!counts_as_processable_stat(WorkItemScope::Managed, &mine));
        assert!(!counts_as_processable_stat(WorkItemScope::History, &mine));
    }

    #[test]
    fn approval_tasks_count_even_though_they_never_get_process() {
        // 单据审批任务的动作集只有 View/Approve/Reject，仍必须计入「待我处理」，
        // 否则列表有条目而指标是 0。
        let approval = ViewAccess::ready(vec![
            WorkItemAllowedAction::View,
            WorkItemAllowedAction::Approve,
            WorkItemAllowedAction::Reject,
        ]);
        assert!(counts_as_processable_stat(WorkItemScope::Mine, &approval));

        let view_only = ViewAccess::ready(vec![WorkItemAllowedAction::View]);
        assert!(!counts_as_processable_stat(WorkItemScope::Mine, &view_only));
    }

    #[test]
    fn missing_approval_context_removes_decisions_but_keeps_view() {
        let mut actions = vec![
            WorkItemAllowedAction::View,
            WorkItemAllowedAction::Approve,
            WorkItemAllowedAction::Reject,
        ];

        assert!(remove_approval_decision_actions(&mut actions));
        assert_eq!(actions, vec![WorkItemAllowedAction::View]);
        assert!(!remove_approval_decision_actions(&mut actions));
    }

    #[test]
    fn managed_approval_items_never_project_generic_responsibility_actions() {
        let access = managed_action_access();

        let type_bound = document_approval_fields_without_execution();
        assert_eq!(
            allowed_actions(&type_bound, WorkItemScope::Managed, "manager-1", &access),
            vec![WorkItemAllowedAction::View]
        );

        let execution_bound = managed_w29_fields(true);
        assert_eq!(
            allowed_actions(&execution_bound, WorkItemScope::Managed, "manager-1", &access,),
            vec![WorkItemAllowedAction::View]
        );
    }

    #[test]
    fn managed_non_approval_item_still_projects_reassign_and_close() {
        let access = managed_action_access();
        let item = managed_w29_fields(false);

        assert_eq!(
            allowed_actions(&item, WorkItemScope::Managed, "manager-1", &access),
            vec![
                WorkItemAllowedAction::View,
                WorkItemAllowedAction::Reassign,
                WorkItemAllowedAction::Close,
            ]
        );
    }

    /// 类型标记或 execution 关联任一成立时，新鲜与回放分支都映射同一稳定错误。
    #[test]
    fn approval_generic_mutation_guard_is_stable_for_fresh_and_replay() {
        let mut type_bound = WorkItem::new_document_approval(
            WorkItemId::new("wi-approval-type-only"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: bpm::ApprovalNodeExecutionId::new("approval-execution-1"),
                business_object_type: "stock_adjustment".to_string(),
                business_object_id: "adjustment-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "stock_adjustment_approver".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "worker-1".to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(100),
        )
        .unwrap();
        type_bound.approval_node_execution_id = None;

        let mut execution_bound = managed_w29_item();
        execution_bound.approval_node_execution_id =
            Some(bpm::ApprovalNodeExecutionId::new("approval-execution-2"));

        for item in [&type_bound, &execution_bound] {
            for branch in ["fresh", "replay"] {
                let error =
                    ensure_generic_work_item_mutation(item).expect_err("审批任务的通用责任变更必须失败关闭");
                assert_eq!(
                    error.code(),
                    Some(ErrorCode::ApprovalGenericWorkItemMutationForbidden),
                    "{branch} 分支必须返回相同稳定错误",
                );
            }
        }
        assert!(ensure_generic_work_item_mutation(&managed_w29_item()).is_ok());
    }

    /// Service 必须在查询正式命令回执前识别并拒绝审批任务。
    #[test]
    fn generic_mutation_guard_precedes_idempotent_replay() {
        for (source, method) in [
            (include_str!("reassign.rs"), "pub async fn reassign("),
            (include_str!("close.rs"), "pub async fn close("),
        ] {
            let production = source.split("#[cfg(test)]").next().expect("生产代码必须存在");
            let body = production
                .split_once(method)
                .map(|(_, tail)| tail)
                .expect("通用责任命令必须存在")
                .split_once("\n    }")
                .map(|(body, _)| body)
                .expect("通用责任命令必须闭合");
            let guard = body
                .find("ensure_generic_work_item_mutation(&item)")
                .expect("命令必须先识别审批任务");
            let replay = body
                .find("idempotent_replay(&receipt, &id)")
                .expect("命令必须保留幂等回放");
            assert!(guard < replay, "审批任务守卫必须先于命令回放");
        }
    }

    #[test]
    fn family_counts_use_the_registered_server_mapping() {
        let counts = family_counts_for_types([
            WorkItemType::DocumentApproval,
            WorkItemType::ProcurementOrderCreation,
            WorkItemType::SalesInvoiceExecution,
            WorkItemType::InventoryAdjustmentReview,
            WorkItemType::BusinessException,
            WorkItemType::SupplierPaymentExecution,
        ]);

        assert_eq!(counts.approval, 1);
        assert_eq!(counts.procurement, 1);
        assert_eq!(counts.fulfillment, 1);
        assert_eq!(counts.finance, 2);
        assert_eq!(counts.exception, 1);
    }

    #[test]
    fn former_responsibility_actor_can_open_terminal_history_detail() {
        let mut item = WorkItem::new_at(
            WorkItemId::new("wi-history"),
            WorkItemData {
                work_item_type: WorkItemType::ImportBusinessConfirmation,
                business_object_type: "LEGACY_IMPORT_BATCH".to_string(),
                business_object_id: "batch-1".to_string(),
                subject_version: "v1".to_string(),

                owner_role: "role-sales".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: "alice".to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            Instant::from_unix_secs(100),
        )
        .unwrap();
        item.reassign("bob", Instant::from_unix_secs(110)).unwrap();
        item.complete_by_domain_command("bob", Instant::from_unix_secs(120))
            .unwrap();
        let access = |actor_id: &str| ActorAccess {
            actor_id: actor_id.to_string(),
            permissions: Vec::new(),
            participant_document_ids: HashSet::new(),
            organization_ids: Vec::new(),
            responsibility_scopes: Vec::new(),
            can_manage: false,
        };

        assert_eq!(
            detail_scope(&item, "alice", &access("alice")).unwrap(),
            WorkItemScope::History
        );
        assert!(detail_scope(&item, "charlie", &access("charlie")).is_err());
    }

    #[test]
    fn expected_task_version_accepts_only_positive_integer_strings() {
        assert_eq!(expected_task_version(" 7 ").unwrap(), 7);
        assert!(expected_task_version("0").is_err());
        assert!(expected_task_version("1.0").is_err());
        assert!(expected_task_version("latest").is_err());
    }

    #[test]
    fn authorized_pagination_reaches_later_batch_and_counts_full_total() {
        let access = w13_access();
        let facts = w13_facts();
        let mut collector = AuthorizedPageCollector::new(1, 2).unwrap();
        let first_candidate_batch = (0..AUTHORIZED_SCAN_BATCH_SIZE.get())
            .map(|index| {
                let mut row = w13_delta_row();
                row.work_item_type = WorkItemType::SalesInvoiceExecution;
                row.id = format!("denied-{index}");
                row.business_object_id = format!("missing-{index}");
                row
            })
            .collect();
        let later_candidate_batch = (0..3)
            .map(|index| {
                let mut row = w13_delta_row();
                row.work_item_type = WorkItemType::SalesInvoiceExecution;
                row.id = format!("allowed-{index}");
                row
            })
            .collect();

        collector.extend(authorized_fields(first_candidate_batch, &access, &facts));
        collector.extend(authorized_fields(later_candidate_batch, &access, &facts));

        let page = collector.finish();
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].id, "allowed-0");
        assert_eq!(page.items[1].id, "allowed-1");
        assert_eq!(page.total, 3);
    }

    #[test]
    fn authorized_pagination_slices_after_authorization() {
        let mut collector = AuthorizedPageCollector::new(2, 2).unwrap();

        collector.extend(["authorized-1"]);
        collector.extend(["authorized-2", "authorized-3", "authorized-4"]);

        assert_eq!(
            collector.finish(),
            AuthorizedPage {
                items: vec!["authorized-3", "authorized-4"],
                total: 4,
            }
        );
    }

    #[test]
    fn approval_assignment_excludes_submitter_starter_history_and_decider() {
        let history = vec!["former-owner".to_string()];
        let decided = ["previous-decider"];

        assert!(!approval_assignment_separated(
            "starter",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &decided,
        ));
        assert!(!approval_assignment_separated(
            "submitter",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &decided,
        ));
        assert!(!approval_assignment_separated(
            "former-owner",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &decided,
        ));
        assert!(!approval_assignment_separated(
            "previous-decider",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &decided,
        ));
        assert!(approval_assignment_separated(
            "next-owner",
            "starter",
            "submitter",
            &history,
            None,
            false,
            &decided,
        ));
    }

    #[test]
    fn assignment_postcheck_allows_only_the_new_current_owner() {
        let history = vec!["candidate".to_string()];
        assert!(!approval_assignment_separated(
            "candidate",
            "starter",
            "submitter",
            &history,
            Some("candidate"),
            false,
            &[],
        ));
        assert!(approval_assignment_separated(
            "candidate",
            "starter",
            "submitter",
            &history,
            Some("candidate"),
            true,
            &[],
        ));
        assert!(!approval_assignment_separated(
            "candidate",
            "starter",
            "submitter",
            &history,
            Some("other-owner"),
            true,
            &[],
        ));
    }

    #[test]
    fn formal_decision_task_types_use_fixed_assignment_separation_policies() {
        for work_item_type in [
            WorkItemType::ImportBusinessConfirmation,
            WorkItemType::ImportBusinessConfirmation,
            WorkItemType::PurchaseOrderReview,
            WorkItemType::SalesChangeImpactReview,
            WorkItemType::SalesChangeFinanceReview,
            WorkItemType::CardFundsReview,
            WorkItemType::CardFundsDeltaReview,
            WorkItemType::InventoryAdjustmentReview,
            WorkItemType::SupplierSettlementReview,
        ] {
            assert_eq!(
                work_item_type.assignment_separation_policy(),
                AssignmentSeparationPolicy::DomainActors
            );
        }
        assert_eq!(
            WorkItemType::DocumentApproval.assignment_separation_policy(),
            AssignmentSeparationPolicy::ApprovalHistory
        );
        assert_eq!(
            WorkItemType::FinanceCorrectionReview.assignment_separation_policy(),
            AssignmentSeparationPolicy::FailClosed
        );
        assert_eq!(
            WorkItemType::DocumentApproval.assignment_separation_policy(),
            AssignmentSeparationPolicy::ApprovalHistory
        );
        assert!(object_policy(WorkItemType::DocumentApproval, "stock_adjustment").is_some());
    }

    #[test]
    fn domain_assignment_actor_facts_fail_closed_when_empty() {
        assert!(non_empty_assignment_actors(Vec::new()).is_err());
        assert!(non_empty_assignment_actors(vec!["  ".to_string(), "__system__".to_string()]).is_err());
        assert_eq!(
            non_empty_assignment_actors(vec![" submitter ".to_string(), "submitter".to_string()]).unwrap(),
            HashSet::from(["submitter".to_string()])
        );
    }

    #[test]
    fn card_funds_assignment_excludes_all_receipt_and_invoice_operators() {
        let receipt_ids = HashSet::from(["receipt-1".to_string()]);
        let receipt_audits = vec![
            audit(
                "customer_receipt",
                "receipt-1",
                "customer_receipt.create",
                "receipt-creator",
            ),
            audit(
                "customer_receipt",
                "receipt-1",
                "customer_receipt.post:receipt-1",
                "receipt-poster",
            ),
        ];
        let invoice_ids = HashSet::from(["blue-1".to_string(), "red-1".to_string()]);
        let invoice_audits = vec![
            audit("invoice", "blue-1", "invoice.create", "invoice-creator"),
            audit("invoice", "blue-1", "invoice.post", "invoice-poster"),
            audit("invoice", "red-1", "invoice.red_issue", "red-issuer"),
        ];

        let mut actors = audited_fact_operator_actors(
            "customer_receipt",
            &receipt_ids,
            &receipt_audits,
            &["customer_receipt.create", "customer_receipt.post:"],
            &["customer_receipt.post:"],
        )
        .unwrap();
        actors.extend(
            audited_fact_operator_actors(
                "invoice",
                &invoice_ids,
                &invoice_audits,
                &["invoice.create", "invoice.post", "invoice.red_issue"],
                &["invoice.post", "invoice.red_issue"],
            )
            .unwrap(),
        );

        assert_eq!(
            actors.into_iter().collect::<HashSet<_>>(),
            HashSet::from([
                "receipt-creator".to_string(),
                "receipt-poster".to_string(),
                "invoice-creator".to_string(),
                "invoice-poster".to_string(),
                "red-issuer".to_string(),
            ])
        );
    }

    #[test]
    fn card_funds_assignment_fails_closed_without_formal_audit_for_every_fact() {
        let receipt_ids = HashSet::from(["receipt-1".to_string(), "receipt-2".to_string()]);
        let audits = vec![
            audit(
                "customer_receipt",
                "receipt-1",
                "customer_receipt.post:receipt-1",
                "poster-1",
            ),
            audit(
                "customer_receipt",
                "receipt-2",
                "customer_receipt.create",
                "creator-2",
            ),
        ];

        assert!(matches!(
            audited_fact_operator_actors(
                "customer_receipt",
                &receipt_ids,
                &audits,
                &["customer_receipt.create", "customer_receipt.post:"],
                &["customer_receipt.post:"],
            ),
            Err(Error::Forbidden(_))
        ));
    }
}

#[cfg(test)]
mod retired_review_tests {
    use super::*;
    use crate::entity::work_item::WorkItemType;

    /// 旧任务可解码，但不得通过通用工作台获得业务对象参与权。
    #[test]
    fn retired_review_has_no_object_policy() {
        for kind in [WorkItemType::CardFundsReview, WorkItemType::CardFundsDeltaReview] {
            assert!(object_policy(kind, "receivable_account").is_none());
        }
    }
}
