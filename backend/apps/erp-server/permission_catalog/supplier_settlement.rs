//! supplier_settlement 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询供应商结算单列表。
pub const STATEMENT_LIST: PermissionDef = PermissionDef {
    code: "supplier_settlement_statement:list",
    group: groups::SUPPLIER_SETTLEMENT,
    description: "查询供应商结算单列表",
};

/// 查询供应商结算单详情。
pub const STATEMENT_DETAIL: PermissionDef = PermissionDef {
    code: "supplier_settlement_statement:detail",
    group: groups::SUPPLIER_SETTLEMENT,
    description: "查询供应商结算单详情",
};

/// 创建供应商结算草稿。
pub const STATEMENT_CREATE: PermissionDef = PermissionDef {
    code: "supplier_settlement_statement:create",
    group: groups::SUPPLIER_SETTLEMENT,
    description: "创建供应商结算草稿",
};

/// 刷新供应商结算权威来源试算。
pub const STATEMENT_REFRESH: PermissionDef = PermissionDef {
    code: "supplier_settlement_statement:update",
    group: groups::SUPPLIER_SETTLEMENT,
    description: "刷新供应商结算权威来源试算",
};

/// 查询最新供应商结算来源证据。
pub const SOURCE_EVIDENCE_LATEST: PermissionDef = PermissionDef {
    code: "supplier_settlement_source_evidence:list",
    group: groups::SUPPLIER_SETTLEMENT,
    description: "查询最新供应商结算来源证据",
};

/// 录入供应商结算不可变来源证据。
pub const SOURCE_EVIDENCE_RECORD: PermissionDef = PermissionDef {
    code: "supplier_settlement_source_evidence:create",
    group: groups::SUPPLIER_SETTLEMENT,
    description: "录入供应商结算不可变来源证据",
};

/// 提交结算复核。
pub const STATEMENT_SUBMIT_REVIEW: PermissionDef = PermissionDef {
    code: "supplier_settlement_statement:submit",
    group: groups::SUPPLIER_SETTLEMENT,
    description: "提交结算复核",
};

/// 执行供应商结算正式复核决定。
pub const STATEMENT_REVIEW_DECIDE: PermissionDef = PermissionDef {
    code: "supplier_settlement_statement:confirm",
    group: groups::SUPPLIER_SETTLEMENT,
    description: "执行供应商结算正式复核决定",
};

/// 作废结算单。
pub const STATEMENT_VOID: PermissionDef = PermissionDef {
    code: "supplier_settlement_statement:void",
    group: groups::SUPPLIER_SETTLEMENT,
    description: "作废结算单",
};

/// 查询供应商结算明细列表。
pub const ITEM_LIST: PermissionDef = PermissionDef {
    code: "supplier_settlement_item:list",
    group: groups::SUPPLIER_SETTLEMENT,
    description: "查询供应商结算明细列表",
};

/// 查询供应商结算差异列表。
pub const DIFFERENCE_LIST: PermissionDef = PermissionDef {
    code: "supplier_settlement_difference:list",
    group: groups::SUPPLIER_SETTLEMENT,
    description: "查询供应商结算差异列表",
};

/// 登记结算差异处理结论。
pub const DIFFERENCE_DECIDE: PermissionDef = PermissionDef {
    code: "supplier_settlement_difference:update",
    group: groups::SUPPLIER_SETTLEMENT,
    description: "登记结算差异处理结论",
};

/// 追加结算差异正式补证。
pub const DIFFERENCE_EVIDENCE_APPEND: PermissionDef =
    DIFFERENCE_DECIDE.described(groups::SUPPLIER_SETTLEMENT, "追加结算差异正式补证");

/// 交接供应商结算对账负责人。
pub const STATEMENT_HANDOVER: PermissionDef =
    STATEMENT_REFRESH.described(groups::SUPPLIER_SETTLEMENT, "交接供应商结算对账负责人");

/// 查询供应商结算交接待选。
pub const STATEMENT_HANDOVER_CANDIDATES: PermissionDef =
    STATEMENT_REFRESH.described(groups::SUPPLIER_SETTLEMENT, "查询供应商结算交接待选");

/// 改派供应商结算差异处理人。
pub const STATEMENT_REASSIGN_DIFFERENCE_HANDLER: PermissionDef =
    STATEMENT_REFRESH.described(groups::SUPPLIER_SETTLEMENT, "改派供应商结算差异处理人");

/// 提交供应商结算复核。
pub const REVIEWER_OPTIONS_SUPPLIER_SETTLEMENT_REVIEWER_OPTIONS: PermissionDef =
    STATEMENT_SUBMIT_REVIEW.described(groups::SUPPLIER_SETTLEMENT, "提交供应商结算复核");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("STATEMENT_LIST", STATEMENT_LIST),
    ("STATEMENT_DETAIL", STATEMENT_DETAIL),
    ("STATEMENT_CREATE", STATEMENT_CREATE),
    ("STATEMENT_REFRESH", STATEMENT_REFRESH),
    ("SOURCE_EVIDENCE_LATEST", SOURCE_EVIDENCE_LATEST),
    ("SOURCE_EVIDENCE_RECORD", SOURCE_EVIDENCE_RECORD),
    ("STATEMENT_SUBMIT_REVIEW", STATEMENT_SUBMIT_REVIEW),
    ("STATEMENT_REVIEW_DECIDE", STATEMENT_REVIEW_DECIDE),
    ("STATEMENT_VOID", STATEMENT_VOID),
    ("ITEM_LIST", ITEM_LIST),
    ("DIFFERENCE_LIST", DIFFERENCE_LIST),
    ("DIFFERENCE_DECIDE", DIFFERENCE_DECIDE),
    ("DIFFERENCE_EVIDENCE_APPEND", DIFFERENCE_EVIDENCE_APPEND),
    ("STATEMENT_HANDOVER", STATEMENT_HANDOVER),
    ("STATEMENT_HANDOVER_CANDIDATES", STATEMENT_HANDOVER_CANDIDATES),
    ("STATEMENT_REASSIGN_DIFFERENCE_HANDLER", STATEMENT_REASSIGN_DIFFERENCE_HANDLER),
    (
        "REVIEWER_OPTIONS_SUPPLIER_SETTLEMENT_REVIEWER_OPTIONS",
        REVIEWER_OPTIONS_SUPPLIER_SETTLEMENT_REVIEWER_OPTIONS,
    ),
];
