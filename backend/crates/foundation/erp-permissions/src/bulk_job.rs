//! bulk_job 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询选择快照列表。
pub const BULK_SELECTION_SNAPSHOT_LIST: Permission =
    Permission::new(resources::BULK_SELECTION_SNAPSHOT, "list");

/// 创建选择快照。
pub const BULK_SELECTION_SNAPSHOT_CREATE: Permission =
    Permission::new(resources::BULK_SELECTION_SNAPSHOT, "create");

/// 确认选择快照。
pub const BULK_SELECTION_SNAPSHOT_CONFIRM: Permission =
    Permission::new(resources::BULK_SELECTION_SNAPSHOT, "confirm");

/// 失效选择快照。
pub const BULK_SELECTION_SNAPSHOT_EXPIRE: Permission =
    Permission::new(resources::BULK_SELECTION_SNAPSHOT, "expire");

/// 查询选择快照逐项结果。
pub const BULK_SELECTION_ITEM_LIST: Permission = Permission::new(resources::BULK_SELECTION_ITEM, "list");

/// 查询后台任务列表。
pub const BACKGROUND_JOB_LIST: Permission = Permission::new(resources::BACKGROUND_JOB, "list");

/// 查询后台任务详情。
pub const BACKGROUND_JOB_DETAIL: Permission = Permission::new(resources::BACKGROUND_JOB, "detail");

/// 创建后台任务。
pub const BACKGROUND_JOB_CREATE: Permission = Permission::new(resources::BACKGROUND_JOB, "create");

/// 取消后台任务。
pub const BACKGROUND_JOB_CANCEL: Permission = Permission::new(resources::BACKGROUND_JOB, "cancel");

/// 停止并取消全部后台任务。
pub const BACKGROUND_JOB_CANCEL_ALL: Permission = Permission::new(resources::BACKGROUND_JOB, "cancel_all");

/// 查询后台任务逐项结果。
pub const BACKGROUND_JOB_ITEM_LIST: Permission = Permission::new(resources::BACKGROUND_JOB_ITEM, "list");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "BULK_SELECTION_SNAPSHOT_LIST",
        PermissionDef {
            permission: &BULK_SELECTION_SNAPSHOT_LIST,
            group: groups::BULK_JOB,
            description: "查询选择快照列表",
        },
    ),
    (
        "BULK_SELECTION_SNAPSHOT_CREATE",
        PermissionDef {
            permission: &BULK_SELECTION_SNAPSHOT_CREATE,
            group: groups::BULK_JOB,
            description: "创建选择快照",
        },
    ),
    (
        "BULK_SELECTION_SNAPSHOT_CONFIRM",
        PermissionDef {
            permission: &BULK_SELECTION_SNAPSHOT_CONFIRM,
            group: groups::BULK_JOB,
            description: "确认选择快照",
        },
    ),
    (
        "BULK_SELECTION_SNAPSHOT_EXPIRE",
        PermissionDef {
            permission: &BULK_SELECTION_SNAPSHOT_EXPIRE,
            group: groups::BULK_JOB,
            description: "失效选择快照",
        },
    ),
    (
        "BULK_SELECTION_ITEM_LIST",
        PermissionDef {
            permission: &BULK_SELECTION_ITEM_LIST,
            group: groups::BULK_JOB,
            description: "查询选择快照逐项结果",
        },
    ),
    (
        "BACKGROUND_JOB_LIST",
        PermissionDef {
            permission: &BACKGROUND_JOB_LIST,
            group: groups::BULK_JOB,
            description: "查询后台任务列表",
        },
    ),
    (
        "BACKGROUND_JOB_DETAIL",
        PermissionDef {
            permission: &BACKGROUND_JOB_DETAIL,
            group: groups::BULK_JOB,
            description: "查询后台任务详情",
        },
    ),
    (
        "BACKGROUND_JOB_CREATE",
        PermissionDef {
            permission: &BACKGROUND_JOB_CREATE,
            group: groups::BULK_JOB,
            description: "创建后台任务",
        },
    ),
    (
        "BACKGROUND_JOB_CANCEL",
        PermissionDef {
            permission: &BACKGROUND_JOB_CANCEL,
            group: groups::BULK_JOB,
            description: "取消后台任务",
        },
    ),
    (
        "BACKGROUND_JOB_CANCEL_ALL",
        PermissionDef {
            permission: &BACKGROUND_JOB_CANCEL_ALL,
            group: groups::BULK_JOB,
            description: "停止并取消全部后台任务",
        },
    ),
    (
        "BACKGROUND_JOB_ITEM_LIST",
        PermissionDef {
            permission: &BACKGROUND_JOB_ITEM_LIST,
            group: groups::BULK_JOB,
            description: "查询后台任务逐项结果",
        },
    ),
];
