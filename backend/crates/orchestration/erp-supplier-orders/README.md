# erp-supplier-orders

## 职责

本 crate 将已生效的采购修订转成真实的供应商 API 订单，并按原执行记录推进地址准备、配送核验、创建、确认、取消和收货通知。它还编排订单状态补查、供应商回调与拉取消息、预存款保障与资金调查、退款及结算来源采集，以及供应商授权凭据的持久化和刷新。

本 crate 负责跨域操作的顺序、事务和失败恢复，不承载服务端 HTTP 路由或供应商协议实现，也不把供应商应答直接当作 ERP 发货、实付或核销。手工供给不进入 API 下单队列。

## 目录结构

公开入口与错误类型见 [`src/lib.rs`](src/lib.rs) 和 [`src/errors.rs`](src/errors.rs)。

| 模块 | 生产入口与用途 |
| --- | --- |
| [`supplier_api_orders`](src/supplier_api_orders/mod.rs) | 采购执行意图入队、冻结请求、正式下单及原动作恢复；含预占确认、支付结果通知、取消和收货通知。 |
| [`supplier_order_tracking`](src/supplier_order_tracking/mod.rs) | 认领到期原单、只读补查，保存供应商状态与物流观察，按领域规则应用包裹、发货和检查点。 |
| [`supplier_deposits`](src/supplier_deposits/mod.rs) | 预存款账户和占用管理、发单资金资格、资金流水核查与账务应用。 |
| [`supplier_callbacks`](src/supplier_callbacks/mod.rs) | 按服务端连接绑定选择协议，验证并保存推送证据及补查意图，再返回协议应答。 |
| [`supplier_messages`](src/supplier_messages/mod.rs) | 按账号拉取消息，保存证据，处理定向补查，最后向供应商确认。 |
| [`supplier_after_sales`](src/supplier_after_sales/mod.rs) | 按原动作发送和查询退款；分页采集结算证据，不把请求受理当作退款完成。 |
| [`supplier_credentials`](src/supplier_credentials/mod.rs) | 京东 VOP 凭据导入、授权、加密保存与版本化 OAuth 刷新。 |

## 架构设计

### 领域、编排与协议适配

- [供给域 `erp-supply`](../../domains/erp-supply/README.md) 持有连接、订单执行状态、供应商履约及结算规则，并定义[供应商协议 Port](../../domains/erp-supply/src/ports/connector/README.md)。采购生效修订由采购域管理；预存款账户、占用和账务规则由[财务域](../../domains/erp-finance/src/service/supplier_deposit/mod.rs)管理；回调及消息接收记录由集成域管理。
- 本 crate 用领域服务、仓储和 Port 完成跨域编排。同一业务事务的持久化步骤沿用调用方提供的 `Executor`，不在事务中发送供应商 HTTP 请求。直发草稿准备和正式直发过账复用 [`erp-fulfillment-execution`](../erp-fulfillment-execution/README.md)，依赖不反向指向本库。
- [服务端组合根](../../../apps/erp-server/src/wiring/application/supplier_connectors.rs)将部署配置启用的协议适配器（如[蛋糕叔叔](../../adapters/erp-connector-dangaoshushu/README.md)、[京东 VOP](../../adapters/erp-connector-jd-vop/README.md)）注册到 `SupplierConnectorRegistry`。适配器负责供应商 HTTP、协议验签、请求与响应转换及失败分类；本库按连接和可用能力选择 Port，不依赖具体 connector crate，也不因注册了协议就假定具备写能力。[应用注入点](../../../apps/erp-server/src/wiring/application/supplier_operations.rs)还通过 `SupplierMessageCatalog` 接入目录应用，通过 `OrdersPaymentTasks` 在资金事实应用后同步原付款任务。

### 下单：先提交意图，再调用供应商

1. **采购正式化。** [采购正式化入口](../erp-procure-to-pay/src/procure_to_pay/review.rs)在调用方事务里调用 [`enqueue_purchase`](src/supplier_api_orders/enqueue.rs)。API 供给按连接、采购行和冻结收件分配分组；执行 ID 由生效采购修订及分组派生，已有同 ID 的意图不另建。此时不调用供应商。
2. **冻结请求并发送。** [后台执行](src/supplier_api_orders/execution.rs)使用原采购、供给修订和收件分配构建[加密请求快照](src/supplier_api_orders/snapshot.rs)，并按阶段准备地址、核验配送和执行下单预检。[创建步骤](src/supplier_api_orders/steps.rs)在外呼前冻结原动作身份和载荷指纹，在认领事务内复验采购修订、连接与资金条件并登记 `CreationUnknown`。事务提交后才调用 `Orders::create`；收到可核实的创建结果后，在另一个本地事务中一起保存供应商单号和执行进度。实际应付超出批准范围时保存原单引用并阻断后续确认。
3. **按原动作恢复。** 创建结果未知时，按原我方订单号和冻结明细查询；暂不可见不等于未创建，不能据此重发；本地意图不等于供应商侧的幂等保证。[预占订单确认](src/supplier_api_orders/confirmation/execution.rs)、[支付结果通知](src/supplier_api_orders/payment.rs)及[取消](src/supplier_api_orders/cancellation.rs)各有独立动作和结果轨道。支付结果通知不登记 ERP 付款或银行交易。[收货通知](src/supplier_api_orders/receipt/mod.rs)以已提交的 ERP 实收为依据，发送前在本地事务复验实收并登记针对原供应商子单的通知动作，结果未知时只查询原目标。详见[协议下单与恢复合同](../../domains/erp-supply/src/ports/connector/README.md#下单与恢复)。

### 订单状态：先留存观察，再应用履约

[订单跟踪](src/supplier_order_tracking/mod.rs)先在[本地事务](src/supplier_order_tracking/persistence.rs)认领到期记录，再在事务外按原连接、外部订单号和冻结明细查询，核对返回的我方订单号和外部订单号。来源观察先保存，再在事务中推进供应商状态和最新观察指针；[履约应用](src/supplier_order_tracking/delivery.rs)在写事务内复验认领资格及当前观察，将符合条件的包裹、ERP 发货和应用检查点一起提交。仓库收货仍以仓库确认的实际收货为准。查询成功不等于 ERP 履约已应用；已被采购变更替代的执行不能应用迟到结果。

### 资金：发单资格不等于实付

- [发单资金门槛](src/supplier_api_orders/funds.rs)按协议区分两类资金资格：按采购冻结的先款后货条件核验有效已过账付款，或核验供应商预存款。预存款路径在发单前核对原执行组的资金覆盖；[自动占用](src/supplier_deposits/automatic.rs)依据服务端的预存款账户、原付款批准及自动授权，不接受客户端指定的执行金额。[预算](src/supplier_api_orders/budget.rs)由完整的生效采购修订和分组计算；读取已冻结预算时，解密服务端快照，按[供给域完整请求快照合同](../../domains/erp-supply/src/api_order/request_snapshot.rs)反序列化，并核对执行组、采购修订与快照总额。缺少预算或身份不符时不使用客户端金额兜底。
- [资金调查](src/supplier_deposits/investigation/process.rs)在事务外按原账号、窗口和游标查询供应商流水，整页证据、处理意图及读取检查点在[同一事务](src/supplier_deposits/investigation/store.rs)保存。余额单独采样，不凭余额差值、空查询或消息正文认定扣款或充值。[正常扣款应用](src/supplier_deposits/investigation/application.rs)使用同一 `Executor` 完成资金事实登记、应付核销、原付款任务同步和应用回执；提交结果未知不进入[明确失败后的异常处理](src/supplier_deposits/investigation/apply.rs)。

### 回调与消息：接收应答和业务完成是两件事

- [推送接收](src/supplier_callbacks/mod.rs)以服务端登记的连接选择协议，调用适配器验证原请求。[接收实现](src/supplier_callbacks/intake.rs)记录协议报告的完整性级别；即使适配器报告正文已认证，当前入口也只接受可补查的通知，不接收可直接应用的业务观察。首次接收先保存加密正文及补查意图，再返回成功应答；重复键须核对原回执及报文证据，一致后才应答。[订单回调消费者](src/supplier_order_tracking/callbacks.rs)安排原单补查，并根据查询与履约应用检查点结束处理，而不是用回调正文改订单状态。参见[协议回调接收合同](../../domains/erp-supply/src/ports/connector/README.md#回调接收)。
- [拉取消息](src/supplier_messages/mod.rs)不沿用推送验签语义：在事务外拉取，按账号和连接在[接收事务](src/supplier_messages/persistence.rs)保存证据及补查意图；补查处理成功后，先保存确认发送状态，再在事务外向远端确认。恢复时将未完成的确认发送保留为未知，不把它当成成功或重新发送。

### 售后与凭据

[退款](src/supplier_after_sales/refund_dispatch.rs)先保存原动作再外呼，之后查询原退款结果；[结算采集](src/supplier_after_sales/settlement.rs)在事务外取页，在同一事务保存证据及游标。[OAuth 刷新](src/supplier_credentials/provider.rs)先持久化版本化的认领与发送状态，在审计事务外请求供应商，再保存结果；保存失败或提交结果不明时读取当前凭据记录，不以此重发刷新请求。

## 验证要求

验证使用库单元测试（测试模块随源码维护）；[`Cargo.toml`](Cargo.toml) 禁用自动发现集成测试。按[后端开发规则](../../../AGENTS.md#验证)，不新增、修改或运行集成测试，不运行 ignored 测试、验收 examples 或依赖真实数据库和外部服务的测试。仅修改本文档时，只对本文件运行 `git diff --check`。
