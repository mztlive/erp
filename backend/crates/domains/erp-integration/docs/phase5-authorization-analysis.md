# FSY-11：erp-integration 第五阶段授权检查（R1 分析）

R1 结论：发现一项读取交付合同缺陷（INT-AUTH-01）、一项授权回归证据缺口（INT-AUTH-02）和一项迁移清理遗留（INT-AUTH-03）。需要进入后端开发核验、修复，再独立 Review。本轮只新增分析文档，没有修改业务实现、测试或共享装配。

## 版本、范围与证据等级

- 事项：FSY-11；父事项：FSY-2。分析日期：2026-11-16。
- 分支：`agent/agent/fsy-11`。
- 实际分析与测试基准：`5ca681584940c7b5a9a6847cad345929eb2b647f`。
- 任务原定位基线：`cc248977e7d6b2b73cc7a935f3897b93ea389a4f`；对两者的 `erp-integration` 及 `apps/erp-server/src/wiring/authorization/` 执行限定路径差异检查，没有差异。交接和复审仍以实际 SHA 为准。
- 报告提交 SHA 记录在事项最终交接评论，避免在文件中写入自身提交造成循环。
- 已读：父/子事项、父事项建任务与三 agent 闭环决定、同族任务分工、`backend/AGENTS.md`、本 crate README、Rust 编码规范，以及 `erp-identity/docs/authorization-service-improvement.md` §3.1、3.2、3.3、3.5、3.6、3.7.4、4.1.5、4.1.6。
- 本报告中的路径，未带 `backend/` 的均相对 `backend/`。下文 `I` 表示 `crates/domains/erp-integration/src`，`A` 表示 `crates/domains/erp-identity/src`，`C` 表示 `crates/orchestration/erp-supplier-catalog/src/integration_resolution`，`R` 表示 `crates/read-models/erp-read-models/src/integration_center`。
- “静态确认”表示已核对生产调用链；“单测通过”仅表示本次运行的实际测试；触发时序推演、其他 crate 中的测试及未运行的服务均不写成行为验证通过。

范围为本 crate 第五阶段业务授权、真实事实/映射、完整业务绑定及必要修复。HTTP/worker/Application/工厂切换、路由鉴权清理、server 全量编译属于第六阶段，不列成本轮缺陷。低层事实/持久化步骤按其实际上层授权前置条件检查，不给每个函数重复引入身份服务。

## 逐入口检查表

权限均来自 `erp-permissions::integration_ops` 的公共常量（定义文件 `crates/foundation/erp-permissions/src/integration_ops.rs`）。本 crate 没有重新解析内置权限、构造政策、选择授权主体或独立编译查询。`IntegrationAccess::{resolve,validate}` 只是共享 `AuthorizationService::{query,validate}` 的薄委托，不持有请求授权缓存。

表内 T1～T5 对应后文测试证据；“未执行”表示只读追踪调用方，没有运行其 crate 测试。

| 入口与位置 | 权限 / 目标 | 授权、业务与交付顺序 | Facts / Mapping / 装配 | 测试与结论 |
| --- | --- | --- | --- | --- |
| `IntegrationOpsService::inbox_message_list`，`I/service/inbox_message.rs:36` | `INBOX_MESSAGE_LIST`；`Operation` | `with_transaction` → `require`（:62）→ 分页与计数（:77）→ 事务成功后返回 | 仅功能资格，无需对象 Facts/Mapping | T1；静态符合 §3.5.2 的纯 RBAC 读取特例；不是漏绑 |
| `inbox_message_detail`，同文件 :112 | `INBOX_MESSAGE_DETAIL`；`Operation` | 原事务中 `require`（:133）→ `find_by_id` → 返回视图 | 同上；ID 用于业务读取，没有伪造对象目标 | T1；静态符合纯功能权限合同 |
| 入站登记准备/保存，`I/service/inbox_message.rs:155,257`；公开业务入口在 `C/inbox_message.rs:46` | `INBOX_MESSAGE_REGISTER`；`Operation` | 上层事务内 :67 先 require，再确认来源、构造消息、保存及审计 | 功能入口，无需 Facts/Mapping | 本 crate 纯领域/序列化测试；上层授权单测未执行；低层步骤不重复授权 |
| 消息结果准备/保存，`I/service/inbox_message.rs:186,203,222,277,298`；业务入口 `C/inbox_message.rs:104` | `INBOX_MESSAGE_WRITE_BACK`；`Operation` | `write_back_inbox_result_stage` 在 :128 require，再读消息、校验版本/状态；失败消息和错误任务沿原 executor 写入 | 功能入口；处理人组织是业务事实，不是第二份授权政策 | T4 及实体状态测试；上层行为未执行 |
| `error_task_list`，`I/service/error_task.rs:30` | `ERROR_TASK_LIST`；`query` | 事务内 `resolve_list_scope` → `search_error_tasks` → :73 validate → 纯视图转换 | 错误任务的真实 owner/org；绑定 B1 | T2/T3；静态顺序正确，端到端授权单元证据不足，见 INT-AUTH-02 |
| 错误任务详情，`R/error_task.rs:32`；本域仓储提供根记录 | `ERROR_TASK_DETAIL`；`query` 加业务 ID 收窄 | 同一事务 :56 query → 已授权根 → 所有证据/名称/审计 → :59 validate | 与列表同一个模型实例；绑定 B1 | 仅静态追踪；拥有入口归 FSY-38；未发现本域 Facts/Mapping 分歧 |
| 错误任务准备/保存，`I/service/error_task.rs:108,196`；业务入口 `C/error_task.rs:34` | `ERROR_TASK_CREATE`；`Operation` | :55 require → 消息存在性 → 处理人有效内部组织 → 实体/正式任务/审计写入 | identity 固定为 Function，不要求候选数据范围；无需 Mapping | 创建入口在上层已有拒绝单测源码，本轮未执行；不把 Operation 判作绕过创建候选 |
| `difference_list`，`I/service/reconciliation_difference.rs:31` | `DIFFERENCE_LIST`；`query` | 同一事务 query → 历史操作人筛选 → 根分页/计数 → **:76 validate → :78 异步补最新决定** | 差异真实 owner/org；绑定 B2；决定仅补当前页根的状态/版本 | T2/T3；INT-AUTH-01、02 待处理 |
| 差异详情，`R/reconciliation_difference.rs:41` | `DIFFERENCE_DETAIL`；`query` 加业务 ID 收窄 | 同一事务 :61 query → 已授权根 → 历史/证据/名称 → :69 validate | 与列表同一个模型实例；绑定 B2 | 仅静态追踪；归 FSY-38；它已在补齐后 validate，可作为顺序参考 |
| 差异准备/保存，`I/service/reconciliation_difference.rs:120,230`；业务入口 `C/reconciliation_difference.rs:34` | `DIFFERENCE_CREATE`；`Operation` | :55 require → :59 处理人组织 → 证据/实体 → 原事务保存 | identity 固定 Function；两侧证据是业务证据，不是新授权关系 | 上层测试未执行；不要求无用 Mapping |
| `execute_task_action`，`I/service/task_decision/action.rs:117`；上层 `C/task_decision/action.rs:105` | `INTEGRATION_TASK_ACTION`；Operation **并且**正式 WorkItem 精确任务授权 | 上层 :140 require(Operation)，再 `load_bound_work_item`；`guard.rs:125` 校验当前结果访问，`ActionCommand::authorize` 校验领域办理，再本域执行、任务 CAS、收据 | 功能动作本身无 Facts；精确 `internal::WORK_ITEM_EXECUTE` 使用 work_item Facts。真实类型、关联、版本不能被普通业务 ID 替代 | T4/T5 只证明本域证据/状态；正式任务合同归 FSY-21/35；未运行上层测试 |
| `complete_domain_item`，`I/service/task_decision/complete.rs:48`；上层 `C/task_decision/complete.rs:94` | `INTEGRATION_TASK_COMPLETION`；Operation 加精确 WorkItem | 上层 :129 require → 同类绑定/办理检查 → 领域证据与主题版本 → 本域完成 → WorkItem 完成/CAS → 收据 | 与上项相同；错误任务主题版本和差异决定序号由本域维护 | T4/T5；本域不应新增“正式任务”或复制任务权限组合 |
| `IntegrationAccess::require_decide`，`I/service/access.rs:92`；`execute_direct_decision`，`I/service/task_decision/direct.rs:36` | `DIFFERENCE_DECISION`；`Task { id: 差异ID, version: 当前base.version }` | 上层 `C/task_decision/direct.rs:112` 同事务调用 require_decide，再禁止正式任务关联、检查请求的决定序号及证据、追加决定；回放同样先授权（`C/task_decision/replay.rs:110`） | 真实差异 Facts 提供 assignee；固定任务政策不要求列表范围 Mapping；根版本与决定序号是两种不同版本 | 静态确认；精确指派/版本成功与拒绝场景尚缺本 crate 行为单测，见 INT-AUTH-02 |

业务编排的 `run_audited` 承担跨集合事务，低层步骤只接 `&mut dyn Executor`。本域 `execute_task_action` / `complete_domain_item` / `execute_direct_decision` 是组成上层用例的步骤，不是独立面向请求的受保护 Service 入口。不能仅因它们是 `pub` 就重复创建身份服务；也不能将它们脱离上表授权前置条件暴露成新请求入口。

### 其他公开能力及调用边界

| 能力及位置 | 实际调用方 / 前置条件 | 检查结论 |
| --- | --- | --- |
| `IntegrationAccess::resolve/validate`，`I/service/access.rs:42,58` | 两类列表及 `R` 的详情；接受固定业务入口的公共 Permission，原样委托共享服务 | 没有新政策或独立编译器；失效错误通过 `I/error.rs` 保留身份错误类别 |
| `require_handler`，`I/service/access.rs:69` | 全仓检索没有调用方 | 仍包含创建动作白名单和目标推断，见 INT-AUTH-03；没有证据表明当前被用于越权 |
| `require_handler_org` / `expand_org_units`，`I/service/access.rs:127,155` | 创建/消息回写的上层用例；范围列表请求组织筛选 | `OrganizationDirectoryService` 提供真实主属组织/展开事实；拒绝 `me`、无有效组织及 `company` 占位；这是保留的业务事实职责 |
| `ensure_message_exists`，`I/service/error_task.rs:88` | `C/error_task.rs:59`，已先 require(create) | 仅关联存在性，不返回业务详情，不替代访问授权 |
| `prepare_*` / `apply_*_outcome` / `validation::ensure_version` | 纯构造、状态/版本不变量，由已授权上层调用 | 不访问身份设施；无需给纯函数增加授权 |
| `persist_error_task/persist_difference/persist_inbox_message/update_inbox_message/persist_error_task_with_message_failure` | `C` 的事务步骤；worker 技术流程也必须具备其系统入口前置条件 | 使用传入 executor；保存本域实体，授权由根用例负责。没有私自开启嵌套事务 |
| `service/task_decision/guard.rs` 的关联加载；`service/evidence.rs` 的策略投影、原因表、证据验证 | `C` 的已授权动作、`R` 的已授权详情，内部传入 actor.id | 状态机、证据政策和业务关联应保留，不把业务 policy 名称误判为旧授权政策 |
| `search_error_tasks` / `search_differences`，`I/repository/integration_ops.rs:240,325` | 对应 Service 列表 | `ensure_authorized_model` :630 检查当前阶段/有效期、resource、collection、key、id_field、Object；读页和计数用同一授权 stages 与业务 filter |
| 基础根查询、解决历史/最新决定/操作人差异 ID、命令收据，`I/repository/integration_ops.rs`、`integration_ops/difference_resolution_batch.rs`、`command_receipt.rs` | `C`、`R`、授权 Facts 或本域列表补齐 | 低层事实读取不自行裁决权限；收据读取前上层回放要求当前授权；最新决定只按已过滤页 ID 查询。INT-AUTH-01 是交付顺序问题，不是该仓储漏一份 RBAC |
| `SupplierCredentialRepository`（load/import/refresh/authorization/configuration），`I/repository/supplier_credential/` | `erp-supplier-orders::supplier_credentials`；管理端授权状态读取在 `authorization/read.rs:43` 要求快照、:45 查询真实连接权限，:47 最终 validate；令牌引擎另有部署连接/租约绑定 | OAuth 的 authorization 不是 ERP 权限政策；本域只存储密文及 CAS 状态，无独立用户查询/授权资源。实际管理/回调入口归 FSY-36；本轮未审结整个令牌流程 |
| SupplierCallback / PulledMessage / Consumer 仓储与 pending 队列 | `erp-supplier-orders::{supplier_callbacks,supplier_messages,supplier_order_tracking}`、`erp-supplier-catalog::supplier_catalog_callbacks` | 协议接收在 `supplier_callbacks/mod.rs:54` 核对服务端连接及协议并经 intake 验签；内部 worker 使用租约/版本及连接绑定。不能为后台证据队列制造无业务定义的 ERP Mapping；协议与 worker 全流程本轮未执行 |
| `repository::organization::has_unsettled_business_org` | `apps/erp-server/src/wiring/organization.rs:29` 的组织停用事实适配器 | 只回答组织是否还有未结业务，不交付业务记录；组织操作授权由 identity 根用例承担 |
| entity / DTO / indexes / ports | 规则、序列化、索引注册、窄事实合同 | 没有独立受保护业务用例；无须强行引入授权调用 |

## 权限 → Facts / Mapping → 生产绑定

| 编号 | 权限与事实 | 映射合同 | 绑定位置及复用 |
| --- | --- | --- | --- |
| B1 | `ERROR_TASK_LIST/DETAIL`；`IntegrationFacts::load` 在 `I/repository/authorization.rs:62` 按真实 ID 从 `integration_error_tasks` 读 owner_user_id、owner_org_unit_id、base.version | 同文件 :113 构造 `QueryOutput::Object`，模型 key/collection 为 `integration_error_tasks`，主键 id；有效记录 deleted_at=0；Owned / OrganizationIn 使用上述真实字段 | `apps/erp-server/src/wiring/authorization/mod.rs:119` 创建共享 Facts；:122～132 将错误任务列表和详情绑定到同一 Arc Mapping |
| B2 | `DIFFERENCE_LIST/DETAIL`；同文件 :73 读 `reconciliation_differences` 的 owner/org/version；:82 将真实 owner_user_id 作为 assignee | 模型 key/collection 为 `reconciliation_differences`；其余静态合同同 B1 | 同装配 :126～132；两个资源共享同一个 Facts 实例，各资源内部列表/详情共享 Mapping 实例 |
| B3 | `DIFFERENCE_DECISION`；固定 Task 入口使用 B2 的差异 Facts、assignee 与精确 base.version | 不是 query；不新增决定 Mapping。identity 的 `task_facts_resource` 在 `A/service/authorization/rules.rs:554` 保留 reconciliation_difference 资源 | B2 已覆盖所需 Facts；Builder 的 `ensure_complete` 会检查任务 Facts |
| B4 | `INBOX_MESSAGE_LIST/DETAIL/REGISTER/WRITE_BACK`、`ERROR_TASK_CREATE`、`DIFFERENCE_CREATE`、`INTEGRATION_TASK_ACTION/COMPLETION` | identity 中为 Function；Operation 仅检查既有功能资格。无范围查询时无需 Facts/Mapping | `A/entity/access_control/scope_spec/catalog/functions.rs:166` 及 `catalog.rs:378`；没有独立绑定是固定合同，并非缺项 |
| B5 | 正式集成任务的办理资格：`internal::WORK_ITEM_EXECUTE`，实际 WorkItem ID/version | 由工作项领域提供真实任务指派/种类/业务关联；集成根的列表可见性不等于任务办理资格 | `apps/erp-server/src/wiring/authorization/mod.rs:97` 绑定 work_item 的 TaskFacts；`erp-workflow/src/service/work_item/access.rs:127` 调用统一 require(Task)；归 FSY-21/35 的上层责任 |

固定范围定义在 `A/entity/access_control/scope_spec/catalog.rs:286`：四个列表/详情权限均为当前范围、默认本人、必需内部组织维度、无历史补充；`rules.rs:304` 将必需组织维度展开为 Owner/Organization 事实要求。Facts 与 Mapping 都使用**当前处理人及其持久化内部组织**，不使用历史 operator、owner_role、业务单据所有人代替。

对象 facts 将协作、历史参与标为已知空集合（`I/repository/authorization.rs:96`），Mapping 对这两项生成永假条件（:150），两侧一致。未知资源、未知维度、未知关系均报错，不放行。返回缺失 ID 的 Facts 集合由 identity 的完整目标集合校验拒绝；本域没有以剩余 ID 认可完整目标的逻辑。该结论为静态调用链核对，不是本轮已执行缺失事实行为单测。

这里的 error task / difference 授权根是集成问题记录，`message_id`、`business_object_id`、左右证据引用是业务关联/证据；当前固定授权规则未声明通过它们展开 sales/purchase 的授权来源，因此本 Mapping 没有 `relations()` 或动态准备是正确的。不应为通过检查而新增跨域授权关系；正式业务重放/修正仍须调用所属领域规则。

Mapping 只有静态字段描述及 predicate/records/key，没有 `preparation()`、空 prepare、伪造修订或请求级共享状态。`model.output=Object` 不放大根行；两类列表的分页和计数都在授权 stages 后执行；本 crate 没有查询式导出/流式导出入口。列表读取统一在事务中，组织请求展开和历史操作人 ID 筛选也使用原 executor，不依赖未经修订保护的非事务动态准备。

`wiring::authorization::build` 在 :36 使用完整 Builder；缺失/重复/不匹配绑定通过错误上抛，没有部分服务或空实现降级。这里只做静态核对。装配模块的 `complete_bindings_construct_shared_service_without_database_reads` 源码存在，本轮没有运行 server 单测或声称 server 编译通过。

## 问题清单与最小修复

### INT-AUTH-01 · P2 · 差异列表在最终复验后继续异步补齐业务结果

- **已确认位置**：`I/service/reconciliation_difference.rs:76` 调用 validate，:78 才 await `project_difference_rows`；该函数在 :188～192 查询最新决定。`A/service/authorization/validate.rs:15` 在最后检查有效期后 :21 调用 `executor.next_read()`，结束旧凭证的读取阶段。
- **触发条件**：列表返回非空页，需要读取当前页的解决记录；授权范围或身份相关有效期在这次追加读取期间到期，或追加逻辑推进阶段。空页也在 validate 结束的阶段后继续走补齐逻辑，但不会触发实际最新决定 DB 读取。
- **预期**：query → 根分页/计数 → 同 executor 补齐全部业务视图 → 一次 validate → 只做同步装配并返回；补齐结束时的阶段/有效期不满足则丢弃整个结果。
- **实际**：先结束授权读取阶段，再读响应中的状态/版本，之后没有交付检查。静态顺序已经违背 §3.5.2；有效期在后续 await 中到期时不会被该列表再次发现。
- **影响边界**：差异列表的响应交付；不是分页/计数 filter 不一致，也不意味着事务看见了事务外撤权。本轮没有运行带真实 DB 延迟的复现，不声称发生过线上泄露。
- **最小修复**：将原 validate 移到 `project_difference_rows(...).await?` 之后、构造/交付响应之前；继续使用原 CompiledAccess 和 executor，不能在先 validate 后再对失效旧凭证 validate。无需 schema、索引、装配或权限编码变化。
- **回归验收**：调用真实生产交付步骤，验证补齐先于最终验证；补齐失败停止交付，最终验证失败丢弃补齐后的页；通过时页和 total 保持原值。使用确定性替身/注入时序，不 sleep、不连接真实 MongoDB，不测试复制出来的流程。

### INT-AUTH-02 · P2（验收证据）· 范围列表、仓储边界和精确差异任务授权缺少本域行为回归

- **已确认覆盖**：`I/repository/authorization.rs:186` 起只有字段 predicate、已知空协作/历史、未知关系的单元测试；`I/service/scope.rs:164` 只检查分页范围版本；`I/service/inbox_message.rs:317` 验证身份设施不可用时读取提前终止；`I/repository/integration_ops.rs:651` 的单元测试仅验证排序。
- **缺失内容**：本 crate 没有执行真实两类范围列表交付流程的允许/拒绝、缺绑定/身份取证故障提前停止、补齐后最终检查；也没有对 `ensure_authorized_model` 的资源错配/失效阶段及 `require_decide` 的精确目标语义建立本域行为证据。181 个测试通过不能证明这些授权场景。
- **问题性质**：这是任务明确要求的证据缺口，不能推断这些场景的当前实现都错误。INT-AUTH-01 的回归属于该缺口中必须先补的部分。
- **最小可执行部分**：先在两类列表私有 executor 步骤增加 `unbound_authorization()` / `unavailable_authorization()` 的拒绝单测，复用现有 `SnapshotExecutor`；断言具体错误，并确保任何业务仓储 I/O 尚未启动。补齐后的交付顺序应通过实际生产 helper 验证成功与失败停止，不新增源码字符串断言。
- **共享能力限制**：`A/testing.rs` 当前公开 test-support 只有上述两个始终拒绝的 fixture，不能构造离线的成功凭证、可控范围/事实和真实过期凭证。生产 Builder 使用真实身份设施且要求完整绑定；直接复制权限算法、开放生产绕过 Builder、伪造公开 CompiledAccess 或扩展 SnapshotExecutor 的边界权限都不可接受。
- **协作方式**：已读父事项中 FSY8-03、FSY-7、FSY5-03、FSY10-02 的同类测试支持记录；本任务应加入同一项协作，不建立第二套夹具。交付前 FSY-9 为 todo、FSY-9/39 评论为空，尚未见已交付的共享测试支持，状态不能证明没有运行。需要完整身份/范围允许场景时，先与父事项的 FSY-9/39 核对并复用共享的 test-support 能力或已有 identity 行为证据；如要扩展共享文件，先在父事项登记拥有者、准确提交、fixture 合同，禁止两个任务分别扩展一套。FSY-11 不擅自修改 identity/server 生产授权合同。
- **完成要求**：开发交接列出各验收场景由哪个实际单元测试覆盖，继承的共享测试必须引用准确 SHA 和真实执行结果；做不到的项明确保留为未验证，不得以 Mapping 文本检查或测试数量替代，也不执行禁用的集成测试来补证。

### INT-AUTH-03 · P3 · 未使用的 require_handler 仍按动作白名单推断目标

- **位置**：`I/service/access.rs:69`，:76～82 按 `[ERROR_TASK_CREATE, DIFFERENCE_CREATE]` 选择 Operation，否则构造 Records。
- **事实与影响**：全仓调用检索没有调用方。现有创建用例在 `C/error_task.rs:55`、`C/reconciliation_difference.rs:55` 已直接传公共常量及 Operation。identity 的固定目标分支在 `A/service/authorization/rules.rs:196`。该 helper 保存了第二份动作→目标知识，属于 §3.7.4 清理遗留；统一 require 仍会拒绝不支持目标，**未证实存在授权绕过**。
- **最小方案**：优先删除零调用的遗留 helper；若保留公开 Rust API 确有需要，则使用显式服务端目标参数/仅已有记录语义，不再维护 create 动作白名单。不要新增“支持动作列表”或临时授权工厂。保留 `require_decide` 取得持久化版本及组织事实方法。
- **验收**：创建/已有记录/精确任务的目标仍由各固定用例明确提供，并由 identity 校验；列出公开 Rust API 是否变化及调用兼容性。无需为一个未使用方法的删除新增结构断言测试。

## 实施顺序、影响文件与兼容风险

1. 后端开发先读取本报告及分析提交，在自己的工作树核对准确基准；保留其他任务变更，新增提交，不用默认分支替代交接版本。
2. 优先修复 INT-AUTH-01，只调整差异列表交付顺序；在该文件内用必要的最小 helper 或现有测试缝隙补真实编排回归，避免为每个 Service/Repository 仅为测试新增接口层。
3. 处理 INT-AUTH-03，删除不再使用的目标推断；记录对公开 Rust API 的影响。
4. 补 INT-AUTH-02 本域可完成的拒绝/交付回归；如共享成功凭证测试支持确实必要，先核对父事项/FSY-9 当前交付再协作。关联入口 FSY-35、FSY-38、FSY-36 分别持有编排、详情读模型、供应商协议/凭据流程；本轮没有要求它们复制修复，也没有修改这些模块。
5. 只运行下文允许的本 crate lib 测试，记录准确命令/结果；报告每个问题的处置、补充证据、分支及交付 SHA 后交独立 Review。审查有问题则保留编号回到分析，不按轮次强制关闭。

预计主要影响 `I/service/reconciliation_difference.rs`、`I/service/access.rs`、本域内联授权测试及本报告。必要的 Facts/Mapping 回归可放 `I/repository/authorization.rs`；仓储守卫回归放 `I/repository/integration_ops.rs`。共享 test-support 的修改是需登记的协作项，当前没有提出任何共享生产实现修改。

修复不需要数据库迁移、改索引、重新分配权限、改变 DTO 或前端生成物。INT-AUTH-01 会让到期/失效结果按既有冲突语义被拒绝，这是预期收紧；不新增业务状态转换。删除 `require_handler` 可能改变 Rust 源码 API，但仓库内当前没有调用方；开发应选择兼容方案并在交接说明。事务提交/真实回滚、跨进程并发、HTTP/worker 运行和第六阶段装配切换仍未验证。

## 测试证据及可执行验收

在 `backend/` 目录实际执行且收集到退出结果：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-integration --lib --locked
```

结果：**exit 0；181 passed，0 failed，0 ignored，0 measured，0 filtered out**。构建与测试使用实际分析 SHA。已有依赖 `proc-macro-error2 v2.0.1` 的 future-incompatibility 警告不影响本次结果，没有据此扩大修改依赖。

| 标识 | 本轮实际测试 / 证据 | 能证明的范围 |
| --- | --- | --- |
| T1 | `service::inbox_message::authorization_tests::inbox_reads_require_identity_before_loading_messages` | 两类入站读取使用实际私有 executor 步骤，身份设施不可用时返回指定身份错误，业务 DB 读取未到达；不证明成功页/真实数据库事务 |
| T2 | `repository::authorization::tests::{actual_handler_and_handler_organization_are_independent_facts,real_integration_object_has_no_collaboration_or_history_membership,history_and_unknown_relations_cannot_grant_integration_read_access}` | 真实 Mapping 的字段翻译、Facts 转换、协作/历史为空和未知关系拒绝；不证明完整 query/validate 或 Mongo 结果 |
| T3 | `service::scope::tests::later_pages_require_the_same_authorization_version`；列表 filter、排序及最新决定纯函数测试 | 分页 scope_version、业务筛选结构、排序、最新决定选取/去重；不证明完整授权过滤的实际结果集 |
| T4 | `service::evidence::authority_sequence_tests::*`，`service::task_decision::action::tests::*` | 空证据先拒绝、首错停止、证据顺序与原 executor、查询之后发现证据；不证明授权资格或真实写入原子性 |
| T5 | 其余 DTO、实体状态机、收据、索引定义、序列化和凭据 CAS filter 单测 | 各自测试覆盖的规则；不是权限范围验收的替代物 |

开发和 Review 验收清单：

- INT-AUTH-01：差异根分页、计数、当前页最新决定/响应补齐均在原凭证有效阶段内；最后一次异步读取完成后才 validate；验证错误没有页结果交付。成功页 items/total/状态/版本保持原语义。
- 两类列表：缺 Mapping、身份取证故障和不支持的入口均失败关闭，不执行业务查询；有资格但空范围保留 identity 空结果条件，不降级全量查询。后续页缺/错 scope_version 返回既有 DATA_SCOPE_CHANGED。
- Facts/Mapping：真实 owner/org 的对象判断与列表成员条件一致；Owner/Organization 未读取不能由其他允许分支掩盖；两类资源无协作/历史/来源补权；未知维度/资源/绑定拒绝。
- 仓储：凭证资源/集合/输出错配以及跨 executor/阶段、写入、事务重启、到期后的复用都不能读取；分页和计数使用同一授权过滤。通用边界由 identity/persistence 单测覆盖时，必须引用其准确已验证版本，不能假称本 crate 已覆盖。
- 精确差异决定：当前 owner/版本允许，其他处理人、缺记录/事实、错版本拒绝；决定序号冲突依然由业务处理。正式 WorkItem 的精确授权由原上层负责，不能换成集成列表可见性。
- 非事务：本域公开列表继续使用快照事务；不因静态 Mapping 而开启没有可靠身份/范围修订复验的非事务旁路。此任务没有新增动态 Mapping 依赖，动态准备/来源份额/导出测试不适用。
- INT-AUTH-03：没有业务侧重复动作白名单或临时服务工厂；有公开 API 变化就记录调用方与兼容性。
- 在允许范围内运行同一条 `cargo test -p erp-integration --lib --locked` 命令并记录结果；通过后不重复 cargo check。不新增、修改、执行集成测试、examples、真实 MongoDB/S3/外部服务测试，也不运行 server/其他 crate 的全量验证。

本轮没有运行其他 crate 测试、集成测试、examples、server 全量编译、真实数据库或外部服务验证；没有把静态事实绑定核对写成完整服务行为通过。分析环节可交付，无环境阻塞；上述代码修复和证据补齐由开发继续，独立审查明确通过后才能置 done。
