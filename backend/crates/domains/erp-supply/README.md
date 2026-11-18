# erp-supply：供给、供应商连接、履约与结算

## 职责

本 crate 管理公司 SKU 的供应来源、供应商 API 连接、供应商侧订单及对账结算。供应商主档说明与谁合作；本域记录该供应商能供什么、按什么条款供货、供应商侧发生了什么，以及这些履约事实如何对账。公司商品与 SKU 归 [erp-catalog](../erp-catalog/README.md)，供应商账户、资质和商业档案归 [erp-supplier](../erp-supplier/README.md)，正式采购单归 [erp-procurement](../erp-procurement/README.md)，应付、成本、付款和核销归 [erp-finance](../erp-finance/README.md)。

### 负责的数据与能力

| 业务 | 本域拥有的记录与规则 |
| --- | --- |
| 供应商品库与供给 | 按连接、外部商品和订货规格保存目录观察；另存同步检查点、授权商品池成员证据和选品记录。正式供给关联公司 SKU 与供应商，分别保存稳定供给身份、商业条款修订（如采购价、税率）和实时可供状态。 |
| 供应商连接 | 保存供应商归属、连接代码、明确声明的协议类型、配置引用、技术配置版本、启停状态及健康检查记录；保留能力、业务确认和命令回执等治理记录。实际可调用能力由运行时提供，不从历史配置推断。 |
| 供应商侧订单 | 保存履约订单、明细、动作、历史和正式退款事实；API 执行另存冻结来源行、加密请求快照、步骤与动作指纹、根单和子单关系、跟踪检查点及来源观察。 |
| 售后与对账结算 | 分开保存 API 退款执行和来源回执、结算扫描检查点与外部流水，以及正式结算的来源批次、冻结明细、差异、补证和复核主题。来源采集结束不等于过账。 |
| 门户与询价 | 门户申请、定向报价资格、归属受限的供给命令及独立回执；采购内部询价报价、反馈轮次、专项采用余额和费用收款依据。两者均不以草稿或询价发布替代正式条款。 |

### 使用场景

- 修改供给关系、商业条款修订、实时可供、目录采集记录或供给资格校验时，从本域的实体与用例入手。
- 修改连接配置、命令意图、供应商侧履约及退款事实，或结算差异与复核规则时，先定位本域记录，再核对调用它的 Process；修改供应商协议则进入相应 Adapter。

### 协作示例

采购人员从供应商品库选中一个外部订货规格后，目录 Process 按冻结预览关联或创建公司 SKU，再由本域写入正式供给、首版条款及初始可供记录。采购获批并进入 API 执行后，订单 Process 先保存执行意图，再调用供应商；供应商回执按本域规则登记。后续收到供应商账单流水时，先作为待核对来源，再关联正式结算来源批次；财务复核过账另有入口。

## 目录结构

### 代码入口

| 入口 | 用途 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | 依赖声明与公开模块入口；`lib.rs` 重导出供给、履约和结算访问器及授权凭证。 |
| [entity](src/entity/mod.rs)、[dto](src/dto/mod.rs) | 领域实体、值对象和确定性规则；用例入参与视图。 |
| [supplier_catalog](src/entity/supplier_catalog.rs)、[supplier_catalog_run](src/entity/supplier_catalog_run.rs)、[catalog_membership](src/catalog_membership/mod.rs)、[catalog_adoption](src/catalog_adoption/mod.rs) | 供应商品库记录、分页检查点、商品池成员证据、选品预览与逐项执行记录。 |
| [service/supplier_offering](src/service/supplier_offering/mod.rs) | 供给创建、商业条款修订、可供与来源报价更新、供给交接。 |
| [service/supplier_api](src/service/supplier_api/mod.rs)、[connector_registry](src/connector_registry.rs) | 连接登记、健康和启停治理、命令意图与回执；按协议和用途分派运行时。 |
| [service/supplier_fulfillment](src/service/supplier_fulfillment/mod.rs)、[api_order](src/api_order/mod.rs)、[order_tracking](src/order_tracking/mod.rs) | 履约订单与结果登记；API 执行意图、动作恢复及不可变来源观察。 |
| [after_sales](src/after_sales/mod.rs)、[service/supplier_settlement](src/service/supplier_settlement/mod.rs) | 退款动作与结算来源收件；正式来源录入、结算草稿、差异及复核。 |
| [inquiry](src/inquiry/mod.rs)、[portal](src/portal/mod.rs) | 询价内部报价与专项采用；门户商业申请、定向报价及正式确认。 |
| [ports](src/ports/mod.rs)、[Connector 合同](src/ports/connector/README.md) | 外域资格、技术引用、旧网关和供应商协议窄接口。具体运行装配及正式业务切换以相应接入合同为准。 |
| [repository](src/repository/mod.rs)、[indexes](src/indexes/mod.rs)、[command_receipt](src/command_receipt/mod.rs) | MongoDB 集合访问、过滤和写入、按既有顺序登记索引；独立命令回执保存原载荷指纹与强类型结果。部分业务子模块自带仓储或索引。 |
| [authorization](src/authorization.rs) | 提供本域授权事实、固定 Mapping 和当前读取阶段使用的编译凭证接口。 |

## 架构设计

### 依赖与协作边界

- **领域与仓储。** `entity/` 保存不依赖 I/O 的不变式和状态迁移，`service/` 组织单域用例，仓储复用调用方的 `persistence_core::Executor`。跨域命令由具名 Process 开启根事务；同一原子写入的校验、仓储及相关 Port 使用同一执行器。供应商 HTTP、S3 等外部 I/O 不得放入数据库事务。
- **外域事实 Port。** [QualificationPort / PortalQuoteQualificationPort](src/ports/offering_qualification.rs) 在原执行器上校验公司 SKU、供应商资格及门户报价依据；目录编排提供[实际实现](../../orchestration/erp-supplier-catalog/src/supply_governance/offering/qualification.rs)。其余外域事实以窄 Port 或显式输入传入。本域的常规、构建和测试依赖不得指向其他业务领域或组合层；`erp-identity` 的公开身份与授权 API 是例外。术语与依赖规则见[总索引](../../README.md#代码中常见名称的含义)。
- **运行时 Port 与适配器。** [SupplierReferenceRegistry](src/ports/supplier_reference_registry.rs) 解析服务端不透明引用和部署配置，并提供能力描述；[SupplierRuntime 与 ConnectorPurpose](src/ports/connector/runtime.rs) 按连接与用途绑定 `SupplierConnector`。窄接口分别提供商品和可供、配送、订单、消息或回调、资金证据、退款及结算。`SupplierApiGateway` 兼容健康检查和目录消费入口，旧 `SupplierGateway` 保留既有动作合同，不用来隐藏多步下单。协议实现放在 [蛋糕叔叔适配器](../../adapters/erp-connector-dangaoshushu/README.md) 与 [京东 VOP 适配器](../../adapters/erp-connector-jd-vop/README.md) 等 crate：负责鉴权、传输、验签和事实归一，不读写 ERP 业务仓储。[服务端组合根](../../../apps/erp-server/src/wiring/application/supplier_connectors.rs) 按部署配置登记运行时并注入 Port；未提供的能力不视为可用。
- **跨域读取。** [erp-read-models](../../read-models/erp-read-models/README.md) 组装需要供应商主档、连接或结算等多域事实的视图，例如[连接详情](../../read-models/erp-read-models/src/supplier_center/supplier_api/query.rs)使用运行时能力描述；无法取得描述时保留未知。协议支持某项操作不代表已获得该操作的业务授权。

### 目录、连接、订单与结算的执行边界

1. **目录观察不等于正式供给。** [目录采集](../../orchestration/erp-supplier-catalog/src/supplier_connection_execution/catalog_scan.rs)由 [erp-supplier-catalog](../../orchestration/erp-supplier-catalog/README.md) 在事务外经 `OfferSource::scan_scoped` 对应的网关入口取页，使用同一事务保存目录观察、跳过项、可用的成员证据与检查点。普通列表缺项不是撤回依据；[成员核对](../../orchestration/erp-supplier-catalog/src/supplier_connection_execution/catalog_membership.rs)要求完整页链及授权范围重验。只有显式[选品确认](../../orchestration/erp-supplier-catalog/src/supplier_catalog_adoption/confirm.rs)才建立公司商品/SKU 与正式供给关联；例行同步可对已关联供给更新来源采购报价和可供，不覆盖公司销售价或人工关系状态。
2. **连接治理与业务准入分开。** 连接保存后端端点、密钥等配置引用，不保存明文密钥；协议由 `connector_type` 指定，不按供应商名称或连接代码推断。[健康检查编排](../../orchestration/erp-supplier-catalog/src/supplier_connection_execution/health.rs)先登记运行，再在事务外只读验证，结果事务重验技术配置版本。启停控制新增下单及未发送的订单确认；目录、通知、既有订单调查和售后按各自用途验证绑定，不借用新增下单或付款资格。
3. **API 执行意图不等于履约结果。** `ApiOrderExecution` 固定正式采购修订、连接和供给修订明细，敏感请求及配送上下文只以密文落库。[erp-supplier-orders](../../orchestration/erp-supplier-orders/README.md)编排地址、配送、建单、订单确认、支付通知、取消、回调跟踪和资金保障。[发单步骤](../../orchestration/erp-supplier-orders/src/supplier_api_orders/steps.rs)在外呼前保存原动作身份、载荷指纹和可能已发送的阶段；超时或暂不可见时查原动作，不盲目重发。履约、取消与退款是独立轨道，协议受理不等于接单或退款完成。[退款执行](src/after_sales/refund.rs)核对来源及原订单可退范围；应用为正式退款事实仍需核对逐行分配，不能据此推定现金到账或核销。
4. **采集流水不等于结算过账。** [售后采集](../../orchestration/erp-supplier-orders/src/supplier_after_sales/settlement.rs)通过只读 `Settlements` 按连接和固定窗口获取扣费、退款流水，整页证据与游标同事务保存；同连接同流水异载荷阻断，缺能力则保留明确的不支持状态，不伪造结果。[erp-supply-settlement](../../orchestration/erp-supply-settlement/README.md)的[正式来源登记](../../orchestration/erp-supply-settlement/src/supply_settlement/source.rs)核对来源批次及逐行证据，与原订单、连接和退款事实关联。本域根据正式来源冻结结算明细、处理差异与补证，并校验复核主题和制单复核分离；[复核过账](../../orchestration/erp-supply-settlement/src/supply_settlement/review_posting.rs)在同一根事务协调结算单、工作项、适用的应付和成本、命令回执及审计。

## 修改执行要求

- 管理端受保护用例使用 `erp-identity::AuthorizationService`；本域仅提供对象事实与固定 Mapping，不解释角色条款、不自行拼接权限范围或构造授权结果。查询使用当前读取阶段的编译凭证，在读取前校验适用范围，交付前沿原执行器复验账号、权限、事实版本和有效期；不得跨请求或事务复用。
- 供应链去重与原结果恢复以本域命令回执为准。`supply_command_receipts` 以稳定命令 ID 唯一保护，供给交接用 `supplier_offering_handover_command_receipts`。回执与业务事实、对应业务事件同事务保存；不从审计展示解析请求指纹、版本或结果。未知 schema、缺失引用或软删除回执须失败关闭，不得当作未执行命令重跑。
- 门户申请存在 `supplier_portal_offering_applications`，与正式条款分离；提交和决定历史只追加，待确认原稿不可改。门户只按服务器已验证的供应商绑定访问，不伪造管理员或补授公司 DataScope。API 来源只读；可供命令只接收数量、有货或临时缺货及必填版本，不修改来源和关系状态。正式确认、可供写入及 `supplier_portal_command_receipts` 复用调用方事务；确认时重验会话、访问、具体工作项与单位精度，原子保存正式结果、申请、任务、审计和成功回执。同一新品供给与相同条款返回原正式版本，实质条款变化才追加修订。
- 询价内部报价按询价、需求版本、反馈轮次、需求行、供应商及供给身份追加不可变版本。询价发布不写通用供给；正式采购按精确报价及稳定余额做版本条件更新（CAS）。采购变更提交保留原数量和地址范围，仅在正式减少或作废后释放；调整须保留原历史。费用内部工作依据仅在 Process 已证明未发布的准确轮次内按同一稳定 ID 更新。
- 调整集合查询时同步评估索引；组合根复用公开索引入口并保持逐集合注册顺序。HTTP 优先复用用例 DTO；变更公开合同时核对 Process、ReadModel 和应用调用方。业务改动补充库单测，覆盖成功、失败、边界及相关幂等或版本冲突。

## 验证要求

仅修改本文档时，核对相对链接并在 `backend/` 运行 `git diff --check -- crates/domains/erp-supply/README.md`，不运行构建或测试。修改业务代码时，在 `backend/` 运行本域库单测：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib --locked
```

代码改动的其他检查见[统一质量门禁](../../README.md#质量门禁)与[后端开发规则](../../../AGENTS.md#验证)。库单测不替代真实供应商验收；本地定向验证不运行集成测试或真实外部服务测试。
