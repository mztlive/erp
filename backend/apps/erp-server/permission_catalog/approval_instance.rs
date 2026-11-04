//! approval_instance 的接口权限目录。
use super::{PermissionDef, groups};

/// 查看实例抄送记录。
pub const COPIES_HISTORY: PermissionDef =
    INSTANCE_LIST.described(groups::APPROVAL_INSTANCE, "查看实例抄送记录");

/// 读取开票审批当前核对依据。
pub const MATERIALS_INVOICE_REVIEW: PermissionDef =
    INSTANCE_LIST.described(groups::APPROVAL_INSTANCE, "读取开票审批当前核对依据");

/// 读取本次提交的审批资料。
pub const MATERIALS_LIST: PermissionDef =
    INSTANCE_LIST.described(groups::APPROVAL_INSTANCE, "读取本次提交的审批资料");

/// 预览本次提交的审批附件。
pub const MATERIALS_PREVIEW: PermissionDef =
    INSTANCE_LIST.described(groups::APPROVAL_INSTANCE, "预览本次提交的审批附件");

/// 下载本次提交的审批附件。
pub const MATERIALS_DOWNLOAD: PermissionDef =
    INSTANCE_LIST.described(groups::APPROVAL_INSTANCE, "下载本次提交的审批附件");

/// 提交当前开放审批任务的通过或驳回。
pub const SUBMIT_DECISION: PermissionDef = PermissionDef {
    code: "approval_instance:decide",
    group: groups::APPROVAL_INSTANCE,
    description: "提交当前开放审批任务的通过或驳回",
};

/// 按固定 view 查询审批实例。
pub const INSTANCE_LIST: PermissionDef = PermissionDef {
    code: "approval_instance:read",
    group: groups::APPROVAL_INSTANCE,
    description: "按固定 view 查询审批实例",
};

/// 查询本人发起审批的未读进展数量。
pub const STARTED_UNREAD_COUNT: PermissionDef =
    INSTANCE_LIST.described(groups::APPROVAL_INSTANCE, "查询本人发起审批的未读进展数量");

/// 确认本人发起审批的已查看进展。
pub const PROGRESS_READ: PermissionDef =
    INSTANCE_LIST.described(groups::APPROVAL_INSTANCE, "确认本人发起审批的已查看进展");

/// 查询审批实例详情。
pub const INSTANCE_DETAIL: PermissionDef =
    INSTANCE_LIST.described(groups::APPROVAL_INSTANCE, "查询审批实例详情");

/// 按轮次读取审批历史。
pub const INSTANCE_HISTORY: PermissionDef =
    INSTANCE_LIST.described(groups::APPROVAL_INSTANCE, "按轮次读取审批历史");

/// 查询当前 blocker 的唯一合法恢复方式。
pub const RECOVERY_OPTIONS: PermissionDef =
    INSTANCE_LIST.described(groups::APPROVAL_INSTANCE, "查询当前 blocker 的唯一合法恢复方式");

/// 原审批人重新合格后恢复当前节点。
pub const RESUME_CURRENT_APPROVER: PermissionDef = PermissionDef {
    code: "approval_instance:resume",
    group: groups::APPROVAL_INSTANCE,
    description: "原审批人重新合格后恢复当前节点",
};

/// 取消非人员一致性 blocker。
pub const CANCEL_BLOCKED: PermissionDef = PermissionDef {
    code: "approval_instance:cancel_blocked",
    group: groups::APPROVAL_INSTANCE,
    description: "取消非人员一致性 blocker",
};

/// 升级未提交单据的审批定义绑定。
pub const UPGRADE_BINDING: PermissionDef = PermissionDef {
    code: "approval_instance:upgrade_binding",
    group: groups::APPROVAL_INSTANCE,
    description: "升级未提交单据的审批定义绑定",
};

/// 查询审批实例详情。
pub const TIMELINE_APPROVAL_TIMELINE: PermissionDef =
    INSTANCE_LIST.described(groups::APPROVAL_INSTANCE, "查询审批实例详情");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("COPIES_HISTORY", COPIES_HISTORY),
    ("MATERIALS_INVOICE_REVIEW", MATERIALS_INVOICE_REVIEW),
    ("MATERIALS_LIST", MATERIALS_LIST),
    ("MATERIALS_PREVIEW", MATERIALS_PREVIEW),
    ("MATERIALS_DOWNLOAD", MATERIALS_DOWNLOAD),
    ("SUBMIT_DECISION", SUBMIT_DECISION),
    ("INSTANCE_LIST", INSTANCE_LIST),
    ("STARTED_UNREAD_COUNT", STARTED_UNREAD_COUNT),
    ("PROGRESS_READ", PROGRESS_READ),
    ("INSTANCE_DETAIL", INSTANCE_DETAIL),
    ("INSTANCE_HISTORY", INSTANCE_HISTORY),
    ("RECOVERY_OPTIONS", RECOVERY_OPTIONS),
    ("RESUME_CURRENT_APPROVER", RESUME_CURRENT_APPROVER),
    ("CANCEL_BLOCKED", CANCEL_BLOCKED),
    ("UPGRADE_BINDING", UPGRADE_BINDING),
    ("TIMELINE_APPROVAL_TIMELINE", TIMELINE_APPROVAL_TIMELINE),
];
