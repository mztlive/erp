# 蛋糕叔叔接入合同

## 1. 所有权与启用范围

1. `erp-supply::ports::connector` 定义供应商无关协议；具体客户端由 `erp-processes::connectors::dangaoshushu` 实现。领域 crate 不得依赖该客户端。
2. Web API 通过 `SafeConfig` 的启动快照装配一个固定连接实例。实例的连接、供应商、环境及凭据不得由业务请求或回调正文替换。
3. 当前运行入口包含技术引用票据签发、连接健康检查、授权只读目录查询，以及推送验签和加密接收。商品查询不创建公司商品，不写入正式供给、商业条款或可供投影。
4. 客户端提供地址准备、配送方案、单组订单创建、独立支付结果通知和订单查询。正式 ERP 履约尚未装配这些分步能力；旧 `SupplierGateway` 继续失败关闭。不得通过旧 `dispatch` 隐藏地址、创建与支付三个动作。
5. `catalog_sync` 返回能力缺口 `DGSS_CATALOG_APPLY_NOT_CONFIGURED`。回调接收记录停留在 `received`，当前没有自动补查及正式业务应用 worker。不得将目录读取、成功应答或健康检查解释为商品已同步或订单状态已生效。
6. 取消、单笔退款和结算流水能力保持未提供。订单累计退款金额不形成正式退款事实，不进行自动成本冲减。

## 2. 配置合同

完整模板位于 [backend/config.toml.example](../backend/config.toml.example)。本地使用已忽略的 `backend/config.toml`；部署使用既有受控 Nacos 配置。禁止另建环境变量凭据通道或把真实密钥写入示例、业务表、文档、普通日志。

```toml
[dangaoshushu]
enabled = false
connection_id = ""
supplier_id = ""
environment = "testing"
base_url = "https://dev.dangaoss.cn"
channel_no = ""
private_key = ""
user_id = ""
timestamp_unit = "milliseconds"
timeout_seconds = 15
callback_max_skew_seconds = 300
requests_per_second = 5
clearing_price_is_tax_inclusive_cny = false

[dangaoshushu.spec_units]
# "<supplier-spec-id>" = "个"

[dangaoshushu.city_regions]
# "<supplier-city-id>" = "<company-region-code>"
```

| 字段 | 执行约束 |
| --- | --- |
| `enabled` | 缺省为 `false`；整节缺省或关闭时不装配蛋糕叔叔运行时 |
| `connection_id`、`supplier_id` | 启用时必填，必须分别匹配 ERP 连接 ID 与该连接的供应商账户 ID；每项为 1 至 128 个 ASCII 字母、数字、`-`、`_`、`.` |
| `environment` | 仅 `testing` 或 `production`；必须与 ERP 连接环境一致 |
| `base_url` | HTTPS origin，不得含用户名、密码、接口路径、查询参数或片段；`production` 禁止使用 `dev.dangaoss.cn` |
| `channel_no` | 启用时必填，使用供应商分配的渠道号；字符限制与连接 ID 相同 |
| `private_key` | 启用时必填，不得含空白或控制字符，不得使用占位值；调试输出整体脱敏 |
| `user_id` | 只读查询可留空；准备地址前必须填写供应商允许使用的固定用户标识，字符限制与连接 ID 相同 |
| `timestamp_unit` | `seconds` 或 `milliseconds`，缺省为毫秒；出站按该项生成，入站只接受 10 位秒或 13 位毫秒时间戳 |
| `timeout_seconds` | 1 至 30，缺省 15；连接超时为 `min(timeout_seconds, 5)` 秒 |
| `callback_max_skew_seconds` | 1 至 900，缺省 300；推送时间与接收时间差超限即拒绝 |
| `requests_per_second` | 1 至 100，缺省 5；单进程实例限额，等待配额最多 `timeout_seconds` 秒，超时返回限流；不自动重试外部请求 |
| `clearing_price_is_tax_inclusive_cny` | 默认 `false`；采购核实 `clearing_price` 为人民币含税供货价后才能设为 `true` |
| `spec_units` | `spec_id` 到公司 SKU 计量单位的明确映射；单位为 1 至 32 个字符，不得有首尾空白；缺项时保留单位未知并不生成报价 |
| `city_regions` | 供应商 `city_id` 到公司标准地区编码的映射；键和值遵循 ID 字符限制，值必须唯一以保证可逆；缺少查询涉及的地区时拒绝映射 |

1. `enabled=true` 时加载校验身份、HTTPS 地址、凭据、限额和映射；配置错误须阻止运行时装配。关闭模板允许保留空字段。
2. 所有字段按进程启动快照使用。修改文件或 Nacos 后必须重启 Web API；Nacos 刷新不重新装配供应商客户端，也不关闭已运行的实例。
3. 调整端点、渠道、密钥、绑定身份及参与技术指纹的设置后，须重新签发并绑定技术引用。旧引用不得继续调用新实例。
4. `spec_units` 只确定计量口径，不建立公司 SKU 的外部身份绑定。正式供给必须单独绑定公司 SKU、连接、`product_id` 与实际订货 `spec_id`。

## 3. 连接建立与技术绑定

按以下顺序执行；所有管理端请求使用既有 JWT、RBAC 与连接数据范围校验。

1. 在 ERP 供应商主档登记或核对蛋糕叔叔账户。通过既有 `POST /admin/supplier-api-connections` 建立 `environment="testing"`、`status="disabled"` 的连接，创建时省略 `endpoint_reference` 与 `credential_reference`。记录响应中的连接 ID；不得把渠道号当作 ERP 连接 ID。
2. 将供应商账户 ID、连接 ID、测试端点、渠道号及密钥写入受控配置。先完成所需映射，保持商业价格口径开关关闭，设置 `enabled=true` 并重启。此步骤只启用协议入口，不启用自动履约。
3. 调用 `POST /admin/supplier-api-connections/{connection_id}/dangaoshushu/reference-tickets`，取得响应 `data.endpoint_ticket`、`data.credential_ticket`、`data.expires_at`。需要 `supplier_api_connection:manage_credential_reference` 权限。票据有效期为五分钟，绑定连接、环境、类型和当前技术指纹；票据不得记录到普通日志。
4. 在有效期内依次调用既有治理命令。第一条成功后重新读取连接，第二条使用最新 `version`；两个动作使用不同且稳定的幂等键。

```http
POST /admin/supplier-api-connections/{connection_id}/commands
Content-Type: application/json
Authorization: Bearer <JWT>

{
  "action": "BIND_ENDPOINT_REFERENCE",
  "expected_version": <current_version>,
  "payload_reference": "<endpoint_ticket>",
  "idempotency_key": "<stable-endpoint-command-key>"
}
```

```http
POST /admin/supplier-api-connections/{connection_id}/commands
Content-Type: application/json
Authorization: Bearer <JWT>

{
  "action": "BIND_CREDENTIAL_REFERENCE",
  "expected_version": <latest_version>,
  "payload_reference": "<credential_ticket>",
  "idempotency_key": "<stable-credential-command-key>"
}
```

5. 两次绑定均完成后，执行只读查询。需要登记技术健康证据时，通过治理命令发送 `RUN_HEALTH_CHECK`、`check_type="AUTHENTICATION"`，使用当前连接版本与新的幂等键；按返回的后台任务查询结果，不把 HTTP 命令受理解释为检查完成。`CONNECTIVITY` 与 `AUTHENTICATION` 只读取品牌接口；`CAPABILITY_METADATA` 返回能力缺口 `DGSS_CAPABILITY_METADATA_UNSUPPORTED`，不得登记为成功。品牌接口成功不证明全部商品、配送、下单或售后能力已验收。
6. 票据只用于端点和凭证引用。业务资料与能力确认继续遵循既有采购治理合同；该运行时不解析 `UPDATE_BUSINESS_PROFILE` 的业务资料引用，不得提交伪造引用以绕过确认。
7. 配置身份、环境、软删除状态或技术引用不匹配时失败关闭。配置中的 `enabled` 与连接业务启停分别管理；紧急关闭全部协议入口须按 §8 修改配置并重启。

本地启动命令在 `backend/` 执行：

```bash
cargo run -p web-api -- --config-path ./config.toml
```

## 4. 授权只读接口

入口为 `GET /admin/supplier-api-connections/{connection_id}/dangaoshushu/catalog`，需要 `supplier_api_connection:protocol_read` 权限。响应采用既有 `ApiResponse`，`data` 为供应商响应中的原始数据，尚未转成正式商品或供给。

| `kind` | 请求参数 | 上游接口 |
| --- | --- | --- |
| `brands` | 无必填业务参数 | `GET /dsapi/brand/brand_city_lists` |
| `catalog` | `page` 缺省 1，`size` 缺省 20、范围 1 至 50；可选 `brand_id`、`city_id` | `GET /dsapi/product/get_product_hot_lists`，固定 `sort_price_type=1` |
| `product` | 必填 `product_id`；可选 `city_id` | `GET /dsapi/product/get_product_details` |
| `cities` | 必填 `product_id`；可选 `city_id` | `GET /dsapi/product/get_product_cities_info` |
| `shops` | 必填 `brand_id`、`city_id` | `GET /dsapi/brand/get_shop_lists` |
| `delivery_map` | 必填 `product_id`、`city_id` | `GET /dsapi/city/get_rules` |

示例请求：

```http
GET /admin/supplier-api-connections/{connection_id}/dangaoshushu/catalog?kind=brands
GET /admin/supplier-api-connections/{connection_id}/dangaoshushu/catalog?kind=catalog&page=1&size=20
GET /admin/supplier-api-connections/{connection_id}/dangaoshushu/catalog?kind=product&product_id={product_id}&city_id={supplier_city_id}
```

1. 仅允许固定 `kind` 与查询字段，不接受任意 URL 或写动作。外部 ID 按字符串传递，保留复合品牌号及前导零。
2. 目录使用新版接口；不得用旧版列表、详情或品牌/城市标识补齐新版结果。响应中含小数或指数的 JSON 数值按完整词法字符串返回，防止精度丢失；整数身份只接受非负整数字面量，字符串复合 ID 保持原值。金额解析仅作用于供应商响应入口，不改变现有工作区的 JSON/BSON 数字序列化。
3. `stock=-9999999` 映射为未报告数量；其他负数拒绝映射。不得把该哨兵当作零库存或无限库存。
4. 来源未报告变更时间时保留缺失；接收、观察或签名时间不得填充 `source_updated_at`。来源版本保留 `Unversioned`，不可按接收顺序宣称上游新旧关系。
5. 目录扫描仅声明部分增量覆盖。列表缺项、扫描结束和单次不可见不代表删除。正式同步、上级对象变化展开和时效保障须由后续持久化调度及领域应用用例承担。

## 5. 下单与支付分步合同

本节约束客户端的进程内能力。当前没有新增管理端下单 HTTP 入口；正式履约调用方启用前必须实现各步骤的持久化意图、回执、权限及恢复协调。

| 独立步骤 | 上游调用 | 执行与恢复要求 |
| --- | --- | --- |
| 地址准备 | `POST /dsapi/addr/oprate_addr`，表单 | 先保存 `ActionKey` 和请求快照；要求 `user_id`、城市、区县及明确的 BD-09 百度经纬度；结果未知转人工，当前无法按动作号恢复地址，不得重建第二份地址 |
| 配送选项 | `POST /dsapi/order/get_distribution_rules`，multipart | 只读；按商品优先、品牌兜底的配送规则拆分，组内明细一致；提货额外读取门店 |
| 单组创建 | `POST /dsapi/order/submit_order`，multipart `order_data` | 每组独立 `out_order_no`；发送前核验地址、配送选项、明细、当前结算价和批准金额；每次调用至多发送一次创建请求 |
| 支付结果通知 | `POST /dsapi/order/order_pay_result`，JSON | 使用原外部单号、我方单号、交易号和金额；与创建分开保存意图与结果，创建方法不得自动调用支付 |
| 查单 | `GET /dsapi/order/order_details` | 支持我方 `out_order_no` 或外部 `order_no`；必须核对响应渠道及原查询单号 |

1. 商品行必须非空、最多 50 行，行号和规格号唯一，数量为正整数，批准单价非负。可选规格属性只接受至多一个非空且不含逗号的“口味”。
2. 收件地区必须有明确地区映射；坐标必须声明 `BD-09`，禁止将 WGS-84 或 GCJ-02 静默作为百度坐标发送。
3. 地址上下文及配送选项由服务端 HMAC 保护，绑定完整配置技术指纹、连接、收件资料、明细、方式和费用。配送选项本地有效期为 60 秒，创建前读价完成后再次核验有效期，HTTP 客户端取得配额后再次阻断过期请求并将请求超时限制到剩余有效窗口；过期后重新获取选项，不自行修改引用。
4. 未确认 `clearing_price` 的人民币含税口径，或缺少 `spec_units` 时不得下单。预查结算价必须与批准单价一致；预查不能保证供应商创建时价格不变。
5. 创建返回外部单号、实付金额与下一步。返回金额超过批准值时仍须保存已创建外部订单，阻断后续支付并登记异常；不得丢弃回执后重新创建。
6. 未核实供应商幂等保护窗口，`ReplayProtection` 保持 `Unverified`。创建结果未知先按原我方单号查询；`NotVisible` 只表示当前不可见，不能作为重提证明。
7. 支付确认结果未知先查原订单的支付状态。支付确认成功只证明支付轨道，不证明供应商已接单。履约、取消、退款、支付分别保存，不由订单取消状态推断退款完成。
8. 供应商缺少单笔稳定退款身份及结算流水身份时，保留原始证据并人工核对；不得用累计退款金额生成多笔财务流水。

## 6. 推送接收合同

在供应商侧登记 HTTPS 公网回调地址，按消息类型使用以下路径。路径中的连接 ID 必须与配置一致，端点和凭证引用须已绑定。

| 路径 | 推送类型 | 保存的刷新目标 |
| --- | --- | --- |
| `POST /callbacks/dangaoshushu/{connection_id}/order` | 订单状态 | 原我方订单号 |
| `POST /callbacks/dangaoshushu/{connection_id}/status` | 品牌或商品可售状态 | 品牌或商品 |
| `POST /callbacks/dangaoshushu/{connection_id}/product` | 商品属性 | 商品 |
| `POST /callbacks/dangaoshushu/{connection_id}/cities` | 品牌或商品可售城市 | 品牌或商品 |
| `POST /callbacks/dangaoshushu/{connection_id}/price` | 价格变化 | `product_id` 与 `spec_id` |

1. 推送使用 JSON 原始正文，最大 256 KiB。路由限制同一接入键每分钟 300 次、全局每分钟 600 次、并发 8；过载不得发送成功应答。
2. 验签核对绑定 `channel_no`、时间戳、签名及接收时间窗口。供应商公式为对 `channel_no` 字面量、渠道号值、`timestamp` 字面量、时间戳值与私钥依次拼接，计算 SHA-1 的小写十六进制，再计算该文本的 MD5 小写十六进制。
3. 该签名不覆盖业务正文，认证范围始终标为 `EnvelopeOnly`。正文中的价格、数量、状态、变更时间不得直接应用为正式事实，只保存对象刷新线索。
4. 当前来源没有可靠事件号，`source_event_id=None`。本地接收 ID 使用 HMAC 绑定配置技术指纹、请求路径和原始正文字节；完全相同报文重放命中唯一索引后，须核实已有接收记录的连接才可应答。该身份只用于字节级接收去重，不能冒充来源事件号，不按对象或状态合并不同报文。
5. 接收层使用应用敏感数据加密器保存原始正文密文，连同刷新提示写入单文档；只有保存成功后才返回 HTTP 200 与 `{"code":200,"message":"success"}`。验签、绑定、加密或数据库保存失败均不返回该成功应答。
6. 供应商成功应答仅表示证据与待处理意图已接收。当前记录状态保留 `received`，须在补查 worker、已绑定对象展开及领域应用完成验收后才可声明自动同步。
7. 验签时间窗口不提供上游单次使用保证。相同报文的重复成功应答只证明原接收证据已落库；路径、配置指纹或正文字节不同均使用不同接收身份。不得依赖接收顺序推断来源版本，正文未认证的限制仍适用。

## 7. 持久化、索引与兼容

1. 新增集合 `supplier_callback_receipts`，由 `erp-integration` 拥有。保存通用 `BaseModel`、连接 ID、来源事件号、认证范围、接收时间、消息类型、原始正文密文、刷新提示与 `received` 状态；不修改既有供给、订单及财务集合。
2. 发布时先确认数据库账号具有既有启动索引登记所需权限。Web API 启动的 `erp-integration` 索引登记自动创建下列索引，执行幂等增量迁移；索引创建失败须停止发布，不跳过校验。

| 索引 | 键与选项 | 用途 |
| --- | --- | --- |
| `uk_supplier_callback_receipts_id` | `{id: 1}`，唯一 | 保护本地接收身份 |
| `ix_supplier_callback_receipts_pending` | `{connection_id: 1, status: 1, received_at: 1, id: 1}` | 按连接、接收时间及 ID 稳定读取最多 50 条待处理记录 |

3. 本次增量集合不回填既有业务文档，不给旧订单制造来源事件号，不修改既有来源时间口径。启用正式可供落库前仍须完成 `source_updated_at` 可缺失的存储、读取和回滚兼容方案。
4. 接收证据不设自动 TTL，当前没有自动归档、消费器、容量告警或清理任务。登记供应商推送前，运营须在部署工单确定容量阈值、告警责任人和暂停入口处置；持续接收期间监控该集合的文档数量、存储/索引容量、按连接的 `received` 数量及最早待处理时间。待处理查询沿用连接与状态索引，分页有界，不用读取全部正文统计积压。
5. 容量或积压达到工单阈值时，先暂停供应商推送并按 §8 关闭运行入口，保留最后接收时间及待处理清单。当前没有消费器，积压不会自动消减；禁止人工将 `received` 改为完成、添加 TTL、删除记录或清空集合来消除告警。
6. 归档必须保存密文、接收身份、连接、时间、认证范围和刷新提示，并保留受控解密能力；先验证备份可恢复及对象数量一致，登记归档范围与位置。未经正式补查、业务处置或人工归档处置确认的未处理证据不得从在线集合删除；单纯导出副本不构成删除授权。
7. 加密证据保留既有应用敏感数据密钥政策。回滚保留能够解密历史证据的应用配置及受控备份，不把正文密文迁入普通日志。

## 8. 停用与回滚

1. 停用前在供应商侧暂停或撤销推送地址，保留双方登记信息及最后接收时间；存在已创建或结果未知的外部订单时先登记人工跟踪清单。
2. 通过既有连接治理 `DISABLE` 命令停用业务连接。若要关闭健康检查、授权只读查询和回调协议入口，还须将 `[dangaoshushu].enabled=false` 并重启 Web API；仅修改数据库连接状态不等同于关闭已装配协议实例。
3. 关闭后，蛋糕叔叔运行时不再装配，授权协议查询返回未配置，回调返回非成功；已有 `supplier_callback_receipts`、连接引用、治理回执和后台任务均保留。
4. 程序回滚使用上一可用制品和对应配置，保留新增集合及两个索引。旧制品未引用该集合，增量数据不阻止旧路径运行；禁止将删除集合或清空待处理记录作为常规回滚步骤。
5. 恢复接入时重新核对配置身份与技术指纹，完成技术引用绑定和只读健康核验，再恢复供应商推送。保存的 `received` 记录继续等待明确的补查与业务处置，不自动标为完成。
6. 配置关闭及程序回滚不会撤销供应商已创建的地址、订单或支付通知；未知外部结果继续按原动作和单号调查，不自动补发写请求。

## 9. 验收执行要求

1. 开发质量验证按后端规则执行受影响 crate 编译、库单元测试、Clippy、workspace 编译、领域边界及权限漂移检查。禁止新增或执行真实 MongoDB、S3 或供应商依赖的集成测试。
2. 单元测试验证签名公式、时间边界、映射失败、令牌绑定、单次写调用及创建与支付分离；替身轨迹只证明本地生产编排，不证明远端幂等或数据库保存效果。
3. 实际环境验收须区分只读认证/目录核验、回调持久化、正式供给应用与正式履约。只读核验不得创建地址、订单或支付通知。
4. 自动业务启用必须取得明确证据：ERP 对象身份绑定、单位与地区映射、价格口径、分步持久化及结果恢复、回调补查与领域应用、来源时间兼容。接口声明或本地通过不能替代这些证据。

## 10. 供应商协议依据

新接口与消息字段按供应商公开文档执行；供应商协议更新须先修改本合同和客户端，再按对应范围验收。

- [API 变更记录](https://www.showdoc.com.cn/dgssapi/3748637402170131)
- [认证公共参数](https://www.showdoc.com.cn/dgssapi/3751620332418883)
- [新版商品列表](https://www.showdoc.com.cn/dgssapi/11415935532258976)、[新版商品详情](https://www.showdoc.com.cn/dgssapi/11415938943549745)
- [配送规则](https://www.showdoc.com.cn/dgssapi/9836051808805470)、[提交订单](https://www.showdoc.com.cn/dgssapi/3760495432341858)
- [支付结果通知](https://www.showdoc.com.cn/dgssapi/3856087224680993)、[新版订单详情](https://www.showdoc.com.cn/dgssapi/11415863424477556)
- [推送公共参数](https://www.showdoc.com.cn/1287092248193413/6462201736746868)
