//! 固定内部对象资格；业务仅提供持久化事实，不能选择政策。
use std::collections::BTreeSet;
use std::sync::LazyLock;

use erp_permissions::{
    admin, approval_instance, bulk_job, catalog, contract, cost, customer, fulfillment, integration_ops,
    internal, organization, payable, purchase_order, purchase_payment_request, receivable, returns,
    sales_inquiry, sales_order, sales_review, sales_selection, supplier, supplier_api, supplier_fulfillment,
    supplier_offering, supplier_settlement, work_item,
};

use crate::entity::authorization::{AccessFact, AccessRule, FactKind, RelationKey, RelationMatch};
use crate::{Permission, QualifiedPermissions};

pub(crate) fn requirements(permission: &Permission) -> Option<Vec<Permission>> {
    if permission == &internal::SALES_INQUIRY_IMPORT_CREATE {
        return Some(vec![
            sales_inquiry::COMMANDS_SALES_CREATE,
            sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW,
            catalog::PRODUCT_SELLABLE_SKU_LIST,
        ]);
    }
    if permission == &internal::SALES_INQUIRY_REINQUIRE_DRAFT {
        return Some(vec![sales_inquiry::COMMANDS_SALES_REINQUIRE, catalog::PRODUCT_SELLABLE_SKU_LIST]);
    }

    if [internal::WORK_ITEM_RECIPIENT, internal::WORK_ITEM_EXECUTION_QUALIFICATION].contains(permission) {
        return Some(vec![]);
    }
    if permission == &internal::WORK_ITEM_EXECUTE {
        return Some(vec![work_item::DETAIL]);
    }
    if permission.resource() == "approval_copy" {
        return Some(vec![]);
    }

    if matches!(
        permission.resource(),
        "supplier_catalog_adoption_preview"
            | "sales_inquiry_import_preview"
            | "supplier_portal_material"
            | "file_asset"
    ) && (matches!(permission.action(), "transfer" | "transfer_owned")
        || permission.resource() != "file_asset")
    {
        return Some(vec![]);
    }
    REQUIREMENTS.iter().find(|(entry, _)| entry == permission).map(|(_, required)| required.clone())
}

static REQUIREMENTS: LazyLock<Vec<(Permission, Vec<Permission>)>> = LazyLock::new(|| {
    vec![
        (internal::CATALOG_BRAND_CREATE_ATTACHMENT_UPLOAD, vec![catalog::PRODUCT_BRAND_CREATE]),
        (internal::CATALOG_BRAND_UPDATE_ATTACHMENT_UPLOAD, vec![catalog::PRODUCT_BRAND_UPDATE]),
        (internal::CATALOG_CREATE_ATTACHMENT_UPLOAD, vec![catalog::PRODUCT_PRODUCT_CREATE]),
        (internal::CATALOG_UPDATE_ATTACHMENT_UPLOAD, vec![catalog::PRODUCT_PRODUCT_UPDATE]),
        (internal::SUPPLIER_CREATE_ATTACHMENT_UPLOAD, vec![supplier::PROFILE_CREATE]),
        (internal::SUPPLIER_UPDATE_ATTACHMENT_UPLOAD, vec![supplier::PROFILE_UPDATE]),
        (
            internal::PURCHASE_EXPENSE_ATTACHMENT_UPLOAD,
            vec![purchase_order::EXPENSE_EXPENSE_OCCURRENCE_CONFIRM],
        ),
        (
            internal::ELECTRONIC_DELIVERY_ATTACHMENT_UPLOAD,
            vec![
                fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST,
                fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM,
            ],
        ),
        (
            internal::SERVICE_FULFILLMENT_ATTACHMENT_UPLOAD,
            vec![
                fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST,
                fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM,
            ],
        ),
        (internal::FINANCE_UPLOAD_CUSTOMER_RECEIPT, vec![receivable::CUSTOMER_RECEIPT_SUBMIT]),
        (internal::FINANCE_UPLOAD_INVOICE_DRAFT, vec![receivable::INVOICE_CREATE]),
        (internal::FINANCE_UPLOAD_INVOICE_POST, vec![receivable::INVOICE_POST]),
        (internal::FINANCE_UPLOAD_PURCHASE_INVOICE, vec![payable::PURCHASE_INVOICE_ALLOCATION_POST]),
        (internal::FINANCE_UPLOAD_EXTRA_COST, vec![cost::ENTRY_CREATE, sales_order::DETAIL]),
        (
            internal::FINANCE_UPLOAD_PURCHASE_PAYMENT,
            vec![purchase_payment_request::CREDIT_LIST, purchase_payment_request::DETAIL],
        ),
        (internal::PROFIT_LOSS_PERIOD_BASIS, vec![cost::ENTRY_LIST]),
        (internal::CUSTOMER_QUALITY_HISTORY, vec![sales_order::LIST]),
        (internal::PURCHASE_DELIVERY_OPTIONS_READ, vec![purchase_order::DETAIL]),
        (internal::CUSTOMER_PROFILE_COMMAND_READ, vec![customer::PROFILE_COMMAND_DETAIL]),
        (internal::CUSTOMER_CREATED_RESULT_READ, vec![customer::CREATE]),
        (internal::SUPPLIER_PROFILE_COMMAND_READ, vec![supplier::PROFILE_COMMAND_DETAIL]),
        (internal::INVOICE_REQUEST_AMOUNTS_READ, vec![receivable::INVOICE_REQUEST_INVOICE_REQUEST_AMOUNTS]),
        (internal::CONTRACT_IMPORT, vec![contract::CREATE]),
        (internal::CONTRACT_SALES_ACCESS, vec![contract::TERMINATE]),
        (internal::CATALOG_ADOPTION, vec![supplier_api::ADOPTIONS_PREVIEW]),
        (
            internal::ORGANIZATION_PERSON_PROFILE_CHANGE,
            vec![organization::CHANGE, admin::ACCOUNT_UPDATE_ADMIN],
        ),
        (internal::SETTLEMENT_REFRESH_PREPARED, vec![supplier_settlement::STATEMENT_REFRESH]),
        (internal::SETTLEMENT_VOID_PREPARED, vec![supplier_settlement::STATEMENT_REFRESH]),
        (internal::SETTLEMENT_RESOLVE_DIFFERENCE, vec![supplier_settlement::STATEMENT_REFRESH]),
        (internal::SALES_CREATED_RESULT_READ, vec![sales_order::CREATE]),
        (internal::SALES_CHANGE_DRAFT_READ, vec![sales_order::HANDOVER]),
        (internal::SALES_CHANGE_APPLIED_RESULT_READ, vec![sales_order::DETAIL]),
        (internal::PURCHASE_CHANGE_SUBMITTED_RESULT_READ, vec![purchase_order::SUBMIT]),
        (internal::PURCHASE_CHANGE_VOIDED_RESULT_READ, vec![purchase_order::SAVE_DRAFT]),
        (internal::SALES_CREATE_AND_SUBMIT, vec![sales_order::CREATE, sales_order::SUBMIT]),
        (internal::SALES_ORDER_EXTRA_COST_READ, vec![sales_order::DETAIL]),
        (
            internal::SUPPLIER_FULFILLMENT_COMPLETED_RESULT_READ,
            vec![
                supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
                supplier_fulfillment::ORDER_INVESTIGATION,
                supplier_fulfillment::ORDER_TASK_COMPLETION,
            ],
        ),
        (internal::PURCHASE_CREATED_RESULT_READ, vec![purchase_order::CREATE]),
        (internal::SOURCING_CREATED_RESULT_READ, vec![purchase_order::CREATE]),
        (internal::INVOICE_POSTED_RESULT_READ, vec![receivable::INVOICE_POST]),
        (internal::PURCHASE_CHANGE_SOURCE, vec![purchase_order::SAVE_DRAFT, purchase_order::SUBMIT]),
        (internal::PURCHASE_CHANGE_CREATE_SOURCE, vec![purchase_order::SAVE_DRAFT]),
        (internal::PURCHASE_CHANGE_SUBMIT_SOURCE, vec![purchase_order::SUBMIT]),
        (internal::PURCHASE_CHANGE_CANCEL_SOURCE, vec![purchase_order::CANCEL_APPROVAL]),
        (internal::PURCHASE_CHANGE_VOID_SOURCE, vec![purchase_order::SAVE_DRAFT]),
        (internal::PURCHASE_EXPENSE_CONFIRM_SOURCE, vec![purchase_order::SAVE_DRAFT]),
        (internal::PURCHASE_CREATE_AND_SUBMIT, vec![purchase_order::CREATE, purchase_order::SUBMIT]),
        (internal::PURCHASE_PORTAL_IMPACT_READ, vec![purchase_order::DETAIL, purchase_order::LIST]),
        (internal::SALES_ORDER_PROFIT_LOSS_READ, vec![sales_order::LIST, cost::ENTRY_LIST]),
        (internal::APPROVAL_MANAGEMENT_READ, vec![approval_instance::INSTANCE_LIST]),
        (internal::APPROVAL_INSTANCE_READ, vec![approval_instance::INSTANCE_LIST]),
        (internal::APPROVAL_STARTED_LIST, vec![approval_instance::INSTANCE_LIST]),
        (
            internal::APPROVAL_MINE_LIST,
            vec![approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION],
        ),
        (
            internal::APPROVAL_RECOVER,
            vec![approval_instance::RESUME_CURRENT_APPROVER, approval_instance::INSTANCE_LIST],
        ),
        (internal::CUSTOMER_QUALITY_CUSTOMERS, vec![customer::LIST]),
        (internal::CUSTOMER_QUALITY_CURRENT, vec![sales_order::LIST]),
        (
            internal::SALES_INQUIRY_TEMPLATE_PRODUCTS,
            vec![sales_inquiry::COMMANDS_SALES_CREATE, catalog::PRODUCT_SELLABLE_SKU_LIST],
        ),
        (internal::DELIVERY_TEMPLATE_READ, vec![supplier_offering::LIST]),
        (internal::DELIVERY_TEMPLATE_WRITE, vec![supplier_offering::REVISE]),
        (internal::SUPPLIER_OFFERING_AVAILABILITY_UPDATE, vec![supplier_offering::REVISE]),
        (internal::SUPPLIER_OFFERING_CREATED_RESULT_READ, vec![supplier_offering::CREATE]),
        (internal::SALES_SELECTION_ORDER_BASIS, vec![sales_selection::PROPOSAL_DETAIL]),
        (
            internal::PURCHASE_RETURN_SOURCE,
            vec![purchase_order::SAVE_DRAFT, returns::PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE],
        ),
        (internal::SALES_ORDER_FINANCIAL_EXPENSE_SOURCE, vec![cost::ENTRY_CREATE, sales_order::DETAIL]),
        (internal::PAYMENT_REQUEST_CANDIDATES, vec![purchase_payment_request::CREATE]),
        (internal::PRODUCT_IMPORT_JOB_READ, vec![catalog::PRODUCT_PRODUCT_LIST]),
        (internal::PRODUCT_IMPORT_SUBMIT, vec![catalog::PRODUCT_PRODUCT_CREATE]),
        (internal::SUPPLIER_IMPORT_JOB_READ, vec![supplier::IMPORT_SUPPLIER_IMPORT]),
        (internal::SUPPLIER_IMPORT_SUBMIT, vec![supplier::IMPORT_SUPPLIER_IMPORT]),
        (internal::BRAND_IMPORT_JOB_READ, vec![catalog::PRODUCT_BRAND_CREATE]),
        (internal::DATA_TRANSFER_JOB_READ, vec![]),
        (internal::PURCHASE_EXPENSE_SOURCE, vec![purchase_order::CREATE]),
        (internal::SALES_ORDER_HANDOVER_RECIPIENT, vec![sales_order::DETAIL]),
        (internal::CUSTOMER_SELECT, vec![customer::DETAIL]),
        (internal::CONNECTION_CATALOG_READ, vec![supplier_api::CONNECTION_DETAIL]),
        (internal::CONNECTION_PROTOCOL_READ, vec![supplier_api::DANGAOSHUSHU_READ]),
        (internal::CONNECTION_REFERENCE_TICKETS, vec![supplier_api::DANGAOSHUSHU_REFERENCE_TICKETS]),
    ]
});

pub(crate) fn independent(permission: &Permission) -> Vec<Permission> {
    if [internal::CONNECTION_PROTOCOL_READ, internal::CONNECTION_REFERENCE_TICKETS].contains(permission) {
        return vec![supplier_api::CONNECTION_DETAIL, supplier::DETAIL];
    }
    if permission == &internal::PURCHASE_DELIVERY_OPTIONS_READ {
        return vec![purchase_order::SAVE_DRAFT];
    }
    if permission == &internal::CUSTOMER_QUALITY_HISTORY {
        return vec![customer::LIST];
    }
    if let Some(required) = independent_command(permission) {
        return vec![required];
    }
    if let Some(required) = purchase_entry(permission) {
        return vec![required];
    }
    if permission == &internal::SALES_CHANGE_DRAFT_READ {
        return vec![sales_review::SALES_CHANGE_DRAFT];
    }
    if permission == &internal::CONNECTION_CATALOG_READ {
        return vec![supplier::DETAIL];
    }
    if permission == &internal::PURCHASE_CHANGE_SUBMITTED_RESULT_READ {
        return vec![purchase_order::PURCHASE_CHANGE_SUBMIT];
    }
    if permission == &internal::PURCHASE_CHANGE_VOIDED_RESULT_READ {
        return vec![purchase_order::CHANGE_VOID_PURCHASE_CHANGE_VOID];
    }
    if permission == &purchase_order::CORRESPONDENCE_SEND_PURCHASE_EMAIL {
        return vec![purchase_order::DETAIL];
    }
    if permission == &internal::SALES_ORDER_EXTRA_COST_READ {
        return vec![cost::ENTRY_LIST];
    }
    if permission.resource() == "supplier_api_connection"
        && matches!(permission.action(), "detail" | "authorize")
    {
        return vec![supplier::DETAIL];
    }
    portal_material_permissions(permission)
}

/// 门户材料按其固定用途取得独立业务资格，不改变其他对象入口。
fn portal_material_permissions(permission: &Permission) -> Vec<Permission> {
    if permission.resource() != "supplier_portal_material" {
        return vec![];
    }
    match permission.action() {
        "read" => vec![supplier::DETAIL],
        "read_offering_update" => vec![supplier::DETAIL, supplier_offering::LIST],
        "review_cooperation" => vec![supplier::PROFILE_UPDATE],
        "review_offering_create" => vec![supplier::DETAIL, supplier_offering::CREATE],
        "review_offering_update" => vec![supplier::DETAIL, supplier_offering::REVISE],
        "review_new_product" => {
            vec![supplier::DETAIL, catalog::PRODUCT_PRODUCT_CREATE, supplier_offering::CREATE]
        },
        _ => vec![],
    }
}

/// 命令派生入口保留与根修改资格相互独立的附加功能资格。
fn independent_command(permission: &Permission) -> Option<Permission> {
    [
        (internal::CONTRACT_SALES_ACCESS, customer::ASSIGNMENT_APPLY),
        (internal::CATALOG_ADOPTION, internal::SUPPLIER_OFFERING_COST_DETAIL),
        (internal::SUPPLIER_OFFERING_AVAILABILITY_UPDATE, supplier_offering::UPDATE_AVAILABILITY),
        (internal::SALES_SELECTION_ORDER_BASIS, sales_order::CREATE),
    ]
    .into_iter()
    .find_map(|(entry, required)| (&entry == permission).then_some(required))
}

/// 采购来源检查保留独立业务入口资格，不要求其与采购范围资格来自同一角色。
fn purchase_entry(permission: &Permission) -> Option<Permission> {
    [
        (internal::PURCHASE_CHANGE_SOURCE, purchase_order::PURCHASE_CHANGE_SUBMIT),
        (internal::PURCHASE_CHANGE_CREATE_SOURCE, purchase_order::PURCHASE_CHANGE_CREATE),
        (internal::PURCHASE_CHANGE_SUBMIT_SOURCE, purchase_order::PURCHASE_CHANGE_SUBMIT),
        (internal::PURCHASE_CHANGE_CANCEL_SOURCE, purchase_order::PURCHASE_CHANGE_CANCEL_APPROVAL),
        (internal::PURCHASE_CHANGE_VOID_SOURCE, purchase_order::CHANGE_VOID_PURCHASE_CHANGE_VOID),
        (internal::PURCHASE_EXPENSE_CONFIRM_SOURCE, purchase_order::EXPENSE_EXPENSE_OCCURRENCE_CONFIRM),
    ]
    .into_iter()
    .find_map(|(entry, required)| (&entry == permission).then_some(required))
}

pub(crate) fn qualified(permission: &Permission, qualified: &QualifiedPermissions) -> bool {
    permission != &bulk_job::BACKGROUND_JOB_CANCEL_ALL || administrator(qualified)
}

/// 返回固定对象入口的数据条件。
/// # 参数
/// permission 为已登记的对象入口；无正式任务的差异决定按当前处理人判断。
/// # 返回
/// 已登记对象入口返回完整条件，其他入口返回 None。
/// # 错误
/// 不执行 I/O；缺失事实由条件执行阶段拒绝。
pub(crate) fn condition(permission: &Permission) -> Option<AccessRule> {
    if permission == &integration_ops::DIFFERENCE_DECISION {
        return Some(
            AccessRule::Fact(AccessFact::AssignedTo).requiring(&BTreeSet::from([FactKind::Assignee])),
        );
    }

    if permission == &internal::APPROVAL_STARTED_LIST {
        return Some(owner());
    }
    if permission == &internal::APPROVAL_MINE_LIST {
        return Some(
            AccessRule::Fact(AccessFact::AssignedTo).requiring(&BTreeSet::from([FactKind::Assignee])),
        );
    }

    match permission.resource() {
        "supplier_api_connection"
            if matches!(
                permission.action(),
                "detail" | "authorize" | "catalog_read" | "protocol_read_target" | "reference_tickets_target"
            ) =>
        {
            Some(AccessRule::Related {
                relation: RelationKey::parse("supplier").expect("固定供应商关系"),
                resource: "supplier".into(),
                mode: RelationMatch::AllNonEmpty,
                rule: Box::new(
                    AccessRule::Allow.requiring(&BTreeSet::from([FactKind::Owner, FactKind::Organization])),
                ),
            })
        },
        "file_asset" if matches!(permission.action(), "transfer" | "transfer_owned") => Some(owner()),
        "supplier_catalog_adoption_preview" | "sales_inquiry_import_preview" => Some(owner()),
        "supplier_portal_material" => Some(AccessRule::Allow),
        "background_job" | "background_job_item" if permission.action() != "cancel_all" => Some(owner()),
        "approval_copy" => Some(owner()),
        _ => None,
    }
}

pub(crate) fn resolved(
    permission: &Permission,
    qualified: &QualifiedPermissions,
    rule: AccessRule,
) -> AccessRule {
    if matches!(permission.resource(), "background_job" | "background_job_item")
        && administrator(qualified)
        && !permission.action().ends_with("_read")
    {
        return AccessRule::Allow.requiring(&BTreeSet::from([FactKind::Owner]));
    }
    rule
}

fn owner() -> AccessRule {
    AccessRule::Fact(AccessFact::Owned).requiring(&BTreeSet::from([FactKind::Owner]))
}

fn administrator(qualified: &QualifiedPermissions) -> bool {
    qualified.roles.iter().any(|role| matches!(role.base.id.as_str(), "role-root" | "role-sysadmin"))
}

/// 可选读取资格只影响固定分支，不成为全部入口的必需资格。
pub(crate) fn optional(permission: &Permission) -> Vec<Permission> {
    super::management_permission(permission).into_iter().collect()
}

/// 结算责任条件与入口范围组合，业务调用方不再追加行级政策。
pub(crate) fn scoped(permission: &Permission, scope: AccessRule) -> AccessRule {
    let responsibility = if [
        internal::SETTLEMENT_REFRESH_PREPARED,
        internal::SETTLEMENT_VOID_PREPARED,
        supplier_settlement::STATEMENT_SUBMIT_REVIEW,
    ]
    .contains(permission)
    {
        Some((AccessFact::Owned, FactKind::Owner))
    } else if permission == &internal::SETTLEMENT_RESOLVE_DIFFERENCE {
        Some((AccessFact::AssignedTo, FactKind::Assignee))
    } else {
        None
    };
    match responsibility {
        Some((fact, kind)) => {
            AccessRule::all(vec![scope, AccessRule::Fact(fact)]).requiring(&BTreeSet::from([kind]))
        },
        None => scope,
    }
}
