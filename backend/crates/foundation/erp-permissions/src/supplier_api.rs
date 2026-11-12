//! supplier_api 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 创建供应商批量入库作业。
pub const ADOPTION_BATCHES_CREATE: Permission = ADOPTIONS_PREVIEW;

/// 查询可恢复的供应商批量入库作业。
pub const ADOPTION_BATCHES_LIST: Permission = ADOPTIONS_PREVIEW;

/// 查询供应商批量入库作业。
pub const ADOPTION_BATCHES_DETAIL: Permission = ADOPTIONS_PREVIEW;

/// 保存供应商批量入库草稿。
pub const ADOPTION_BATCHES_SAVE: Permission = ADOPTIONS_PREVIEW;

/// 核对供应商批量入库结果。
pub const ADOPTION_BATCHES_PREVIEW: Permission = ADOPTIONS_PREVIEW;

/// 确认供应商批量入库作业。
pub const ADOPTION_BATCHES_SUBMIT: Permission = ADOPTIONS_PREVIEW;

/// 核实供应商批量入库原操作结果。
pub const ADOPTION_BATCHES_RECONCILE: Permission = ADOPTIONS_PREVIEW;

/// 查证供应商批量入库原确认。
pub const ADOPTION_BATCHES_SUBMISSION_RESULT: Permission = ADOPTIONS_PREVIEW;

/// 读取供应商批次选择集合。
pub const ADOPTION_BATCHES_SELECTION: Permission = ADOPTIONS_PREVIEW;

/// 查询供应商选品准备进度。
pub const ADOPTION_BATCHES_PREPARATION: Permission = ADOPTIONS_PREVIEW;

/// 重试或取消供应商选品准备。
pub const ADOPTION_BATCHES_CONTROL_PREPARATION: Permission = ADOPTIONS_PREVIEW;

/// 恢复供应商批次保存与核对。
pub const ADOPTION_BATCHES_CONTROL_WORK: Permission = ADOPTIONS_PREVIEW;

/// 同步供应商商品目录。
pub const ADOPTIONS_PREVIEW: Permission = Permission::new(resources::SUPPLIER_API_CONNECTION, "catalog_sync");

/// 同步供应商商品目录。
pub const ADOPTIONS_CONFIRM: Permission = ADOPTIONS_PREVIEW;

/// 同步供应商商品目录。
pub const ADOPTIONS_RESOLVE: Permission = ADOPTIONS_PREVIEW;

/// 查看供应商 API 连接详情。
pub const CATALOG_LIST: Permission = CONNECTION_DETAIL;

/// 查看供应商 API 连接详情。
pub const CATALOG_IDENTITY_REVIEW: Permission = CONNECTION_DETAIL;

/// 同步供应商商品目录。
pub const CATALOG_CONFIRM_IDENTITY_REVIEW: Permission = ADOPTIONS_PREVIEW;

/// 创建供应商 API 连接。
pub const CONNECTOR_TYPES_OPTIONS: Permission = CONNECTION_CREATE;

/// 读取供应商协议目录资料。
pub const DANGAOSHUSHU_READ: Permission =
    Permission::new(resources::SUPPLIER_API_CONNECTION, "protocol_read");

/// 签发供应商技术绑定票据。
pub const DANGAOSHUSHU_REFERENCE_TICKETS: Permission =
    Permission::new(resources::SUPPLIER_API_CONNECTION, "manage_credential_reference");

/// 配置京东访问及刷新令牌。
pub const JD_VOP_TOKENS_CONFIGURE: Permission = JD_VOP_AUTHORIZE;

/// 查询京东账号授权状态。
pub const JD_VOP_STATUS: Permission = CONNECTION_DETAIL;

/// 授权京东企业采购账号。
pub const JD_VOP_AUTHORIZE: Permission = Permission::new(resources::SUPPLIER_API_CONNECTION, "authorize");

/// 查询供应商 API 连接列表。
pub const CONNECTION_LIST: Permission = Permission::new(resources::SUPPLIER_API_CONNECTION, "list");

/// 查询供应商 API 连接详情。
pub const CONNECTION_DETAIL: Permission = Permission::new(resources::SUPPLIER_API_CONNECTION, "detail");

/// 创建供应商 API 连接。
pub const CONNECTION_CREATE: Permission = Permission::new(resources::SUPPLIER_API_CONNECTION, "create");

/// 执行供应商 API 连接治理强命令。
pub const CONNECTION_COMMAND: Permission = Permission::new(resources::SUPPLIER_API_CONNECTION, "command");

/// 查询供应商 API 连接后台任务。
pub const CONNECTION_JOB_DETAIL: Permission = CONNECTION_DETAIL;

/// 查看供应商接口配置选项。
pub const REFERENCES_OPTIONS: Permission =
    Permission::new(resources::SUPPLIER_API_CONNECTION, "view_reference_metadata");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "ADOPTIONS_PREVIEW",
        PermissionDef {
            permission: &ADOPTIONS_PREVIEW,
            group: groups::SUPPLIER_API,
            description: "同步供应商商品目录",
        },
    ),
    (
        "DANGAOSHUSHU_READ",
        PermissionDef {
            permission: &DANGAOSHUSHU_READ,
            group: groups::SUPPLIER_API,
            description: "读取供应商协议目录资料",
        },
    ),
    (
        "DANGAOSHUSHU_REFERENCE_TICKETS",
        PermissionDef {
            permission: &DANGAOSHUSHU_REFERENCE_TICKETS,
            group: groups::SUPPLIER_API,
            description: "签发供应商技术绑定票据",
        },
    ),
    (
        "JD_VOP_AUTHORIZE",
        PermissionDef {
            permission: &JD_VOP_AUTHORIZE,
            group: groups::SUPPLIER_API,
            description: "授权京东企业采购账号",
        },
    ),
    (
        "CONNECTION_LIST",
        PermissionDef {
            permission: &CONNECTION_LIST,
            group: groups::SUPPLIER_API,
            description: "查询供应商 API 连接列表",
        },
    ),
    (
        "CONNECTION_DETAIL",
        PermissionDef {
            permission: &CONNECTION_DETAIL,
            group: groups::SUPPLIER_API,
            description: "查询供应商 API 连接详情",
        },
    ),
    (
        "CONNECTION_CREATE",
        PermissionDef {
            permission: &CONNECTION_CREATE,
            group: groups::SUPPLIER_API,
            description: "创建供应商 API 连接",
        },
    ),
    (
        "CONNECTION_COMMAND",
        PermissionDef {
            permission: &CONNECTION_COMMAND,
            group: groups::SUPPLIER_API,
            description: "执行供应商 API 连接治理强命令",
        },
    ),
    (
        "REFERENCES_OPTIONS",
        PermissionDef {
            permission: &REFERENCES_OPTIONS,
            group: groups::SUPPLIER_API,
            description: "查看供应商接口配置选项",
        },
    ),
];
