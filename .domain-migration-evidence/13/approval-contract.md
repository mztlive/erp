# 阶段 13：审批装配分片执行与验收合同

## 1. 输入、范围与证据

- 实施输入为 `de112614e03714bb6d1ca4ac9e25f580c5c273c8`；唯一工作树为 `/private/tmp/erp-domain-crate-13-returns`。
- 本分片迁移旧 `services/src/returns/{mod.rs,version_conflict.rs,adapter.rs,adapter/**,start_approval.rs,start_approval/**,cancel_approval.rs}`，共 15 个源文件。已删除全部已迁旧叶；没有修改历史 `tests/**`、运行 MongoDB 或提交代码。
- [逐符号与源哈希证据](/private/tmp/returns13-approval-evidence.json) 核销 110 个原生产函数；94 个函数体去除空白、注释并校正 owner 路径后相同。其余 16 项为构造路径显式化、领域错误转换、本域 CAS 委托、真实授权/取消 Port 抽取；差异见 [函数体对照](/private/tmp/returns13-approval-body-diffs.txt)。
- 原 40 个内联测试入口保留；其中四个只读审批展示测试随唯一 View 实现迁入 read-models。本分片新增 7 个使用非零大小 Executor 的真实 Port 替身测试。测试是否通过须引用集成负责人的实际日志，源入口存在不等于执行通过。
- 定向 rustfmt 与 git diff --check 已执行，退出 0。本分片未运行 Cargo；集成负责人已报告 workspace check 第 2 轮退出 0，随后四个测试专用导入警告已按实际 cfg(test) 使用修正。

## 2. 冻结公开接口与注册

| 所有者 | 实际公开接口 | 合同 |
| --- | --- | --- |
| `erp_returns::service` | `ReturnsService::new(Database)` | 只存本域 Database，不持 RBAC、object-read 或 workflow。 |
| `erp_returns::service::shared` | `return_command_no`、`ensure_posted_source` | 稳定编号保持 SHA256(actor + `|` + trim(key)) 前 8 位；来源版本冲突先于未过账业务错误。 |
| `erp_returns::service::version_conflict` | `conflict_if_stale_version(bool)` | 原实体版本比较结果映射原 ConflictError 与重试文字。 |
| `erp_returns::service::approval` | 四类 `start_*_approval`、四类 `cancel_*_to_draft`、客户 `ensure_final_approve_posting` 及其余三类 `ensure_*_final_approve_posting` | 原 12 个纯状态/守卫函数唯一实现；不接 workflow 动作枚举。 |
| `erp_processes::reverse_flow` | `ReturnsProcess::new(Database)`、`with_object_read`、`with_rbac`；叶使用 `domain()` / `reads()` | `new` 保留原 shared RBAC 与 FailClosed 默认；对象读取注入保留；不提前加载业务数据。 |
| `erp_processes::reverse_flow` | 四类 `*_object_readable` | 根逐项 pub use 原函数与签名；不公开整个 adapter 模块。 |
| `erp_processes::reverse_flow` | `finalize_approved_return_in_transaction`、`cancel_approval_in_transaction` | 审批 dispatch 直接使用；最终退款 callback 接原 ClientSession，取消 callback 接原 Executor，不调用 standalone 根。 |
| `erp_processes::reverse_flow` | `PaymentReversalProcess`、`ReceiptReversalProcess` | 付款实现由 D 提取到 payment_posting 后根重新导出；回款实现继续保留原 receipt_reversal 及其命令子模块。 |
| `erp_read_models::returns_center` | `ReturnsReadService`、审批 View、四个 allowed-actions、definition projection、`RECENT_HISTORY_LIMIT=8` | 由 C/G 唯一持有。Process 原三处丢弃读取的展示常量项已删除，仅保原 start-command-kind 调用；没有第二份常量或哑消费。 |

领域 service 根登记 customer_refund、supplier_refund、receipt_reversal、payment_reversal、sales_return、purchase_return 与三个纯 helper 叶。Process 根登记六类命令、两个既有最终冲正过程、adapter、start、cancel 与实际取消写入 Provider。根接口保持 HTTP 原对象读取注入链。

## 3. 审批适配与本域状态合同

- 四类 adapter 规格、subject ref、冻结 binding、StartCommand、command kind、对象读取、责任组织与快照构造实际实现迁入 `reverse_flow::adapter`，没有 re-export 旧 services 实现。
- `ApprovalDomainAction` 分派继续留 Process：仅本类型 Post/Cancel 两个原动作分支，错误类型与原文字不变；调用唯一领域 guard/cancel 后显式转换为 services 对应错误，不按字符串改分类。
- Start/Cancel/最终通过 guard 原函数体保留。客户与供应商最终准备中的第一次 guard、Process 签署动作的第二次 guard 继续分别在原位置，不能合并。
- 领域版本冲突、稳定命令号和已过账来源规则均不接 workflow 或身份完整聚合。四种本域 CAS 由各业务叶提供的事务内方法执行，使用传入 Executor；客户方法为实例 `persist_customer_refund`，其余三类为 associated `persist_*`，没有改变仓储更新语义。
- 四类 Snapshot 的客户/供应商/party、金额、单号、responsible organization、actor、submitted_at、line_count 与无数量字段保持原值与构造顺序。

## 4. 普通 submit 与一体 commit 不同写序

### 4.1 普通 submit

四个 `persist_*_start` 保留原 PreparedExecution 分支。Replay 直接返回已准备的本域对象，不写运行事实。Apply 的原根事务顺序为：

1. `insert_command_receipt`，保留 `map_receipt_first_write_error`。
2. `business_documents.mark_approval_started`，使用原对象、DocumentType、冻结 definition ID/version、now 与同一 session；未命中返回各类型原 ConflictError。
3. 原 `persist_*_runtime`：入口执行存在性检查 → `create_bpm_runtime_after_receipt` → 原 `next_id()` 创建不可变 subject snapshot → 保存 snapshot → 顺序遍历 HumanTaskRequested，原 `next_id()` / WorkItem 构造 / create。
4. 本域单据 CAS。唯一变化是原仓储 update 由领域事务内方法原位承接。
5. submit 审计 create，然后返回该本域对象。

`start_approval::mapping::list_projection_from_execution` 是实际 BPM runtime 持久化投影，继续留 Process。其原测试保留，不能因名称含 list 将其移入页面读模型。

### 4.2 一体 commit 消费方

B/D 的四个 commit 继续使用同一套原 runtime writer，保留来源事务复验、绑定/document、graph、prepare_start、业务 create、runtime、三项审计的原插入位置。本分片没有把普通 submit 的 receipt-first 流程替换到一体 commit，也没有增加 commit 根不存在的 StartApproval 回执仲裁。

普通退款前置 replay 与八次 fresh recovery 由 B/D 命令叶持有；本分片共享的实际授权和精确回执读路径在第 5 节固定。回款/付款冲正普通 submit 没有新增退款专用恢复分支。

## 5. fresh replay 的真实授权和精确回执

`start_approval::prepare::{ensure_return_start_actor_active,ensure_return_start_replay_authorized}` 实际调用 `authorization::MongoReplayAuthorization`。Provider 已逐方法展开：

| 生产调用 | 原提供方 | 顺序与失败合同 |
| --- | --- | --- |
| `actor_active` | `approval_actor_is_active_with_executor`，原 `services::workflow_compose::workflow_auth(db.clone(),rbac.clone())` | 当前 actor 与原 Executor；false 返回“当前账号不可提交该退款或冲正单”，不读后续范围。 |
| `action_scope` | `approval_document_action_scope_with_executor` | 原 actor、submit permission 与同一 Executor；错误直接返回。 |
| `read_scope` | `approval_document_read_scope_with_executor` | 原 actor、具体 DocumentType 与同一 Executor；错误直接返回。 |
| 范围合取 | 原 `ApprovalManagementScope::covers` | 两项读取完成后才检查 action/read 均覆盖 organization，保留“无权提交该责任组织的退款或冲正单”。 |

Provider 构造只保存 db/rbac 引用；每一步仍在原位置构造 workflow auth。调用方在业务资源读取前的首次 active 检查和 replay-authorized 内再次 active 检查均保留，不进行缓存或去重。

`replay_return_start_with_executor` 原函数体保持：normalize key → process kind → 完整 start identity（subject/type/version、binding ID/version、actor）→ V3/legacy scope 候选依次读回执 → Fresh/PayloadConflict/SamePayload → 读取 result instance → 校验实例 ID、process kind、subject kind/id/version、started_by、definition ID/version → 返回实例 ID。不能只凭 key 或单一 scope 命中成功。

`replay_subject_versions` 保留 current>0 时先 current，再 checked_add(1)，溢出原 ConflictError。当前客户与供应商真实调用均保持先 active → 资源/组织 → authorized → binding → subject/version candidates → exact receipt，并使用 fresh with_transaction。两条 bounded recovery 均保留 8 次、原 command_may_have_committed 分类、间隔和最后原错误；客户内联 fresh 根与供应商 helper 结构分别保留。

新增四个授权测试调用同一生产 runner，验证非零大小 Executor 身份、参数传递、三步错误停止、账号失败的首错，以及 action/read 各自不覆盖时仍保持原双读取顺序。

## 6. Cancel Apply/Replay 的真实写入合同

原 `load_cancel_runtime` 保留 instance → cancellation policy → current execution → open tasks → 任务数量 → bound graph 顺序；原 RUNNING/BLOCKED 数量合同不变。四个取消输入 builder 的 ID、key、reason、now、receipt 与 prepare-cancel 规则函数体保持。

四个 `persist_*_cancel` 根均实际构造 `cancel_write::MongoCancelWrite` 并调用同一个生产 runner；没有复制只供测试调用的步骤表。

| 顺序 | Apply 实际操作 | Replay 实际操作 |
| --- | --- | --- |
| 1 | `WorkItem::close_all_for_approval_cancellation`：按原顺序逐任务形成关闭事实，任一失败不写后续步骤。 | 跳过。 |
| 2 | 原 `bpm_workflow.persist_cancelled_runtime`。展开仍为先实例 CAS/逐执行 CAS，再插入原命令回执；没有改用 receipt-first 变体。 | 跳过。 |
| 3 | 原 `work_items.persist_cancelled_approval_tasks`，最终逐任务执行原 OPEN/版本/执行引用 CAS。 | 跳过。 |
| 4 | 对应领域 `persist_*` 执行原本域单据 CAS。 | 仍执行原本域单据 CAS。 |
| 5 | 原 audit create。 | 仍执行原 audit create。 |

`MongoCancelWrite` 中四种 record variant 逐项显式调用对应领域方法；传入的是同一借用记录和 Executor。新增空 `closed_tasks` 只存放原关闭规则输出；没有新增读取、ID、clock 或业务错误。每次 await 均以 `?` 停止，错误保留原类别。

新增三个取消测试通过生产 runner 验证 Apply 与 Replay 轨迹、同一非零大小 Executor，以及两分支每个原写入步骤失败时没有后续调用。它们不代替真实 MongoDB 回滚验证。

## 7. 审批 callback 与错误传播

- `finalize_approved_return_in_transaction` 保留原 CustomerRefund/SupplierRefund 两分派；回款与付款冲正仍由各自既有 Process callback 执行，不扩展本函数原分支。
- `cancel_approval_in_transaction` 保留具体 DocumentType → 本域读取/原 NotFound → 本类型签署动作 → 本域 CAS → `returns.cancel_approval` 统一审计。外部运行时传入的 Executor 不被替换，不另开事务。
- `erp_returns::Error` 经 services 显式 From 对应映射，Conflict/Logic/Repository/OutcomeUnknown 等类别保留；领域方法不吞错或重新拼装 provider 错误。
- 没有新增外部 I/O、发信或数据库迁移；没有将审批状态、RBAC、BPM、WorkItem 或 AuditLog 放入 returns 领域。

## 8. 集成核销要求

1. 以固定阶段输出提交重新比对 JSON 中源哈希；有变化须先核对真实函数/Provider，再封存 source_commit，不能只替换哈希。
2. 原 40 个测试、7 个新增 Port 测试与四类最终 post 拒绝合同的实际运行结果分别由集成日志记录；本文件不声明尚未读取日志的门禁通过。
3. 本分片 110 个函数的删除/目标与新 Provider 均已登记；B/D 最终金融写入、C/G View/DTO、A 实体/仓储及全局 HTTP/Cargo 由相应分片独立核销。
4. 固定记录“真实数据库运行未验证”；不得把编译、静态比较或替身轨迹称为真实事务回滚、并发或未知提交验证。
