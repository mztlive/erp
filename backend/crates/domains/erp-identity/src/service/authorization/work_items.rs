//! 工作项办理从真实任务种类选择固定资格及来源，不接受业务调用方拼装政策。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_permissions::{
    Permission, approval_instance, fulfillment, integration_ops, internal, inventory, legacy_import, payable,
    purchase_order, purchase_payment_request, receivable, returns, sales_inquiry, sales_order, sales_review,
    supplier, supplier_fulfillment, supplier_offering, supplier_portal, supplier_settlement, work_item,
};
use persistence_core::Executor;

use super::targets::TargetFacts;
use super::{AuthorizationService, rules};
use crate::entity::authorization::{AccessContext, AccessTarget, ObjectFacts, RelationKey};
use crate::repository::authorization::AccessProof;
use crate::{Error, QualifiedPermissions, Result};

const DECISION: &[Permission] = &[approval_instance::COPIES_HISTORY, approval_instance::SUBMIT_DECISION];

const READ_PERMISSIONS: &[(&str, &str, Permission)] = &[
    ("SALES_INQUIRY_PROCESSING", "sales_inquiry", sales_inquiry::QUERY_PROCUREMENT_DETAIL),
    ("PURCHASE_PAYMENT_ARRANGEMENT", "purchase_order", purchase_order::DETAIL),
    ("SUPPLIER_PORTAL_REVIEW", "supplier_portal_request", supplier_portal::ADMIN_IMPACTS),
    ("PROCUREMENT_ORDER_CREATION", "sales_order", purchase_order::CREATE),
    ("PURCHASE_RETURN_INVOICE", "purchase_order", purchase_order::DETAIL),
    ("PURCHASE_RETURN_REFUND", "purchase_order", purchase_order::DETAIL),
    ("PURCHASE_SOURCE_UPDATE", "purchase_order", purchase_order::DETAIL),
    ("FULFILLMENT_OPERATION", "purchase_receipt", fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_POST),
    ("FULFILLMENT_OPERATION", "delivery", fulfillment::DELIVERY_DELIVERY_POST),
    (
        "FULFILLMENT_OPERATION",
        "electronic_delivery",
        fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM,
    ),
    (
        "FULFILLMENT_OPERATION",
        "service_fulfillment",
        fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM,
    ),
    ("CUSTOMER_ACCEPTANCE_REGISTRATION", "sales_order", sales_order::DETAIL),
    ("DOCUMENT_APPROVAL", "sales_order", sales_order::DETAIL),
    ("SUPPLIER_PAYMENT_EXECUTION", "purchase_payment_request", purchase_payment_request::DETAIL),
    ("SALES_INVOICE_EXECUTION", "receivable_account", receivable::ACCOUNT_DETAIL),
    ("SUPPLIER_SETTLEMENT_REVIEW", "supplier_settlement_statement", supplier_settlement::STATEMENT_DETAIL),
    ("IMPORT_BUSINESS_CONFIRMATION", "LEGACY_IMPORT_BATCH", legacy_import::BATCH_DETAIL),
    ("INTEGRATION_RESULT_UNKNOWN", "integration_error_task", integration_ops::ERROR_TASK_DETAIL),
    ("BUSINESS_EXCEPTION", "integration_error_task", integration_ops::ERROR_TASK_DETAIL),
    ("BUSINESS_EXCEPTION", "reconciliation_difference", integration_ops::DIFFERENCE_DETAIL),
    ("INTEGRATION_RESULT_UNKNOWN", "reconciliation_difference", integration_ops::DIFFERENCE_DETAIL),
    ("INTEGRATION_RESULT_UNKNOWN", "SUPPLIER_FULFILLMENT_ORDER", supplier_fulfillment::ORDER_DETAIL),
    ("BUSINESS_EXCEPTION", "SUPPLIER_FULFILLMENT_ORDER", supplier_fulfillment::ORDER_DETAIL),
    ("BUSINESS_EXCEPTION", "SUPPLIER_OFFERING", supplier_offering::LIST),
    ("DOCUMENT_APPROVAL", "voucher_sales_order", sales_order::DETAIL),
    ("DOCUMENT_APPROVAL", "sales_change_order", sales_review::SALES_CHANGE_ORDER_DETAIL),
    ("DOCUMENT_APPROVAL", "purchase_order", purchase_order::DETAIL),
    ("DOCUMENT_APPROVAL", "purchase_change_order", purchase_order::PURCHASE_CHANGE_DETAIL),
    ("DOCUMENT_APPROVAL", "stock_adjustment", inventory::STOCK_ADJUSTMENT_DETAIL),
    ("DOCUMENT_APPROVAL", "purchase_payment_request", purchase_payment_request::DETAIL),
    ("DOCUMENT_APPROVAL", "sales_invoice_request", receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL),
    ("DOCUMENT_APPROVAL", "customer_receipt", receivable::ACCOUNT_DETAIL),
    ("DOCUMENT_APPROVAL", "customer_refund", receivable::ACCOUNT_DETAIL),
    ("DOCUMENT_APPROVAL", "receipt_reversal", receivable::ACCOUNT_DETAIL),
    ("DOCUMENT_APPROVAL", "supplier_refund", purchase_order::DETAIL),
    ("DOCUMENT_APPROVAL", "payment_reversal", purchase_order::DETAIL),
    ("APPROVAL_REWORK", "sales_order", sales_order::DETAIL),
    ("APPROVAL_REWORK", "voucher_sales_order", sales_order::DETAIL),
    ("APPROVAL_REWORK", "sales_change_order", sales_review::SALES_CHANGE_ORDER_DETAIL),
    ("APPROVAL_REWORK", "purchase_order", purchase_order::DETAIL),
    ("APPROVAL_REWORK", "purchase_change_order", purchase_order::PURCHASE_CHANGE_DETAIL),
    ("APPROVAL_REWORK", "stock_adjustment", inventory::STOCK_ADJUSTMENT_DETAIL),
    ("APPROVAL_REWORK", "purchase_payment_request", purchase_payment_request::DETAIL),
    ("APPROVAL_REWORK", "sales_invoice_request", receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL),
    ("APPROVAL_REWORK", "customer_receipt", receivable::ACCOUNT_DETAIL),
    ("APPROVAL_REWORK", "customer_refund", receivable::ACCOUNT_DETAIL),
    ("APPROVAL_REWORK", "receipt_reversal", receivable::ACCOUNT_DETAIL),
    ("APPROVAL_REWORK", "supplier_refund", purchase_order::DETAIL),
    ("APPROVAL_REWORK", "payment_reversal", purchase_order::DETAIL),
];

const EXECUTION: &[(&str, &str, &[Permission])] = &[
    (
        "INTEGRATION_RESULT_UNKNOWN",
        "SUPPLIER_FULFILLMENT_ORDER",
        &[
            supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
            supplier_fulfillment::ORDER_INVESTIGATION,
            supplier_fulfillment::ORDER_TASK_COMPLETION,
        ],
    ),
    (
        "BUSINESS_EXCEPTION",
        "SUPPLIER_FULFILLMENT_ORDER",
        &[
            supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
            supplier_fulfillment::ORDER_INVESTIGATION,
            supplier_fulfillment::ORDER_TASK_COMPLETION,
        ],
    ),
    ("BUSINESS_EXCEPTION", "SUPPLIER_OFFERING", &[supplier_offering::COMPLETE_SUPPLY_EXCEPTION_TASK]),
    (
        "SALES_INQUIRY_PROCESSING",
        "sales_inquiry",
        &[
            sales_inquiry::QUERY_PROCUREMENT_DETAIL,
            sales_inquiry::COMMANDS_PROCUREMENT_SAVE_FEEDBACK,
            sales_inquiry::COMMANDS_PROCUREMENT_PUBLISH,
        ],
    ),
    ("PROCUREMENT_ORDER_CREATION", "sales_order", &[purchase_order::CREATE]),
    (
        "PURCHASE_PAYMENT_ARRANGEMENT",
        "purchase_order",
        &[purchase_order::DETAIL, purchase_payment_request::CREATE, purchase_payment_request::SUBMIT],
    ),
    (
        "PURCHASE_RETURN_REFUND",
        "purchase_order",
        &[
            purchase_order::DETAIL,
            payable::SUPPLIER_PAYMENT_LIST,
            payable::SUPPLIER_PAYMENT_DETAIL,
            returns::DRAFT_UPDATE_SUPPLIER_REFUND_UPDATE,
        ],
    ),
    (
        "PURCHASE_RETURN_INVOICE",
        "purchase_order",
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
        "SUPPLIER_PORTAL_REVIEW",
        "supplier_portal_request",
        &[supplier_portal::ADMIN_APPLICATION, supplier_portal::ADMIN_REVIEW],
    ),
    (
        "FULFILLMENT_OPERATION",
        "purchase_receipt",
        &[
            fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST,
            fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL,
            fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_UPDATE,
            fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_POST,
        ],
    ),
    (
        "FULFILLMENT_OPERATION",
        "delivery",
        &[
            fulfillment::DELIVERY_DELIVERY_LIST,
            fulfillment::DELIVERY_DELIVERY_DETAIL,
            fulfillment::DELIVERY_DELIVERY_UPDATE,
            fulfillment::DELIVERY_DELIVERY_POST,
        ],
    ),
    (
        "FULFILLMENT_OPERATION",
        "electronic_delivery",
        &[
            fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST,
            fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM,
        ],
    ),
    (
        "FULFILLMENT_OPERATION",
        "service_fulfillment",
        &[
            fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST,
            fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM,
        ],
    ),
    (
        "CUSTOMER_ACCEPTANCE_REGISTRATION",
        "sales_order",
        &[
            fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST,
            fulfillment::ACCEPTANCE_EVIDENCE_CUSTOMER_ACCEPTANCE_EVIDENCE,
            fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE,
            fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT,
            sales_order::EVIDENCE_SALES_ORDER_EVIDENCE_DOWNLOAD,
        ],
    ),
    (
        "SUPPLIER_PAYMENT_EXECUTION",
        "purchase_payment_request",
        &[purchase_payment_request::MATERIALS_SUBMISSION, purchase_payment_request::CREDIT_LIST],
    ),
    (
        "SALES_INVOICE_EXECUTION",
        "receivable_account",
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
];

/// 资格预取覆盖全部真实种类；实际完整组合在持久化任务加载后确定。
pub(super) fn qualifications(permission: &Permission) -> Vec<Permission> {
    if permission == &internal::WORK_ITEM_EXECUTION_QUALIFICATION {
        return EXECUTION
            .iter()
            .flat_map(|(_, _, required)| required.iter().cloned())
            .chain([work_item::DETAIL])
            .collect();
    }
    if ![internal::WORK_ITEM_EXECUTE, internal::WORK_ITEM_RECIPIENT].contains(permission)
        && !view_permission(permission)
    {
        return vec![];
    }
    let mut required =
        READ_PERMISSIONS.iter().map(|(_, _, permission)| permission.clone()).collect::<Vec<_>>();
    required.extend(EXECUTION.iter().flat_map(|(_, _, required)| required.iter().cloned()));
    required.push(work_item::DETAIL);
    required.extend_from_slice(DECISION);
    for permission in [
        internal::PORTAL_MATERIAL_READ,
        internal::PORTAL_MATERIAL_READ_OFFERING,
        internal::PORTAL_MATERIAL_REVIEW_COOPERATION,
        internal::PORTAL_MATERIAL_REVIEW_OFFERING_CREATE,
        internal::PORTAL_MATERIAL_REVIEW_OFFERING_UPDATE,
        internal::PORTAL_MATERIAL_REVIEW_NEW_PRODUCT,
    ] {
        required.extend(rules::qualifications(&permission));
    }
    required
}

/// 功能组合由真实任务种类与真实对象类型共同确定，未知组合拒绝。
fn requirements(kind: &str, source: &str) -> Result<Vec<Permission>> {
    let read = READ_PERMISSIONS
        .iter()
        .find(|(task, resource, _)| *task == kind && *resource == source)
        .ok_or_else(|| Error::ValidationError("任务类型或业务对象未登记完整办理资格".into()))?;
    let mut required = execution_requirements(kind, source).to_vec();
    required.push(read.2.clone());
    required.sort();
    required.dedup();
    Ok(required)
}

/// 完整任务资格和形成前预检共用执行子集，返工保留既有工作项详情要求。
fn execution_requirements(kind: &str, source: &str) -> &'static [Permission] {
    if kind == "APPROVAL_REWORK" {
        return const { &[work_item::DETAIL] };
    }
    EXECUTION
        .iter()
        .find(|(task, resource, _)| *task == kind && *resource == source)
        .map(|(_, _, required)| *required)
        .unwrap_or_default()
}

/// 同次工作项查询的固定展示入口，不能用于其他资源或权限。
pub(crate) fn view_permission(permission: &Permission) -> bool {
    [work_item::LIST, work_item::DETAIL, work_item::MANAGE].contains(permission)
}

/// 以固定种类和来源复用真实办理的完整同角色组合。
pub(crate) fn execution_qualified(
    kind: &str,
    source: &str,
    qualified: &QualifiedPermissions,
) -> Result<bool> {
    Ok(qualified.same_role(&requirements(kind, source)?))
}

/// 审批按钮只复用原有同角色决定资格，不签发任务办理凭证。
pub(crate) fn decision_qualified(qualified: &QualifiedPermissions) -> bool {
    qualified.same_role(DECISION)
}

/// 任务形成前只证明原有执行资格，不追加读取权限或当前指派条件。
fn candidate_requirements(object: &ObjectFacts) -> Result<&'static [Permission]> {
    if object.resource != "work_item"
        || !object.id.is_empty()
        || !object.version.is_empty()
        || !object.relations.is_empty()
    {
        return Err(Error::ValidationError("任务资格候选只能提供服务端种类和来源类型".into()));
    }
    let kind = object.subject_type.get()?.as_str();
    let source = object.subject_source_resource.get()?.as_str();
    requirements(kind, source)?;
    Ok(execution_requirements(kind, source))
}

impl AuthorizationService {
    /// 在原快照按服务端候选种类核验形成任务所需的完整执行资格。
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

    /// 当前办理和终态回放共用完整资格；具体状态和是否必须当前指派由所属命令核验。
    pub(super) async fn work_item_sources(
        &self,
        proof: &AccessProof,
        facts: &TargetFacts,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccessProof>> {
        if ![internal::WORK_ITEM_EXECUTE, internal::WORK_ITEM_RECIPIENT].contains(&proof.context.permission) {
            return Ok(vec![]);
        }
        let mut sources = Vec::new();
        for object in facts.roots() {
            if !execution_qualified(
                object.subject_type.get()?,
                object.subject_source_resource.get()?,
                &proof.qualified,
            )? {
                return Err(Error::Forbidden("当前账号不具备同角色完整任务办理资格".into()));
            }
            if let Some(resource) = order_resource(object.subject_source_resource.get()?) {
                sources.push(self.work_item_order(proof, facts, object, resource, executor).await?);
            }
            if object.subject_source_resource.get()? == "supplier_portal_request" {
                sources.extend(self.work_item_portal(proof, facts, object, executor).await?);
            }
        }
        Ok(sources)
    }

    /// 保留任务详情入口的独立订单读取范围及显式记录读取补充。
    async fn work_item_order(
        &self,
        proof: &AccessProof,
        facts: &TargetFacts,
        object: &ObjectFacts,
        resource: &str,
        executor: &mut dyn Executor,
    ) -> Result<AccessProof> {
        let source = exact_source(object, "order_source", resource)?;
        let permission = if resource == "sales_order" { sales_order::DETAIL } else { purchase_order::DETAIL };
        let context = AccessContext { permission: work_item::DETAIL, ..proof.context.clone() };
        let access = self
            .resolve_approval_source(&context, &permission, &[work_item::DETAIL], &proof.qualified, executor)
            .await?;
        self.check_work_item_source(&access, facts, source, executor).await?;
        Ok(access)
    }

    /// 按申请持久化种类执行原读取与审核两个独立固定入口，不接受调用方选择材料政策。
    async fn work_item_portal(
        &self,
        proof: &AccessProof,
        facts: &TargetFacts,
        object: &ObjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AccessProof>> {
        let source = exact_source(object, "portal_source", "supplier_portal_material")?;
        let permissions = match source.subject_type.get()?.as_str() {
            "offering_create" => {
                [internal::PORTAL_MATERIAL_READ, internal::PORTAL_MATERIAL_REVIEW_OFFERING_CREATE]
            },
            "offering_update" => {
                [internal::PORTAL_MATERIAL_READ_OFFERING, internal::PORTAL_MATERIAL_REVIEW_OFFERING_UPDATE]
            },
            "new_product" => [internal::PORTAL_MATERIAL_READ, internal::PORTAL_MATERIAL_REVIEW_NEW_PRODUCT],
            "cooperation" => [internal::PORTAL_MATERIAL_READ, internal::PORTAL_MATERIAL_REVIEW_COOPERATION],
            _ => return Err(Error::ValidationError("任务的真实供应商申请种类未登记".into())),
        };
        let mut sources = Vec::new();
        for permission in permissions {
            if !rules::qualified(&permission, &proof.qualified) {
                return Err(Error::Forbidden("当前账号不具备任务真实申请的业务资格".into()));
            }
            let context =
                AccessContext { permission, resource: source.resource.clone(), ..proof.context.clone() };
            let access = self.resolve_context(context, None, false, &proof.qualified, executor).await?;
            self.check_work_item_source(&access, facts, source, executor).await?;
            sources.push(access);
        }
        Ok(sources)
    }

    /// 所有来源和最早到期时点留在同一个原始读取阶段，禁止创建新的授权阶段。
    async fn check_work_item_source(
        &self,
        proof: &AccessProof,
        facts: &TargetFacts,
        source: &ObjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let grants = self.target_grants(&proof.context, &proof.rule, facts, executor).await?;
        if !proof.rule.allows_object(proof.context.actor.id(), source, &grants)? {
            return Err(Error::Forbidden("任务真实来源不在当前可读取范围内".into()));
        }
        proof.ensure_current(executor)
    }
}

/// 只有原任务政策要求独立订单读取的真实业务种类才检查订单来源。
fn order_resource(source: &str) -> Option<&'static str> {
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

/// 固定来源必须恰有一个真实对象，引用集合、身份及资源均必须匹配。
fn exact_source<'a>(object: &'a ObjectFacts, key: &str, resource: &str) -> Result<&'a ObjectFacts> {
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
