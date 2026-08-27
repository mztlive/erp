# 审批流程运维手册

> 状态：运维合同
>
> 本手册定义监控、告警、阻塞处理、通知 outbox 和开发环境硬切换操作。
> 不得改变业务语义。业务不变量见 [approval-workflow-contract.md](../approval-workflow-contract.md)。
> 错误码见 [approval-workflow-error-catalog.md](../approval-workflow-error-catalog.md)。
> 线协议见 [approval-workflow-openapi.yaml](../approval-workflow-openapi.yaml)。
> 重置脚本参数见 [reset-dev-business-data.md](../../backend/scripts/reset-dev-business-data.md)。
> 切换顺序见 [09-development-reset-and-cutover.md](../approval-workflow-implementation-plan/09-development-reset-and-cutover.md) 与 [approval-workflow.md](../dev-plan/approval-workflow.md) §5。

`bpm` 是无 ERP、无 I/O 的下层流程引擎。`services::approval` 是 ERP 适配和事务编排层。
运维操作必须通过已发布命令、HTTP 受控端口和登记脚本完成。

## 1. 禁止项

值班与实施人员必须遵守：

1. 不得直接改库跳过节点、重开旧任务、改写审计、改写冻结 `subject_version` 或手工修补命令收据。
2. 不得恢复旧二进制、重建旧索引、读取旧审批集合或回退旧运行时。
3. 不得启用双写、兼容读取、默认办理人或 `Noop` 领域动作。
4. 不得新增全局审批运行开关，也不得引入 `DISABLED`/`ENABLED` 运行模式。
5. 不得对 `INTERNAL_INVARIANT_BROKEN` 构造半结构终态。
6. 日志、preview 报告和进程回显不得输出 URI、Token、密码、证书或完整连接串。

## 2. 监控与告警

必须持续观察下列指标。任一越过阈值必须按本手册处置，不得静默忽略。

| 指标 | 完成条件 / 告警 | 责任 |
| --- | --- | --- |
| `BLOCKED` 实例数量 | 任一环境出现新增 `BLOCKED` 必须告警；共享开发环境连续存在超过 30 分钟必须升级 | 运行管理员 |
| 单个实例最长 `BLOCKED` 持续时间 | 人员失效超过 4 小时仍未恢复必须升级；结构性 blocker 出现即升级 | 运行管理员 |
| 按 `blocker_code` 分类计数 | 分类必须与合同 §12.2 一致；未知码视为 `INTERNAL_INVARIANT_BROKEN` | 运行管理员 |
| 决定/恢复/受阻取消 P99 延迟 | 超过 3 秒必须检查 CAS 冲突率与仓储 | 值班 |
| `409` 版本/幂等冲突率 | 同一实例 5 分钟内超过 10 次必须检查重复提交或时钟/重试 | 值班 |
| outbox backlog | 待投递超过 50 条必须告警 | 通知值班 |
| outbox oldest age | 超过当前退避档上限加 10 分钟必须告警 | 通知值班 |
| outbox retry / dead letter | 任一消息进入 dead letter 必须告警，不得静默丢弃 | 通知值班 |
| readiness | `APPROVAL_POLICY_NOT_REGISTERED` 必须使 readiness 失败并停止发布定义 | 值班 |
| ACTIVE 执行与 OPEN 任务一致性 | 任一违反第 4 节断言必须立即停写该实例并升级 | 运行管理员 |

## 3. BLOCKED 排查

### 3.1 输入

- 实例 ID 或单据业务编号
- `correlation_id`
- 实例状态、当前执行状态、`blocker_code`
- `GET /admin/approval-instances/{id}/recovery-options` 的返回

### 3.2 分类与唯一动作

必须先读取 `recovery-options`，只允许执行其返回的动作：

| 类别 | `blocker_code` | 唯一合法动作 |
| --- | --- | --- |
| 人员失效 | `APPROVER_ACCOUNT_INACTIVE`、`APPROVER_EMPLOYMENT_INVALID`、`APPROVER_NOT_ELIGIBLE`、`APPROVER_OUT_OF_DATA_SCOPE`、`APPROVER_CANNOT_READ_SUBJECT`、`SEPARATION_OF_DUTIES_VIOLATION` | 原审批人重新合格后只允许 `resume-current-approver`；仍失效则保持受阻并升级处置 |
| 图或关联损坏 | `DEFINITION_GRAPH_CORRUPTED`、`INSTANCE_LINK_CORRUPTED` | 只允许 `cancel-blocked` |
| 任务冲突 | `OPEN_TASK_CONFLICT` | 只允许 `cancel-blocked` |
| 版本损坏 | `SUBJECT_VERSION_CONFLICT` | 只允许 `cancel-blocked` |
| 内部不变量 | `INTERNAL_INVARIANT_BROKEN` | 保持冻结、readiness 失败并前向修复代码 |

### 3.3 动作与完成条件

1. 人员失效：运行管理员必须具备该类型 `approval_runtime_admin` 与实例 DataScope。原审批人重新合格前必须保持受阻；恢复成功后必须出现新 `ACTIVE` 执行和新 `OPEN` 任务，旧任务保持 `CLOSED`。审批运行时不得转交或改派。
2. 非人员一致性 blocker：必须填写原因并执行政策绑定的同一 `cancel_action`。成功后执行和实例均为 `CANCELLED`。人员失效调用受阻取消必须返回 `APPROVAL_BLOCKED_CANCEL_NOT_ALLOWED`。
3. 原提交人只允许在业务资源 `allowed_actions` 允许时撤回；具备类型运行管理权的管理员可填写原因应急撤回。非人员一致性 blocker 不得走普通撤回。
4. `INTERNAL_INVARIANT_BROKEN`：必须停写该实例、保留日志与 correlation ID、前向修复后部署。不得直接改库，不得手工取消。

## 4. ACTIVE 执行与 OPEN WorkItem 不一致

必须持续证明：

1. 每个 `ACTIVE` 节点执行恰好一个 `OPEN` 且 `owner_user_id` 非空的 `DocumentApproval` WorkItem。
2. `BLOCKED` 执行不得存在 `OPEN` WorkItem；人员失效进入受阻时旧任务关闭原因必须为 `APPROVAL_RUNTIME_BLOCKED`。
3. 同一 `approval_node_execution_id` 全生命周期最多一个 WorkItem。
4. 不得重开旧任务来修复不一致。

发现不一致时必须：

1. 立即停止对该实例的决定写入。
2. 读取 `recovery-options` 与审计/收据，确认是否已有合法 BLOCKED 或取消事实。
3. 若能形成合同 §12.2 的结构化 blocker，只允许走对应恢复或受阻取消端口。
4. 若不能形成合法阻塞或取消计划，按 `INTERNAL_INVARIANT_BROKEN` 冻结并前向修复。
5. 完成条件：断言恢复为第 1—3 条，且审计记录完整。不得 `update` 任务状态或删除多余 WorkItem。

## 5. 决定、CAS 与幂等冲突

### 5.1 判定

| 现象 | 稳定码 | 处置 |
| --- | --- | --- |
| 任务/实例/执行/定义锁版本过期 | `APPROVAL_*_VERSION_CONFLICT` | 读取 409 回读版本，由用户显式重提同一意图 |
| 同幂等键不同 payload | `APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT` | 停止自动重放；新意图必须使用新幂等键 |
| 同幂等键同 payload 且仍有权 | 2xx `IDEMPOTENT_REPLAY` | 展示最新摘要，不得提示「重复请求」 |
| 人员失效已提交阻塞 | `APPROVAL_INSTANCE_BLOCKED` | 按第 3 节处置；不得重试决定 |
| 未接入类型 | `APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER` | 不得回退旧路径 |

### 5.2 延迟

决定 P99 超过 3 秒时必须检查：冲突率、仓储 CAS、通知 outbox 追加是否仍在同一事务内（必须在事务内只追加 outbox，不得在事务内调用外部通知）。不得通过跳过版本校验换取延迟下降。

## 6. 通知 outbox

事务内只允许追加 outbox。投递由独立 worker 在事务外完成。去重、收件人与模板以合同 §16.5 为准。

| 观察项 | 必须动作 |
| --- | --- |
| backlog 增长 | 检查 worker 是否运行、租约是否过期未接管、外部发送接口是否超时 |
| oldest age 超过当前退避档 | 第 1—5 次失败后的退避必须为 1 分钟、5 分钟、15 分钟、1 小时、6 小时；不得手工改重试计数 |
| retry 耗尽 | 第 6 次失败必须进入 dead letter 并告警 |
| dead letter | 核对去重键与模板参数后，仅允许修复发送依赖并前向重放该去重键；不得删除死信伪装成功 |
| 租约残留 | 仅允许等待租约到期由其他实例接管；不得手工改 worker ID 抢锁 |

模板参数只允许单据类型中文名、业务编号、当前节点名称、当前审批人显示名、轮次号和驳回原因摘要。不得包含 Token、金额明细或完整单据。

## 7. 开发环境 reset

本专项不建设数据迁移。只允许在已确认无必须保留业务数据的开发环境执行。入口：

```bash
backend/scripts/reset-dev-business-data.sh
```

### 7.1 preview

必须先停写 Web API、前端开发服务、通知 worker 和全部业务写入进程，再运行默认 preview。完成条件：

1. 输出目标主机（不含 URI）、数据库名、集合摘要、allowlist 与预计删除数量。
2. preview 与后续 execute/verify 使用同一目标与集合摘要。
3. 未输出连接串、Token、密码或证书。

### 7.2 显式确认

执行人必须确认：当前是开发环境、不存在必须保留的业务/审批数据、账号 RBAC 主数据按脚本保护合同保留、目标主机与库名通过安全门禁。任一不成立必须停止。

### 7.3 execute

本地：

```bash
backend/scripts/reset-dev-business-data.sh \
  --execute \
  --confirm-db <database.db_name> \
  --expect-summary <preview 集合摘要>
```

远程开发库还必须 `--allow-remote`，并设置精确 `ERP_RESET_ALLOWED_REMOTE_HOSTS`。缺少任一参数、摘要不一致或系统库必须失败关闭。不得调用 `dropDatabase()`。

### 7.4 verify

```bash
backend/scripts/reset-dev-business-data.sh \
  --verify \
  --expect-summary <preview 集合摘要>
```

完成条件必须同时成立：

1. 旧审批集合与新 BPM/集成集合均为空或不存在。
2. `work_items` 中旧卡券审批类型、`DOCUMENT_APPROVAL`、`approval_step_instance_id` 与 `approval_node_execution_id` 均为 0。
3. 冲突索引 allowlist 中的索引不存在。
4. 账号、RBAC、组织、主数据、`source_systems`、`file_assets` 和计数器仍在。
5. 脚本以零状态退出。

中断后必须保持停写，并以完全相同命令幂等重跑。不得在半清理状态恢复写入。

## 8. 逐类型启用顺序

不得设置全局运行开关。某 `PROCESS_REQUIRED` 类型不存在已发布定义时，创建必须返回 `APPROVAL_PROCESS_NOT_CONFIGURED`。发布定义即该类型实质启用。

固定顺序：

1. 停止写入并完成第 7 节 preview / 确认 / execute / verify。
2. 部署只包含新审批模型的后端与前端。
3. 创建并验证全部新索引；任一索引失败不得继续发布定义。
4. 运行 readiness、权限种子、边界扫描和旧符号清零。`APPROVAL_POLICY_NOT_REGISTERED` 必须使 readiness 失败。
5. 仅先为唯一试点 `StockAdjustment` 创建并发布定义，完成创建、绑定、提交、决定、恢复、取消和通知冒烟。
6. 试点通过后，按 [approval-workflow.md](../dev-plan/approval-workflow.md) §3 对其余 10 个 `PROCESS_REQUIRED` 类型逐个发布定义并完成同一组冒烟：
   `SalesOrder` → `VoucherSalesOrder` → `SalesChangeOrder` → `PurchaseOrder` → `PurchaseChangeOrder` → `CustomerReceipt` → `CustomerRefund` → `SupplierRefund` → `ReceiptReversal` → `PaymentReversal`。
7. `SupplierPayment` 必须按 `NO_APPROVAL` 冒烟：采购单最终通过形成应付和指定出纳付款任务；工作台展示收款账户摘要；当前责任人可审计揭示完整账号；提交携带页面所见账户 ID 与版本并在付款事务内完成 CAS 写锁；连续两次部分付款必须使用不同会话与幂等键并各自产生一条过账付款记录；全部付款均在同一事务登记、过账、核销并更新任务，且审批绑定、实例和审批任务均为 0。
8. 其余 `NO_APPROVAL` 类型不得发布定义、不得创建审批实例。
9. P6-PILOT 只允许在专用空数据库演练试点，不得触碰共享开发环境。共享开发环境只在 P6-FINAL 启用前门禁通过后执行第 1—7 步。

## 9. 启用失败

任一类型冒烟或发布失败时必须：

1. 立即退役该类型已发布定义，停止其新单据进入审批；已通过的类型不得一并停摆。
2. 必要时退役全部已发布定义并停止全部业务写入。
3. 保留日志、correlation ID、索引和失败证据。
4. 修复代码或合同后前向部署。
5. 若数据已被污染，按第 7 节再次硬重置。
6. 重新完成索引、readiness 和启用前门禁后才能重新发布定义，并重做试点与受影响类型冒烟。

退役只阻止新绑定，不影响已运行实例。必须同时终止在途实例时，只允许合同 §12.1 撤回或 §12.5 受阻取消，不得直接改库。
