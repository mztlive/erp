# erp-resource-grants

负责自动记录读取授权的业务编排：根据业务事件读取事实、计算授权结果，再交给 `erp-identity` 保存。公开入口为 `ResourceGrantProcess`，同时提供授权就绪查询、重算和存量回填。

系统架构及与 identity、authorization-wiring 的关系见 [ERP 权限系统设计](../../../../docs/authorization-system-design.md)。

## 职责边界

| 模块 | 职责 |
| --- | --- |
| 业务领域或业务编排 | 决定何时产生授权事件，在业务事务中追加事件 |
| 本 crate | 读取业务事实、执行自动授权规则、生成授权结果 |
| [erp-identity](../../domains/erp-identity/README.md) | 提供事件与授权模型、存储、来源管理和统一权限判断 |
| [erp-server worker](../../../apps/erp-server/src/worker/resource_grants.rs) | 装配处理器，运行消费与对账循环，管理进程生命周期 |

业务事实通过拥有领域的仓储读取；授权事件、来源和授权记录通过 identity 的公开服务维护。业务访问检查继续使用 `AuthorizationService`。

## 如何使用

### 产生授权事件

在业务用例中构造 `GrantEventEvidence`，使用当前业务事务的执行器调用 identity 的 `append_event`：

```rust
use erp_identity::service::resource_access::append_event;

// evidence 由业务用例构造，executor 为当前业务事务执行器。
append_event(db, evidence, executor).await?;
```

事件与业务变更必须同事务提交，追加失败须向上传播。事件类型与字段见 [GrantEventPayload](../../domains/erp-identity/src/entity/resource_access/event.rs)。现有生产方示例见 [审批事件接入](../erp-approval-runtime/src/resource_grants.rs)。

### 处理授权事件

组合层创建 `ResourceGrantProcess`，由 worker 调用：

```rust
use erp_resource_grants::ResourceGrantProcess;

let process = ResourceGrantProcess::new(db, rbac);
// token 为本次领取的唯一令牌，now 为当前秒级时间。
let claimed = process.tick(&token, now).await?;
```

`new` 不启动后台任务；`tick` 处理一次领取及发布，返回是否领取到事件。运行循环复用 [现有 worker](../../../apps/erp-server/src/worker/resource_grants.rs)。

处理链路为：业务事务追加事件 → worker 领取 → 本 crate 读取事实并计算结果 → identity 保存授权。事件追加成功与授权生效是两个阶段。

### 查询和维护

以下方法均属于 `ResourceGrantProcess`：

| 场景 | 入口 | 使用说明 |
| --- | --- | --- |
| 按来源查询授权就绪状态 | `source_readiness` | 传入服务端确定的 `SourceKey` 和当前执行器 |
| 查询开票授权是否待生成 | `invoice_pending` | 传入申请 ID、读取账号和当前执行器 |
| 后台对账 | `reconcile` | 按返回游标继续处理后续页 |
| 查询队列运行情况 | `health` | 获取队列指标 |
| 管理授权来源 | `management` | 返回绑定业务事实校验的 `ResourceGrantService` |
| 存量回填 | `backfill` | 接收 `BackfillRequest`，按页预览或追加事件 |

`source_readiness` 返回 `GrantReadiness`，用于区分来源不存在、待生成、已就绪和已终止；`invoice_pending` 返回是否存在待生成的开票授权。业务访问仍须执行完整权限检查。

管理端通过 `management()` 返回服务的 `grant`、`revoke`、`sources`、`recompute_managed` 操作授权来源；回填使用 `backfill`。后台 `reconcile` 不执行管理员资格检查，不能直接作为管理端入口。管理请求见 [management.rs](src/management.rs)，回填请求与结果见 [backfill.rs](src/backfill.rs)；请求与结果类型统一从 crate 根导入。HTTP 接线见 [管理 handler](../../../apps/erp-server/src/http/handler/access_control/resource_grants.rs)。

## 如何扩展

自动授权规则通过代码接入。新增规则需要同时接通事件生产、事实读取、授权计算和维护入口。

| 步骤 | 修改位置 | 接入内容 |
| --- | --- | --- |
| 1. 定义事件 | identity 的 [event.rs](../../domains/erp-identity/src/entity/resource_access/event.rs) | 增加 `GrantEventPayload` 变体及来源、事件身份映射 |
| 2. 接入触发点 | 对应业务领域或业务编排 | 构造事件证据，在业务事务中调用 `append_event` |
| 3. 读取事实 | 本 crate 的 [facts.rs](src/facts.rs) | 扩展 `Facts::target` 分派，调用拥有领域仓储；由 `compute` 生成 `SourceUpdate` |
| 4. 实现规则 | 本 crate 的 [rules.rs](src/rules.rs) | 增加或调整不依赖 I/O 的规则校验，并由事实计算路径调用 |
| 5. 接入重算 | identity 的 [queue.rs](../../domains/erp-identity/src/repository/resource_grants/queue.rs) | 扩展 `GrantRule` 及按规则、资源筛选的映射 |
| 6. 验证 | 规则、事件和生产方的对应测试模块 | 覆盖新规则行为、事件接入和维护路径 |

修改已有规则时，从 `facts.rs`、`rules.rs` 进入，检查事件证据是否仍满足计算需要，并确定已有授权是否需要重算。新增规则无需另建 worker；继续复用 `ResourceGrantProcess` 的消费和发布流程。

按需求扩展以下配套能力：

- **新增授权资源类型**：扩展 identity 的 [ResourceKind](../../domains/erp-identity/src/entity/resource_access/source.rs)、本 crate 的目标校验与重算映射，并将业务读取入口接入相应授权政策和 Mapping。
- **支持历史数据**：由拥有领域仓储提供证据读取，在 [backfill.rs](src/backfill.rs) 接入转换与分页处理。现有仓储示例见 [grant_history.rs](../../domains/erp-workflow/src/repository/grant_history.rs)。
- **增加管理参数或返回数据**：普通管理请求修改 [management.rs](src/management.rs)，回填请求与结果修改 [backfill.rs](src/backfill.rs)，同步管理 handler。

扩展时保留公共处理约束：事实读取沿用传入的 `Executor`；读取失败返回错误，不能转换为空授权结果；授权发布继续经过 `ResourceGrantService::apply_source`，复用身份域的幂等、版本和撤销控制。

## 代码导航

| 文件 | 维护内容 |
| --- | --- |
| [lib.rs](src/lib.rs) | 公开 API 导出 |
| [process.rs](src/process.rs) | 事件处理、事务编排、后台对账和队列观测 |
| [readiness.rs](src/readiness.rs) | 来源就绪与开票授权待生成查询 |
| [facts.rs](src/facts.rs) | 业务数据读取、事件分派、授权结果计算及发布前重验 |
| [rules.rs](src/rules.rs) | 纯规则校验与测试 |
| [backfill.rs](src/backfill.rs) | 回填请求与结果、历史证据转换、回填预览与执行 |
| [management.rs](src/management.rs) | 管理请求、参数校验与管理服务装配 |

## 验证

在 `backend/` 下执行本 crate 与身份域的库测试：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-resource-grants -p erp-identity --lib --locked
```

修改事件生产方时，执行对应业务 crate 的受影响测试；修改跨域依赖时，执行 `./scripts/check-domain-boundaries.sh`。涉及数据库事务、并发消费或恢复行为的变更，另行执行对应集成验证。
