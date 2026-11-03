//! contract 的接口权限目录。
use super::{PermissionDef, groups};

/// 上传识别合同。
pub const IMPORTS_UPLOAD: PermissionDef = CREATE.described(groups::CONTRACT, "上传识别合同");

/// 查询本人合同导入。
pub const IMPORTS_LIST: PermissionDef = CREATE.described(groups::CONTRACT, "查询本人合同导入");

/// 查询本人合同导入结果。
pub const IMPORTS_DETAIL: PermissionDef = CREATE.described(groups::CONTRACT, "查询本人合同导入结果");

/// 执行或重试合同识别。
pub const IMPORTS_RUN: PermissionDef = CREATE.described(groups::CONTRACT, "执行或重试合同识别");

/// 确认合同识别信息并归档。
pub const IMPORTS_CONFIRM: PermissionDef = CREATE.described(groups::CONTRACT, "确认合同识别信息并归档");

/// 预览本人导入原文。
pub const IMPORTS_PREVIEW: PermissionDef = CREATE.described(groups::CONTRACT, "预览本人导入原文");

/// 管理合同共享销售。
pub const SALES_ACCESS: PermissionDef = TERMINATE.described(groups::CONTRACT, "管理合同共享销售");

/// 查询合同列表。
pub const LIST: PermissionDef =
    PermissionDef { code: "contract:list", group: groups::CONTRACT, description: "查询合同列表" };

/// 首次归档合同 PDF。
pub const CREATE: PermissionDef =
    PermissionDef { code: "contract:create", group: groups::CONTRACT, description: "首次归档合同 PDF" };

/// 查询合同详情。
pub const DETAIL: PermissionDef =
    PermissionDef { code: "contract:detail", group: groups::CONTRACT, description: "查询合同详情" };

/// 查询合同附件。
pub const FILE: PermissionDef = DETAIL.described(groups::CONTRACT, "查询合同附件");

/// 预览合同附件。
pub const FILE_PREVIEW: PermissionDef = DETAIL.described(groups::CONTRACT, "预览合同附件");

/// 归档合同新版本。
pub const ARCHIVE_REVISION: PermissionDef = TERMINATE.described(groups::CONTRACT, "归档合同新版本");

/// 终止合同。
pub const TERMINATE: PermissionDef =
    PermissionDef { code: "contract:update", group: groups::CONTRACT, description: "终止合同" };

/// 查询合同模板。
pub const TEMPLATES_LIST: PermissionDef = PermissionDef {
    code: "contract_template:list",
    group: groups::CONTRACT_TEMPLATE,
    description: "查询合同模板",
};

/// 维护合同模板与编号规则。
pub const TEMPLATES_CREATE: PermissionDef = PermissionDef {
    code: "contract_template:manage",
    group: groups::CONTRACT_TEMPLATE,
    description: "维护合同模板与编号规则",
};

/// 启停合同模板。
pub const TEMPLATES_STATUS: PermissionDef =
    TEMPLATES_CREATE.described(groups::CONTRACT_TEMPLATE, "启停合同模板");

/// 读取年度合同流水。
pub const TEMPLATES_COUNTERS: PermissionDef =
    TEMPLATES_CREATE.described(groups::CONTRACT_TEMPLATE, "读取年度合同流水");

/// 校准年度合同流水。
pub const TEMPLATES_CONFIGURE: PermissionDef =
    TEMPLATES_CREATE.described(groups::CONTRACT_TEMPLATE, "校准年度合同流水");

/// 查询本人合同申请。
pub const TEMPLATES_APPLICATIONS: PermissionDef = PermissionDef {
    code: "contract_application:list",
    group: groups::CONTRACT_APPLICATION,
    description: "查询本人合同申请",
};

/// 申请销售合同编号。
pub const TEMPLATES_APPLY: PermissionDef = PermissionDef {
    code: "contract_application:create",
    group: groups::CONTRACT_APPLICATION,
    description: "申请销售合同编号",
};

/// 下载本人带编号 Word 合同。
pub const TEMPLATES_DOWNLOAD: PermissionDef = PermissionDef {
    code: "contract_application:download",
    group: groups::CONTRACT_APPLICATION,
    description: "下载本人带编号 Word 合同",
};

/// 下载编号样张。
pub const TEMPLATES_SAMPLE: PermissionDef =
    TEMPLATES_CREATE.described(groups::CONTRACT_TEMPLATE, "下载编号样张");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("IMPORTS_UPLOAD", IMPORTS_UPLOAD),
    ("IMPORTS_LIST", IMPORTS_LIST),
    ("IMPORTS_DETAIL", IMPORTS_DETAIL),
    ("IMPORTS_RUN", IMPORTS_RUN),
    ("IMPORTS_CONFIRM", IMPORTS_CONFIRM),
    ("IMPORTS_PREVIEW", IMPORTS_PREVIEW),
    ("SALES_ACCESS", SALES_ACCESS),
    ("LIST", LIST),
    ("CREATE", CREATE),
    ("DETAIL", DETAIL),
    ("FILE", FILE),
    ("FILE_PREVIEW", FILE_PREVIEW),
    ("ARCHIVE_REVISION", ARCHIVE_REVISION),
    ("TERMINATE", TERMINATE),
    ("TEMPLATES_LIST", TEMPLATES_LIST),
    ("TEMPLATES_CREATE", TEMPLATES_CREATE),
    ("TEMPLATES_STATUS", TEMPLATES_STATUS),
    ("TEMPLATES_COUNTERS", TEMPLATES_COUNTERS),
    ("TEMPLATES_CONFIGURE", TEMPLATES_CONFIGURE),
    ("TEMPLATES_APPLICATIONS", TEMPLATES_APPLICATIONS),
    ("TEMPLATES_APPLY", TEMPLATES_APPLY),
    ("TEMPLATES_DOWNLOAD", TEMPLATES_DOWNLOAD),
    ("TEMPLATES_SAMPLE", TEMPLATES_SAMPLE),
];
