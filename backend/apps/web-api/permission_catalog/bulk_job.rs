//! bulk_job 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询选择快照列表。
pub const BULK_SELECTION_SNAPSHOT_LIST: PermissionDef = PermissionDef {
    code: "bulk_selection_snapshot:list",
    group: groups::BULK_JOB,
    description: "查询选择快照列表",
};

/// 创建选择快照。
pub const BULK_SELECTION_SNAPSHOT_CREATE: PermissionDef = PermissionDef {
    code: "bulk_selection_snapshot:create",
    group: groups::BULK_JOB,
    description: "创建选择快照",
};

/// 确认选择快照。
pub const BULK_SELECTION_SNAPSHOT_CONFIRM: PermissionDef = PermissionDef {
    code: "bulk_selection_snapshot:confirm",
    group: groups::BULK_JOB,
    description: "确认选择快照",
};

/// 失效选择快照。
pub const BULK_SELECTION_SNAPSHOT_EXPIRE: PermissionDef = PermissionDef {
    code: "bulk_selection_snapshot:expire",
    group: groups::BULK_JOB,
    description: "失效选择快照",
};

/// 查询选择快照逐项结果。
pub const BULK_SELECTION_ITEM_LIST: PermissionDef = PermissionDef {
    code: "bulk_selection_item:list",
    group: groups::BULK_JOB,
    description: "查询选择快照逐项结果",
};

/// 查询后台任务列表。
pub const BACKGROUND_JOB_LIST: PermissionDef =
    PermissionDef {
        code: "background_job:list", group: groups::BULK_JOB, description: "查询后台任务列表"
    };

/// 查询后台任务详情。
pub const BACKGROUND_JOB_DETAIL: PermissionDef =
    PermissionDef {
        code: "background_job:detail", group: groups::BULK_JOB, description: "查询后台任务详情"
    };

/// 创建后台任务。
pub const BACKGROUND_JOB_CREATE: PermissionDef =
    PermissionDef {
        code: "background_job:create", group: groups::BULK_JOB, description: "创建后台任务"
    };

/// 取消后台任务。
pub const BACKGROUND_JOB_CANCEL: PermissionDef =
    PermissionDef {
        code: "background_job:cancel", group: groups::BULK_JOB, description: "取消后台任务"
    };

/// 停止并取消全部后台任务。
pub const BACKGROUND_JOB_CANCEL_ALL: PermissionDef = PermissionDef {
    code: "background_job:cancel_all",
    group: groups::BULK_JOB,
    description: "停止并取消全部后台任务",
};

/// 查询后台任务逐项结果。
pub const BACKGROUND_JOB_ITEM_LIST: PermissionDef = PermissionDef {
    code: "background_job_item:list",
    group: groups::BULK_JOB,
    description: "查询后台任务逐项结果",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("BULK_SELECTION_SNAPSHOT_LIST", BULK_SELECTION_SNAPSHOT_LIST),
    ("BULK_SELECTION_SNAPSHOT_CREATE", BULK_SELECTION_SNAPSHOT_CREATE),
    ("BULK_SELECTION_SNAPSHOT_CONFIRM", BULK_SELECTION_SNAPSHOT_CONFIRM),
    ("BULK_SELECTION_SNAPSHOT_EXPIRE", BULK_SELECTION_SNAPSHOT_EXPIRE),
    ("BULK_SELECTION_ITEM_LIST", BULK_SELECTION_ITEM_LIST),
    ("BACKGROUND_JOB_LIST", BACKGROUND_JOB_LIST),
    ("BACKGROUND_JOB_DETAIL", BACKGROUND_JOB_DETAIL),
    ("BACKGROUND_JOB_CREATE", BACKGROUND_JOB_CREATE),
    ("BACKGROUND_JOB_CANCEL", BACKGROUND_JOB_CANCEL),
    ("BACKGROUND_JOB_CANCEL_ALL", BACKGROUND_JOB_CANCEL_ALL),
    ("BACKGROUND_JOB_ITEM_LIST", BACKGROUND_JOB_ITEM_LIST),
];
