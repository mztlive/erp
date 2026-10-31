//! integration_ops 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询入站消息列表。
pub const INBOX_MESSAGE_LIST: PermissionDef = PermissionDef {
    code: "inbox_message:list",
    group: groups::INTEGRATION_OPS,
    description: "查询入站消息列表",
};

/// 查询入站消息详情。
pub const INBOX_MESSAGE_DETAIL: PermissionDef = PermissionDef {
    code: "inbox_message:detail",
    group: groups::INTEGRATION_OPS,
    description: "查询入站消息详情",
};

/// 登记入站消息。
pub const INBOX_MESSAGE_REGISTER: PermissionDef = PermissionDef {
    code: "inbox_message:register",
    group: groups::INTEGRATION_OPS,
    description: "登记入站消息",
};

/// 回写入站消息处理结果。
pub const INBOX_MESSAGE_WRITE_BACK: PermissionDef = PermissionDef {
    code: "inbox_message:writeback",
    group: groups::INTEGRATION_OPS,
    description: "回写入站消息处理结果",
};

/// 查询集成错误任务列表。
pub const ERROR_TASK_LIST: PermissionDef = PermissionDef {
    code: "integration_error_task:list",
    group: groups::INTEGRATION_OPS,
    description: "查询集成错误任务列表",
};

/// 查询集成错误任务详情。
pub const ERROR_TASK_DETAIL: PermissionDef = PermissionDef {
    code: "integration_error_task:detail",
    group: groups::INTEGRATION_OPS,
    description: "查询集成错误任务详情",
};

/// 登记集成错误任务。
pub const ERROR_TASK_CREATE: PermissionDef = PermissionDef {
    code: "integration_error_task:create",
    group: groups::INTEGRATION_OPS,
    description: "登记集成错误任务",
};

/// 执行集成任务非终结动作。
pub const INTEGRATION_TASK_ACTION: PermissionDef = PermissionDef {
    code: "integration_task:process",
    group: groups::INTEGRATION_OPS,
    description: "执行集成任务非终结动作",
};

/// 根据已验证结果完成集成任务。
pub const INTEGRATION_TASK_COMPLETION: PermissionDef = PermissionDef {
    code: "integration_task:complete",
    group: groups::INTEGRATION_OPS,
    description: "根据已验证结果完成集成任务",
};

/// 查询对账差异列表。
pub const DIFFERENCE_LIST: PermissionDef = PermissionDef {
    code: "reconciliation_difference:list",
    group: groups::INTEGRATION_OPS,
    description: "查询对账差异列表",
};

/// 查询对账差异详情。
pub const DIFFERENCE_DETAIL: PermissionDef = PermissionDef {
    code: "reconciliation_difference:detail",
    group: groups::INTEGRATION_OPS,
    description: "查询对账差异详情",
};

/// 登记对账差异。
pub const DIFFERENCE_CREATE: PermissionDef = PermissionDef {
    code: "reconciliation_difference:create",
    group: groups::INTEGRATION_OPS,
    description: "登记对账差异",
};

/// 提交无正式任务的对账差异决定。
pub const DIFFERENCE_DECISION: PermissionDef = PermissionDef {
    code: "reconciliation_difference:decide",
    group: groups::INTEGRATION_OPS,
    description: "提交无正式任务的对账差异决定",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("INBOX_MESSAGE_LIST", INBOX_MESSAGE_LIST),
    ("INBOX_MESSAGE_DETAIL", INBOX_MESSAGE_DETAIL),
    ("INBOX_MESSAGE_REGISTER", INBOX_MESSAGE_REGISTER),
    ("INBOX_MESSAGE_WRITE_BACK", INBOX_MESSAGE_WRITE_BACK),
    ("ERROR_TASK_LIST", ERROR_TASK_LIST),
    ("ERROR_TASK_DETAIL", ERROR_TASK_DETAIL),
    ("ERROR_TASK_CREATE", ERROR_TASK_CREATE),
    ("INTEGRATION_TASK_ACTION", INTEGRATION_TASK_ACTION),
    ("INTEGRATION_TASK_COMPLETION", INTEGRATION_TASK_COMPLETION),
    ("DIFFERENCE_LIST", DIFFERENCE_LIST),
    ("DIFFERENCE_DETAIL", DIFFERENCE_DETAIL),
    ("DIFFERENCE_CREATE", DIFFERENCE_CREATE),
    ("DIFFERENCE_DECISION", DIFFERENCE_DECISION),
];
