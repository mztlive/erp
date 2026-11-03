//! document_registry 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询单据注册列表。
pub const BUSINESS_DOCUMENT_LIST: PermissionDef = PermissionDef {
    code: "business_document:list",
    group: groups::DOCUMENT_REGISTRY,
    description: "查询单据注册列表",
};

/// 注册业务单据（幂等）。
pub const BUSINESS_DOCUMENT_REGISTER: PermissionDef = PermissionDef {
    code: "business_document:create",
    group: groups::DOCUMENT_REGISTRY,
    description: "注册业务单据（幂等）",
};

/// 查询单据注册详情。
pub const BUSINESS_DOCUMENT_DETAIL: PermissionDef = PermissionDef {
    code: "business_document:detail",
    group: groups::DOCUMENT_REGISTRY,
    description: "查询单据注册详情",
};

/// 查询工作流动作列表。
pub const WORKFLOW_ACTION_LIST: PermissionDef = PermissionDef {
    code: "workflow_action:list",
    group: groups::DOCUMENT_REGISTRY,
    description: "查询工作流动作列表",
};

/// 追加工作流动作。
pub const WORKFLOW_ACTION_CREATE: PermissionDef = PermissionDef {
    code: "workflow_action:create",
    group: groups::DOCUMENT_REGISTRY,
    description: "追加工作流动作",
};

/// 查询单据关系列表。
pub const DOCUMENT_RELATION_LIST: PermissionDef = PermissionDef {
    code: "document_relation:list",
    group: groups::DOCUMENT_REGISTRY,
    description: "查询单据关系列表",
};

/// 建立单据关系。
pub const DOCUMENT_RELATION_CREATE: PermissionDef = PermissionDef {
    code: "document_relation:create",
    group: groups::DOCUMENT_REGISTRY,
    description: "建立单据关系",
};

/// 查询单据参与人列表。
pub const DOCUMENT_PARTICIPANT_LIST: PermissionDef = PermissionDef {
    code: "document_participant:list",
    group: groups::DOCUMENT_REGISTRY,
    description: "查询单据参与人列表",
};

/// 登记单据参与人。
pub const DOCUMENT_PARTICIPANT_CREATE: PermissionDef = PermissionDef {
    code: "document_participant:create",
    group: groups::DOCUMENT_REGISTRY,
    description: "登记单据参与人",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("BUSINESS_DOCUMENT_LIST", BUSINESS_DOCUMENT_LIST),
    ("BUSINESS_DOCUMENT_REGISTER", BUSINESS_DOCUMENT_REGISTER),
    ("BUSINESS_DOCUMENT_DETAIL", BUSINESS_DOCUMENT_DETAIL),
    ("WORKFLOW_ACTION_LIST", WORKFLOW_ACTION_LIST),
    ("WORKFLOW_ACTION_CREATE", WORKFLOW_ACTION_CREATE),
    ("DOCUMENT_RELATION_LIST", DOCUMENT_RELATION_LIST),
    ("DOCUMENT_RELATION_CREATE", DOCUMENT_RELATION_CREATE),
    ("DOCUMENT_PARTICIPANT_LIST", DOCUMENT_PARTICIPANT_LIST),
    ("DOCUMENT_PARTICIPANT_CREATE", DOCUMENT_PARTICIPANT_CREATE),
];
