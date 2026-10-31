//! approval_process 的接口权限目录。
use super::{PermissionDef, groups};

/// 读取固定单据类型审批目录。
pub const DEFINITION_CATALOG: PermissionDef =
    DEFINITION_DETAIL.described(groups::APPROVAL_PROCESS, "读取固定单据类型审批目录");

/// 读取某单据类型的定义版本。
pub const DEFINITION_VERSIONS: PermissionDef =
    DEFINITION_DETAIL.described(groups::APPROVAL_PROCESS, "读取某单据类型的定义版本");

/// 读取审批流程定义详情。
pub const DEFINITION_DETAIL: PermissionDef = PermissionDef {
    code: "approval_process:read",
    group: groups::APPROVAL_PROCESS,
    description: "读取审批流程定义详情",
};

/// 创建更高版本审批流程草稿。
pub const CREATE_DEFINITION_DRAFT: PermissionDef = PermissionDef {
    code: "approval_process:create",
    group: groups::APPROVAL_PROCESS,
    description: "创建更高版本审批流程草稿",
};

/// 整组替换草稿节点。
pub const REPLACE_DEFINITION_NODES: PermissionDef = PermissionDef {
    code: "approval_process:edit",
    group: groups::APPROVAL_PROCESS,
    description: "整组替换草稿节点",
};

/// 发布审批流程草稿。
pub const PUBLISH_DEFINITION: PermissionDef = PermissionDef {
    code: "approval_process:publish",
    group: groups::APPROVAL_PROCESS,
    description: "发布审批流程草稿",
};

/// 退役当前已发布审批流程。
pub const RETIRE_DEFINITION: PermissionDef = PermissionDef {
    code: "approval_process:retire",
    group: groups::APPROVAL_PROCESS,
    description: "退役当前已发布审批流程",
};

/// 按定义期规则搜索可选审批人。
pub const ELIGIBLE_ASSIGNEES: PermissionDef =
    DEFINITION_DETAIL.described(groups::APPROVAL_PROCESS, "按定义期规则搜索可选审批人");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("DEFINITION_CATALOG", DEFINITION_CATALOG),
    ("DEFINITION_VERSIONS", DEFINITION_VERSIONS),
    ("DEFINITION_DETAIL", DEFINITION_DETAIL),
    ("CREATE_DEFINITION_DRAFT", CREATE_DEFINITION_DRAFT),
    ("REPLACE_DEFINITION_NODES", REPLACE_DEFINITION_NODES),
    ("PUBLISH_DEFINITION", PUBLISH_DEFINITION),
    ("RETIRE_DEFINITION", RETIRE_DEFINITION),
    ("ELIGIBLE_ASSIGNEES", ELIGIBLE_ASSIGNEES),
];
