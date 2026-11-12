//! fulfillment 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询客户验收单详情。
pub const ACCEPTANCE_EVIDENCE_CUSTOMER_ACCEPTANCE_EVIDENCE: Permission =
    CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_DETAIL;

/// 查询客户验收单列表。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST: Permission =
    Permission::new(resources::CUSTOMER_ACCEPTANCE, "list");

/// 查询客户验收单详情。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_DETAIL: Permission =
    Permission::new(resources::CUSTOMER_ACCEPTANCE, "detail");

/// 创建客户验收单。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE: Permission =
    Permission::new(resources::CUSTOMER_ACCEPTANCE, "create");

/// 原子登记客户验收。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT: Permission =
    CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_POST;

/// 过账客户验收。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_POST: Permission =
    Permission::new(resources::CUSTOMER_ACCEPTANCE, "post");

/// 冲正客户验收。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_REVERSE: Permission =
    Permission::new(resources::CUSTOMER_ACCEPTANCE, "reverse");

/// 查询客户验收工作台。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_ELIGIBLE: Permission =
    CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST;

/// 查询发货单列表。
pub const DELIVERY_DELIVERY_LIST: Permission = Permission::new(resources::DELIVERY, "list");

/// 查询发货单详情。
pub const DELIVERY_DELIVERY_DETAIL: Permission = Permission::new(resources::DELIVERY, "detail");

/// 创建发货单。
pub const DELIVERY_DELIVERY_CREATE: Permission = Permission::new(resources::DELIVERY, "create");

/// 更新发货单。
pub const DELIVERY_DELIVERY_UPDATE: Permission = Permission::new(resources::DELIVERY, "update");

/// 过账发货。
pub const DELIVERY_DELIVERY_POST: Permission = Permission::new(resources::DELIVERY, "post");

/// 查询电子交付记录列表。
pub const ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST: Permission =
    Permission::new(resources::ELECTRONIC_DELIVERY, "list");

/// 查询电子交付记录详情。
pub const ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_DETAIL: Permission =
    ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST;

/// 创建电子交付记录。
pub const ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CREATE: Permission =
    Permission::new(resources::ELECTRONIC_DELIVERY, "create");

/// 确认电子交付。
pub const ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM: Permission =
    Permission::new(resources::ELECTRONIC_DELIVERY, "confirm");

/// 查询采购入库单列表。
pub const PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST: Permission =
    Permission::new(resources::PURCHASE_RECEIPT, "list");

/// 查询采购入库单详情。
pub const PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL: Permission =
    Permission::new(resources::PURCHASE_RECEIPT, "detail");

/// 创建采购入库单。
pub const PURCHASE_RECEIPT_PURCHASE_RECEIPT_CREATE: Permission =
    Permission::new(resources::PURCHASE_RECEIPT, "create");

/// 更新采购入库单。
pub const PURCHASE_RECEIPT_PURCHASE_RECEIPT_UPDATE: Permission =
    Permission::new(resources::PURCHASE_RECEIPT, "update");

/// 过账采购入库。
pub const PURCHASE_RECEIPT_PURCHASE_RECEIPT_POST: Permission =
    Permission::new(resources::PURCHASE_RECEIPT, "post");

/// 查询服务履约记录列表。
pub const SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST: Permission =
    Permission::new(resources::SERVICE_FULFILLMENT, "list");

/// 查询服务履约记录详情。
pub const SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_DETAIL: Permission =
    SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST;

/// 创建服务履约记录。
pub const SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CREATE: Permission =
    Permission::new(resources::SERVICE_FULFILLMENT, "create");

/// 确认服务履约。
pub const SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM: Permission =
    Permission::new(resources::SERVICE_FULFILLMENT, "confirm");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST",
        PermissionDef {
            permission: &CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST,
            group: groups::FULFILLMENT,
            description: "查询客户验收单列表",
        },
    ),
    (
        "CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_DETAIL",
        PermissionDef {
            permission: &CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_DETAIL,
            group: groups::FULFILLMENT,
            description: "查询客户验收单详情",
        },
    ),
    (
        "CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE",
        PermissionDef {
            permission: &CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE,
            group: groups::FULFILLMENT,
            description: "创建客户验收单",
        },
    ),
    (
        "CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_POST",
        PermissionDef {
            permission: &CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_POST,
            group: groups::FULFILLMENT,
            description: "过账客户验收",
        },
    ),
    (
        "CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_REVERSE",
        PermissionDef {
            permission: &CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_REVERSE,
            group: groups::FULFILLMENT,
            description: "冲正客户验收",
        },
    ),
    (
        "DELIVERY_DELIVERY_LIST",
        PermissionDef {
            permission: &DELIVERY_DELIVERY_LIST,
            group: groups::FULFILLMENT,
            description: "查询发货单列表",
        },
    ),
    (
        "DELIVERY_DELIVERY_DETAIL",
        PermissionDef {
            permission: &DELIVERY_DELIVERY_DETAIL,
            group: groups::FULFILLMENT,
            description: "查询发货单详情",
        },
    ),
    (
        "DELIVERY_DELIVERY_CREATE",
        PermissionDef {
            permission: &DELIVERY_DELIVERY_CREATE,
            group: groups::FULFILLMENT,
            description: "创建发货单",
        },
    ),
    (
        "DELIVERY_DELIVERY_UPDATE",
        PermissionDef {
            permission: &DELIVERY_DELIVERY_UPDATE,
            group: groups::FULFILLMENT,
            description: "更新发货单",
        },
    ),
    (
        "DELIVERY_DELIVERY_POST",
        PermissionDef {
            permission: &DELIVERY_DELIVERY_POST,
            group: groups::FULFILLMENT,
            description: "过账发货",
        },
    ),
    (
        "ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST",
        PermissionDef {
            permission: &ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST,
            group: groups::FULFILLMENT,
            description: "查询电子交付记录列表",
        },
    ),
    (
        "ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CREATE",
        PermissionDef {
            permission: &ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CREATE,
            group: groups::FULFILLMENT,
            description: "创建电子交付记录",
        },
    ),
    (
        "ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM",
        PermissionDef {
            permission: &ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM,
            group: groups::FULFILLMENT,
            description: "确认电子交付",
        },
    ),
    (
        "PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST",
        PermissionDef {
            permission: &PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST,
            group: groups::FULFILLMENT,
            description: "查询采购入库单列表",
        },
    ),
    (
        "PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL",
        PermissionDef {
            permission: &PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL,
            group: groups::FULFILLMENT,
            description: "查询采购入库单详情",
        },
    ),
    (
        "PURCHASE_RECEIPT_PURCHASE_RECEIPT_CREATE",
        PermissionDef {
            permission: &PURCHASE_RECEIPT_PURCHASE_RECEIPT_CREATE,
            group: groups::FULFILLMENT,
            description: "创建采购入库单",
        },
    ),
    (
        "PURCHASE_RECEIPT_PURCHASE_RECEIPT_UPDATE",
        PermissionDef {
            permission: &PURCHASE_RECEIPT_PURCHASE_RECEIPT_UPDATE,
            group: groups::FULFILLMENT,
            description: "更新采购入库单",
        },
    ),
    (
        "PURCHASE_RECEIPT_PURCHASE_RECEIPT_POST",
        PermissionDef {
            permission: &PURCHASE_RECEIPT_PURCHASE_RECEIPT_POST,
            group: groups::FULFILLMENT,
            description: "过账采购入库",
        },
    ),
    (
        "SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST",
        PermissionDef {
            permission: &SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST,
            group: groups::FULFILLMENT,
            description: "查询服务履约记录列表",
        },
    ),
    (
        "SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CREATE",
        PermissionDef {
            permission: &SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CREATE,
            group: groups::FULFILLMENT,
            description: "创建服务履约记录",
        },
    ),
    (
        "SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM",
        PermissionDef {
            permission: &SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM,
            group: groups::FULFILLMENT,
            description: "确认服务履约",
        },
    ),
];
