# erp-supplier-catalog：供应目录与供给编排

## 职责

`erp-supplier-catalog` 把供应商目录观察、公司商品和正式供给串成跨域用例：采集与补查更新来源；明确选品才创建或复用公司商品、SKU 并建立供给；后续同步只更新符合来源关系和领域规则的资料。本库也负责供应商门户的提报与审核、连接与供给治理、来源媒体归档，以及相关集成异常处理。

实体、集合、索引、状态转换和资格规则仍由拥有领域维护：`erp-supply` 管供应连接、目录观察、选品记录和正式供给，`erp-catalog` 管公司商品、SKU、资料来源和门户新品提报，`erp-supplier` 管供应商账户和合作资格，`erp-identity` 判定功能权限与对象范围，`erp-workflow` 管正式工作项，`erp-support` 管后台任务和文件资产，`erp-integration` 管回调与集成治理事实。本库调用这些领域的服务和仓储，决定何时重验、调用哪些步骤、哪些写入同事务提交；跨域查询投影归 `erp-read-models`。订单执行、供应商消息领取与确认不由目录写入入口承担。

## 目录结构

公开入口见 [`src/lib.rs`](src/lib.rs)，消费依赖与正式工作项 Port 见 [`src/wiring.rs`](src/wiring.rs)。

| 源码入口 | 用途 |
| --- | --- |
| [`supplier_connection_execution/`](src/supplier_connection_execution/mod.rs) | 连接健康检查、目录分页采集、目录池成员核对和消息目录写入。 |
| [`supplier_catalog_adoption/`](src/supplier_catalog_adoption/mod.rs)、[`product_source_sync.rs`](src/product_source_sync.rs) | 选品预览、确认、批次执行与查证；建立公司商品的资料来源，按领域规则应用后续观察。 |
| [`supplier_catalog_callbacks/`](src/supplier_catalog_callbacks/mod.rs) | 消费已验签、已持久化的商品回调，补查当前事实并逐步记录检查点。 |
| [`supplier_portal/`](src/supplier_portal/mod.rs) | 门户账号与授权、可供上报、供给／新品／合作申请、审核任务和素材。 |
| [`supply_governance/`](src/supply_governance/mod.rs)、[`supplier_delivery_template.rs`](src/supplier_delivery_template.rs)、[`source_registry.rs`](src/source_registry.rs) | 连接及供给治理、交付模板维护与分配、来源系统登记。 |
| [`source_media/`](src/source_media/mod.rs) | 来源图片下载、内容核验、资产批次准备、后台补齐及对象清理。 |
| [`integration_resolution/`](src/integration_resolution/mod.rs) | 集成入站登记及结果回写、错误任务和对账差异处理，协调权威证据、正式工作项与审计。 |
| [`background/`](src/background/mod.rs)、[`delivery_coverage_refresh.rs`](src/delivery_coverage_refresh.rs)、[`sellable_projection_refresh.rs`](src/sellable_projection_refresh.rs) | 对接通用后台运行时；刷新 SKU 配送覆盖与公司商品池可售投影。 |

## 架构设计

### Port 与应用装配

- [应用组合根](../../../apps/erp-server/src/wiring/supplier_catalog.rs)注入 `wiring::CatalogServices`：商品服务及商品对象授权、供给服务及供给对象授权、供应商对象授权和 `CatalogWorkItems`。本库不自行创建共享身份范围适配器。正式任务操作由 `CatalogWorkItems` 发起，应用注入已装配授权、对象事实和审计 Port 的 `WorkItemService`；任务步骤沿用调用方的 `Executor`。参见[业务授权 API](../../domains/erp-identity/README.md#业务授权-api)。
- 连接执行依赖 `erp-supply` 的 [`SupplierApiGateway`](../../domains/erp-supply/src/ports/supplier_api_gateway.rs)，选品以 [`SupplierReferenceRegistry`](../../domains/erp-supply/src/ports/supplier_reference_registry.rs) 检查来源部署和商品能力，商品回调经 [`SupplierConnectorRegistry`](../../domains/erp-supply/src/connector_registry.rs) 绑定当前连接。实际[协议运行时](../../domains/erp-supply/src/ports/connector/runtime.rs)由[应用注册](../../../apps/erp-server/src/wiring/application/supplier_connectors.rs)。协议适配器负责协议、签名、传输与标准事实转换，本库不依赖具体供应商协议 crate，也不通过自动下单资格判断能否采集或选品。
- 外部商品 HTTP、图片下载以及对象存储读写不进入数据库事务。事务内用同一 `Executor` 完成决定写入的授权、版本检查、领域写入和相应审计；提交未知时按原操作身份查证，不把未读到回执当成未提交。参见[后端授权和事务规则](../../../AGENTS.md#授权和事务)。

### 连接同步、回调与消息补查

1. [连接治理](src/supply_governance/supplier_api/jobs.rs)同事务登记后台任务、运行记录和命令回执，HTTP 不等待网关。连接执行的 [`start → invoke → finish`](src/supplier_connection_execution/execution.rs) 先提交启动事务，再执行网关调用，最后以独立事务登记结果。以[目录同步](src/supplier_connection_execution/catalog.rs)为例，启动重验采集权限，将任务由 `Pending` 置为 `Running`；收尾登记 `Succeeded` 或 `Failed` 及审计，普通采集失败按分类登记错误任务，停机中断单独记录续跑标记。
2. [分页采集](src/supplier_connection_execution/catalog_scan.rs)按运行记录的扫描窗口和游标，在事务外调用 `catalog_page`、整理来源观察与撤回项。每页事务重新校验采集权限、技术配置、任务状态和检查点版本，一起写入目录观察、已关联的正式供给和公司资料、适用的目录池证据、下一游标及心跳。当前页失败不会撤销已提交页面；[恢复](src/supply_governance/supplier_api/catalog_recovery.rs)只沿用兼容的未完成检查点。[目录池缺项核对](src/supplier_connection_execution/catalog_membership.rs)要求完整且经事务外重验的范围证据，范围不确定时保留观察，不按缺项撤回。
3. 商品回调在事务外[补查当前事实](src/supplier_catalog_callbacks/query.rs)，[结果事务](src/supplier_catalog_callbacks/store.rs)重验采集授权、配置和租约，把来源更新和逐规格检查点一并提交。另一条消息补查链路由订单编排声明 [`SupplierMessageCatalog`](../erp-supplier-orders/src/supplier_messages/catalog.rs)，[应用](../../../apps/erp-server/src/wiring/application/supplier_operations.rs)将它接到 [`CatalogMessageWriter`](src/supplier_connection_execution/message_writer.rs)：先找回原采集授权的有效操作人，补查后用原连接快照在结果事务重验配置及授权，再复用目录观察写入。该写入器不领取或确认原消息，也不写订单状态。

### 选品入库与资料更新

[选品确认](src/supplier_catalog_adoption/confirm.rs)按冻结的来源、确认的映射和采购条款执行。持久化[批次入口](src/supplier_catalog_adoption/batch_execution.rs)按原操作号冻结选中的就绪行，交给后台认领，提交请求不逐行入库。批次行由 `Ready` 进入 `Processing`，之后记录 `Succeeded`、`Failed` 或 `Unknown`；待核实只查原回执，不能直接重做。正式入库按来源规格逐项开事务，重验权限、来源与匹配，原子提交商品／SKU、供给、来源关联、选品回执与审计；批次进度另行保存。

采集不代替选品。已选目录项的[供给同步](src/supplier_connection_execution/catalog_supply.rs)只更新同源 API 供给的采购报价和可供事实，不改公司销售价或人工维护的供给关系状态。[公司资料同步](src/product_source_sync.rs)只应用已绑定的同源观察；`erp-catalog` 按字段判断哪些可自动采用，人工维护值或尚不可采用的素材留待确认。

### 门户申请与正式审核

[门户命令](src/supplier_portal/command.rs)使用真实供应商账号，事务内重验可写会话；内部命令先过固定入口资格，再解析真实目标、核对供应商范围。命令按账号、供应商、动作、幂等键和载荷绑定回执，匹配的回放只返回原结果。以[新品提报](src/supplier_portal/new_products.rs)为例：草稿提交为 `Pending` 时冻结本次提交并创建正式审核任务；审核通过后生成或复用商品和 SKU、写入供给、完成任务并标记 `Effective`，退回则完成本次任务并标记 `Returned`，撤回则联动撤回原任务。申请、业务事实、任务、回执和审计沿用同一事务。审核前复验申请、主题、任务版本及当前处理人；[审核回放](src/supplier_portal/tasks.rs)仍要求当前完整办理资格和任务与原申请的关联，并不要求已完成任务重新开放。

[`supplier_portal_material:read/review`](src/supplier_portal/request_access.rs)（含按申请种类区分的动作）只验证内部材料的完整来源资格，不登记为 HTTP 路由或角色权限。调用方必须先证明任务、申请或素材入口资格，材料政策不能代替入口授权。应用接入 `AccessFactsPort` 读取真实申请及其供应商和（适用时）供给事实，由 identity 分别检查来源对象的功能资格和范围。[门户附件上传](../../../apps/erp-server/src/http/handler/supplier_portal/assets.rs)由 HTTP 层检查文件并事务外写对象存储，[登记命令](src/supplier_portal/assets.rs)在事务内保存资产元数据、草稿引用、回执和审计。

### 来源媒体与后台运行

目录分页和补查先保存来源图片及待处理时间，不等待下载；选品使用已归档图片，未归档项留给[媒体 worker](src/source_media/worker.rs)。`SourceMediaPreparer` 通过 `SourceImageTransport` 与 `SourceImageStore` 在事务外读取、核验并保存字节，生产实现为[有超时和响应体上限的 HTTP 读取器](src/source_media/transport.rs)与 S3 存储。已归档图片可复用；未核验图片不携带资产证明。准备器通过 [`erp-file-assets`](../erp-file-assets/src/catalog.rs) 将 `PendingFileAssets` 转成商品 `PendingAttachmentBatch` Port。worker 补图后在结果事务重验到期条目的版本，同事务保存资产、审计、来源媒体和可应用的公司资料。未完成项写回重试时间。[对象清理](src/source_media/cleanup.rs)只补偿本次新增且确认未被提交引用的字节；提交结果未知时保留对象。

[`background/`](src/background/mod.rs) 只提供本业务的任务适配器：目录续跑、选品批次以及两类投影刷新。目录的到期计划由 XXL-JOB 入口触发，持久化任务的消费和恢复由后台适配器处理。[应用 worker](../../../apps/erp-server/src/worker/mod.rs)负责注册、启停和监督；通用轮询、周期执行、预算与排空使用 [background-runtime](../../platform/background-runtime/README.md)，媒体按 `periodic` 独立运行，停止时完成当前条目的写入和清理。
