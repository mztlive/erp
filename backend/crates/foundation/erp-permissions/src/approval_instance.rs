//! approval_instance 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查看实例抄送记录。
pub const COPIES_HISTORY: Permission = INSTANCE_LIST;

/// 读取开票审批当前核对依据。
pub const MATERIALS_INVOICE_REVIEW: Permission = INSTANCE_LIST;

/// 读取本次提交的审批资料。
pub const MATERIALS_LIST: Permission = INSTANCE_LIST;

/// 预览本次提交的审批附件。
pub const MATERIALS_PREVIEW: Permission = INSTANCE_LIST;

/// 下载本次提交的审批附件。
pub const MATERIALS_DOWNLOAD: Permission = INSTANCE_LIST;

/// 提交当前开放审批任务的通过或驳回。
pub const SUBMIT_DECISION: Permission = Permission::new(resources::APPROVAL_INSTANCE, "decide");

/// 按固定 view 查询审批实例。
pub const INSTANCE_LIST: Permission = Permission::new(resources::APPROVAL_INSTANCE, "read");

/// 查询本人发起审批的未读进展数量。
pub const STARTED_UNREAD_COUNT: Permission = INSTANCE_LIST;

/// 确认本人发起审批的已查看进展。
pub const PROGRESS_READ: Permission = INSTANCE_LIST;

/// 查询审批实例详情。
pub const INSTANCE_DETAIL: Permission = INSTANCE_LIST;

/// 按轮次读取审批历史。
pub const INSTANCE_HISTORY: Permission = INSTANCE_LIST;

/// 查询当前 blocker 的唯一合法恢复方式。
pub const RECOVERY_OPTIONS: Permission = INSTANCE_LIST;

/// 原审批人重新合格后恢复当前节点。
pub const RESUME_CURRENT_APPROVER: Permission = Permission::new(resources::APPROVAL_INSTANCE, "resume");

/// 取消非人员一致性 blocker。
pub const CANCEL_BLOCKED: Permission = Permission::new(resources::APPROVAL_INSTANCE, "cancel_blocked");

/// 升级未提交单据的审批定义绑定。
pub const UPGRADE_BINDING: Permission = Permission::new(resources::APPROVAL_INSTANCE, "upgrade_binding");

/// 查询审批实例详情。
pub const TIMELINE_APPROVAL_TIMELINE: Permission = INSTANCE_LIST;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "SUBMIT_DECISION",
        PermissionDef {
            permission: &SUBMIT_DECISION,
            group: groups::APPROVAL_INSTANCE,
            description: "提交当前开放审批任务的通过或驳回",
        },
    ),
    (
        "INSTANCE_LIST",
        PermissionDef {
            permission: &INSTANCE_LIST,
            group: groups::APPROVAL_INSTANCE,
            description: "按固定 view 查询审批实例",
        },
    ),
    (
        "RESUME_CURRENT_APPROVER",
        PermissionDef {
            permission: &RESUME_CURRENT_APPROVER,
            group: groups::APPROVAL_INSTANCE,
            description: "原审批人重新合格后恢复当前节点",
        },
    ),
    (
        "CANCEL_BLOCKED",
        PermissionDef {
            permission: &CANCEL_BLOCKED,
            group: groups::APPROVAL_INSTANCE,
            description: "取消非人员一致性 blocker",
        },
    ),
    (
        "UPGRADE_BINDING",
        PermissionDef {
            permission: &UPGRADE_BINDING,
            group: groups::APPROVAL_INSTANCE,
            description: "升级未提交单据的审批定义绑定",
        },
    ),
];
