# 审批流程合同技术落地总纲

> 状态：执行合同；DOC-A、DOC-B、DOC-C 已合并，P0-A 已解锁；DOC-D 等待 P3-HTTP
>
> 权威输入：`docs/approval-workflow-contract.md`、`docs/dev-plan/approval-workflow.md`
>
> 代码基线：2026-08-17 当前工作树
>
> 交付模型：P0 地基先行；通用能力与唯一试点完成后执行 `P6-PILOT`；试点通过后按机器阶段逐类型接入；`P0-D` 删除全部旧路径；`P6-FINAL` 完成全量验收

## 1. 交付目标

本次改造必须按目标合同重建审批定义、单据绑定、运行实例、节点执行、实例审批人和审批任务链路。项目处于开发期且无须保留业务数据，旧审批模型、旧集合读取、旧历史投影、双写、双运行时和运行期 fallback 必须全部删除；开发环境通过阶段 09 硬重置切换。

下列旧合同必须退出生产写路径：

| 旧实现 | 当前证据 | 目标处理 |
| --- | --- | --- |
| 编译期流程结构注册 | `backend/services/src/approval/registry.rs` | 只保留强类型 `DocumentType` 政策；流程结构来自已发布定义 |
| 启动期 definition bootstrap | `backend/services/src/approval/bootstrap.rs`、`backend/apps/web-api/src/main.rs` | 删除启动写定义；定义由管理端创建和发布 |
| 审批节点 `DIRECT/POOL` 双模式 | `backend/entities/src/approval/types.rs` | 审批节点只指定一个用户；审批任务固定个人责任 |
| 全系统责任池与团队任务 | `backend/entities/src/work_item/work_item.rs::AssignmentMode`、`services/src/work_item/**`、`erp-client/features/unified-task-queue/**` | 删除 `AssignmentMode`、`claim`/`start_processing`/`release_to_team` 与「团队待处理」视图；任何任务创建即指定到人 |
| 采购二次确认作为团队业务任务 | `backend/entities/src/sales_review/procurement_confirmation.rs`、`services/src/sales_review/procurement_decision.rs` | 收敛为 `SalesOrder` 审批链中的普通节点；实体、状态机与驳回原因码删除，选源后移到采购单 |
| 低毛利上级确认 | `backend/entities/src/sales_review/low_margin_manager_confirmation.rs`、`services/src/sales_review/low_margin_confirmation.rs`、`services/src/sales_order/procurement_rejection.rs` | 整体删除；毛利只保留只读风险提示 |
| 待办队列与个人工作台两个页面 | `erp-client/features/{workspace,unified-task-queue}/**` | 合并为唯一 `/workspace` 页面（列表 + 详情主从），`/workspace/tasks` 永久重定向 |
| resolver、角色池和 handler 字符串 | `backend/services/src/approval/{resolver,registry}.rs` | 删除流程结构 resolver/handler；资格和领域动作使用强类型政策 |
| 启动时预建全部 `WAITING` 步骤 | `backend/services/src/approval/runtime.rs` | 每次令牌进入节点时新建一条执行 |
| 驳回或终止实例 | `backend/entities/src/approval/types.rs`、`runtime.rs` | 决定只允许通过或驳回；驳回开启下一轮并回到入口节点 |
| 通用恢复命令 | `backend/services/src/approval/dto.rs`、`runtime.rs` | 删除 `RETRY_CURRENT_STEP`；人员失效只允许恢复已重新合格的原审批人，或改派仍失效的当前审批人 |
| 客户端选择定义、下一节点或处理人 | 当前启动和决定 DTO | 一律从单据绑定、冻结图和当前执行读取 |
| `sequence_no + 1` 推进 | `runtime.rs::advance_after_approval` | 只读取冻结 transition 的唯一出口 |

## 2. 实施前权威输入门禁

DOC-A 必须先于 P0 合并。DOC-A 合并前，P0-A 不得开始。下列合同已在 `approval-workflow-contract.md` 中以唯一确定值生效，`docs/dev-plan/approval-workflow.md` §1 登记合并状态。任何一项被发现仍有缺项、候选动作、二选一或待定说明时，必须停止对应阶段并退回 DOC 阶段修订合同；实现人员不得替业务合同设置默认值。

### 2.1 单据政策与生命周期矩阵（已签署）

| 输入 | 权威位置 |
| --- | --- |
| 20 行政策矩阵与类型级权限前缀 | 合同 §4.3 |
| 12 行状态、可提交状态与 `subject_version` 权威来源 | 合同 §4.4.1 |
| 业务状态机收敛（删除逐节点审批态） | 合同 §4.4.2 |
| 团队业务任务整体废止、采购二次确认收敛为审批节点、低毛利上级确认删除 | 合同 §4.4.3 |
| 强类型动作、最终副作用与撤回合同 | 合同 §4.4.4 |
| `subject_snapshot` 有界字段、`owner_role` 与责任组织来源 | 合同 §4.4.5 |
| 毛利风险只读提示 | 合同 §4.4.6 |
| 唯一试点 `StockAdjustment` | 合同 §4.5 |
| 唯一责任模型（无责任池、无领取） | 合同 §1 第 7 条、§13.2 |
| W01/W02 合并为唯一工作台 | 合同 §16.4 |

固定结论：

1. 全部固定 `DocumentType` 为 **20** 个，其中 **12** 个 `PROCESS_REQUIRED`、**8** 个 `NO_APPROVAL`；
2. `SalesOrder` 已按 `BusinessType` 拆为两个独立 `DocumentType`：`SalesOrder`（`GoodsService`）与新增 `VoucherSalesOrder`（`Voucher`）。创建时必须以穷尽 `match` 分派，不得在同一类型内暗中选择流程；
3. 9 个类型必须新增 `approval_subject_version: u32` 作为 `subject_version` 权威来源，不得复用 `BaseModel.version`；`PurchaseOrder` 不得使用最终通过后才生成的 `purchase_revision.revision_no`；
4. 不存在团队业务任务。`SalesOrder` 定义必须恰好包含一个采购确认用途节点，卡券运营是普通审批节点，低毛利上级确认整体删除；12 个类型的审批启动点一律是该类型自身的提交命令（合同 §4.4.1、§4.4.3、§5.2）；
5. `ReviewStatus` 为**三值**（`NOT_SUBMITTED`、`IN_APPROVAL`、`APPROVED`）；12 个类型均删除审批导致的业务 `REJECTED`，受控撤回和受阻取消统一回到可修正草稿，`PurchaseReviewStatus` 必须删除（合同 §4.4.2 第 1、5 条）；
6. 全系统只有一种责任模型：任何 `OPEN` 任务都有非空 `owner_user_id`。`AssignmentMode` 枚举与 `assignment_mode` 字段、`claim`/`start_processing`/`release_to_team`、`AssignmentSource::SelfStart`/`AdminRelease` 全部删除（合同 §1 第 7 条、§13.2）；
7. 待办队列与个人工作台合并为唯一 `/workspace` 页面，布局为列表 + 详情主从（合同 §16.4）。

### 2.2 校验时点合同

校验职责固定如下：

| 时点 | 必须校验 | 不得校验 |
| --- | --- | --- |
| 定义发布 | 账户有效、静态审批权限、节点用途完整性、节点间人员规则、图不变式 | 具体单据 DataScope、具体提交人与审批人的动态隔离 |
| 单据创建绑定 | 当前发布定义存在、定义适用、单据组织和创建人上下文 | 未来节点的运行期状态 |
| 启动和每次进入节点 | 账户有效、审批权限、单据读取权、DataScope、提交人隔离 | 从角色池推测替代人员 |
| 审批决定 | 当前责任、全部运行期资格、对象版本、CAS 版本 | 依赖前端 `allowed_actions` 代替服务端校验 |
| 恢复/改派 | 当前阻塞原因为人员失效；恢复要求原审批人全部重新合格，改派要求原审批人仍失效且候选人全部合格 | 图损坏、任务冲突、版本损坏等非人员阻塞恢复 |

合同 §15.1 第 13 条已签署：**不采用**「审批人必须具备该 DocumentType 全组织范围」的规则。因此发布只校验账户有效、静态审批权限、节点间可静态判断的岗位分离和图不变式，不得把无具体资源的发布动作描述为完成了实例级 DataScope 校验。

### 2.3 WorkItem 适配合同（已签署）

映射已冻结于合同 §13.3，包含 `WorkItemType::DocumentApproval`、`approval_node_execution_id` 唯一关联字段、删除 `AssignmentMode`/`assignment_mode`、新增 `AssignmentSource::ApprovalRuntime`、`owner_role = <prefix>_approver`、`owner_organization_id` 取 `subject_snapshot.responsible_org_id`、`WorkItemFamily::Approval`、通用命令失败关闭错误码、`BLOCKED` 时的任务关闭原因，以及同一 `approval_node_execution_id` 在全生命周期最多一个 WorkItem 的约束。

现有责任模式字段是 `assignment_mode`（类型 `AssignmentMode`，取值 `Direct`/`Pool`）。按合同 §13.2 第 5 条，该字段与枚举整体删除，不保留只剩 `Direct` 的单值枚举，也不改名为 `responsibility_mode`。`AssignmentSource` 现有 6 个值（`StepResolver`、`SystemRule`、`SelfStart`、`AdminReassign`、`AdminRelease`、`RecoveryResolver`）中没有等价的审批运行时来源：必须新增 `ApprovalRuntime`，删除 `StepResolver`、`RecoveryResolver`（旧审批解析语义）和 `SelfStart`、`AdminRelease`（责任池语义），最终为 `SystemRule`、`AdminReassign`、`ApprovalRuntime` 三值。

非审批任务的创建方必须同批改造：`services/src/{integration_ops,legacy_import,mall_sync,publication,supplier_fulfillment,supplier_settlement}/**` 等所有 WorkItem 构造调用方必须在创建时解析出唯一 `owner_user_id`；解析失败必须失败关闭并告警，不得写入空责任人。

### 2.4 权限双门禁（已签署）

动作级 12 个权限见合同 §15.2。每个请求还必须通过合同 §4.6 按动作穷尽映射的 Service 门禁：定义管理与绑定升级使用 `<prefix>:approval_definition_admin`，运行管理使用 `<prefix>:approval_runtime_admin`，普通决定使用当前任务责任与审批资格，普通撤回使用原提交人身份；应急撤回才允许运行管理员代办并要求原因。不得把两类管理员权限施加给全部普通运行操作。

具备动作级权限不得自动获得全部 DocumentType、实例或业务对象的管理权。两类管理员权限必须进入后端权限种子、生成物和权限目录；缺失政策或应注册的类型权限属于服务端部署不变量错误。

### 2.5 通知合同（已签署）

10 类事件的收件人、去重键、模板参数白名单、首次投递加最多 5 次重试（共最多 6 次尝试）、退避序列（1m / 5m / 15m / 1h / 6h）和死信处置已冻结于合同 §16.5。事务内只写 outbox，不得调用外部通知服务。

## 3. 目标架构与事务边界

```text
HTTP / Business Handler
        |
        v
ERP Application Service ---------------------> Approval Policy / Business Adapter
        |                    |                                |
        |                    +-----------> entities ----------+
        |                                     |
        +-----------> bpm <-------------------+
        |                |
        |                +-- pure model / graph / state engine
        |
        +-----------> database -----------> bpm + entities
                             |
                  one MongoDB transaction
```

审批业务依赖方向固定为：

```text
apps/web-api -> services
services     -> bpm + entities + database
database     -> bpm + entities
entities     -> bpm
bpm          -> entity-core + entity-macros + workspace 外部基础库
```

`bpm` 必须是下层纯领域 crate，不得依赖 `entities`、`database`、`services`、`apps/web-api`、`mongodb`，不得包含 ERP `DocumentType`、权限、DataScope、WorkItem、业务动作、HTTP DTO、通知投递或 MongoDB `Executor`。`DocumentType` 必须由 `services` 通过穷尽映射转换为 `bpm::ProcessKind`；业务对象必须转换为 `bpm::SubjectRef`。Handler 不得直连数据库；Repository 不得决定流程状态；跨集合原子性和业务副作用由 Service 负责。

`bpm` 只根据完整输入计算领域状态和 `TransitionPlan`/领域事件。Service 必须在同一 MongoDB 事务中加载 BPM 与业务事实、执行写时授权重验、调用 BPM 引擎、执行强类型业务动作，并持久化 BPM、业务主体快照、业务单据、WorkItem、审计、命令收据和通知意图。BPM 引擎不得自行开启事务或调用业务回调。

## 4. 阶段与所有权

| 阶段 | 文件 | 所有权与交付 |
| --- | --- | --- |
| 10 / DOC-A | [合同与工作面文档同步](./10-contract-document-synchronization.md) | **P0-A 的唯一前置**。权威业务合同、两份 phase 文档和 `docs/dev-plan` |
| 10 / DOC-B | 同上 | **P2 的前置**。`erp-data-model.md` |
| 10 / DOC-C | 同上 | **P4 的前置**。W01 扩写为唯一工作台、W02 标记废止、W05/W19/W24 与 `ui-glossary.md` |
| 10 / DOC-D | 同上 | 依赖 P3-HTTP，**P6-PILOT 的前置**。OpenAPI、错误目录、runbook、`openapi:lint` |
| 00 / P0 | [共享地基与冻结合同](./00-foundation-and-shared-contracts.md) | `bpm` workspace/manifest、单向依赖、共享注册与目标模块占位、错误、路由入口、依赖注入、权限生成和可编译接口地基 |
| 01 / P1 | [BPM 领域模型与 ERP 集成实体](./01-policy-and-domain-model.md) | `bpm` 流程模型/图规则；`entities` 仅承载单据绑定、业务对象快照、WorkItem 和 outbox 集成模型 |
| 02 / P2 | [持久化、索引与 Repository](./02-persistence-and-indexes.md) | Repository、索引和事务扩展；不编写集成测试 |
| 03 / P3 | [流程定义管理 Service](./03-definition-management-service.md) | ERP 定义草稿编排、BPM 图校验调用、发布、退役和类型级权限 |
| 04 / P3 | [单据绑定与业务适配](./04-document-binding-and-business-adapters.md) | Adapter 合同、一个试点和逐 DocumentType 独立批次 |
| 05 / P3 | [BPM 引擎与 ERP 运行编排](./05-runtime-rejection-and-reassignment.md) | `bpm` 单令牌状态引擎；Service 事务编排、WorkItem 适配、阻塞恢复、幂等和 outbox 投递 |
| 06 / P3 | [HTTP、权限与错误合同](./06-http-permissions-and-errors.md) | HTTP DTO、分页、权限组合、错误映射和 OpenAPI 实现 |
| 07 / P4 | [前端审批流程配置](./07-frontend-definition-management.md) | 配置、发布、退役和未提交单据升级入口 |
| 08 / P4 | [前端单据审批区与工作台](./08-frontend-document-and-workbench.md) | 审批区、决定、撤回、历史、W01/W02 合并后的统一工作台、人员恢复/改派和受阻取消 |
| 09 / P5 | [开发环境重置与硬切换](./09-development-reset-and-cutover.md) | 受控清空旧审批业务数据、索引重建、试点冒烟和只前向切换 |
| 11 / P0-D | [试点验收与最终验收](./11-integration-and-acceptance.md) | 全部逐类型阶段后删除旧模型、旧运行时、旧责任动作、旧端点和旧权限 |
| 11 / P6 | [试点验收与最终验收](./11-integration-and-acceptance.md) | `P6-PILOT` 纵向试点门禁；`P6-FINAL` 全仓、浏览器、硬切换和发布证据 |

## 5. 执行规则

0. **DOC-A（文件 10 的合同与 `docs/dev-plan` 部分）必须先合并**。第 2 节全部权威输入生效、`docs/dev-plan` 五个文件存在后，才允许开始 P0-A。DOC 的其余三段各自卡住自己的下游阶段：DOC-B 卡 P2，DOC-C 卡 P4，DOC-D 卡 P6-PILOT。DOC 不得与其下游阶段并行，也不得由 P0 或 P6 承担其内容。
1. P0-A 初始地基随后合并；P1—P5 在 rebase 最新 P0 后独立实施。实现产物需要 AppState、路由、权限生成或 workspace 接线时，必须再提交对应单主题 P0-B/P0-C amendment，合并后相关分支重新 rebase。
2. 一个子阶段一个分支、一个 PR，只能修改所属 `owns` 前缀，或 `_meta.json` 用 `ownsWithin` 明确分配给它的符号范围。
3. P1—P5 不得修改冻结文件；确需修改时必须停止当前 PR，单独提交 `chore/erp-p0-amend-<主题>`，合并后所有分支 rebase。P0-A 已按 `conventions.md` 第 2 节创建全部目标模块占位并完成模块声明，P1—P5 不应再因注册模块而触碰冻结文件。
4. P2、P3 不得编写 Mongo/HTTP 集成测试；`P6-PILOT` 和 `P6-FINAL` 共同独占集成测试目录。
5. 每个合并阶段必须在最新目标分支上通过 `docs/dev-plan/conventions.md` 第 6 节适用的全仓门禁。尚未形成业务闭环的目标模块必须保持未接线或失败关闭；不得合并无法编译的中间态。
6. 不得以兼容枚举、旧字段 fallback、双写、前端推断或 Noop 领域动作换取局部编译通过。
7. 最终阶段不得承担共享入口接线；P0-A/P0-B/P0-C 和试点所需共享入口必须在 `P6-PILOT` 前完成。全部逐类型阶段完成后必须先执行已登记的 `P0-D` 硬切换清理并通过全仓门禁，`P6-FINAL` 只负责验收。
8. 准备阶段允许旧文件继续存在以保持未切换调用方可编译，但旧路径必须不可达且不得被目标代码引用。未完成 rollout 的 `PROCESS_REQUIRED` 类型必须返回 `APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER`，不得回退旧运行时；`P0-D` 后旧符号必须零命中。

## 6. 全局不变量

实施和评审必须同时满足：

1. ERP 政策将 `DocumentType` 穷尽映射为唯一 `ProcessKind`，流程结构只由该 kind 当前已发布定义决定；
2. 单据创建时冻结定义 ID 和定义版本；单据提交启动时冻结权威 `subject_version`；
3. 已绑定定义退役后仍可启动，运行实例不得切换定义；
4. 一个实例同时最多一个 `ACTIVE` 或 `BLOCKED` 节点执行；
5. 一个活动节点执行至多一个关联 WorkItem；只有 `ACTIVE` 且责任有效时该任务为 OPEN；
6. `task_version`、`instance_version`、`execution_version`、`assignment_version` 和 `subject_version` 各自独立；
7. 决定、执行状态、实例状态、业务主体快照、领域动作、WorkItem、审计、命令收据和通知意图位于同一事务；不适用的写入项必须由命令类型显式裁剪，不得拆分提交；
8. 驳回不修改业务单据、不结束实例、不由请求选择节点或处理人；
9. 人员失效阻塞若继续运行，只能恢复原审批人或改派；原提交人或具备类型运行管理权的应急运行管理员可在服务端允许时退出并撤回到草稿。非人员一致性 blocker 只能执行受阻取消，不得修改定义、跳过节点或改写冻结版本；
10. 权限、DataScope、对象读取权和岗位分离在每个写事务中重验；
11. 前端只展示服务端事实和 `allowed_actions`，不得推导责任、下一节点或恢复方式；
12. 旧审批模型、旧审批集合和旧字段不得进入生产读写路径；开发环境硬切换后必须清零。
13. `bpm` 对 ERP 业务层保持零反向依赖；任何 `entities`、`database`、`services`、`apps/web-api`、`mongodb` 引用均为阻断。
14. BPM 引擎输出只包含流程状态、领域事件和中性任务意图；不得输出 ERP URL、权限名、业务命令或通知模板。
15. 任何 `status=OPEN` 的 `work_item`（审批与非审批）都必须有非空 `owner_user_id`；责任池、领取、开始处理和退回团队在全仓不得存在实现、端点、权限项或文案。
16. `SalesOrder` 与 `VoucherSalesOrder` 的提交与责任模型完全一致，不得为采购确认节点写专用识别分支；销售单选源只允许出现在采购单创建路径。

## 7. 合同覆盖

| 合同范围 | 主责阶段 | 完成证明 |
| --- | --- | --- |
| 系统边界、政策和共享合同 | 00、01、10 | 政策矩阵、权限矩阵、WorkItem 映射和可编译地基 |
| 定义与发布生命周期 | 01—03、07 | 图不变式、发布事务、管理页面和冲突处理 |
| 单据创建绑定与业务动作 | 04、08 | 试点证据、逐类型生命周期表和独立批次验收 |
| 运行、驳回、取消、阻塞、恢复和改派 | 01、02、05、08 | 状态机、CAS、阻塞矩阵、责任任务和历史回放 |
| HTTP、权限、安全与查询 | 06、10 | OpenAPI、错误目录、权限双门禁和分页合同 |
| 开发重置和硬切换 | 09、11 | reset preview/execute/verify、索引证明和旧运行时清零 |
| 集成与发布 | 11 | 真实 MongoDB、HTTP、浏览器、并发和事务回滚证据 |

## 8. 修改面总索引

实施人员必须以本表定位 owner，再进入对应阶段文档执行。表中路径是当前基线的最小修改面；阶段实施时还必须使用全仓符号搜索识别调用方，不得只改表内单个入口。

| 修改面 | 文件或目录 | Owner |
| --- | --- | --- |
| BPM workspace 与依赖边界 | `backend/Cargo.toml`、`backend/crates/bpm/Cargo.toml`、`backend/scripts/check-bpm-boundaries.sh` | P0-A |
| 后端仓库架构指南 | `backend/AGENTS.md` | P0-A；必须登记 BPM/ERP 单向依赖和文件归属 |
| BPM 公共入口与稳定 ID | `backend/crates/bpm/src/{lib,ids,error}.rs` | P0-A |
| ID newtype 宏 | `backend/crates/entity-macros/src/lib.rs` | P0-A；`id_type!` 必须实现为函数式过程宏供 `bpm` 与 `entities` 共用；`proc-macro` crate 不得导出普通 `macro_rules!` |
| ERP/BPM 种类映射 | `backend/services/src/approval/process_kind.rs` | P0-A；`DocumentType <-> ProcessKind` 唯一穷尽映射，覆盖 20 个值 |
| `DocumentType` 枚举与销售单拆分 | `backend/entities/src/document_registry/business_document.rs` | P0-A（`ownsWithin`：仅 `DocumentType` 枚举，新增 `VoucherSalesOrder` 共 20 值）；同文件的 `ApprovalDefinitionBinding` 值对象归 P1 |
| 三层共享声明与目标模块占位 | `backend/entities/src/{lib,ids}.rs`、`backend/database/src/{lib.rs,repository/mod.rs,repository/extensions/mod.rs,indexes/mod.rs}`、`backend/services/src/approval/mod.rs` | P0-A；必须同时创建后续阶段的失败关闭占位模块 |
| 旧审批持久化实现删除 | `backend/database/src/repository/{approval.rs,extensions/approval.rs}`、`backend/database/src/indexes/approval.rs` | P0-D（`deletes`）；P2 只交付独立新路径并使旧路径不可达 |
| 旧审批运行时与解析器删除 | `backend/services/src/approval/{runtime.rs,resolver.rs}` | P0-D（`deletes`）；P3-RUNTIME 只交付 `approval/execution/**` 并使旧命令失败关闭 |
| 阶段执行合同 | `docs/dev-plan/**` | DOC-A；P0 与 P6 均不得修改 |
| BPM 流程领域模型与图规则 | `backend/crates/bpm/src/{model,graph}/**` | P1 |
| BPM 纯状态引擎 | `backend/crates/bpm/src/engine/**` | P3 Runtime |
| 业务实体依赖接线 | `backend/entities/{Cargo.toml,src/lib.rs,src/ids.rs}` 及现有审批 ID 调用方 | P0-A；审批 ID 从 `entities` 移出，禁止保留第二定义源 |
| `entities::approval` 临时 facade 清理 | `backend/entities/src/{lib.rs,approval/mod.rs}` | P0-D；逐类型切换期间只允许未切换旧调用方编译使用，目标代码不得引用 |
| 单据绑定与动作审计 | `backend/entities/src/document_registry/{business_document,workflow_action}.rs` | P1 |
| 审批 WorkItem 类型与字段 | `backend/entities/src/work_item/work_item.rs` | P1 |
| 业务对象快照与通知 outbox 集成实体 | `backend/entities/src/approval_integration/**` | P1 |
| Repository 与扩展 | `backend/database/src/repository/{bpm,approval_integration,work_item}.rs`、`repository/extensions/{bpm,approval_integration}.rs` | P2 |
| 索引 | `backend/database/src/indexes/{bpm,approval_integration,work_item}.rs` | P2 |
| 数据库共享注册 | `backend/database/src/{lib.rs,repository/mod.rs,repository/extensions/mod.rs,indexes/mod.rs}` | P0-A |
| 定义、政策与 ERP 运行时编排 | `backend/services/src/approval/**` | P3；不得重新实现 BPM 状态机 |
| WorkItem 写保护与路由投影 | `backend/services/src/work_item/**` | P3 |
| 单据注册与绑定查询 | `backend/services/src/document_registry/**` | P3 Adapter Base |
| 销售单与销售变更 | `backend/services/src/{sales_order,sales_review}/**` | 对应 P3 DocumentType 子阶段；`SalesOrder` 与 `VoucherSalesOrder` 各自一个批次 |
| 采购二次确认与低毛利确认删除 | `backend/entities/src/sales_review/{procurement_confirmation,low_margin_manager_confirmation}.rs`、`backend/database/src/repository/{sales_review.rs,extensions/sales_review.rs}`、`backend/database/src/indexes/sales_review.rs`、`backend/services/src/sales_review/{procurement_decision,low_margin_confirmation}.rs`、`backend/services/src/sales_order/procurement_rejection.rs`、`backend/apps/web-api/src/core/{handler/sales_review/**,routes/sales_review.rs}` | `P3-ADAPTER-SALES-ORDER` 先停止新写入并移除端点可达性；P0-D 删除旧文件、字段和跨域残留；选源改由采购单创建路径承担 |
| 全系统责任模型收敛 | `backend/entities/src/work_item/work_item.rs`、`backend/services/src/work_item/**`、`backend/apps/web-api/src/core/{handler,routes}/work_item*`、全部 WorkItem 构造调用方 | P3-RUNTIME 先失败关闭旧动作，P4-WORKFLOW 删除前端调用，P0-D 删除旧符号与端点；任何已切换路径不得产生无 owner 任务 |
| 采购单与采购变更 | `backend/services/src/purchase_order/**` | 对应 P3 DocumentType 子阶段 |
| 收货、交付、履约、验收 | `backend/services/src/fulfillment/**` | 对应 P3 DocumentType 子阶段 |
| 库存调整 | `backend/services/src/inventory/**` | 对应 P3 DocumentType 子阶段 |
| 收款、发票 | `backend/services/src/receivable/**` | 对应 P3 DocumentType 子阶段 |
| 付款 | `backend/services/src/payable/**` | 对应 P3 DocumentType 子阶段 |
| 退货、退款、冲正 | `backend/services/src/returns/**` | 对应 P3 DocumentType 子阶段 |
| 其它 WorkItem 构造调用方 | `backend/services/src/{integration_ops,legacy_import,mall_sync,publication,supplier_fulfillment,supplier_settlement}/**` | 所属 P3 域子阶段 |
| Service 共享导出与错误 | `backend/services/src/{lib,errors}.rs` | P0-A/P0-B |
| 审批 HTTP 与 WorkItem 写保护 | `backend/apps/web-api/src/core/handler/{approval_process,approval_instance,work_item}/**`、`core/routes/{approval_process,approval_instance,work_item}.rs` | P3 HTTP |
| 单据提交、撤回和升级 Handler | 各业务 `core/handler/<domain>/**` 与 `core/routes/<domain>.rs` | 对应 P3 DocumentType 子阶段；试点的 `handler/inventory/**` 与 `routes/inventory.rs` 属于 `P3-ADAPTER-PILOT` 的 `owns`，必须在同一 PR 内删除 approve/reject 端点 |
| Web 共享入口 | `backend/apps/web-api/src/{main,app_state}.rs`、`core/{handler,routes}/mod.rs`、`core/routes/admin.rs`、`build.rs` | P0-B |
| 权限种子与生成物 | `backend/services/src/iam/predefined_roles.rs`、`erp-client/lib/permissions.generated.ts` | P3 HTTP / P0-B、P0-C |
| 流程配置前端 | `erp-client/features/approval-processes/**`、`app/(workspace)/system/approval-processes/**` | P4 |
| 通用审批前端 | `erp-client/features/approval-workflow/**` | P4 |
| 统一工作台与 WorkItem | `erp-client/features/{work-items,unified-task-queue,workspace}/**`、`erp-client/app/(workspace)/workspace/**` | P4；W01/W02 合并为唯一 `/workspace` 页面，`unified-task-queue` feature 的查询与列表能力并入 `workspace` 后该目录整体删除，`/workspace/tasks` 保留为永久重定向 |
| 各单据详情与创建结果 | 对应 `erp-client/features/<document-domain>/**` 与 `erp-client/app/(workspace)/<domain>/**` | 对应 P4 DocumentType 子阶段 |
| 前端共享注册 | `erp-client/lib/workspace-registry.ts`、生成权限和冻结 API 入口 | P0-C |
| 开发重置脚本与切换 runbook | `backend/scripts/reset-dev-business-data.{sh,mongosh.js,md}`、`backend/scripts/test-reset-dev-business-data.sh` | P5；`docs/runbooks/approval-workflow.md` 归 DOC-D |
| Repository/HTTP 集成测试 | `backend/database/tests/**`、`backend/apps/web-api/tests/**` 的审批目标文件 | P6 |
| 权威与工作面文档 | 阶段 10 §1 所列文件 | 文档阶段 |

旧字段 `approval_step_instance_id` / `approvalStepInstanceId` 必须在全仓搜索后按 owner 分批删除；除开发重置脚本中的删除条件外，全仓最终命中必须为 0。

## 9. 完成定义

只有 `P6-FINAL` 的全部门禁通过，才允许把主合同状态改为“已实施”。任何单一阶段完成、局部编译通过、页面可见、`P6-PILOT` 通过或重置脚本成功均不构成整体交付。
