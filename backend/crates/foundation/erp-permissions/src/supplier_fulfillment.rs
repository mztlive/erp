//! supplier_fulfillment 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 读取自动执行配送资料。
pub const EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP: Permission = ORDER_DETAIL;

/// 补充自动执行配送资料。
pub const EXECUTION_SETUP_SAVE_SUPPLIER_ORDER_EXECUTION_SETUP: Permission = ORDER_SUBMIT;

/// 查询供应商收货地址候选。
pub const EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_ADDRESS_CANDIDATES: Permission = ORDER_SUBMIT;

/// 查询原供应商执行结果。
pub const EXECUTION_SETUP_QUERY_SUPPLIER_ORDER_EXECUTION_RESULT: Permission = ORDER_SUBMIT;

/// 查询供应商履约订单列表。
pub const ORDER_LIST: Permission = Permission::new(resources::SUPPLIER_FULFILLMENT_ORDER, "list");

/// 查询供应商履约订单详情。
pub const ORDER_DETAIL: Permission = Permission::new(resources::SUPPLIER_FULFILLMENT_ORDER, "detail");

/// 从订单入口调查供应商原动作结果。
pub const ORDER_INVESTIGATION: Permission =
    Permission::new(resources::SUPPLIER_FULFILLMENT_ORDER, "investigate");

/// 从正式任务调查供应商原动作结果。
pub const ORDER_TASK_INVESTIGATION: Permission = ORDER_INVESTIGATION;

/// 根据已验证结果完成供应商履约任务。
pub const ORDER_TASK_COMPLETION: Permission =
    Permission::new(resources::SUPPLIER_FULFILLMENT_ORDER, "complete");

/// 向供应商提交下单。
pub const ORDER_SUBMIT: Permission = Permission::new(resources::SUPPLIER_FULFILLMENT_ORDER, "submit");

/// 向供应商提交取消。
pub const ORDER_CANCEL: Permission = Permission::new(resources::SUPPLIER_FULFILLMENT_ORDER, "cancel");

/// 向供应商提交退款。
pub const ORDER_REFUND: Permission = Permission::new(resources::SUPPLIER_FULFILLMENT_ORDER, "refund");

/// 登记供应商拒单结果。
pub const ORDER_REJECT: Permission = Permission::new(resources::SUPPLIER_FULFILLMENT_ORDER, "reject");

/// 登记供应商退款成功结果。
pub const SUPPLIER_REFUND_FACT_POST: Permission = Permission::new(resources::SUPPLIER_REFUND_FACT, "post");

/// 交接供应商履约订单跟进人。
pub const ORDER_HANDOVER: Permission = Permission::new(resources::SUPPLIER_FULFILLMENT_ORDER, "handover");

/// 查询供应商履约订单交接待选目标。
pub const ORDER_HANDOVER_CANDIDATES: Permission = ORDER_HANDOVER;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "ORDER_LIST",
        PermissionDef {
            permission: &ORDER_LIST,
            group: groups::SUPPLIER_FULFILLMENT,
            description: "查询供应商履约订单列表",
        },
    ),
    (
        "ORDER_DETAIL",
        PermissionDef {
            permission: &ORDER_DETAIL,
            group: groups::SUPPLIER_FULFILLMENT,
            description: "查询供应商履约订单详情",
        },
    ),
    (
        "ORDER_INVESTIGATION",
        PermissionDef {
            permission: &ORDER_INVESTIGATION,
            group: groups::SUPPLIER_FULFILLMENT,
            description: "从订单入口调查供应商原动作结果",
        },
    ),
    (
        "ORDER_TASK_COMPLETION",
        PermissionDef {
            permission: &ORDER_TASK_COMPLETION,
            group: groups::SUPPLIER_FULFILLMENT,
            description: "根据已验证结果完成供应商履约任务",
        },
    ),
    (
        "ORDER_SUBMIT",
        PermissionDef {
            permission: &ORDER_SUBMIT,
            group: groups::SUPPLIER_FULFILLMENT,
            description: "向供应商提交下单",
        },
    ),
    (
        "ORDER_CANCEL",
        PermissionDef {
            permission: &ORDER_CANCEL,
            group: groups::SUPPLIER_FULFILLMENT,
            description: "向供应商提交取消",
        },
    ),
    (
        "ORDER_REFUND",
        PermissionDef {
            permission: &ORDER_REFUND,
            group: groups::SUPPLIER_FULFILLMENT,
            description: "向供应商提交退款",
        },
    ),
    (
        "ORDER_REJECT",
        PermissionDef {
            permission: &ORDER_REJECT,
            group: groups::SUPPLIER_FULFILLMENT,
            description: "登记供应商拒单结果",
        },
    ),
    (
        "SUPPLIER_REFUND_FACT_POST",
        PermissionDef {
            permission: &SUPPLIER_REFUND_FACT_POST,
            group: groups::SUPPLIER_FULFILLMENT,
            description: "登记供应商退款成功结果",
        },
    ),
    (
        "ORDER_HANDOVER",
        PermissionDef {
            permission: &ORDER_HANDOVER,
            group: groups::SUPPLIER_FULFILLMENT,
            description: "交接供应商履约订单跟进人",
        },
    ),
];
