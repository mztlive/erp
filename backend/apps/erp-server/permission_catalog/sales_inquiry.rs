//! sales_inquiry 的接口权限目录。
use super::{PermissionDef, cost, groups, purchase_order};

/// 创建销售询价。
pub const COMMANDS_SALES_CREATE: PermissionDef =
    PermissionDef {
        code: "sales_inquiry:create", group: groups::SALES_INQUIRY, description: "创建销售询价"
    };

/// 保存询价需求。
pub const COMMANDS_SALES_SAVE: PermissionDef =
    PermissionDef {
        code: "sales_inquiry:update", group: groups::SALES_INQUIRY, description: "保存询价需求"
    };

/// 提交询价需求。
pub const COMMANDS_SALES_SUBMIT: PermissionDef =
    PermissionDef {
        code: "sales_inquiry:submit", group: groups::SALES_INQUIRY, description: "提交询价需求"
    };

/// 接受询价反馈。
pub const COMMANDS_SALES_ACCEPT: PermissionDef =
    PermissionDef {
        code: "sales_inquiry:accept", group: groups::SALES_INQUIRY, description: "接受询价反馈"
    };

/// 要求重新询价。
pub const COMMANDS_SALES_REINQUIRE: PermissionDef = PermissionDef {
    code: "sales_inquiry:reinquire",
    group: groups::SALES_INQUIRY,
    description: "要求重新询价",
};

/// 关闭销售询价。
pub const COMMANDS_SALES_CLOSE: PermissionDef =
    PermissionDef {
        code: "sales_inquiry:close", group: groups::SALES_INQUIRY, description: "关闭销售询价"
    };

/// 要求补充询价需求。
pub const COMMANDS_PROCUREMENT_SUPPLEMENT: PermissionDef = PermissionDef {
    code: "procurement_inquiry:supplement",
    group: groups::PROCUREMENT_INQUIRY,
    description: "要求补充询价需求",
};

/// 改派询价责任。
pub const COMMANDS_PROCUREMENT_REASSIGN: PermissionDef = PermissionDef {
    code: "procurement_inquiry:reassign",
    group: groups::PROCUREMENT_INQUIRY,
    description: "改派询价责任",
};

/// 保存询价反馈草稿。
pub const COMMANDS_PROCUREMENT_SAVE_FEEDBACK: PermissionDef = PermissionDef {
    code: "procurement_inquiry:save_feedback",
    group: groups::PROCUREMENT_INQUIRY,
    description: "保存询价反馈草稿",
};

/// 发布询价完整反馈。
pub const COMMANDS_PROCUREMENT_PUBLISH: PermissionDef =
    COMMANDS_PROCUREMENT_RENEW.described(groups::PROCUREMENT_INQUIRY, "发布询价完整反馈");

/// 发起新一轮询价报价。
pub const COMMANDS_PROCUREMENT_RENEW: PermissionDef = PermissionDef {
    code: "procurement_inquiry:publish",
    group: groups::PROCUREMENT_INQUIRY,
    description: "发起新一轮询价报价",
};

/// 读取财务登记询价费用候选。
pub const CONVERSION_FINANCIAL_INQUIRY_CANDIDATES: PermissionDef =
    cost::ENTRY_CREATE.described(groups::COST_ENTRY_2, "读取财务登记询价费用候选");

/// 读取销售来源询价费用候选。
pub const CONVERSION_EXPENSE_INQUIRY_CANDIDATES: PermissionDef =
    purchase_order::CREATE.described(groups::PURCHASE_ORDER, "读取销售来源询价费用候选");

/// 读取询价转销售依据。
pub const CONVERSION_SALES_BASIS: PermissionDef = PermissionDef {
    code: "sales_inquiry:convert_sales",
    group: groups::SALES_INQUIRY,
    description: "读取询价转销售依据",
};

/// 查询询价费用归属销售。
pub const CONVERSION_FEE_SALES: PermissionDef =
    QUERY_SALES_DETAIL.described(groups::SALES_INQUIRY, "查询询价费用归属销售");

/// 分配询价费用归属。
pub const CONVERSION_ALLOCATE_FEES: PermissionDef =
    CONVERSION_SALES_BASIS.described(groups::SALES_INQUIRY, "分配询价费用归属");

/// 预览客户选品导入。
pub const IMPORT_SALES_IMPORT_PREVIEW: PermissionDef = PermissionDef {
    code: "sales_inquiry:import",
    group: groups::SALES_INQUIRY,
    description: "预览客户选品导入",
};

/// 确认客户选品导入。
pub const IMPORT_SALES_IMPORT_CONFIRM: PermissionDef =
    IMPORT_SALES_IMPORT_PREVIEW.described(groups::SALES_INQUIRY, "确认客户选品导入");

/// 预览导入新建询价。
pub const IMPORT_CREATION_CREATION_PREVIEW: PermissionDef =
    IMPORT_SALES_IMPORT_PREVIEW.described(groups::SALES_INQUIRY, "预览导入新建询价");

/// 导入新建询价。
pub const IMPORT_CREATION_CREATION_CONFIRM: PermissionDef =
    IMPORT_SALES_IMPORT_PREVIEW.described(groups::SALES_INQUIRY, "导入新建询价");

/// 导入供应商回填报价。
pub const INTERNAL_IMPORT_QUOTES: PermissionDef =
    INTERNAL_FEE_BASES.described(groups::PROCUREMENT_INQUIRY, "导入供应商回填报价");

/// 保存询价内部报价。
pub const INTERNAL_SAVE_QUOTE: PermissionDef =
    INTERNAL_FEE_BASES.described(groups::PROCUREMENT_INQUIRY, "保存询价内部报价");

/// 保存询价费用内部依据。
pub const INTERNAL_SAVE_FEE_BASIS: PermissionDef =
    INTERNAL_FEE_BASES.described(groups::PROCUREMENT_INQUIRY, "保存询价费用内部依据");

/// 读取询价费用内部依据。
pub const INTERNAL_FEE_BASES: PermissionDef = PermissionDef {
    code: "procurement_inquiry:internal_quote",
    group: groups::PROCUREMENT_INQUIRY,
    description: "读取询价费用内部依据",
};

/// 下载当前责任费用依据附件。
pub const INTERNAL_FEE_BASIS_ATTACHMENT: PermissionDef =
    INTERNAL_FEE_BASES.described(groups::PROCUREMENT_INQUIRY, "下载当前责任费用依据附件");

/// 查询销售询价列表。
pub const QUERY_SALES_LIST: PermissionDef = PermissionDef {
    code: "sales_inquiry:list",
    group: groups::SALES_INQUIRY,
    description: "查询销售询价列表",
};

/// 读取销售询价详情。
pub const QUERY_SALES_DETAIL: PermissionDef = PermissionDef {
    code: "sales_inquiry:detail",
    group: groups::SALES_INQUIRY,
    description: "读取销售询价详情",
};

/// 查询当前采购询价任务。
pub const QUERY_PROCUREMENT_LIST: PermissionDef = PermissionDef {
    code: "procurement_inquiry:list",
    group: groups::PROCUREMENT_INQUIRY,
    description: "查询当前采购询价任务",
};

/// 读取当前采购询价责任资料。
pub const QUERY_PROCUREMENT_DETAIL: PermissionDef = PermissionDef {
    code: "procurement_inquiry:detail",
    group: groups::PROCUREMENT_INQUIRY,
    description: "读取当前采购询价责任资料",
};

/// 读取当前责任行内部报价。
pub const QUERY_PROCUREMENT_INTERNAL_QUOTES: PermissionDef =
    INTERNAL_FEE_BASES.described(groups::PROCUREMENT_INQUIRY, "读取当前责任行内部报价");

/// 重验客户模板商品资格。
pub const QUERY_SALES_TEMPLATE_PRODUCTS: PermissionDef =
    COMMANDS_SALES_CREATE.described(groups::SALES_INQUIRY, "重验客户模板商品资格");

/// 核对销售询价命令结果。
pub const RECOVERY_SALES_RECOVER: PermissionDef = PermissionDef {
    code: "sales_inquiry:recover",
    group: groups::SALES_INQUIRY,
    description: "核对销售询价命令结果",
};

/// 核对新建询价命令结果。
pub const RECOVERY_SALES_CREATE_RECOVER: PermissionDef =
    RECOVERY_SALES_RECOVER.described(groups::SALES_INQUIRY, "核对新建询价命令结果");

/// 核对采购询价命令结果。
pub const RECOVERY_PROCUREMENT_RECOVER: PermissionDef = PermissionDef {
    code: "procurement_inquiry:recover",
    group: groups::PROCUREMENT_INQUIRY,
    description: "核对采购询价命令结果",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("COMMANDS_SALES_CREATE", COMMANDS_SALES_CREATE),
    ("COMMANDS_SALES_SAVE", COMMANDS_SALES_SAVE),
    ("COMMANDS_SALES_SUBMIT", COMMANDS_SALES_SUBMIT),
    ("COMMANDS_SALES_ACCEPT", COMMANDS_SALES_ACCEPT),
    ("COMMANDS_SALES_REINQUIRE", COMMANDS_SALES_REINQUIRE),
    ("COMMANDS_SALES_CLOSE", COMMANDS_SALES_CLOSE),
    ("COMMANDS_PROCUREMENT_SUPPLEMENT", COMMANDS_PROCUREMENT_SUPPLEMENT),
    ("COMMANDS_PROCUREMENT_REASSIGN", COMMANDS_PROCUREMENT_REASSIGN),
    ("COMMANDS_PROCUREMENT_SAVE_FEEDBACK", COMMANDS_PROCUREMENT_SAVE_FEEDBACK),
    ("COMMANDS_PROCUREMENT_PUBLISH", COMMANDS_PROCUREMENT_PUBLISH),
    ("COMMANDS_PROCUREMENT_RENEW", COMMANDS_PROCUREMENT_RENEW),
    ("CONVERSION_FINANCIAL_INQUIRY_CANDIDATES", CONVERSION_FINANCIAL_INQUIRY_CANDIDATES),
    ("CONVERSION_EXPENSE_INQUIRY_CANDIDATES", CONVERSION_EXPENSE_INQUIRY_CANDIDATES),
    ("CONVERSION_SALES_BASIS", CONVERSION_SALES_BASIS),
    ("CONVERSION_FEE_SALES", CONVERSION_FEE_SALES),
    ("CONVERSION_ALLOCATE_FEES", CONVERSION_ALLOCATE_FEES),
    ("IMPORT_SALES_IMPORT_PREVIEW", IMPORT_SALES_IMPORT_PREVIEW),
    ("IMPORT_SALES_IMPORT_CONFIRM", IMPORT_SALES_IMPORT_CONFIRM),
    ("IMPORT_CREATION_CREATION_PREVIEW", IMPORT_CREATION_CREATION_PREVIEW),
    ("IMPORT_CREATION_CREATION_CONFIRM", IMPORT_CREATION_CREATION_CONFIRM),
    ("INTERNAL_IMPORT_QUOTES", INTERNAL_IMPORT_QUOTES),
    ("INTERNAL_SAVE_QUOTE", INTERNAL_SAVE_QUOTE),
    ("INTERNAL_SAVE_FEE_BASIS", INTERNAL_SAVE_FEE_BASIS),
    ("INTERNAL_FEE_BASES", INTERNAL_FEE_BASES),
    ("INTERNAL_FEE_BASIS_ATTACHMENT", INTERNAL_FEE_BASIS_ATTACHMENT),
    ("QUERY_SALES_LIST", QUERY_SALES_LIST),
    ("QUERY_SALES_DETAIL", QUERY_SALES_DETAIL),
    ("QUERY_PROCUREMENT_LIST", QUERY_PROCUREMENT_LIST),
    ("QUERY_PROCUREMENT_DETAIL", QUERY_PROCUREMENT_DETAIL),
    ("QUERY_PROCUREMENT_INTERNAL_QUOTES", QUERY_PROCUREMENT_INTERNAL_QUOTES),
    ("QUERY_SALES_TEMPLATE_PRODUCTS", QUERY_SALES_TEMPLATE_PRODUCTS),
    ("RECOVERY_SALES_RECOVER", RECOVERY_SALES_RECOVER),
    ("RECOVERY_SALES_CREATE_RECOVER", RECOVERY_SALES_CREATE_RECOVER),
    ("RECOVERY_PROCUREMENT_RECOVER", RECOVERY_PROCUREMENT_RECOVER),
];
