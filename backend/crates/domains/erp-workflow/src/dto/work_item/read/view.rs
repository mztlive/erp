use std::ops::{Deref, DerefMut};

use serde::Serialize;

use super::status::{ProcessingBlockerView, ProcessingState, WorkItemAllowedAction};
use crate::dto::work_item::presentation::{
    UNRESOLVED_OWNER_DISPLAY_NAME, next_action_hint, reason_label, usable_impact_summary,
};
use crate::dto::work_item::{WorkItemApprovalContextView, WorkItemPartyView};
use crate::entity::work_item::{AssignmentSource, WorkItem, WorkItemPriority, WorkItemStatus, WorkItemType};
use crate::error::{Error, Result};

/// 受控路由上下文。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct WorkItemRouteContext {
    /// 导入确认范围；不适用时为空。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirmation_scope: Option<String>,
    /// 单据审批的 DocumentType 稳定代码。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub document_type: Option<String>,
}

/// 人工任务队列安全投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemView {
    /// 服务端返回的关键词命中位置。
    pub search_matches: Vec<String>,
    pub id: String,
    pub work_item_type: WorkItemType,
    pub handler_key: String,
    pub destination_workspace_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route_context: Option<WorkItemRouteContext>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_node_execution_id: Option<String>,
    /// 驳回任务绑定的原审批实例；不从当前业务审批推断。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_process_instance_id: Option<String>,
    /// 由审批节点执行和有界实例投影补齐的判断上下文。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub approval_context: Option<WorkItemApprovalContextView>,
    pub status: WorkItemStatus,
    pub assignment_source: AssignmentSource,
    pub owner_role: String,
    pub owner_role_label: String,
    pub owner_organization_id: String,
    pub owner_organization: WorkItemPartyView,
    pub owner_user_id: Option<String>,
    pub owner_user: Option<WorkItemPartyView>,
    pub processing_state: ProcessingState,
    pub processing_blocker: Option<ProcessingBlockerView>,
    pub business_object_type: String,
    pub business_object_id: String,
    /// 业务对象所属的工作面根对象；与任务对象相同时返回同一 ID。
    pub root_business_object_id: String,
    pub next_action_hint: String,
    pub subject_version: String,
    pub task_version: String,
    pub allowed_actions: Vec<WorkItemAllowedAction>,
    pub action_blockers: Vec<ProcessingBlockerView>,
    pub priority: WorkItemPriority,
    pub due_at: Option<u64>,
    pub reason_code: Option<String>,
    pub reason_label: String,
    pub impact_summary: String,
    pub assigned_at: Option<u64>,
    pub started_at: Option<u64>,
    pub current_assignment_at: Option<u64>,
    pub last_activity_at: Option<u64>,
    pub completed_at: Option<u64>,
    pub completed_by: Option<String>,
    pub closed_at: Option<u64>,
    pub closed_by: Option<String>,
    pub close_reason: Option<String>,
    pub created_at: u64,
    pub queue_context_id: String,
}

impl WorkItemView {
    // 设置服务端完成的处理状态与动作判断。
    // 返回写入上述字段后的投影。
    //
    // `processing_blocker`：整体阻断；没有阻断时为 `None`；`allowed_actions`：允许动作。
    pub(crate) fn with_access(
        mut self,
        processing_state: ProcessingState,
        processing_blocker: Option<ProcessingBlockerView>,
        allowed_actions: Vec<WorkItemAllowedAction>,
        action_blockers: Vec<ProcessingBlockerView>,
    ) -> Self {
        self.processing_state = processing_state;
        self.processing_blocker = processing_blocker;
        self.allowed_actions = allowed_actions;
        self.action_blockers = action_blockers;
        self
    }

    // 设置与本任务审批节点严格绑定的运行上下文。
    // `approval_context`：服务层按节点执行批量解析的有界运行事实。
    pub(crate) fn set_approval_context(&mut self, approval_context: WorkItemApprovalContextView) {
        self.approval_context = Some(approval_context);
    }

    // 由已授权字段生成队列安全投影。
    // 返回原因、影响和下一步均已翻译成业务语言的投影；处理人姓名仍可能是占位，由服务层补齐。
    //
    // 任务类型已退役、业务对象或责任角色未登记处理面、单据审批缺少已签署页面映射时返回 `Error::ValidationError`。
    //
    // `fields`：已通过对象授权的任务字段；`queue_context_id`：当前队列上下文。
    pub(crate) fn from_fields(fields: WorkItemFields, queue_context_id: String) -> Result<Self> {
        let WorkItemFields { inner, .. } = fields;
        let route = handler_route(inner.work_item_type, &inner.business_object_type, &inner.owner_role)?;

        Ok(Self {
            search_matches: fields.search_matches,
            id: inner.id,
            work_item_type: inner.work_item_type,
            handler_key: route.handler_key.to_string(),
            destination_workspace_id: route.destination_workspace_id.to_string(),
            route_context: route.route_context,
            approval_node_execution_id: inner.approval_node_execution_id,
            approval_context: None,
            approval_process_instance_id: fields.rework_instance_id,
            status: inner.status,
            assignment_source: inner.assignment_source,
            owner_role_label: role_label(&inner.owner_role),
            owner_role: inner.owner_role,
            owner_organization: WorkItemPartyView {
                id: inner.owner_organization_id.clone(),
                display_name: "责任组织".to_string(),
            },
            owner_organization_id: inner.owner_organization_id,
            owner_user: inner.owner_user_id.as_deref().map(unresolved_owner),
            owner_user_id: inner.owner_user_id,
            processing_state: ProcessingState::Ready,
            processing_blocker: None,
            next_action_hint: next_action_hint(inner.work_item_type),
            business_object_type: inner.business_object_type,
            business_object_id: inner.business_object_id,
            root_business_object_id: inner.root_business_object_id,
            subject_version: inner.subject_version,
            task_version: inner.task_version.to_string(),
            allowed_actions: Vec::new(),
            action_blockers: Vec::new(),
            priority: inner.priority,
            due_at: seconds(inner.due_at),
            reason_label: reason_label(inner.reason_code.as_deref(), inner.work_item_type),
            reason_code: inner.reason_code,
            impact_summary: usable_impact_summary(None, inner.work_item_type),
            assigned_at: seconds(inner.assigned_at),
            started_at: seconds(inner.started_at),
            current_assignment_at: seconds(inner.current_assignment_at),
            last_activity_at: seconds(inner.last_activity_at),
            completed_at: seconds(inner.completed_at),
            completed_by: inner.completed_by,
            closed_at: seconds(inner.closed_at),
            closed_by: inner.closed_by,
            close_reason: inner.close_reason,
            created_at: inner.created_at,
            queue_context_id,
        })
    }
}

fn unresolved_owner(id: &str) -> WorkItemPartyView {
    WorkItemPartyView { id: id.to_owned(), display_name: UNRESOLVED_OWNER_DISPLAY_NAME.to_owned() }
}

/// 已授权任务查询字段与任务内检索结果。
///
/// Identity and status fields are [`crate::dto::work_item::WorkItemFields`].
#[derive(Debug, Clone)]
pub(crate) struct WorkItemFields {
    pub(crate) search: crate::service::work_item::read::search::SearchDocument,
    pub search_matches: Vec<String>,
    inner: crate::dto::work_item::WorkItemFields,
    /// 由持久化责任键提取的原审批实例。
    rework_instance_id: Option<String>,
}

impl WorkItemFields {
    // 本人待办只由持久化的当前负责人和开放状态决定，不使用业务权限或历史参与。
    // 任务开放且当前负责人是该账号时返回 `true`。
    //
    // `actor_id` 为当前已认证账号；字段来自已排除软删除的仓储结果。
    pub(crate) fn is_pending_for(&self, actor_id: &str) -> bool {
        self.status == WorkItemStatus::Open && self.owner_user_id.as_deref() == Some(actor_id)
    }
}

impl Deref for WorkItemFields {
    type Target = crate::dto::work_item::WorkItemFields;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for WorkItemFields {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl From<WorkItem> for WorkItemFields {
    // 详情投影保留驳回返工任务指向的原审批实例；其他任务不借该字段开放实例读取入口。
    fn from(item: WorkItem) -> Self {
        let rework_instance_id = rework_instance_id(item.work_item_type, item.responsibility_key());
        Self {
            inner: item.into(),
            rework_instance_id,
            search: Default::default(),
            search_matches: Vec::new(),
        }
    }
}

impl From<crate::WorkItemRow> for WorkItemFields {
    // 列表从持久化责任键提取返工任务的原审批实例，与详情使用同一来源。
    fn from(item: crate::WorkItemRow) -> Self {
        let rework_instance_id = rework_instance_id(item.work_item_type, item.responsibility_key.as_deref());
        Self {
            inner: item.into(),
            rework_instance_id,
            search: Default::default(),
            search_matches: Vec::new(),
        }
    }
}

// 仅从驳回任务自身的固定关联读取原审批实例。
fn rework_instance_id(work_item_type: WorkItemType, responsibility_key: Option<&str>) -> Option<String> {
    responsibility_key
        .filter(|_| work_item_type == WorkItemType::ApprovalRework)
        .and_then(|key| key.strip_prefix("approval_rework:"))
        .filter(|id| !id.trim().is_empty())
        .map(str::to_owned)
}

/// 从工作台唯一任务路由规则投影处理器和目标工作面。
///
/// 保留该规则对任务类型、业务对象和责任角色的验证及失败分类；不返回路由上下文。
///
/// 成功时返回处理器键和目标工作面 ID。
///
/// 对象未登记、导入确认责任角色未映射、单据审批缺少已签署页面映射或任务类型已退役时返回 `Error::ValidationError`。
///
/// `owner_role`：责任角色码。
pub fn work_item_destination(
    work_item_type: WorkItemType,
    business_object_type: &str,
    owner_role: &str,
) -> Result<(&'static str, &'static str)> {
    let route = handler_route(work_item_type, business_object_type, owner_role)?;
    Ok((route.handler_key, route.destination_workspace_id))
}

pub(crate) struct HandlerRoute {
    pub(super) handler_key: &'static str,
    pub(super) destination_workspace_id: &'static str,
    pub(super) route_context: Option<WorkItemRouteContext>,
}

// 按任务类型、业务对象和责任角色解析处理面与路由上下文。
// 返回处理器键、目标工作面，以及导入确认范围或单据类型路由上下文。其他任务类型的上下文为 `None`。
//
// 对象与固定处理面不符、异常来源未登记、单据审批缺少已签署页面映射、导入确认责任角色未登记或任务类型已退役时返回 `Error::ValidationError`。
//
// `owner_role`：责任角色码；只参与导入确认范围。
pub(crate) fn handler_route(
    work_item_type: WorkItemType,
    business_object_type: &str,
    owner_role: &str,
) -> Result<HandlerRoute> {
    let (handler_key, destination_workspace_id) = handler_destination(work_item_type, business_object_type)?;

    let mut route_context = if work_item_type == WorkItemType::ImportBusinessConfirmation {
        let scope = w18_confirmation_scope(owner_role)
            .ok_or_else(|| Error::ValidationError("IMPORT_CONFIRMATION_SCOPE_UNMAPPED".to_string()))?;
        Some(WorkItemRouteContext { confirmation_scope: Some(scope.to_string()), ..Default::default() })
    } else {
        None
    };
    if matches!(work_item_type, WorkItemType::DocumentApproval | WorkItemType::ApprovalRework) {
        route_context = Some(WorkItemRouteContext {
            document_type: Some(business_object_type.to_string()),
            ..Default::default()
        });
    }

    Ok(HandlerRoute { handler_key, destination_workspace_id, route_context })
}

/// 固定任务处理面的允许对象、处理器、工作面与越界错误。
type FixedRoute = (WorkItemType, &'static [&'static str], &'static str, &'static str, &'static str);

const FIXED_TASK_ROUTES: &[FixedRoute] = &[
    (
        WorkItemType::SalesInquiryProcessing,
        &["sales_inquiry"],
        "sales_inquiry_processing",
        "W08",
        "询价任务业务对象未注册",
    ),
    (
        WorkItemType::PurchasePaymentArrangement,
        &["purchase_order"],
        "purchase_payment_arrangement",
        "W08",
        "付款安排任务业务对象未注册",
    ),
    (
        WorkItemType::PurchaseReturnRefund,
        &["purchase_order"],
        "purchase_return_refund",
        "W12",
        "退货退款任务业务对象未注册",
    ),
    (
        WorkItemType::PurchaseReturnInvoice,
        &["purchase_order"],
        "purchase_return_invoice",
        "W12",
        "退货红票任务业务对象未注册",
    ),
    (
        WorkItemType::SupplierPortalReview,
        &["supplier_portal_request"],
        "supplier_portal_review",
        "W21",
        "供应商申请任务业务对象未注册",
    ),
    (
        WorkItemType::ProcurementOrderCreation,
        &["sales_order"],
        "procurement_order_creation",
        "W08",
        "供给分配任务业务对象未注册",
    ),
    (
        WorkItemType::PurchaseSourceUpdate,
        &["purchase_order"],
        "purchase_source_update",
        "W08",
        "采购依据更新任务业务对象未注册",
    ),
    (
        WorkItemType::FulfillmentOperation,
        &["purchase_receipt", "delivery", "electronic_delivery", "service_fulfillment"],
        "fulfillment_operation",
        "W01",
        "履约任务业务对象未注册",
    ),
    (
        WorkItemType::CustomerAcceptanceRegistration,
        &["sales_order"],
        "customer_acceptance_registration",
        "W06",
        "客户验收任务业务对象未注册",
    ),
    (
        WorkItemType::SupplierPaymentExecution,
        &["purchase_payment_request"],
        "supplier_payment_execution",
        "W12",
        "付款执行任务业务对象未注册",
    ),
    (
        WorkItemType::SalesInvoiceExecution,
        &["receivable_account"],
        "sales_invoice_execution",
        "W11",
        "销项开票执行任务业务对象未注册",
    ),
    (
        WorkItemType::SupplierSettlementReview,
        &["supplier_settlement_statement"],
        "supplier_settlement",
        "W27",
        "WORK_ITEM_HANDLER_UNMAPPED",
    ),
    (
        WorkItemType::ImportBusinessConfirmation,
        &["LEGACY_IMPORT_BATCH"],
        "import_business_confirmation",
        "W18",
        "WORK_ITEM_HANDLER_UNMAPPED",
    ),
];

// 展示任务时根据登记类型选择固定处理面；对象类型未知时保留拒绝原因，不猜业务入口。
fn handler_destination(kind: WorkItemType, object: &str) -> Result<(&'static str, &'static str)> {
    if let Some((_, expected, handler, workspace, error)) = FIXED_TASK_ROUTES.iter().find(|row| row.0 == kind)
    {
        return fixed_destination(object, expected, handler, workspace, error);
    }
    match kind {
        WorkItemType::IntegrationResultUnknown | WorkItemType::BusinessException => {
            exception_destination(kind, object)
        },
        WorkItemType::DocumentApproval => document_approval_route(object),
        WorkItemType::ApprovalRework => {
            document_approval_route(object).map(|(_, workspace)| ("approval_rework", workspace))
        },
        WorkItemType::PurchaseOrderReview
        | WorkItemType::SalesChangeImpactReview
        | WorkItemType::SalesChangeFinanceReview
        | WorkItemType::OwnershipMigrationSalesConfirmation
        | WorkItemType::OwnershipMigrationFinanceConfirmation
        | WorkItemType::InventoryAdjustmentReview
        | WorkItemType::FinanceCorrectionReview => {
            Err(Error::ValidationError("WORK_ITEM_TYPE_RETIRED".into()))
        },
        _ => Err(Error::ValidationError("WORK_ITEM_HANDLER_UNMAPPED".into())),
    }
}

// 处理面只与显式登记的业务对象类型绑定，未知类型不从其他任务字段推断。
fn fixed_destination(
    object: &str,
    expected: &[&str],
    handler: &'static str,
    workspace: &'static str,
    error: &'static str,
) -> Result<(&'static str, &'static str)> {
    if expected.contains(&object) {
        Ok((handler, workspace))
    } else {
        Err(Error::ValidationError(error.into()))
    }
}

// 为集成异常从其真实来源类型选择对应调查处理面，不沿普通审批任务路由。
fn exception_destination(kind: WorkItemType, object: &str) -> Result<(&'static str, &'static str)> {
    match (kind, object) {
        (_, "SUPPLIER_FULFILLMENT_ORDER") => Ok(("supplier_fulfillment_investigation", "W26")),
        (WorkItemType::BusinessException, "SUPPLIER_OFFERING") => Ok(("supplier_supply_exception", "W21")),
        (WorkItemType::IntegrationResultUnknown, "integration_error_task" | "reconciliation_difference") => {
            Ok(("integration_unknown", "W29"))
        },
        (WorkItemType::BusinessException, "integration_error_task" | "reconciliation_difference") => {
            Ok(("business_exception", "W29"))
        },
        _ => Err(Error::ValidationError("WORK_ITEM_HANDLER_UNMAPPED".into())),
    }
}

fn w18_confirmation_scope(owner_role: &str) -> Option<&'static str> {
    match owner_role {
        "role-sales" => Some("SALES"),
        "role-procurement" => Some("PROCUREMENT"),
        "role-operations" => Some("OPERATIONS"),
        "role-warehouse" => Some("WAREHOUSE"),
        "role-finance" => Some("FINANCE"),
        _ => None,
    }
}

// 按已签署页面映射单据审批目标工作面。缺少映射失败关闭，不得回落 W05。
// 返回 handler 与目标 workspace。
//
// 未签署映射时返回稳定错误，不得回落默认工作面。
//
// `business_object_type`：WorkItem 中的 DocumentType 稳定代码。
fn document_approval_route(business_object_type: &str) -> Result<(&'static str, &'static str)> {
    match business_object_type {
        "sales_order" | "voucher_sales_order" | "sales_change_order" => Ok(("document_approval", "W05")),
        "purchase_order" | "purchase_change_order" => Ok(("document_approval", "W08")),
        "stock_adjustment" => Ok(("document_approval", "W10")),
        "sales_invoice_request" | "customer_receipt" | "customer_refund" | "receipt_reversal" => {
            Ok(("document_approval", "W11"))
        },
        "purchase_payment_request" | "supplier_refund" | "payment_reversal" => {
            Ok(("document_approval", "W12"))
        },
        _ => Err(Error::ValidationError("APPROVAL_DOCUMENT_ROUTE_UNMAPPED".to_string())),
    }
}

fn seconds(value: Option<erp_core::common::time::Instant>) -> Option<u64> {
    value.and_then(|instant| u64::try_from(instant.unix_secs()).ok())
}

#[cfg(test)]
mod payment_request_tests {
    use bpm::ids::ApprovalNodeExecutionId;
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;

    use super::*;
    use crate::entity::work_item::{
        DocumentApprovalWorkItemData, SupplierPaymentTaskReason, SupplierPaymentTaskSpec,
        new_approved_purchase_payment_request_task,
    };

    // 验证：每类审批驳回均进入同一个处理器，并保留原单据工作面。
    #[test]
    fn rejected_documents_share_rework_handler_and_preserve_document_routes() {
        let documents = [
            ("sales_order", "W05"),
            ("voucher_sales_order", "W05"),
            ("sales_change_order", "W05"),
            ("purchase_order", "W08"),
            ("purchase_change_order", "W08"),
            ("stock_adjustment", "W10"),
            ("sales_invoice_request", "W11"),
            ("customer_receipt", "W11"),
            ("customer_refund", "W11"),
            ("receipt_reversal", "W11"),
            ("purchase_payment_request", "W12"),
            ("supplier_refund", "W12"),
            ("payment_reversal", "W12"),
        ];
        for (document, workspace) in documents {
            assert_eq!(
                handler_destination(WorkItemType::ApprovalRework, document).unwrap(),
                ("approval_rework", workspace)
            );
            assert!(WorkItemType::ApprovalRework.brief_relation(document).is_some());
        }
        assert!(handler_destination(WorkItemType::ApprovalRework, "unregistered").is_err());
    }

    // 验证：正式付款申请审批任务进入队列时保留精确执行和路由。
    #[test]
    fn payment_request_approval_has_registered_queue_route() {
        let at = Instant::now();
        let fields: WorkItemFields = WorkItem::new_document_approval(
            WorkItemId::new("payment-request-task"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new("payment-request-execution"),
                business_object_type: "purchase_payment_request".into(),
                business_object_id: "payment-request".into(),
                subject_version: "2".into(),
                owner_role: "purchase_payment_request_approver".into(),
                owner_organization_id: "procurement-org".into(),
                owner_user_id: "finance-approver".into(),
                priority: WorkItemPriority::Normal,
                due_at: Some(at),
            },
            at,
        )
        .unwrap()
        .into();
        let view = WorkItemView::from_fields(fields, "finance-queue".into())
            .expect("合法付款申请必须显示在审批队列");
        assert_eq!(view.handler_key, "document_approval");
        assert_eq!(view.destination_workspace_id, "W12");
        assert_eq!(
            view.route_context.as_ref().and_then(|context| context.document_type.as_deref()),
            Some("purchase_payment_request")
        );
        assert_eq!(view.subject_version, "2");
        assert_eq!(view.due_at, seconds(Some(at)));
        assert_eq!(view.approval_node_execution_id.as_deref(), Some("payment-request-execution"));
        assert_eq!(view.processing_state, ProcessingState::Ready);
    }

    // 验证：批准申请产生的真实付款任务在队列中保留申请身份。
    #[test]
    fn approved_payment_request_execution_has_registered_queue_route() {
        let fields = approved_execution_fields();
        let view = WorkItemView::from_fields(fields.clone(), "cashier-queue".into())
            .expect("批准申请的付款执行任务必须显示在出纳队列");
        assert_eq!(view.handler_key, "supplier_payment_execution");
        assert_eq!(view.destination_workspace_id, "W12");
        assert!(view.route_context.is_none());
        assert_eq!(view.business_object_type, "purchase_payment_request");
        assert_eq!(view.business_object_id, "approved-payment-request");
        assert_eq!(view.subject_version, "2");
        assert_eq!(view.due_at, seconds(fields.due_at));
        assert_eq!(view.reason_code.as_deref(), Some("PURCHASE_PAYMENT_REQUEST_APPROVED"));
        assert!(view.approval_node_execution_id.is_none());
        assert_eq!(view.processing_state, ProcessingState::Ready);
    }

    // 验证：旧应付子账付款任务不得绕过批准申请进入办理页面。
    #[test]
    fn legacy_payable_execution_has_no_registered_queue_route() {
        let mut fields = approved_execution_fields();
        fields.business_object_type = "payable_account".into();
        fields.business_object_id = "legacy-payable-account".into();
        let queue = WorkItemView::from_fields(fields, "cashier-queue".into());
        assert!(
            matches!(queue, Err(Error::ValidationError(message)) if message == "付款执行任务业务对象未注册")
        );
    }

    fn approved_execution_fields() -> WorkItemFields {
        new_approved_purchase_payment_request_task(
            WorkItemId::new("approved-payment-request-task"),
            SupplierPaymentTaskSpec {
                account_id: "approved-payment-request".into(),
                subject_version: "2".into(),
                owner_organization_id: "procurement-org".into(),
                owner_user_id: "cashier".into(),
                reason: SupplierPaymentTaskReason::Initial,
                due_at: Instant::now(),
                open_total: "100.00".parse().unwrap(),
            },
            "finance:SUPPLIER_PAYMENT:DEFAULT",
            "approved-payment-request",
            "FK202610100001",
        )
        .expect("正式批准付款申请任务工厂应生成合法身份")
        .into()
    }
}

/// 把责任角色码翻译成界面文案。
///
/// 组织角色与审批责任角色（`approval/policy.rs` 的 `owner_role`）都会落到 `owner_role`
/// 字段。未覆盖的码不得原样上屏——界面禁止展示实现标识符，回退到通用「责任人」。
///
/// 返回中文角色名；未覆盖的码返回「责任人」。
///
/// `role`：责任角色码。
pub fn role_label(role: &str) -> String {
    match role {
        "role-sales" | "sales" => "销售",
        "sales_order_owner" => "负责销售",
        "role-sales-leader" | "sales_leader" => "销售领导",
        "role-procurement" | "procurement" => "采购",
        "supplier_portal_reviewer" => "采购确认人",
        "inquiry_buyer" => "询价采购",
        "role-operations" | "operations" => "运营",
        "role-warehouse" | "warehouse" => "仓储",
        "role-sysadmin" | "sysadmin" | "integration-operator" => "系统管理员",
        "role-cashier" | "cashier" => "出纳",
        "role-invoicer" | "invoicer" => "开票",
        "role-finance" | "finance" => "财务",
        "role-management" | "management" => "管理层",
        "sales_order_approver" => "销售单审批人",
        "voucher_sales_order_approver" => "卡券销售单审批人",
        "sales_change_order_approver" => "销售变更单审批人",
        "purchase_order_approver" => "采购单审批人",
        "purchase_change_order_approver" => "采购变更单审批人",
        "stock_adjustment_approver" => "库存调整单审批人",
        "customer_receipt_approver" => "回款复核人",
        "customer_refund_approver" => "客户退款审批人",
        "supplier_refund_approver" => "供应商退款审批人",
        "receipt_reversal_approver" => "回款冲正审批人",
        "payment_reversal_approver" => "付款冲正审批人",
        "approval_submitter" => "原提交人",
        "approver" => "审批人",
        _ => "责任人",
    }
    .to_string()
}

#[cfg(test)]
mod role_name_tests {
    use super::role_label;

    #[test]
    fn operational_roles_have_readable_labels_and_hide_unknown_codes() {
        assert_eq!(role_label("role-warehouse"), "仓储");
        assert_eq!(role_label("role-sysadmin"), "系统管理员");
        assert_eq!(role_label("role-cashier"), "出纳");
        assert_eq!(role_label("unknown-internal-role"), "责任人");
    }
}
