//! 跨 crate 共享的内部授权入口；不作为可分配的菜单功能展示。
//! 单流程的派生动作优先复用主权限或通过 `AccessTarget` 表达目标形态。
use crate::Permission;

/// 按持久化任务种类核验完整办理资格；沿用工作项详情与业务动作资格。
pub const WORK_ITEM_EXECUTE: Permission = Permission::new("work_item", "execute");

/// 按精确真实任务的种类和来源检查接收者资格，不要求接收者已经持有该任务。
pub const WORK_ITEM_RECIPIENT: Permission = Permission::new("work_item", "recipient");

/// 在任务形成前按服务端确认的种类及来源类型检查完整执行功能资格。
pub const WORK_ITEM_EXECUTION_QUALIFICATION: Permission =
    Permission::new("work_item", "execution_qualification");

/// 合同识别任务的功能入口；归档时另行检查真实合同创建候选。
pub const CONTRACT_IMPORT: Permission = Permission::new("contract_import", "operate");

/// 合同销售共享沿用合同修改范围，并独立核对客户负责人分配资格。
pub const CONTRACT_SALES_ACCESS: Permission = Permission::new("contract", "sales_access_update");

/// 受保护内部对象的固定授权入口。
pub const CATALOG_PREVIEW_DETAIL: Permission = Permission::new("supplier_catalog_adoption_preview", "detail");

/// 选品及其基础单位补齐使用既有选品入口和成本读取的完整资格。
pub const CATALOG_ADOPTION: Permission = Permission::new("supplier_catalog_adoption", "operate");

/// 受保护内部对象的固定授权入口。
pub const PORTAL_MATERIAL_READ: Permission = Permission::new("supplier_portal_material", "read");

/// 受保护内部对象的固定授权入口。
pub const PORTAL_MATERIAL_READ_OFFERING: Permission =
    Permission::new("supplier_portal_material", "read_offering_update");

/// 受保护内部对象的固定授权入口。
pub const PORTAL_MATERIAL_REVIEW_COOPERATION: Permission =
    Permission::new("supplier_portal_material", "review_cooperation");

/// 受保护内部对象的固定授权入口。
pub const PORTAL_MATERIAL_REVIEW_OFFERING_CREATE: Permission =
    Permission::new("supplier_portal_material", "review_offering_create");

/// 受保护内部对象的固定授权入口。
pub const PORTAL_MATERIAL_REVIEW_OFFERING_UPDATE: Permission =
    Permission::new("supplier_portal_material", "review_offering_update");

/// 受保护内部对象的固定授权入口。
pub const PORTAL_MATERIAL_REVIEW_NEW_PRODUCT: Permission =
    Permission::new("supplier_portal_material", "review_new_product");

/// 既有内部流程的具体功能资格。
pub const SUPPLIER_OFFERING_COST_DETAIL: Permission = Permission::new("supplier_offering_cost", "detail");

/// 按供给维护范围更新可供状态，同时核验独立的可供更新功能资格。
pub const SUPPLIER_OFFERING_AVAILABILITY_UPDATE: Permission =
    Permission::new("supplier_offering", "availability_update");

/// 按当前供给创建资格读取原创建命令的真实供给结果。
pub const SUPPLIER_OFFERING_CREATED_RESULT_READ: Permission =
    Permission::new("supplier_offering", "created_result_read");

/// 按方案详情范围读取开单预填，同时核验销售建单功能资格。
pub const SALES_SELECTION_ORDER_BASIS: Permission =
    Permission::new("sales_selection_proposal", "order_basis_read");

/// 既有内部流程的具体功能资格。
pub const CONNECTION_UPDATE_BUSINESS_PROFILE: Permission =
    Permission::new("supplier_api_connection", "update_business_profile");

/// 既有内部流程的具体功能资格。
pub const CONNECTION_BIND_ENDPOINT: Permission =
    Permission::new("supplier_api_connection", "bind_endpoint_reference");

/// 既有内部流程的具体功能资格。
pub const CONNECTION_HEALTH_CHECK: Permission = Permission::new("supplier_api_connection", "health_check");

/// 既有内部流程的具体功能资格。
pub const CONNECTION_ENABLE: Permission = Permission::new("supplier_api_connection", "enable");

/// 既有内部流程的具体功能资格。
pub const CONNECTION_DISABLE: Permission = Permission::new("supplier_api_connection", "disable");

/// 在客户当前范围内选择客户开展新业务；沿用客户详情功能资格。
pub const CUSTOMER_SELECT: Permission = Permission::new("customer", "select_current");

/// 客户创建回执按原创建资格和当前真实归属重新读取结果。
pub const CUSTOMER_CREATED_RESULT_READ: Permission = Permission::new("customer", "created_result_read");

/// 查询客户资料命令回执的既有详情功能资格。
pub const CUSTOMER_PROFILE_COMMAND_READ: Permission = Permission::new("customer", "profile_command_read");

/// 查询供应商资料命令回执的既有详情功能资格。
pub const SUPPLIER_PROFILE_COMMAND_READ: Permission = Permission::new("supplier", "profile_command_read");

/// 开票金额候选读取沿用申请列表功能资格，不签发应收对象凭证。
pub const INVOICE_REQUEST_AMOUNTS_READ: Permission = Permission::new("sales_invoice_request", "amounts_read");

/// 沿用提交资格核验开票申请的真实销售来源。
pub const INVOICE_REQUEST_SUBMIT_SOURCE: Permission =
    Permission::new("sales_invoice_request", "submit_source");
/// 沿用撤回资格核验开票申请的真实销售来源。
pub const INVOICE_REQUEST_CANCEL_SOURCE: Permission =
    Permission::new("sales_invoice_request", "cancel_source");

/// 读取本人销售询价导入预览。
pub const SALES_INQUIRY_IMPORT_PREVIEW: Permission =
    Permission::new("sales_inquiry_import_preview", "detail");

/// 按拟交接归属检查接收人的当前销售单读取资格。
pub const SALES_ORDER_HANDOVER_RECIPIENT: Permission = Permission::new("sales_order", "accept_handover");

/// 按采购创建入口的合格角色查询可采购销售费用来源。
pub const PURCHASE_EXPENSE_SOURCE: Permission = Permission::new("sales_order", "purchase_expense_source");

/// 读取创建付款申请可选的真实采购订单。
pub const PAYMENT_REQUEST_CANDIDATES: Permission =
    Permission::new("purchase_order", "payment_request_candidate");
/// 按商品入口资格查看本人导入任务。
pub const PRODUCT_IMPORT_JOB_READ: Permission = Permission::new("background_job", "product_import_read");
/// 按商品创建功能资格操作导入上传与登记任务；逐行创建仍校验真实候选范围。
pub const PRODUCT_IMPORT_SUBMIT: Permission = Permission::new("product_import_job", "submit");
/// 按供应商创建资格查看本人导入任务。
pub const SUPPLIER_IMPORT_JOB_READ: Permission = Permission::new("background_job", "supplier_import_read");
/// 按供应商导入功能资格登记后台任务；逐行创建仍校验真实候选范围。
pub const SUPPLIER_IMPORT_SUBMIT: Permission = Permission::new("supplier_import_job", "submit");
/// 按品牌创建资格查看本人导入任务。
pub const BRAND_IMPORT_JOB_READ: Permission = Permission::new("background_job", "brand_import_read");
/// 按客户创建资格查看本人客户导入任务。
pub const CUSTOMER_IMPORT_JOB_READ: Permission = Permission::new("background_job", "customer_import_read");
/// 查看本人数据传输任务；交付结果仍核验所属业务资格。
pub const DATA_TRANSFER_JOB_READ: Permission = Permission::new("background_job", "data_transfer_read");

/// 按导入资格及商品选择资格创建销售询价候选。
pub const SALES_INQUIRY_IMPORT_CREATE: Permission = Permission::new("sales_inquiry", "import_create");
/// 重新提交新需求时同时校验商品选择资格。
pub const SALES_INQUIRY_REINQUIRE_DRAFT: Permission =
    Permission::new("sales_inquiry", "reinquire_with_draft");

/// 同角色具备成本创建资格时按销售详情范围选取费用来源。
pub const SALES_ORDER_FINANCIAL_EXPENSE_SOURCE: Permission =
    Permission::new("sales_order", "financial_expense_source");

/// 查询公司共享的配送模板，沿用供给列表功能资格。
pub const DELIVERY_TEMPLATE_READ: Permission = Permission::new("supplier_delivery_template", "read");
/// 维护公司共享的配送模板，沿用供给修改功能资格。
pub const DELIVERY_TEMPLATE_WRITE: Permission = Permission::new("supplier_delivery_template", "write");

/// 查询当前账号收到的审批抄送。
pub const APPROVAL_COPY_LIST: Permission = Permission::new("approval_copy", "list");
/// 查看当前账号收到的审批抄送。
pub const APPROVAL_COPY_DETAIL: Permission = Permission::new("approval_copy", "detail");
/// 标记当前账号收到的审批抄送已读。
pub const APPROVAL_COPY_READ: Permission = Permission::new("approval_copy", "read");
/// 在采购维护范围内以同角色退货资格取得退货来源。
pub const PURCHASE_RETURN_SOURCE: Permission = Permission::new("purchase_order", "return_source");

/// 查询实际经营盈亏的期间口径，仅要求成本列表功能资格。
pub const PROFIT_LOSS_PERIOD_BASIS: Permission =
    Permission::new("cost_entry", "profit_loss_period_basis_read");
/// 读取采购编辑交付资料，独立要求修改资格并沿用详情对象范围。
pub const PURCHASE_DELIVERY_OPTIONS_READ: Permission =
    Permission::new("purchase_order", "delivery_options_read");
/// 按销售列表范围读取客户质量历史，同时独立要求客户列表功能资格。
pub const CUSTOMER_QUALITY_HISTORY: Permission = Permission::new("sales_order", "customer_quality_history");

/// 按销售与客户各自当前范围读取客户质量订单交集。
pub const CUSTOMER_QUALITY_CURRENT: Permission = Permission::new("sales_order", "customer_quality_current");
/// 只读询价需求商品模板，沿用创建与商品选择的同角色资格。
pub const SALES_INQUIRY_TEMPLATE_PRODUCTS: Permission =
    Permission::new("sales_inquiry", "template_products_read");
/// 按冻结审批主体的管理资格恢复当前实例。
pub const APPROVAL_RECOVER: Permission = Permission::new("approval_instance", "recover");

/// 在客户质量查询中按列表资格读取当前客户范围，关闭历史参与补充。
pub const CUSTOMER_QUALITY_CUSTOMERS: Permission = Permission::new("customer", "quality_current");

/// 按冻结类型管理资格读取真实审批实例，不要求指派给当前账号。
pub const APPROVAL_MANAGEMENT_READ: Permission = Permission::new("approval_instance", "management_read");

/// 按真实发起、当前职责、已决定参与或管理来源读取审批实例。
pub const APPROVAL_INSTANCE_READ: Permission = Permission::new("approval_instance", "instance_read");
/// 查询当前账号真实发起的审批实例。
pub const APPROVAL_STARTED_LIST: Permission = Permission::new("approval_instance", "started_list");
/// 按当前审批办理资格查询本人仍负责的审批实例。
pub const APPROVAL_MINE_LIST: Permission = Permission::new("approval_instance", "mine_list");

/// 按销售列表与成本列表的同角色资格读取销售盈亏报表根。
pub const SALES_ORDER_PROFIT_LOSS_READ: Permission = Permission::new("sales_order", "profit_loss_read");

/// 同一角色具备采购列表与详情资格时读取供应商门户影响采购。
pub const PURCHASE_PORTAL_IMPACT_READ: Permission = Permission::new("purchase_order", "portal_impact_read");

/// 同角色创建并提交采购单，按服务端候选同时校验两项固定公司范围。
pub const PURCHASE_CREATE_AND_SUBMIT: Permission = Permission::new("purchase_order", "create_and_submit");

/// 按变更提交资格及同角色采购维护、提交资格读取真实原采购单。
pub const PURCHASE_CHANGE_SOURCE: Permission = Permission::new("purchase_order", "change_source");

/// 按变更创建资格及采购更新范围检查真实原采购单。
pub const PURCHASE_CHANGE_CREATE_SOURCE: Permission =
    Permission::new("purchase_order", "change_create_source");
/// 按变更提交资格及采购提交范围检查真实原采购单。
pub const PURCHASE_CHANGE_SUBMIT_SOURCE: Permission =
    Permission::new("purchase_order", "change_submit_source");
/// 按变更撤回资格及采购撤回范围检查真实原采购单。
pub const PURCHASE_CHANGE_CANCEL_SOURCE: Permission =
    Permission::new("purchase_order", "change_cancel_source");
/// 按变更作废资格及采购更新范围检查真实原采购单。
pub const PURCHASE_CHANGE_VOID_SOURCE: Permission = Permission::new("purchase_order", "change_void_source");
/// 按费用确认资格及采购更新范围检查真实采购单。
pub const PURCHASE_EXPENSE_CONFIRM_SOURCE: Permission =
    Permission::new("purchase_order", "expense_confirm_source");

/// 沿开票资格和完整销售来源只读已登记的开票结果。
pub const INVOICE_POSTED_RESULT_READ: Permission = Permission::new("invoice", "posted_result_read");

/// 以创建资格回读本人精确命令所创建的真实采购结果。
pub const PURCHASE_CREATED_RESULT_READ: Permission = Permission::new("purchase_order", "created_result_read");
/// 以创建资格回读本人精确选源命令的真实销售来源及结果。
pub const SOURCING_CREATED_RESULT_READ: Permission =
    Permission::new("sales_order", "sourcing_created_result_read");

/// 按创建资格及当前创建范围回读本人精确销售命令的真实结果。
pub const SALES_CREATED_RESULT_READ: Permission = Permission::new("sales_order", "created_result_read");

/// 按创建与提交的同角色资格校验销售创建范围及命令回放。
pub const SALES_CREATE_AND_SUBMIT: Permission = Permission::new("sales_order", "create_and_submit");

/// 具备成本列表及销售详情资格时，按销售详情范围读取附加费用。
pub const SALES_ORDER_EXTRA_COST_READ: Permission = Permission::new("sales_order", "extra_cost_read");

/// 按当前完整执行资格回读本人精确供应履约完成命令的结果。
pub const SUPPLIER_FULFILLMENT_COMPLETED_RESULT_READ: Permission =
    Permission::new("supplier_fulfillment_order", "completed_result_read");

/// 以结算维护资格刷新本人制单的结算单。
pub const SETTLEMENT_REFRESH_PREPARED: Permission =
    Permission::new("supplier_settlement_statement", "refresh_prepared");
/// 以结算维护资格作废本人制单的结算单。
pub const SETTLEMENT_VOID_PREPARED: Permission =
    Permission::new("supplier_settlement_statement", "void_prepared");
/// 以结算维护资格处理本人负责的结算差异。
pub const SETTLEMENT_RESOLVE_DIFFERENCE: Permission =
    Permission::new("supplier_settlement_statement", "resolve_assigned_difference");

/// 审批生效后的销售变更结果沿用销售详情资格。
pub const SALES_CHANGE_APPLIED_RESULT_READ: Permission =
    Permission::new("sales_change_order", "applied_result_read");
/// 采购变更提交回包使用原提交资格与原采购来源。
pub const PURCHASE_CHANGE_SUBMITTED_RESULT_READ: Permission =
    Permission::new("purchase_order", "change_submitted_result_read");
/// 采购变更作废回包使用原作废资格与原采购来源。
pub const PURCHASE_CHANGE_VOIDED_RESULT_READ: Permission =
    Permission::new("purchase_order", "change_voided_result_read");

/// 读取供应连接商品目录，保持连接详情资格与供应商对象读取范围的独立检查。
pub const CONNECTION_CATALOG_READ: Permission = Permission::new("supplier_api_connection", "catalog_read");

/// 读取协议目录前校验原入口资格与真实连接来源。
pub const CONNECTION_PROTOCOL_READ: Permission =
    Permission::new("supplier_api_connection", "protocol_read_target");

/// 签发技术绑定票据前校验原入口资格与真实连接来源。
pub const CONNECTION_REFERENCE_TICKETS: Permission =
    Permission::new("supplier_api_connection", "reference_tickets_target");

/// 按审批定义读取资格及真实单据类型查询定义期候选人员。
pub const APPROVAL_DEFINITION_ASSIGNEES: Permission =
    Permission::new("approval_process", "definition_assignees");

/// 按销售修改范围及独立变更提交资格读取完整变更草稿。
pub const SALES_CHANGE_DRAFT_READ: Permission = Permission::new("sales_order", "change_draft_read");

/// 核验商品池导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_PRODUCT_POOL: Permission = Permission::new("data_transfer", "product_pool");
/// 核验供应商询价归档与预览的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_SUPPLIER_QUOTES: Permission = Permission::new("data_transfer", "supplier_quotes");
/// 核验销售单导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_SALES_ORDERS: Permission = Permission::new("data_transfer", "sales_orders");
/// 核验采购单导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_PURCHASE_ORDERS: Permission = Permission::new("data_transfer", "purchase_orders");
/// 核验合同导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_CONTRACTS: Permission = Permission::new("data_transfer", "contracts");
/// 核验客户导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_CUSTOMERS: Permission = Permission::new("data_transfer", "customers");
/// 核验全部已授权客户导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_ALL_CUSTOMERS: Permission = Permission::new("data_transfer", "all_customers");
/// 核验库存余额导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_STOCK_BALANCE: Permission = Permission::new("data_transfer", "stock_balance");
/// 核验库存流水导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_STOCK_MOVEMENT: Permission = Permission::new("data_transfer", "stock_movement");
/// 核验库存调整导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_STOCK_ADJUSTMENT: Permission = Permission::new("data_transfer", "stock_adjustment");
/// 核验库存预占导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_STOCK_RESERVATION: Permission = Permission::new("data_transfer", "stock_reservation");
/// 核验供应商履约订单导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_SUPPLIER_ORDERS: Permission = Permission::new("data_transfer", "supplier_orders");
/// 核验应收子账导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_RECEIVABLES: Permission = Permission::new("data_transfer", "receivables");
/// 核验客户回款导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_RECEIPTS: Permission = Permission::new("data_transfer", "receipts");
/// 核验销项发票导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_INVOICES: Permission = Permission::new("data_transfer", "invoices");
/// 核验应付子账导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_PAYABLES: Permission = Permission::new("data_transfer", "payables");
/// 核验供应商付款导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_PAYMENTS: Permission = Permission::new("data_transfer", "payments");
/// 核验进项发票分配导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_INVOICE_ALLOCATIONS: Permission =
    Permission::new("data_transfer", "invoice_allocations");
/// 核验开票申请导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_INVOICE_REQUESTS: Permission = Permission::new("data_transfer", "invoice_requests");
/// 核验销售盈亏报表导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_PROFIT_LOSS: Permission = Permission::new("data_transfer", "profit_loss");
/// 核验客户质量报表导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_CUSTOMER_QUALITY: Permission = Permission::new("data_transfer", "customer_quality");
/// 核验商品主数据导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_PRODUCTS: Permission = Permission::new("data_transfer", "products");
/// 核验供应商主数据导出的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_SUPPLIERS: Permission = Permission::new("data_transfer", "suppliers");
/// 核验询价导入预览的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_INQUIRY_PREVIEW: Permission = Permission::new("data_transfer", "inquiry_preview");
/// 核验导入新建询价预览的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_INQUIRY_CREATION: Permission = Permission::new("data_transfer", "inquiry_creation");
/// 核验供给批量调价的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_OFFERING_REVISE: Permission = Permission::new("data_transfer", "offering_revise");
/// 核验供给批量可供维护的功能资格；实际对象与数据范围由所属业务入口授权。
pub const DATA_TRANSFER_OFFERING_AVAILABILITY: Permission =
    Permission::new("data_transfer", "offering_availability");

/// 以原库存动作的完整资格和范围核验真实仓库，不授予库存对象访问。
pub const INVENTORY_BALANCE_WAREHOUSES: Permission =
    Permission::new("inventory_warehouse", "stock_balance_list");
/// 以原库存动作的完整资格和范围核验真实仓库，不授予库存对象访问。
pub const INVENTORY_BALANCE_DETAIL_WAREHOUSES: Permission =
    Permission::new("inventory_warehouse", "stock_balance_detail");
/// 以原库存动作的完整资格和范围核验真实仓库，不授予库存对象访问。
pub const INVENTORY_MOVEMENT_WAREHOUSES: Permission =
    Permission::new("inventory_warehouse", "stock_movement_list");
/// 以原库存动作的完整资格和范围核验真实仓库，不授予库存对象访问。
pub const INVENTORY_RESERVATION_WAREHOUSES: Permission =
    Permission::new("inventory_warehouse", "stock_reservation_list");
/// 以原库存动作的完整资格和范围核验真实仓库，不授予库存对象访问。
pub const INVENTORY_ADJUSTMENT_WAREHOUSES: Permission =
    Permission::new("inventory_warehouse", "stock_adjustment_list");
/// 以原库存动作的完整资格和范围核验真实仓库，不授予库存对象访问。
pub const INVENTORY_ADJUSTMENT_DETAIL_WAREHOUSES: Permission =
    Permission::new("inventory_warehouse", "stock_adjustment_detail");
/// 以原库存动作的完整资格和范围核验真实仓库，不授予库存对象访问。
pub const INVENTORY_CREATE_WAREHOUSES: Permission =
    Permission::new("inventory_warehouse", "stock_adjustment_create");
/// 以原库存动作的完整资格和范围核验真实仓库，不授予库存对象访问。
pub const INVENTORY_UPDATE_WAREHOUSES: Permission =
    Permission::new("inventory_warehouse", "stock_adjustment_update");

/// 回款附件存储前保留原提交入口功能资格，不证明对象或来源资格。
pub const FINANCE_UPLOAD_CUSTOMER_RECEIPT: Permission =
    Permission::new("customer_receipt", "prepare_attachment_upload");

/// 发票草稿附件存储前检查创建功能资格。
pub const FINANCE_UPLOAD_INVOICE_DRAFT: Permission =
    Permission::new("invoice", "prepare_draft_attachment_upload");

/// 开票附件存储前检查过账功能资格，不替代精确任务授权。
pub const FINANCE_UPLOAD_INVOICE_POST: Permission =
    Permission::new("invoice", "prepare_post_attachment_upload");

/// 进项发票附件存储前检查原登记入口功能资格。
pub const FINANCE_UPLOAD_PURCHASE_INVOICE: Permission =
    Permission::new("purchase_invoice_allocation", "prepare_attachment_upload");

/// 附加费用凭证存储前检查完整同角色功能资格。
pub const FINANCE_UPLOAD_EXTRA_COST: Permission = Permission::new("cost_entry", "prepare_attachment_upload");

/// 付款回单存储前检查付款入口功能资格，不替代当前任务或来源授权。
pub const FINANCE_UPLOAD_PURCHASE_PAYMENT: Permission =
    Permission::new("purchase_payment_request", "prepare_attachment_upload");

/// 商品新增附件存储前检查创建入口功能资格。
pub const CATALOG_CREATE_ATTACHMENT_UPLOAD: Permission =
    Permission::new("product", "prepare_create_attachment_upload");

/// 商品修改附件存储前检查更新入口功能资格。
pub const CATALOG_UPDATE_ATTACHMENT_UPLOAD: Permission =
    Permission::new("product", "prepare_update_attachment_upload");

/// 供应商新增附件存储前检查创建入口功能资格。
pub const SUPPLIER_CREATE_ATTACHMENT_UPLOAD: Permission =
    Permission::new("supplier", "prepare_create_attachment_upload");

/// 供应商修改附件存储前检查更新入口功能资格。
pub const SUPPLIER_UPDATE_ATTACHMENT_UPLOAD: Permission =
    Permission::new("supplier", "prepare_update_attachment_upload");

/// 采购费用附件存储前检查原确认入口功能资格。
pub const PURCHASE_EXPENSE_ATTACHMENT_UPLOAD: Permission =
    Permission::new("purchase_expense_occurrence", "prepare_attachment_upload");

/// 电子交付附件存储前检查完整同角色功能资格。
pub const ELECTRONIC_DELIVERY_ATTACHMENT_UPLOAD: Permission =
    Permission::new("electronic_delivery", "prepare_attachment_upload");

/// 服务履约附件存储前检查完整同角色功能资格。
pub const SERVICE_FULFILLMENT_ATTACHMENT_UPLOAD: Permission =
    Permission::new("service_fulfillment", "prepare_attachment_upload");

/// 品牌新增附件存储前检查创建入口功能资格。
pub const CATALOG_BRAND_CREATE_ATTACHMENT_UPLOAD: Permission =
    Permission::new("product_brand", "prepare_create_attachment_upload");

/// 品牌修改附件存储前检查更新入口功能资格。
pub const CATALOG_BRAND_UPDATE_ATTACHMENT_UPLOAD: Permission =
    Permission::new("product_brand", "prepare_update_attachment_upload");
