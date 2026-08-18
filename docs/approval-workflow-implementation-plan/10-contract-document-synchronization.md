# 阶段 10 / DOC：合同与工作面文档同步

> 阶段性质：文档合同工作包，拆为四段，每段是各自下游阶段的前置
>
> 阶段目标：使业务状态、数据、API、权限、运行、页面、开发环境重置和运维文档只描述一套可执行审批语义
>
> 完成限制：可以在代码集成前完成目标合同；“已实施”状态只能由阶段 11 验收后更新

## 0. 四段划分与门禁

| 段 | 交付 | 前置 | 卡住的阶段 | 状态 |
| --- | --- | --- | --- | --- |
| DOC-A | 权威合同、`erp-phase-1.md`、`erp-phase-2.md`、`docs/dev-plan/**`、实施计划 | — | P0-A | 已合并 |
| DOC-B | `erp-data-model.md` | DOC-A | P2 | 内容就绪，待 DOC-A 后独立合并 |
| DOC-C | `erp-ui-flows.md`、`erp-ui-design.md`、`ui-glossary.md`、`docs/ui-workspaces/**` | DOC-A | P4-DEFINITION、P4-WORKFLOW | 内容就绪，待 DOC-A 后独立合并 |
| DOC-D | `approval-workflow-openapi.yaml`、错误目录、`runbooks/approval-workflow.md`、`openapi:lint` 与 `@redocly/cli` 锁定 | P3-HTTP | P6-PILOT | 未开始 |

DOC-A 已合并，P0-A 已解锁。其余三段各自只阻断自己的下游阶段，不阻断其他阶段。DOC-D 依赖 P3-HTTP 的端点稳定，因此排在 P3 之后。

DOC-A 的生效决定固定如下：

| 合同项 | 权威位置 | 生效取值 |
| --- | --- | --- |
| 销售单提交与审批启动 | 合同 §4.4.1 | `SalesOrder` 与 `VoucherSalesOrder` 均由提交命令在同一事务直接调用 `on_approval_start`；不存在提交准入或第二启动路径 |
| `ReviewStatus` 与业务 `REJECTED` | 合同 §4.4.2 | `ReviewStatus` 仅有 `NOT_SUBMITTED`、`IN_APPROVAL`、`APPROVED`；12 个类型均不得用业务 `REJECTED` 表达审批结果或受阻取消 |
| 销售订单采购确认 | 合同 §4.4.3 | `SalesOrder` 已发布定义必须恰有一个 `node_purpose=SALES_ORDER_PROCUREMENT_CONFIRMATION` 的普通单人审批节点；运行时不得按该 purpose 分支 |
| 责任模型 | 合同 §4.4.3、§13.2 | 低毛利上级确认整体删除；全系统删除责任池、领取、开始处理和退回团队；任何开放任务创建即指定到人 |
| 待办与工作台合并 | 合同 §16.4 | W01 扩写为唯一 `/workspace` 页面（列表 + 详情主从）；W02 废止；`/workspace/tasks` 永久重定向 |

## 1. 文件责任

按段划分，每个文件只属于一段：

| 段 | 必须修改 | 必须新增 |
| --- | --- | --- |
| DOC-A | `docs/approval-workflow-contract.md`、`docs/erp-phase-1.md`、`docs/erp-phase-2.md`、`docs/approval-workflow-implementation-plan/**` | `docs/dev-plan/{README.md,conventions.md,domains.md,_meta.json,approval-workflow.md}` |
| DOC-B | `docs/erp-data-model.md` | — |
| DOC-C | `docs/erp-ui-flows.md`、`docs/erp-ui-design.md`、`docs/ui-glossary.md`、`docs/ui-workspaces/**` 中全部受审批合同影响的现有工作面 | `docs/ui-workspaces/w24-approval-processes.md` |
| DOC-D | `erp-client/package.json`、`erp-client/package-lock.json`（仅登记 OpenAPI 校验脚本与锁定工具版本） | `docs/approval-workflow-openapi.yaml`、`docs/approval-workflow-error-catalog.md`、`docs/runbooks/approval-workflow.md` |

`docs/dev-plan/**` 由 DOC-A 独占。阶段 00 及其他任何阶段不得创建或修改其中文件；阶段 00 只负责在 P0-A 开工前确认这五个文件存在且已登记其 `owns`、`ownsWithin`、`creates` 和 `deletes`。

若仓库已有等价权威 OpenAPI、错误目录或 runbook，必须修改现行权威文件并删除重复草案。不得建立两个同时有效的标准。

## 2. 权威关系

文件职责固定如下：

1. `approval-workflow-contract.md` 定义跨单据审批基础设施、责任和运行不变量；
2. `erp-phase-1/2.md` 定义每种业务单据的准入、状态、版本、强类型动作和副作用；
3. `erp-data-model.md` 定义持久化字段、索引、事务和硬切换后的唯一数据事实；
4. `docs/dev-plan` 定义 P0—P6 文件所有权、冻结入口、分支、阶段和门禁；
5. W 文档定义页面责任，不得重新定义后端状态机；
6. OpenAPI 和错误目录定义线协议；
7. runbook 定义监控、告警、阻塞处理和切换操作，不得改变业务语义。

所有权威文件必须统一声明 `bpm` 是无 ERP、无 I/O 的下层流程引擎，`services::approval` 是 ERP 适配和事务编排层。任何文档不得把 BPM 描述为 MongoDB Repository、HTTP 服务、权限执行器、WorkItem 服务或业务动作回调容器。

出现冲突时必须先修订权威合同，再实施代码。实现人员不得选择旧段落或自行设置兼容解释。

## 3. `approval-workflow-contract.md` 修改合同

DOC-A 合并前必须在下列权威位置形成唯一取值，位置索引见 `docs/dev-plan/approval-workflow.md` §1：

- 全部 `DocumentType -> NO_APPROVAL|PROCESS_REQUIRED` 政策矩阵 —— 已签署为 §4.3，20 行；
- `SalesOrder` 是否拆类型的最终决定 —— 已签署为拆分，新增 `VoucherSalesOrder`；
- 每个 `PROCESS_REQUIRED` 类型的定义管理权限、运行管理权限、审批资格和岗位分离；
- 每种类型的创建状态、提交命令、`subject_version` 来源、快照、最终命令、最终状态、副作用和撤回；
- 唯一试点 DocumentType；
- 启动、进入节点、决定、阻塞、恢复、改派、正常取消和受阻取消的通知收件人、模板、去重、重试和死信策略；
- WorkItem 字段、类型、责任、路由、保护命令和全生命周期唯一性；
- BLOCKED 原因分类、原审批人恢复、改派准入、受阻取消和结构性 blocker 处置责任；
- 查询 view、游标、新运行历史上限和权限脱敏响应；
- P0 地基、唯一试点、P6-PILOT、逐类型 rollout、P6-FINAL 和开发环境硬切换顺序。
- `apps/web-api -> services -> bpm/entities/database`、`database/entities -> bpm` 的单向依赖合同；
- `bpm` 禁止依赖 ERP、MongoDB、HTTP、权限、WorkItem、通知投递和业务动作；
- `DocumentType -> ProcessKind`、业务对象到 `SubjectRef` 的唯一穷尽映射；
- BPM `TransitionPlan/BpmEvent` 与 Service 同事务应用、业务副作用和 outbox 映射责任。

发布时校验必须改成可执行时点：发布校验账户、静态权限、图和可静态判断的岗位分离；具体单据 DataScope、对象读取权和提交人隔离在绑定、启动、进入节点和决定时校验。合同 §15.1 第 13 条已签署为**不采用**全组织审批资格规则，因此发布不得声明完成实例级 DataScope 校验。

必须整节删除旧的「数据迁移与应用切换」章节（原 §17.1 定义迁移、§17.2 单据绑定迁移、§17.3 运行实例迁移），替换为「开发环境硬切换」：无数据搬运、无 legacy history、无双写、无旧运行时回退、无迁移阻断清单。不得保留任何「切换时仍活动的旧审批必须受控取消后重新提交」一类的迁移条款。

只有阶段 11 全部门禁通过后，才能把状态改为“已实施”。

## 4. 业务状态合同修改

### 4.1 `erp-phase-1.md` 与 `erp-phase-2.md`

必须删除或重写：

- “不建设可配置审批流”或“审批结构按代码注册”；
- 卡券 `CARD_SALES_APPROVAL` 编译期结构；
- 审批节点 `POOL`；
- 驳回结束实例、返回改单后新建实例；
- 审批终止决定；
- 客户端选择定义、节点、驳回目标或下一审批人；
- 把人工业务确认任务误写为审批节点。

生命周期矩阵的**唯一权威位置是 `approval-workflow-contract.md` §4.4**，不得在 `erp-phase-1.md` / `erp-phase-2.md` 中重复一份可能漂移的副本。两份 phase 文档只允许：

1. 引用合同 §4.3 / §4.4 的行，不得自行给出取值；
2. 描述该单据自身与审批无关的业务规则；
3. 在业务状态机描述中体现 §4.4.2 的收敛结果（唯一「审批中」态，无逐节点审批态）。

采购二次确认必须实现为 `SalesOrder` 已发布定义中带固定 `node_purpose=SALES_ORDER_PROCUREMENT_CONFIRMATION` 的普通单人审批节点。定义必须恰有一个该 purpose，客户端不得写入、删除或复制该 purpose；运行时不得按 purpose 分支。低毛利上级确认整体删除，毛利只保留只读风险提示。phase 文档不得再把二者写成提交准入、独立状态机或团队业务任务；选源事实只允许出现在采购单创建路径（`erp-phase-1.md` §7.4）。

### 4.2 `erp-data-model.md`

必须用目标集合替换旧 definition/step/instance/step_instance 模型，并增加：

- `bpm` 拥有的定义、节点、连线、实例、节点执行、实例审批人和命令收据；
- `entities` 拥有的单据绑定、强类型业务对象快照、WorkItem 和通知 outbox 集成实体；
- 定义的 `ProcessKind`、实例的 `SubjectRef`，以及 ERP `DocumentType`/业务对象到 BPM 边界类型的映射；
- 独立 `approval_binding_version`、实例/执行/分派/任务版本；
- `WorkItemType::DocumentApproval` 和 `approval_node_execution_id`；
- 命令收据 canonical payload hash；
- 通知 outbox 租约、重试和死信；
- 最近驳回有界投影；
- 全生命周期 WorkItem 唯一索引、当前执行唯一索引和分页索引；
- 旧审批集合、旧审批 WorkItem 和旧字段的开发环境删除范围；
- 新索引创建顺序、启用门禁和空数据验证。

必须删除 `assignment_mode` 字段与任何按责任池过滤的索引。任何 `status=OPEN` 的 WorkItem（审批与非审批）都必须有非空 `owner_user_id`。

## 5. 页面与术语合同修改

### 5.1 `erp-ui-flows.md` 与 `erp-ui-design.md`

页面流固定为：创建绑定 → 编辑草稿 → 提交启动 → 单人节点 → 通过下一节点或最终批准 → 驳回开启下一轮入口。人员失效 BLOCKED 若继续运行，只允许恢复当前审批人或改派；原提交人可在服务端允许时撤回，具备类型运行管理权的管理员可填写原因应急撤回。非人员一致性 blocker 只允许运行管理员执行受阻取消。

必须删除审批角色池认领、开始处理、退回团队、任意转签、终止和退回申请人编辑。全系统不存在「团队业务任务」分区；非审批任务与审批任务使用同一「待我处理」口径，创建即指定到人。

### 5.2 W01、W02、W05、W19、W24

- W01：扩写为唯一工作台，布局为列表 + 详情主从（合同 §16.4）；指标带点击即筛选，无团队待处理分区；
- W02：标记为已废止，能力并入 W01；`/workspace/tasks` 永久重定向到 `/workspace`；
- W05：删除卡券专用步骤和终止/驳回终态，引用通用审批组件与轮次历史；采购确认不再有独立业务任务工作面；
- W19：登记动作级权限、类型级权限、运行恢复/改派/受阻取消权限和审计事件；
- W24：定义目录、草稿编辑、发布、退役、历史、冲突、权限和升级入口。

### 5.3 `ui-glossary.md`

必须增加“审批流程、审批节点、当前轮次、当前审批人、受阻、恢复当前审批人、改派当前审批人、取消受阻审批、更新审批流程版本、撤回审批、待我处理、待我审批、我发起的审批”。必须删除「团队待处理」「未分派」「领取」「开始处理」「退回团队」「采购二次确认待办」「低毛利上级确认」等术语。

界面不得显示 `POOL`、`DIRECT`、instance、execution、binding、retry、幂等键或内部 ID。

## 6. `docs/dev-plan` 合同（DOC-A）

本目录由 DOC-A 独占；DOC-A 合并前必须同时交付：

- `README.md` 声明该目录是阶段执行唯一来源，并登记 DOC → P0 → P1—P5 → P6 的固定顺序；
- `conventions.md` 登记 owns 规则、冻结文件清单、P0 amendment 流程、集成测试独占目录、质量门禁和分支命名；
- `domains.md` 登记各 crate 的允许/禁止依赖、审批专项模块归属和业务域到 `DocumentType` 的映射；
- `_meta.json` 机器可读地登记阶段 ID、分支、`owns`、`forbids`、依赖和验收命令，并列出逐类型批次；
- `approval-workflow.md` 登记已签署输入引用表、阶段映射、逐类型批次状态、`BusinessDocument` 注册前置、P0 amendment 记录和切换状态表。

生命周期矩阵**不在**本目录重复，只在 `approval-workflow.md` §1 登记其在合同中的位置。

## 7. OpenAPI、错误目录与校验

OpenAPI 必须覆盖阶段 06 全部端点、认证、权限双门禁的按动作映射、请求白名单、字符串版本、幂等键、view、游标、页上限、成功响应和 403/404/409/422/500 envelope。

错误目录必须逐项规定错误码、HTTP、触发条件、是否可重试、是否提交业务事实、前端动作和允许暴露的信息。`APPROVAL_POLICY_NOT_REGISTERED` 必须属于 500/readiness，不得作为客户端 422。

必须在 `erp-client` 登记 `@redocly/cli` 开发依赖并由 `package-lock.json` 锁定，增加脚本：

```json
"openapi:lint": "redocly lint ../docs/approval-workflow-openapi.yaml"
```

阶段 11 必须通过 `npm run openapi:lint` 执行。禁止依赖开发者全局安装、临时 `npx` 下载或未提交的第二种 lockfile。

## 8. 运维合同

`docs/runbooks/approval-workflow.md` 必须规定：

- BLOCKED 数量、最长持续时间和按 blocker 分类排查；
- ACTIVE 执行与 OPEN WorkItem 不一致处置；
- 决定/CAS/幂等冲突和延迟指标；
- outbox backlog、oldest age、retry、dead letter 处理；
- 开发环境 reset preview、显式确认、执行和 verify 操作；
- 逐类型发布定义的启用门禁与顺序（无全局运行开关）；
- 启用失败时再次停写、硬重置和前向修复责任；
- 禁止直接改库跳过节点、重开旧任务或改写审计。

## 9. 一致性扫描

扫描必须区分“目标实现”和“删除合同”。权威文档、P0-D `deletes` 清单与开发重置脚本允许引用旧名称；不得把这些合同性命中计入生产实现零命中。P0-D 必须对 `backend` 与 `erp-client` 执行下列生产扫描，并只允许重置脚本命中：

```bash
rg -n 'CARD_SALES_APPROVAL|approval_step_instance|RETRY_CURRENT_STEP|TERMINATE_APPROVAL|REJECT_TO_APPLICANT|responsibility_mode|assignment_mode|claim|start_processing|release_to_team|PendingSalesLeader|PendingOperations|PENDING_PROCUREMENT_CONFIRMATION|PENDING_LOW_MARGIN_SUPERIOR|PurchaseReviewStatus|approval_instance:(recover|diagnose)|card_sales_(operations_pool|unique_sales_manager)|APPROVER_BECAME_INELIGIBLE' \
  backend erp-client \
  --glob '!backend/scripts/reset-dev-business-data.sh'
```

该命令在 P0-D 合并前必须无输出。开发重置脚本中的命中必须逐项对应固定删除字段、集合或权限，不得被生产代码引用。`POOL`、`WAITING`、`REJECTED`、`DISABLED`、`ENABLED` 是跨域通用词，只允许由人工按审批上下文分类，不得使用全仓字符串零命中代替语义检查。

BPM 边界必须由 `backend/scripts/check-bpm-boundaries.sh` 失败关闭地验证。宏定义必须执行：

```bash
test "$(rg -n -U '#\[proc_macro\]\s*pub fn id_type\b' backend/crates/entity-macros/src/lib.rs | wc -l | tr -d ' ')" = 1
test "$(rg -n 'macro_rules![[:space:]]+id_type' backend | wc -l | tr -d ' ')" = 0
```

文档合并门禁固定为 `git diff --check`、`jq empty docs/dev-plan/_meta.json`、相对 Markdown 链接检查和阶段元数据引用完整性检查。下列过程式表述在 DOC-A/B/C 合并前必须为零：

```bash
rg -n '[补]签|二次[补]签' \
  docs/approval-workflow-contract.md \
  docs/erp-phase-1.md \
  docs/erp-phase-2.md \
  docs/approval-workflow-implementation-plan \
  docs/dev-plan
```

该命令必须无输出。文档中对旧名称的引用必须是明确的“删除、禁止、失败关闭或扫描”条款；未分类命中不得通过。

## 10. 文档写法

所有文档必须使用合同式、执行指导式表达：使用“必须、不得、仅允许、完成条件”；明确责任主体、输入、动作、输出和失败处理；只记录生效合同，不记录讨论过程或实施复盘。

## 11. 完成条件

### 11.1 DOC-A（已合并；P0-A 已解锁）

- [x] 政策、生命周期、试点、权限、WorkItem、BLOCKED 和通知合同已签署。
- [x] 第 0 节生效决定已写入合同，销售提交、`ReviewStatus`、采购确认 purpose、责任模型和工作台取值唯一。
- [x] `docs/dev-plan` 五个文件完整存在，并登记 `owns`、`ownsWithin`、`creates`、`deletes`。
- [x] 每个必改文件和每个必删旧文件都有唯一责任阶段；无主文件为 0。
- [x] `erp-phase-1.md`、`erp-phase-2.md` 的「不建设可配置审批流」「审批定义按代码注册」「`CARD_SALES_APPROVAL` 固定步骤」「运营 `POOL` 审批节点」「驳回后改单重提」已全部重写，包括 §8.1.1 的固定顺序代码块、§16 页面规则、§17.2 切换步骤和时序图。
- [x] 合同旧「数据迁移与应用切换」章节已整节替换为「开发环境硬切换」。

### 11.2 DOC-B（内容就绪，待 DOC-A 后合并；继续阻断 P2）

- [x] `erp-data-model.md` 已用目标集合替换旧 `approval_definition` / `approval_step_definition` / `approval_instance` / `approval_step_instance` 模型。
- [x] `work_items.approval_step_instance_id` 与 `uk_work_items_open_approval_step` 已替换为 `approval_node_execution_id` 与 `uk_work_items_approval_execution`。
- [x] 审批节点的 `assignment_mode = DIRECT/POOL` 描述已删除；全系统 WorkItem 不再存在 `POOL` 模型，任何 `OPEN` 任务的 `owner_user_id` 必填。
- [x] 已新增第 4.2 节列出的目标字段、版本、索引和开发环境删除范围。

### 11.3 DOC-C（内容就绪，待 DOC-A 后合并；继续阻断 P4）

- [x] W01 已扩写为唯一工作台（合同 §16.4），W02 已标记废止，W05/W19 已同步，`w24-approval-processes.md` 已新增。
- [x] `ui-glossary.md` 已收录第 5.3 节术语，并删除「团队待处理」「领取」「开始处理」「退回团队」；界面不出现 `POOL`、`DIRECT`、instance、execution、binding、retry、幂等键和内部 ID。

### 11.4 DOC-D（卡 P6-PILOT）

- [ ] OpenAPI 覆盖阶段 06 全部端点、权限双门禁的按动作映射、白名单、游标和 envelope。
- [ ] 错误目录逐项规定错误码、HTTP、可重试性、是否提交业务事实和前端动作。
- [ ] `runbooks/approval-workflow.md` 覆盖 BLOCKED 排查、outbox 处置、reset 操作和逐类型启用顺序。
- [ ] `@redocly/cli` 已登记为 `erp-client` 开发依赖并由 `package-lock.json` 锁定，`npm run openapi:lint` 通过。

### 11.5 全段共同

- [ ] 业务、数据、API、错误、页面、术语和 runbook 只描述一套审批语义。
- [ ] 全部权威文档使用同一 BPM/ERP 单向依赖、模型归属和事务应用合同。
- [ ] 全部权威文档明确无数据迁移、硬重置和前向切换，不包含双写、旧运行时或兼容读取。
- [ ] 文档旧语义扫描的每个剩余命中都有合法归属。
- [ ] 文档不包含讨论复盘，所有条款可以直接分派和验收。
