//! access_control 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 检查人员访问权限。
pub const INSPECTION_INSPECT: Permission = PERSON_SCOPE_LIST;

/// 查询审计事件列表。
pub const AUDIT_EVENT_LIST: Permission = Permission::new(resources::AUDIT_EVENT, "list");

/// 查询销售人员。
pub const PERSON_DIRECTORY_LIST_SALESPEOPLE: Permission = Permission::new(resources::SALES_PERSON, "list");

/// 回显已选销售人员。
pub const PERSON_DIRECTORY_LIST_SELECTED_SALESPEOPLE: Permission = PERSON_DIRECTORY_LIST_SALESPEOPLE;

/// 查询采购负责人。
pub const PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE: Permission =
    Permission::new(resources::PROCUREMENT_PERSON, "list");

/// 回显已选采购负责人。
pub const PERSON_DIRECTORY_LIST_SELECTED_PROCUREMENT_PEOPLE: Permission =
    PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE;

/// 查询后台人员。
pub const PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE: Permission =
    Permission::new(resources::BUSINESS_PERSON, "list");

/// 回显已选后台人员。
pub const PERSON_DIRECTORY_LIST_SELECTED_BUSINESS_PEOPLE: Permission = PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE;

/// 读取人员查询资格。
pub const PERSON_DIRECTORY_GET_QUALIFICATION: Permission =
    Permission::new(resources::PERSON_QUERY_QUALIFICATION, "manage");

/// 维护人员查询资格。
pub const PERSON_DIRECTORY_CHANGE_QUALIFICATION: Permission = PERSON_DIRECTORY_GET_QUALIFICATION;

/// 查看数据范围。
pub const PERSON_SCOPE_LIST: Permission = Permission::new(resources::DATA_SCOPE, "list");

/// 创建数据范围。
pub const PERSON_SCOPE_SAVE: Permission = Permission::new(resources::DATA_SCOPE, "create");

/// 校验授权文件。
pub const POLICY_BUNDLE_VALIDATE: Permission = POLICY_BUNDLE_PREVIEW;

/// 预览授权文件。
pub const POLICY_BUNDLE_PREVIEW: Permission = Permission::new(resources::AUTHORIZATION_POLICY, "preview");

/// 应用授权文件。
pub const POLICY_BUNDLE_APPLY: Permission = Permission::new(resources::AUTHORIZATION_POLICY, "apply");

/// 导出授权文件。
pub const POLICY_BUNDLE_EXPORT: Permission = Permission::new(resources::AUTHORIZATION_POLICY, "export");

/// 本模块可供路由引用的显式权限定义。
/// 指定记录授权维护沿用数据范围维护资格；服务另验同角色全公司组织管理。
pub const RESOURCE_GRANT_MANAGE: Permission = PERSON_SCOPE_SAVE;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "AUDIT_EVENT_LIST",
        PermissionDef {
            permission: &AUDIT_EVENT_LIST,
            group: groups::ACCESS_CONTROL,
            description: "查询审计事件列表",
        },
    ),
    (
        "PERSON_DIRECTORY_LIST_SALESPEOPLE",
        PermissionDef {
            permission: &PERSON_DIRECTORY_LIST_SALESPEOPLE,
            group: groups::SALES_PERSON,
            description: "查询销售人员",
        },
    ),
    (
        "PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE",
        PermissionDef {
            permission: &PERSON_DIRECTORY_LIST_PROCUREMENT_PEOPLE,
            group: groups::SALES_PERSON,
            description: "查询采购负责人",
        },
    ),
    (
        "PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE",
        PermissionDef {
            permission: &PERSON_DIRECTORY_LIST_BUSINESS_PEOPLE,
            group: groups::SALES_PERSON,
            description: "查询后台人员",
        },
    ),
    (
        "PERSON_DIRECTORY_GET_QUALIFICATION",
        PermissionDef {
            permission: &PERSON_DIRECTORY_GET_QUALIFICATION,
            group: groups::SALES_PERSON,
            description: "读取人员查询资格",
        },
    ),
    (
        "PERSON_SCOPE_LIST",
        PermissionDef {
            permission: &PERSON_SCOPE_LIST,
            group: groups::ACCESS_CONTROL,
            description: "查看数据范围",
        },
    ),
    (
        "PERSON_SCOPE_SAVE",
        PermissionDef {
            permission: &PERSON_SCOPE_SAVE,
            group: groups::ACCESS_CONTROL,
            description: "创建数据范围",
        },
    ),
    (
        "POLICY_BUNDLE_PREVIEW",
        PermissionDef {
            permission: &POLICY_BUNDLE_PREVIEW,
            group: groups::AUTHORIZATION_POLICY,
            description: "预览授权文件",
        },
    ),
    (
        "POLICY_BUNDLE_APPLY",
        PermissionDef {
            permission: &POLICY_BUNDLE_APPLY,
            group: groups::AUTHORIZATION_POLICY,
            description: "应用授权文件",
        },
    ),
    (
        "POLICY_BUNDLE_EXPORT",
        PermissionDef {
            permission: &POLICY_BUNDLE_EXPORT,
            group: groups::AUTHORIZATION_POLICY,
            description: "导出授权文件",
        },
    ),
];
