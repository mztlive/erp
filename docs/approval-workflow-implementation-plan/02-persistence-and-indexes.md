# 阶段 02：持久化、索引与 Repository

> 阶段性质：P2 Repository 工作包
>
> 阶段目标：为 `bpm` 模型和 ERP 审批集成实体建立物理约束、查询能力和原子 CAS，不把 MongoDB 依赖反向引入 BPM
>
> 允许状态：可引用阶段 00、01 冻结的目标类型；共享扩展入口必须已由 P0 注册

## 1. 文件责任

本阶段负责：

- `backend/database/src/repository/bpm.rs` 的 BPM 模型持久化实现；
- `backend/database/src/repository/extensions/bpm.rs` 的目标 trait；
- `backend/database/src/repository/approval_integration.rs` 和 `repository/extensions/approval_integration.rs` 的业务对象快照/outbox 实现；
- `backend/database/src/indexes/bpm.rs` 和 `indexes/approval_integration.rs`；
- `backend/database/src/indexes/work_item.rs` 中审批任务索引；
- `backend/database/src/repository/work_item.rs` 中审批执行关联的持久化能力；
- `backend/database/src/indexes/document_registry.rs` 中 `BusinessDocument` 非空编号部分唯一索引；
- `backend/database/src/repository/document_registry.rs` 中可空编号注册、一次性编号赋值与 CAS；
- 上述模块内不依赖外部 MongoDB 的单元测试。

`database` 必须直接依赖 `bpm` 和 `entities`：流程定义、实例、执行、审批人和命令收据使用 `bpm` 模型；业务对象快照、WorkItem 和通知 outbox 使用 `entities` 集成模型。Repository 只负责 MongoDB 映射和原子读写，不得把 BSON、MongoDB、`Executor` 或 Repository trait 放进 `bpm`。

本阶段不负责应用状态流转、BPM 图/状态决策、HTTP、开发数据重置脚本、MongoDB/HTTP 集成测试和共享 `mod.rs` 接线。试点真实数据库集成测试只由 P6-PILOT 编写，全量补齐只由 P6-FINAL 编写。

## 2. 集合合同

新运行时使用独立集合，避免旧 `step` 模型与新 `node execution` 模型在同一集合内混写：

| 集合 | 实体 | 写入规则 |
| --- | --- | --- |
| `approval_process_definitions` | `ApprovalProcessDefinition` | 草稿可更新；发布/退役后不可改结构 |
| `approval_node_definitions` | `ApprovalNodeDefinition` | 只随草稿整组替换 |
| `approval_transition_definitions` | `ApprovalTransitionDefinition` | 服务端生成；只随草稿整组替换 |
| `approval_process_instances` | `ApprovalProcessInstance` | 运行状态源，使用实例版本 CAS |
| `approval_node_executions` | `ApprovalNodeExecution` | 每次进入新建，结束事实不可覆盖 |
| `approval_instance_assignees` | `ApprovalInstanceAssignee` | 启动时冻结，改派使用分派版本 CAS |
| `approval_command_receipts` | `ApprovalCommandReceipt` | 只追加，永久幂等 |
| `approval_subject_snapshots` | `entities::approval_integration::ApprovalSubjectSnapshot` | 启动时与实例同事务写入；写后不可变 |
| `approval_notification_outbox` | `entities::approval_integration::ApprovalNotificationOutbox` | 与运行事务一起追加；投递状态由独立 worker 更新 |
目标集合必须作为独立新增集合落地。既有集合只为未切换代码保持可编译，任何目标路径不得读取、写入或回退到既有集合；`P0-D` 删除旧 Repository 和索引注册，阶段 09 在开发环境硬切换时清空旧集合。新代码不得双写。

## 3. 索引合同

### 3.1 流程定义

必须创建：

```text
uk_approval_process_definitions_id
uk_approval_process_definitions_kind_version
uk_approval_process_definitions_published_kind
uk_approval_process_definitions_active_draft_kind
idx_approval_process_definitions_history
```

键：

- `(process_kind, definition_version)` 唯一；
- `process_kind` 在 `status=PUBLISHED` 部分唯一；
- `(process_kind, definition_version desc)` 历史查询；
- 同一 `process_kind` 活动草稿部分唯一，落实合同“同时最多一个活动草稿”。

MongoDB 中的 `process_kind` 只保存 `bpm::ProcessKind` 稳定值。Repository 不得解析 `DocumentType`；`DocumentType <-> ProcessKind` 的注册和反向校验只允许由 P0 冻结的 `services::approval::process_kind` 完成。

### 3.2 节点与连线

必须创建：

- 节点 `(approval_process_definition_id, node_key)` 唯一；
- 节点 `(approval_process_definition_id, display_order)` 唯一；
- 连线 `(approval_process_definition_id, from_node_key, event)` 唯一；
- 按 definition ID 批量读取节点和连线的覆盖索引。

### 3.3 运行实例

必须创建：

- `(subject.kind, subject.id, subject_version)` 在 `RUNNING|BLOCKED` 状态部分唯一；定义 ID 不得进入该唯一键，避免同一业务版本因绑定漂移产生两条活动链；
- `(subject.kind, subject.id, started_at desc)` 历史索引；
- `(status, blocked_at desc, id desc)` 公司级阻塞列表索引；
- `(started_by, started_at desc, id desc)` “我发起的审批”索引；
- `(updated_at desc, id desc)` 管理范围未指定状态的实例列表索引；
- `(status, updated_at desc, id desc)` 管理范围实例列表索引。

启动幂等不得在实例上保存第二套 `start_idempotency_key`；唯一来源是 `approval_command_receipts` 的 `(command_kind, scope_id, idempotency_key)` 索引与 payload hash。

实例必须保存用于列表读取的有界投影：当前节点、当前审批人、最近驳回执行 ID、最近驳回原因摘要和最近状态变更时间。投影只能在运行事务中更新，列表不得逐实例扫描执行历史获取最近驳回事实。

不得保留 `runtime_kind + external_instance_id` 索引。

### 3.4 节点执行

必须创建：

- `(approval_process_instance_id, execution_no)` 唯一；
- `(approval_process_instance_id, round_no, node_key, execution_no)` 历史索引；同轮同节点因人员改派可产生新执行，不得仅按前三项唯一；
- `approval_process_instance_id` 在 `ACTIVE|BLOCKED` 状态部分唯一；
- `(approval_process_instance_id, round_no, execution_no)` 历史索引；
- `(assignee_participant_id, status, activated_at)` BPM 当前责任辅助索引；待我审批的用户查询必须走 WorkItem 的 `owner_user_id` 索引。

同一节点允许跨轮次重复，不得继续使用旧 `(instance_id, step_key)` 唯一索引。

### 3.5 实例审批人和任务

必须创建：

- 实例审批人 `(approval_process_instance_id, node_key)` 唯一；
- 审批任务 `approval_node_execution_id` 在字段存在且为字符串时部分唯一，不得按状态过滤；
- 审批任务 `(status, owner_user_id, assigned_at desc, id desc)` 待我审批索引。

硬切换时删除旧字段 `work_items.approval_step_instance_id` 和旧索引 `uk_work_items_open_approval_step`，并创建 `approval_node_execution_id` 与 `uk_work_items_approval_execution`。该约束必须保证同一执行在 OPEN、COMPLETED、CLOSED 等全部状态合计最多一条任务。

按合同 §13.2，全系统不再存在 `POOL` 任务：`assignment_mode` 字段随实体删除，因此按该字段过滤的团队队列索引必须一并删除；非审批任务的对象唯一索引保留并改为在 `owner_user_id` 必填前提下建立。`(status, owner_user_id, due_at, id)` 是合同 §16.4 统一工作台的主查询索引，必须覆盖审批与非审批任务的同一口径查询。

采购确认与低毛利持久化实现不得被目标路径调用。对应 `sales_review` Repository、扩展、索引和销售单低毛利过滤分支由 `P0-D` 在全部调用方切换后删除；P2 不得提前删除仍被未切换代码引用的符号。

### 3.6 业务对象快照、收据与通知

业务对象快照必须以 `approval_process_instance_id` 唯一，并提供 `(document_type, business_object_id, subject_version)` 查询索引。命令收据必须以 `(command_kind, scope_id, idempotency_key)` 唯一，并保存 `payload_hash`。通知 outbox 必须以业务事件去重键唯一，并提供 `(delivery_status, next_attempt_at)`、`(lease_expires_at, delivery_status)` 有界批量索引和死信查询索引。

### 3.7 `BusinessDocument` 草稿编号

`business_documents` 的 `(document_type, document_no)` 与 `document_no` 搜索索引只允许覆盖非空字符串。Repository 必须允许业务实体创建事务以空编号注册稳定 `document_id`，并提供以 `id + document_no is null + expected_version` 为条件的一次性编号赋值；赋值必须同时写 `document_no_assigned_at`，不得覆盖已有编号。重复同载荷必须回读同一结果，不同编号竞争只能一个成功。

## 4. Repository API

`BpmExt` 必须暴露 BPM 目标集合 Repository，并提供聚合仓储 `bpm_workflow()`；`ApprovalIntegrationExt` 必须仅暴露业务对象快照和通知 outbox Repository。不得用一个 Repository 返回混合 BPM 与业务聚合。至少实现：

### 4.1 定义查询与命令

```text
find_published_by_process_kind(process_kind, executor)
find_definition_version(process_kind, version, executor)
list_definition_versions(process_kind, executor)
find_active_draft(process_kind, executor)
load_definition_graph(definition_id, executor)
replace_draft_graph(definition, nodes, transitions, executor)
publish_and_retire_previous(definition, previous, executor)
```

`load_definition_graph` 必须批量读取节点和连线，不得逐节点 N+1。

### 4.2 实例与执行

```text
find_non_terminal_by_subject(...)
find_current_execution(instance_id, executor)
list_execution_history(instance_id, after_execution_no, limit, executor)
list_instance_assignees(instance_id, executor)
find_instance_assignee(instance_id, node_key, executor)
create_bpm_runtime(instance, assignees, first_execution, receipt, executor)
```

`create_bpm_runtime` 只写 BPM 模型。Service 必须在同一事务中另行调用 WorkItem Repository 和 `ApprovalIntegrationExt` 写入业务对象快照、任务与 outbox；BPM Repository 不得接收 ERP 实体。全部写入只执行已由 Service 根据 BPM `TransitionPlan` 和业务校验形成的事实，不得决定节点、审批人、业务动作或 WorkItem 路由。

Repository 还必须提供：

```text
list_instance_summaries(filter, cursor, limit, executor)
lease_outbox_batch(worker_id, now, lease_until, limit, executor)
mark_outbox_delivered(outbox_id, expected_lease_owner, delivered_at, executor)
reschedule_outbox(outbox_id, expected_lease_owner, next_attempt_at, error_kind, executor)
dead_letter_outbox(outbox_id, expected_lease_owner, error_kind, executor)
```

outbox 取租约必须使用原子条件更新，两个 worker 不得同时取得同一消息。租约到期后允许其他 worker 重新取得；成功投递后不得再次取得。此处租约是消息投递机制，与已删除的人工任务领取语义无关。

### 4.3 CAS 命令

必须为以下写入提供带版本及当前状态条件的原子更新：

- 草稿定义更新：`id + DRAFT + expected_definition_lock_version`；
- 实例推进：`id + expected_instance_version + current_execution_id + RUNNING|BLOCKED`；
- 执行结束：`id + expected_execution_version + ACTIVE`；
- 阻塞执行结束：`id + expected_execution_version + BLOCKED`，成功后固定写 `SUPERSEDED` 和结束原因；
- 实例审批人改派：`instance_id + node_key + expected_assignment_version`；
- 审批任务完成/关闭：`id + OPEN + expected_task_version + approval_node_execution_id`。改派和人员恢复不得更新旧 CLOSED 任务，只能为新执行插入新任务。

CAS 未命中必须区分“不存在”“版本冲突”“当前状态变化”，统一交给应用层映射稳定 404/409，Repository 不得返回用户文案。

## 5. 事务约束

1. Repository 每个方法继续接收 `&mut dyn Executor`；
2. Repository 不得自行开启事务；
3. 多集合写入只允许 Service 通过 `Transactional::with_transaction` 建立唯一事务；
4. 事务内不得执行外部 HTTP 或通知发送；
5. MongoDB transient transaction error 允许由最外层按仓库既有策略重试；命令幂等收据必须防止重复领域动作；
6. 索引创建失败必须阻止应用就绪，不得忽略。
7. BPM 引擎不接收 `Executor`，Repository 不调用 BPM 决策函数；Service 必须先取得 `TransitionPlan`，再在唯一事务中执行 CAS 和集成写入。

## 6. 查询和 DataScope

Repository 只接收 Service 已计算的授权范围过滤条件，但必须在 MongoDB 查询中执行，不得先全量读取后过滤。至少支持：

- 当前用户的开放审批任务；
- 当前用户发起的审批；
- 管理权限范围内的实例和阻塞实例；
- 单据参与者可见的新运行历史；
- 按实例、轮次、执行序号稳定排序的历史。

所有分页必须有上限；定义节点最多 20 条可一次读取，实例历史必须使用稳定游标分页。详情响应只返回实例摘要和最近受控条数，不得内嵌无界历史数组。`mine/started/managed/blocked` 必须分别使用阶段 06 冻结的排序字段和匹配索引；`managed` 未指定状态时使用 `(updated_at, id)` 索引，指定状态时使用 `(status, updated_at, id)` 索引。必须在 MongoDB 查询中同时施加 `document_type`、状态和 DataScope 过滤；不得先读取无界 ID 集合再在内存排序或过滤。P6 必须对每个 view 的无可选过滤、仅 `document_type`、仅 `status`和两者同时存在的合法查询形状执行 `explain`；出现 blocking sort 或无界扫描时必须补目标复合索引，不得以页大小作为豁免。

## 7. 开发重置脚本交接

阶段 09 必须同步修改：

- `backend/scripts/reset-dev-business-data.mongosh.js`；
- `backend/scripts/reset-dev-business-data.md`；
- `backend/scripts/test-reset-dev-business-data.sh`。

P2 只交付集合名称清单，不得修改或执行数据重置脚本。阶段 09 将旧审批集合、新审批运行集合、收据、通知 outbox 和审批 WorkItem 加入开发业务数据重置范围；账号、RBAC、审计、主数据和对象存储继续受保护。

## 8. P6-PILOT 与 P6-FINAL 集成测试合同

阶段 11 / P6 必须新增或重写：

- `backend/database/tests/approval_workflow_repository.rs`；
- `backend/apps/web-api/tests/approval_workflow_invariants.rs` 中跨集合不变量用例。

本阶段只在索引模块内编写纯结构单元测试，不得在 `backend/database/tests/**` 新增或修改文件。

必须覆盖：

1. 同一 `ProcessKind` 只能有一个 `PUBLISHED`；
2. 同一 `ProcessKind` 只能有一个活动草稿；
3. 同实例只能有一个 `ACTIVE|BLOCKED` 执行；
4. 同节点可在不同轮次创建执行；同轮改派或原审批人恢复时旧执行转为 `SUPERSEDED`，新执行可创建；
5. 每个执行在全部任务状态合计最多一个关联任务；
6. 所有 CAS 的陈旧版本只允许一个写入成功；
7. 收据同键同载荷回读、同键异载荷冲突；
8. 查询使用预期索引键顺序和部分过滤条件；
9. outbox 竞争取租约、租约到期、重试上限和死信；
10. 历史游标稳定且单次返回不超过最大页大小；四种列表 view 的全部合法可选过滤组合都由 explain 证明排序命中匹配索引且无无界扫描。
11. 每个 BPM 实例恰有一个不可变业务对象快照，快照查询不依赖扫描执行历史。
12. 多个空编号草稿可并存；非空 `(document_type, document_no)` 仍唯一；一次性编号赋值的并发竞争只能一个成功。

## 9. 阶段验收

- [ ] 新运行集合与旧集合物理隔离，不存在双写。
- [ ] `database -> bpm + entities` 为单向依赖，`bpm` 不依赖 MongoDB 或 database。
- [ ] 全部唯一性和当前令牌不变量有数据库约束。
- [ ] Repository 不含流程路由、资格、`DocumentType` 政策映射或领域状态判断。
- [ ] 所有新增查询均有匹配索引和有界分页。
- [ ] outbox 取租约和重试查询均有匹配索引。
- [ ] 目标 Repository 和索引不包含旧责任模式、采购确认或低毛利集合引用；全仓旧索引零命中由 `P0-D` 验收。
- [ ] 业务对象快照与 BPM 实例一一对应、写后不可变且有匹配索引。
- [ ] `BusinessDocument` 空编号草稿可注册，非空编号部分唯一，一次性赋值具备 CAS 和单元测试。
- [ ] 本阶段未修改 `backend/database/tests/**`。
- [ ] `conventions.md` 第 6 节全部后端门禁、`cargo test -p database --lib` 和 `./scripts/check-bpm-boundaries.sh` 通过；真实 MongoDB 验收由阶段 11 执行。
