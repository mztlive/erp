# erp-connector-dangaoshushu：蛋糕叔叔协议适配器

## 职责

本 crate 实现蛋糕叔叔新版 `/dsapi` 协议：绑定服务端供应商连接，签名并发送 HTTP 请求，验证回调信封，将目录、报价、可供、配送及订单响应转换为 [`erp-supply` 定义的协议 Port](../../domains/erp-supply/src/ports/connector/mod.rs) 所接受的标准来源事实。它不读写 ERP 业务仓储，也不决定采购是否可发单、目录是否可选品或观察结果能否应用为履约。

应用负责登记运行时并注入 Port；供给编排负责业务授权与重验、技术引用绑定的持久化、敏感请求快照、执行状态和回执。HTTP 不得隐式重放；供应商写入结果未知时保留原分类，由编排调查原动作，不视为未执行。

## 目录结构

| 入口 | 用途 |
| --- | --- |
| [`Cargo.toml`](Cargo.toml)、[`lib.rs`](src/lib.rs) | 声明配置、领域合同和 HTTP 等依赖；导出运行时、绑定连接器及诊断查询类型，声明实际提供的窄 Port。 |
| [`runtime.rs`](src/runtime.rs)、[`proof.rs`](src/proof.rs) | 校验部署配置，按用途绑定连接，生成内部引用与短期票据；用 HMAC 保护绑定指纹、地址上下文和配送选项。 |
| [`transport.rs`](src/transport.rs)、[`signing.rs`](src/signing.rs)、[`wire.rs`](src/wire.rs)、[`parsing.rs`](src/parsing.rs) | 传输接口及默认 HTTP 实现、供应商签名、保精度的 JSON 解码，以及标识、金额、时间和来源快照转换。 |
| [`catalog.rs`](src/catalog.rs)、[`catalog_scan.rs`](src/catalog_scan.rs)、[`transform.rs`](src/transform.rs)、[`catalog_details.rs`](src/catalog_details.rs)、[`categories.rs`](src/categories.rs) | 商品、规格、报价、可供和可售地区读取；分页补查详情，共用规格转换与分类字典。 |
| [`delivery.rs`](src/delivery.rs)、[`order.rs`](src/order.rs) | 地址准备，配送分组、运费和时段核验，单组建单，独立支付结果通知与原单查询。 |
| [`callback.rs`](src/callback.rs)、[`read.rs`](src/read.rs) | 回调验签与补查线索；后台专用的白名单原始资料查询。 |
| [`test_support.rs`](src/test_support.rs) | 库单测使用的记录型传输替身，不参与生产装配。 |

## 架构设计

### 部署装配与连接绑定

- [服务端启动](../../../apps/erp-server/src/bootstrap.rs)读取 `SafeConfig` 快照；[组合根](../../../apps/erp-server/src/wiring/application/supplier_connectors.rs)只在 `[dangaoshushu]` 已启用时构造 `DangaoshushuRuntime`，按 `Dangaoshushu` 注册到 [`SupplierConnectorRegistry`](../../domains/erp-supply/src/connector_registry.rs)。同一运行时下的连接共享 HTTP 连接池和进程内渠道限流。[应用工厂](../../../apps/erp-server/src/wiring/application.rs)将注册表作为目录和健康网关、技术引用注册表或订单运行时注入；新链路不经旧 `SupplierGateway` 发单。
- [配置定义](../../../config/src/dangaoshushu.rs)约束 HTTPS 源地址、渠道凭据、限额及映射。测试或正式接口由部署配置决定；ERP 连接 ID、供应商身份、协议类型和业务启停来自后台连接记录，不从渠道号、连接代码或回调正文推断。[连接治理](../../orchestration/erp-supplier-catalog/src/supply_governance/supplier_api/deployment.rs)解析当前部署并应用不透明端点与凭据引用，业务记录不保存明文密钥。部署配置变更后须重启；旧绑定不符时拒绝调用，新增下单须以当前配置重新通过健康检查。部署、升级及回滚以[蛋糕叔叔接入合同](../../../../docs/dangaoshushu-connector-contract.md)为准。
- [`SupplierRuntime::bind`](src/runtime.rs)按服务端用途选择绑定。新增订单要求连接为 `Active` 且当前技术配置通过健康检查；目录、回调与原单调查不借用新增下单资格，仍拒绝已删除、协议不符或引用缺失、陈旧的连接。健康检查读取 `/dsapi/brand/brand_city_lists`，不创建订单，也不构成能力元数据或写接口的验证证据。
- [管理端处理器](../../../apps/erp-server/src/http/handler/supplier_api/dangaoshushu.rs)先经[读模型](../../read-models/erp-read-models/src/supplier_center/supplier_api/protocol.rs)按操作授权，再调用 `runtime.read` 或 `runtime.reference_tickets`。诊断只读品牌、目录、商品、可售城市、门店和配送地图，返回供应商响应的 `data`，不接受任意 URL、不入库。独立票据入口签发绑定当前连接、供应商和部署配置的五分钟端点与凭据引用票据，不返回明文凭据，也不是常规连接配置的必经步骤。

### 请求、传输与错误

协议模块选定固定路径与载荷，交给 [`Transport::send`](src/transport.rs)。默认实现按配置生成秒或毫秒时间戳，[供应商签名](src/signing.rs)对小写 SHA-1 十六进制文本再计算 MD5；GET、表单和 multipart 请求在查询串携带鉴权字段，JSON POST 则将鉴权字段放在正文。默认传输禁用重定向与自动重试，对配额等待、HTTP 请求耗时及响应大小设上限；建单请求携带配送截止时间，等待配额后再检查，过期不发送。

传输仅在 HTTP 成功且协议信封 `code` 为 `200`、存在 `data` 时交付数据。[解码层](src/wire.rs)保留字符串标识及数字精度，拒绝重复键，不让金额经浮点转换。出错时不传播上游原始错误文本；读取异常、限流、鉴权失败和明确业务拒绝按各自类别返回，写入后因通信中断或无法解析响应而不能确认结果时返回 `ResultUnknown`。`with_transport` 可替换通信边界，但仍走真实协议转换和连接绑定，替代实现也不得重放请求。

### 实际调用与事实归属

1. **目录与可供。** [目录采集流程](../../orchestration/erp-supplier-catalog/src/supplier_connection_execution/catalog_scan.rs)在事务外调用 `SupplierApiGateway::catalog_page`，[领域网关桥接](../../domains/erp-supply/src/ports/connector/runtime.rs)按目录用途绑定并调用 `OfferSource::scan_scoped`。[扫描实现](src/catalog_scan.rs)读取 `/dsapi/product/get_product_hot_lists`，按商品去重补查 `/dsapi/product/get_product_details`，有分类编号时按页共用分类字典。返回逐规格 `Snapshot<OfferChange>`、下一页游标及单项映射跳过原因；按原始商品数判断续页，整页结构、分类字典或通信失败不产出完成页。仅支持部分增量，不提供成员证据，缺项不表示下架。目录编排重验权限与连接配置，在同一事务保存观察、异常和检查点；[选品确认](../../orchestration/erp-supplier-catalog/src/supplier_catalog_adoption/confirm.rs)才建立正式商品/SKU 与供给关联。
2. **来源字段映射。** [`OfferSource`](src/catalog.rs) 用商品号与规格号定位来源，`clearing_price` 映射为直发采购报价，`market_price` 独立保留为市场参考价；不推断集采价、税率或缺失单位。`AvailabilitySource` 单次查一个规格，结合品牌、商品、规格状态与明确零库存给出可供判断，未报告数量不当作零库存；`ServiceAreas` 将供应商城市映射为公司标准地区，未映射则失败。来源快照区分观察时间和上游变更时间，缺失不补填，版本保留 `Unversioned`。
3. **配送、建单与支付结果通知。** [订单编排步骤](../../orchestration/erp-supplier-orders/src/supplier_api_orders/steps.rs)冻结动作与明细并在事务内复验采购、连接和资金门槛，外呼在事务提交后执行。`DeliverySource::prepare` 用 BD-09 收件坐标向 `/dsapi/addr/oprate_addr` 发送表单并返回绑定上下文；`options` 用 multipart 请求 `/dsapi/order/get_distribution_rules`，返回按规则分组的配送或自提选项。选项绑定地址上下文、冻结明细、配送方式、费用和有效期。编排不接受与冻结明细不符的分组；`Orders::create` 复验选项及报价后只向 `/dsapi/order/submit_order` 发送一次单组建单请求，返回外部单号、实际应付与下一步骤，不在内部拆单或支付。[独立通知步骤](../../orchestration/erp-supplier-orders/src/supplier_api_orders/payment.rs)调用 `PaymentConfirmation::confirm`，向 `/dsapi/order/order_pay_result` 发送 JSON；通知不是 ERP 付款、银行交易或预占订单确认。实际应付超出批准范围时由编排保留原单并阻断通知。
4. **原单恢复。** 建单或通知结果未知时，编排按原我方单号查询，适配器的 `Orders::order` 也接受供应商单号。它读取 `/dsapi/order/order_details`，核对渠道及所查订单号，取得 `OrderSnapshot` 或 `Lookup::NotVisible`。暂不可见不等于未创建，不重发。地址准备没有按原动作补查能力，结果未知须人工核验。
5. **推送回调。** [HTTP 回调入口](../../../apps/erp-server/src/http/routes/supplier_callbacks.rs)把原报文交给[回调接收编排](../../orchestration/erp-supplier-orders/src/supplier_callbacks/mod.rs)，按服务端连接选择协议后调用 `Callbacks::verify`。它校验渠道、签名和时间窗口；签名不覆盖业务正文，只返回 `EnvelopeOnly` 和订单、商品、可供、地区或价格的补查线索，不生成业务观察。接收编排加密正文并保存回执及补查线索，或核对重复请求的原回执后，才返回成功应答。[订单跟踪](../../orchestration/erp-supplier-orders/src/supplier_order_tracking/callbacks.rs)和[商品回调消费者](../../orchestration/erp-supplier-catalog/src/supplier_catalog_callbacks/mod.rs)再补查并按各自规则处理；应答不代表业务已应用。

能力以 [`DangaoshushuConnector` 实际提供的窄 Port](src/lib.rs) 为准：商品与价格、可供、可售地区、配送、建单与查单、支付结果通知、推送回调。没有主动取消、单笔退款或结算流水能力，也不提供拉取消息、供应商资金证据、预占订单确认或收货通知；查到取消状态不代表能发起取消。技术能力不授予业务权限。

## 验证要求

按[后端开发规则](../../../AGENTS.md#验证)，仅改本文档时，在 `backend/` 对本文件运行 `git diff --check -- crates/adapters/erp-connector-dangaoshushu/README.md`，不调用服务、不编译、不运行测试。
