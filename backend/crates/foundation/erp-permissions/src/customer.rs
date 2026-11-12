//! customer 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 创建完整客户资料。
pub const PROFILE_CREATE: Permission = CREATE;

/// 修订完整客户资料。
pub const PROFILE_UPDATE: Permission = UPDATE;

/// 查询完整客户资料。
pub const PROFILE_DETAIL: Permission = DETAIL;

/// 查询客户中心合同与销售摘要。
pub const CENTER_RELATED: Permission = DETAIL;

/// 查询客户中心应收汇总。
pub const CENTER_RECEIVABLE: Permission = DETAIL;

/// 查询客户资料保存结果。
pub const PROFILE_COMMAND_DETAIL: Permission = DETAIL;

/// 查看客户敏感字段。
pub const SENSITIVE_REVEAL: Permission = Permission::new(resources::CUSTOMER_SENSITIVE, "reveal");

/// 查询客户列表。
pub const LIST: Permission = Permission::new(resources::CUSTOMER, "list");

/// 查询全部有权客户。
pub const ALL_AUTHORIZED_LIST: Permission = Permission::new(resources::CUSTOMER_SCOPE, "detail");

/// 创建客户。
pub const CREATE: Permission = Permission::new(resources::CUSTOMER, "create");

/// 查询客户详情。
pub const DETAIL: Permission = Permission::new(resources::CUSTOMER, "detail");

/// 更新客户。
pub const UPDATE: Permission = Permission::new(resources::CUSTOMER, "update");

/// 删除客户。
pub const DELETE: Permission = Permission::new(resources::CUSTOMER, "delete");

/// 查询客户归属列表。
pub const ASSIGNMENT_LIST: Permission = Permission::new(resources::CUSTOMER_ASSIGNMENT, "list");

/// 调整客户归属。
pub const ASSIGNMENT_APPLY: Permission = Permission::new(resources::CUSTOMER_ASSIGNMENT, "create");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "SENSITIVE_REVEAL",
        PermissionDef {
            permission: &SENSITIVE_REVEAL,
            group: groups::CUSTOMER,
            description: "查看客户敏感字段",
        },
    ),
    ("LIST", PermissionDef { permission: &LIST, group: groups::CUSTOMER, description: "查询客户列表" }),
    (
        "ALL_AUTHORIZED_LIST",
        PermissionDef {
            permission: &ALL_AUTHORIZED_LIST,
            group: groups::CUSTOMER,
            description: "查询全部有权客户",
        },
    ),
    ("CREATE", PermissionDef { permission: &CREATE, group: groups::CUSTOMER, description: "创建客户" }),
    ("DETAIL", PermissionDef { permission: &DETAIL, group: groups::CUSTOMER, description: "查询客户详情" }),
    ("UPDATE", PermissionDef { permission: &UPDATE, group: groups::CUSTOMER, description: "更新客户" }),
    ("DELETE", PermissionDef { permission: &DELETE, group: groups::CUSTOMER, description: "删除客户" }),
    (
        "ASSIGNMENT_LIST",
        PermissionDef {
            permission: &ASSIGNMENT_LIST,
            group: groups::CUSTOMER,
            description: "查询客户归属列表",
        },
    ),
    (
        "ASSIGNMENT_APPLY",
        PermissionDef {
            permission: &ASSIGNMENT_APPLY, group: groups::CUSTOMER, description: "调整客户归属"
        },
    ),
];
