//! integration_ops 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询入站消息列表。
pub const INBOX_MESSAGE_LIST: Permission = Permission::new(resources::INBOX_MESSAGE, "list");

/// 查询入站消息详情。
pub const INBOX_MESSAGE_DETAIL: Permission = Permission::new(resources::INBOX_MESSAGE, "detail");

/// 登记入站消息。
pub const INBOX_MESSAGE_REGISTER: Permission = Permission::new(resources::INBOX_MESSAGE, "register");

/// 回写入站消息处理结果。
pub const INBOX_MESSAGE_WRITE_BACK: Permission = Permission::new(resources::INBOX_MESSAGE, "writeback");

/// 查询集成错误任务列表。
pub const ERROR_TASK_LIST: Permission = Permission::new(resources::INTEGRATION_ERROR_TASK, "list");

/// 查询集成错误任务详情。
pub const ERROR_TASK_DETAIL: Permission = Permission::new(resources::INTEGRATION_ERROR_TASK, "detail");

/// 登记集成错误任务。
pub const ERROR_TASK_CREATE: Permission = Permission::new(resources::INTEGRATION_ERROR_TASK, "create");

/// 执行集成任务非终结动作。
pub const INTEGRATION_TASK_ACTION: Permission = Permission::new(resources::INTEGRATION_TASK, "process");

/// 根据已验证结果完成集成任务。
pub const INTEGRATION_TASK_COMPLETION: Permission = Permission::new(resources::INTEGRATION_TASK, "complete");

/// 查询对账差异列表。
pub const DIFFERENCE_LIST: Permission = Permission::new(resources::RECONCILIATION_DIFFERENCE, "list");

/// 查询对账差异详情。
pub const DIFFERENCE_DETAIL: Permission = Permission::new(resources::RECONCILIATION_DIFFERENCE, "detail");

/// 登记对账差异。
pub const DIFFERENCE_CREATE: Permission = Permission::new(resources::RECONCILIATION_DIFFERENCE, "create");

/// 提交无正式任务的对账差异决定。
pub const DIFFERENCE_DECISION: Permission = Permission::new(resources::RECONCILIATION_DIFFERENCE, "decide");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "INBOX_MESSAGE_LIST",
        PermissionDef {
            permission: &INBOX_MESSAGE_LIST,
            group: groups::INTEGRATION_OPS,
            description: "查询入站消息列表",
        },
    ),
    (
        "INBOX_MESSAGE_DETAIL",
        PermissionDef {
            permission: &INBOX_MESSAGE_DETAIL,
            group: groups::INTEGRATION_OPS,
            description: "查询入站消息详情",
        },
    ),
    (
        "INBOX_MESSAGE_REGISTER",
        PermissionDef {
            permission: &INBOX_MESSAGE_REGISTER,
            group: groups::INTEGRATION_OPS,
            description: "登记入站消息",
        },
    ),
    (
        "INBOX_MESSAGE_WRITE_BACK",
        PermissionDef {
            permission: &INBOX_MESSAGE_WRITE_BACK,
            group: groups::INTEGRATION_OPS,
            description: "回写入站消息处理结果",
        },
    ),
    (
        "ERROR_TASK_LIST",
        PermissionDef {
            permission: &ERROR_TASK_LIST,
            group: groups::INTEGRATION_OPS,
            description: "查询集成错误任务列表",
        },
    ),
    (
        "ERROR_TASK_DETAIL",
        PermissionDef {
            permission: &ERROR_TASK_DETAIL,
            group: groups::INTEGRATION_OPS,
            description: "查询集成错误任务详情",
        },
    ),
    (
        "ERROR_TASK_CREATE",
        PermissionDef {
            permission: &ERROR_TASK_CREATE,
            group: groups::INTEGRATION_OPS,
            description: "登记集成错误任务",
        },
    ),
    (
        "INTEGRATION_TASK_ACTION",
        PermissionDef {
            permission: &INTEGRATION_TASK_ACTION,
            group: groups::INTEGRATION_OPS,
            description: "执行集成任务非终结动作",
        },
    ),
    (
        "INTEGRATION_TASK_COMPLETION",
        PermissionDef {
            permission: &INTEGRATION_TASK_COMPLETION,
            group: groups::INTEGRATION_OPS,
            description: "根据已验证结果完成集成任务",
        },
    ),
    (
        "DIFFERENCE_LIST",
        PermissionDef {
            permission: &DIFFERENCE_LIST,
            group: groups::INTEGRATION_OPS,
            description: "查询对账差异列表",
        },
    ),
    (
        "DIFFERENCE_DETAIL",
        PermissionDef {
            permission: &DIFFERENCE_DETAIL,
            group: groups::INTEGRATION_OPS,
            description: "查询对账差异详情",
        },
    ),
    (
        "DIFFERENCE_CREATE",
        PermissionDef {
            permission: &DIFFERENCE_CREATE,
            group: groups::INTEGRATION_OPS,
            description: "登记对账差异",
        },
    ),
    (
        "DIFFERENCE_DECISION",
        PermissionDef {
            permission: &DIFFERENCE_DECISION,
            group: groups::INTEGRATION_OPS,
            description: "提交无正式任务的对账差异决定",
        },
    ),
];
