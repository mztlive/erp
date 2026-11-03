//! fulfillment 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询客户验收单详情。
pub const ACCEPTANCE_EVIDENCE_CUSTOMER_ACCEPTANCE_EVIDENCE: PermissionDef =
    CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_DETAIL;

/// 查询客户验收单列表。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST: PermissionDef = PermissionDef {
    code: "customer_acceptance:list",
    group: groups::FULFILLMENT,
    description: "查询客户验收单列表",
};

/// 查询客户验收单详情。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_DETAIL: PermissionDef = PermissionDef {
    code: "customer_acceptance:detail",
    group: groups::FULFILLMENT,
    description: "查询客户验收单详情",
};

/// 创建客户验收单。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE: PermissionDef = PermissionDef {
    code: "customer_acceptance:create",
    group: groups::FULFILLMENT,
    description: "创建客户验收单",
};

/// 原子登记客户验收。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT: PermissionDef =
    CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_POST.described(groups::FULFILLMENT, "原子登记客户验收");

/// 过账客户验收。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_POST: PermissionDef = PermissionDef {
    code: "customer_acceptance:post",
    group: groups::FULFILLMENT,
    description: "过账客户验收",
};

/// 冲正客户验收。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_REVERSE: PermissionDef = PermissionDef {
    code: "customer_acceptance:reverse",
    group: groups::FULFILLMENT,
    description: "冲正客户验收",
};

/// 查询客户验收工作台。
pub const CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_ELIGIBLE: PermissionDef =
    CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST.described(groups::FULFILLMENT, "查询客户验收工作台");

/// 查询发货单列表。
pub const DELIVERY_DELIVERY_LIST: PermissionDef =
    PermissionDef { code: "delivery:list", group: groups::FULFILLMENT, description: "查询发货单列表" };

/// 查询发货单详情。
pub const DELIVERY_DELIVERY_DETAIL: PermissionDef =
    PermissionDef {
        code: "delivery:detail", group: groups::FULFILLMENT, description: "查询发货单详情"
    };

/// 创建发货单。
pub const DELIVERY_DELIVERY_CREATE: PermissionDef =
    PermissionDef { code: "delivery:create", group: groups::FULFILLMENT, description: "创建发货单" };

/// 更新发货单。
pub const DELIVERY_DELIVERY_UPDATE: PermissionDef =
    PermissionDef { code: "delivery:update", group: groups::FULFILLMENT, description: "更新发货单" };

/// 过账发货。
pub const DELIVERY_DELIVERY_POST: PermissionDef =
    PermissionDef { code: "delivery:post", group: groups::FULFILLMENT, description: "过账发货" };

/// 查询电子交付记录列表。
pub const ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST: PermissionDef = PermissionDef {
    code: "electronic_delivery:list",
    group: groups::FULFILLMENT,
    description: "查询电子交付记录列表",
};

/// 查询电子交付记录详情。
pub const ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_DETAIL: PermissionDef =
    ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST.described(groups::FULFILLMENT, "查询电子交付记录详情");

/// 创建电子交付记录。
pub const ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CREATE: PermissionDef = PermissionDef {
    code: "electronic_delivery:create",
    group: groups::FULFILLMENT,
    description: "创建电子交付记录",
};

/// 确认电子交付。
pub const ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM: PermissionDef = PermissionDef {
    code: "electronic_delivery:confirm",
    group: groups::FULFILLMENT,
    description: "确认电子交付",
};

/// 查询采购入库单列表。
pub const PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST: PermissionDef = PermissionDef {
    code: "purchase_receipt:list",
    group: groups::FULFILLMENT,
    description: "查询采购入库单列表",
};

/// 查询采购入库单详情。
pub const PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL: PermissionDef = PermissionDef {
    code: "purchase_receipt:detail",
    group: groups::FULFILLMENT,
    description: "查询采购入库单详情",
};

/// 创建采购入库单。
pub const PURCHASE_RECEIPT_PURCHASE_RECEIPT_CREATE: PermissionDef = PermissionDef {
    code: "purchase_receipt:create",
    group: groups::FULFILLMENT,
    description: "创建采购入库单",
};

/// 更新采购入库单。
pub const PURCHASE_RECEIPT_PURCHASE_RECEIPT_UPDATE: PermissionDef = PermissionDef {
    code: "purchase_receipt:update",
    group: groups::FULFILLMENT,
    description: "更新采购入库单",
};

/// 过账采购入库。
pub const PURCHASE_RECEIPT_PURCHASE_RECEIPT_POST: PermissionDef =
    PermissionDef {
        code: "purchase_receipt:post", group: groups::FULFILLMENT, description: "过账采购入库"
    };

/// 查询服务履约记录列表。
pub const SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST: PermissionDef = PermissionDef {
    code: "service_fulfillment:list",
    group: groups::FULFILLMENT,
    description: "查询服务履约记录列表",
};

/// 查询服务履约记录详情。
pub const SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_DETAIL: PermissionDef =
    SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST.described(groups::FULFILLMENT, "查询服务履约记录详情");

/// 创建服务履约记录。
pub const SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CREATE: PermissionDef = PermissionDef {
    code: "service_fulfillment:create",
    group: groups::FULFILLMENT,
    description: "创建服务履约记录",
};

/// 确认服务履约。
pub const SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM: PermissionDef = PermissionDef {
    code: "service_fulfillment:confirm",
    group: groups::FULFILLMENT,
    description: "确认服务履约",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("ACCEPTANCE_EVIDENCE_CUSTOMER_ACCEPTANCE_EVIDENCE", ACCEPTANCE_EVIDENCE_CUSTOMER_ACCEPTANCE_EVIDENCE),
    ("CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST", CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST),
    ("CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_DETAIL", CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_DETAIL),
    ("CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE", CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE),
    ("CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT", CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT),
    ("CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_POST", CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_POST),
    ("CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_REVERSE", CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_REVERSE),
    ("CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_ELIGIBLE", CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_ELIGIBLE),
    ("DELIVERY_DELIVERY_LIST", DELIVERY_DELIVERY_LIST),
    ("DELIVERY_DELIVERY_DETAIL", DELIVERY_DELIVERY_DETAIL),
    ("DELIVERY_DELIVERY_CREATE", DELIVERY_DELIVERY_CREATE),
    ("DELIVERY_DELIVERY_UPDATE", DELIVERY_DELIVERY_UPDATE),
    ("DELIVERY_DELIVERY_POST", DELIVERY_DELIVERY_POST),
    ("ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST", ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST),
    ("ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_DETAIL", ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_DETAIL),
    ("ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CREATE", ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CREATE),
    ("ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM", ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM),
    ("PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST", PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST),
    ("PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL", PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL),
    ("PURCHASE_RECEIPT_PURCHASE_RECEIPT_CREATE", PURCHASE_RECEIPT_PURCHASE_RECEIPT_CREATE),
    ("PURCHASE_RECEIPT_PURCHASE_RECEIPT_UPDATE", PURCHASE_RECEIPT_PURCHASE_RECEIPT_UPDATE),
    ("PURCHASE_RECEIPT_PURCHASE_RECEIPT_POST", PURCHASE_RECEIPT_PURCHASE_RECEIPT_POST),
    ("SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST", SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST),
    ("SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_DETAIL", SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_DETAIL),
    ("SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CREATE", SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CREATE),
    ("SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM", SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM),
];
