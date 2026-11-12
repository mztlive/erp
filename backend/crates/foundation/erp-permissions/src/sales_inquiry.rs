//! sales_inquiry 的内置权限。
use crate::{Permission, PermissionDef, cost, groups, purchase_order, resources};

/// 创建销售询价。
pub const COMMANDS_SALES_CREATE: Permission = Permission::new(resources::SALES_INQUIRY, "create");

/// 保存询价需求。
pub const COMMANDS_SALES_SAVE: Permission = Permission::new(resources::SALES_INQUIRY, "update");

/// 提交询价需求。
pub const COMMANDS_SALES_SUBMIT: Permission = Permission::new(resources::SALES_INQUIRY, "submit");

/// 接受询价反馈。
pub const COMMANDS_SALES_ACCEPT: Permission = Permission::new(resources::SALES_INQUIRY, "accept");

/// 要求重新询价。
pub const COMMANDS_SALES_REINQUIRE: Permission = Permission::new(resources::SALES_INQUIRY, "reinquire");

/// 关闭销售询价。
pub const COMMANDS_SALES_CLOSE: Permission = Permission::new(resources::SALES_INQUIRY, "close");

/// 要求补充询价需求。
pub const COMMANDS_PROCUREMENT_SUPPLEMENT: Permission =
    Permission::new(resources::PROCUREMENT_INQUIRY, "supplement");

/// 改派询价责任。
pub const COMMANDS_PROCUREMENT_REASSIGN: Permission =
    Permission::new(resources::PROCUREMENT_INQUIRY, "reassign");

/// 保存询价反馈草稿。
pub const COMMANDS_PROCUREMENT_SAVE_FEEDBACK: Permission =
    Permission::new(resources::PROCUREMENT_INQUIRY, "save_feedback");

/// 发布询价完整反馈。
pub const COMMANDS_PROCUREMENT_PUBLISH: Permission = COMMANDS_PROCUREMENT_RENEW;

/// 发起新一轮询价报价。
pub const COMMANDS_PROCUREMENT_RENEW: Permission = Permission::new(resources::PROCUREMENT_INQUIRY, "publish");

/// 读取财务登记询价费用候选。
pub const CONVERSION_FINANCIAL_INQUIRY_CANDIDATES: Permission = cost::ENTRY_CREATE;

/// 读取销售来源询价费用候选。
pub const CONVERSION_EXPENSE_INQUIRY_CANDIDATES: Permission = purchase_order::CREATE;

/// 读取询价转销售依据。
pub const CONVERSION_SALES_BASIS: Permission = Permission::new(resources::SALES_INQUIRY, "convert_sales");

/// 查询询价费用归属销售。
pub const CONVERSION_FEE_SALES: Permission = QUERY_SALES_DETAIL;

/// 分配询价费用归属。
pub const CONVERSION_ALLOCATE_FEES: Permission = CONVERSION_SALES_BASIS;

/// 预览客户选品导入。
pub const IMPORT_SALES_IMPORT_PREVIEW: Permission = Permission::new(resources::SALES_INQUIRY, "import");

/// 确认客户选品导入。
pub const IMPORT_SALES_IMPORT_CONFIRM: Permission = IMPORT_SALES_IMPORT_PREVIEW;

/// 预览导入新建询价。
pub const IMPORT_CREATION_CREATION_PREVIEW: Permission = IMPORT_SALES_IMPORT_PREVIEW;

/// 导入新建询价。
pub const IMPORT_CREATION_CREATION_CONFIRM: Permission = IMPORT_SALES_IMPORT_PREVIEW;

/// 导入供应商回填报价。
pub const INTERNAL_IMPORT_QUOTES: Permission = INTERNAL_FEE_BASES;

/// 保存询价内部报价。
pub const INTERNAL_SAVE_QUOTE: Permission = INTERNAL_FEE_BASES;

/// 保存询价费用内部依据。
pub const INTERNAL_SAVE_FEE_BASIS: Permission = INTERNAL_FEE_BASES;

/// 读取询价费用内部依据。
pub const INTERNAL_FEE_BASES: Permission = Permission::new(resources::PROCUREMENT_INQUIRY, "internal_quote");

/// 下载当前责任费用依据附件。
pub const INTERNAL_FEE_BASIS_ATTACHMENT: Permission = INTERNAL_FEE_BASES;

/// 查询销售询价列表。
pub const QUERY_SALES_LIST: Permission = Permission::new(resources::SALES_INQUIRY, "list");

/// 读取销售询价详情。
pub const QUERY_SALES_DETAIL: Permission = Permission::new(resources::SALES_INQUIRY, "detail");

/// 查询当前采购询价任务。
pub const QUERY_PROCUREMENT_LIST: Permission = Permission::new(resources::PROCUREMENT_INQUIRY, "list");

/// 读取当前采购询价责任资料。
pub const QUERY_PROCUREMENT_DETAIL: Permission = Permission::new(resources::PROCUREMENT_INQUIRY, "detail");

/// 读取当前责任行内部报价。
pub const QUERY_PROCUREMENT_INTERNAL_QUOTES: Permission = INTERNAL_FEE_BASES;

/// 重验客户模板商品资格。
pub const QUERY_SALES_TEMPLATE_PRODUCTS: Permission = COMMANDS_SALES_CREATE;

/// 核对销售询价命令结果。
pub const RECOVERY_SALES_RECOVER: Permission = Permission::new(resources::SALES_INQUIRY, "recover");

/// 核对新建询价命令结果。
pub const RECOVERY_SALES_CREATE_RECOVER: Permission = RECOVERY_SALES_RECOVER;

/// 核对采购询价命令结果。
pub const RECOVERY_PROCUREMENT_RECOVER: Permission =
    Permission::new(resources::PROCUREMENT_INQUIRY, "recover");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "COMMANDS_SALES_CREATE",
        PermissionDef {
            permission: &COMMANDS_SALES_CREATE,
            group: groups::SALES_INQUIRY,
            description: "创建销售询价",
        },
    ),
    (
        "COMMANDS_SALES_SAVE",
        PermissionDef {
            permission: &COMMANDS_SALES_SAVE,
            group: groups::SALES_INQUIRY,
            description: "保存询价需求",
        },
    ),
    (
        "COMMANDS_SALES_SUBMIT",
        PermissionDef {
            permission: &COMMANDS_SALES_SUBMIT,
            group: groups::SALES_INQUIRY,
            description: "提交询价需求",
        },
    ),
    (
        "COMMANDS_SALES_ACCEPT",
        PermissionDef {
            permission: &COMMANDS_SALES_ACCEPT,
            group: groups::SALES_INQUIRY,
            description: "接受询价反馈",
        },
    ),
    (
        "COMMANDS_SALES_REINQUIRE",
        PermissionDef {
            permission: &COMMANDS_SALES_REINQUIRE,
            group: groups::SALES_INQUIRY,
            description: "要求重新询价",
        },
    ),
    (
        "COMMANDS_SALES_CLOSE",
        PermissionDef {
            permission: &COMMANDS_SALES_CLOSE,
            group: groups::SALES_INQUIRY,
            description: "关闭销售询价",
        },
    ),
    (
        "COMMANDS_PROCUREMENT_SUPPLEMENT",
        PermissionDef {
            permission: &COMMANDS_PROCUREMENT_SUPPLEMENT,
            group: groups::PROCUREMENT_INQUIRY,
            description: "要求补充询价需求",
        },
    ),
    (
        "COMMANDS_PROCUREMENT_REASSIGN",
        PermissionDef {
            permission: &COMMANDS_PROCUREMENT_REASSIGN,
            group: groups::PROCUREMENT_INQUIRY,
            description: "改派询价责任",
        },
    ),
    (
        "COMMANDS_PROCUREMENT_SAVE_FEEDBACK",
        PermissionDef {
            permission: &COMMANDS_PROCUREMENT_SAVE_FEEDBACK,
            group: groups::PROCUREMENT_INQUIRY,
            description: "保存询价反馈草稿",
        },
    ),
    (
        "COMMANDS_PROCUREMENT_RENEW",
        PermissionDef {
            permission: &COMMANDS_PROCUREMENT_RENEW,
            group: groups::PROCUREMENT_INQUIRY,
            description: "发起新一轮询价报价",
        },
    ),
    (
        "CONVERSION_SALES_BASIS",
        PermissionDef {
            permission: &CONVERSION_SALES_BASIS,
            group: groups::SALES_INQUIRY,
            description: "读取询价转销售依据",
        },
    ),
    (
        "IMPORT_SALES_IMPORT_PREVIEW",
        PermissionDef {
            permission: &IMPORT_SALES_IMPORT_PREVIEW,
            group: groups::SALES_INQUIRY,
            description: "预览客户选品导入",
        },
    ),
    (
        "INTERNAL_FEE_BASES",
        PermissionDef {
            permission: &INTERNAL_FEE_BASES,
            group: groups::PROCUREMENT_INQUIRY,
            description: "读取询价费用内部依据",
        },
    ),
    (
        "QUERY_SALES_LIST",
        PermissionDef {
            permission: &QUERY_SALES_LIST,
            group: groups::SALES_INQUIRY,
            description: "查询销售询价列表",
        },
    ),
    (
        "QUERY_SALES_DETAIL",
        PermissionDef {
            permission: &QUERY_SALES_DETAIL,
            group: groups::SALES_INQUIRY,
            description: "读取销售询价详情",
        },
    ),
    (
        "QUERY_PROCUREMENT_LIST",
        PermissionDef {
            permission: &QUERY_PROCUREMENT_LIST,
            group: groups::PROCUREMENT_INQUIRY,
            description: "查询当前采购询价任务",
        },
    ),
    (
        "QUERY_PROCUREMENT_DETAIL",
        PermissionDef {
            permission: &QUERY_PROCUREMENT_DETAIL,
            group: groups::PROCUREMENT_INQUIRY,
            description: "读取当前采购询价责任资料",
        },
    ),
    (
        "RECOVERY_SALES_RECOVER",
        PermissionDef {
            permission: &RECOVERY_SALES_RECOVER,
            group: groups::SALES_INQUIRY,
            description: "核对销售询价命令结果",
        },
    ),
    (
        "RECOVERY_PROCUREMENT_RECOVER",
        PermissionDef {
            permission: &RECOVERY_PROCUREMENT_RECOVER,
            group: groups::PROCUREMENT_INQUIRY,
            description: "核对采购询价命令结果",
        },
    ),
];
