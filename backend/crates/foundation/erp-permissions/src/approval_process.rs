//! approval_process 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 读取固定单据类型审批目录。
pub const DEFINITION_CATALOG: Permission = DEFINITION_DETAIL;

/// 读取某单据类型的定义版本。
pub const DEFINITION_VERSIONS: Permission = DEFINITION_DETAIL;

/// 读取审批流程定义详情。
pub const DEFINITION_DETAIL: Permission = Permission::new(resources::APPROVAL_PROCESS, "read");

/// 创建更高版本审批流程草稿。
pub const CREATE_DEFINITION_DRAFT: Permission = Permission::new(resources::APPROVAL_PROCESS, "create");

/// 整组替换草稿节点。
pub const REPLACE_DEFINITION_NODES: Permission = Permission::new(resources::APPROVAL_PROCESS, "edit");

/// 发布审批流程草稿。
pub const PUBLISH_DEFINITION: Permission = Permission::new(resources::APPROVAL_PROCESS, "publish");

/// 退役当前已发布审批流程。
pub const RETIRE_DEFINITION: Permission = Permission::new(resources::APPROVAL_PROCESS, "retire");

/// 按定义期规则搜索可选审批人。
pub const ELIGIBLE_ASSIGNEES: Permission = DEFINITION_DETAIL;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "DEFINITION_DETAIL",
        PermissionDef {
            permission: &DEFINITION_DETAIL,
            group: groups::APPROVAL_PROCESS,
            description: "读取审批流程定义详情",
        },
    ),
    (
        "CREATE_DEFINITION_DRAFT",
        PermissionDef {
            permission: &CREATE_DEFINITION_DRAFT,
            group: groups::APPROVAL_PROCESS,
            description: "创建更高版本审批流程草稿",
        },
    ),
    (
        "REPLACE_DEFINITION_NODES",
        PermissionDef {
            permission: &REPLACE_DEFINITION_NODES,
            group: groups::APPROVAL_PROCESS,
            description: "整组替换草稿节点",
        },
    ),
    (
        "PUBLISH_DEFINITION",
        PermissionDef {
            permission: &PUBLISH_DEFINITION,
            group: groups::APPROVAL_PROCESS,
            description: "发布审批流程草稿",
        },
    ),
    (
        "RETIRE_DEFINITION",
        PermissionDef {
            permission: &RETIRE_DEFINITION,
            group: groups::APPROVAL_PROCESS,
            description: "退役当前已发布审批流程",
        },
    ),
];
