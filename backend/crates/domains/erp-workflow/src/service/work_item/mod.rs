//! D03 人工任务责任查询与责任动作编排。
//!
//! 查询范围由认证身份、RBAC 角色与数据范围形成；客户端不能提交任意责任人或
//! 组织扩大范围。正式业务决定继续由各任务类型的强类型命令与审批运行时完成。

mod abandon_rework;
mod authorization;
pub use authorization::WorkItemAuthorization;
use erp_identity::SharedRbacService;

pub mod access;
pub mod approval_rework;
pub use abandon_rework::AbandonApprovalReworkRequest;
mod close;
mod finance_responsibility;
pub mod order_access;
pub mod read;
pub use read::WorkItemReadService;
mod reassign;
mod reassign_apply;
mod supplier_portal;
mod write;

use std::sync::Arc;

#[cfg(test)]
use access::{
    TaskEligibility, candidate_facts_current, ensure_generic_work_item_mutation, task_facts_current,
};
pub use dto::{
    CloseWorkItemRequest, ProcessingBlockerView, ProcessingState, ReassignWorkItemRequest,
    WorkItemAllowedAction, WorkItemApprovalContextView, WorkItemCommandView, WorkItemConflict,
    WorkItemConflictKind, WorkItemMutationOutcome, WorkItemPartyView, WorkItemReassignCandidateView,
    WorkItemScope,
};
pub use finance_responsibility::{
    CreateFinanceResponsibilityRuleRequest, FinanceResponsibilityOwnerOptionView,
    FinanceResponsibilityRuleView, ResolvedFinanceResponsibility, UpdateFinanceResponsibilityRuleRequest,
};
use mongodb::Database;
#[cfg(test)]
use reassign::{
    AssignmentSeparationPolicy, approval_assignment_separated, audited_fact_operator_actors,
    non_empty_assignment_actors, purchase_order_fulfillment_responsibility_id,
};
pub use write::{AuthorizedWorkItem, expected_task_version};

use crate::dto::work_item as dto;
use crate::ports::{
    FailClosedAuditPort, FailClosedObjectFactPort, ObjectFactPort, WorkflowAuditPort,
    WorkflowAuthorizationPort,
};
#[cfg(test)]
use crate::ports::{ObjectFact, ObjectFactMap, ObjectKind};
use crate::repository::WorkItemExt;

pub type WorkItemFilter = <mongodb::Database as WorkItemExt>::WorkItemFilter;

/// 人工任务责任服务。
#[derive(Clone)]
pub struct WorkItemService<A> {
    pub db: Database,
    pub auth: A,
    pub identity: SharedRbacService,
    pub authorization: WorkItemAuthorization,
    pub facts: Arc<dyn ObjectFactPort>,
    pub audit: Arc<dyn WorkflowAuditPort>,
}

impl<A: WorkflowAuthorizationPort> WorkItemService<A> {
    /// 创建服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `rbac` - 与读取入口共享的 identity 服务
    /// * `auth` - 注入的业务授权 Port
    ///
    /// # 返回
    /// 返回绑定当前应用授权源的服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database, rbac: SharedRbacService, auth: A) -> Self {
        Self::with_ports(db, rbac, auth, Arc::new(FailClosedObjectFactPort), Arc::new(FailClosedAuditPort))
    }

    /// 创建注入对象事实与审计端口的任务服务。
    ///
    /// # 参数
    /// * `db` - MongoDB 数据库
    /// * `rbac` - 与读取入口共享的 identity 服务
    /// * `auth` - 注入的业务授权 Port
    /// * `facts` - 对象事实端口
    /// * `audit` - 审计写入端口
    ///
    /// # 返回
    /// 返回使用给定端口的服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_ports(
        db: Database,
        rbac: SharedRbacService,
        auth: A,
        facts: Arc<dyn ObjectFactPort>,
        audit: Arc<dyn WorkflowAuditPort>,
    ) -> Self {
        let authorization = WorkItemAuthorization::new(db.clone(), rbac.clone());
        Self { db, auth, identity: rbac, authorization, facts, audit }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;

    use super::{
        AssignmentSeparationPolicy, ObjectFact, ObjectFactMap, ObjectKind, TaskEligibility,
        WorkItemAllowedAction, WorkItemScope, approval_assignment_separated, audited_fact_operator_actors,
        candidate_facts_current, ensure_generic_work_item_mutation, expected_task_version,
        non_empty_assignment_actors, purchase_order_fulfillment_responsibility_id, task_facts_current,
    };
    use crate::entity::work_item::{
        AssignmentSource, DocumentApprovalWorkItemData, WorkItem, WorkItemData, WorkItemPriority,
        WorkItemType,
    };
    use crate::error::{Error, ErrorCode};

    fn audit(
        resource_type: &str,
        resource_id: &str,
        action: &str,
        actor_id: &str,
    ) -> crate::ports::WorkflowAuditFact {
        crate::ports::WorkflowAuditFact::successful(actor_id, action, resource_type, resource_id)
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

    fn managed_action_access() -> TaskEligibility {
        let mut access = TaskEligibility::new(AuditActor::new(
            "manager-1".to_string(),
            "manager-1".to_string(),
            AccountKind::Admin,
        ));
        access.task.managed_task_ids.insert("wi-managed-w29".into());
        access
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
        assert!(WorkItemType::BusinessException.brief_relation("MASTER_MAPPING_TASK").is_none());
        assert!(!WorkItemType::BusinessException.is_w29_closable("SUPPLIER_OFFERING", false));
        assert!(!WorkItemType::BusinessException.is_w29_closable("SUPPLIER_FULFILLMENT_ORDER", false,));
        assert!(!WorkItemType::BusinessException.is_w29_closable("integration_error_task", true,));
    }

    #[test]
    fn w29_business_exception_integration_error_object_policy_is_registered() {
        let policy = WorkItemType::BusinessException.brief_relation("integration_error_task").unwrap();

        assert_eq!(policy.object_kind, ObjectKind::IntegrationErrorTask);
        assert_eq!(policy.read_permission, "integration_error_task:detail");
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
        assert_eq!(purchase_order_fulfillment_responsibility_id(&warehouse).unwrap(), None);

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
    fn managed_approval_items_never_project_generic_responsibility_actions() {
        let access = managed_action_access();

        let type_bound = document_approval_fields_without_execution();
        assert_eq!(
            type_bound.allowed_actions(WorkItemScope::Managed, "manager-1", &access.task),
            vec![WorkItemAllowedAction::View]
        );

        let execution_bound = managed_w29_fields(true);
        assert_eq!(
            execution_bound.allowed_actions(WorkItemScope::Managed, "manager-1", &access.task),
            vec![WorkItemAllowedAction::View]
        );
    }

    #[test]
    fn managed_non_approval_item_still_projects_reassign_and_close() {
        let access = managed_action_access();
        let item = managed_w29_fields(false);

        assert_eq!(
            item.allowed_actions(WorkItemScope::Managed, "manager-1", &access.task),
            vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Reassign, WorkItemAllowedAction::Close,]
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

    #[test]
    fn authorized_terminal_task_projects_history_without_reauthorizing() {
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
        item.complete_by_domain_command("bob", Instant::from_unix_secs(120)).unwrap();
        assert_eq!(WorkItemScope::for_task(&item, "alice"), WorkItemScope::History);
        assert_eq!(WorkItemScope::for_task(&item, "charlie"), WorkItemScope::History);
    }

    #[test]
    fn expected_task_version_accepts_only_positive_integer_strings() {
        assert_eq!(expected_task_version(" 7 ").unwrap(), 7);
        assert!(expected_task_version("0").is_err());
        assert!(expected_task_version("1.0").is_err());
        assert!(expected_task_version("latest").is_err());
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
        assert!(WorkItemType::DocumentApproval.brief_relation("stock_adjustment").is_some());
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
            audit("customer_receipt", "receipt-1", "customer_receipt.create", "receipt-creator"),
            audit("customer_receipt", "receipt-1", "customer_receipt.post:receipt-1", "receipt-poster"),
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
            audit("customer_receipt", "receipt-1", "customer_receipt.post:receipt-1", "poster-1"),
            audit("customer_receipt", "receipt-2", "customer_receipt.create", "creator-2"),
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
    #[test]
    fn authorized_task_business_facts_require_existing_root_and_current_version() {
        use crate::entity::work_item::WorkItemSubjectVersions;
        let item = procurement_item("buyer-1");
        let mut facts = procurement_facts();
        let fact = facts.values_mut().next().unwrap();
        fact.created_by = "unrelated-creator".into();
        fact.subject_versions = WorkItemSubjectVersions::constrained(vec!["submission-1".into()]).unwrap();
        assert!(task_facts_current(&item, "buyer-1", &facts));
        assert!(candidate_facts_current(&item, &facts));
        facts.values_mut().next().unwrap().subject_versions =
            WorkItemSubjectVersions::constrained(vec!["submission-2".into()]).unwrap();
        assert!(!task_facts_current(&item, "buyer-1", &facts));
        assert!(!candidate_facts_current(&item, &facts));
        assert!(!task_facts_current(&item, "buyer-1", &ObjectFactMap::new()));
    }
    #[test]
    fn closed_procurement_task_cannot_accept_a_new_candidate() {
        let mut item = procurement_item("buyer-1");
        item.status = crate::entity::work_item::WorkItemStatus::Completed;
        assert!(!candidate_facts_current(&item, &procurement_facts()));
    }
}

#[cfg(test)]
mod retired_review_tests {
    use crate::entity::work_item::WorkItemType;

    /// 旧任务可解码，但不得通过通用工作台获得业务对象参与权。
    #[test]
    fn retired_review_has_no_object_policy() {
        for kind in [WorkItemType::CardFundsReview, WorkItemType::CardFundsDeltaReview] {
            assert!(kind.brief_relation("receivable_account").is_none());
        }
    }
}
