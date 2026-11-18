# erp-read-models：跨领域查询与展示投影

## 职责

组合多个领域的业务事实，为对象中心、工作台、列表、统计和导出提供面向使用场景的查询。业务记录、状态规则和写入仍归所属领域；本库定义跨域读取步骤、投影及展示口径，不执行提交、生效、审批决定或过账。

### 负责的数据与能力

- 在已有 MongoDB 业务数据上联查、批量补齐和汇总，返回查询 DTO、展示视图或供调用方消费的窄事实。ReadModel 不要求另建读库，也不默认复制业务数据。
- 为工作流和 Process 提供正式对象的只读事实、命令结果投影和冻结材料的采集入口。保存冻结材料、推进任务及执行业务命令由调用方负责。

### 使用场景

当一个查询必须同时使用多域事实才能确定可见结果、筛选或汇总口径时，放在本库。只处理一个领域自身事实的查询仍由该领域提供；需要原子更新多个领域的用例交给对应 Process。

## 目录结构

[Cargo.toml](Cargo.toml) 声明依赖，[src/lib.rs](src/lib.rs) 是公开模块和类型的入口。投影按业务场景组织：

| 分组 | 源码入口与职责 |
| --- | --- |
| 客户与合同 | [`customer_center`](src/customer_center/mod.rs)：客户资料、合同与销售摘要、应收汇总；[`contract_attachment`](src/contract_attachment/mod.rs)：合同附件元数据与预览。 |
| 销售与采购 | [`sales_center`](src/sales_center/mod.rs)：[销售单](src/sales_center/order/mod.rs)、[询价](src/sales_center/inquiry/mod.rs)、[选品册与方案](src/sales_center/selection/mod.rs)；[`sales_timeline`](src/sales_timeline/mod.rs)：销售全流程与任务动态；[`purchase_center`](src/purchase_center/mod.rs)：采购列表、对象中心、创建依据与责任规则。 |
| 商品、供给与供应商 | [`catalog_center`](src/catalog_center/mod.rs)：商品中心、配送覆盖及采购责任；[`catalog_supply`](src/catalog_supply/mod.rs)：商品与供给的跨域聚合及可售资格；[`supplier_center`](src/supplier_center/mod.rs)：供给、连接、履约及结算；[`supplier_portal`](src/supplier_portal/mod.rs)：门户的商品、合作和申请视图。 |
| 履约、库存与逆向业务 | [`fulfillment_center`](src/fulfillment_center/mod.rs)：客户验收资格与登记结果；[`fulfillment_queue`](src/fulfillment_queue/mod.rs)：履约队列聚合仓储；[`inventory_center`](src/inventory_center/mod.rs)：库存调整详情与审批投影；[`returns_center`](src/returns_center/mod.rs)：采购退货、退款及冲正相关跨域视图。 |
| 财务与经营报表 | [`finance`](src/finance/mod.rs)：应收应付、收付款与开票单据、成本和[非卡券实际经营盈亏](src/finance/actual_profit_loss/mod.rs)；[`customer_quality`](src/customer_quality/mod.rs)：按当前主责或首次生效归属分别查询及导出客户经营质量；[`historical_directory`](src/historical_directory.rs)：报表共用的历史人员及组织目录合同。 |
| 工作台、审批与授权检查 | [`workbench`](src/workbench/mod.rs)：业务对象事实、审批材料与简报、履约责任队列入口；内部 [`approval_runtime`](src/approval_runtime/mod.rs) 组装单据审批运行信息；[`approval_subject`](src/approval_subject/mod.rs) 读取审批绑定升级所需的强业务事实；[`access_inspection`](src/access_inspection.rs) 检查权限配置及已接入的销售单条件，不交付单据。 |
| 导入与集成 | [`import_center`](src/import_center.rs)：导入批次与来源系统名称；[`integration_center`](src/integration_center/mod.rs)：集成异常、对账差异及其责任与证据。 |

### 代码入口

| 共享入口 | 用途 |
| --- | --- |
| [`ports`](src/ports/mod.rs) | 审批实例、[正式工作项授权](src/ports/work_item_authorization.rs)和开票授权所需的窄合同，由调用方装配实现。 |
| [`work_item_links`](src/work_item_links/mod.rs) | 仅供写流程在原事务采集直接对象和真实上级链路，不供任务查询使用。 |
| [`sales_attribution`](src/sales_attribution.rs)、[`sales_money_progress`](src/sales_money_progress.rs) | 分别采集与复验销售首次生效归属，以及按调用方执行器提供应收子账余额事实。 |

## 架构设计

### 跨域取数与结果口径

通过拥有领域的公开服务、Repository 或窄 Port 取数。跨集合聚合放在本库的只读仓储，复用拥有领域公开的集合访问合同；不得绕过集合边界，也不得复制事实写入规则。列表、统计、总数与导出须保持同一授权和筛选口径；分页不得截断汇总，排序须稳定。

### 授权、执行器与交付复验

受保护查询使用 `erp-permissions` 的原动作权限和共享 `AuthorizationService`，复用领域对象范围、字段资格及正式任务授权，不从路由认证、列表可见性或客户端筛选推断详情权限。授权范围须在计数、分页或汇总前生效；交付前按用例合同复验原凭证。需要独立检查最新范围时，在新读取阶段重新解析授权和业务事实，不携带原快照凭证。

读取入口按用例选择执行器：独立一致读取可用 `with_transaction` 建立快照；已有快照的调用方传入原 `Executor`，仓储和 Port 不另开事务。同一读取阶段的授权、取数和凭证校验必须复用该执行器。无需快照但需要凭证复验时用同一个 `ReadExecutor`，不能用 `NoTransaction` 代替。参见 [业务授权 API](../../domains/erp-identity/README.md#业务授权-api)和[持久化合同的读取边界](../../foundation/persistence-core/README.md#读取边界)。

### 协作示例

- **客户中心。** [`CustomerCenterReadService::related`](src/customer_center/service.rs) 在同一读取事务确认客户详情资格，分别解析合同、销售列表范围。[关联聚合](src/customer_center/repository/related.rs) 先按各自范围过滤，再用 `$facet` 分别取最近摘要，以及不受摘要条数限制的有效合同和进行中销售单计数；服务沿原执行器复验各范围。`receivable` 另以[应收来源授权](src/finance/funds_scope/receivable.rs)限定和汇总可读子账，并在交付前复验。
- **采购列表。** [`PurchaseOrderReadService::purchase_order_list`](src/purchase_center/query.rs) 在[首个快照](src/purchase_center/scope.rs)解析采购列表授权和外域关键词匹配。采购仓储负责范围内的筛选、排序、分页及总数；[列表事实读取](src/purchase_center/repository/list_facts.rs) 批量补齐当前页的供应商法定名称、来源销售单号、负责人、提交或版本及财务核销状态，不逐行查询。原快照先校验原凭证；后续页必须回传范围版本，交付前在独立快照重新计算授权范围与全部匹配采购单的版本指纹，变化则拒绝交付。
- **履约责任队列。** [工作台入口](src/workbench/fulfillment_queue.rs) 通过 `WorkItemReadService` 取得正式任务授权，在同一事务对当前账号负责的开放履约作业做关联查询、分页与指标聚合，返回前复验任务授权。
- **经营报表。** [实际经营盈亏](src/finance/actual_profit_loss/mod.rs) 的列表和导出共用授权读取：按可见销售单读正式销售版本和财务成本分配，按完整匹配集计算指标；交付前在新快照复核授权与销售单范围版本，不按供应商推测成本归属。

### 使用与边界要求

- **依赖方向。** 本库可依赖多个领域的公开接口，不依赖编排库，也不得被领域 crate 反向依赖。HTTP/应用组合层负责协议与装配；Process 可调用本库读取来源事实或结果，事务与业务写入仍由 Process 持有。例如 [`work_item_links::capture`](src/work_item_links/mod.rs) 只在创建任务或切换业务版本的原写事务中采集链路，任务列表、详情和统计只消费已保存的链路，不重新向上遍历。
- **库存调整。** [`InventoryAdjustmentReadService`](src/inventory_center/detail.rs) 复用同一 `Executor` 读取授权、单据、明细、过账流水、审批绑定、实例和人员身份。独立详情入口创建快照，核心事实成功后沿原执行器补齐可降级的姓名与 SKU 名称。命令收据指定审批实例和主题版本时必须精确读取，不得回落到最新实例。
- **商品与供给。** [`catalog_supply`](src/catalog_supply/repository/sellable_search.rs) 提供实时聚合和已建投影查询，不负责追平或写入投影。[应用层 Port 装配](../../../apps/erp-server/src/wiring/catalog_supply_query/mod.rs) 先读取配送范围与勾选范围的交集，空集直接返回，不触发写入；仅当资格业务日为当天时，才调用[目录编排的刷新器](../../orchestration/erp-supplier-catalog/src/sellable_projection_refresh.rs)尝试追平。投影不可用时走实时聚合。
- **合同附件。** 必须通过 [`ContractAttachmentReadService::metadata/preview`](src/contract_attachment/mod.rs) 读取：沿同一快照校验合同完整详情权限、文件与不可变修订的关联，读取资产元数据，预览时记录审计，并在交付前复验原凭证。不额外要求文件管理菜单资格；[HTTP 预览入口](../../../apps/erp-server/src/http/handler/contract/mod.rs)在事务结束后读取对象存储内容。

## 查询归属判定

| 查询或操作 | 归属 |
| --- | --- |
| 客户账户自身资料的普通列表、分配记录 | [erp-customer](../../domains/erp-customer/README.md) 的 Service/Repository。 |
| 完整客户资料详情，或客户关联的合同、销售与应收摘要 | 本库的 [`customer_center/profile`](src/customer_center/profile/mod.rs) 或 [`customer_center`](src/customer_center/service.rs)；资料详情还须按字段权限裁剪从属资料和敏感字段令牌。 |
| 采购列表补齐供应商名称、销售单号、负责人等外域事实 | 本库的 [`purchase_center`](src/purchase_center/mod.rs)。 |
| 询价、选品册与方案、供应商履约列表及结算列表 | 必须使用本库的 [`inquiry`](src/sales_center/inquiry/mod.rs)、[`selection`](src/sales_center/selection/mod.rs)、[`fulfillment_list`](src/supplier_center/fulfillment_list.rs) 或 [`settlement/list`](src/supplier_center/settlement/list.rs)，复用其原权限常量、授权结果和事务快照。 |
| 通用任务列表、详情、统计与人员显示名 | [erp-workflow 的 `WorkItemReadService`](../../domains/erp-workflow/README.md#任务查询与关联链路)。 |
| 跨业务审批材料、简报及履约业务队列 | 本库的 [`workbench`](src/workbench/mod.rs)；任务列表可见性不能代替业务材料授权。 |
| 销售退货单域列表、详情和命令结果装配 | [erp-returns](../../domains/erp-returns/README.md)：列表与详情使用 `SalesReturnQueryService`；已完成命令授权的结果装配使用 `ReturnsService::load_sales_return_case_view` 并传入原 `Executor`。本库不保留这些查询入口。 |
| 执行订单提交、生效、财务过账或维护落库投影 | 对应业务 Process；如需结果视图或依据事实，调用本库只读入口。 |

## 验证要求

仅修改文档时，在 `backend/` 执行 `git diff --check -- crates/read-models/erp-read-models/README.md`，不编译、不测试。代码改动按[后端开发规则](../../../AGENTS.md#验证)和[统一质量门禁](../../README.md#质量门禁)执行；本地检查限定受影响 crate，测试仅运行库单元测试，不执行集成测试或依赖真实外部服务的测试。
