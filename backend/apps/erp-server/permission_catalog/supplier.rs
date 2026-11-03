//! supplier 的接口权限目录。
use super::{PermissionDef, groups};

/// 导入供应商资料。
pub const IMPORT_SUPPLIER_IMPORT: PermissionDef =
    PermissionDef { code: "supplier:create", group: groups::SUPPLIER, description: "导入供应商资料" };

/// 提交供应商导入任务。
pub const IMPORT_SUPPLIER_IMPORT_SUBMIT: PermissionDef =
    IMPORT_SUPPLIER_IMPORT.described(groups::SUPPLIER, "提交供应商导入任务");

/// 下载供应商导入待处理行。
pub const IMPORT_SUPPLIER_IMPORT_FAILURES: PermissionDef =
    IMPORT_SUPPLIER_IMPORT.described(groups::SUPPLIER, "下载供应商导入待处理行");

/// 创建完整供应商资料。
pub const PROFILE_CREATE: PermissionDef =
    IMPORT_SUPPLIER_IMPORT.described(groups::SUPPLIER, "创建完整供应商资料");

/// 一次创建供应商资料及资质文件。
pub const PROFILE_CREATE_WITH_ASSETS: PermissionDef =
    IMPORT_SUPPLIER_IMPORT.described(groups::SUPPLIER, "一次创建供应商资料及资质文件");

/// 修订完整供应商资料。
pub const PROFILE_UPDATE: PermissionDef = HANDOVER.described(groups::SUPPLIER, "修订完整供应商资料");

/// 一次修订供应商资料及资质文件。
pub const PROFILE_UPDATE_WITH_ASSETS: PermissionDef =
    HANDOVER.described(groups::SUPPLIER, "一次修订供应商资料及资质文件");

/// 查询供应商资料保存结果。
pub const PROFILE_COMMAND_DETAIL: PermissionDef =
    DETAIL.described(groups::SUPPLIER, "查询供应商资料保存结果");

/// 短时查看供应商敏感字段。
pub const SENSITIVE_REVEAL: PermissionDef = PermissionDef {
    code: "supplier_sensitive:reveal",
    group: groups::SUPPLIER,
    description: "短时查看供应商敏感字段",
};

/// 查询供应商列表。
pub const LIST: PermissionDef =
    PermissionDef { code: "supplier:list", group: groups::SUPPLIER, description: "查询供应商列表" };

/// 查询供应商详情。
pub const DETAIL: PermissionDef =
    PermissionDef { code: "supplier:detail", group: groups::SUPPLIER, description: "查询供应商详情" };

/// 删除供应商。
pub const DELETE: PermissionDef =
    PermissionDef { code: "supplier:delete", group: groups::SUPPLIER, description: "删除供应商" };

/// 交接供应商整体维护人。
pub const HANDOVER: PermissionDef =
    PermissionDef {
        code: "supplier:update", group: groups::SUPPLIER, description: "交接供应商整体维护人"
    };

/// 查询供应商交接待选人。
pub const HANDOVER_CANDIDATES: PermissionDef = HANDOVER.described(groups::SUPPLIER, "查询供应商交接待选人");

/// 交接供给能力负责人。
pub const CAPABILITY_HANDOVER: PermissionDef = HANDOVER.described(groups::SUPPLIER, "交接供给能力负责人");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("IMPORT_SUPPLIER_IMPORT", IMPORT_SUPPLIER_IMPORT),
    ("IMPORT_SUPPLIER_IMPORT_SUBMIT", IMPORT_SUPPLIER_IMPORT_SUBMIT),
    ("IMPORT_SUPPLIER_IMPORT_FAILURES", IMPORT_SUPPLIER_IMPORT_FAILURES),
    ("PROFILE_CREATE", PROFILE_CREATE),
    ("PROFILE_CREATE_WITH_ASSETS", PROFILE_CREATE_WITH_ASSETS),
    ("PROFILE_UPDATE", PROFILE_UPDATE),
    ("PROFILE_UPDATE_WITH_ASSETS", PROFILE_UPDATE_WITH_ASSETS),
    ("PROFILE_COMMAND_DETAIL", PROFILE_COMMAND_DETAIL),
    ("SENSITIVE_REVEAL", SENSITIVE_REVEAL),
    ("LIST", LIST),
    ("DETAIL", DETAIL),
    ("DELETE", DELETE),
    ("HANDOVER", HANDOVER),
    ("HANDOVER_CANDIDATES", HANDOVER_CANDIDATES),
    ("CAPABILITY_HANDOVER", CAPABILITY_HANDOVER),
];
