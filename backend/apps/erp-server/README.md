# ERP 服务端应用合同

## 包与启动入口

本包名为 `erp-server`，提供两个独立二进制。命令在 `backend/` 执行：

```bash
cargo run -p erp-server --bin web-api -- --config-path ./config.toml
cargo run -p erp-server --bin erp-worker -- --config-path ./config.worker.local.toml
```

- `web-api` 提供业务 HTTP，不启动常驻消费者。
- `erp-worker` 提供内部探针与受 token 保护的 XXL-JOB 协议，执行常驻消费及调度工作，不挂载业务 HTTP 路由。
- 两个进程使用不同监听端口，连接同一业务数据库，并保持对象存储、内容指纹与敏感数据密钥配置一致。
- Worker 必须配置完整、合法的 `[xxl_job]`；API 可省略该节。任何进程提供该节时均执行合法性校验。
- 定时入口由 XXL-JOB 驱动，不提供内嵌 worker 或本地替代调度开关。
- 发布镜像包含上述两个二进制，部署命令及服务名称保持稳定。

## 目录与依赖边界

```text
src/
├── main.rs                    # web-api 入口
├── bin/erp-worker.rs          # worker 入口
├── bootstrap.rs              # 配置、基础资源、初始化与配置变更观察
├── indexes.rs                # 按既有顺序调用领域公开索引入口
├── http/
│   ├── state.rs              # HttpState、JWT 缓存、京东页面授权任务
│   ├── tasks.rs              # 请求衍生任务登记与排空
│   ├── auth/                 # JWT 协议
│   ├── extractor/
│   ├── middleware/
│   ├── handler/
│   ├── routes/
│   ├── errors.rs
│   ├── response.rs
│   ├── tracing.rs            # 请求追踪
│   └── upload/
├── worker/
│   ├── mod.rs                # 常驻任务装配、监督与停止
│   ├── supplier_orders.rs
│   ├── approval_outbox.rs
│   ├── data_transfer.rs
│   ├── resource_grants.rs
│   └── xxl/                  # 执行器协议、注册、执行与回报
├── wiring/
│   ├── application.rs        # Application：共享资源及业务能力工厂
│   ├── application/          # 外部连接器、识别、导入等能力装配
│   ├── authorization/        # 完整 Facts/Mapping 与共享授权服务装配
│   │   └── resource_grants/  # 正式事实取证、记录授权发布、就绪状态及回填
│   ├── approval_dispatch/    # 审批动作与对象读取接线
│   ├── data_transfer/        # 导入导出 Process 和业务读取接线
│   └── ...                   # 消费方 Port 到提供方领域的 adapter
└── runtime/
    ├── role.rs               # 二进制固定职责
    ├── server.rs             # 监听、信号、失败监督及有界排空
    └── telemetry.rs          # 进程日志与 OpenTelemetry 初始化
```

1. `http/` 与 `worker/` 均消费 `wiring/`，不得相互依赖。
2. `wiring/` 持有共享资源并装配 Service、Process、ReadModel 和 Port；不得依赖 HTTP 类型、启动常驻循环或管理进程退出。
3. `HttpState` 持有 HTTP 专属缓存与请求任务边界，通过 `application()` 访问业务装配。Worker 直接使用 `Application`。
4. Handler 只提取协议输入、传递已认证身份、调用用例并包装响应。单域用例归拥有领域，跨域写入归所属编排库，跨域读取归 `erp-read-models`。
5. HTTP 和 worker 各自在启动时调用 `wiring::authorization::build(db, rbac)`，将完整校验后的 `Arc<AuthorizationService>` 保存在 `Application`。`authorization()` 只克隆该 `Arc`；业务工厂注入同一实例。共享服务不得保存请求身份、事务、候选对象或授权结果。

## 记录读取授权装配

`Application` 在启动时以数据库及共享授权服务装配 [`ResourceGrantRuntime`](src/wiring/authorization/resource_grants/mod.rs)，通过 [`resource_grants()`](src/wiring/application.rs) 提供轻量句柄。HTTP 使用其维护入口；[`erp-worker`](src/worker/resource_grants.rs) 使用 `tick` 单轮领取及发布，按游标调用 `reconcile` 做有界对账，并通过 `health` 观测队列。CLI 可按需单向复用 `erp_server::wiring`，保持独立命令运行时。

业务用例在原事务追加正式批准、任务转派或订单审批分配事件。[`facts.rs`](src/wiring/authorization/resource_grants/facts.rs) 通过工作流快照和拥有领域仓储取证，[`rules.rs`](src/wiring/authorization/resource_grants/rules.rs) 验证关联及正式分配。`erp-identity` 持有队列与授权记录，由 `ResourceGrantService::apply_source` 在同一事务复核领取、应用授权并完成事件。发布提交结果未知时保留原租约，恢复时检查事件是否已完成。完整职责见 [ERP 权限系统设计](../../../docs/authorization-system-design.md#记录读取授权的生成与使用)。

[`readiness.rs`](src/wiring/authorization/resource_grants/readiness.rs) 在调用方的执行器中检查正式来源及其版本，区分无事件、待生成、已就绪和已终止。审批与资金接线以此识别授权待生成；实际访问仍由完整授权检查决定。

新增自动规则时，在 identity 的 [`GrantRule`](../../crates/domains/erp-identity/src/entity/resource_access/rule.rs) 与[事件模型](../../crates/domains/erp-identity/src/entity/resource_access/event.rs) 定义稳定来源和事件证据，接入业务事务生产方，再扩展事实读取与纯规则。已有来源身份应保持稳定；变更规则时检查是否需要重算历史授权。

### 记录授权管理接口

[管理端 handler](src/http/handler/access_control/resource_grants.rs) 复用 [identity 的管理请求 DTO](../../crates/domains/erp-identity/src/dto/resource_access.rs)，通过 `management()` 调用 `grant`、`revoke`、`sources` 或 `recompute_managed`，由身份域校验管理员权限。worker 的例行对账入口 `reconcile` 仅供后台使用。

[`backfill(actor, request)`](src/wiring/authorization/resource_grants/backfill.rs) 校验管理员资格，分页核验历史审批、正式分派及转派回执。默认 `dry_run: true`，预览返回可回填、已有、永久终止、无效证据及续页游标。显式关闭预览才追加核验通过的授权事件，随后由 worker 发布。回填不重放审批、转派或财务命令。

## 执行与退出合同

| 工作类型 | 所有者 | 停止与恢复要求 |
| --- | --- | --- |
| 常驻消费、轮询、outbox | Worker | 停止领取新工作，等待当前轮次与派生任务，未完成工作沿原持久状态恢复 |
| XXL 定时触发 | Worker | 停止接收新执行请求，排空执行与回报；回报重试不得重放业务 |
| 合同识别、供应商连接请求衍生任务 | HTTP 请求任务管理器 | 请求取消不取消已登记工作；HTTP 请求排空后停止任务准入并等待收尾 |
| 京东页面授权 | HTTP 授权任务边界 | 请求取消不取消授权结果保存，停机时等待原任务，未知结果不得重发 |

进程收到停止信号或发现受监督常驻循环退出时，先关闭工作入口并停止 HTTP 受理，再并行排空 HTTP 和 Worker。HTTP 排空后等待请求衍生任务与京东授权保存。总排空预算为 60 秒，超时返回进程错误，后续恢复使用原任务身份与原幂等键。

Worker `/health` 表示进程存活，`/ready` 在排空时关闭；不得将其作为 XXL 注册成功、业务成功或无积压的证明。API `/ready` 保持既有外部连接器就绪检查。

数据库与 S3 连接固定为启动配置；变更后记录重启要求。XXL 配置变更不热切换执行器。JWT 引擎按当前配置密钥核验缓存，密钥变化后重新构建。

## HTTP 与权限合同

- 管理端由 JWT 确认身份，Handler 将当前身份传给受保护的 Service、Process 或 ReadModel。业务用例通过 `require` 或 `query` 在读取业务数据或写入前完成授权；仅功能权限操作使用 `AccessTarget::Operation`。
- 路由通过 `permission_route(route, &permission)` 引用 `erp-permissions` 公共目录中登记的权限主常量。该包装仅提供构建元数据，运行时原样返回路由；编码、分组和默认说明只维护一份。
- 业务附件在写入对象存储前调用固定业务准备入口检查完整功能资格；文件存储后的写事务按真实对象、来源或任务重新授权。该准备不签发对象凭证，也不额外要求独立文件上传菜单。
- 静态选项、字典候选、期间口径及审计日志沿用各自业务入口资格。审计展示通过 `AuditLogReadService::new(db, authorization)`，在同一快照中完成资格检查、分页与计数；登录审计写入继续使用 `AuditLogService`。
- 公开入口和供应商门户沿用既有认证链路，不以管理端权限元数据替代公开令牌或门户身份检查。
- `route_descriptions.rs` 仅登记不同于默认值的接口说明，按 HTTP 方法及不含 `/admin` 的管理端路径匹配。`build_route_overrides.rs` 保存少量分组及展示权限差异，只参与生成，不改变鉴权权限。不存在或重复登记的接口差异使生成失败。
- 请求与响应优先复用拥有领域或编排库 DTO；协议包装必须提供显式转换并说明原因。
- `build.rs` 使用公共权限定义与 `src/http/routes/` 生成路由权限合同、后端白名单及前端权限目录。生成物不得手改。
- 在 `backend/` 执行 `python3 scripts/generate-permissions.py` 可独立运行同一生成器；`--test` 执行目录与路由合同单元测试。`check-permissions-drift.sh` 使用独立生成结果比较文件，无需编译业务 crate。
- 人员范围和权限检查接口使用 identity 的当前 DTO，不返回 `authorization_policy` 总分类。前端按 `configurable_actions` 控制编辑，按检查步骤展示已检查项和待检查项。
- 响应字段、HTTP 状态、错误文本、上传限制、限流窗口及路由路径遵循各接口合同。配置变更与错误分类遵循 [后端规则](../../AGENTS.md)。

## 验证与历史材料

开发和常规提交只检查受影响 crate；命令和触发条件以 [后端规则](../../AGENTS.md) 为准。跨层授权切换执行以下整体检查：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-server -p config --lib --locked
cargo check --workspace --locked
cargo clippy -p erp-server -p config --lib --bins --locked -- -D warnings
./scripts/check-domain-boundaries.sh --cutover
./scripts/check-bpm-boundaries.sh
./scripts/check-permissions-drift.sh
```

`examples/` 是历史真实环境验收材料，保留原始内容，不自动发现为 Cargo target；禁止新增、修改或执行。该目录不构成当前运行入口。测试与验收范围执行后端规则，不得把静态或库测试结果扩大为真实 MongoDB、供应商、HTTP 服务或生产环境验收。
