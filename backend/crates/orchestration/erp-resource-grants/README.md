# erp-resource-grants

## 职责

根据正式审批和任务分派事实，为指定账号生成销售单、采购单的记录读取授权。销售、采购审批正式分派分别授权读取对应订单；开票审批通过和开票任务转派授权读取应收关联的销售单。[`ResourceGrantProcess`](src/lib.rs) 是消费、就绪查询、对账和回填的公开入口。

业务用例决定何时产生事件；本 crate 负责取证、计算和事务发布；[erp-identity](../../domains/erp-identity/README.md) 持有事件队列、授权来源与记录，并通过 `AuthorizationService` 统一判断访问权限。指定记录读取授权既不授予入口功能权限，也不扩大原单据的写入范围或任务办理资格。整体分工见 [ERP 权限系统设计](../../../../docs/authorization-system-design.md)。

## 目录结构

生产代码位于 `src/`，由 [lib.rs](src/lib.rs) 导出流程入口、就绪状态及管理请求和回填结果类型：

| 源码 | 维护内容 |
| --- | --- |
| [process.rs](src/process.rs)、[readiness.rs](src/readiness.rs) | 领取和发布事件、对账与队列观测；按正式来源查询授权生成状态。 |
| [facts.rs](src/facts.rs)、[rules.rs](src/rules.rs) | 通过拥有领域的仓储读取事实，计算完整授权目标；关联和分派规则不做 I/O。 |
| [management.rs](src/management.rs)、[backfill.rs](src/backfill.rs) | 管理请求校验和服务装配；分页核验历史证据，预览或追加授权事件。 |

## 架构设计

### 产生授权事件

生产方构造 [`GrantEventEvidence` 和 `GrantEventPayload`](../../domains/erp-identity/src/entity/resource_access/event.rs)，在原业务事务内调用 identity 的 [`append_event(db, evidence, executor)`](../../domains/erp-identity/src/service/resource_access/command.rs)，并向上传播追加错误。现有接入点是[销售及采购审批分派](../erp-approval-runtime/src/resource_grants.rs)、[开票批准和派工](../erp-finance-posting/src/finance_posting/receivable/invoice_request/execution.rs)及[开票任务转派](../../domains/erp-workflow/src/service/work_item/reassign_apply.rs)。追加成功只表示授权事件入队，不表示授权已生效，也不需要重放业务命令来催办授权。

### 处理授权事件

[`erp-worker` 的授权循环](../../../apps/erp-server/src/worker/resource_grants.rs)用数据库和共享 `AuthorizationService` 构造 `ResourceGrantProcess::new`；构造本身不启动后台任务。每次 `tick(token, now)` 使用唯一领取令牌和秒级时间，至多处理一条事件；成功返回的布尔值表示是否领到事件。

领取后，[发布事务](src/process.rs)将领取凭据传给 identity 的 `ResourceGrantService::apply_source`。身份域复核租约后调用一次事实端口，经 `erp-workflow` 的授权事实快照与仓储核验正式审批、工作项及转派回执，经 `erp-finance` 读取开票申请与应收关联。写入时仍经 `erp-sales`、`erp-procurement` 仓储校验目标存在。取证和写入共用 `persistence-core` 的 `Executor`，授权变更与事件完成一同提交。证据错配、事实版本低于事件证据或读取失败时返回错误；已完成事件直接返回，永久终止的来源跳过取证，较旧的事实不覆盖新版本。

发布失败时，[队列](../../domains/erp-identity/src/repository/resource_grants/queue.rs)只回写仍持有原令牌的事件，并按退避规则安排重试；提交结果未知时保留租约，恢复时先检查事件是否已处理。worker 定期调用 `reconcile` 按游标逐页把已处理的正式事件重新排队，用 `health` 观察积压、最老未完成时长和失败数量。停机由[服务端进程](../../../apps/erp-server/README.md#执行与退出合同)停止领取并等待已开始的轮次。

### 查询授权就绪状态

[`source_readiness`](src/readiness.rs)接收服务端确定的 `SourceKey`，在调用方执行器中查找正式事件并重算结果：无事件是 `Absent`，来源永久终止是 `Terminated`，规则版本、事实版本和目标均匹配是 `Ready`，其余是 `NotReady`；取证失败不当作待生成。[审批接线](../../../apps/erp-server/src/wiring/workflow/resource_access.rs)只在实际授权被拒绝后用它识别当前分派尚未就绪。[资金接线](../../../apps/erp-server/src/wiring/funds_data_scope.rs)通过 `invoice_pending` 检查该申请与账号的首次开票批准及后续转派来源。就绪结果只用于区分状态，不能代替完整授权检查。

## 管理接口

[管理端 handler](../../../apps/erp-server/src/http/handler/access_control/resource_grants.rs)使用 [identity 的管理请求 DTO](../../domains/erp-identity/src/dto/resource_access.rs)，通过 `management()` 调用 `grant`（按稳定操作号授予人工来源）、`revoke`（按期望版本永久终止单个来源）、`sources`（分页查看指定账号的来源，含终止记录）或 `recompute_managed`（按固定范围分页重新排队）。这些操作校验管理员权限；后台 `reconcile` 不做这项检查，不能接到管理端。管理请求不接受客户端提交的自动分派证据。

`backfill(actor, request)` 同样检查管理员资格。它按[工作流仓储](../../domains/erp-workflow/src/repository/grant_history.rs)中的正式审批、订单分派或开票转派证据逐页处理，默认 `dry_run: true`，返回已有、永久终止、无效及可回填的统计和续页游标。显式关闭预览才追加核验通过的事件；不直接发布授权，也不重放审批、转派或财务命令。

## 如何扩展

新增自动规则时，在 identity 的[事件模型](../../domains/erp-identity/src/entity/resource_access/event.rs)定义载荷、稳定来源和事件身份，接入业务事务生产方，再扩展 [`Facts::target`](src/facts.rs) 和 [纯规则](src/rules.rs)；发布继续走 `ResourceGrantService::apply_source`，无需另建 worker。在 identity 的 [`GrantRule`](../../domains/erp-identity/src/entity/resource_access/rule.rs) 登记稳定代码、来源类型与目标资源，供重算和健康检查共用。规则调整不应改变来源身份；检查现有事件证据是否足够，以及已有授权是否需要重算。

新资源类型还要扩展 [`ResourceKind`](../../domains/erp-identity/src/entity/resource_access/source.rs)、目标校验及业务读取政策和 Mapping。需要历史支持时，在拥有领域的仓储读取证据，在 [backfill.rs](src/backfill.rs) 接入分页转换；调整管理参数或返回值时同步更新管理 handler。

## 验证

仅改文档时按[后端规则](../../../AGENTS.md)检查 diff。修改实现时，在 `backend/` 运行本 crate 和身份域的库测试：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-resource-grants -p erp-identity --lib --locked
```

修改规则或生产方时，覆盖有效分派、错配证据及相应维护路径，并检查受影响业务 crate；修改跨域依赖时执行 `./scripts/check-domain-boundaries.sh --cutover`。事务、并发消费和恢复的上线验收口径见[记录授权设计](../../../../docs/resource-authorization-design.md#5-验收)，不能以库测试结果代替真实环境验收。
