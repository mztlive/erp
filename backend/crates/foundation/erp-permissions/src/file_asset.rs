//! file_asset 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询文件资产列表。
pub const LIST: Permission = Permission::new(resources::FILE_ASSET, "list");

/// 查询文件资产详情。
pub const DETAIL: Permission = Permission::new(resources::FILE_ASSET, "detail");

/// 预览文件资产内容。
pub const PREVIEW: Permission = Permission::new(resources::FILE_ASSET, "preview");

/// 上传并登记文件资产。
pub const UPLOAD: Permission = Permission::new(resources::FILE_ASSET, "create");

/// 登记文件资产。
pub const REGISTER: Permission = UPLOAD;

/// 建立单据附件关联。
pub const DOCUMENT_ATTACHMENT_CREATE: Permission = Permission::new(resources::DOCUMENT_ATTACHMENT, "create");

/// 查询单据附件列表。
pub const DOCUMENT_ATTACHMENT_LIST: Permission = Permission::new(resources::DOCUMENT_ATTACHMENT, "list");

/// 记录安全检查结果。
pub const SCAN_RESULT: Permission = Permission::new(resources::FILE_ASSET, "update");

/// 销毁文件资产。
pub const DESTROY: Permission = Permission::new(resources::FILE_ASSET, "delete");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "LIST",
        PermissionDef {
            permission: &LIST, group: groups::FILE_ASSET, description: "查询文件资产列表"
        },
    ),
    (
        "DETAIL",
        PermissionDef {
            permission: &DETAIL, group: groups::FILE_ASSET, description: "查询文件资产详情"
        },
    ),
    (
        "PREVIEW",
        PermissionDef {
            permission: &PREVIEW, group: groups::FILE_ASSET, description: "预览文件资产内容"
        },
    ),
    (
        "UPLOAD",
        PermissionDef {
            permission: &UPLOAD, group: groups::FILE_ASSET, description: "上传并登记文件资产"
        },
    ),
    (
        "DOCUMENT_ATTACHMENT_CREATE",
        PermissionDef {
            permission: &DOCUMENT_ATTACHMENT_CREATE,
            group: groups::FILE_ASSET,
            description: "建立单据附件关联",
        },
    ),
    (
        "DOCUMENT_ATTACHMENT_LIST",
        PermissionDef {
            permission: &DOCUMENT_ATTACHMENT_LIST,
            group: groups::FILE_ASSET,
            description: "查询单据附件列表",
        },
    ),
    (
        "SCAN_RESULT",
        PermissionDef {
            permission: &SCAN_RESULT,
            group: groups::FILE_ASSET,
            description: "记录安全检查结果",
        },
    ),
    (
        "DESTROY",
        PermissionDef { permission: &DESTROY, group: groups::FILE_ASSET, description: "销毁文件资产" },
    ),
];

/// 将本人上传的文件关联到业务材料。
pub const TRANSFER: Permission = Permission::new(resources::FILE_ASSET, "transfer");

/// 将本人上传文件关联到只接受原上传者材料的业务命令。
pub const OWNED_TRANSFER: Permission = Permission::new(resources::FILE_ASSET, "transfer_owned");
