//! sales_order 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 核对后补销售合同。
pub const CONTRACT_CHECK_SALES_ORDER_CONTRACT_CHECK: Permission = HANDOVER;

/// 下载销售单建单凭证。
pub const EVIDENCE_SALES_ORDER_EVIDENCE_DOWNLOAD: Permission = DETAIL;

/// 查询销售单列表。
pub const LIST: Permission = Permission::new(resources::SALES_ORDER, "list");

/// 创建销售单。
pub const CREATE: Permission = Permission::new(resources::SALES_ORDER, "create");

/// 查询销售单详情。
pub const DETAIL: Permission = Permission::new(resources::SALES_ORDER, "detail");

/// 保存销售单草稿。
pub const SAVE_WORKING_COPY: Permission = HANDOVER;

/// 提交销售单。
pub const SUBMIT: Permission = Permission::new(resources::SALES_ORDER, "submit");

/// 撤回销售单审批。
pub const CANCEL_APPROVAL: Permission = Permission::new(resources::SALES_ORDER, "cancel_approval");

/// 作废销售单草稿。
pub const VOID: Permission = Permission::new(resources::SALES_ORDER, "delete");

/// 交接销售单责任。
pub const HANDOVER: Permission = Permission::new(resources::SALES_ORDER, "update");

/// 查询销售交接待选目标。
pub const HANDOVER_CANDIDATES: Permission = LIST;

/// 补录销售单合同。
pub const BIND_CONTRACT: Permission = HANDOVER;

/// 读取销售方案开单内容。
pub const SELECTION_SELECTION_BASIS: Permission = CREATE;

/// 查询销售单详情。
pub const TIMELINE_SALES_TIMELINE: Permission = DETAIL;

/// 补充或调整未发货部分的收货安排。
pub const DELIVERY_ARRANGEMENT: Permission = HANDOVER;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("LIST", PermissionDef { permission: &LIST, group: groups::SALES_ORDER, description: "查询销售单列表" }),
    ("CREATE", PermissionDef { permission: &CREATE, group: groups::SALES_ORDER, description: "创建销售单" }),
    (
        "DETAIL",
        PermissionDef {
            permission: &DETAIL, group: groups::SALES_ORDER, description: "查询销售单详情"
        },
    ),
    ("SUBMIT", PermissionDef { permission: &SUBMIT, group: groups::SALES_ORDER, description: "提交销售单" }),
    (
        "CANCEL_APPROVAL",
        PermissionDef {
            permission: &CANCEL_APPROVAL,
            group: groups::SALES_ORDER,
            description: "撤回销售单审批",
        },
    ),
    ("VOID", PermissionDef { permission: &VOID, group: groups::SALES_ORDER, description: "作废销售单草稿" }),
    (
        "HANDOVER",
        PermissionDef {
            permission: &HANDOVER, group: groups::SALES_ORDER, description: "交接销售单责任"
        },
    ),
];
