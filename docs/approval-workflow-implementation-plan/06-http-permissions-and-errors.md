# 阶段 06：HTTP、权限与错误合同

> 阶段性质：P3 HTTP 工作包
>
> 阶段目标：以最小请求面暴露定义管理、决定、查询、人员恢复、改派和受阻取消能力，并建立稳定权限、分页与错误合同
>
> 允许状态：依赖 P0-A 冻结的端口和错误；Handler 文件稳定后由 P0-B 完成 `AppState`、路由和权限生成接线

## 1. 文件责任

本阶段负责：

- `backend/apps/web-api/src/core/handler/approval_process/**`；
- `backend/apps/web-api/src/core/routes/approval_process.rs`；
- `backend/apps/web-api/src/core/handler/approval_instance/**`；
- `backend/apps/web-api/src/core/routes/approval_instance.rs`；
- `backend/apps/web-api/src/core/handler/work_item/**` 与 `backend/apps/web-api/src/core/routes/work_item.rs` 中的审批任务保护响应；
- `backend/services/src/iam/predefined_roles.rs` 的权限种子精确升级；
- HTTP DTO、权限元数据和错误转换的模块单元测试。

各业务 Handler 的提交、撤回和升级绑定请求**不属于本阶段**：它们随对应 `DocumentType` 子阶段一起改，`handler/<domain>/**` 与 `routes/<domain>.rs` 已登记在该子阶段的 `owns` 内（试点为 `P3-ADAPTER-PILOT` 的 `handler/inventory/**`、`routes/inventory.rs`）。本阶段只交付通用审批端点、WorkItem 保护和权限种子。

`build.rs`、共享 `handler/mod.rs`、`routes/mod.rs`、`routes/admin.rs`、`app_state.rs`、公共错误/响应文件和前端生成权限文件均为冻结入口，只能由阶段 00 或独立 P0 amendment 修改。HTTP 集成测试只由阶段 11 在 P6-PILOT/P6-FINAL 编写。

HTTP 层只允许依赖 `services` 的请求/响应 DTO 和应用端口，不得直接依赖 `bpm` 或 `database`。BPM ID 和状态可以经 Service DTO 序列化为线协议字段，但 Handler 不得构造 `TransitionPlan`、调用 BPM 引擎、解释 `ProcessKind` 或把 BPM 错误直接暴露给客户端。

## 2. 定义管理 API

固定提供：

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| `GET` | `/approval-processes/catalog` | 固定单据类型、政策、当前版本和配置状态 |
| `GET` | `/approval-processes/{document_type}/versions` | 历史版本列表 |
| `GET` | `/approval-process-definitions/{id}` | 定义图详情 |
| `POST` | `/approval-process-definitions/drafts` | 创建更高版本草稿 |
| `PUT` | `/approval-process-definitions/{id}/nodes` | 整组替换草稿节点 |
| `POST` | `/approval-process-definitions/{id}/publish` | 发布并退役旧版本 |
| `POST` | `/approval-process-definitions/{id}/retire` | 退役当前发布版本 |
| `GET` | `/approval-processes/{document_type}/eligible-assignees` | 服务端过滤可选审批人 |

创建草稿请求必须包含 `document_type`、名称、`draft_source=EMPTY|CURRENT_PUBLISHED` 和幂等键。不得提交源定义 ID；选择 `CURRENT_PUBLISHED` 但当前无发布定义时返回稳定冲突。

节点写请求必须 `deny_unknown_fields`。新增节点不接受 `node_key`；编辑已有节点只接受本定义内 `node_id`。请求不得接受连线、入口、角色、候选池、脚本、处理器、业务动作或状态。

所有 ID、版本和枚举必须在 Handler 边界通过 Service DTO 完成强类型转换。Handler 只负责协议适配、认证上下文和响应转换，不得查询数据库、调用 BPM 或拼装状态机。

## 3. 运行与查询 API

| 方法 | 路径 | 用途 |
| --- | --- | --- |
| `POST` | `/approval-decisions` | 对当前开放审批任务通过或驳回 |
| `GET` | `/approval-instances` | 按固定 view、DataScope 和游标查询实例摘要 |
| `GET` | `/approval-instances/{id}` | 实例、当前责任和最近受控条数历史 |
| `GET` | `/approval-instances/{id}/history` | 按轮次和执行序号读取完整历史 |
| `GET` | `/approval-instances/{id}/recovery-options` | 返回当前 blocker 的唯一合法恢复方式 |
| `GET` | `/approval-instances/{id}/eligible-reassignees` | 按当前单据、节点和岗位分离条件搜索改派候选人 |
| `POST` | `/approval-instances/{id}/resume-current-approver` | 原审批人重新合格后创建新执行和新任务 |
| `POST` | `/approval-instances/{id}/reassign-current-approver` | 仅处理人员失效 blocker |
| `POST` | `/approval-instances/{id}/cancel-blocked` | 取消非人员一致性 blocker，不允许继续推进 |
| `POST` | `/business-documents/{document_type}/{id}/approval-definition/upgrade` | 升级未提交单据绑定 |

单据提交和撤回继续使用各业务资源路由，内部调用统一审批端口。不得增加绕过业务状态校验的通用 `/start` 或 `/cancel` 路由。

升级请求只允许原因、`expected_document_version`、`expected_approval_binding_version` 和幂等键。目标定义固定取当前发布版本，不得由客户端提交 definition ID。

必须删除 `POST /approval-instances/{id}/recover`，不得保留 `RETRY_CURRENT_STEP` 别名。

### 3.1 列表查询合同

`GET /approval-instances` 只允许：

```text
view=mine|started|managed|blocked
document_type?
status?
cursor?
limit?（默认 20，最大 100）
```

语义固定为：

- `mine`：当前用户拥有的 OPEN `DocumentApproval` WorkItem，且关联执行必须为当前 ACTIVE 执行；
- `started`：`started_by` 为当前用户，且用户仍有业务单据读取权；
- `managed`：当前用户同时具备动作级读取权限、类型级管理权限和实例 DataScope；
- `blocked`：`managed` 子集且实例状态为 BLOCKED。

`status` 只允许阶段 01 冻结的实例状态。`mine` 只接受省略 `status` 或 `status=RUNNING`，`blocked` 只接受省略 `status` 或 `status=BLOCKED`；其他组合必须返回 422，不得返回伪空列表。`started` 和 `managed` 允许按任一实例状态过滤。

排序和 cursor 固定按 view 执行：

- `mine`：`assigned_at desc, work_item_id desc`；
- `started`：`started_at desc, instance_id desc`；
- `managed`：`updated_at desc, instance_id desc`；
- `blocked`：`blocked_at desc, instance_id desc`。

cursor 必须编码当前 view 的两个排序字段及 view 名称，跨 view 使用必须拒绝。不得接受任意排序字段、任意 owner ID 或跳过 DataScope 的 `all=true`。

### 3.2 详情与历史合同

详情最多返回最近 20 条执行摘要并携带 `history_next_cursor`。历史端点默认 50、最大 100，按 `round_no asc, execution_no asc, id asc` 稳定排序。

历史只返回新运行模型。开发环境硬切换会清空旧审批集合，HTTP 不得读取旧实例、旧步骤或 `approval_step_instance_id`。

## 4. 决定请求白名单

请求体只允许：

```json
{
  "work_item_id": "...",
  "decision": "APPROVE",
  "reason": null,
  "expected_task_version": "3",
  "idempotency_key": "..."
}
```

必须拒绝额外的 instance、execution、definition、subject、next node、reject target、next assignee、业务 action 和 actor 字段。actor 只从认证上下文注入。

版本必须沿用仓库现行字符串安全整数合同，不得向浏览器暴露 JS 不安全整数。

## 5. 人员恢复、改派与受阻取消请求

恢复当前审批人请求只允许：`expected_instance_version`、`expected_execution_version`、`expected_assignment_version`、可空的 `expected_closed_task_version` 和幂等键。不得接受目标用户、决定、节点或恢复动作枚举。

改派请求只允许：目标用户、非空原因、可空的 `expected_closed_task_version`、`expected_instance_version`、`expected_execution_version`、`expected_assignment_version` 和幂等键。实例 ID 来自路径，当前节点由服务端推导。

不得接受 node key、定义 ID、原审批人或恢复动作枚举。改派接口只接受人员失效 blocker；ACTIVE 实例、原审批人已恢复有效和结构性 blocker 必须返回稳定冲突，不能把该接口实现成普通管理员转签。

受阻取消请求只允许：非空原因、`expected_instance_version`、`expected_execution_version`、可空的 `expected_task_version` 和幂等键。它只接受非人员一致性 blocker，并执行政策绑定的同一 `cancel_action`；不得接受新定义、修复值、下一节点或目标用户。

`eligible-reassignees` 必须接收 `search`、稳定 cursor 和 `limit`（默认 20、最大 50），并以当前实例对应的具体业务对象重验账号、任职、审批资格、对象读取权、DataScope 和岗位分离。定义管理使用的 `eligible-assignees` 只能执行定义期静态过滤，两者不得复用成不区分资源上下文的候选列表。

## 6. 权限合同

动作级权限固定为：

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

执行规则：

1. 每个 Handler 通过权限宏校验动作级权限；
2. Service 再根据合同 §4.6 的穷尽映射校验资源级政策门禁；不得把管理员类型级权限施加给普通决定、普通撤回或普通读取；
3. `approval_process:read` 可读取固定 20 行非敏感目录；读取某个 `PROCESS_REQUIRED` 类型的定义版本、节点与审批人详情时，还必须具备该类型定义管理权或运行管理权。动作级 `approval_process:*` 不得自动授予所有单据类型管理权；
4. `approval_instance:decide` 只允许进入 Handler，Service 仍须校验任务本人责任、对象读取权、DataScope 和岗位分离；
5. `approval_instance:cancel` 必须要求非空原因并再校验 actor 为原提交人；若不是原提交人，则必须具备该 `DocumentType` 的运行管理权和实例 DataScope，并记录应急代办身份；
6. `approval_instance:resume|reassign|cancel_blocked` 都必须再校验该 `DocumentType` 的运行管理权和实例 DataScope，不隐含定义编辑权；
7. `recovery-options` 和 `eligible-reassignees` 只对通过同一运行管理权与 DataScope 的 actor 返回结果，不得泄露实例或候选人存在性；
8. 查看审批实例不隐含查看全部业务字段；
9. 类型级权限名称和默认角色授权来自签署政策矩阵，不得在 Handler 中按字符串动态拼接；
10. `predefined_roles.rs` 必须用精确旧快照升级，保留自定义角色授权，不得覆盖管理员后配权限；
11. 权限生成文件只能由 P0 登记的生成命令更新，禁止手工编辑。

删除 `approval_instance:recover`、`approval_instance:diagnose` 和卡券专用决定权限前，必须完成调用方切换和预置角色回归测试。`approval_instance:diagnose` 现被 `services/src/approval/scope.rs` 用于受阻列表范围计算，切换后由 `approval_instance:read` 加类型级 `approval_runtime_admin` 取代，不得保留为兼容权限。

## 7. 稳定错误码

| 错误码 | HTTP | 使用条件 |
| --- | --- | --- |
| `APPROVAL_POLICY_NOT_REGISTERED` | 500 | 固定类型缺少政策；同时使 readiness 失败 |
| `APPROVAL_PROCESS_NOT_CONFIGURED` | 409 | 必须审批但无可绑定发布定义 |
| `APPROVAL_DRAFT_SOURCE_NOT_AVAILABLE` | 409 | 请求从当前发布版本创建草稿，但当前无发布定义 |
| `APPROVAL_DEFINITION_NOT_DRAFT` | 409 | 修改非草稿定义 |
| `APPROVAL_DEFINITION_VERSION_CONFLICT` | 409 | 定义锁版本过期 |
| `APPROVAL_DEFINITION_INVALID` | 422 | 图、节点、人员或动作校验失败 |
| `APPROVAL_DEFINITION_BINDING_CORRUPTED` | 409 | 单据绑定缺失或不一致 |
| `APPROVAL_ALREADY_STARTED` | 409 | 同一提交版本已有非终态实例 |
| `APPROVAL_TASK_NOT_OPEN` | 409 | 任务已完成或关闭 |
| `APPROVAL_TASK_NOT_ASSIGNED_TO_ACTOR` | 403 | 当前用户不是三方一致责任人 |
| `APPROVAL_TASK_VERSION_CONFLICT` | 409 | 任务版本过期 |
| `APPROVAL_INSTANCE_VERSION_CONFLICT` | 409 | 实例并发变化 |
| `APPROVAL_EXECUTION_VERSION_CONFLICT` | 409 | 节点执行并发变化 |
| `APPROVAL_SUBJECT_VERSION_CONFLICT` | 409 | 单据提交版本不一致 |
| `APPROVAL_REJECT_REASON_REQUIRED` | 422 | 驳回原因为空 |
| `APPROVAL_INSTANCE_BLOCKED` | 409 | 当前实例已受阻，不能决定 |
| `APPROVAL_RESUME_NOT_ALLOWED_FOR_BLOCKER` | 409 | 实例非 BLOCKED 或当前 blocker 不属于人员失效 |
| `APPROVAL_CURRENT_APPROVER_NOT_RECOVERED` | 409 | 恢复命令执行时原审批人仍不合格 |
| `APPROVAL_CURRENT_APPROVER_RECOVERED` | 409 | 改派时原审批人已经恢复，只允许调用恢复端口 |
| `APPROVAL_REASSIGN_TARGET_INELIGIBLE` | 422 | 改派目标不满足资格 |
| `APPROVAL_REASSIGN_NOT_ALLOWED_FOR_BLOCKER` | 409 | 实例非 BLOCKED 或当前 blocker 不属于人员失效 |
| `APPROVAL_BLOCKED_CANCEL_NOT_ALLOWED` | 409 | 实例非 BLOCKED 或当前 blocker 属于人员失效；政策缺少必需动作属于 `APPROVAL_POLICY_NOT_REGISTERED`/readiness 500 |
| `APPROVAL_GENERIC_WORK_ITEM_MUTATION_FORBIDDEN` | 409 | 通用 WorkItem 命令试图修改审批任务 |
| `APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT` | 409 | 同幂等键不同 canonical payload |

内部数据损坏应进入 BLOCKED 并记录结构化 blocker；无法形成合法阻塞事实时返回 500、告警并冻结该实例写入。响应不得泄露账号隐私、授权策略细节、数据库结构或资源存在性。

## 8. 响应与冲突回读

成功响应统一返回最新实例摘要。幂等重复成功必须先执行当前动作权限及合同 §4.6 对应的责任、资格、适用类型权限、DataScope 和对象读取权重验；失权时返回不泄露资源存在性的 403/404，不得返回收据引用。仍有权时返回收据中的不可变命令结果引用以及调用者当前有权读取的最新实例摘要，并使用正常 2xx；不得把当前可变视图描述为原请求时刻快照，也不得返回“重复请求”冲突。

409 响应必须包含稳定错误码、correlation ID 和调用者有权查看的最新任务/实例版本。403/404 不得附带可推断资源存在性或内部版本的详情。

决定时检测到人员失效的命令必须先提交 BLOCKED 事实，再由 Handler 把 `DecisionOutcome::Blocked` 映射为 409；不得通过错误传播回滚阻塞状态。运行管理员后续只能按 `recovery-options` 返回的动作恢复或改派；原提交人只能在业务资源 `allowed_actions` 允许时撤回，具备类型运行管理权的管理员可填写原因应急撤回。非人员一致性 blocker 的 `recovery-options` 只能返回受阻取消。

## 9. 通用 WorkItem 路由

本阶段删除 `/work-items/{id}/start-processing`、`release-to-team`、`claim` 及对应 Handler、路由和权限；P3-RUNTIME 已使 Service 旧命令稳定失败关闭，P4-WORKFLOW 删除前端调用，P0-D 删除剩余 Service 符号。通用 `reassign`、`close` 仅供非审批任务使用；Handler 和 Service 均必须拒绝 `WorkItemType::DocumentApproval` 或带 `approval_node_execution_id` 的任务。

列表 DTO 必须输出 `responsibility_kind=PERSONAL_APPROVAL|PERSONAL_BUSINESS_TASK`、`work_item_type`、目标路由摘要和 `allowed_actions`。`PERSONAL_APPROVAL` 必须对应 `DocumentApproval`；两种责任类型的 `owner_user_id` 均必填。前端不得根据历史责任模式或卡券类型推断路由。

## 10. OpenAPI 与测试责任

本阶段合并后必须立即启动 **DOC-D**：把本文件全部路径、白名单、分页、权限和错误写入 `docs/approval-workflow-openapi.yaml` 与 `docs/approval-workflow-error-catalog.md`，并交付 `docs/runbooks/approval-workflow.md` 和 `openapi:lint` 脚本。DOC-D 是 `P6-PILOT` 的前置。阶段 11 必须运行 OpenAPI lint/schema 校验和 HTTP 集成测试。

本阶段只允许在模块内编写 DTO 反序列化、权限元数据和错误转换单元测试，不得修改 `backend/apps/web-api/tests/**`。

## 11. 阶段验收

- [ ] 草稿、决定、节点、恢复、改派和受阻取消 DTO 使用 `deny_unknown_fields` 并拒绝全部禁用字段。
- [ ] 新节点 key 只能由服务端生成，已有节点不能跨定义引用或换 key。
- [ ] 每个 Handler 有动作级权限宏，Service 对合同 §4.6 每类动作都有对应资源门禁测试；管理动作缺类型权限、普通动作缺责任/资格或任一动作缺 DataScope 时均拒绝。
- [ ] 普通审批人不能管理定义或改派，只能决定本人当前任务。
- [ ] DataScope 在 Service 查询和写事务中生效，不依赖前端隐藏。
- [ ] Handler 不直接依赖 `bpm`/`database`，不解释 `ProcessKind`、BPM 计划或内部错误。
- [ ] 列表 view、稳定 cursor、最大页大小和详情历史上限已经固化。
- [ ] 通用 WorkItem 写接口对审批任务全部失败关闭。
- [ ] `APPROVAL_POLICY_NOT_REGISTERED` 作为 500 和 readiness 错误测试，不得出现在 4xx 目录。
- [ ] 模块单元测试覆盖 DTO 和错误转换；试点 2xx、403、404、409、422、500 及幂等回读由 P6-PILOT 覆盖，全量类型由 P6-FINAL 补齐。
- [ ] `cargo test -p web-api --lib` 通过；权限生成文件无手工修改。
