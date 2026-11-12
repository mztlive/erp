//! contract 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 上传识别合同。
pub const IMPORTS_UPLOAD: Permission = CREATE;

/// 查询本人合同导入。
pub const IMPORTS_LIST: Permission = CREATE;

/// 查询本人合同导入结果。
pub const IMPORTS_DETAIL: Permission = CREATE;

/// 执行或重试合同识别。
pub const IMPORTS_RUN: Permission = CREATE;

/// 确认合同识别信息并归档。
pub const IMPORTS_CONFIRM: Permission = CREATE;

/// 预览本人导入原文。
pub const IMPORTS_PREVIEW: Permission = CREATE;

/// 管理合同共享销售。
pub const SALES_ACCESS: Permission = TERMINATE;

/// 查询合同列表。
pub const LIST: Permission = Permission::new(resources::CONTRACT, "list");

/// 首次归档合同 PDF。
pub const CREATE: Permission = Permission::new(resources::CONTRACT, "create");

/// 查询合同详情。
pub const DETAIL: Permission = Permission::new(resources::CONTRACT, "detail");

/// 查询合同附件。
pub const FILE: Permission = DETAIL;

/// 预览合同附件。
pub const FILE_PREVIEW: Permission = DETAIL;

/// 归档合同新版本。
pub const ARCHIVE_REVISION: Permission = TERMINATE;

/// 终止合同。
pub const TERMINATE: Permission = Permission::new(resources::CONTRACT, "update");

/// 查询合同模板。
pub const TEMPLATES_LIST: Permission = Permission::new(resources::CONTRACT_TEMPLATE, "list");

/// 维护合同模板与编号规则。
pub const TEMPLATES_CREATE: Permission = Permission::new(resources::CONTRACT_TEMPLATE, "manage");

/// 启停合同模板。
pub const TEMPLATES_STATUS: Permission = TEMPLATES_CREATE;

/// 读取年度合同流水。
pub const TEMPLATES_COUNTERS: Permission = TEMPLATES_CREATE;

/// 校准年度合同流水。
pub const TEMPLATES_CONFIGURE: Permission = TEMPLATES_CREATE;

/// 查询本人合同申请。
pub const TEMPLATES_APPLICATIONS: Permission = Permission::new(resources::CONTRACT_APPLICATION, "list");

/// 申请销售合同编号。
pub const TEMPLATES_APPLY: Permission = Permission::new(resources::CONTRACT_APPLICATION, "create");

/// 下载本人带编号 Word 合同。
pub const TEMPLATES_DOWNLOAD: Permission = Permission::new(resources::CONTRACT_APPLICATION, "download");

/// 下载编号样张。
pub const TEMPLATES_SAMPLE: Permission = TEMPLATES_CREATE;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("LIST", PermissionDef { permission: &LIST, group: groups::CONTRACT, description: "查询合同列表" }),
    (
        "CREATE",
        PermissionDef { permission: &CREATE, group: groups::CONTRACT, description: "首次归档合同 PDF" },
    ),
    ("DETAIL", PermissionDef { permission: &DETAIL, group: groups::CONTRACT, description: "查询合同详情" }),
    ("TERMINATE", PermissionDef { permission: &TERMINATE, group: groups::CONTRACT, description: "终止合同" }),
    (
        "TEMPLATES_LIST",
        PermissionDef {
            permission: &TEMPLATES_LIST,
            group: groups::CONTRACT_TEMPLATE,
            description: "查询合同模板",
        },
    ),
    (
        "TEMPLATES_CREATE",
        PermissionDef {
            permission: &TEMPLATES_CREATE,
            group: groups::CONTRACT_TEMPLATE,
            description: "维护合同模板与编号规则",
        },
    ),
    (
        "TEMPLATES_APPLICATIONS",
        PermissionDef {
            permission: &TEMPLATES_APPLICATIONS,
            group: groups::CONTRACT_APPLICATION,
            description: "查询本人合同申请",
        },
    ),
    (
        "TEMPLATES_APPLY",
        PermissionDef {
            permission: &TEMPLATES_APPLY,
            group: groups::CONTRACT_APPLICATION,
            description: "申请销售合同编号",
        },
    ),
    (
        "TEMPLATES_DOWNLOAD",
        PermissionDef {
            permission: &TEMPLATES_DOWNLOAD,
            group: groups::CONTRACT_APPLICATION,
            description: "下载本人带编号 Word 合同",
        },
    ),
];
