//! customer 的接口权限目录。
use super::{PermissionDef, groups};

/// 创建完整客户资料。
pub const PROFILE_CREATE: PermissionDef = CREATE.described(groups::CUSTOMER, "创建完整客户资料");

/// 修订完整客户资料。
pub const PROFILE_UPDATE: PermissionDef = UPDATE.described(groups::CUSTOMER, "修订完整客户资料");

/// 查询完整客户资料。
pub const PROFILE_DETAIL: PermissionDef = DETAIL.described(groups::CUSTOMER, "查询完整客户资料");

/// 查询客户中心合同与销售摘要。
pub const CENTER_RELATED: PermissionDef = DETAIL.described(groups::CUSTOMER, "查询客户中心合同与销售摘要");

/// 查询客户中心应收汇总。
pub const CENTER_RECEIVABLE: PermissionDef = DETAIL.described(groups::CUSTOMER, "查询客户中心应收汇总");

/// 查询客户资料保存结果。
pub const PROFILE_COMMAND_DETAIL: PermissionDef = DETAIL.described(groups::CUSTOMER, "查询客户资料保存结果");

/// 查看客户敏感字段。
pub const SENSITIVE_REVEAL: PermissionDef = PermissionDef {
    code: "customer_sensitive:reveal",
    group: groups::CUSTOMER,
    description: "查看客户敏感字段",
};

/// 查询客户列表。
pub const LIST: PermissionDef =
    PermissionDef { code: "customer:list", group: groups::CUSTOMER, description: "查询客户列表" };

/// 查询全部有权客户。
pub const ALL_AUTHORIZED_LIST: PermissionDef =
    PermissionDef {
        code: "customer_scope:detail", group: groups::CUSTOMER, description: "查询全部有权客户"
    };

/// 创建客户。
pub const CREATE: PermissionDef =
    PermissionDef { code: "customer:create", group: groups::CUSTOMER, description: "创建客户" };

/// 查询客户详情。
pub const DETAIL: PermissionDef =
    PermissionDef { code: "customer:detail", group: groups::CUSTOMER, description: "查询客户详情" };

/// 更新客户。
pub const UPDATE: PermissionDef =
    PermissionDef { code: "customer:update", group: groups::CUSTOMER, description: "更新客户" };

/// 删除客户。
pub const DELETE: PermissionDef =
    PermissionDef { code: "customer:delete", group: groups::CUSTOMER, description: "删除客户" };

/// 查询客户归属列表。
pub const ASSIGNMENT_LIST: PermissionDef = PermissionDef {
    code: "customer_assignment:list",
    group: groups::CUSTOMER,
    description: "查询客户归属列表",
};

/// 调整客户归属。
pub const ASSIGNMENT_APPLY: PermissionDef = PermissionDef {
    code: "customer_assignment:create",
    group: groups::CUSTOMER,
    description: "调整客户归属",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("PROFILE_CREATE", PROFILE_CREATE),
    ("PROFILE_UPDATE", PROFILE_UPDATE),
    ("PROFILE_DETAIL", PROFILE_DETAIL),
    ("CENTER_RELATED", CENTER_RELATED),
    ("CENTER_RECEIVABLE", CENTER_RECEIVABLE),
    ("PROFILE_COMMAND_DETAIL", PROFILE_COMMAND_DETAIL),
    ("SENSITIVE_REVEAL", SENSITIVE_REVEAL),
    ("LIST", LIST),
    ("ALL_AUTHORIZED_LIST", ALL_AUTHORIZED_LIST),
    ("CREATE", CREATE),
    ("DETAIL", DETAIL),
    ("UPDATE", UPDATE),
    ("DELETE", DELETE),
    ("ASSIGNMENT_LIST", ASSIGNMENT_LIST),
    ("ASSIGNMENT_APPLY", ASSIGNMENT_APPLY),
];
