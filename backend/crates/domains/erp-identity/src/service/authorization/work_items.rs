//! 工作项办理从真实任务种类选择固定资格及来源，不接受业务调用方拼装政策。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{
    Permission, approval_instance, fulfillment, integration_ops, internal, inventory, legacy_import, payable,
    purchase_order, purchase_payment_request, receivable, returns, sales_inquiry, sales_order, sales_review,
    supplier, supplier_fulfillment, supplier_offering, supplier_portal, supplier_settlement, work_item,
};
use persistence_core::Executor;

use super::targets::{SourceEvidence, TargetFacts};
use super::{AuthorizationService, rules};
use crate::entity::authorization::{AccessContext, AccessTarget, ObjectFacts, RelationKey};
use crate::repository::authorization::AccessProof;
use crate::{Error, QualifiedPermissions, Result};

const DECISION: &[Permission] = &[approval_instance::COPIES_HISTORY, approval_instance::SUBMIT_DECISION];

const REWORK_EXECUTION: &[Permission] = &[work_item::DETAIL];

const PURCHASE_RECEIPT_EXECUTION: &[Permission] = &[
    fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST,
    fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL,
    fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_UPDATE,
    fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_POST,
];
const DELIVERY_EXECUTION: &[Permission] = &[
    fulfillment::DELIVERY_DELIVERY_LIST,
    fulfillment::DELIVERY_DELIVERY_DETAIL,
    fulfillment::DELIVERY_DELIVERY_UPDATE,
    fulfillment::DELIVERY_DELIVERY_POST,
];
const ELECTRONIC_DELIVERY_EXECUTION: &[Permission] = &[
    fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST,
    fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM,
];
const SERVICE_FULFILLMENT_EXECUTION: &[Permission] = &[
    fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST,
    fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM,
];
const CUSTOMER_ACCEPTANCE_EXECUTION: &[Permission] = &[
    fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST,
    fulfillment::ACCEPTANCE_EVIDENCE_CUSTOMER_ACCEPTANCE_EVIDENCE,
    fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE,
    fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT,
    sales_order::EVIDENCE_SALES_ORDER_EVIDENCE_DOWNLOAD,
];
const SUPPLIER_ORDER_INVESTIGATION_EXECUTION: &[Permission] = &[
    supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
    supplier_fulfillment::ORDER_INVESTIGATION,
    supplier_fulfillment::ORDER_TASK_COMPLETION,
];

type TaskRequirements = (&'static str, &'static str, Permission, &'static [Permission]);

// 每个配对分别登记读取权限和执行子集；部分读取权限也是形成前预检的必需资格。
static TASK_REQUIREMENTS: &[TaskRequirements] = &[
    (
        "SALES_INQUIRY_PROCESSING",
        "sales_inquiry",
        sales_inquiry::QUERY_PROCUREMENT_DETAIL,
        &[
            sales_inquiry::QUERY_PROCUREMENT_DETAIL,
            sales_inquiry::COMMANDS_PROCUREMENT_SAVE_FEEDBACK,
            sales_inquiry::COMMANDS_PROCUREMENT_PUBLISH,
        ],
    ),
    (
        "PURCHASE_PAYMENT_ARRANGEMENT",
        "purchase_order",
        purchase_order::DETAIL,
        &[purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT],
    ),
    (
        "SUPPLIER_PORTAL_REVIEW",
        "supplier_portal_request",
        supplier_portal::ADMIN_IMPACTS,
        &[supplier_portal::ADMIN_APPLICATION, supplier_portal::ADMIN_REVIEW],
    ),
    ("PROCUREMENT_ORDER_CREATION", "sales_order", purchase_order::CREATE, &[purchase_order::CREATE]),
    (
        "PURCHASE_RETURN_INVOICE",
        "purchase_order",
        purchase_order::DETAIL,
        &[
            purchase_order::DETAIL,
            supplier::DETAIL,
            payable::PURCHASE_INVOICE_ALLOCATION_LIST,
            receivable::INVOICE_LIST,
            receivable::INVOICE_DETAIL,
            receivable::INVOICE_RED_ISSUE,
        ],
    ),
    (
        "PURCHASE_RETURN_REFUND",
        "purchase_order",
        purchase_order::DETAIL,
        &[
            purchase_order::DETAIL,
            payable::SUPPLIER_PAYMENT_LIST,
            payable::SUPPLIER_PAYMENT_DETAIL,
            returns::DRAFT_UPDATE_SUPPLIER_REFUND_UPDATE,
        ],
    ),
    ("PURCHASE_SOURCE_UPDATE", "purchase_order", purchase_order::DETAIL, &[]),
    (
        "FULFILLMENT_OPERATION",
        "purchase_receipt",
        fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_POST,
        PURCHASE_RECEIPT_EXECUTION,
    ),
    ("FULFILLMENT_OPERATION", "delivery", fulfillment::DELIVERY_DELIVERY_POST, DELIVERY_EXECUTION),
    (
        "FULFILLMENT_OPERATION",
        "electronic_delivery",
        fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM,
        ELECTRONIC_DELIVERY_EXECUTION,
    ),
    (
        "FULFILLMENT_OPERATION",
        "service_fulfillment",
        fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM,
        SERVICE_FULFILLMENT_EXECUTION,
    ),
    ("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order", sales_order::DETAIL, CUSTOMER_ACCEPTANCE_EXECUTION),
    ("DOCUMENT_APPROVAL", "sales_order", sales_order::DETAIL, &[]),
    (
        "SUPPLIER_PAYMENT_EXECUTION",
        "purchase_payment_request",
        purchase_payment_request::DETAIL,
        &[purchase_payment_request::MATERIALS_SUBMISSION, purchase_payment_request::CREDIT_LIST],
    ),
    (
        "SALES_INVOICE_EXECUTION",
        "receivable_account",
        receivable::ACCOUNT_DETAIL,
        &[
            receivable::ACCOUNT_LIST,
            receivable::ACCOUNT_DETAIL,
            receivable::INVOICE_LIST,
            receivable::INVOICE_DETAIL,
            receivable::INVOICE_FILES_INVOICE_CREATE_WITH_FILES,
            receivable::INVOICE_REQUEST_INVOICE_REQUEST_LIST,
            receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL,
            receivable::INVOICE_FILES_INVOICE_COMMIT_WITH_FILES,
        ],
    ),
    (
        "SUPPLIER_SETTLEMENT_REVIEW",
        "supplier_settlement_statement",
        supplier_settlement::STATEMENT_DETAIL,
        &[supplier_settlement::STATEMENT_REVIEW_DECIDE],
    ),
    ("IMPORT_BUSINESS_CONFIRMATION", "LEGACY_IMPORT_BATCH", legacy_import::BATCH_DETAIL, &[]),
    ("INTEGRATION_RESULT_UNKNOWN", "integration_error_task", integration_ops::ERROR_TASK_DETAIL, &[]),
    ("BUSINESS_EXCEPTION", "integration_error_task", integration_ops::ERROR_TASK_DETAIL, &[]),
    ("BUSINESS_EXCEPTION", "reconciliation_difference", integration_ops::DIFFERENCE_DETAIL, &[]),
    ("INTEGRATION_RESULT_UNKNOWN", "reconciliation_difference", integration_ops::DIFFERENCE_DETAIL, &[]),
    (
        "INTEGRATION_RESULT_UNKNOWN",
        "SUPPLIER_FULFILLMENT_ORDER",
        supplier_fulfillment::ORDER_DETAIL,
        SUPPLIER_ORDER_INVESTIGATION_EXECUTION,
    ),
    (
        "BUSINESS_EXCEPTION",
        "SUPPLIER_FULFILLMENT_ORDER",
        supplier_fulfillment::ORDER_DETAIL,
        SUPPLIER_ORDER_INVESTIGATION_EXECUTION,
    ),
    (
        "BUSINESS_EXCEPTION",
        "SUPPLIER_OFFERING",
        supplier_offering::LIST,
        &[supplier_offering::COMPLETE_SUPPLY_EXCEPTION_TASK],
    ),
    ("DOCUMENT_APPROVAL", "voucher_sales_order", sales_order::DETAIL, &[]),
    ("DOCUMENT_APPROVAL", "sales_change_order", sales_review::SALES_CHANGE_ORDER_DETAIL, &[]),
    ("DOCUMENT_APPROVAL", "purchase_order", purchase_order::DETAIL, &[]),
    ("DOCUMENT_APPROVAL", "purchase_change_order", purchase_order::PURCHASE_CHANGE_DETAIL, &[]),
    ("DOCUMENT_APPROVAL", "stock_adjustment", inventory::STOCK_ADJUSTMENT_DETAIL, &[]),
    ("DOCUMENT_APPROVAL", "purchase_payment_request", purchase_payment_request::DETAIL, &[]),
    ("DOCUMENT_APPROVAL", "sales_invoice_request", receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL, &[]),
    ("DOCUMENT_APPROVAL", "customer_receipt", receivable::ACCOUNT_DETAIL, &[]),
    ("DOCUMENT_APPROVAL", "customer_refund", receivable::ACCOUNT_DETAIL, &[]),
    ("DOCUMENT_APPROVAL", "receipt_reversal", receivable::ACCOUNT_DETAIL, &[]),
    ("DOCUMENT_APPROVAL", "supplier_refund", purchase_order::DETAIL, &[]),
    ("DOCUMENT_APPROVAL", "payment_reversal", purchase_order::DETAIL, &[]),
    ("APPROVAL_REWORK", "sales_order", sales_order::DETAIL, REWORK_EXECUTION),
    ("APPROVAL_REWORK", "voucher_sales_order", sales_order::DETAIL, REWORK_EXECUTION),
    ("APPROVAL_REWORK", "sales_change_order", sales_review::SALES_CHANGE_ORDER_DETAIL, REWORK_EXECUTION),
    ("APPROVAL_REWORK", "purchase_order", purchase_order::DETAIL, REWORK_EXECUTION),
    ("APPROVAL_REWORK", "purchase_change_order", purchase_order::PURCHASE_CHANGE_DETAIL, REWORK_EXECUTION),
    ("APPROVAL_REWORK", "stock_adjustment", inventory::STOCK_ADJUSTMENT_DETAIL, REWORK_EXECUTION),
    ("APPROVAL_REWORK", "purchase_payment_request", purchase_payment_request::DETAIL, REWORK_EXECUTION),
    (
        "APPROVAL_REWORK",
        "sales_invoice_request",
        receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL,
        REWORK_EXECUTION,
    ),
    ("APPROVAL_REWORK", "customer_receipt", receivable::ACCOUNT_DETAIL, REWORK_EXECUTION),
    ("APPROVAL_REWORK", "customer_refund", receivable::ACCOUNT_DETAIL, REWORK_EXECUTION),
    ("APPROVAL_REWORK", "receipt_reversal", receivable::ACCOUNT_DETAIL, REWORK_EXECUTION),
    ("APPROVAL_REWORK", "supplier_refund", purchase_order::DETAIL, REWORK_EXECUTION),
    ("APPROVAL_REWORK", "payment_reversal", purchase_order::DETAIL, REWORK_EXECUTION),
];

/// 为履约类精确任务选取固定执行资格；供应商履约仅为订单调查入口返回相应组合。
pub(super) fn fulfillment_task_permissions(permission: &Permission) -> &'static [Permission] {
    match permission.resource() {
        "purchase_receipt" => PURCHASE_RECEIPT_EXECUTION,
        "delivery" => DELIVERY_EXECUTION,
        "electronic_delivery" => ELECTRONIC_DELIVERY_EXECUTION,
        "service_fulfillment" => SERVICE_FULFILLMENT_EXECUTION,
        "customer_acceptance" => CUSTOMER_ACCEPTANCE_EXECUTION,
        "supplier_fulfillment_order" if permission == &supplier_fulfillment::ORDER_INVESTIGATION => {
            SUPPLIER_ORDER_INVESTIGATION_EXECUTION
        },
        _ => &[],
    }
}

/// 工作项引用供应商门户申请时使用的固定详情关联及读审入口。
pub(super) struct PortalRequest {
    // 持久化的门户申请种类，用于选取固定政策。
    kind: &'static str,
    /// 详情查询绑定该种类真实材料的关联键。
    pub(super) detail_key: &'static str,
    /// 读取真实门户材料使用的固定权限。
    pub(super) read: Permission,
    /// 办理对应门户申请使用的固定审核权限。
    pub(super) review: Permission,
}

pub(super) static PORTAL_REQUESTS: &[PortalRequest] = &[
    PortalRequest {
        kind: "offering_create",
        detail_key: "detail_portal_application",
        read: internal::PORTAL_MATERIAL_READ,
        review: internal::PORTAL_MATERIAL_REVIEW_OFFERING_CREATE,
    },
    PortalRequest {
        kind: "offering_update",
        detail_key: "detail_portal_offering",
        read: internal::PORTAL_MATERIAL_READ_OFFERING,
        review: internal::PORTAL_MATERIAL_REVIEW_OFFERING_UPDATE,
    },
    PortalRequest {
        kind: "new_product",
        detail_key: "detail_portal_new_product",
        read: internal::PORTAL_MATERIAL_READ,
        review: internal::PORTAL_MATERIAL_REVIEW_NEW_PRODUCT,
    },
    PortalRequest {
        kind: "cooperation",
        detail_key: "detail_portal_cooperation",
        read: internal::PORTAL_MATERIAL_READ,
        review: internal::PORTAL_MATERIAL_REVIEW_COOPERATION,
    },
];

/// 按真实申请种类查找固定读审政策；未登记时不选择其他入口代替。
pub(super) fn portal_request(kind: &str) -> Option<&'static PortalRequest> {
    PORTAL_REQUESTS.iter().find(|request| request.kind == kind)
}

/// 在读取任务前一次预取相关功能资格；实际组合由入口用途及持久化任务种类和来源决定。
pub(super) fn qualifications(permission: &Permission) -> Vec<Permission> {
    if permission == &internal::WORK_ITEM_EXECUTION_QUALIFICATION {
        return TASK_REQUIREMENTS.iter().flat_map(|(_, _, _, execution)| execution.iter().cloned()).collect();
    }
    if ![internal::WORK_ITEM_EXECUTE, internal::WORK_ITEM_RECIPIENT].contains(permission)
        && !view_permission(permission)
    {
        return vec![];
    }

    let mut required = TASK_REQUIREMENTS
        .iter()
        .flat_map(|(_, _, read, execution)| std::iter::once(read).chain(*execution).cloned())
        .collect::<Vec<_>>();
    required.push(work_item::DETAIL);
    required.extend_from_slice(DECISION);

    if [work_item::DETAIL, internal::WORK_ITEM_EXECUTE, internal::WORK_ITEM_RECIPIENT].contains(permission) {
        required.extend(super::approvals::qualifications(&internal::APPROVAL_MANAGEMENT_READ));
    }
    for permission in PORTAL_REQUESTS.iter().flat_map(|request| [&request.read, &request.review]) {
        required.extend(rules::qualifications(permission));
    }
    required
}

// 按任务真实种类与业务来源配对查找读取和办理资格；未登记组合拒绝。
fn requirements(kind: &str, source: &str) -> Result<&'static TaskRequirements> {
    TASK_REQUIREMENTS
        .iter()
        .find(|(task, resource, _, _)| *task == kind && *resource == source)
        .ok_or_else(|| Error::ValidationError("任务类型或业务对象未登记完整办理资格".into()))
}

/// 识别提供工作项按钮资格的固定列表、详情和管理查询，其他资源不能复用该展示能力。
pub(crate) fn view_permission(permission: &Permission) -> bool {
    [work_item::LIST, work_item::DETAIL, work_item::MANAGE].contains(permission)
}

/// 按真实任务种类和来源，检查同一启用角色是否同时具备读取及执行权限；展示资格不证明目标可办理。
pub(crate) fn execution_qualified(
    kind: &str,
    source: &str,
    qualified: &QualifiedPermissions,
) -> Result<bool> {
    let (_, _, read, execution) = requirements(kind, source)?;
    let mut required = execution.to_vec();
    if !required.contains(read) {
        required.push(read.clone());
    }
    Ok(qualified.same_role(&required))
}

/// 检查同一启用角色是否具备审批历史及提交决定资格；仅供按钮展示，不能代替任务授权。
pub(crate) fn decision_qualified(qualified: &QualifiedPermissions) -> bool {
    qualified.same_role(DECISION)
}

// 任务形成前从服务端候选的种类和来源选择执行子集，拒绝夹带已有任务身份、版本或关联事实。
fn candidate_requirements(object: &ObjectFacts) -> Result<&'static [Permission]> {
    if object.resource != "work_item"
        || !object.id.is_empty()
        || !object.version.is_empty()
        || !object.relations.is_empty()
    {
        return Err(Error::ValidationError("任务资格候选只能提供服务端种类和来源类型".into()));
    }

    let (_, _, _, execution) =
        requirements(object.subject_type.get()?, object.subject_source_resource.get()?)?;
    Ok(execution)
}

impl AuthorizationService {
    /// 生成任务前在同一快照，以服务端候选检查目标账号的同角色执行资格和候选期限，不要求已有任务或指派事实。
    pub(super) async fn require_work_item_qualification(
        &self,
        actor: &AuditActor,
        target: &AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let AccessTarget::Candidate(candidate) = target else {
            return Err(Error::ValidationError("任务形成资格必须使用服务端候选".into()));
        };
        let required = candidate_requirements(candidate)?;
        let boundary =
            executor.boundary().ok_or_else(|| Error::ValidationError("任务资格检查缺少原事务边界".into()))?;

        let qualified = self
            .permissions(actor, required, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("目标账号不存在或已失效".into()))?;
        if !required.is_empty() && !qualified.same_role(required) {
            return Err(Error::Forbidden("目标账号不具备同角色完整任务执行资格".into()));
        }

        if !boundary.is_current(executor) {
            return Err(Error::ValidationError("任务资格检查跨越执行阶段".into()));
        }
        if candidate.valid_until.is_some_and(|until| Instant::now() >= until) {
            return Err(Error::ConflictError("DATA_SCOPE_CHANGED：任务资格候选已失效".into()));
        }
        Ok(())
    }

    /// 实际办理和终态幂等回放沿用同一套来源与功能检查；具体状态和当前指派仍由业务命令核验。
    pub(super) async fn work_item_sources(
        &self,
        proof: &AccessProof,
        qualified: &QualifiedPermissions,
        facts: &TargetFacts,
        executor: &mut dyn Executor,
    ) -> Result<SourceEvidence> {
        if proof.context.permission == work_item::DETAIL {
            return self.work_item_detail_sources(proof, qualified, facts, executor).await;
        }
        if ![internal::WORK_ITEM_EXECUTE, internal::WORK_ITEM_RECIPIENT].contains(&proof.context.permission) {
            return Ok(SourceEvidence::default());
        }

        let mut sources = SourceEvidence::default();
        for object in facts.roots() {
            if object.subject_type.get()? == "DOCUMENT_APPROVAL" {
                requirements("DOCUMENT_APPROVAL", object.subject_source_resource.get()?)?;
                sources.extend(self.approval_task_sources(proof, qualified, object, executor).await?);
                continue;
            }

            if !execution_qualified(
                object.subject_type.get()?,
                object.subject_source_resource.get()?,
                qualified,
            )? {
                return Err(Error::Forbidden("当前账号不具备同角色完整任务办理资格".into()));
            }

            if let Some(resource) = order_resource(object.subject_source_resource.get()?) {
                sources
                    .proofs
                    .push(self.work_item_order(proof, qualified, facts, object, resource, executor).await?);
            }
            if object.subject_source_resource.get()? == "supplier_portal_request" {
                sources
                    .proofs
                    .extend(self.work_item_portal(proof, qualified, facts, object, executor).await?);
            }
        }
        Ok(sources)
    }

    /// 工作项读取真实订单时独立取证其读取范围，并保留规则允许的记录读取补充。
    pub(super) async fn work_item_order(
        &self,
        proof: &AccessProof,
        qualified: &QualifiedPermissions,
        facts: &TargetFacts,
        object: &ObjectFacts,
        resource: &str,
        executor: &mut dyn Executor,
    ) -> Result<AccessProof> {
        let source = exact_source(object, "order_source", resource)?;
        let permission = if resource == "sales_order" { sales_order::DETAIL } else { purchase_order::DETAIL };
        let context = AccessContext { permission: work_item::DETAIL, ..proof.context.clone() };
        let access = self
            .resolve_approval_source(&context, &permission, &[work_item::DETAIL], qualified, executor)
            .await?;
        self.check_work_item_source(&access, facts, source, executor).await?;
        Ok(access)
    }

    // 从持久化申请种类取得固定材料读取及审核入口，分别证明两项资格，调用方不能选择政策。
    async fn work_item_portal(
        &self,
        proof: &AccessProof,
        qualified: &QualifiedPermissions,
        facts: &TargetFacts,
        object: &ObjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccessProof>> {
        let source = exact_source(object, "portal_source", "supplier_portal_material")?;
        let request = portal_request(source.subject_type.get()?)
            .ok_or_else(|| Error::ValidationError("任务的真实供应商申请种类未登记".into()))?;

        let mut sources = Vec::new();
        for permission in [request.read.clone(), request.review.clone()] {
            if !rules::qualified(&permission, qualified) {
                return Err(Error::Forbidden("当前账号不具备任务真实申请的业务资格".into()));
            }
            let context =
                AccessContext { permission, resource: source.resource.clone(), ..proof.context.clone() };
            let access = self.resolve_context(context, None, false, qualified, executor).await?;
            self.check_work_item_source(&access, facts, source, executor).await?;
            sources.push(access);
        }
        Ok(sources)
    }

    /// 加载规则允许的记录补权并检查真实任务来源，再核对来源证明的原阶段与期限，不能用新阶段续期。
    pub(super) async fn check_work_item_source(
        &self,
        proof: &AccessProof,
        facts: &TargetFacts,
        source: &ObjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let grants = self.target_grants(&proof.context, &proof.resolved.rules.root, facts, executor).await?;
        if !proof.resolved.rules.root.allows_object(proof.context.actor.id(), source, &grants)? {
            return Err(Error::Forbidden("任务真实来源不在当前可读取范围内".into()));
        }
        proof.validity.ensure_current(executor)
    }
}

/// 只将已有独立订单读取要求的任务来源归到对应订单资源，不给其他工作项追加来源权限。
pub(super) fn order_resource(source: &str) -> Option<&'static str> {
    match source {
        "sales_order" | "voucher_sales_order" | "sales_change_order" | "delivery" => Some("sales_order"),
        "purchase_order"
        | "purchase_change_order"
        | "purchase_receipt"
        | "electronic_delivery"
        | "service_fulfillment" => Some("purchase_order"),
        _ => None,
    }
}

/// 固定任务来源授权要求恰好一个实际对象，且引用集、资源及对象身份完全吻合。
pub(super) fn exact_source<'a>(
    object: &'a ObjectFacts,
    key: &str,
    resource: &str,
) -> Result<&'a ObjectFacts> {
    let related = object
        .relations
        .get(&RelationKey::parse(key)?)
        .ok_or_else(|| Error::ValidationError("任务缺少完整真实业务来源".into()))?;
    related.validate()?;
    let [source] = related.objects.as_slice() else {
        return Err(Error::ValidationError("任务业务来源必须唯一且完整".into()));
    };
    if related.expected_ids.len() != 1 || source.resource != resource {
        return Err(Error::ValidationError("任务业务来源身份或资源不符".into()));
    }
    Ok(source)
}
