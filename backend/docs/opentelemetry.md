# OpenTelemetry Trace 接入与方法耗时合同

## 1. 适用范围

本合同适用于 `web-api` 的 HTTP 请求、Service 业务用例、Repository 聚合操作和外部依赖调用。
OpenTelemetry 用于分布式链路与墙上耗时观测；全函数 CPU 自耗时必须使用 profiler，不得通过为
每个私有函数创建 span 实现。

## 2. 启动合同

- 应用必须继续使用现有 `tracing` subscriber；不得创建第二套日志 subscriber。
- 同时满足以下条件时，应用必须创建 OTLP/gRPC trace exporter：
  - `OTEL_SDK_DISABLED` 未设置为 `true`、`1` 或 `yes`；
  - `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` 或 `OTEL_EXPORTER_OTLP_ENDPOINT` 至少一项为非空值。
- 未配置 endpoint 时，应用必须保持纯结构化日志模式，不得尝试连接默认 Collector。
- OTLP 协议必须使用 gRPC，Collector 默认监听端口为 `4317`。
- 应用必须使用 W3C `traceparent` 和 `tracestate` 继承上游上下文。
- 应用必须在收到 Ctrl-C 或 SIGTERM 后停止接收请求，并在运行时退出前关闭 tracer provider。

## 3. 环境变量

| 变量                                 | 必填条件     | 执行要求                                                                |
| ------------------------------------ | ------------ | ----------------------------------------------------------------------- |
| `OTEL_SDK_DISABLED`                  | 否           | 默认可设为 `true`；启用导出时必须设为 `false`。                         |
| `OTEL_EXPORTER_OTLP_ENDPOINT`        | 启用时二选一 | Collector gRPC 根地址，例如 `http://otel-collector:4317`。              |
| `OTEL_EXPORTER_OTLP_TRACES_ENDPOINT` | 启用时二选一 | Trace 专用地址；同时配置时优先于通用地址。                              |
| `OTEL_EXPORTER_OTLP_PROTOCOL`        | 启用时       | 必须为 `grpc`。                                                         |
| `OTEL_SERVICE_NAME`                  | 否           | 默认 `erp-web-api`；同一部署中的名称必须稳定。                          |
| `OTEL_RESOURCE_ATTRIBUTES`           | 否           | 必须使用低基数资源属性，例如 `deployment.environment.name=production`。 |
| `OTEL_TRACES_SAMPLER`                | 否           | 生产环境按第 7 节设置。                                                 |
| `OTEL_TRACES_SAMPLER_ARG`            | 比例采样时   | 必须为 `0.0` 至 `1.0`。                                                 |

本地启用命令：

```bash
OTEL_SDK_DISABLED=false \
OTEL_EXPORTER_OTLP_PROTOCOL=grpc \
OTEL_EXPORTER_OTLP_ENDPOINT=http://127.0.0.1:4317 \
OTEL_SERVICE_NAME=erp-web-api \
OTEL_RESOURCE_ATTRIBUTES=deployment.environment.name=local \
cargo run -p web-api -- --config-path ./config.toml
```

## 4. HTTP span 合同

- 每个 HTTP 请求必须自动创建一个 `SpanKind.SERVER` 根 span。
- span 名称必须使用 `METHOD + MatchedPath`，例如 `GET /admin/orders/{id}`。
- `http.route` 必须使用 Axum 路由模板，不得使用包含实际 ID 的 URL path 替代。
- 必须记录 `http.request.method`、`url.path`、`url.scheme`、`http.response.status_code` 和耗时。
- HTTP 4xx 的 server span 状态必须保持未设置；HTTP 5xx 必须设置为 Error。
- 现有 `X-Trace-Id` 必须继续写入请求上下文和响应头，并作为 span 属性用于日志关联。
- URL 查询参数、Authorization、Cookie 和请求体不得写入 span。

## 5. 方法 span 合同

### 5.1 必须打点

- Service 对外业务用例入口。
- Repository 中包含数据库 I/O 的聚合操作、事务阶段和已确认的慢操作。
- S3、HTTP、消息或其他外部依赖调用。
- 后台任务的整次执行和有界批次。

### 5.2 禁止无差别打点

- getter、mapper、格式转换和简单校验函数。
- 高频循环中的每条记录处理。
- 不含 I/O 且耗时可忽略的私有 helper。
- 可能把密码、Token、个人敏感数据或完整 DTO 自动写入字段的方法。

### 5.3 Rust 写法

```rust
#[tracing::instrument(
    name = "sales_order.create",
    skip_all,
    fields(domain = "sales_order", operation = "create")
)]
pub async fn create(/* ... */) -> Result</* ... */> {
    // 业务逻辑
}
```

- 方法 span 必须使用 `skip_all`，再显式声明允许记录的稳定字段。
- span 名称必须为稳定的 `domain.operation`，不得拼接订单号、账号 ID 或错误文本。
- `request_id`、订单号等关联值只允许进入 trace/log 属性，不得成为指标标签。
- 错误必须使用稳定类别记录；不得把底层连接字符串、凭据或内部响应体写入 span。

### 5.4 首批方法覆盖合同

Service 必须覆盖以下业务入口：

- 销售单：列表、详情、客户范围解析、创建、保存工作副本、提交、撤回审批、作废和正式化。
- 履约：采购收货、仓库发货、客户验收、电子交付、服务履约的列表、详情、创建、更新、确认、
  提交、冲销，以及客户验收资格计算。
- 库存：余额、流水、预占、调整单的列表与详情，以及调整单创建、更新、提交、撤回审批和过账。

Repository 必须覆盖以下数据库 I/O 边界：

- 销售单分页检索、聚合明细读取、工作副本提交和批准提交正式化。
- 履约分页检索、验收资格聚合读取、草稿发货读取，以及表头与明细的聚合写入或替换。
- 库存分页检索、可用量与预占聚合读取、余额及预占原子写，以及调整单表头与明细聚合写入。

Repository span 必须表示整个仓储方法的墙上耗时；一个 span 内可能包含多次 MongoDB 命令，不得把该
耗时解释为单次网络往返。应用与 MongoDB 驱动的 OpenTelemetry 依赖版本未统一前，不得启用驱动级
命令 span。新增方法必须按 5.1 与 5.2 节分类，不得仅因方法为 `async` 就自动打点。

## 6. 耗时读取合同

- 单次请求耗时必须从 trace 瀑布读取。
- 方法调用次数、错误率、P50、P95 和 P99 必须由 Collector 的 span metrics 能力生成。
- 应用不得为已经存在的 span 再维护一套重复的 `Instant` 方法计时指标。
- dashboard 聚合维度只允许使用 `service.name`、稳定 span 名称、环境和稳定结果类别。

## 7. 采样合同

- 开发和验收环境可使用 `parentbased_always_on`。
- 应用连接本机或同节点 Collector 时，应由 Collector 执行 tail sampling：保留全部错误、慢 trace，
  并对普通成功 trace 按比例保留。
- 应用直接连接远端 Collector 且流量受限时，可使用：

```bash
OTEL_TRACES_SAMPLER=parentbased_traceidratio
OTEL_TRACES_SAMPLER_ARG=0.1
```

- 采样比例必须按实际吞吐、存储成本和排障保留期调整，不得把示例值视为固定生产阈值。

## 8. 验收合同

启用 Collector 后必须完成以下验收：

1. 普通请求产生 HTTP server 根 span。
2. 带有效 `traceparent` 的请求继承指定 Trace ID。
3. HTTP span 名称显示路由模板，不显示实际资源 ID。
4. `X-Trace-Id` 继续出现在响应头。
5. 已有 `#[tracing::instrument]` 方法显示为 HTTP span 的子 span。
6. HTTP 5xx span 标记为 Error，4xx 不误标为服务端故障。
7. SIGTERM 后服务停止接收请求，已完成 span 在进程退出前完成 flush。
8. 代码通过 workspace 格式、编译、Clippy、测试和 BPM 边界检查。
