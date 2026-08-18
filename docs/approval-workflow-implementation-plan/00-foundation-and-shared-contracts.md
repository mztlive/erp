# 阶段 00：P0 共享地基与冻结合同

> 阶段性质：顺序前置地基
>
> 阶段目标：在业务实现开始前建立 `bpm` crate、冻结单向依赖、跨层名称、公共入口和文件所有权，并提供可合并、可编译的最小地基
>
> 分支要求：`chore/erp-p0-amend-approval-workflow-foundation`

## 1. 执行边界

本阶段是 P1—P5 的共享入口责任。共享修改按四个可独立合并的波次执行：

| 波次 | 时点 | 单一责任 |
| --- | --- | --- |
| P0-A | P1—P5 开始前 | `bpm` workspace/manifest、依赖边界、ID、模块名、公共错误、端口签名和可编译占位 |
| P0-B | P3 HTTP/Service 产物稳定后、P6-PILOT 前 | `AppState` 注入、Handler/route 合并、断开旧 bootstrap 启动调用、权限扫描和生成 |
| P0-C | 通用与试点 P4 产物稳定后、P6-PILOT 前 | W24 workspace 注册和前端权限生成 |
| P0-D | 全部逐类型 P3/P4 阶段完成后、P6-FINAL 前 | 删除为准备期编译保留的旧模型、旧运行时、旧责任动作、旧端点和旧权限 |

每个波次合并后，受影响实施分支必须 rebase 并且只修改自己的 owns 前缀。P0-A 先行不表示 P0-B/P0-C 可以合并到 P6；P0-B 和 P0-C 必须在 P6-PILOT 前分别独立完成。P0-D 只能在 `_meta.json` 的 `ALL_DOCUMENT_TYPE_ROLLOUTS` 完成后开始，且必须在 P6-FINAL 前独立合并。

本阶段只允许修改 `_meta.json` 中 `P0-A` 的 `owns`、`ownsWithin` 与 `creates` 登记的文件，即：

```text
backend/AGENTS.md
backend/Cargo.toml
backend/crates/bpm/{Cargo.toml,src/lib.rs,src/ids.rs,src/error.rs}
backend/crates/entity-macros/src/lib.rs
backend/{entities,database,services}/Cargo.toml
backend/entities/src/{lib.rs,ids.rs}
backend/entities/src/document_registry/business_document.rs   仅 DocumentType 枚举
backend/database/src/{lib.rs,repository/mod.rs,repository/extensions/mod.rs,indexes/mod.rs}
backend/services/src/approval/{mod.rs,process_kind.rs}
backend/scripts/check-bpm-boundaries.sh
以及 creates 登记的全部目标模块占位文件
```

`ApprovalDefinitionBinding` 值对象虽然位于 `business_document.rs`，但按 `ownsWithin` 归阶段 01；本阶段不得提前实现它。现有审批 ID 在准备阶段保持原调用方可编译；目标代码必须直接消费 `bpm` ID，旧 ID 与调用方由 P0-D 在全类型切换后一起删除。占位只声明稳定类型或空模块入口，不得注册可被业务调用的伪引擎。P0-B 必须在真实 Service 已存在后再注入 `AppState` 和路由，不得用 Noop 或 `Option` 绕过依赖。

若一个 PR 无法以单一地基主题描述，必须拆成多个 `chore/erp-p0-amend-<主题>` PR，逐个合并。

## 2. 必须冻结的名称

### 2.1 BPM crate、领域与 ID

P0-A 必须在 `backend/Cargo.toml` 完成：

- 将 `crates/bpm` 加入 workspace members；
- 将 `bpm = { path = "crates/bpm" }` 加入 `[workspace.dependencies]`；
- 使 `entities`、`database`、`services` 通过 `workspace = true` 直接依赖 `bpm`；
- `apps/web-api` 的审批 Handler 与审批路由只通过 `services` 使用 BPM 应用能力，不得直接调用 `bpm` 或审批 Repository；本条不得被扩大解释为清理其他业务域现有的基础设施依赖；
- 不得在成员 manifest 重复声明 BPM path、版本或外部依赖版本。

P0-A 必须同步更新 `backend/AGENTS.md`：将 `crates/bpm` 登记为纯流程领域和状态引擎；将 `entities` 登记为 ERP 业务实体及 BPM 集成引用；将 `services::approval` 登记为政策、授权、事务和业务副作用适配层；将 `database` 登记为 BPM/业务模型的 MongoDB 适配层。仓库指南不得继续把审批领域模型归入 `entities/src/approval` 或把状态机归入 Service。

`backend/crates/bpm/Cargo.toml` 只允许依赖 `entity-core`、`entity-macros` 和 `serde`、`chrono`、`thiserror` 等 workspace 外部基础库。不得依赖 `entities`、`database`、`services`、`config`、`apps/web-api`、`mongodb`、`axum`、`id-generator`、权限宏或通知客户端。

关于 ID 与时间的两条必须遵守的实现约束：

1. `id_type!` 当前是 `backend/entities/src/ids.rs` 内的私有 `macro_rules!`，`bpm` 无法引用。`entity-macros` 是 `proc-macro` crate，因此 P0-A 必须把 `id_type!` 实现为 `#[proc_macro]` 函数式过程宏，由 `entities` 与 `bpm` 共用；禁止尝试从 `proc-macro` crate 导出普通 `macro_rules!`，也不得在 `bpm` 内复制第二份宏或手写重复 newtype 样板。`bpm` 不依赖 `id-generator`：ID 值一律由调用方生成并传入；
2. `entity-core::BaseModel::new()` 内部调用 `chrono::Local::now()`。若 `bpm` 模型复用 `BaseModel` 承载 `version`，**不得**调用 `BaseModel::new()`，只允许由调用方提供的 `Timestamp` 逐字段构造 `BaseModel`。`check-bpm-boundaries.sh` 必须同时禁止 `bpm` 源码中出现 `Local::now`、`Utc::now`、`SystemTime::now` 和 `Instant::now`。

必须在 `bpm` 公共 ID 和模块入口中登记：

- `ApprovalProcessDefinitionId`；
- `ApprovalNodeDefinitionId`；
- `ApprovalTransitionDefinitionId`；
- `ApprovalProcessInstanceId`；
- `ApprovalNodeExecutionId`；
- `ApprovalInstanceAssigneeId`；
- `ApprovalCommandReceiptId`。

必须同时冻结下列无 ERP 语义的 BPM 边界类型：

- `ProcessKind`：流程种类稳定值，不得直接使用 `entities::DocumentType`；
- `SubjectRef`：由 `subject_kind + subject_id` 组成的业务对象引用；
- `ParticipantId`：BPM 对处理人的不透明引用；
- `Timestamp`：BPM 自有 UTC 时间值对象；不得引用 `entities::common::time::Instant`；
- `TransitionPlan` 和 `BpmEvent` 的公共模块入口，P0 只建立不可调用占位，具体行为由阶段 05 实现。

上述审批 ID 不得进入任何目标模块。准备阶段允许既有 `entities::ids` 定义仅供未切换调用方编译；全部目标调用方必须直接引用 `bpm::...`。P0-D 必须在逐类型调用方切换完成后删除旧 ID、`entities/src/lib.rs` 旧入口和旧模型。硬切换后，生产代码、测试和工具均不得引用旧类型。

`ApprovalSubjectSnapshotId` 和 `ApprovalNotificationOutboxId` 属于 ERP 集成实体，必须继续由 `entities` 定义；不得因名称含 approval 而移入 `bpm`。

### 2.2 WorkItem

必须按合同 §13.3 冻结：

- 新增 `WorkItemType::DocumentApproval`；
- 新增字段 `approval_node_execution_id`，替换旧 `approval_step_instance_id`；
- 新增 `AssignmentSource::ApprovalRuntime`。仓库现有 6 个值 `StepResolver`、`SystemRule`、`SelfStart`、`AdminReassign`、`AdminRelease`、`RecoveryResolver` 中不存在等价语义，必须新增而不是复用；最终只保留 `SystemRule`、`AdminReassign`、`ApprovalRuntime` 三值，删除 `StepResolver`、`SelfStart`、`AdminRelease`、`RecoveryResolver`；
- 按合同 §13.2 删除 `AssignmentMode` 枚举与 `assignment_mode` 字段，不保留只剩 `Direct` 的单值枚举，也不改名为 `responsibility_mode`。任何 `OPEN` 任务的 `owner_user_id` 必填；
- 复用**已存在**的 `WorkItemFamily::Approval`（`services/src/work_item/dto.rs`），只把 `DocumentApproval` 穷尽映射进该 family 并建立详情路由解析入口；不得新增第二个审批 family；
- 通用 WorkItem command 对审批任务返回受保护错误的公共错误码 `APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN`。

`owner_role` 固定为合同 §4.4.5 的 `<prefix>_approver`；`owner_organization_id` 固定取 `subject_snapshot.responsible_org_id`；不得用当前登录人的组织或空字符串补位。

### 2.3 权限和错误

必须冻结动作级权限：

```text
approval_process:read
approval_process:create
approval_process:edit
approval_process:publish
approval_process:retire
approval_instance:read
approval_instance:decide
approval_instance:cancel
approval_instance:resume
approval_instance:reassign
approval_instance:cancel_blocked
approval_instance:upgrade_binding
```

必须冻结公共错误枚举、HTTP envelope 接口和权限生成入口。类型级权限由政策逐项登记，但命名规则必须在本阶段固定。

`APPROVAL_POLICY_NOT_REGISTERED` 属于部署不变量错误，只允许映射为内部错误并触发启动/readiness 失败，不得映射为 4xx。

### 2.4 Repository、Service 和 HTTP 入口

P0-A 必须完成下列共享入口的最小可编译注册；P0-B/P0-C 在实现稳定后完成真实接线：

- `bpm`、entities 集成模型、database repository/index、services 适配层的模块声明与 re-export。P0-A 必须为 `_meta.json` `creates` 列出的每个目标模块创建失败关闭占位文件，使阶段 01—05 只需填充自己 `owns` 的文件，不必回头修改冻结的 `lib.rs` / `mod.rs`。现有 `approval/runtime.rs` 保持旧基线名称；新运行编排固定放入 `approval/execution/**`，不得同时创建 `approval/runtime.rs` 与 `approval/runtime/mod.rs`；
- P0-B：`AppState` 的真实 Service/worker 依赖注入；
- P0-B：approval handler、route 模块、admin route 和 `main.rs` 合并；
- P0-B：从 `main.rs` 断开 definition bootstrap 启动调用并接入 outbox worker 启停；`bootstrap.rs` 文件由 P0-D 删除；
- P0-D：在全仓目标调用方改为直接引用 `bpm` 后删除 `entities::approval` 旧入口、旧 ID 和旧模型；
- P0-B：更新 `build.rs` 权限发现范围并生成权限；
- P0-C：前端 W24、生成权限和 API 基础入口接线。

占位实现必须失败关闭，不得返回伪造成功、Noop 领域动作或默认审批人。

### 2.5 单向依赖和职责冻结

P0-A 必须新增 `backend/scripts/check-bpm-boundaries.sh`，并使其在本地和 CI 中失败关闭地验证：

1. `bpm` 的 Cargo 依赖图不包含 `entities`、`database`、`services`、`web-api`、`mongodb` 或 `axum`；
2. `backend/crates/bpm/src/**` 不引用 `DocumentType`、`WorkItem`、`DataScope`、`Permission`、`Executor`、MongoDB、HTTP、业务 Repository 或业务 action，且不出现 `Local::now`、`Utc::now`、`SystemTime::now`、`Instant::now` 或 ID 生成调用；
3. `entities`、`database`、`services` 均直接声明 `bpm = { workspace = true }`，不得依赖 `services` facade 间接取得 BPM 类型；
4. 流程定义、实例、执行、审批人和命令收据 ID 在生产代码中只有 `bpm` 一个定义源；subject snapshot/outbox ID 只有 `entities` 一个定义源；
5. `services` 的 `DocumentType -> ProcessKind` 映射入口存在并使用穷尽 `match`，不得以可缺项 HashMap 或任意字符串替代。

P0-A 必须根据合同 §4.3 的 20 个固定 `DocumentType` 实现完整双向映射和一对一完整性测试，其中包括新增的 `VoucherSalesOrder`；阶段 03、04 只能复用，不得修改或复制。边界脚本缺失、被跳过或允许禁用时，P0-A 不得合并。

## 3. `docs/dev-plan` 前置

`docs/dev-plan` 由 DOC-A（文件 10）独占并已生效，本阶段**不得**创建或修改其中任何文件。

P0-A 开始前必须确认下列文件存在且已登记本方案阶段映射、`owns` / `ownsWithin` / `creates` / `deletes`、冻结清单、分支命名和集成测试独占目录：

```text
docs/dev-plan/README.md
docs/dev-plan/conventions.md
docs/dev-plan/domains.md
docs/dev-plan/_meta.json
docs/dev-plan/approval-workflow.md
```

任一文件缺失或未登记本阶段 `owns` / `ownsWithin` / `creates` 时，必须退回 DOC-A 补齐，不得由本阶段代为创建。

## 4. 编译与合并要求

P0 不适用“允许跨层尚未接线导致编译失败”。每个地基 PR 必须可独立合并并通过：

```bash
cd backend
cargo fmt --all -- --check
cargo test -p bpm --lib
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
./scripts/check-bpm-boundaries.sh
```

前端共享入口变更还必须执行仓库现行前端静态门禁。生成文件必须由生成命令产生，不得手工编辑。

## 5. 完成条件

- [ ] DOC-A 已合并，`docs/dev-plan` 五个文件存在且已登记本阶段 `owns`、`ownsWithin`、`creates`。
- [ ] `_meta.json` `creates` 的每个占位模块都已创建、已在共享声明文件中声明、且失败关闭；`cargo check --workspace` 通过。
- [ ] 政策、WorkItem、权限和错误名称已经签署并冻结（合同 §4.3、§13.3、§15.2）。
- [ ] `DocumentType` 已含 `VoucherSalesOrder`，共 20 个值。
- [ ] `id_type!` 已实现为 `entity-macros` 的函数式过程宏，`bpm` 与 `entities` 共用同一份定义。
- [ ] `bpm` 不依赖 `id-generator`，源码中无系统时钟调用。
- [ ] `crates/bpm` 已加入 workspace，`entities/database/services -> bpm` 单向依赖可编译。
- [ ] `backend/AGENTS.md` 已登记目标依赖方向、crate 职责和禁止反向依赖。
- [ ] `bpm` 不含 ERP、MongoDB、HTTP、权限、WorkItem 或业务动作依赖。
- [ ] BPM Core ID 与 ERP 集成 ID 各自只有一个定义源，`ProcessKind`、`SubjectRef`、`ParticipantId` 和 `Timestamp` 已冻结。
- [ ] `DocumentType <-> ProcessKind` 双向映射已穷尽实现并通过一对一完整性测试。
- [ ] `check-bpm-boundaries.sh` 在本地和 CI 中执行且失败关闭。
- [ ] 所有共享注册点可编译，且不包含业务 Noop。
- [ ] 本阶段未修改 `docs/dev-plan/**`。
- [ ] P0-A、P0-B、P0-C、P0-D 的完成时点、输入和编译证明均已登记。
- [ ] P0-D 完成后不存在 `entities::approval` public 入口或旧审批 ID，生产目标调用方直接引用 `bpm`。
- [ ] P1—P5 不再需要直接修改冻结文件。
- [ ] 后续新增冻结修改必须通过单主题 P0 amendment。
