//! 权限展示分组；名称与说明在此统一定义。
use crate::PermissionGroupDef;

/// 权限与审计。
pub const ACCESS_CONTROL: PermissionGroupDef = PermissionGroupDef {
    name: "权限与审计",
    description: "权限目录、数据范围、用户授权与审计查询",
};

/// 人员目录。
pub const SALES_PERSON: PermissionGroupDef =
    PermissionGroupDef { name: "人员目录", description: "独立人员查询与资格维护" };

/// 授权文件。
pub const AUTHORIZATION_POLICY: PermissionGroupDef =
    PermissionGroupDef { name: "授权文件", description: "声明式授权配置" };

/// 账号管理。
pub const ADMIN: PermissionGroupDef =
    PermissionGroupDef { name: "账号管理", description: "管理员账号的增删改查" };

/// 系统审计。
pub const AUDIT_LOG: PermissionGroupDef =
    PermissionGroupDef { name: "系统审计", description: "系统操作日志" };

/// 采购责任管理。
pub const PROCUREMENT_RESPONSIBILITY: PermissionGroupDef = PermissionGroupDef {
    name: "采购责任管理",
    description: "维护采购责任规则并预览逐行负责人",
};

/// 角色管理。
pub const ROLE: PermissionGroupDef =
    PermissionGroupDef { name: "角色管理", description: "系统角色和权限配置" };

/// 审批实例。
pub const APPROVAL_INSTANCE: PermissionGroupDef =
    PermissionGroupDef { name: "审批实例", description: "审批运行、决定、恢复与取消" };

/// 审批流程。
pub const APPROVAL_PROCESS: PermissionGroupDef =
    PermissionGroupDef { name: "审批流程", description: "固定单据类型审批流程定义管理" };

/// 批量任务。
pub const BULK_JOB: PermissionGroupDef =
    PermissionGroupDef { name: "批量任务", description: "批量选择快照与后台任务中心" };

/// 商品与仓库。
pub const CATALOG: PermissionGroupDef = PermissionGroupDef {
    name: "商品与仓库",
    description: "公司商品池、商品、类目、供应商与仓库基础资料",
};

/// 合同。
pub const CONTRACT: PermissionGroupDef =
    PermissionGroupDef { name: "合同", description: "合同 PDF 档案管理" };

/// 合同模板。
pub const CONTRACT_TEMPLATE: PermissionGroupDef =
    PermissionGroupDef { name: "合同模板", description: "Word 模板与合同领号" };

/// 合同申请。
pub const CONTRACT_APPLICATION: PermissionGroupDef =
    PermissionGroupDef { name: "合同申请", description: "本人合同领号及下载" };

/// 实际经营盈亏。
pub const COST: PermissionGroupDef = PermissionGroupDef {
    name: "实际经营盈亏",
    description: "成本事实与成本分配管理（W16）",
};

/// 历史归属目录。
pub const COST_ENTRY: PermissionGroupDef =
    PermissionGroupDef { name: "历史归属目录", description: "冻结归属查询" };

/// 客户。
pub const CUSTOMER: PermissionGroupDef =
    PermissionGroupDef { name: "客户", description: "客户角色与归属管理" };

/// 客户经营质量。
pub const CUSTOMER_QUALITY: PermissionGroupDef = PermissionGroupDef {
    name: "客户经营质量",
    description: "当前负责与历史贡献双口径（M10）",
};

/// 演示主数据。
pub const DEMO_MASTER_DATA: PermissionGroupDef = PermissionGroupDef {
    name: "演示主数据",
    description: "生成演示资料；清空数据库时仅保留 admin",
};

/// 单据注册。
pub const DOCUMENT_REGISTRY: PermissionGroupDef =
    PermissionGroupDef { name: "单据注册", description: "跨域单据稳定注册表与工作流动作" };

/// 文件资产。
pub const FILE_ASSET: PermissionGroupDef =
    PermissionGroupDef { name: "文件资产", description: "文件元数据、单据附件与安全检查" };

/// 统一待办。
pub const FINANCIAL_FILES: PermissionGroupDef =
    PermissionGroupDef { name: "统一待办", description: "待办队列与责任处理" };

/// 客户往来。
pub const INVOICE: PermissionGroupDef =
    PermissionGroupDef { name: "客户往来", description: "应收台账、回款与销项发票管理" };

/// 销售单。
pub const SALES_ORDER: PermissionGroupDef =
    PermissionGroupDef { name: "销售单", description: "销售单（W05）管理" };

/// 履约。
pub const FULFILLMENT: PermissionGroupDef = PermissionGroupDef {
    name: "履约",
    description: "采购入库、发货、交付、服务与客户验收管理",
};

/// 集成治理。
pub const INTEGRATION_OPS: PermissionGroupDef = PermissionGroupDef {
    name: "集成治理",
    description: "入站消息、错误任务与对账差异（W29）",
};

/// 库存。
pub const INVENTORY: PermissionGroupDef =
    PermissionGroupDef { name: "库存", description: "库存台账与库存调整管理" };

/// 导入与期初。
pub const LEGACY_IMPORT: PermissionGroupDef = PermissionGroupDef {
    name: "导入与期初",
    description: "旧数据导入批次、导入行与业务确认事实管理",
};

/// 组织管理。
pub const ORGANIZATION: PermissionGroupDef =
    PermissionGroupDef { name: "组织管理", description: "内部组织与成员" };

/// 公司主体。
pub const PARTY: PermissionGroupDef =
    PermissionGroupDef { name: "公司主体", description: "我方公司及导入别名维护" };

/// 主体。
pub const PARTY_2: PermissionGroupDef =
    PermissionGroupDef { name: "主体", description: "企业主体稳定身份与历史名称资料管理" };

/// 供应商往来。
pub const PAYABLE: PermissionGroupDef = PermissionGroupDef {
    name: "供应商往来",
    description: "应付台账、付款单与进项发票登记管理（W12）",
};

/// 采购单。
pub const PURCHASE_ORDER: PermissionGroupDef =
    PermissionGroupDef { name: "采购单", description: "采购单、采购提交与采购变更管理" };

/// 采购付款申请。
pub const PURCHASE_PAYMENT_REQUEST: PermissionGroupDef = PermissionGroupDef {
    name: "采购付款申请",
    description: "采购申请、财务审核和批准额度内付款",
};

/// 开票申请。
pub const RECEIVABLE: PermissionGroupDef =
    PermissionGroupDef { name: "开票申请", description: "开票申请与授权管理" };

/// 退货退款。
pub const RETURNS: PermissionGroupDef = PermissionGroupDef {
    name: "退货退款",
    description: "销售退货/拒收、采购退货与退款冲正管理（W05/W09/W11/W12）",
};

/// 销售询价。
pub const SALES_INQUIRY: PermissionGroupDef =
    PermissionGroupDef { name: "销售询价", description: "询价需求与采购反馈协作" };

/// 采购询价。
pub const PROCUREMENT_INQUIRY: PermissionGroupDef =
    PermissionGroupDef { name: "采购询价", description: "询价需求与采购反馈协作" };

/// 成本费用。
pub const COST_ENTRY_2: PermissionGroupDef =
    PermissionGroupDef { name: "成本费用", description: "成本事实与费用登记" };

/// 销售复核。
pub const SALES_REVIEW: PermissionGroupDef = PermissionGroupDef {
    name: "销售复核",
    description: "销售审批与采购二次确认（W05/W07）管理",
};

/// 销售选品。
pub const SALES_SELECTION: PermissionGroupDef =
    PermissionGroupDef { name: "销售选品", description: "选品册与销售方案" };

/// 来源注册。
pub const SOURCE_REGISTRY: PermissionGroupDef =
    PermissionGroupDef { name: "来源注册", description: "来源系统与外部身份映射管理" };

/// 供应商。
pub const SUPPLIER: PermissionGroupDef = PermissionGroupDef {
    name: "供应商",
    description: "供应商角色、商务结算版本、能力与资质管理",
};

/// API 供应商连接。
pub const SUPPLIER_API: PermissionGroupDef = PermissionGroupDef {
    name: "API 供应商连接",
    description: "供应商 API 连接与能力治理（W20）",
};

/// 供应商预存款。
pub const SUPPLIER_DEPOSIT: PermissionGroupDef = PermissionGroupDef {
    name: "供应商预存款",
    description: "真实充值、逐单占用与资金核实",
};

/// 供应商订单。
pub const SUPPLIER_FULFILLMENT: PermissionGroupDef = PermissionGroupDef {
    name: "供应商订单",
    description: "供应商履约订单、动作与退款事实管理",
};

/// 供应商供给。
pub const SUPPLIER_OFFERING: PermissionGroupDef = PermissionGroupDef {
    name: "供应商供给",
    description: "维护公司 SKU 的供应商供给、价格条款和可供状态",
};

/// 供应商门户管理。
pub const SUPPLIER_PORTAL: PermissionGroupDef = PermissionGroupDef {
    name: "供应商门户管理",
    description: "管理外部账号、定向报价与专项申请",
};

/// 供应商结算。
pub const SUPPLIER_SETTLEMENT: PermissionGroupDef = PermissionGroupDef {
    name: "供应商结算",
    description: "供应商周期结算单、明细与差异管理",
};

/// 对象目录。
pub const WAREHOUSE: PermissionGroupDef =
    PermissionGroupDef { name: "对象目录", description: "按独立范围查询业务对象" };

/// 财务责任管理。
pub const WORK_ITEM: PermissionGroupDef = PermissionGroupDef {
    name: "财务责任管理",
    description: "维护付款与销项开票的具体负责人规则",
};

/// 财务整账。
pub const FINANCE_LEDGER: PermissionGroupDef = PermissionGroupDef {
    name: "财务整账",
    description: "完整财务台账及无来源记录的读取资格；仍检查业务读取动作和来源边界",
};
