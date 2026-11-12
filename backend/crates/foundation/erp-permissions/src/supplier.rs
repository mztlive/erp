//! supplier 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 导入供应商资料。
pub const IMPORT_SUPPLIER_IMPORT: Permission = Permission::new(resources::SUPPLIER, "create");

/// 提交供应商导入任务。
pub const IMPORT_SUPPLIER_IMPORT_SUBMIT: Permission = IMPORT_SUPPLIER_IMPORT;

/// 下载供应商导入待处理行。
pub const IMPORT_SUPPLIER_IMPORT_FAILURES: Permission = IMPORT_SUPPLIER_IMPORT;

/// 创建完整供应商资料。
pub const PROFILE_CREATE: Permission = IMPORT_SUPPLIER_IMPORT;

/// 一次创建供应商资料及资质文件。
pub const PROFILE_CREATE_WITH_ASSETS: Permission = IMPORT_SUPPLIER_IMPORT;

/// 修订完整供应商资料。
pub const PROFILE_UPDATE: Permission = HANDOVER;

/// 一次修订供应商资料及资质文件。
pub const PROFILE_UPDATE_WITH_ASSETS: Permission = HANDOVER;

/// 查询供应商资料保存结果。
pub const PROFILE_COMMAND_DETAIL: Permission = DETAIL;

/// 短时查看供应商敏感字段。
pub const SENSITIVE_REVEAL: Permission = Permission::new(resources::SUPPLIER_SENSITIVE, "reveal");

/// 查询供应商列表。
pub const LIST: Permission = Permission::new(resources::SUPPLIER, "list");

/// 查询供应商详情。
pub const DETAIL: Permission = Permission::new(resources::SUPPLIER, "detail");

/// 删除供应商。
pub const DELETE: Permission = Permission::new(resources::SUPPLIER, "delete");

/// 交接供应商整体维护人。
pub const HANDOVER: Permission = Permission::new(resources::SUPPLIER, "update");

/// 查询供应商交接待选人。
pub const HANDOVER_CANDIDATES: Permission = HANDOVER;

/// 交接供给能力负责人。
pub const CAPABILITY_HANDOVER: Permission = HANDOVER;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "IMPORT_SUPPLIER_IMPORT",
        PermissionDef {
            permission: &IMPORT_SUPPLIER_IMPORT,
            group: groups::SUPPLIER,
            description: "导入供应商资料",
        },
    ),
    (
        "SENSITIVE_REVEAL",
        PermissionDef {
            permission: &SENSITIVE_REVEAL,
            group: groups::SUPPLIER,
            description: "短时查看供应商敏感字段",
        },
    ),
    ("LIST", PermissionDef { permission: &LIST, group: groups::SUPPLIER, description: "查询供应商列表" }),
    ("DETAIL", PermissionDef { permission: &DETAIL, group: groups::SUPPLIER, description: "查询供应商详情" }),
    ("DELETE", PermissionDef { permission: &DELETE, group: groups::SUPPLIER, description: "删除供应商" }),
    (
        "HANDOVER",
        PermissionDef {
            permission: &HANDOVER, group: groups::SUPPLIER, description: "交接供应商整体维护人"
        },
    ),
];
