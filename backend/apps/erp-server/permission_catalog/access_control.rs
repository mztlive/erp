//! access_control 的接口权限目录。
use super::{PermissionDef, groups};

/// 检查人员访问权限。
pub const INSPECTION_INSPECT: PermissionDef =
    PERSON_SCOPE_LIST.described(groups::ACCESS_CONTROL, "检查人员访问权限");

/// 查询权限定义列表。
pub const PERMISSION_LIST: PermissionDef =
    PermissionDef {
        code: "permission:list", group: groups::ACCESS_CONTROL, description: "查询权限定义列表"
    };

/// 创建权限定义。
pub const PERMISSION_CREATE: PermissionDef =
    PermissionDef {
        code: "permission:create", group: groups::ACCESS_CONTROL, description: "创建权限定义"
    };

/// 更新权限定义。
pub const PERMISSION_UPDATE: PermissionDef =
    PermissionDef {
        code: "permission:update", group: groups::ACCESS_CONTROL, description: "更新权限定义"
    };

/// 删除权限定义。
pub const PERMISSION_DELETE: PermissionDef =
    PermissionDef {
        code: "permission:delete", group: groups::ACCESS_CONTROL, description: "删除权限定义"
    };

/// 查询用户角色绑定。
pub const USER_ROLE_LIST: PermissionDef =
    PermissionDef {
        code: "user_role:list", group: groups::ACCESS_CONTROL, description: "查询用户角色绑定"
    };

/// 分配用户角色。
pub const USER_ROLE_CREATE: PermissionDef =
    PermissionDef {
        code: "user_role:create", group: groups::ACCESS_CONTROL, description: "分配用户角色"
    };

/// 撤权用户角色。
pub const USER_ROLE_REVOKE: PermissionDef =
    PermissionDef {
        code: "user_role:revoke", group: groups::ACCESS_CONTROL, description: "撤权用户角色"
    };

/// 查询审计事件列表。
pub const AUDIT_EVENT_LIST: PermissionDef = PermissionDef {
    code: "audit_event:list",
    group: groups::ACCESS_CONTROL,
    description: "查询审计事件列表",
};

/// 查询销售人员。
pub const PERSON_DIRECTORY_LIST_SALESPEOPLE: PermissionDef =
    PermissionDef {
        code: "sales_person:list", group: groups::SALES_PERSON, description: "查询销售人员"
    };

/// 回显已选销售人员。
pub const PERSON_DIRECTORY_LIST_SELECTED_SALESPEOPLE: PermissionDef =
    PERSON_DIRECTORY_LIST_SALESPEOPLE.described(groups::SALES_PERSON, "回显已选销售人员");

/// 查询采购负责人。
pub const PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE: PermissionDef = PermissionDef {
    code: "procurement_person:list",
    group: groups::SALES_PERSON,
    description: "查询采购负责人",
};

/// 回显已选采购负责人。
pub const PERSON_DIRECTORY_LIST_SELECTED_PROCUREMENT_PEOPLE: PermissionDef =
    PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE.described(groups::SALES_PERSON, "回显已选采购负责人");

/// 查询后台人员。
pub const PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE: PermissionDef =
    PermissionDef {
        code: "business_person:list", group: groups::SALES_PERSON, description: "查询后台人员"
    };

/// 回显已选后台人员。
pub const PERSON_DIRECTORY_LIST_SELECTED_BUSINESS_PEOPLE: PermissionDef =
    PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE.described(groups::SALES_PERSON, "回显已选后台人员");

/// 读取人员查询资格。
pub const PERSON_DIRECTORY_GET_QUALIFICATION: PermissionDef = PermissionDef {
    code: "person_query_qualification:manage",
    group: groups::SALES_PERSON,
    description: "读取人员查询资格",
};

/// 维护人员查询资格。
pub const PERSON_DIRECTORY_CHANGE_QUALIFICATION: PermissionDef =
    PERSON_DIRECTORY_GET_QUALIFICATION.described(groups::SALES_PERSON, "维护人员查询资格");

/// 查看数据范围。
pub const PERSON_SCOPE_LIST: PermissionDef =
    PermissionDef {
        code: "data_scope:list", group: groups::ACCESS_CONTROL, description: "查看数据范围"
    };

/// 创建数据范围。
pub const PERSON_SCOPE_SAVE: PermissionDef =
    PermissionDef {
        code: "data_scope:create", group: groups::ACCESS_CONTROL, description: "创建数据范围"
    };

/// 校验授权文件。
pub const POLICY_BUNDLE_VALIDATE: PermissionDef =
    POLICY_BUNDLE_PREVIEW.described(groups::AUTHORIZATION_POLICY, "校验授权文件");

/// 预览授权文件。
pub const POLICY_BUNDLE_PREVIEW: PermissionDef = PermissionDef {
    code: "authorization_policy:preview",
    group: groups::AUTHORIZATION_POLICY,
    description: "预览授权文件",
};

/// 应用授权文件。
pub const POLICY_BUNDLE_APPLY: PermissionDef = PermissionDef {
    code: "authorization_policy:apply",
    group: groups::AUTHORIZATION_POLICY,
    description: "应用授权文件",
};

/// 导出授权文件。
pub const POLICY_BUNDLE_EXPORT: PermissionDef = PermissionDef {
    code: "authorization_policy:export",
    group: groups::AUTHORIZATION_POLICY,
    description: "导出授权文件",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("INSPECTION_INSPECT", INSPECTION_INSPECT),
    ("PERMISSION_LIST", PERMISSION_LIST),
    ("PERMISSION_CREATE", PERMISSION_CREATE),
    ("PERMISSION_UPDATE", PERMISSION_UPDATE),
    ("PERMISSION_DELETE", PERMISSION_DELETE),
    ("USER_ROLE_LIST", USER_ROLE_LIST),
    ("USER_ROLE_CREATE", USER_ROLE_CREATE),
    ("USER_ROLE_REVOKE", USER_ROLE_REVOKE),
    ("AUDIT_EVENT_LIST", AUDIT_EVENT_LIST),
    ("PERSON_DIRECTORY_LIST_SALESPEOPLE", PERSON_DIRECTORY_LIST_SALESPEOPLE),
    ("PERSON_DIRECTORY_LIST_SELECTED_SALESPEOPLE", PERSON_DIRECTORY_LIST_SELECTED_SALESPEOPLE),
    ("PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE", PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE),
    ("PERSON_DIRECTORY_LIST_SELECTED_PROCUREMENT_PEOPLE", PERSON_DIRECTORY_LIST_SELECTED_PROCUREMENT_PEOPLE),
    ("PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE", PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE),
    ("PERSON_DIRECTORY_LIST_SELECTED_BUSINESS_PEOPLE", PERSON_DIRECTORY_LIST_SELECTED_BUSINESS_PEOPLE),
    ("PERSON_DIRECTORY_GET_QUALIFICATION", PERSON_DIRECTORY_GET_QUALIFICATION),
    ("PERSON_DIRECTORY_CHANGE_QUALIFICATION", PERSON_DIRECTORY_CHANGE_QUALIFICATION),
    ("PERSON_SCOPE_LIST", PERSON_SCOPE_LIST),
    ("PERSON_SCOPE_SAVE", PERSON_SCOPE_SAVE),
    ("POLICY_BUNDLE_VALIDATE", POLICY_BUNDLE_VALIDATE),
    ("POLICY_BUNDLE_PREVIEW", POLICY_BUNDLE_PREVIEW),
    ("POLICY_BUNDLE_APPLY", POLICY_BUNDLE_APPLY),
    ("POLICY_BUNDLE_EXPORT", POLICY_BUNDLE_EXPORT),
];
