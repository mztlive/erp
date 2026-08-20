# 阶段 11：P6 试点、P0-D 硬清理与最终验收

> 阶段性质：`P6-PILOT` 纵向试点门禁 + `P0-D` 硬切换清理 + `P6-FINAL` 全量集成与发布门禁
>
> 阶段目标：在不修改其他阶段所有权文件的前提下，证明跨层合同、开发环境硬切换和真实用户流程可发布
>
> 完成限制：任一门禁未通过时，审批合同不得标记“已实施”，不得进入生产切换

## 1. 启动前置条件

`P6-PILOT` 开始前必须确认：

- P0-A、P0-B、P0-C 及试点所需 P0 amendment 已合并，所有试点实现分支已 rebase 最新 main；
- `crates/bpm` 已注册到 workspace，`entities/database/services -> bpm` 单向依赖和边界检查脚本已在 CI 生效；
- 合同 §4.3 的 20 行政策矩阵已生效，唯一试点已指定为 `StockAdjustment`；
- 试点类型的强类型动作、版本、快照、状态、副作用和撤回合同已经生效；
- 审批资格、DataScope、对象读取权、岗位分离和权限双门禁已签署；
- WorkItem 映射、BLOCKED 恢复、通知和开发环境硬切换合同已经生效；
- DOC-A、DOC-B、DOC-C、DOC-D 四段全部合并：合同与 `docs/dev-plan`、`erp-data-model.md`、W 页面与术语、OpenAPI/错误目录/runbook/`openapi:lint` 均已就绪；任一段缺失时不得开始 `P6-PILOT`；
- P1、P2、通用 P3、试点 P3、通用 P4、试点 P4、文档合同和开发环境重置合同已交付各自单元测试；
- 全仓能够完成默认编译，不存在“交给最终阶段接线”的缺失符号。

`P6-PILOT` 通过后，`_meta.json.perDocumentTypeStages` 登记的 19 组 P3/P4 阶段才允许按固定依赖顺序接入。未接入的 `PROCESS_REQUIRED` 类型必须返回 `APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER`，不得调用旧运行时。`P0-D` 开始前必须确认依赖组 `ALL_DOCUMENT_TYPE_ROLLOUTS` 全部完成。`P6-FINAL` 开始前必须额外确认 P0-D 已合并、旧路径零命中、专用空数据库演练通过且全仓默认门禁通过。

任一缺失时停止对应 P6 门禁，并把问题退回对应 owner 或新建单主题 P0 amendment。P6 不得直接修改冻结文件、领域实现、Repository、Service、Handler 或前端业务组件。

## 2. P6 文件所有权

`P6-PILOT` 和 `P6-FINAL` 共同独占：

```text
backend/database/tests/approval_workflow_repository.rs
backend/apps/web-api/tests/approval_workflow_api.rs
backend/apps/web-api/tests/approval_workflow_invariants.rs
backend/apps/web-api/tests/approval_workflow_concurrency.rs
backend/apps/web-api/tests/approval_workflow_cutover.rs
```

上述 Web API 文件必须作为 Cargo 自动发现的顶层 integration-test target，不得放入未注册子目录或依赖共享 `mod.rs`。实际命名必须服从 `docs/dev-plan/conventions.md`；若需要其他共享测试入口，只能由 P0 amendment 登记和修改，P6 不得修改共享测试注册文件。

P6-PILOT 启动前必须删除或将旧入口的有效用例完整改写到上述新文件：

```text
backend/database/tests/approval_runtime_repository.rs
backend/apps/web-api/tests/approval_runtime_api.rs
```

两组文件不得并存。旧文件不得继续编译、保留旧语义 fixture 或作为回归入口。

所有真实 MongoDB 测试必须：

- 使用 `#[ignore]` 和 `require_mongo!()`；
- 只从 `ERP_TEST_MONGO_URI` 读取连接；
- 使用独立随机数据库名；
- 测试结束后精确 drop 自己创建的数据库；
- 不依赖开发者现有数据；
- 不在日志打印完整 URI、凭证或敏感单据字段。

## 3. 跨阶段完整性审查

P6 必须先执行文件与符号审查，证明每项修改由正确 owner 完成：

| 修改面 | 完成 owner | P6 证明 |
| --- | --- | --- |
| BPM workspace、公共 ID、模块、错误和依赖边界 | P0 | metadata、边界脚本、编译和冻结文件变更记录 |
| BPM 流程模型、图规则和 ERP 绑定/业务对象快照/WorkItem/outbox 集成实体 | P1 | BPM/实体单元测试、序列化测试和第二定义源扫描 |
| Repository、索引、CAS、分页、outbox lease | P2 | 真实 MongoDB 集成测试 |
| 定义、业务 Adapter、BPM 纯引擎、ERP 运行编排、HTTP | P3 | 纯引擎确定性、HTTP、事务、权限和并发测试 |
| 配置、单据、工作台、升级、撤回页面 | P4 | feature 测试和真实浏览器验收 |
| 开发环境重置、索引重建和硬切换 | P5 | reset preview/execute/verify 报告和切换证明 |
| 合同、phase 文档、dev-plan | DOC-A | 逐项对照与旧语义扫描 |
| `erp-data-model.md` | DOC-B | 与 P2 实际集合、字段和索引逐项对照 |
| W 页面合同与术语表 | DOC-C | 与 P4 实际页面和文案逐项对照 |
| OpenAPI、错误目录、runbook | DOC-D | `npm run openapi:lint` 与错误码逐项对照 |
| 旧模型、旧运行时、旧责任动作与旧权限删除 | P0-D | `deletes` 对照、生产扫描零命中和全仓门禁 |

发现 owner 遗漏时必须退回原阶段修复，P6 只补集成测试和验收证据。

P6 必须额外证明：`bpm` 只依赖 P0 allowlist；`entities` 和 `services` 中不存在 BPM 流程模型、状态机或审批 ID 第二定义源；`database` 只做 BPM/集成实体持久化；HTTP 和前端不直接使用 BPM 内部计划或 `ProcessKind`。

P6-PILOT 启动前，任何目标调用方通过 `entities::approval` 间接取得 BPM 类型均为阻断；为未切换调用方保留的旧文件必须不可达。P6-FINAL 启动前，`entities::approval` 旧入口必须已由 P0-D 删除。

## 4. P0-D WorkItem 与旧字段全仓清零

本节是 P0-D 的合并门禁，不要求 P6-PILOT 提前删除仍用于未切换代码编译的文件。P0-D 必须搜索旧步骤关联字段的 Rust/TypeScript 命名，逐项分类：

- 生产新模型、DTO、前端、索引和 WorkItem 查询：命中必须为 0；
- 开发重置脚本：只允许以待删除字段名或旧集合清理条件出现；
- 其它生产代码、测试和前端：命中必须为 0；合同与实施计划只允许在删除、禁止或扫描条款中引用旧字段。

必须证明：

1. 新审批 WorkItem 类型固定 `DocumentApproval`；
2. owner user、role、organization、assignment source 和 route family 均符合 P0 映射；
3. 同一 `approval_node_execution_id` 在全部状态合计最多一条 WorkItem；
4. 每个 BPM 实例恰有一个不可变 ERP 业务对象快照；ACTIVE 当前执行恰有一个 OPEN WorkItem；BLOCKED 执行没有 OPEN WorkItem；
5. 改派结束旧执行和旧任务，并为新执行创建新任务；
6. 仍然保留的通用 `complete`、`close` 和 `transfer` 均不能修改审批任务；`claim`、`start_processing`、`release_to_team` 在全仓命中为 0；
7. 旧审批 WorkItem 已由开发环境重置删除，不会进入统一工作台。

## 5. P0-D 旧路径清零

P0-D 必须从生产代码移除：

- `approval/bootstrap.rs` 和启动 definition bootstrap；
- `entities::approval` 临时 BPM re-export facade 和所有生产目标间接引用；
- 编译期流程结构及 `CARD_SALES_APPROVAL`；
- 审批 resolver、handler key、BPM external ID；
- 预建 WAITING 步骤和 `sequence_no + 1` 推进；
- 审批 POOL、开始处理和退回团队；
- `RETRY_CURRENT_STEP` 及 `/recover`；
- `TERMINATE_APPROVAL`、`REJECT_TO_APPLICANT` 和实例驳回/终止终态；
- 卡券专用审批 HTTP、前端组件和权限；
- 客户端提交 definition、node key、next node、reject target、next assignee 或业务 action。

旧审批集合和旧审批类型不得存在生产读取路径。对 `POOL`、`WAITING`、`REJECTED` 的命中必须人工分类；审批与非审批任务的 `POOL` 命中必须为 0（开发重置脚本的删除条件除外）。

### 5.1 P0-D 完成条件

P0-D 只有同时满足下列条件才允许合并：

1. `_meta.json` 的 `ALL_DOCUMENT_TYPE_ROLLOUTS` 依赖组全部完成，20 个固定 `DocumentType` 均已有独立 P3/P4 验收记录；
2. P0-D `deletes` 清单中的旧模型、旧运行时、旧 resolver、旧审批 HTTP、旧责任动作、旧权限和废止前端目录全部删除；
3. 未切换类型失败码 `APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER` 及其临时分支全部删除，所有 `PROCESS_REQUIRED` 类型只进入新运行时；
4. 阶段 10 第 9 节生产扫描无输出，开发重置脚本命中逐项核准；
5. 后端与前端全仓合并门禁全部通过，不存在由 P6 补接共享入口或修复编译的待办；
6. P0-D 合并提交与删除报告已归档，`P6-FINAL` 只读取结果并执行集成验收。

## 6. 后端质量门禁

在 `backend` 执行并留档：

```bash
cargo fmt --all -- --check
cargo test -p bpm --lib
./scripts/check-bpm-boundaries.sh
cargo check --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo test --workspace
cargo test --workspace -- --include-ignored
```

最后一条命令必须在 MongoDB replica set 环境中执行，并提供 `ERP_TEST_MONGO_URI`。不得把未提供数据库导致的 ignored 状态表述为集成测试通过。

所有新增或修改的非生成 Rust 方法/函数必须具有仓库要求的多行 `///`；生产代码不得使用未经允许的 `unwrap`、`expect` 或 `panic!`。P6 必须检查事务中的外部 I/O、无界查询、N+1 和敏感日志。

`check-bpm-boundaries.sh` 必须失败关闭地证明 `bpm` 不依赖或引用 `entities`、`database`、`services`、`web-api`、`mongodb`、`axum`、`id-generator`、`DocumentType`、WorkItem、权限、`Executor` 或业务动作，且源码中不出现 `Local::now`、`Utc::now`、`SystemTime::now`、`Instant::now` 或 ID 生成调用。不得以测试未覆盖、feature 未启用或间接依赖为理由豁免。

还必须证明 `id_type!` 只有一个定义源：`entity-macros` 中恰有一个 `#[proc_macro] pub fn id_type`，`macro_rules! id_type` 在全仓命中为 0，`bpm` 与 `entities` 共用该函数式过程宏，不存在第二份宏或手写重复 newtype 样板。

## 7. 前端与协议门禁

在 `erp-client` 执行并留档：

```bash
npm run format:check
npm run lint
npm run test
npm run test:node
npm exec -- tsc --noEmit
npm run build
npm run openapi:lint
```

`openapi:lint` 必须由 DOC-D 作为仓库脚本和 `package-lock.json` 锁定依赖提供，不得临时联网安装或生成第二种 lockfile。

审查必须证明：

- 业务页面使用 Client Component，不通过 SSR/RSC 取业务数据；
- 所有请求通过 TanStack Query，API 边界使用现有 ResultAsync 合同；
- 所有表单使用 `useAppForm + Zod`，提交使用 mutation；
- 新增函数、hook 和组件有 JSDoc；
- 用户文案通过术语表，不暴露内部枚举或 ID；
- 权限生成文件由后端生成且无手工漂移。

静态门禁通过不等于浏览器验收。

## 8. Repository、HTTP 与事务测试矩阵

### 8.1 定义和索引

必须覆盖：

1. 每个 `ProcessKind` 最多一个活动草稿和一个 PUBLISHED；
2. 节点和连线唯一性、1/2/20 节点图；
3. 客户端不能提交 node key、连线、角色、handler 或 action；
4. 新节点 key 服务端生成，已有节点 key 不可更换或跨定义引用；
5. 发布静态校验和运行期具体单据 DataScope 校验时点准确；
6. 新索引定义与开发重置脚本的创建清单完全一致；
7. 每个 `DocumentType` 通过穷尽映射得到唯一稳定 `ProcessKind`，Repository 只按 `ProcessKind` 查询定义；
8. BPM 图生成和验证对相同输入产生确定结果，Service 中不存在第二套实现；
9. `SalesOrder` 发布定义不强制 `node_purpose=SALES_ORDER_PROCUREMENT_CONFIRMATION`，其它类型不得包含该 purpose；客户端不能写入 purpose，BPM 运行时不得按 purpose 分支。

### 8.2 运行与并发

必须覆盖：

1. BPM `start/decide/cancel/reassign/enter_node` 在无数据库、无业务 Service 的单元测试中可独立运行并产生确定 `TransitionPlan`；
2. 创建只绑定，提交才启动；
3. 1 节点和 20 节点完整通过；
4. 中间节点及第一节点驳回均开启下一轮入口新执行；
5. 下一节点人员失效时保留前一通过事实并形成 BLOCKED；
6. 当前审批人在决定时失效会提交 BLOCKED、关闭任务并返回 409；
7. 原审批人恢复后只能调用 `resume_current_approver`：旧 BLOCKED 执行转 `SUPERSEDED/ASSIGNEE_RECOVERED`，旧 CLOSED 任务保持关闭，同轮同节点创建新 ACTIVE 执行和新 OPEN 任务；
8. 人员 blocker 改派只能在原审批人仍失效时执行：旧执行转 `SUPERSEDED/ADMIN_REASSIGNED`，并创建新执行、新任务；
9. ACTIVE、结构、任务、版本和内部 blocker 不能通过改派恢复；非人员一致性 blocker 只能执行 `cancel_blocked_approval`，不得跳过节点或切换定义；
10. 业务撤回一律要求非空原因，只接受具备 `approval_instance:cancel`、对象读取权和 DataScope 的原提交人，或额外具备类型运行管理权并记录应急代办身份的运行管理员；实例只接受 `RUNNING` 或人员失效类别的 `BLOCKED`。非人员一致性 blocker 只接受受阻取消；两条路径都执行同一政策强类型动作、取消当前执行并清空实例当前执行引用；
11. 最终领域动作故障导致决定、任务、实例、业务状态、审计、收据和 outbox 整体回滚；
12. 相同幂等键同 canonical payload 回读不可变命令结果引用与当前授权后的最新视图，异 payload 冲突；
13. duplicate-key 并发在事务外回读已提交收据；
14. 两个并发决定只有一个成功；
15. WorkItem、实例和执行的陈旧 CAS 全部返回稳定冲突；
16. 无对象读取权的审批人不能从任务责任获得敏感内容；
17. 历史可按 cursor、round 和 execution 完整重放，无 N+1 和无界读取；
18. 每个实例的强类型业务对象快照与 BPM `SubjectRef + subject_version` 一致且写后不可变；
19. 任一 BPM 计划应用、业务动作、业务对象快照、WorkItem、审计或 outbox 写入失败均使同一事务整体回滚；
20. 同一 `SubjectRef + subject_version` 即使提交不同定义 ID，也只能存在一条 `RUNNING|BLOCKED` 活动链。

### 8.3 权限和错误

定义详情、定义管理、绑定升级和运行管理动作必须覆盖动作级权限与对应类型级权限的四种组合，只有两者同时满足才成功；固定非敏感类型目录只要求 `approval_process:read`。普通决定必须覆盖动作权限、当前任务责任、审批资格、对象读取权、DataScope 和岗位分离；普通撤回必须覆盖动作权限、原提交人身份及运行管理员应急路径。HTTP 必须覆盖 2xx、403、404、409、422、500、幂等回读和权限失败不泄露资源存在性。

`APPROVAL_POLICY_NOT_REGISTERED` 必须导致 readiness 失败并映射 500，不能由客户端修复。

### 8.4 outbox 和观测

必须覆盖两个 worker 竞争领取、租约到期接管、投递幂等、退避、首次投递加最多 5 次重试、连续第 6 次失败进入 dead letter，以及进程退出。第 1—5 次失败后的退避必须依次为 1 分钟、5 分钟、15 分钟、1 小时、6 小时。必须验证下列指标存在且标签低基数：BLOCKED 数量/年龄、决定/CAS/幂等冲突、决定延迟、ACTIVE/OPEN WorkItem 不一致、outbox backlog/oldest/retry/dead letter。硬切换 reset 失败只记录在执行报告和运维日志，不作为长期业务时序指标。

## 9. 逐 DocumentType 验收

先在 `P6-PILOT` 对试点 `StockAdjustment` 完成完整 P3/P4、事务、浏览器和专用空数据库硬切换演练。试点未通过时，`_meta.json.perDocumentTypeStages` 中其余 19 个类型的阶段均不得开始；其中 11 个 `PROCESS_REQUIRED` 类型不得接入运行时或在共享开发环境发布定义，8 个 `NO_APPROVAL` 类型不得跳过其无绑定、无实例、无任务验收。

其余类型接入完成后，`P6-FINAL` 对全部 20 个固定 DocumentType 逐行验证：

- `NO_APPROVAL`（8 个）：创建成功且无绑定、实例或任务，且未注册空适配器；
- `PROCESS_REQUIRED`：无定义创建失败；有定义创建只绑定；提交后启动；最终强类型动作、状态和副作用符合生命周期矩阵；
- 发布新版本后，旧单据保持旧绑定，新单据绑定新版本；
- 已绑定旧定义的未提交单据可以启动，受控升级只能到当前发布版本；
- 创建、绑定、启动、进入节点和决定各自执行合同规定的资格校验；
- 页面使用通用组件，升级、撤回、决定和改派只按 `allowed_actions` 显示。

每个类型必须形成独立验收记录，不得用一条通用 happy-path 代表全部类型。

## 10. 真实浏览器验收

至少使用制单人、审批人、流程管理员三个真实授权身份完成：

1. 管理员创建、编辑、发布和退役定义；
2. 制单人创建后看到冻结路线，提交后看到当前责任；
3. 审批人只看到本人 ACTIVE 开放任务并执行通过/驳回；
4. 驳回后显示同一单据下一轮和完整历史；
5. 原提交人在允许时撤回；原提交人无法处理时，具备类型运行管理权的管理员可填写原因应急撤回；管理员在允许时升级未提交绑定；
6. 人员失效 blocker 按服务端事实显示“恢复当前审批人”或“改派当前审批人”；非人员一致性 blocker 只对运行管理员显示“取消受阻审批”；
7. 无权限身份不能查看或调用管理动作；
8. 统一工作台无团队分区，列表 + 详情主从可连续提交审批决定，`/workspace/tasks` 重定向到 `/workspace`；
9. 背景刷新保留现有内容，不出现整页骨架闪烁；
10. 内部 ID、POOL、DIRECT、execution、retry 等词不出现在界面。

## 11. P6-FINAL 共享开发环境硬切换门禁

必须在受控开发环境执行：

```text
stop writers -> reset preview -> explicit reset approval -> reset execute
-> verify empty old/new runtime data -> deploy new code -> create indexes
-> pass pre-enable gates -> publish pilot definition -> pilot smoke
-> publish remaining 11 definitions one by one -> per-type smoke
-> on failure: retire only that type's definition
```

演练必须证明：

- reset preview 与实际删除集合完全一致，并经过显式目标确认；
- 旧审批定义、实例、步骤、任务关联和旧索引全部清零；
- 新索引在发布任何定义前全部创建并验证；
- 发布任何定义前不存在业务单据、审批实例或审批 WorkItem；
- 部署后但未发布定义时，12 个 `PROCESS_REQUIRED` 类型的创建全部失败关闭并返回 `APPROVAL_PROCESS_NOT_CONFIGURED`；
- 账号、RBAC、主数据、审计和对象存储仍按重置脚本保护合同保留；
- 发布定义后只产生新模型数据，旧集合和旧字段保持零新增；
- 不存在全局审批运行开关或运行模式配置项；
- 回退只允许退役定义、停写、重置审批业务数据并前向部署，不恢复旧运行时。

## 12. 最终证据包

必须提交：

- 政策、生命周期、试点、权限、WorkItem、BLOCKED 和通知的生效合同（合同 §4.3—§4.6、§12.2—§12.5、§13.2、§13.3、§15.2、§16.4、§16.5），包括销售提交直启、`ReviewStatus` 三值、审批导致的业务 `REJECTED` 删除、销售订单采购确认 purpose、单人责任模型和 W01/W02 合并；
- 全部 20 个 `DocumentType` 的政策矩阵实现与穷尽性测试证据；
- 9 个新增 `approval_subject_version` 字段的实现与不可变性证据；
- `docs/dev-plan` 与实际阶段所有权、分支和验收命令一致的核对结果；
- 各 owner 文件变更清单和冻结文件审查；
- BPM 依赖图、边界脚本、纯引擎确定性和第二定义源清零报告；
- 后端、前端、OpenAPI、索引和浏览器门禁结果；
- 每个 DocumentType 的独立验收记录；
- 并发、事务回滚、幂等、outbox 和权限测试证据；
- 开发环境 reset preview/execute/verify 报告及硬切换证明；
- 旧语义和旧字段清零报告；
- dashboard、告警和 runbook 入口；
- 未通过项、责任人和禁止上线结论。

只有全部清单通过且不存在未豁免阻断项，才允许把合同标记为“已实施”并启用新运行时。
