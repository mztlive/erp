# erp-connector-jd-vop

京东 VOP 的供应商协议适配库。它把 ERP 的类型化请求转换成京东接口调用，再把响应转换成供应链领域可消费的来源事实。

## 职责

- 实现 `erp-supply` 定义的[供应商连接器 Port](../../domains/erp-supply/src/ports/connector/mod.rs)和[京东认证 Port](../../domains/erp-supply/src/ports/jd_vop/mod.rs)：核对连接与账号配置的绑定，处理授权码交换、令牌刷新和网关签名，按实际装配提供商品、地址、订单、物流、消息及资金读取等能力。
- 解释京东的响应封装、结果码和字段，交付标准快照、回执或脱敏的分类错误。来源事实不等于 ERP 的采购批准、库存变动、实收或记账结论。
- 本库不直接访问 ERP 仓储。应用负责运行时装配；业务编排负责操作人授权重验、写意图和业务回执持久化、加密保存凭据，以及将来源事实应用到 ERP。

## 目录结构

| 源码 | 用途 |
| --- | --- |
| [`lib.rs`](src/lib.rs)、[`runtime.rs`](src/runtime.rs)、[`configured.rs`](src/configured.rs) | 公开入口、可选能力、连接绑定、健康检查，以及读取、执行和消息能力的装配。 |
| [`authorization_http.rs`](src/authorization_http.rs)、[`authorization_protocol.rs`](src/authorization_protocol.rs)、[`oauth_http.rs`](src/oauth_http.rs)、[`oauth.rs`](src/oauth.rs) | 页面授权、回调地址校验、授权码换令牌，以及刷新请求和令牌响应解析。 |
| [`transport.rs`](src/transport.rs)、[`http_transport/`](src/http_transport/mod.rs)、[`request_encoding/`](src/request_encoding/mod.rs)、[`signing.rs`](src/signing.rs) | 区分读写请求白名单，共享请求配额，按方法编码和签名，执行单次 HTTP 发送。 |
| [`wire.rs`](src/wire.rs) 和 `wire_*.rs` | 解码网关与 RPC 封装，定义各接口的响应结构，保留金额和长编号的精度。 |
| [`catalog.rs`](src/catalog.rs)、[`transform.rs`](src/transform.rs) | 扫描授权商品池，合并商品详情、报价、图片和类目，返回标准供货资料、游标及商品池成员证据。 |
| [`address.rs`](src/address.rs)、[`delivery.rs`](src/delivery.rs)、[`preflight.rs`](src/preflight.rs) | 查询与复核四级地址，核验配送选择及下单条件。 |
| [`order.rs`](src/order.rs)、[`order_payload.rs`](src/order_payload.rs)、[`order_mapping.rs`](src/order_mapping.rs)、[`order_dispatch.rs`](src/order_dispatch.rs)、[`logistics.rs`](src/logistics.rs)、[`receipt.rs`](src/receipt.rs) | 预占下单、订单确认、取消及收货通知；构建原单拓扑，分别映射发货数量和物流轨迹。 |
| [`messages.rs`](src/messages.rs)、[`funds.rs`](src/funds.rs) | 轮询与确认消息，读取预存款余额和流水、封装资金证据。 |

## 架构设计

### 应用装配与能力边界

[服务端启动](../../../apps/erp-server/src/bootstrap.rs)从启动配置快照取值，连接应用数据库后，由[供应商连接器装配](../../../apps/erp-server/src/wiring/application/supplier_connectors.rs)注入 MongoDB 凭据提供器、OAuth 刷新客户端和网关传输，并注册 `JdVopRuntime`。ERP 连接用端点和凭据引用绑定部署配置，不保存京东密钥或令牌；运行时核对原连接、供应商、部署账号及配置绑定，拒绝已删除的连接和陈旧或不匹配的引用。

启用[京东技术配置](../../../config/src/jd_vop.rs)默认只装配目录和地址读取。已核实的账号执行资料用于开放订单执行能力；新增下单和预占确认还需要已核实的确认资金规则。新增下单另要求 ERP 连接已启用自动下单、当前技术配置通过健康检查，并绑定核实的外部预存款账号。关闭自动下单不切换账号，也不阻断仍具备相应能力的原单跟踪。消息订阅独立装配，拉取和删除消息不授予订单写权限；资金查询需要与执行账号一致的资金资料。配置或持有令牌都不证明账号已获全部接口权限，商品池读取健康检查也不替代完整能力核验。

### 从页面授权到网关请求

1. [HTTP 状态](../../../apps/erp-server/src/http/state.rs)将 `ReqwestAuthorizationHttp` 注入 ERP 页面授权流程。[授权存储](../../orchestration/erp-supplier-orders/src/supplier_credentials/authorization/store.rs)在获取授权码和换取令牌之前重验操作人权限及原连接绑定，并在事务内登记发送阶段。适配器按京东协议准备授权参数，向固定授权端点 `authorizeForVOP` 请求，从 `302` 的 `Location` 中核对回调地址并取授权码，不访问回调地址，再向 `access_token` 端点换取令牌。[授权流程](../../orchestration/erp-supplier-orders/src/supplier_credentials/authorization/engine.rs)把返回的令牌加密保存到原凭据记录。
2. 业务调用按部署凭据标识向[令牌提供器](../../orchestration/erp-supplier-orders/src/supplier_credentials/provider.rs)取令牌，网关传输校验令牌期限和账号范围。进入刷新窗口时，提供器按 MongoDB 记录版本争抢续期资格，先登记发送，再通过 `OAuthHttp` 单次刷新。刷新结果未知时不沿用旧令牌，也不在租期结束后重发原刷新请求。错误摘要不回传密钥、令牌、敏感请求 URI 或原始错误正文。
3. 以商品池读取为例，`JdVopReadRequest::Pools` 选定 `jingdong.vop.goods.getSkuPoolInfo`，编码器将部署渠道放入该方法的请求对象。传输用固定 UTC+8 时间生成系统参数；签名按参数名排序，对原值拼接计算大写 MD5，再将同一份业务 JSON 和签名进行表单编码。随后由[生产发送器](src/http_transport/sender.rs)执行一次 POST，禁止重定向和隐式重试，并限制响应大小。读写共用运行时的并发和速率限制；获取并发许可、等待限速和执行传输共用一次请求的超时预算，响应解码在该预算之外。HTTP 成功不等于业务成功，仍须核对请求对应的网关及 RPC 结果。

### 来源事实与 ERP 写入

- **同步目录**：适配器全量扫描授权商品池（不支持增量），按 SKU 读取详情、数量为 1 的报价、图片及适用的类目资料。商品池成员证据与报价分开，失败页不推进游标，不能因池中缺项直接判定商品撤回。[目录编排](../../orchestration/erp-supplier-catalog/src/supplier_connection_execution/catalog_scan.rs)在事务外取数，在事务内保存来源资料和检查点。
- **采购下单与履约**：[订单编排](../../orchestration/erp-supplier-orders/src/supplier_api_orders/steps.rs)按冻结采购版本核验资格及 ERP 预存款保障，提交写意图后才在事务外调用适配器。适配器按冻结行复核四级地址、可售状态、库存、单价、运费和京东余额，再单次发送预占下单。京东余额不能代替 ERP 的预存款占用。[预占确认](../../orchestration/erp-supplier-orders/src/supplier_api_orders/confirmation/execution.rs)按当前原单及子单事实单独登记和发送，不重新下单。查询时按原单号补齐父子拓扑，只以核实的最终子单映射发货数量，不重复累计父单，也不从运单推算数量。[收货通知](../../orchestration/erp-supplier-orders/src/supplier_api_orders/receipt/dispatch.rs)由 ERP 实收证据触发并保存回执；适配器通知京东厂家直送子单，不用物流妥投代替 ERP 实收。
- **消息与资金**：京东消息是补查线索，不是扣款或退款凭证。[消息编排](../../orchestration/erp-supplier-orders/src/supplier_messages/mod.rs)先保存加密消息和补查意图，应用补查结果后再按原消息号单次确认（远端删除）。适配器按已核实的资金口径返回余额、流水和加密证据；[资金调查流程](../../orchestration/erp-supplier-orders/src/supplier_deposits/investigation/worker.rs)核对原账号及证据，由 ERP 财务规则决定如何入账或对账。
- **未知结果**：京东写入没有已核实的重放保障。进入单次写发送流程后，排队或发送超时、传输失败、回执无法解码或不足以确认结果，都保留 `ResultUnknown`；消息删除使用 `MessageAcknowledgement::Unknown`。业务侧保留原动作，按原第三方订单号或京东单号查询订单、收货状态或取消进度；暂不可见不等于未执行，不得据此重发。消息删除结果未知时同样保留记录，不重复删除。
