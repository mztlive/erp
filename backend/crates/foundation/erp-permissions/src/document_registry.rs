//! document_registry 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询单据注册列表。
pub const BUSINESS_DOCUMENT_LIST: Permission = Permission::new(resources::BUSINESS_DOCUMENT, "list");

/// 注册业务单据（幂等）。
pub const BUSINESS_DOCUMENT_REGISTER: Permission = Permission::new(resources::BUSINESS_DOCUMENT, "create");

/// 查询单据注册详情。
pub const BUSINESS_DOCUMENT_DETAIL: Permission = Permission::new(resources::BUSINESS_DOCUMENT, "detail");

/// 查询工作流动作列表。
pub const WORKFLOW_ACTION_LIST: Permission = Permission::new(resources::WORKFLOW_ACTION, "list");

/// 追加工作流动作。
pub const WORKFLOW_ACTION_CREATE: Permission = Permission::new(resources::WORKFLOW_ACTION, "create");

/// 查询单据关系列表。
pub const DOCUMENT_RELATION_LIST: Permission = Permission::new(resources::DOCUMENT_RELATION, "list");

/// 建立单据关系。
pub const DOCUMENT_RELATION_CREATE: Permission = Permission::new(resources::DOCUMENT_RELATION, "create");

/// 查询单据参与人列表。
pub const DOCUMENT_PARTICIPANT_LIST: Permission = Permission::new(resources::DOCUMENT_PARTICIPANT, "list");

/// 登记单据参与人。
pub const DOCUMENT_PARTICIPANT_CREATE: Permission =
    Permission::new(resources::DOCUMENT_PARTICIPANT, "create");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "BUSINESS_DOCUMENT_LIST",
        PermissionDef {
            permission: &BUSINESS_DOCUMENT_LIST,
            group: groups::DOCUMENT_REGISTRY,
            description: "查询单据注册列表",
        },
    ),
    (
        "BUSINESS_DOCUMENT_REGISTER",
        PermissionDef {
            permission: &BUSINESS_DOCUMENT_REGISTER,
            group: groups::DOCUMENT_REGISTRY,
            description: "注册业务单据（幂等）",
        },
    ),
    (
        "BUSINESS_DOCUMENT_DETAIL",
        PermissionDef {
            permission: &BUSINESS_DOCUMENT_DETAIL,
            group: groups::DOCUMENT_REGISTRY,
            description: "查询单据注册详情",
        },
    ),
    (
        "WORKFLOW_ACTION_LIST",
        PermissionDef {
            permission: &WORKFLOW_ACTION_LIST,
            group: groups::DOCUMENT_REGISTRY,
            description: "查询工作流动作列表",
        },
    ),
    (
        "WORKFLOW_ACTION_CREATE",
        PermissionDef {
            permission: &WORKFLOW_ACTION_CREATE,
            group: groups::DOCUMENT_REGISTRY,
            description: "追加工作流动作",
        },
    ),
    (
        "DOCUMENT_RELATION_LIST",
        PermissionDef {
            permission: &DOCUMENT_RELATION_LIST,
            group: groups::DOCUMENT_REGISTRY,
            description: "查询单据关系列表",
        },
    ),
    (
        "DOCUMENT_RELATION_CREATE",
        PermissionDef {
            permission: &DOCUMENT_RELATION_CREATE,
            group: groups::DOCUMENT_REGISTRY,
            description: "建立单据关系",
        },
    ),
    (
        "DOCUMENT_PARTICIPANT_LIST",
        PermissionDef {
            permission: &DOCUMENT_PARTICIPANT_LIST,
            group: groups::DOCUMENT_REGISTRY,
            description: "查询单据参与人列表",
        },
    ),
    (
        "DOCUMENT_PARTICIPANT_CREATE",
        PermissionDef {
            permission: &DOCUMENT_PARTICIPANT_CREATE,
            group: groups::DOCUMENT_REGISTRY,
            description: "登记单据参与人",
        },
    ),
];
