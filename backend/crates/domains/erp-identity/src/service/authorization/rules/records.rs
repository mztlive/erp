//! 固定内部与组合入口的功能资格及对象规则；业务仅提供持久化事实，不能选择政策。
use std::collections::BTreeSet;

use erp_permissions::{
    admin, approval_instance, bulk_job, catalog, contract, cost, customer, fulfillment, integration_ops,
    internal, inventory, organization, payable, purchase_order, purchase_payment_request, receivable,
    returns, sales_inquiry, sales_order, sales_review, sales_selection, supplier, supplier_api,
    supplier_fulfillment, supplier_offering, supplier_settlement, work_item,
};

use crate::entity::authorization::{AccessFact, AccessRule, FactKind, RelationKey, RelationMatch};
use crate::entity::person_profile_change::ORGANIZATION_PERSON_PROFILE_CHANGE;
use crate::{Permission, QualifiedPermissions};

const SAME_ROLE: &[(Permission, &[Permission])] = &[
    // 零公共功能门槛的内部与归属任务入口
    (internal::WORK_ITEM_RECIPIENT, &[]),
    (internal::WORK_ITEM_EXECUTION_QUALIFICATION, &[]),
    (internal::DATA_TRANSFER_JOB_READ, &[]),
    // 组合与借用同角色资格
    (
        internal::SALES_INQUIRY_IMPORT_CREATE,
        &[
            sales_inquiry::COMMANDS_SALES_CREATE,
            sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW,
            catalog::PRODUCT_SELLABLE_SKU_LIST,
        ],
    ),
    (
        internal::SALES_INQUIRY_REINQUIRE_DRAFT,
        &[sales_inquiry::COMMANDS_SALES_REINQUIRE, catalog::PRODUCT_SELLABLE_SKU_LIST],
    ),
    (
        sales_inquiry::COMMANDS_SALES_CREATE,
        &[sales_inquiry::COMMANDS_SALES_CREATE, catalog::PRODUCT_SELLABLE_SKU_LIST],
    ),
    (
        sales_inquiry::COMMANDS_SALES_SAVE,
        &[sales_inquiry::COMMANDS_SALES_SAVE, catalog::PRODUCT_SELLABLE_SKU_LIST],
    ),
    (
        sales_inquiry::COMMANDS_SALES_SUBMIT,
        &[sales_inquiry::COMMANDS_SALES_SUBMIT, catalog::PRODUCT_SELLABLE_SKU_LIST],
    ),
    (
        sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW,
        &[sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW, catalog::PRODUCT_SELLABLE_SKU_LIST],
    ),
    (internal::WORK_ITEM_EXECUTE, &[work_item::DETAIL]),
    (
        approval_instance::SUBMIT_DECISION,
        &[approval_instance::SUBMIT_DECISION, approval_instance::INSTANCE_LIST],
    ),
    (
        approval_instance::CANCEL_BLOCKED,
        &[approval_instance::CANCEL_BLOCKED, approval_instance::INSTANCE_LIST],
    ),
    (
        approval_instance::UPGRADE_BINDING,
        &[approval_instance::UPGRADE_BINDING, approval_instance::INSTANCE_LIST],
    ),
    (
        inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
        &[inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL, approval_instance::INSTANCE_LIST],
    ),
    (sales_order::SUBMIT, &[sales_order::SUBMIT, sales_order::CREATE]),
    (cost::ENTRY_LIST, &[cost::ENTRY_LIST, sales_order::LIST]),
    (cost::ENTRY_CREATE, &[cost::ENTRY_CREATE, sales_order::DETAIL]),
    (
        inventory::STOCK_ADJUSTMENT_LIST,
        &[inventory::STOCK_ADJUSTMENT_LIST, inventory::STOCK_ADJUSTMENT_DETAIL],
    ),
    (
        inventory::STOCK_ADJUSTMENT_CREATE,
        &[inventory::STOCK_ADJUSTMENT_CREATE, inventory::STOCK_ADJUSTMENT_DETAIL],
    ),
    (
        inventory::STOCK_ADJUSTMENT_UPDATE,
        &[inventory::STOCK_ADJUSTMENT_UPDATE, inventory::STOCK_ADJUSTMENT_DETAIL],
    ),
    (
        inventory::STOCK_ADJUSTMENT_SUBMIT,
        &[inventory::STOCK_ADJUSTMENT_SUBMIT, inventory::STOCK_ADJUSTMENT_DETAIL],
    ),
    (
        purchase_payment_request::CREDIT_LIST,
        &[purchase_payment_request::CREDIT_LIST, purchase_payment_request::DETAIL],
    ),
    (internal::CATALOG_BRAND_CREATE_ATTACHMENT_UPLOAD, &[catalog::PRODUCT_BRAND_CREATE]),
    (internal::CATALOG_BRAND_UPDATE_ATTACHMENT_UPLOAD, &[catalog::PRODUCT_BRAND_UPDATE]),
    (internal::CATALOG_CREATE_ATTACHMENT_UPLOAD, &[catalog::PRODUCT_PRODUCT_CREATE]),
    (internal::CATALOG_UPDATE_ATTACHMENT_UPLOAD, &[catalog::PRODUCT_PRODUCT_UPDATE]),
    (internal::SUPPLIER_CREATE_ATTACHMENT_UPLOAD, &[supplier::PROFILE_CREATE]),
    (internal::SUPPLIER_UPDATE_ATTACHMENT_UPLOAD, &[supplier::PROFILE_UPDATE]),
    (internal::PURCHASE_EXPENSE_ATTACHMENT_UPLOAD, &[purchase_order::EXPENSE_EXPENSE_OCCURRENCE_CONFIRM]),
    (
        internal::ELECTRONIC_DELIVERY_ATTACHMENT_UPLOAD,
        &[
            fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST,
            fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM,
        ],
    ),
    (
        internal::SERVICE_FULFILLMENT_ATTACHMENT_UPLOAD,
        &[
            fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST,
            fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM,
        ],
    ),
    (internal::FINANCE_UPLOAD_CUSTOMER_RECEIPT, &[receivable::CUSTOMER_RECEIPT_SUBMIT]),
    (internal::FINANCE_UPLOAD_INVOICE_DRAFT, &[receivable::INVOICE_CREATE]),
    (internal::FINANCE_UPLOAD_INVOICE_POST, &[receivable::INVOICE_POST]),
    (internal::FINANCE_UPLOAD_PURCHASE_INVOICE, &[payable::PURCHASE_INVOICE_ALLOCATION_POST]),
    (internal::FINANCE_UPLOAD_EXTRA_COST, &[cost::ENTRY_CREATE, sales_order::DETAIL]),
    (
        internal::FINANCE_UPLOAD_PURCHASE_PAYMENT,
        &[purchase_payment_request::CREDIT_LIST, purchase_payment_request::DETAIL],
    ),
    (internal::PROFIT_LOSS_PERIOD_BASIS, &[cost::ENTRY_LIST]),
    (internal::CUSTOMER_QUALITY_HISTORY, &[sales_order::LIST]),
    (internal::PURCHASE_DELIVERY_OPTIONS_READ, &[purchase_order::DETAIL]),
    (internal::CUSTOMER_PROFILE_COMMAND_READ, &[customer::PROFILE_COMMAND_DETAIL]),
    (internal::CUSTOMER_CREATED_RESULT_READ, &[customer::CREATE]),
    (internal::SUPPLIER_PROFILE_COMMAND_READ, &[supplier::PROFILE_COMMAND_DETAIL]),
    (internal::INVOICE_REQUEST_AMOUNTS_READ, &[receivable::INVOICE_REQUEST_INVOICE_REQUEST_AMOUNTS]),
    (internal::INVOICE_REQUEST_SUBMIT_SOURCE, &[receivable::INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT]),
    (internal::INVOICE_REQUEST_CANCEL_SOURCE, &[receivable::INVOICE_REQUEST_INVOICE_REQUEST_CANCEL]),
    (internal::CONTRACT_IMPORT, &[contract::CREATE]),
    (internal::CONTRACT_SALES_ACCESS, &[contract::TERMINATE]),
    (internal::CATALOG_ADOPTION, &[supplier_api::ADOPTIONS_PREVIEW]),
    (ORGANIZATION_PERSON_PROFILE_CHANGE, &[organization::CHANGE, admin::ACCOUNT_UPDATE_ADMIN]),
    (internal::SETTLEMENT_REFRESH_PREPARED, &[supplier_settlement::STATEMENT_REFRESH]),
    (internal::SETTLEMENT_VOID_PREPARED, &[supplier_settlement::STATEMENT_REFRESH]),
    (internal::SETTLEMENT_RESOLVE_DIFFERENCE, &[supplier_settlement::STATEMENT_REFRESH]),
    (internal::SALES_CREATED_RESULT_READ, &[sales_order::CREATE]),
    (internal::SALES_CHANGE_DRAFT_READ, &[sales_order::HANDOVER]),
    (internal::SALES_CHANGE_APPLIED_RESULT_READ, &[sales_order::DETAIL]),
    (internal::PURCHASE_CHANGE_SUBMITTED_RESULT_READ, &[purchase_order::SUBMIT]),
    (internal::PURCHASE_CHANGE_VOIDED_RESULT_READ, &[purchase_order::SAVE_DRAFT]),
    (internal::SALES_CREATE_AND_SUBMIT, &[sales_order::CREATE, sales_order::SUBMIT]),
    (internal::SALES_ORDER_EXTRA_COST_READ, &[sales_order::DETAIL]),
    (
        internal::SUPPLIER_FULFILLMENT_COMPLETED_RESULT_READ,
        &[
            supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
            supplier_fulfillment::ORDER_INVESTIGATION,
            supplier_fulfillment::ORDER_TASK_COMPLETION,
        ],
    ),
    (internal::PURCHASE_CREATED_RESULT_READ, &[purchase_order::CREATE]),
    (internal::SOURCING_CREATED_RESULT_READ, &[purchase_order::CREATE]),
    (internal::INVOICE_POSTED_RESULT_READ, &[receivable::INVOICE_POST]),
    (internal::PURCHASE_CHANGE_SOURCE, &[purchase_order::SAVE_DRAFT, purchase_order::SUBMIT]),
    (internal::PURCHASE_CHANGE_CREATE_SOURCE, &[purchase_order::SAVE_DRAFT]),
    (internal::PURCHASE_CHANGE_SUBMIT_SOURCE, &[purchase_order::SUBMIT]),
    (internal::PURCHASE_CHANGE_CANCEL_SOURCE, &[purchase_order::CANCEL_APPROVAL]),
    (internal::PURCHASE_CHANGE_VOID_SOURCE, &[purchase_order::SAVE_DRAFT]),
    (internal::PURCHASE_EXPENSE_CONFIRM_SOURCE, &[purchase_order::SAVE_DRAFT]),
    (internal::PURCHASE_CREATE_AND_SUBMIT, &[purchase_order::CREATE, purchase_order::SUBMIT]),
    (internal::PURCHASE_PORTAL_IMPACT_READ, &[purchase_order::DETAIL, purchase_order::LIST]),
    (internal::SALES_ORDER_PROFIT_LOSS_READ, &[sales_order::LIST, cost::ENTRY_LIST]),
    (internal::APPROVAL_MANAGEMENT_READ, &[approval_instance::INSTANCE_LIST]),
    (internal::APPROVAL_INSTANCE_READ, &[approval_instance::INSTANCE_LIST]),
    (internal::APPROVAL_STARTED_LIST, &[approval_instance::INSTANCE_LIST]),
    (internal::APPROVAL_MINE_LIST, &[approval_instance::INSTANCE_LIST, approval_instance::SUBMIT_DECISION]),
    (
        internal::APPROVAL_RECOVER,
        &[approval_instance::RESUME_CURRENT_APPROVER, approval_instance::INSTANCE_LIST],
    ),
    (internal::CUSTOMER_QUALITY_CUSTOMERS, &[customer::LIST]),
    (internal::CUSTOMER_QUALITY_CURRENT, &[sales_order::LIST]),
    (
        internal::SALES_INQUIRY_TEMPLATE_PRODUCTS,
        &[sales_inquiry::COMMANDS_SALES_CREATE, catalog::PRODUCT_SELLABLE_SKU_LIST],
    ),
    (internal::DELIVERY_TEMPLATE_READ, &[supplier_offering::LIST]),
    (internal::DELIVERY_TEMPLATE_WRITE, &[supplier_offering::REVISE]),
    (internal::SUPPLIER_OFFERING_AVAILABILITY_UPDATE, &[supplier_offering::REVISE]),
    (internal::SUPPLIER_OFFERING_CREATED_RESULT_READ, &[supplier_offering::CREATE]),
    (internal::SALES_SELECTION_ORDER_BASIS, &[sales_selection::PROPOSAL_DETAIL]),
    (
        internal::PURCHASE_RETURN_SOURCE,
        &[purchase_order::SAVE_DRAFT, returns::PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE],
    ),
    (internal::SALES_ORDER_FINANCIAL_EXPENSE_SOURCE, &[cost::ENTRY_CREATE, sales_order::DETAIL]),
    (internal::PAYMENT_REQUEST_CANDIDATES, &[purchase_payment_request::CREATE]),
    (internal::PRODUCT_IMPORT_JOB_READ, &[catalog::PRODUCT_PRODUCT_LIST]),
    (internal::PRODUCT_IMPORT_SUBMIT, &[catalog::PRODUCT_PRODUCT_CREATE]),
    (internal::SUPPLIER_IMPORT_JOB_READ, &[supplier::IMPORT_SUPPLIER_IMPORT]),
    (internal::SUPPLIER_IMPORT_SUBMIT, &[supplier::IMPORT_SUPPLIER_IMPORT]),
    (internal::BRAND_IMPORT_JOB_READ, &[catalog::PRODUCT_BRAND_CREATE]),
    (internal::PURCHASE_EXPENSE_SOURCE, &[purchase_order::CREATE]),
    (internal::SALES_ORDER_HANDOVER_RECIPIENT, &[sales_order::DETAIL]),
    (internal::CUSTOMER_SELECT, &[customer::DETAIL]),
    (internal::CONNECTION_CATALOG_READ, &[supplier_api::CONNECTION_DETAIL]),
    (internal::CONNECTION_PROTOCOL_READ, &[supplier_api::DANGAOSHUSHU_READ]),
    (internal::CONNECTION_REFERENCE_TICKETS, &[supplier_api::DANGAOSHUSHU_REFERENCE_TICKETS]),
];

const INDEPENDENT: &[(Permission, &[Permission])] = &[
    (internal::CONNECTION_PROTOCOL_READ, &[supplier_api::CONNECTION_DETAIL, supplier::DETAIL]),
    (internal::CONNECTION_REFERENCE_TICKETS, &[supplier_api::CONNECTION_DETAIL, supplier::DETAIL]),
    (internal::PURCHASE_DELIVERY_OPTIONS_READ, &[purchase_order::SAVE_DRAFT]),
    (internal::CUSTOMER_QUALITY_HISTORY, &[customer::LIST]),
    (internal::CONTRACT_SALES_ACCESS, &[customer::ASSIGNMENT_APPLY]),
    (internal::CATALOG_ADOPTION, &[internal::SUPPLIER_OFFERING_COST_DETAIL]),
    (internal::SUPPLIER_OFFERING_AVAILABILITY_UPDATE, &[supplier_offering::UPDATE_AVAILABILITY]),
    (internal::SALES_SELECTION_ORDER_BASIS, &[sales_order::CREATE]),
    (internal::PURCHASE_CHANGE_SOURCE, &[purchase_order::PURCHASE_CHANGE_SUBMIT]),
    (internal::PURCHASE_CHANGE_CREATE_SOURCE, &[purchase_order::PURCHASE_CHANGE_CREATE]),
    (internal::PURCHASE_CHANGE_SUBMIT_SOURCE, &[purchase_order::PURCHASE_CHANGE_SUBMIT]),
    (internal::PURCHASE_CHANGE_CANCEL_SOURCE, &[purchase_order::PURCHASE_CHANGE_CANCEL_APPROVAL]),
    (internal::PURCHASE_CHANGE_VOID_SOURCE, &[purchase_order::CHANGE_VOID_PURCHASE_CHANGE_VOID]),
    (internal::PURCHASE_EXPENSE_CONFIRM_SOURCE, &[purchase_order::EXPENSE_EXPENSE_OCCURRENCE_CONFIRM]),
    (internal::SALES_CHANGE_DRAFT_READ, &[sales_review::SALES_CHANGE_DRAFT]),
    (internal::CONNECTION_CATALOG_READ, &[supplier::DETAIL]),
    (internal::PURCHASE_CHANGE_SUBMITTED_RESULT_READ, &[purchase_order::PURCHASE_CHANGE_SUBMIT]),
    (internal::PURCHASE_CHANGE_VOIDED_RESULT_READ, &[purchase_order::CHANGE_VOID_PURCHASE_CHANGE_VOID]),
    (purchase_order::CORRESPONDENCE_SEND_PURCHASE_EMAIL, &[purchase_order::DETAIL]),
    (internal::SALES_ORDER_EXTRA_COST_READ, &[cost::ENTRY_LIST]),
    (supplier_api::CONNECTION_DETAIL, &[supplier::DETAIL]),
    (supplier_api::JD_VOP_AUTHORIZE, &[supplier::DETAIL]),
    (internal::PORTAL_MATERIAL_READ, &[supplier::DETAIL]),
    (internal::PORTAL_MATERIAL_READ_OFFERING, &[supplier::DETAIL, supplier_offering::LIST]),
    (internal::PORTAL_MATERIAL_REVIEW_COOPERATION, &[supplier::PROFILE_UPDATE]),
    (internal::PORTAL_MATERIAL_REVIEW_OFFERING_CREATE, &[supplier::DETAIL, supplier_offering::CREATE]),
    (internal::PORTAL_MATERIAL_REVIEW_OFFERING_UPDATE, &[supplier::DETAIL, supplier_offering::REVISE]),
    (
        internal::PORTAL_MATERIAL_REVIEW_NEW_PRODUCT,
        &[supplier::DETAIL, catalog::PRODUCT_PRODUCT_CREATE, supplier_offering::CREATE],
    ),
];

/// 返回固定入口的同角色功能资格覆盖；空切片表示无公共功能门槛，未登记时由调用方沿用权限自身。
pub(crate) fn requirements(permission: &Permission) -> Option<&'static [Permission]> {
    if permission.resource() == "approval_copy"
        || matches!(
            permission.resource(),
            "supplier_catalog_adoption_preview" | "sales_inquiry_import_preview" | "supplier_portal_material"
        )
        || (permission.resource() == "file_asset"
            && matches!(permission.action(), "transfer" | "transfer_owned"))
    {
        return Some(&[]);
    }
    SAME_ROLE.iter().find(|(entry, _)| entry == permission).map(|(_, required)| *required)
}

/// 列出内部业务入口须分别取证的额外资格；可由不同启用角色满足，不能用根入口替代。
pub(crate) fn independent(permission: &Permission) -> &'static [Permission] {
    INDEPENDENT.iter().find(|(entry, _)| entry == permission).map_or(&[], |(_, required)| *required)
}

/// 除通用资格外，为后台任务全部取消额外核验身份快照包含启用的管理员角色。
pub(crate) fn qualified(permission: &Permission, qualified: &QualifiedPermissions) -> bool {
    permission != &bulk_job::BACKGROUND_JOB_CANCEL_ALL || administrator(qualified)
}

/// 返回已登记对象入口的固定数据规则，其他入口交由调用方处理。不执行 I/O，必需事实由求值阶段核验；无正式任务的差异决定按当前处理人判断。
pub(crate) fn condition(permission: &Permission) -> Option<AccessRule> {
    if permission == &integration_ops::DIFFERENCE_DECISION || permission == &internal::APPROVAL_MINE_LIST {
        return Some(
            AccessRule::Fact(AccessFact::AssignedTo).requiring(&BTreeSet::from([FactKind::Assignee])),
        );
    }
    if permission == &internal::APPROVAL_STARTED_LIST {
        return Some(owner());
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
        "supplier_catalog_adoption_preview" | "sales_inquiry_import_preview" | "approval_copy" => {
            Some(owner())
        },
        "supplier_portal_material" => Some(AccessRule::Allow),
        "background_job" | "background_job_item" if permission.action() != "cancel_all" => Some(owner()),
        _ => None,
    }
}

/// 为启用管理员按固定动作放宽后台任务的本人过滤，仍要求负责人事实，专用读取入口不放宽。
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

/// 列出固定可选读取分支所需的工作项管理资格，不将其当成根对象共同前提。
pub(crate) fn optional(permission: &Permission) -> Vec<Permission> {
    super::management_permission(permission).into_iter().collect()
}

/// 把结算单经办人或差异当前处理人的责任要求并入原范围，其他入口保留原条件。
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
