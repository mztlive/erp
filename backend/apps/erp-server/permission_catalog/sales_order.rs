//! sales_order 的接口权限目录。
use super::{PermissionDef, groups};

/// 核对后补销售合同。
pub const CONTRACT_CHECK_SALES_ORDER_CONTRACT_CHECK: PermissionDef =
    HANDOVER.described(groups::SALES_ORDER, "核对后补销售合同");

/// 下载销售单建单凭证。
pub const EVIDENCE_SALES_ORDER_EVIDENCE_DOWNLOAD: PermissionDef =
    DETAIL.described(groups::SALES_ORDER, "下载销售单建单凭证");

/// 查询销售单列表。
pub const LIST: PermissionDef =
    PermissionDef {
        code: "sales_order:list", group: groups::SALES_ORDER, description: "查询销售单列表"
    };

/// 创建销售单。
pub const CREATE: PermissionDef =
    PermissionDef { code: "sales_order:create", group: groups::SALES_ORDER, description: "创建销售单" };

/// 查询销售单详情。
pub const DETAIL: PermissionDef =
    PermissionDef {
        code: "sales_order:detail", group: groups::SALES_ORDER, description: "查询销售单详情"
    };

/// 保存销售单草稿。
pub const SAVE_WORKING_COPY: PermissionDef = HANDOVER.described(groups::SALES_ORDER, "保存销售单草稿");

/// 提交销售单。
pub const SUBMIT: PermissionDef =
    PermissionDef { code: "sales_order:submit", group: groups::SALES_ORDER, description: "提交销售单" };

/// 撤回销售单审批。
pub const CANCEL_APPROVAL: PermissionDef = PermissionDef {
    code: "sales_order:cancel_approval",
    group: groups::SALES_ORDER,
    description: "撤回销售单审批",
};

/// 作废销售单草稿。
pub const VOID: PermissionDef =
    PermissionDef {
        code: "sales_order:delete", group: groups::SALES_ORDER, description: "作废销售单草稿"
    };

/// 交接销售单责任。
pub const HANDOVER: PermissionDef =
    PermissionDef {
        code: "sales_order:update", group: groups::SALES_ORDER, description: "交接销售单责任"
    };

/// 查询销售交接待选目标。
pub const HANDOVER_CANDIDATES: PermissionDef = LIST.described(groups::SALES_ORDER, "查询销售交接待选目标");

/// 补录销售单合同。
pub const BIND_CONTRACT: PermissionDef = HANDOVER.described(groups::SALES_ORDER, "补录销售单合同");

/// 读取销售方案开单内容。
pub const SELECTION_SELECTION_BASIS: PermissionDef =
    CREATE.described(groups::SALES_ORDER, "读取销售方案开单内容");

/// 查询销售单详情。
pub const TIMELINE_SALES_TIMELINE: PermissionDef = DETAIL;

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("CONTRACT_CHECK_SALES_ORDER_CONTRACT_CHECK", CONTRACT_CHECK_SALES_ORDER_CONTRACT_CHECK),
    ("EVIDENCE_SALES_ORDER_EVIDENCE_DOWNLOAD", EVIDENCE_SALES_ORDER_EVIDENCE_DOWNLOAD),
    ("LIST", LIST),
    ("CREATE", CREATE),
    ("DETAIL", DETAIL),
    ("SAVE_WORKING_COPY", SAVE_WORKING_COPY),
    ("SUBMIT", SUBMIT),
    ("CANCEL_APPROVAL", CANCEL_APPROVAL),
    ("VOID", VOID),
    ("HANDOVER", HANDOVER),
    ("HANDOVER_CANDIDATES", HANDOVER_CANDIDATES),
    ("BIND_CONTRACT", BIND_CONTRACT),
    ("SELECTION_SELECTION_BASIS", SELECTION_SELECTION_BASIS),
    ("TIMELINE_SALES_TIMELINE", TIMELINE_SALES_TIMELINE),
];
