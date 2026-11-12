//! purchase_order 的内置权限。
use crate::{Permission, PermissionDef, groups, purchase_payment_request, resources};

/// 读取原采购变更草稿。
pub const CHANGE_DRAFT_PURCHASE_CHANGE_DRAFT: Permission = PURCHASE_CHANGE_SUBMIT;

/// 保存原采购变更草稿。
pub const CHANGE_DRAFT_SAVE: Permission = PURCHASE_CHANGE_SUBMIT;

/// 作废采购变更单。
pub const CHANGE_VOID_PURCHASE_CHANGE_VOID: Permission =
    Permission::new(resources::PURCHASE_CHANGE_ORDER, "delete");

/// 下载供应商采购单。
pub const CORRESPONDENCE_PURCHASE_PAPER: Permission = DETAIL;

/// 发送采购单邮件给供应商。
pub const CORRESPONDENCE_SEND_PURCHASE_EMAIL: Permission =
    Permission::new(resources::PURCHASE_ORDER, "send_email");

/// 查看采购邮件发送记录。
pub const CORRESPONDENCE_PURCHASE_EMAIL_HISTORY: Permission = DETAIL;

/// 保存采购草稿。
pub const DELIVERY_OPTIONS_PURCHASE_DELIVERY_OPTIONS: Permission = SAVE_DRAFT;

/// 查询费用采购来源销售单。
pub const EXPENSE_EXPENSE_SOURCES: Permission = CREATE;

/// 读取费用采购来源明细。
pub const EXPENSE_EXPENSE_SOURCE_DETAIL: Permission = CREATE;

/// 从销售单追加费用采购单。
pub const EXPENSE_EXPENSE_PURCHASE_CREATE: Permission = CREATE;

/// 确认采购费用行已发生。
pub const EXPENSE_EXPENSE_OCCURRENCE_CONFIRM: Permission =
    Permission::new(resources::PURCHASE_ORDER, "confirm_expense");

/// 标记供应商不收运费。
pub const FREIGHT_FREIGHT_WAIVER_CREATE: Permission = SAVE_DRAFT;

/// 查询采购单列表。
pub const LIST: Permission = Permission::new(resources::PURCHASE_ORDER, "list");

/// 查询采购单对象中心。
pub const DETAIL: Permission = Permission::new(resources::PURCHASE_ORDER, "detail");

/// 依据采购确认创建采购单。
pub const CREATE: Permission = Permission::new(resources::PURCHASE_ORDER, "create");

/// 确认供给分配。
pub const CREATE_FROM_SOURCING: Permission = CREATE;

/// 保存采购草稿。
pub const SAVE_DRAFT: Permission = Permission::new(resources::PURCHASE_ORDER, "update");

/// 作废采购单草稿。
pub const VOID: Permission = Permission::new(resources::PURCHASE_ORDER, "delete");

/// 提交采购单审批。
pub const SUBMIT: Permission = Permission::new(resources::PURCHASE_ORDER, "submit");

/// 登记维持原采购依据。
pub const MAINTAIN_SOURCE: Permission = Permission::new(resources::PURCHASE_ORDER, "maintain_source");

/// 撤回采购单审批。
pub const CANCEL_APPROVAL: Permission = Permission::new(resources::PURCHASE_ORDER, "cancel_approval");

/// 查询供给分配依据。
pub const PURCHASE_CREATION_BASIS_LIST: Permission = CREATE;

/// 发起采购变更。
pub const PURCHASE_CHANGE_CREATE: Permission = Permission::new(resources::PURCHASE_CHANGE_ORDER, "create");

/// 提交采购变更审批。
pub const PURCHASE_CHANGE_SUBMIT: Permission = Permission::new(resources::PURCHASE_CHANGE_ORDER, "submit");

/// 采购变更生效。
pub const PURCHASE_CHANGE_EFFECT: Permission = Permission::new(resources::PURCHASE_CHANGE_ORDER, "post");

/// 撤回采购变更审批。
pub const PURCHASE_CHANGE_CANCEL_APPROVAL: Permission =
    Permission::new(resources::PURCHASE_CHANGE_ORDER, "cancel_approval");

/// 查询采购变更单列表。
pub const PURCHASE_CHANGE_LIST: Permission = Permission::new(resources::PURCHASE_CHANGE_ORDER, "list");

/// 查询采购变更单详情。
pub const PURCHASE_CHANGE_DETAIL: Permission = Permission::new(resources::PURCHASE_CHANGE_ORDER, "detail");

/// 查询采购付款安排。
pub const PAYMENT_ARRANGEMENT_PAYMENT_ARRANGEMENT: Permission = DETAIL;

/// 安排采购付款跟进。
pub const PAYMENT_ARRANGEMENT_SCHEDULE_PAYMENT_FOLLOW_UP: Permission = purchase_payment_request::SUBMIT;

/// 查询采购单对象中心。
pub const PAYMENTS_PURCHASE_PAYMENTS: Permission = DETAIL;

/// 查询采购单对象中心。
pub const PAYMENTS_PURCHASE_PAYMENT_RECEIPT: Permission = DETAIL;

/// 下载采购关联销售合同及凭证。
pub const SALES_MATERIAL_PURCHASE_SALES_MATERIAL_DOWNLOAD: Permission = DETAIL;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "CHANGE_VOID_PURCHASE_CHANGE_VOID",
        PermissionDef {
            permission: &CHANGE_VOID_PURCHASE_CHANGE_VOID,
            group: groups::PURCHASE_ORDER,
            description: "作废采购变更单",
        },
    ),
    (
        "CORRESPONDENCE_SEND_PURCHASE_EMAIL",
        PermissionDef {
            permission: &CORRESPONDENCE_SEND_PURCHASE_EMAIL,
            group: groups::PURCHASE_ORDER,
            description: "发送采购单邮件给供应商",
        },
    ),
    (
        "EXPENSE_EXPENSE_OCCURRENCE_CONFIRM",
        PermissionDef {
            permission: &EXPENSE_EXPENSE_OCCURRENCE_CONFIRM,
            group: groups::PURCHASE_ORDER,
            description: "确认采购费用行已发生",
        },
    ),
    (
        "LIST",
        PermissionDef {
            permission: &LIST, group: groups::PURCHASE_ORDER, description: "查询采购单列表"
        },
    ),
    (
        "DETAIL",
        PermissionDef {
            permission: &DETAIL,
            group: groups::PURCHASE_ORDER,
            description: "查询采购单对象中心",
        },
    ),
    (
        "CREATE",
        PermissionDef {
            permission: &CREATE,
            group: groups::PURCHASE_ORDER,
            description: "依据采购确认创建采购单",
        },
    ),
    (
        "SAVE_DRAFT",
        PermissionDef {
            permission: &SAVE_DRAFT, group: groups::PURCHASE_ORDER, description: "保存采购草稿"
        },
    ),
    (
        "VOID",
        PermissionDef {
            permission: &VOID, group: groups::PURCHASE_ORDER, description: "作废采购单草稿"
        },
    ),
    (
        "SUBMIT",
        PermissionDef {
            permission: &SUBMIT, group: groups::PURCHASE_ORDER, description: "提交采购单审批"
        },
    ),
    (
        "MAINTAIN_SOURCE",
        PermissionDef {
            permission: &MAINTAIN_SOURCE,
            group: groups::PURCHASE_ORDER,
            description: "登记维持原采购依据",
        },
    ),
    (
        "CANCEL_APPROVAL",
        PermissionDef {
            permission: &CANCEL_APPROVAL,
            group: groups::PURCHASE_ORDER,
            description: "撤回采购单审批",
        },
    ),
    (
        "PURCHASE_CHANGE_CREATE",
        PermissionDef {
            permission: &PURCHASE_CHANGE_CREATE,
            group: groups::PURCHASE_ORDER,
            description: "发起采购变更",
        },
    ),
    (
        "PURCHASE_CHANGE_SUBMIT",
        PermissionDef {
            permission: &PURCHASE_CHANGE_SUBMIT,
            group: groups::PURCHASE_ORDER,
            description: "提交采购变更审批",
        },
    ),
    (
        "PURCHASE_CHANGE_EFFECT",
        PermissionDef {
            permission: &PURCHASE_CHANGE_EFFECT,
            group: groups::PURCHASE_ORDER,
            description: "采购变更生效",
        },
    ),
    (
        "PURCHASE_CHANGE_CANCEL_APPROVAL",
        PermissionDef {
            permission: &PURCHASE_CHANGE_CANCEL_APPROVAL,
            group: groups::PURCHASE_ORDER,
            description: "撤回采购变更审批",
        },
    ),
    (
        "PURCHASE_CHANGE_LIST",
        PermissionDef {
            permission: &PURCHASE_CHANGE_LIST,
            group: groups::PURCHASE_ORDER,
            description: "查询采购变更单列表",
        },
    ),
    (
        "PURCHASE_CHANGE_DETAIL",
        PermissionDef {
            permission: &PURCHASE_CHANGE_DETAIL,
            group: groups::PURCHASE_ORDER,
            description: "查询采购变更单详情",
        },
    ),
];
