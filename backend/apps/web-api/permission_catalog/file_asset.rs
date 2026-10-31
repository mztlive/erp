//! file_asset 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询文件资产列表。
pub const LIST: PermissionDef =
    PermissionDef {
        code: "file_asset:list", group: groups::FILE_ASSET, description: "查询文件资产列表"
    };

/// 查询文件资产详情。
pub const DETAIL: PermissionDef =
    PermissionDef {
        code: "file_asset:detail", group: groups::FILE_ASSET, description: "查询文件资产详情"
    };

/// 预览文件资产内容。
pub const PREVIEW: PermissionDef =
    PermissionDef {
        code: "file_asset:preview", group: groups::FILE_ASSET, description: "预览文件资产内容"
    };

/// 上传并登记文件资产。
pub const UPLOAD: PermissionDef =
    PermissionDef {
        code: "file_asset:create", group: groups::FILE_ASSET, description: "上传并登记文件资产"
    };

/// 登记文件资产。
pub const REGISTER: PermissionDef = UPLOAD.described(groups::FILE_ASSET, "登记文件资产");

/// 建立单据附件关联。
pub const DOCUMENT_ATTACHMENT_CREATE: PermissionDef = PermissionDef {
    code: "document_attachment:create",
    group: groups::FILE_ASSET,
    description: "建立单据附件关联",
};

/// 查询单据附件列表。
pub const DOCUMENT_ATTACHMENT_LIST: PermissionDef = PermissionDef {
    code: "document_attachment:list",
    group: groups::FILE_ASSET,
    description: "查询单据附件列表",
};

/// 记录安全检查结果。
pub const SCAN_RESULT: PermissionDef =
    PermissionDef {
        code: "file_asset:update", group: groups::FILE_ASSET, description: "记录安全检查结果"
    };

/// 销毁文件资产。
pub const DESTROY: PermissionDef =
    PermissionDef { code: "file_asset:delete", group: groups::FILE_ASSET, description: "销毁文件资产" };

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("LIST", LIST),
    ("DETAIL", DETAIL),
    ("PREVIEW", PREVIEW),
    ("UPLOAD", UPLOAD),
    ("REGISTER", REGISTER),
    ("DOCUMENT_ATTACHMENT_CREATE", DOCUMENT_ATTACHMENT_CREATE),
    ("DOCUMENT_ATTACHMENT_LIST", DOCUMENT_ATTACHMENT_LIST),
    ("SCAN_RESULT", SCAN_RESULT),
    ("DESTROY", DESTROY),
];
