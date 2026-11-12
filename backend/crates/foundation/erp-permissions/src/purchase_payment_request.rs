//! purchase_payment_request 的内置权限。
use crate::{Permission, PermissionDef, groups, payable, resources, work_item};

/// 查询可申请采购来源。
pub const CANDIDATE_LIST_PAGE: Permission = CREATE;

/// 查询可申请采购来源。
pub const CANDIDATE_LIST_IDS: Permission = CREATE;

/// 在批准采购付款申请内登记付款。
pub const CREDIT_LIST: Permission = Permission::new(resources::PURCHASE_PAYMENT_REQUEST, "pay");

/// 在批准采购付款申请内登记付款。
pub const CREDIT_APPLY: Permission = CREDIT_LIST;

/// 在批准采购付款申请内执行预存款。
pub const DEPOSIT_EXECUTE: Permission = CREDIT_LIST;

/// 读取采购付款申请历史提交。
pub const MATERIALS_SUBMISSION: Permission = DETAIL;

/// 读取付款申请任务上下文。
pub const MATERIALS_TASK_CONTEXT: Permission = work_item::DETAIL;

/// 下载本付款申请冻结材料。
pub const MATERIALS_MATERIAL_DOWNLOAD: Permission = DETAIL;

/// 预览本付款申请冻结材料。
pub const MATERIALS_MATERIAL_PREVIEW: Permission = DETAIL;

/// 查询采购付款申请。
pub const LIST: Permission = Permission::new(resources::PURCHASE_PAYMENT_REQUEST, "list");

/// 查询可申请采购来源。
pub const CANDIDATES: Permission = CREATE;

/// 读取采购付款申请。
pub const DETAIL: Permission = Permission::new(resources::PURCHASE_PAYMENT_REQUEST, "detail");

/// 创建采购付款申请。
pub const CREATE: Permission = Permission::new(resources::PURCHASE_PAYMENT_REQUEST, "create");

/// 修改采购付款申请草稿。
pub const UPDATE: Permission = Permission::new(resources::PURCHASE_PAYMENT_REQUEST, "update");

/// 提交采购付款申请审核。
pub const SUBMIT: Permission = Permission::new(resources::PURCHASE_PAYMENT_REQUEST, "submit");

/// 撤回采购付款申请。
pub const WITHDRAW: Permission = Permission::new(resources::PURCHASE_PAYMENT_REQUEST, "withdraw");

/// 关闭采购付款申请未付部分。
pub const CLOSE: Permission = Permission::new(resources::PURCHASE_PAYMENT_REQUEST, "close");

/// 批量创建采购付款申请。
pub const CREATE_BATCH: Permission = CREATE;

/// 在批准采购付款申请内登记付款。
pub const PAYMENTS_PAY: Permission = CREDIT_LIST;

/// 主动查看当前付款申请完整收款账号。
pub const PAYMENTS_REVEAL_ACCOUNT: Permission = payable::PAYMENT_RECIPIENT_REVEAL;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "CREDIT_LIST",
        PermissionDef {
            permission: &CREDIT_LIST,
            group: groups::PURCHASE_PAYMENT_REQUEST,
            description: "在批准采购付款申请内登记付款",
        },
    ),
    (
        "LIST",
        PermissionDef {
            permission: &LIST,
            group: groups::PURCHASE_PAYMENT_REQUEST,
            description: "查询采购付款申请",
        },
    ),
    (
        "DETAIL",
        PermissionDef {
            permission: &DETAIL,
            group: groups::PURCHASE_PAYMENT_REQUEST,
            description: "读取采购付款申请",
        },
    ),
    (
        "CREATE",
        PermissionDef {
            permission: &CREATE,
            group: groups::PURCHASE_PAYMENT_REQUEST,
            description: "创建采购付款申请",
        },
    ),
    (
        "UPDATE",
        PermissionDef {
            permission: &UPDATE,
            group: groups::PURCHASE_PAYMENT_REQUEST,
            description: "修改采购付款申请草稿",
        },
    ),
    (
        "SUBMIT",
        PermissionDef {
            permission: &SUBMIT,
            group: groups::PURCHASE_PAYMENT_REQUEST,
            description: "提交采购付款申请审核",
        },
    ),
    (
        "WITHDRAW",
        PermissionDef {
            permission: &WITHDRAW,
            group: groups::PURCHASE_PAYMENT_REQUEST,
            description: "撤回采购付款申请",
        },
    ),
    (
        "CLOSE",
        PermissionDef {
            permission: &CLOSE,
            group: groups::PURCHASE_PAYMENT_REQUEST,
            description: "关闭采购付款申请未付部分",
        },
    ),
];
