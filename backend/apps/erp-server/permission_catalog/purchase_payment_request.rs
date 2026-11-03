//! purchase_payment_request 的接口权限目录。
use super::{PermissionDef, groups, payable, work_item};

/// 查询可申请采购来源。
pub const CANDIDATE_LIST_PAGE: PermissionDef =
    CREATE.described(groups::PURCHASE_PAYMENT_REQUEST, "查询可申请采购来源");

/// 查询可申请采购来源。
pub const CANDIDATE_LIST_IDS: PermissionDef =
    CREATE.described(groups::PURCHASE_PAYMENT_REQUEST, "查询可申请采购来源");

/// 在批准采购付款申请内登记付款。
pub const CREDIT_LIST: PermissionDef = PermissionDef {
    code: "purchase_payment_request:pay",
    group: groups::PURCHASE_PAYMENT_REQUEST,
    description: "在批准采购付款申请内登记付款",
};

/// 在批准采购付款申请内登记付款。
pub const CREDIT_APPLY: PermissionDef = CREDIT_LIST;

/// 在批准采购付款申请内执行预存款。
pub const DEPOSIT_EXECUTE: PermissionDef =
    CREDIT_LIST.described(groups::PURCHASE_PAYMENT_REQUEST, "在批准采购付款申请内执行预存款");

/// 读取采购付款申请历史提交。
pub const MATERIALS_SUBMISSION: PermissionDef =
    DETAIL.described(groups::PURCHASE_PAYMENT_REQUEST, "读取采购付款申请历史提交");

/// 读取付款申请任务上下文。
pub const MATERIALS_TASK_CONTEXT: PermissionDef =
    work_item::DETAIL.described(groups::FINANCIAL_FILES, "读取付款申请任务上下文");

/// 下载本付款申请冻结材料。
pub const MATERIALS_MATERIAL_DOWNLOAD: PermissionDef =
    DETAIL.described(groups::PURCHASE_PAYMENT_REQUEST, "下载本付款申请冻结材料");

/// 预览本付款申请冻结材料。
pub const MATERIALS_MATERIAL_PREVIEW: PermissionDef =
    DETAIL.described(groups::PURCHASE_PAYMENT_REQUEST, "预览本付款申请冻结材料");

/// 查询采购付款申请。
pub const LIST: PermissionDef = PermissionDef {
    code: "purchase_payment_request:list",
    group: groups::PURCHASE_PAYMENT_REQUEST,
    description: "查询采购付款申请",
};

/// 查询可申请采购来源。
pub const CANDIDATES: PermissionDef =
    CREATE.described(groups::PURCHASE_PAYMENT_REQUEST, "查询可申请采购来源");

/// 读取采购付款申请。
pub const DETAIL: PermissionDef = PermissionDef {
    code: "purchase_payment_request:detail",
    group: groups::PURCHASE_PAYMENT_REQUEST,
    description: "读取采购付款申请",
};

/// 创建采购付款申请。
pub const CREATE: PermissionDef = PermissionDef {
    code: "purchase_payment_request:create",
    group: groups::PURCHASE_PAYMENT_REQUEST,
    description: "创建采购付款申请",
};

/// 修改采购付款申请草稿。
pub const UPDATE: PermissionDef = PermissionDef {
    code: "purchase_payment_request:update",
    group: groups::PURCHASE_PAYMENT_REQUEST,
    description: "修改采购付款申请草稿",
};

/// 提交采购付款申请审核。
pub const SUBMIT: PermissionDef = PermissionDef {
    code: "purchase_payment_request:submit",
    group: groups::PURCHASE_PAYMENT_REQUEST,
    description: "提交采购付款申请审核",
};

/// 撤回采购付款申请。
pub const WITHDRAW: PermissionDef = PermissionDef {
    code: "purchase_payment_request:withdraw",
    group: groups::PURCHASE_PAYMENT_REQUEST,
    description: "撤回采购付款申请",
};

/// 关闭采购付款申请未付部分。
pub const CLOSE: PermissionDef = PermissionDef {
    code: "purchase_payment_request:close",
    group: groups::PURCHASE_PAYMENT_REQUEST,
    description: "关闭采购付款申请未付部分",
};

/// 批量创建采购付款申请。
pub const CREATE_BATCH: PermissionDef =
    CREATE.described(groups::PURCHASE_PAYMENT_REQUEST, "批量创建采购付款申请");

/// 在批准采购付款申请内登记付款。
pub const PAYMENTS_PAY: PermissionDef = CREDIT_LIST;

/// 主动查看当前付款申请完整收款账号。
pub const PAYMENTS_REVEAL_ACCOUNT: PermissionDef = payable::PAYMENT_RECIPIENT_REVEAL
    .described(groups::PURCHASE_PAYMENT_REQUEST, "主动查看当前付款申请完整收款账号");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("CANDIDATE_LIST_PAGE", CANDIDATE_LIST_PAGE),
    ("CANDIDATE_LIST_IDS", CANDIDATE_LIST_IDS),
    ("CREDIT_LIST", CREDIT_LIST),
    ("CREDIT_APPLY", CREDIT_APPLY),
    ("DEPOSIT_EXECUTE", DEPOSIT_EXECUTE),
    ("MATERIALS_SUBMISSION", MATERIALS_SUBMISSION),
    ("MATERIALS_TASK_CONTEXT", MATERIALS_TASK_CONTEXT),
    ("MATERIALS_MATERIAL_DOWNLOAD", MATERIALS_MATERIAL_DOWNLOAD),
    ("MATERIALS_MATERIAL_PREVIEW", MATERIALS_MATERIAL_PREVIEW),
    ("LIST", LIST),
    ("CANDIDATES", CANDIDATES),
    ("DETAIL", DETAIL),
    ("CREATE", CREATE),
    ("UPDATE", UPDATE),
    ("SUBMIT", SUBMIT),
    ("WITHDRAW", WITHDRAW),
    ("CLOSE", CLOSE),
    ("CREATE_BATCH", CREATE_BATCH),
    ("PAYMENTS_PAY", PAYMENTS_PAY),
    ("PAYMENTS_REVEAL_ACCOUNT", PAYMENTS_REVEAL_ACCOUNT),
];
