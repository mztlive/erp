//! supplier_fulfillment 的接口权限目录。
use super::{PermissionDef, groups};

/// 读取自动执行配送资料。
pub const EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP: PermissionDef =
    ORDER_DETAIL.described(groups::SUPPLIER_FULFILLMENT, "读取自动执行配送资料");

/// 补充自动执行配送资料。
pub const EXECUTION_SETUP_SAVE_SUPPLIER_ORDER_EXECUTION_SETUP: PermissionDef =
    ORDER_SUBMIT.described(groups::SUPPLIER_FULFILLMENT, "补充自动执行配送资料");

/// 查询供应商收货地址候选。
pub const EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_ADDRESS_CANDIDATES: PermissionDef =
    ORDER_SUBMIT.described(groups::SUPPLIER_FULFILLMENT, "查询供应商收货地址候选");

/// 查询原供应商执行结果。
pub const EXECUTION_SETUP_QUERY_SUPPLIER_ORDER_EXECUTION_RESULT: PermissionDef =
    ORDER_SUBMIT.described(groups::SUPPLIER_FULFILLMENT, "查询原供应商执行结果");

/// 查询供应商履约订单列表。
pub const ORDER_LIST: PermissionDef = PermissionDef {
    code: "supplier_fulfillment_order:list",
    group: groups::SUPPLIER_FULFILLMENT,
    description: "查询供应商履约订单列表",
};

/// 查询供应商履约订单详情。
pub const ORDER_DETAIL: PermissionDef = PermissionDef {
    code: "supplier_fulfillment_order:detail",
    group: groups::SUPPLIER_FULFILLMENT,
    description: "查询供应商履约订单详情",
};

/// 从订单入口调查供应商原动作结果。
pub const ORDER_INVESTIGATION: PermissionDef = PermissionDef {
    code: "supplier_fulfillment_order:investigate",
    group: groups::SUPPLIER_FULFILLMENT,
    description: "从订单入口调查供应商原动作结果",
};

/// 从正式任务调查供应商原动作结果。
pub const ORDER_TASK_INVESTIGATION: PermissionDef =
    ORDER_INVESTIGATION.described(groups::SUPPLIER_FULFILLMENT, "从正式任务调查供应商原动作结果");

/// 根据已验证结果完成供应商履约任务。
pub const ORDER_TASK_COMPLETION: PermissionDef = PermissionDef {
    code: "supplier_fulfillment_order:complete",
    group: groups::SUPPLIER_FULFILLMENT,
    description: "根据已验证结果完成供应商履约任务",
};

/// 向供应商提交下单。
pub const ORDER_SUBMIT: PermissionDef = PermissionDef {
    code: "supplier_fulfillment_order:submit",
    group: groups::SUPPLIER_FULFILLMENT,
    description: "向供应商提交下单",
};

/// 向供应商提交取消。
pub const ORDER_CANCEL: PermissionDef = PermissionDef {
    code: "supplier_fulfillment_order:cancel",
    group: groups::SUPPLIER_FULFILLMENT,
    description: "向供应商提交取消",
};

/// 向供应商提交退款。
pub const ORDER_REFUND: PermissionDef = PermissionDef {
    code: "supplier_fulfillment_order:refund",
    group: groups::SUPPLIER_FULFILLMENT,
    description: "向供应商提交退款",
};

/// 登记供应商拒单结果。
pub const ORDER_REJECT: PermissionDef = PermissionDef {
    code: "supplier_fulfillment_order:reject",
    group: groups::SUPPLIER_FULFILLMENT,
    description: "登记供应商拒单结果",
};

/// 登记供应商退款成功结果。
pub const SUPPLIER_REFUND_FACT_POST: PermissionDef = PermissionDef {
    code: "supplier_refund_fact:post",
    group: groups::SUPPLIER_FULFILLMENT,
    description: "登记供应商退款成功结果",
};

/// 交接供应商履约订单跟进人。
pub const ORDER_HANDOVER: PermissionDef = PermissionDef {
    code: "supplier_fulfillment_order:handover",
    group: groups::SUPPLIER_FULFILLMENT,
    description: "交接供应商履约订单跟进人",
};

/// 查询供应商履约订单交接待选目标。
pub const ORDER_HANDOVER_CANDIDATES: PermissionDef =
    ORDER_HANDOVER.described(groups::SUPPLIER_FULFILLMENT, "查询供应商履约订单交接待选目标");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP", EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP),
    (
        "EXECUTION_SETUP_SAVE_SUPPLIER_ORDER_EXECUTION_SETUP",
        EXECUTION_SETUP_SAVE_SUPPLIER_ORDER_EXECUTION_SETUP,
    ),
    (
        "EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_ADDRESS_CANDIDATES",
        EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_ADDRESS_CANDIDATES,
    ),
    (
        "EXECUTION_SETUP_QUERY_SUPPLIER_ORDER_EXECUTION_RESULT",
        EXECUTION_SETUP_QUERY_SUPPLIER_ORDER_EXECUTION_RESULT,
    ),
    ("ORDER_LIST", ORDER_LIST),
    ("ORDER_DETAIL", ORDER_DETAIL),
    ("ORDER_INVESTIGATION", ORDER_INVESTIGATION),
    ("ORDER_TASK_INVESTIGATION", ORDER_TASK_INVESTIGATION),
    ("ORDER_TASK_COMPLETION", ORDER_TASK_COMPLETION),
    ("ORDER_SUBMIT", ORDER_SUBMIT),
    ("ORDER_CANCEL", ORDER_CANCEL),
    ("ORDER_REFUND", ORDER_REFUND),
    ("ORDER_REJECT", ORDER_REJECT),
    ("SUPPLIER_REFUND_FACT_POST", SUPPLIER_REFUND_FACT_POST),
    ("ORDER_HANDOVER", ORDER_HANDOVER),
    ("ORDER_HANDOVER_CANDIDATES", ORDER_HANDOVER_CANDIDATES),
];
