//! supplier_api 的接口权限目录。
use super::{PermissionDef, groups};

/// 创建供应商批量入库作业。
pub const ADOPTION_BATCHES_CREATE: PermissionDef =
    ADOPTIONS_PREVIEW.described(groups::SUPPLIER_API, "创建供应商批量入库作业");

/// 查询可恢复的供应商批量入库作业。
pub const ADOPTION_BATCHES_LIST: PermissionDef =
    ADOPTIONS_PREVIEW.described(groups::SUPPLIER_API, "查询可恢复的供应商批量入库作业");

/// 查询供应商批量入库作业。
pub const ADOPTION_BATCHES_DETAIL: PermissionDef =
    ADOPTIONS_PREVIEW.described(groups::SUPPLIER_API, "查询供应商批量入库作业");

/// 保存供应商批量入库草稿。
pub const ADOPTION_BATCHES_SAVE: PermissionDef =
    ADOPTIONS_PREVIEW.described(groups::SUPPLIER_API, "保存供应商批量入库草稿");

/// 核对供应商批量入库结果。
pub const ADOPTION_BATCHES_PREVIEW: PermissionDef =
    ADOPTIONS_PREVIEW.described(groups::SUPPLIER_API, "核对供应商批量入库结果");

/// 确认供应商批量入库作业。
pub const ADOPTION_BATCHES_SUBMIT: PermissionDef =
    ADOPTIONS_PREVIEW.described(groups::SUPPLIER_API, "确认供应商批量入库作业");

/// 核实供应商批量入库原操作结果。
pub const ADOPTION_BATCHES_RECONCILE: PermissionDef =
    ADOPTIONS_PREVIEW.described(groups::SUPPLIER_API, "核实供应商批量入库原操作结果");

/// 查证供应商批量入库原确认。
pub const ADOPTION_BATCHES_SUBMISSION_RESULT: PermissionDef =
    ADOPTIONS_PREVIEW.described(groups::SUPPLIER_API, "查证供应商批量入库原确认");

/// 读取供应商批次选择集合。
pub const ADOPTION_BATCHES_SELECTION: PermissionDef =
    ADOPTIONS_PREVIEW.described(groups::SUPPLIER_API, "读取供应商批次选择集合");

/// 查询供应商选品准备进度。
pub const ADOPTION_BATCHES_PREPARATION: PermissionDef =
    ADOPTIONS_PREVIEW.described(groups::SUPPLIER_API, "查询供应商选品准备进度");

/// 重试或取消供应商选品准备。
pub const ADOPTION_BATCHES_CONTROL_PREPARATION: PermissionDef =
    ADOPTIONS_PREVIEW.described(groups::SUPPLIER_API, "重试或取消供应商选品准备");

/// 恢复供应商批次保存与核对。
pub const ADOPTION_BATCHES_CONTROL_WORK: PermissionDef =
    ADOPTIONS_PREVIEW.described(groups::SUPPLIER_API, "恢复供应商批次保存与核对");

/// 同步供应商商品目录。
pub const ADOPTIONS_PREVIEW: PermissionDef = PermissionDef {
    code: "supplier_api_connection:catalog_sync",
    group: groups::SUPPLIER_API,
    description: "同步供应商商品目录",
};

/// 同步供应商商品目录。
pub const ADOPTIONS_CONFIRM: PermissionDef = ADOPTIONS_PREVIEW;

/// 同步供应商商品目录。
pub const ADOPTIONS_RESOLVE: PermissionDef = ADOPTIONS_PREVIEW;

/// 查看供应商 API 连接详情。
pub const CATALOG_LIST: PermissionDef =
    CONNECTION_DETAIL.described(groups::SUPPLIER_API, "查看供应商 API 连接详情");

/// 查看供应商 API 连接详情。
pub const CATALOG_IDENTITY_REVIEW: PermissionDef =
    CONNECTION_DETAIL.described(groups::SUPPLIER_API, "查看供应商 API 连接详情");

/// 同步供应商商品目录。
pub const CATALOG_CONFIRM_IDENTITY_REVIEW: PermissionDef = ADOPTIONS_PREVIEW;

/// 创建供应商 API 连接。
pub const CONNECTOR_TYPES_OPTIONS: PermissionDef = CONNECTION_CREATE;

/// 读取供应商协议目录资料。
pub const DANGAOSHUSHU_READ: PermissionDef = PermissionDef {
    code: "supplier_api_connection:protocol_read",
    group: groups::SUPPLIER_API,
    description: "读取供应商协议目录资料",
};

/// 签发供应商技术绑定票据。
pub const DANGAOSHUSHU_REFERENCE_TICKETS: PermissionDef = PermissionDef {
    code: "supplier_api_connection:manage_credential_reference",
    group: groups::SUPPLIER_API,
    description: "签发供应商技术绑定票据",
};

/// 配置京东访问及刷新令牌。
pub const JD_VOP_TOKENS_CONFIGURE: PermissionDef =
    JD_VOP_AUTHORIZE.described(groups::SUPPLIER_API, "配置京东访问及刷新令牌");

/// 查询京东账号授权状态。
pub const JD_VOP_STATUS: PermissionDef =
    CONNECTION_DETAIL.described(groups::SUPPLIER_API, "查询京东账号授权状态");

/// 授权京东企业采购账号。
pub const JD_VOP_AUTHORIZE: PermissionDef = PermissionDef {
    code: "supplier_api_connection:authorize",
    group: groups::SUPPLIER_API,
    description: "授权京东企业采购账号",
};

/// 查询供应商 API 连接列表。
pub const CONNECTION_LIST: PermissionDef = PermissionDef {
    code: "supplier_api_connection:list",
    group: groups::SUPPLIER_API,
    description: "查询供应商 API 连接列表",
};

/// 查询供应商 API 连接详情。
pub const CONNECTION_DETAIL: PermissionDef = PermissionDef {
    code: "supplier_api_connection:detail",
    group: groups::SUPPLIER_API,
    description: "查询供应商 API 连接详情",
};

/// 创建供应商 API 连接。
pub const CONNECTION_CREATE: PermissionDef = PermissionDef {
    code: "supplier_api_connection:create",
    group: groups::SUPPLIER_API,
    description: "创建供应商 API 连接",
};

/// 执行供应商 API 连接治理强命令。
pub const CONNECTION_COMMAND: PermissionDef = PermissionDef {
    code: "supplier_api_connection:command",
    group: groups::SUPPLIER_API,
    description: "执行供应商 API 连接治理强命令",
};

/// 查询供应商 API 连接后台任务。
pub const CONNECTION_JOB_DETAIL: PermissionDef =
    CONNECTION_DETAIL.described(groups::SUPPLIER_API, "查询供应商 API 连接后台任务");

/// 查看供应商接口配置选项。
pub const REFERENCES_OPTIONS: PermissionDef = PermissionDef {
    code: "supplier_api_connection:view_reference_metadata",
    group: groups::SUPPLIER_API,
    description: "查看供应商接口配置选项",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("ADOPTION_BATCHES_CREATE", ADOPTION_BATCHES_CREATE),
    ("ADOPTION_BATCHES_LIST", ADOPTION_BATCHES_LIST),
    ("ADOPTION_BATCHES_DETAIL", ADOPTION_BATCHES_DETAIL),
    ("ADOPTION_BATCHES_SAVE", ADOPTION_BATCHES_SAVE),
    ("ADOPTION_BATCHES_PREVIEW", ADOPTION_BATCHES_PREVIEW),
    ("ADOPTION_BATCHES_SUBMIT", ADOPTION_BATCHES_SUBMIT),
    ("ADOPTION_BATCHES_RECONCILE", ADOPTION_BATCHES_RECONCILE),
    ("ADOPTION_BATCHES_SUBMISSION_RESULT", ADOPTION_BATCHES_SUBMISSION_RESULT),
    ("ADOPTION_BATCHES_SELECTION", ADOPTION_BATCHES_SELECTION),
    ("ADOPTION_BATCHES_PREPARATION", ADOPTION_BATCHES_PREPARATION),
    ("ADOPTION_BATCHES_CONTROL_PREPARATION", ADOPTION_BATCHES_CONTROL_PREPARATION),
    ("ADOPTION_BATCHES_CONTROL_WORK", ADOPTION_BATCHES_CONTROL_WORK),
    ("ADOPTIONS_PREVIEW", ADOPTIONS_PREVIEW),
    ("ADOPTIONS_CONFIRM", ADOPTIONS_CONFIRM),
    ("ADOPTIONS_RESOLVE", ADOPTIONS_RESOLVE),
    ("CATALOG_LIST", CATALOG_LIST),
    ("CATALOG_IDENTITY_REVIEW", CATALOG_IDENTITY_REVIEW),
    ("CATALOG_CONFIRM_IDENTITY_REVIEW", CATALOG_CONFIRM_IDENTITY_REVIEW),
    ("CONNECTOR_TYPES_OPTIONS", CONNECTOR_TYPES_OPTIONS),
    ("DANGAOSHUSHU_READ", DANGAOSHUSHU_READ),
    ("DANGAOSHUSHU_REFERENCE_TICKETS", DANGAOSHUSHU_REFERENCE_TICKETS),
    ("JD_VOP_TOKENS_CONFIGURE", JD_VOP_TOKENS_CONFIGURE),
    ("JD_VOP_STATUS", JD_VOP_STATUS),
    ("JD_VOP_AUTHORIZE", JD_VOP_AUTHORIZE),
    ("CONNECTION_LIST", CONNECTION_LIST),
    ("CONNECTION_DETAIL", CONNECTION_DETAIL),
    ("CONNECTION_CREATE", CONNECTION_CREATE),
    ("CONNECTION_COMMAND", CONNECTION_COMMAND),
    ("CONNECTION_JOB_DETAIL", CONNECTION_JOB_DETAIL),
    ("REFERENCES_OPTIONS", REFERENCES_OPTIONS),
];
