# erp-integration：集成消息与差异处理

`erp-integration` 是集成事实的拥有领域。它记录外部消息收到了什么、处理在哪里失败、对账发现了什么差异，以及依据哪些证据作出了决定。这些记录不能由日志替代，也不能代替供给、订单、退款或财务领域的正式单据。

## 职责

### 负责的数据与能力

| 本域集合 | 保存的事实 |
| --- | --- |
| `inbox_messages` | 通用入站信封：来源系统、来源事件号、消息类型、业务事实键、载荷版本及引用、收发时间和处理结果。 |
| `integration_error_tasks` | 消息或业务对象的失败分类、处理责任、状态、尝试次数、最近尝试时间和脱敏摘要。投递与重试进度由任务表达，不另设投递状态机或尝试集合。 |
| `reconciliation_differences`、`reconciliation_difference_resolutions` | 差异对象、分类和至少一侧的不可变事实引用；决定历史只追加，当前处理状态与决定版本从末条决定派生。 |
| `integration_command_receipts` | W29 集成处置命令的幂等身份、请求指纹和可重放结果，不保存原始幂等键。 |
| `supplier_callback_receipts` | 推送接收证据：绑定连接、认证范围、可选来源事件号、原始正文密文、待补查目标及处理进度。没有可靠来源事件号时，本地接收号不能冒充来源事件号。 |
| `supplier_pulled_messages`、`supplier_message_consumers` | 拉取消息的加密证据、补查及远端确认状态，以及账号唯一消费连接和租约。拉取记录不具有已验签回调的语义。 |
| `supplier_credentials` | 京东 VOP 技术凭据的账号与应用绑定、令牌密文以及配置、授权和续期状态。领域不解析明文令牌；已发送且结果未知的续期不因租约到期而重发。 |

## 目录结构

### 代码入口

| 路径 | 用途 |
| --- | --- |
| [`Cargo.toml`](Cargo.toml)、[`src/lib.rs`](src/lib.rs) | 依赖声明和公开模块，导出 `IntegrationOpsService`、`IntegrationAccess`、`ScopedIntegrationList` 和错误类型。 |
| [`src/entity/`](src/entity/mod.rs) | 实体、值对象和确定性规则。[`integration_ops`](src/entity/integration_ops/mod.rs) 管消息、任务、差异、责任规则和证据引用；供应商的[回调](src/entity/supplier_callback.rs)、[拉取消息](src/entity/supplier_message.rs)、[消费绑定](src/entity/supplier_message_consumer.rs)及[技术凭据](src/entity/supplier_credential/mod.rs)单独建模。 |
| [`src/dto/`](src/dto/mod.rs) | 请求、视图、查询条件归一化和命令输入准备，供应用、编排和读模型复用。 |
| [`src/service/`](src/service/mod.rs) | `IntegrationOpsService` 提供本域授权查询；消息、错误任务、差异和 [`task_decision`](src/service/task_decision/mod.rs) 模块提供领域对象准备、版本与证据校验，以及调用方事务内的写入步骤。 |
| [`src/ports/`](src/ports/mod.rs) | [`IntegrationEvidenceAuthority`](src/ports/evidence.rs) 定义原结果查询、原消息重排队、归集核验和受控证据发现与复验的合同。 |
| [`src/repository/`](src/repository/mod.rs)、[`src/indexes/`](src/indexes/mod.rs) | 本域 MongoDB 集合访问、过滤分页、批量读取与索引声明。启动和 CLI 复用 `indexes::ensure`；技术凭据另有 [`supplier_credential::ensure`](src/indexes/supplier_credential.rs)，供明确导入等写入入口建立索引。 |

## 架构设计

### 依赖与协作边界

- **领域负责规则，Process 负责跨域写入。** [Cargo.toml](Cargo.toml) 中的 `erp-identity` 是共享授权例外；常规、构建和测试依赖均不得指向其他业务领域、编排、协议适配或应用。跨域协作使用 Port 或显式传入的事实。以 [erp-supplier-catalog](../../orchestration/erp-supplier-catalog/README.md) 的 [`IntegrationResolutionProcess`](../../orchestration/erp-supplier-catalog/src/integration_resolution/mod.rs) 为例：它核对授权和正式工作项关联，在同一事务中协调本域事实、`erp-workflow` 工作项、成功审计及适用的命令回执。仓储和证据端口复用原 `Executor`，不自行开启事务；外部 HTTP 不放在事务内。
- **本域定义证据，权威数据由拥有方提供。** 应用[组合根](../../../apps/erp-server/src/wiring/application.rs)将编排层的 [`MongoIntegrationEvidenceAuthority`](../../orchestration/erp-supplier-catalog/src/integration_resolution/evidence_adapter.rs) 注入命令及读模型。提交的引用必须重验类型、记录存在性、状态和业务关联；未注册的证据或无法证明的结果不能据此终结。若需要更正、补偿或重放正式业务，应走拥有领域的入口，集成决定只关联核实后的结果，不直接改写单据。
- **协议负责外部交互，接收证据不等于业务应用。** [erp-supply](../erp-supply/README.md) 持有供应商连接配置和[连接器能力合同](../erp-supply/src/ports/connector/mod.rs)。`adapters/` 下的具体协议实现验签、报文转换和传输；[erp-supplier-orders](../../orchestration/erp-supplier-orders/README.md) 等 Process 负责接收、补查、凭据操作及跨域应用，应用负责注册协议并启动后台任务。本域不直连供应商，也不持有正式供给、订单或财务事实。
- **HTTP 只做协议接入，跨域展示由读模型完成。** [集成管理 handler](../../../apps/erp-server/src/http/handler/integration_ops/mod.rs) 复用 DTO，写命令交给 Process，入站列表与详情交给本域服务。[集成中心读模型](../../read-models/erp-read-models/src/integration_center/mod.rs) 组合错误任务与差异详情，列表复用本域在授权范围内读取的根记录，在同一读取阶段补齐展示名并复验凭证后交付。

术语与统一分工见[总索引](../../README.md#代码中常见名称的含义)。

### 使用场景

#### 登记消息并回写失败

1. [`register_inbox_message`](../../orchestration/erp-supplier-catalog/src/integration_resolution/inbox_message.rs) 在事务内先核验操作权限并检查来源系统是否存在，再调用本域 [`prepare_registered_inbox_message` / `persist_inbox_message`](src/service/inbox_message.rs) 写入 `received` 信封。`(source_system_id, source_event_id)` 与 `(message_type, business_fact_key)` 分别约束消息和业务事实去重，重复登记由[唯一索引](src/indexes/integration_ops.rs)返回冲突，不先查再插。
2. 同一 Process 的 `write_back_inbox_result` 校验期望版本：`processed` 写完成时间；`failed` 需要错误分类，生成关联任务，有脱敏摘要时记录一次尝试。仓储 [`create_error_task_with_message_failure`](src/repository/integration_ops.rs) 先插入任务、再按版本更新消息；[编排](../../orchestration/erp-supplier-catalog/src/integration_resolution/creation_writes.rs)在同一事务补写正式工作项和审计。同消息同分类已有进行中任务或消息版本过期时，整笔失败，不留下半套记录。
3. 错误任务也可脱离消息登记：[手工入口 `create_error_task`](../../orchestration/erp-supplier-catalog/src/integration_resolution/error_task.rs) 仅在提交了消息 ID 时核对其存在；例如[供应商连接健康检查失败](../../orchestration/erp-supplier-catalog/src/supplier_connection_execution/failure.rs)关联连接而不是伪造信封。临时故障和限流属于可自动重试分类；`result_unknown` 必须先查原结果。对错误任务，只有确认无结果且分类允许时才可请求重放；当前[权威实现](../../orchestration/erp-supplier-catalog/src/integration_resolution/evidence_adapter.rs)还会重验已登记的消息类型、状态及已知结果，沿原消息身份改回 `received`，不直接重发外部命令，也不视为已解决。

#### 登记并处置对账差异

1. 例如发现 `amount_mismatch`，由 [`create_difference`](../../orchestration/erp-supplier-catalog/src/integration_resolution/reconciliation_difference.rs) 检查权限与处理人组织，调用本域准备差异事实，再与正式工作项、审计一并提交。对象类型、对象 ID 和差异类型组合唯一，至少提供一侧事实引用；后续处置不改写差异内容，转交时可同步责任人和组织。列表按[最新决定](src/service/reconciliation_difference.rs)投影状态和版本，尚无决定时状态为空、版本为 `0`。
2. 已绑定正式工作项时，[`apply_task_action`](../../orchestration/erp-supplier-catalog/src/integration_resolution/task_decision/action.rs) 执行查原结果、补证或关联补偿等非终结动作，不关闭任务。[`complete_task`](../../orchestration/erp-supplier-catalog/src/integration_resolution/task_decision/complete.rs) 重验当前版本及终态证据，追加差异决定并在同一事务完成工作项；例如金额差异的[完成策略](src/entity/integration_ops/decision_policy.rs)要求补偿结果和财务对账证据。开始、退回、转交以及重复或误派关闭走正式工作项（W02）入口，[关闭](../../../apps/erp-server/src/wiring/workflow/w29_close.rs)不等于业务已解决。
3. 对确实没有绑定正式工作项的差异，[`decide_difference`](../../orchestration/erp-supplier-catalog/src/integration_resolution/task_decision/direct.rs) 只追加决定，不创建或完成工作项；终结结论必须符合原因注册表及证据要求。两条路径均由本域 [`task_decision`](src/service/task_decision/mod.rs) 校验开放状态、主题版本和证据。W29 动作、完成和直接决定按原幂等键查[命令回执](src/repository/command_receipt.rs)，重新授权后才能返回已提交结果；同键异参拒绝，提交未知时不换键重做。

### 协作示例

- **蛋糕叔叔推送。** [HTTP 回调路由](../../../apps/erp-server/src/http/routes/supplier_callbacks.rs) 接入 [`supplier_callbacks::receive`](../../orchestration/erp-supplier-orders/src/supplier_callbacks/mod.rs)，按服务端连接绑定选择[协议实现](../../adapters/erp-connector-dangaoshushu/src/callback.rs)。蛋糕叔叔只认证信封（`envelope_only`），正文只提取补查线索。接收编排加密正文、保存回执，或核实重复请求的原回执后，才返回协议成功应答。此时 `received` 仍表示等待补查；[商品回调](../../orchestration/erp-supplier-catalog/src/supplier_catalog_callbacks/mod.rs)按检查点补查后应用，[订单跟踪](../../orchestration/erp-supplier-orders/src/supplier_order_tracking/callbacks.rs)按原连接定位订单，补查和应用完成后才结束回执。失败保留进度或延期，不能拿推送正文直接覆盖正式事实。
- **京东 VOP 拉取。** [协议适配器](../../adapters/erp-connector-jd-vop/src/messages.rs)实现拉取和远端删除；[`SupplierMessageProcess::run`](../../orchestration/erp-supplier-orders/src/supplier_messages/mod.rs) 在账号租约下先恢复本地记录，再拉取、加密落库和补查。同账号同来源消息号重复不重置进度，载荷不同时保留冲突证据并隔离。补查失败延期；成功应用后才进入待确认，先持久化发送状态再调用远端删除，发送中断或结果未知记为 `ACK_UNKNOWN`，不自动重发删除。技术凭据的导入与续期由 [`supplier_credentials`](../../orchestration/erp-supplier-orders/src/supplier_credentials/mod.rs) 编排，与拉取收件和正式业务授权分开。

## 修改执行要求

1. 将无 I/O 的校验、状态迁移和不变式放入实体或值对象；Service 组织本域查询和写入，跨域原子写入由 Process 控制事务。Repository 使用调用方的 `persistence_core::Executor`。
2. 变更集合查询时同步评估索引和历史数据。组合根使用本域公开索引入口，保持既有逐集合注册顺序。`supplier_callback_receipts` 仅增量创建集合和索引，不设 TTL，回滚保留接收证据与检查点，不把 `received` 标为已处理。蛋糕叔叔的认证、应答、索引和启停按[接入合同](../../../../docs/dangaoshushu-connector-contract.md)执行。
3. HTTP 优先复用本域 DTO；变更公开合同须检查 Process、ReadModel 和应用调用方。业务改动补充库单元测试，覆盖成功、失败、边界及相关幂等或版本冲突。

## 验证要求

在 `backend/` 执行。仅文档改动只运行涉及文件的 `git diff --check`。代码改动按[统一质量门禁](../../README.md#质量门禁)检查，库单元测试命令为 `env -u ERP_TEST_MONGO_URI cargo test -p erp-integration --lib --locked`；不执行集成测试或真实外部服务测试。
