//! legacy_import 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询导入批次列表。
pub const BATCH_LIST: PermissionDef = PermissionDef {
    code: "legacy_import_batch:list",
    group: groups::LEGACY_IMPORT,
    description: "查询导入批次列表",
};

/// 创建导入批次。
pub const BATCH_CREATE: PermissionDef = PermissionDef {
    code: "legacy_import_batch:create",
    group: groups::LEGACY_IMPORT,
    description: "创建导入批次",
};

/// 查询导入批次详情。
pub const BATCH_DETAIL: PermissionDef = PermissionDef {
    code: "legacy_import_batch:detail",
    group: groups::LEGACY_IMPORT,
    description: "查询导入批次详情",
};

/// 应用导入批次。
pub const BATCH_APPLY: PermissionDef = PermissionDef {
    code: "legacy_import_batch:apply",
    group: groups::LEGACY_IMPORT,
    description: "应用导入批次",
};

/// 执行导入应用命令。
pub const EXECUTION_COMMAND: PermissionDef = PermissionDef {
    code: "legacy_import_batch:execute",
    group: groups::LEGACY_IMPORT,
    description: "执行导入应用命令",
};

/// 查询导入行列表。
pub const ROW_LIST: PermissionDef = PermissionDef {
    code: "legacy_import_row:list",
    group: groups::LEGACY_IMPORT,
    description: "查询导入行列表",
};

/// 查询导入确认事实列表。
pub const CONFIRMATION_LIST: PermissionDef = PermissionDef {
    code: "legacy_import_confirmation:list",
    group: groups::LEGACY_IMPORT,
    description: "查询导入确认事实列表",
};

/// 创建导入确认事实。
pub const CONFIRMATION_CREATE: PermissionDef = PermissionDef {
    code: "legacy_import_confirmation:create",
    group: groups::LEGACY_IMPORT,
    description: "创建导入确认事实",
};

/// 完成导入业务确认任务。
pub const CONFIRMATION_COMPLETE: PermissionDef = PermissionDef {
    code: "legacy_import_confirmation:complete",
    group: groups::LEGACY_IMPORT,
    description: "完成导入业务确认任务",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("BATCH_LIST", BATCH_LIST),
    ("BATCH_CREATE", BATCH_CREATE),
    ("BATCH_DETAIL", BATCH_DETAIL),
    ("BATCH_APPLY", BATCH_APPLY),
    ("EXECUTION_COMMAND", EXECUTION_COMMAND),
    ("ROW_LIST", ROW_LIST),
    ("CONFIRMATION_LIST", CONFIRMATION_LIST),
    ("CONFIRMATION_CREATE", CONFIRMATION_CREATE),
    ("CONFIRMATION_COMPLETE", CONFIRMATION_COMPLETE),
];
