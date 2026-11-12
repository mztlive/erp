//! legacy_import 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询导入批次列表。
pub const BATCH_LIST: Permission = Permission::new(resources::LEGACY_IMPORT_BATCH, "list");

/// 创建导入批次。
pub const BATCH_CREATE: Permission = Permission::new(resources::LEGACY_IMPORT_BATCH, "create");

/// 查询导入批次详情。
pub const BATCH_DETAIL: Permission = Permission::new(resources::LEGACY_IMPORT_BATCH, "detail");

/// 应用导入批次。
pub const BATCH_APPLY: Permission = Permission::new(resources::LEGACY_IMPORT_BATCH, "apply");

/// 执行导入应用命令。
pub const EXECUTION_COMMAND: Permission = Permission::new(resources::LEGACY_IMPORT_BATCH, "execute");

/// 查询导入行列表。
pub const ROW_LIST: Permission = Permission::new(resources::LEGACY_IMPORT_ROW, "list");

/// 查询导入确认事实列表。
pub const CONFIRMATION_LIST: Permission = Permission::new(resources::LEGACY_IMPORT_CONFIRMATION, "list");

/// 创建导入确认事实。
pub const CONFIRMATION_CREATE: Permission = Permission::new(resources::LEGACY_IMPORT_CONFIRMATION, "create");

/// 完成导入业务确认任务。
pub const CONFIRMATION_COMPLETE: Permission =
    Permission::new(resources::LEGACY_IMPORT_CONFIRMATION, "complete");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "BATCH_LIST",
        PermissionDef {
            permission: &BATCH_LIST,
            group: groups::LEGACY_IMPORT,
            description: "查询导入批次列表",
        },
    ),
    (
        "BATCH_CREATE",
        PermissionDef {
            permission: &BATCH_CREATE,
            group: groups::LEGACY_IMPORT,
            description: "创建导入批次",
        },
    ),
    (
        "BATCH_DETAIL",
        PermissionDef {
            permission: &BATCH_DETAIL,
            group: groups::LEGACY_IMPORT,
            description: "查询导入批次详情",
        },
    ),
    (
        "BATCH_APPLY",
        PermissionDef {
            permission: &BATCH_APPLY, group: groups::LEGACY_IMPORT, description: "应用导入批次"
        },
    ),
    (
        "EXECUTION_COMMAND",
        PermissionDef {
            permission: &EXECUTION_COMMAND,
            group: groups::LEGACY_IMPORT,
            description: "执行导入应用命令",
        },
    ),
    (
        "ROW_LIST",
        PermissionDef {
            permission: &ROW_LIST, group: groups::LEGACY_IMPORT, description: "查询导入行列表"
        },
    ),
    (
        "CONFIRMATION_LIST",
        PermissionDef {
            permission: &CONFIRMATION_LIST,
            group: groups::LEGACY_IMPORT,
            description: "查询导入确认事实列表",
        },
    ),
    (
        "CONFIRMATION_CREATE",
        PermissionDef {
            permission: &CONFIRMATION_CREATE,
            group: groups::LEGACY_IMPORT,
            description: "创建导入确认事实",
        },
    ),
    (
        "CONFIRMATION_COMPLETE",
        PermissionDef {
            permission: &CONFIRMATION_COMPLETE,
            group: groups::LEGACY_IMPORT,
            description: "完成导入业务确认任务",
        },
    ),
];
