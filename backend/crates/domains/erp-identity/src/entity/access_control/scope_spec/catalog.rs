//! 范围与非范围动作的静态目录，引用共享权限及本域专用权限。
mod functions;

use erp_permissions::{
    access_control as access, approval_instance, bulk_job, catalog, contract, cost, customer, file_asset,
    integration_ops as integration, internal, inventory, organization, payable, purchase_order,
    purchase_payment_request as payment, receivable, returns, sales_inquiry as inquiry, sales_order,
    sales_selection as selection, supplier, supplier_api, supplier_deposit as deposit,
    supplier_fulfillment as fulfillment, supplier_offering as offering, supplier_settlement as settlement,
    warehouse, work_item,
};
pub(super) use functions::PERMISSIONS as FUNCTION_ACTIONS;

use super::super::{DataScopeType, ScopeDimension};
use super::{ScopeSource, ScopeSpec};
use crate::Permission;
use crate::entity::person_profile_change::ORGANIZATION_PERSON_PROFILE_CHANGE;

const ALL: &[DataScopeType] = &[
    DataScopeType::Company,
    DataScopeType::Organization,
    DataScopeType::Team,
    DataScopeType::SelfOwned,
    DataScopeType::Collaborative,
];
const PERSON: &[DataScopeType] =
    &[DataScopeType::Company, DataScopeType::Organization, DataScopeType::Team, DataScopeType::SelfOwned];
const OBJECT: &[DataScopeType] = &[DataScopeType::Company, DataScopeType::Organization];
const ORG: &[ScopeDimension] = &[ScopeDimension::InternalOrg];
const WAREHOUSE: &[ScopeDimension] = &[ScopeDimension::Warehouse];
const DEPOSIT_DIMS: &[ScopeDimension] = &[ScopeDimension::InternalOrg, ScopeDimension::SettlementParty];

const fn current(permission: Permission) -> ScopeSpec {
    ScopeSpec::new(permission, ScopeSource::Current, false, ALL, ORG, false)
}
const fn current_self(permission: Permission) -> ScopeSpec {
    ScopeSpec::new(permission, ScopeSource::Current, true, ALL, ORG, false)
}
const fn current_self_history(permission: Permission) -> ScopeSpec {
    ScopeSpec::new(permission, ScopeSource::Current, true, ALL, ORG, true)
}
const fn company(permission: Permission) -> ScopeSpec {
    ScopeSpec::new(permission, ScopeSource::Company, false, ALL, ORG, false)
}
const fn company_history(permission: Permission) -> ScopeSpec {
    ScopeSpec::new(permission, ScopeSource::Company, false, ALL, ORG, true)
}
const fn reuse(permission: Permission, source: Permission) -> ScopeSpec {
    ScopeSpec::new(permission, ScopeSource::Permission(source), false, ALL, ORG, false)
}
const fn reuse_history(permission: Permission, source: Permission) -> ScopeSpec {
    ScopeSpec::new(permission, ScopeSource::Permission(source), false, ALL, ORG, true)
}
const fn wh(permission: Permission) -> ScopeSpec {
    ScopeSpec::new(permission, ScopeSource::Current, false, ALL, WAREHOUSE, false)
}
const fn wh_reuse(permission: Permission, source: Permission) -> ScopeSpec {
    ScopeSpec::new(permission, ScopeSource::Permission(source), false, ALL, WAREHOUSE, false)
}
const fn deposit_spec(permission: Permission) -> ScopeSpec {
    ScopeSpec::new(permission, ScopeSource::Current, false, ALL, DEPOSIT_DIMS, false)
}

// 仓库入口同时复用原库存动作的资格和范围，且全部支持查询。
pub(crate) static INVENTORY_WAREHOUSE_ENTRIES: &[(Permission, Permission)] = &[
    (internal::INVENTORY_BALANCE_WAREHOUSES, inventory::STOCK_BALANCE_LIST),
    (internal::INVENTORY_BALANCE_DETAIL_WAREHOUSES, inventory::STOCK_BALANCE_DETAIL),
    (internal::INVENTORY_MOVEMENT_WAREHOUSES, inventory::STOCK_MOVEMENT_LIST),
    (internal::INVENTORY_RESERVATION_WAREHOUSES, inventory::STOCK_RESERVATION_LIST),
    (internal::INVENTORY_ADJUSTMENT_WAREHOUSES, inventory::STOCK_ADJUSTMENT_LIST),
    (internal::INVENTORY_ADJUSTMENT_DETAIL_WAREHOUSES, inventory::STOCK_ADJUSTMENT_DETAIL),
    (internal::INVENTORY_CREATE_WAREHOUSES, inventory::STOCK_ADJUSTMENT_CREATE),
    (internal::INVENTORY_UPDATE_WAREHOUSES, inventory::STOCK_ADJUSTMENT_UPDATE),
];

pub(super) const SPECS: &[ScopeSpec] = &[
    // 内部销售/采购/质量/库存仓库来源入口
    reuse(internal::SALES_CREATE_AND_SUBMIT, sales_order::CREATE),
    reuse_history(internal::SALES_ORDER_EXTRA_COST_READ, sales_order::DETAIL),
    reuse(internal::SALES_CREATED_RESULT_READ, sales_order::CREATE),
    company(internal::PURCHASE_CREATED_RESULT_READ),
    company(internal::SOURCING_CREATED_RESULT_READ),
    company(internal::PURCHASE_CHANGE_SOURCE),
    company(internal::PURCHASE_CHANGE_CREATE_SOURCE),
    company(internal::PURCHASE_CHANGE_SUBMIT_SOURCE),
    company(internal::PURCHASE_CHANGE_CANCEL_SOURCE),
    company(internal::PURCHASE_CHANGE_VOID_SOURCE),
    company(internal::PURCHASE_EXPENSE_CONFIRM_SOURCE),
    company(internal::PURCHASE_CHANGE_SUBMITTED_RESULT_READ),
    company(internal::PURCHASE_CHANGE_VOIDED_RESULT_READ),
    company(internal::PURCHASE_CREATE_AND_SUBMIT),
    company(internal::PURCHASE_PORTAL_IMPACT_READ),
    reuse_history(internal::SALES_ORDER_PROFIT_LOSS_READ, sales_order::LIST),
    reuse(internal::CUSTOMER_QUALITY_CUSTOMERS, customer::LIST),
    reuse_history(internal::CUSTOMER_QUALITY_HISTORY, sales_order::LIST),
    reuse(internal::CUSTOMER_QUALITY_CURRENT, sales_order::LIST),
    company(internal::PURCHASE_RETURN_SOURCE),
    reuse_history(internal::SALES_ORDER_FINANCIAL_EXPENSE_SOURCE, sales_order::DETAIL),
    reuse(internal::PURCHASE_EXPENSE_SOURCE, sales_order::DETAIL),
    wh_reuse(internal::INVENTORY_BALANCE_WAREHOUSES, inventory::STOCK_BALANCE_LIST),
    wh_reuse(internal::INVENTORY_BALANCE_DETAIL_WAREHOUSES, inventory::STOCK_BALANCE_DETAIL),
    wh_reuse(internal::INVENTORY_MOVEMENT_WAREHOUSES, inventory::STOCK_MOVEMENT_LIST),
    wh_reuse(internal::INVENTORY_RESERVATION_WAREHOUSES, inventory::STOCK_RESERVATION_LIST),
    wh_reuse(internal::INVENTORY_ADJUSTMENT_WAREHOUSES, inventory::STOCK_ADJUSTMENT_LIST),
    wh_reuse(internal::INVENTORY_ADJUSTMENT_DETAIL_WAREHOUSES, inventory::STOCK_ADJUSTMENT_DETAIL),
    wh_reuse(internal::INVENTORY_CREATE_WAREHOUSES, inventory::STOCK_ADJUSTMENT_CREATE),
    wh_reuse(internal::INVENTORY_UPDATE_WAREHOUSES, inventory::STOCK_ADJUSTMENT_UPDATE),
    // 库存
    wh(inventory::STOCK_ADJUSTMENT_LIST),
    wh(inventory::STOCK_ADJUSTMENT_DETAIL),
    wh(inventory::STOCK_ADJUSTMENT_CREATE),
    wh(inventory::STOCK_ADJUSTMENT_UPDATE),
    wh(inventory::STOCK_ADJUSTMENT_SUBMIT),
    wh(inventory::STOCK_BALANCE_LIST),
    wh(inventory::STOCK_BALANCE_DETAIL),
    wh(inventory::STOCK_MOVEMENT_LIST),
    wh(inventory::STOCK_RESERVATION_LIST),
    // 供应商结算
    company(settlement::STATEMENT_LIST),
    company(settlement::STATEMENT_DETAIL),
    company(settlement::STATEMENT_CREATE),
    company(settlement::STATEMENT_REFRESH),
    company(internal::SETTLEMENT_REFRESH_PREPARED),
    company(internal::SETTLEMENT_VOID_PREPARED),
    company(internal::SETTLEMENT_RESOLVE_DIFFERENCE),
    company(settlement::STATEMENT_SUBMIT_REVIEW),
    // 工作项与组织
    current_self(work_item::LIST),
    current_self(work_item::DETAIL),
    reuse(internal::WORK_ITEM_EXECUTE, work_item::DETAIL),
    current(work_item::MANAGE),
    current(organization::LIST),
    current(organization::CHANGE),
    reuse(ORGANIZATION_PERSON_PROFILE_CHANGE, organization::CHANGE),
    // 客户与合同
    reuse(internal::CUSTOMER_SELECT, customer::DETAIL),
    reuse(internal::CUSTOMER_CREATED_RESULT_READ, customer::CREATE),
    current_self_history(customer::LIST),
    current_self_history(customer::DETAIL),
    current_self(customer::CREATE),
    current_self(customer::UPDATE),
    current_self(customer::DELETE),
    reuse_history(contract::LIST, customer::LIST),
    reuse_history(contract::DETAIL, customer::LIST),
    reuse(contract::CREATE, customer::LIST),
    reuse(contract::TERMINATE, customer::LIST),
    reuse(internal::CONTRACT_SALES_ACCESS, customer::LIST),
    // 销售单与询价
    reuse(internal::SALES_ORDER_HANDOVER_RECIPIENT, sales_order::DETAIL),
    current_self_history(sales_order::LIST),
    current_self_history(sales_order::DETAIL),
    current_self(sales_order::CREATE),
    current_self(sales_order::HANDOVER),
    reuse(internal::SALES_CHANGE_DRAFT_READ, sales_order::HANDOVER),
    current_self(sales_order::VOID),
    current_self(sales_order::SUBMIT),
    current_self(sales_order::CANCEL_APPROVAL),
    reuse(internal::SALES_INQUIRY_IMPORT_CREATE, inquiry::COMMANDS_SALES_CREATE),
    reuse(internal::SALES_INQUIRY_REINQUIRE_DRAFT, inquiry::COMMANDS_SALES_REINQUIRE),
    current_self(inquiry::QUERY_SALES_LIST),
    current_self(inquiry::QUERY_SALES_DETAIL),
    current_self(inquiry::COMMANDS_SALES_CREATE),
    current_self(inquiry::COMMANDS_SALES_SAVE),
    current_self(inquiry::COMMANDS_SALES_SUBMIT),
    current_self(inquiry::COMMANDS_SALES_ACCEPT),
    current_self(inquiry::COMMANDS_SALES_REINQUIRE),
    current_self(inquiry::COMMANDS_SALES_CLOSE),
    current_self(inquiry::IMPORT_SALES_IMPORT_PREVIEW),
    current_self(inquiry::CONVERSION_SALES_BASIS),
    current_self(inquiry::RECOVERY_SALES_RECOVER),
    // 采购单
    company(internal::PAYMENT_REQUEST_CANDIDATES),
    company_history(purchase_order::LIST),
    company_history(purchase_order::DETAIL),
    company_history(internal::PURCHASE_DELIVERY_OPTIONS_READ),
    company(purchase_order::CORRESPONDENCE_SEND_PURCHASE_EMAIL),
    company(purchase_order::CREATE),
    company(purchase_order::SAVE_DRAFT),
    company(purchase_order::VOID),
    company(purchase_order::SUBMIT),
    company(purchase_order::CANCEL_APPROVAL),
    company(purchase_order::MAINTAIN_SOURCE),
    // 选品
    current_self(selection::BOOKLET_LIST),
    current_self(selection::ADMIN_IMAGE),
    current_self(selection::BOOKLET_CREATE),
    current_self(selection::BOOKLET_DELETE_ITEM),
    current_self(selection::BOOKLET_PREPARE),
    current_self(selection::BOOKLET_PUBLISH),
    current_self(selection::BOOKLET_COPY_LINK),
    current_self(selection::BOOKLET_ROTATE_LINK),
    current_self(selection::BOOKLET_CLOSE),
    current_self(selection::BOOKLET_REVOKE),
    current_self(selection::BOOKLET_VOID),
    reuse(selection::PROPOSAL_LIST, selection::BOOKLET_LIST),
    reuse(selection::PROPOSAL_DETAIL, selection::ADMIN_IMAGE),
    reuse(internal::SALES_SELECTION_ORDER_BASIS, selection::ADMIN_IMAGE),
    // 供应商预存款
    deposit_spec(deposit::LIST),
    deposit_spec(deposit::DETAIL),
    deposit_spec(deposit::OPEN),
    deposit_spec(deposit::REQUEST_RECHARGE),
    deposit_spec(deposit::APPROVE_RECHARGE),
    deposit_spec(deposit::PAY_RECHARGE),
    deposit_spec(deposit::RECEIVE_RECHARGE),
    deposit_spec(deposit::AUTHORIZE),
    deposit_spec(deposit::REVOKE),
    deposit_spec(deposit::RECONCILE),
    deposit_spec(deposit::RESOLUTION_RESOLVE),
    deposit_spec(deposit::RESOLUTION_RELEASE),
    deposit_spec(deposit::RESERVE),
    deposit_spec(deposit::SETTLE),
    deposit_spec(deposit::RETURN_USAGE),
    deposit_spec(deposit::BIND),
    // 集成异常、供应商、商品、供给与履约订单
    current_self(integration::ERROR_TASK_LIST),
    current_self(integration::ERROR_TASK_DETAIL),
    current_self(integration::DIFFERENCE_LIST),
    current_self(integration::DIFFERENCE_DETAIL),
    company(supplier::LIST),
    company(supplier::DETAIL),
    company(supplier::IMPORT_SUPPLIER_IMPORT),
    company(supplier::HANDOVER),
    company(supplier::DELETE),
    company(catalog::PRODUCT_PRODUCT_LIST),
    company(catalog::PRODUCT_PRODUCT_DETAIL),
    company(catalog::PRODUCT_PRODUCT_CREATE),
    company(catalog::PRODUCT_PRODUCT_UPDATE),
    company(offering::LIST),
    company(offering::DETAIL),
    company(offering::CREATE),
    company(internal::SUPPLIER_OFFERING_CREATED_RESULT_READ),
    company(offering::REVISE),
    company(internal::SUPPLIER_OFFERING_AVAILABILITY_UPDATE),
    company(fulfillment::ORDER_LIST),
    company(fulfillment::ORDER_DETAIL),
    company(fulfillment::ORDER_INVESTIGATION),
    company(fulfillment::ORDER_TASK_COMPLETION),
    company(fulfillment::ORDER_SUBMIT),
    company(fulfillment::ORDER_CANCEL),
    company(fulfillment::ORDER_REFUND),
    company(fulfillment::ORDER_REJECT),
    company(fulfillment::ORDER_HANDOVER),
    // 目录与查询资格
    current(access::PERSON_DIRECTORY_GET_QUALIFICATION).with_types(PERSON),
    wh(warehouse::LIST).with_types(OBJECT),
    current(access::PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE).with_types(PERSON),
    current(access::PERSON_DIRECTORY_LIST_SALESPEOPLE).with_types(PERSON),
    current(access::PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE).with_types(PERSON),
];

pub(super) const TASK_ACTIONS: &[Permission] = &[
    internal::CONNECTION_CATALOG_READ,
    internal::CONNECTION_PROTOCOL_READ,
    internal::CONNECTION_REFERENCE_TICKETS,
    internal::APPROVAL_MANAGEMENT_READ,
    internal::APPROVAL_INSTANCE_READ,
    internal::APPROVAL_STARTED_LIST,
    internal::APPROVAL_MINE_LIST,
    internal::APPROVAL_RECOVER,
    internal::APPROVAL_COPY_LIST,
    internal::APPROVAL_COPY_DETAIL,
    internal::APPROVAL_COPY_READ,
    internal::PRODUCT_IMPORT_JOB_READ,
    internal::SUPPLIER_IMPORT_JOB_READ,
    internal::BRAND_IMPORT_JOB_READ,
    internal::CUSTOMER_IMPORT_JOB_READ,
    internal::DATA_TRANSFER_JOB_READ,
    internal::CATALOG_PREVIEW_DETAIL,
    internal::SALES_INQUIRY_IMPORT_PREVIEW,
    internal::PORTAL_MATERIAL_READ,
    internal::PORTAL_MATERIAL_READ_OFFERING,
    internal::PORTAL_MATERIAL_REVIEW_COOPERATION,
    internal::PORTAL_MATERIAL_REVIEW_OFFERING_CREATE,
    internal::PORTAL_MATERIAL_REVIEW_OFFERING_UPDATE,
    internal::PORTAL_MATERIAL_REVIEW_NEW_PRODUCT,
    file_asset::TRANSFER,
    file_asset::OWNED_TRANSFER,
    bulk_job::BACKGROUND_JOB_LIST,
    bulk_job::BACKGROUND_JOB_DETAIL,
    bulk_job::BACKGROUND_JOB_CANCEL,
    bulk_job::BACKGROUND_JOB_ITEM_LIST,
    supplier_api::CONNECTION_DETAIL,
    supplier_api::JD_VOP_AUTHORIZE,
    approval_instance::INSTANCE_LIST,
    internal::WORK_ITEM_RECIPIENT,
    approval_instance::SUBMIT_DECISION,
    approval_instance::RESUME_CURRENT_APPROVER,
    inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL,
    approval_instance::CANCEL_BLOCKED,
    approval_instance::UPGRADE_BINDING,
    inquiry::QUERY_PROCUREMENT_LIST,
    inquiry::QUERY_PROCUREMENT_DETAIL,
    inquiry::COMMANDS_PROCUREMENT_CLAIM,
    inquiry::COMMANDS_PROCUREMENT_SUPPLEMENT,
    inquiry::COMMANDS_PROCUREMENT_REASSIGN,
    inquiry::INTERNAL_FEE_BASES,
    inquiry::COMMANDS_PROCUREMENT_SAVE_FEEDBACK,
    inquiry::COMMANDS_PROCUREMENT_RENEW,
    inquiry::COMMANDS_PROCUREMENT_CONVERT_EXPENSES,
    inquiry::RECOVERY_PROCUREMENT_RECOVER,
    settlement::STATEMENT_REVIEW_DECIDE,
    integration::DIFFERENCE_DECISION,
];

pub(super) const SOURCE_ACTIONS: &[Permission] = &[
    internal::INVOICE_POSTED_RESULT_READ,
    receivable::ACCOUNT_CREATE,
    receivable::CUSTOMER_RECEIPT_SUBMIT,
    receivable::INVOICE_POST,
    payment::CREDIT_LIST,
    returns::CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT,
    returns::SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT,
    returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT,
    returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT,
    cost::ENTRY_LIST,
    cost::ENTRY_DETAIL,
    cost::ENTRY_CREATE,
    cost::ALLOCATION_LIST,
    receivable::ACCOUNT_LIST,
    receivable::ACCOUNT_DETAIL,
    receivable::CUSTOMER_RECEIPT_LIST,
    receivable::CUSTOMER_RECEIPT_DETAIL,
    receivable::INVOICE_LIST,
    receivable::INVOICE_DETAIL,
    receivable::INVOICE_REQUEST_INVOICE_REQUEST_LIST,
    receivable::INVOICE_REQUEST_INVOICE_REQUEST_DETAIL,
    internal::INVOICE_REQUEST_SUBMIT_SOURCE,
    internal::INVOICE_REQUEST_CANCEL_SOURCE,
    payable::ACCOUNT_LIST,
    payable::ACCOUNT_DETAIL,
    payment::LIST,
    payment::DETAIL,
    payment::CREATE,
    payment::UPDATE,
    payment::SUBMIT,
    payment::WITHDRAW,
    payment::CLOSE,
    payable::SUPPLIER_PAYMENT_LIST,
    payable::SUPPLIER_PAYMENT_DETAIL,
    payable::PURCHASE_INVOICE_ALLOCATION_LIST,
];
