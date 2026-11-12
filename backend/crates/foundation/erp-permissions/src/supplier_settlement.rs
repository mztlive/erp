//! supplier_settlement 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询供应商结算单列表。
pub const STATEMENT_LIST: Permission = Permission::new(resources::SUPPLIER_SETTLEMENT_STATEMENT, "list");

/// 查询供应商结算单详情。
pub const STATEMENT_DETAIL: Permission = Permission::new(resources::SUPPLIER_SETTLEMENT_STATEMENT, "detail");

/// 创建供应商结算草稿。
pub const STATEMENT_CREATE: Permission = Permission::new(resources::SUPPLIER_SETTLEMENT_STATEMENT, "create");

/// 刷新供应商结算权威来源试算。
pub const STATEMENT_REFRESH: Permission = Permission::new(resources::SUPPLIER_SETTLEMENT_STATEMENT, "update");

/// 查询最新供应商结算来源证据。
pub const SOURCE_EVIDENCE_LATEST: Permission =
    Permission::new(resources::SUPPLIER_SETTLEMENT_SOURCE_EVIDENCE, "list");

/// 录入供应商结算不可变来源证据。
pub const SOURCE_EVIDENCE_RECORD: Permission =
    Permission::new(resources::SUPPLIER_SETTLEMENT_SOURCE_EVIDENCE, "create");

/// 提交结算复核。
pub const STATEMENT_SUBMIT_REVIEW: Permission =
    Permission::new(resources::SUPPLIER_SETTLEMENT_STATEMENT, "submit");

/// 执行供应商结算正式复核决定。
pub const STATEMENT_REVIEW_DECIDE: Permission =
    Permission::new(resources::SUPPLIER_SETTLEMENT_STATEMENT, "confirm");

/// 作废结算单。
pub const STATEMENT_VOID: Permission = Permission::new(resources::SUPPLIER_SETTLEMENT_STATEMENT, "void");

/// 查询供应商结算明细列表。
pub const ITEM_LIST: Permission = Permission::new(resources::SUPPLIER_SETTLEMENT_ITEM, "list");

/// 查询供应商结算差异列表。
pub const DIFFERENCE_LIST: Permission = Permission::new(resources::SUPPLIER_SETTLEMENT_DIFFERENCE, "list");

/// 登记结算差异处理结论。
pub const DIFFERENCE_DECIDE: Permission =
    Permission::new(resources::SUPPLIER_SETTLEMENT_DIFFERENCE, "update");

/// 追加结算差异正式补证。
pub const DIFFERENCE_EVIDENCE_APPEND: Permission = DIFFERENCE_DECIDE;

/// 交接供应商结算对账负责人。
pub const STATEMENT_HANDOVER: Permission = STATEMENT_REFRESH;

/// 查询供应商结算交接待选。
pub const STATEMENT_HANDOVER_CANDIDATES: Permission = STATEMENT_REFRESH;

/// 改派供应商结算差异处理人。
pub const STATEMENT_REASSIGN_DIFFERENCE_HANDLER: Permission = STATEMENT_REFRESH;

/// 提交供应商结算复核。
pub const REVIEWER_OPTIONS_SUPPLIER_SETTLEMENT_REVIEWER_OPTIONS: Permission = STATEMENT_SUBMIT_REVIEW;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "STATEMENT_LIST",
        PermissionDef {
            permission: &STATEMENT_LIST,
            group: groups::SUPPLIER_SETTLEMENT,
            description: "查询供应商结算单列表",
        },
    ),
    (
        "STATEMENT_DETAIL",
        PermissionDef {
            permission: &STATEMENT_DETAIL,
            group: groups::SUPPLIER_SETTLEMENT,
            description: "查询供应商结算单详情",
        },
    ),
    (
        "STATEMENT_CREATE",
        PermissionDef {
            permission: &STATEMENT_CREATE,
            group: groups::SUPPLIER_SETTLEMENT,
            description: "创建供应商结算草稿",
        },
    ),
    (
        "STATEMENT_REFRESH",
        PermissionDef {
            permission: &STATEMENT_REFRESH,
            group: groups::SUPPLIER_SETTLEMENT,
            description: "刷新供应商结算权威来源试算",
        },
    ),
    (
        "SOURCE_EVIDENCE_LATEST",
        PermissionDef {
            permission: &SOURCE_EVIDENCE_LATEST,
            group: groups::SUPPLIER_SETTLEMENT,
            description: "查询最新供应商结算来源证据",
        },
    ),
    (
        "SOURCE_EVIDENCE_RECORD",
        PermissionDef {
            permission: &SOURCE_EVIDENCE_RECORD,
            group: groups::SUPPLIER_SETTLEMENT,
            description: "录入供应商结算不可变来源证据",
        },
    ),
    (
        "STATEMENT_SUBMIT_REVIEW",
        PermissionDef {
            permission: &STATEMENT_SUBMIT_REVIEW,
            group: groups::SUPPLIER_SETTLEMENT,
            description: "提交结算复核",
        },
    ),
    (
        "STATEMENT_REVIEW_DECIDE",
        PermissionDef {
            permission: &STATEMENT_REVIEW_DECIDE,
            group: groups::SUPPLIER_SETTLEMENT,
            description: "执行供应商结算正式复核决定",
        },
    ),
    (
        "STATEMENT_VOID",
        PermissionDef {
            permission: &STATEMENT_VOID,
            group: groups::SUPPLIER_SETTLEMENT,
            description: "作废结算单",
        },
    ),
    (
        "ITEM_LIST",
        PermissionDef {
            permission: &ITEM_LIST,
            group: groups::SUPPLIER_SETTLEMENT,
            description: "查询供应商结算明细列表",
        },
    ),
    (
        "DIFFERENCE_LIST",
        PermissionDef {
            permission: &DIFFERENCE_LIST,
            group: groups::SUPPLIER_SETTLEMENT,
            description: "查询供应商结算差异列表",
        },
    ),
    (
        "DIFFERENCE_DECIDE",
        PermissionDef {
            permission: &DIFFERENCE_DECIDE,
            group: groups::SUPPLIER_SETTLEMENT,
            description: "登记结算差异处理结论",
        },
    ),
];
