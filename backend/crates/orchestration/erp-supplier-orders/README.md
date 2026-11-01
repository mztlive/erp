# erp-supplier-orders

供应商正式订单执行、回调和消息接收、授权凭据持久化及资金保障编排属于本 crate。

- 原订单执行身份、采购修订、请求快照、未知结果核查和回执顺序必须保持不变。
- 同一业务事务必须沿用调用方 `Executor`；供应商 HTTP 调用必须在事务之外。
- 具体协议从 `erp-supply` Port 和应用注册表注入，不依赖具体 connector crate。
- 目录消息应用通过 `SupplierMessageCatalog` 注入；资金事实后的原付款任务同步通过 `OrdersPaymentTasks` 注入。
- 直发准备与正式履约复用 `erp-fulfillment-execution`，不得由履约编排反向依赖本库。
- 冻结预算反序列化和身份校验使用供给域完整请求快照合同，客户端金额不得作为执行依据。

验证使用内联库单元测试；禁止集成测试、ignored 测试、验收 examples 和真实外部服务。
