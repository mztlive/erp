# 阶段 10 语义复核记录

## 1. 输入、结论与证据边界

- Before：`/private/tmp/erp-domain-crate-09-finance`，HEAD `abf36f41d741dc243f689c877af44f6196eff62b`。
- After：源码已固定于实现提交 `07da7863ec0fb975d2093e4e97a43c7e0813e607`；工作树 `/private/tmp/erp-domain-crate-10-sales`。
- 本次只读源码和差异，输出本记录；未修改仓库源码，未运行 Cargo，未执行数据库或外部服务测试。**真实数据库运行未验证**。
- 复核完成后 After 的生产代码已冻结。本次已纳入复核期间新增的 `formalization_posting.rs`、真实 `MongoPosting`、最终 `progress.rs::refresh_money_progress/DatabaseProgressStore` 抽取和非零执行器测试修正；After HEAD 仍为上述输入提交，后续生产文件修改须重新核销对应条目。
- 本轮在下列逐符号范围内未发现需要阻止迁移的生产行为差异。发现的 1 项 Executor 测试证据缺口已完成源码修正复核；对应运行结果仍由 root 门禁登记。
- `sales_review` 为本人实现，单列为“实现自审”，不得视为独立交叉审查。
- 根静态报告 `missing-drift-report.json` 当前为 `captured_contracts_equal=true`、changed/missing/added 均为 0、needs_review 为 48。其有界词法扫描不证明运行时事务、ID 时点、失败优先级；本记录补充本片生产调用链对照，不替代全仓 needs_review 核销。
- root 报告 workspace check 已退出 0；本轮未自行执行或复核 Cargo 输出，不把该消息回报的门禁结果用作本记录的运行证据。

## 2. 发现及关闭要求

### F-01：正式化流程的执行器身份测试（源码修正已核销，运行待验证）

- 分类：测试证据缺口；未确认生产行为漂移。
- 位置：After `backend/crates/erp-processes/src/order_to_cash/formalization_posting.rs`，`formalization_preserves_all_domain_steps_and_executor`、`every_failure_stops_later_domains_and_preserves_original_error`。
- 初次复核发现：测试以 `NoTransaction` 地址建立预期身份；`NoTransaction` 为零大小类型。不同实例允许共享地址，因此该断言不能可靠排除流程误换另一个 `NoTransaction` 的回归。
- 生产核对：`post` → `execute` → `MongoPosting::apply` 及其 sales/finance/procurement/workflow 方法均接收同一传入 `&mut dyn Executor`，未发现私建执行器或内层事务。
- 修正复核：分片负责人已改为 `TestExecutor { _identity: u8 }`，`Executor::session()` 返回 None；两个测试逐步断言传入 trait object 的 data pointer 与该非零实例地址相等。现有 10 步顺序、各步失败短路及原错误分类断言均保留，生产流程未因该修正改变。
- 状态：源码证据缺口关闭；本轮只读，不登记测试执行通过，由 root 统一运行门禁补齐运行结果。

## 3. 源路径约定

下列路径相对各工作树的 `backend/`：

| 简写 | 路径 |
| --- | --- |
| BSO | Before `services/src/sales_order/` |
| BAP | Before `services/src/fulfillment/customer_acceptance_posting.rs` |
| BRR | Before `services/src/returns/receipt_reversal.rs` |
| ASO | After `crates/erp-sales/src/service/sales_order/` |
| AOC | After `crates/erp-processes/src/order_to_cash/` |
| AAP | After `crates/erp-processes/src/fulfillment_execution/customer_acceptance/` |
| ARS | After `services/src/fulfillment/customer_acceptance_posting.rs` |
| ARR | After `crates/erp-processes/src/reverse_flow/receipt_reversal.rs` |
| AFR | After `crates/erp-finance/src/service/receivable/receipt_reversal.rs` |

## 4. sales_order 命令与准备符号

| Before 符号 | After 符号 | 具体对照与结论 |
| --- | --- | --- |
| BSO `command/create.rs::sales_command_customer_id` | AOC 同名方法 | 仍先读取合同并由合同确定客户身份，供 HTTP 客户数据范围校验；未改成从客户端 customer_id 取权威身份。其主体实现仅位置和调用类型变化。 |
| BSO `command/create.rs::resolve_sales_command_draft` | AOC 同名方法 | 合同/合同修订/客户/结算主体解析顺序与草稿映射未改；current contract 查询留 Process，销售构造收到的仍是相同冻结字段。 |
| BSO `command/create.rs::create_sales_order` 前置段 | AOC 同名方法、ASO `lifecycle.rs::prepare_order` | Validate → trim 幂等键/拒空 → 规范键写回请求 → 稳定审计 ID/完整请求 fingerprint → 已提交审计回放 → 合同草稿解析 → 精确 SKU 资格 → 客户存在/active → 分配销售单 ID/构造。`prepare_order` 在原构造位置生成同一类别 ID，`origin_system=Erp`、source fields=None、contract/customer/party 原值保留。 |
| BSO `create_sales_order` Draft 分支 | AOC 同名方法 + ASO `create_order/create_working_copy` | 根事务原序保留：精确 SKU 事务内复验 → sales order.create → 发布定义绑定并 BusinessDocument.create → 新稳定行逐条 create → working copy.create → copy lines 逐条 create → create audit。`create_working_copy` 实现已展开核对，并非仅凭外层名称判断。 |
| BSO `create_sales_order` Submit 分支 | AOC 同名方法 + ASO `create_order/create_working_copy/create_submission` | 仍在事务外构造工作副本、冻结提交、WorkflowAction、now 与创建/提交审计；事务内 SKU 复验 → 绑定登记 → 图/启动计划 → order → stable lines → copy → copy lines → submission → submission lines → WorkflowAction → 原 runtime 写入 → create audit → submit audit。不得将该分支描述为新增 receipt-first；原运行事实持久化位置保留。 |
| BSO `command/create.rs::replay_sales_order_creation` | AOC 同名方法 | 同一 audit_id 查询；核对 action/resource_type/actor → fingerprint → result resource ID → 销售存在性/created_by。payload mismatch 仍 Conflict，收据结构/主体不一致仍 Internal。任意建单事务错误后的原回读分支仍存在，无额外 retry loop。 |
| BSO `command/identity.rs::sales_submission_audit_id` | ASO `command/identity.rs` 同名 | 哈希输入、分隔与稳定前缀不变；可见性变化未改变摘要格式。 |
| BSO `command/identity.rs::sales_submission_fingerprint` | ASO 同名 | 完整 actor/order/request tuple 及 serde_json → SHA256 计算未改；序列化错误文案与分类保留。 |
| BSO `command/identity.rs::sales_order_create_audit_id` | ASO 同名 | actor 和规范化 key 的原哈希输入和前缀保留。 |
| BSO `command/identity.rs::sales_order_create_fingerprint` | ASO 同名 | 原泛型 Serialize 输入仍是调用方同一请求值；未删字段、未换字段次序或规范化策略。 |
| BSO `command/identity.rs::sales_create_bind_command` | AOC 同名 | GoodsService/Voucher 对应原 DocumentType；business object ID/version、组织、creator 均一对一。 |
| BSO `command/identity.rs::persist_bound_sales_document` | AOC 同名 | 本地对象读取守卫 → 原 workflow_compose 真实 auth/audit adapter → 发布定义绑定 → 缺绑定 Internal → attach binding → BusinessDocument.create；使用传入 ClientSession。 |
| BSO `command/save.rs::save_working_copy` | AOC 同名 + ASO `prepare_saved_working_copy` | 先请求/合同/销售可编辑与归属校验、SKU 资格，再 load/reopen。已有副本保留 `header_snapshot` 在 `build_working_copy_lines` 之前、随后金额/next_version/update/save_draft 的顺序；旧行读取和审计创建仍位于纯准备之后。 |
| BSO `save_working_copy` 事务段 | ASO `lifecycle.rs::persist_saved_working_copy` | 事务内 SKU 复验 → stable additions → old rows 逐条 soft_delete → new rows 逐条 create → copy CAS update → Process audit。未改为批量预写，后继步骤遇错终止。 |
| BSO `draft_working_copy.rs::collect_stable_lines_for_draft` | ASO 同名 | 函数体归一化空白后相同：读现有稳定行，按 draft 行序复用 line_no，缺行才 next_id/new，创建队列顺序不变。仍 NoTransaction。 |
| BSO `build_reopened_first_submission_working_copy` | ASO 同名 | 函数体归一化空白后相同：仍调用 build_working_copy(..., 1, actor)，不回写 Submitted 历史副本。 |
| BSO `load_or_reopen_first_submission_working_copy` | AOC 同名 | 可编辑守卫 → collect stable（可能分配行 ID）→ 查 active copy → 存在则校验请求版本；无 active 则新建副本并执行自身根事务。保存调用方遇 opened_new 仍直接返回视图，不再次覆盖保存。 |
| BSO `persist_reopened_first_submission_working_copy` | AOC 同名 + ASO `create_working_copy` | 原 save_draft 审计在事务前构造；事务内 SKU 复验 → 新稳定行 → 新 copy → copy lines → audit；原旧 Submitted 副本保持。 |
| BSO `command/submit.rs::submit_sales_order` 前置段 | AOC 同名 | Validate/trim key → audit ID/fingerprint → 稳定审计回放仍早于合同、销售状态和草稿版本读取。随后合同解析 → order editable/归属 → draft SKU → active copy → stable lines 保持原序。此处与 sales_review 提交的前置优先级不同，不得机械统一。 |
| BSO submit active-copy/None 分支 | ASO `lifecycle.rs::prepare_submission_copy` | active 分支先 version，再 old copy lines，再新行 ID/金额，再含 `header_snapshot` 的 update/save_draft；None 分支构造新首次提交副本。保存路径 snapshot-first 与提交路径 line-build-first 的原区别均保留。 |
| BSO submit 构造/复用冻结提交 | AOC 同名方法 | 原 working-copy-version 对应 submission 查询/直接返回仍在副本准备之后、第二次 SKU 和采购责任检查之前。新增提交号仍 latest+实体 next_submission_no，随后提交头/行、copy.submit、启动组合。未在迁移中提前或删除该复用分支。 |
| BSO submit `start_approval_submission` | AOC 同名方法 | document ports/subject → frozen binding → sales state start → now/snapshot/start input → graph/receipt/prepare_start → WorkflowAction → stable submission audit → SKU refs 的 NoTransaction 复验 → root persist。完整冻结身份与调用上下文保留。 |
| BSO submit `replay_sales_submission` | AOC 同名方法 | 同一稳定 audit ID、actor、action、resource_type、fingerprint、销售归属、submission ID/行读取与视图组装保留；未将顶层 audit receipt 等同于 BPM receipt。 |
| BSO submit `recover_sales_submission_start` | AOC 同名方法 | 仅 command_may_have_committed 进入；8 次，每次 fresh transaction，重读 order/type/org/object-readable/binding，再对完整 start identity 与 instance 回放。成功后仍回读稳定提交审计获取视图；可恢复错误/无结果退避，非恢复错误即返，耗尽返 original_error。 |
| BSO `command/cancel.rs::cancel_approval_submission` | AOC 同名 + ASO `lifecycle::cancel_sales_order_to_draft` | 请求/销售版本 → ports/binding/subject/latest submission no → runtime task policy → now/key/prepare_cancel → domain cancel → audit → 事务。无新增提前回执查找；传入 receipt 语义保持原值。 |
| BSO `cancel_approval.rs::persist_sales_order_cancel` | AOC 同名 + ASO `persist_order` | Replay 无写；Apply close_all 后根事务 claim_and_persist_document_cancel_runtime → order CAS → audit。回调入口仍复用外部事务，没有把 HTTP 根事务套进审批事务。 |
| BSO `command/void.rs::void_sales_order` | AOC 同名 + ASO `lifecycle::persist_void` | Validate/版本/草稿作废 → 读取并 abandon 可选首次工作副本 → 审计构造 → 根事务 order CAS → 可选 copy CAS → audit。未新增已生效单作废旁路。 |

### 4.1 构造与读取适配器的复核

| 原符号/能力 | 迁移后真实实现 | 对照结论 |
| --- | --- | --- |
| BSO mapper `build_stable_lines/build_working_copy/build_working_copy_lines/header_snapshot/build_submission/build_submission_lines` | ASO `mapper.rs` 同名 | 全文件差异仅 crate 路径、DTO 导入、可见性和错误模块路径；构造体、ID 分配位置和业务表达式未改。正式快照不回填当前主数据。 |
| BSO sellable `ensure_sellable_draft_lines/ensure_sellable_working_copy_lines/sellable_working_copy_refs/ensure_sellable_refs` | ASO `sellable.rs` + AOC `CatalogQualificationAdapter::qualified_refs` | 空引用直接成功；非空先 HashSet，再原位置 BusinessDate::today；adapter 原样调用 catalog.find_sellable_sku_refs(refs,date,同executor)，只映射 sku/revision pair。invalid 集合仍排序，错误仍 BusinessLogicError，文案与 erp_catalog::sellable_sku_invalid_error 逐字一致。 |
| BSO procurement `working_copy_inputs/submission_procurement_inputs` | ASO `procurement.rs` + AOC `resolution_input` | 非 GoodsService 行 Conflict、缺 SKU Validation 的优先级保留；line_key 是稳定 sales_order_line_id，SKU/service_region 一对一；无新增 catalog 查询或默认责任。 |
| BSO `ensure_procurement_responsibility_before_submit` | AOC 同名 | 非 GoodsService 原样跳过；GoodsService 仍用旧真实 ProcurementResponsibilityService::resolve_strict，RBAC 依赖在 Process 持有。 |

## 5. 首次正式化与金额进度

| Before 符号 | After 符号 | 具体对照与结论 |
| --- | --- | --- |
| BSO `formalize.rs::prepare_approved_submission` | AOC 同名 | 使用调用方 Executor 读取 order；fully_formalized 仍先返回 None。其后 final-approve 状态、最新提交/行、采购责任计划、require_rbac、纯正式化准备顺序保留。 |
| BSO `load_latest_submission` | ASO `formalize.rs::load_latest_submission` | find_latest_by_order → 缺提交 Conflict → list_lines_by_submissions，同 executor 和查询参数不变。 |
| BSO `build_procurement_formalization_plan` | AOC 同名 | Voucher 返回 None；GoodsService 的 line facts → resolve_strict 保持原流程，未将授权查询塞入销售域。 |
| BSO `allocate_formal_revision_identities/build_revision_for_order` | ASO 同名 | 先版本 ID，再逐提交行公共 ID/子类型 ID；version=1、ErpApproval、parent/business_type/now 原值。原 factory 和错误 Logic 映射不变。 |
| BSO `prepare_formalized_submission_write` | AOC 同名 + ASO `approve_submission` | now → sales aggregate → 按责任人构造采购 WorkItem（含其 ID）→ order.approve → attach revision → submission.approve。未把状态校验或 ID 生成提前至采购解析之前。 |
| BSO `build_procurement_work_items/persist_procurement_work_items` | AOC 同名 | BTreeMap 按 owner 分组，line IDs 排序去重，原 responsibility key、role/org/subject_version、reason/priority/due_at 保留。真实仓储侧开放任务查重和责任范围漂移校验保留；owner eligibility 在前置 resolver/revalidate 步骤复验，不归因于该仓储 helper。 |
| Before Process `order_to_cash::formalize_approved_submission` | AOC `formalization_root.rs` 同名 | 有采购 policy_revision 时仍 run_authorized_policy_transaction；无采购则普通根事务。已形式化无再次写入，最后只读取详情。 |
| Before Process `formalize_approved_submission_in_transaction` | AOC 同名 | 仍接审批运行时 ClientSession；未自行起事务或切 NoTransaction 写入。 |
| BSO `persist_formalized_submission_write` + Before Process `persist_formalized_submission/create_original_receivable` | AOC `formalization_posting.rs::{post,execute,MongoPosting::apply}` | 真实 10 步：采购 revalidate → procurement WorkItems → 查可选 BusinessDocument 并 formalize/update → sales revision → procurement task sync → sales submission update → finance initial account/entry → initial funds task → invoice task → audit。前三个采购相关步骤原条件保留；sales 当前版本仍先于 task sync。 |
| 新 ASO `formalize::persist_revision/persist_submission` | 上述 MongoPosting 的 SalesRevision/SalesSubmission 分支 | 内层分别原样调用 sales_order.formalize_submission 与 sales_order_submissions.update；不生成 ID、不开事务、错误即返。 |
| finance `initial_account::create_initial_receivable` | MongoPosting Receivable 分支直接调用原接口 | phase09 财务构造/写入实现未改；消费事实逐字段一对一。账户 ID → 分录 ID → 账户构造 → today → 分录构造及仓储写序保持。两个后续任务复用本次已写入 account。 |
| BSO `progress::update_sales_order_money_progress` | AOC progress → ASO progress → `FinanceMoneyProgressAdapter` | 必须先读取销售存在性，再请求余额；生产 adapter 在构造时不读库。仍逐账户输入 collection/invoice projection，再原位置 Instant::now()/refresh_progress；无变化不写；有变化 order CAS。 |
| BSO progress 内联读取/刷新/写入 | ASO `progress::{refresh_money_progress,DatabaseProgressStore::{load,update}}` | 最终 helper 已展开复核：生产调用传入 `Instant::now` 函数值，只有销售读取与财务读取成功后才调用 `now()`；不存在/读取失败不取时、不写回。真实 store.load 是原 `sales_orders().find_by_id`，store.update 是原 `sales_orders().update`，均直接使用调用方 executor；无变化仍已经取时但不 CAS，与旧实现一致。 |
| BSO progress 仓储错误直返 | ASO error → services error | finance facts 仓储方法继续返回 persistence_core::Result；销售 store 的乐观锁冲突/瞬态事务/未知提交转换与 services 原分类一致，Process 再逐变体映射至原 services::Error；未统一转成 Internal。 |
| 新 progress 内联替身 | ASO `Fixture/RecordingExecutor` | `RecordingExecutor { visits: usize }` 为非零类型，真实 helper 由同时实现 sales store/finance port 的 Fixture 消费；断言 sales.read → money.read → clock → sales.write、三次相同地址及访问数，并覆盖缺单/两次读失败/写失败/无变化/余额组合。这里只确认测试结构与生产调用关联，不登记运行通过。 |
| finance 原 `list_by_sales_order` | 新 `ReceivableAccountRepository::money_progress_facts` | 新事实读取内部继续调用原 list_by_sales_order(id,executor)，无聚合、去零、额外 review-status filter 或排序变化。四个金额字段一对一；空列表与全零账户列表仍有区别。 |
| sales `refresh_progress` / 进度枚举 | 迁移实体同一实现 | 本次调用没有改变业务规则：关闭依赖履约完成与应收结清，开票完成不加入关闭条件；fulfillment=None 不修改已有履约进度。 |

执行器证据：上表逐级检查真实 MongoPosting 与 provider adapter，未只依赖 `execute` 的替身测试。F-01 的非零执行器替身修正已复读确认；该防回归测试的实际执行结果仍待 root 门禁登记。

## 6. customer_acceptance 三条根入口

| Before 符号 | After 符号 | 具体对照与结论 |
| --- | --- | --- |
| BAP `commit_customer_acceptance` 前置/幂等 | AAP `commit.rs` 同名 | Validate → task ID/version 成对 → draft ID/version 成对 → CommandReceipt::from_payload → committed_resource_id 回放。新单编号仍在事务前读取；随后 acceptance ID，再 CustomerAcceptanceLineBatch 行/ID；未在回放前分配新业务 ID。 |
| BAP commit existing 读取/守卫 | ARS `load_customer_acceptance_commit_draft` | 只对请求有草稿 ID 时读取；存在性 → sales ownership → expected version/ensure_draft_version。随后才在 Process 读取销售并检查 expected_sales_order_version，再准备责任任务，保持原首错顺序。 |
| BAP commit 创建/替换表头 | ARS `prepare_customer_acceptance_commit/persist_customer_acceptance_commit` | 原任务成功后才 update/new 表头；new 分支仍先 register_created_customer_acceptance_document，再 create head+lines；existing 分支 head CAS → replace lines。生成编号缺失 Internal 与实体校验先后不变。 |
| BAP commit 逐行分配 | ARS `persist_customer_acceptance_commit` | 依 final_lines 顺序找输入行 → 缺行/空分配 → ensure_allocation_conserved → 每条 write_acceptance_allocation；仍穿插查询、校验、next_id 和逐条写入，没有先生成全部 APPLY。最后 mark_posted → head CAS。 |
| BAP commit 完成段 | AAP `CompletionKind::Commit` → `DatabaseCompletion` | 履约进度读取/销售刷新 → task after posting → command receipt audit。未额外插 business audit；失败仍整个根事务返错。 |
| BAP commit error recovery | AAP commit error 分支 | 任意事务错误后仍读取 committed_resource_id，存在则复用原已提交详情和剩余资格，缺收据返原错误；未替换成仅某错误类恢复或增加新事务重跑。 |
| BAP `post_customer_acceptance` | AAP `post.rs` + ARS `load_customer_acceptance_for_post/persist_customer_acceptance_post` | Validate/task pair → 根事务 acceptance 存在性/ensure_draft → task → stored lines → ensure_posting_lines → ensure_post_lines_match → 逐行守恒/APPLY → mark_posted/CAS → progress → task → business audit。普通 post 仍没有命令回执回放，重复 post 仍状态冲突。 |
| BAP `reverse_customer_acceptance` 前置/恢复 | AAP `reverse.rs` 同名 | Validate → from_resource_parts(id,key,expected_version,reason) → receipt 回放；事务结果错误后仍用同一 receipt 查结果。未改为接受旧 version 的非回放重做。 |
| BAP reverse 原事实读取 | ARS `persist_customer_acceptance_reverse` | 原 head → ensure_reversible(expected_version) → 原 lines → 原 allocations → ensure_reversible_source；只有上述成功才生成反向 head ID/now。 |
| BAP reverse ID 与写入 | ARS 同名 | 原 reverse number=REV-原号，result=Rejected；先 reverse head ID，再按旧 lines 顺序各 line ID，再按原 allocations 顺序各 reverse allocation ID。写 head+lines → reverse allocation rows → reverse head mark_posted/CAS → original.reverse/CAS 保留。 |
| BAP reverse 完成段 | AAP `CompletionKind::Reverse` → `DatabaseCompletion` | 反向单 sales_order_id 来源原单，故以反向单传入 refresh 的 ID 与旧 original ID 相同。progress → 若 remaining 则 ReopenedByReversal task → business reverse audit 引用原单 → command receipt 引用反向单；两个审计顺序和资源 ID 保留。 |
| BAP `write_acceptance_allocation` | ARS 同名 | 归一化空白后函数体相同：履约事实分类/归属/数量读校验、net applied 上限、allocation ID 和 create 的穿插位置相同。没有只检查外层分配循环。 |
| BAP `load_fulfillment_fact` | ARS 同名 | 归一化空白后函数体相同；Delivery/Electronic/Service 的权威事实读取与归属/净数量校验保持。 |
| BAP `ensure_existing_acceptance_draft/ensure_post_lines_match/ensure_task_context_pair` | ARS 同名及 public 本域入口 | 三项原私有规则归一化空白后函数体相同；外层调用位置与前述表格一致。 |
| BAP `update_sales_order_fulfillment_progress` 读取段 | ARS `load_customer_acceptance_progress` | order → GoodsService gate → current revision/lines/goods subtype → delivery/electronic/service facts及各 allocations → build_line_eligibilities → AcceptanceProgress::derive，原顺序保留。原返回 false 的无投影情形现为 None。 |
| BAP 进度写入段 | AAP `apply_projection/SalesProgressWriter::write` | None 直接 false，**连 finance balances 读取也不执行**；Some 时原 fulfillment enum → AOC money progress → remaining 标记。两层销售读取（投影读与刷新读）均保留且仍同 executor。 |
| BAP 过账/冲正后的 task/audit | AAP `finish/DatabaseCompletion::{refresh_sales,persist_task,business_audit,command_receipt}` | 已展开核对真实 adapter。Commit/Post 消费先前准备 task，Reverse 仅剩余可验收时重开；任务/审计不在独立事务或提交后执行。 |
| 原 HTTP 履约 service 构造 | After handler `acceptance_process` / AAP `CustomerAcceptanceProcess::new` | read 字段仍使用原 `service(state)`，保留 secret、sensitive_data 与 object_read 配置；根 Process 同时收到 state db/rbac/object_read。未以空配置另建读取服务。 |

## 7. receipt_reversal 财务拆分

| Before 符号 | After 符号 | 具体对照与结论 |
| --- | --- | --- |
| BRR `post_receipt_reversal` | ARR `ReceiptReversalProcess::post_receipt_reversal` | 原根事务仍包围最终过账全部操作，成功后返回原 ReturnsService 详情；客户端直接过账的原拒绝入口未恢复。 |
| BRR `apply_receipt_reversal_final_post` 入口守卫 | ReturnsService `prepare_receipt_reversal_post` | 先读取 reversal；Reversed 特定 BusinessLogicError → final approve gate → 原 ReceiptReversalPost 动作；上述均早于原回款读取。未用“回款不存在”掩盖更早的冲正状态错误。 |
| BRR `apply_receipt_reversal_posting` 原回款守卫 | AFR `load_posted_receipt` | 原回款存在性 NotFound → status=Posted，否则原 BusinessLogicError。真实调用位于 reversal 守卫之后、累计金额查询之前。 |
| BRR 累计额度判定 | ReturnsService `validate_receipt_reversal_amount` | 按 original receipt ID 读取 posted total 并排除本 reversal ID，再 CumulativeAmountLimit(receipt.amount,reversed_before,reversal.amount)，原累计超额文案保留。 |
| BRR `persist_reversal_offsets_and_mark_receipt` | AFR `reverse_receipt_allocations` | allocations query → plan_reverse → next_seq_range → revert_receipt_settlements → persist_reverse_allocations → receipt.transition(Reversed) → receipt CAS。金额和 occurred_at 由 Process 一对一传入，没有接收整个 returns 聚合。 |
| BRR `revert_receipt_settlements` | AFR 同名 | 批量 offset facts 读取仍先于 chunk 循环；逐块 entry/account 缺项检查 → 条件 revert_settlement；false 保留“冲正冲减超过已核销金额” BusinessLogicError。没有拿准备快照作无条件余额覆盖。 |
| 原 returns `offset_batch::load_receivable_offset_facts` 及其索引工具 | AFR 同名/`unique_ids_in_first_seen_order/index_required_by_id/unique_account_ids_for_entries` | 首见顺序去重 → 两次批读 → 缺项失败关闭 → 按原 ID 取映射；不改为 HashMap 迭代决定业务次序。 |
| BRR `persist_reverse_allocations` | AFR 同名 | 仍 reverse_rows.zip(seqs) 逐条 next_id → ReceiptAllocation::new → create；allocated_at 仍 reversal.occurred_at，引用原 allocation ID 不变。没有提前分配全部 ID 或更改 sequence。 |
| BRR reversal 状态/审计 | ARR `DatabasePosting::{post_reversal,audit}` + ReturnsService `persist_posted_receipt_reversal` | 财务逆向与原回款更新成功后 reversal.mark_posted/CAS，再原 receipt_reversal.post 审计。各步骤同一 Executor，错误即停止。 |
| BRR 审计后 allocations query / `sales_order_ids_for_receipt_allocations` | ARR `DatabasePosting::refresh_sales` + AFR `receipt_allocation_sales_order_ids` | **仍在成功审计之后重新查询完整 allocations**，不复用财务逆转前快照。随后两次 offset facts 批读、按 allocations 找 sales ID、排序去重，再逐单刷新；输入 actor 与 fulfillment=None 保留。 |
| Before approval final dispatch | After `approval_dispatch/action_registry.rs` ReceiptReversalPost 分支 | 原 require_transaction 仍存在，转 `post_receipt_reversal_in_transaction`；后者直接复用 ClientSession。原 `actor_id` 来自 actor.id，新 Process 也使用 actor.id，不改变审计/余额更新操作者。 |
| 原最终过账防重 | ARR 全路径 | 本片未新建命令回执；重复最终动作仍先遇原 reversal/receipt 状态或累计额度闸门，未改成返回成功的兼容旁路。审批运行时自身回执行为未在本次财务拆分中重写。 |

## 8. sales_review 实现自审（非独立审查）

| 原符号 | 新符号 | 自审记录 |
| --- | --- | --- |
| `create_sales_change_order` | Process commands + sales `prepare_creation/CreatedChangeWrite::persist` | 校验、ID 与历史 snapshot 派生保留；绑定/BusinessDocument 在销售集合创建前，销售 head/copy/lines 在 audit 前；创建 key 仍仅 Validate，不新增回放。 |
| `submit_sales_change/start_change_approval` | sales `load_for_submission/prepare_submission/SalesChangeSubmissionWrite::persist` + Process commands/start_approval | 版本/草稿校验早于 BPM receipt 查询的现状保留；freeze submission、锁 copy、start state 均在 graph/receipt/prepare_start 之前。根写序 receipt → document guard → submission head/lines/change → copy → WorkflowAction → BPM/snapshot/tasks → audit。 |
| `prepare_effective_change/prepare_effective_revision_write` | sales `effective.rs` + Process `prepare_receivable_delta` | 首次 change 加载用调用方 executor；其余销售准备仍 NoTransaction。财务主账户查询仍在销售 revision 和本域状态计划完成后，且先于 ReceivableDelta/账户状态计算；零差额仍查询账户。 |
| `build_receivable_delta` | finance `sales_change.rs::prepare_sales_change_receivable/build_sales_change_receivable` | 新接口仅稳定 IDs、财务业务分类、金额、posted_at、actor；方向/绝对金额/复核迁移/缺主账户错误保留。财务 account.update 后才生成 entry ID，再 today/entry；source_fact_type=SALES_CHANGE、source_document_id=销售单、source_revision_id=新版本、sequence=1。 |
| `EffectiveChangeWrite::take_receivable_delta` | Process 持有独立 `Option<SalesChangeReceivableWrite>` | sales 不再持有财务类型；真实 posting 仍 revision → delta.persist(entry→account) → funds task → invoice task → change → audit；Option 仅消费一次。 |
| 详情 DTO 与审批 projection | read-models `sales_center::review` | 原 16 个 DTO struct 的字段/serde/Validate 静态比对一致；原 15 个 review 内联测试各保留一次。该结果属于实现时自审证据，不替代独立运行回归。 |

自审附注：`revision_gross` 的纯公共行求和现通过 sales `revision_fact` 读取，在进入财务 provider 前执行；其输入是已构造的同一正式聚合，无新增 I/O、ID 或取时。不得据此声称逐 token 原位迁移；本记录核销的是校验与可观察副作用顺序。

## 9. 验收使用规则

1. F-01 已完成源码核销；须由 root 运行修正后的正式化 Port 测试，再登记执行器替换防回归测试通过。
2. 对本记录生成后变更的生产方法重新复核对应行，不把正在修改的 After 工作树当成固定提交。
3. root 统一运行阶段 10 原定 library/clippy/边界与合同门禁；本记录不登记任何未执行测试通过。
4. 本记录仅核销明确列出的源码语义；不得外推为真实 MongoDB 并发、回滚、未知提交恢复或全仓业务均已验证。

## 10. 实现固定与执行证据

- Before 固定为 `abf36f41d741dc243f689c877af44f6196eff62b:<表中源路径>`；After 固定为 `07da7863ec0fb975d2093e4e97a43c7e0813e607:<表中目标路径>`。
- 全 workspace library 门禁：3351 passed、0 failed、68 ignored；正式化、资金进度、客户验收与回款冲正的非零大小 Executor 替身测试均包含在 unit-tests.log。
- 最终静态扫描 changed/missing/added 均为 0；原始 needs_review=48 与 exit 2 保留。46 项词法解析限制见 parser-limitations-review.json；其余幂等/事务符号按本文件逐项核销。
- 真实数据库运行未验证；不得将本报告用于声明实际 MongoDB 回滚、并发或未知提交恢复已经通过。
