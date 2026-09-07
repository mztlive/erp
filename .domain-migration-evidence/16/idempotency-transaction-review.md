# 阶段 16 幂等、事务与事实来源源码核验记录

- 输入提交：`ed8015e28d36e5b9323b87e1a1fd5f63c122eb70`。
- 候选：`/private/tmp/erp-domain-crate-16-supply`；绑定：`72a0c79a2261d33699b869329376e534edfb1ef4`。
- 状态：`source_review_complete`。
- 范围：88 个 source-map 源、21 个 owned 来源、15 个额外实际调用/provider 文件；502 个 before qualified 函数，规范化主体一致 393 个，改变/提取后实际追踪 109 个。
- 结论限定：当前实际审查未发现语义漂移；未封存时不得据此宣布阶段完成。
- 证据级别：实际函数及其生产 provider 的源码核验；未运行 Cargo、MongoDB、外部 gateway 或历史 tests。**真实数据库运行未验证**。
- 完整逐符号 before/after/provider 路径、行、文件 SHA256、函数源码 SHA256、主体 token SHA256 存于同名 JSON。

## api_identity：连接命令身份、回执及错误恢复边界

- execute_connection_command 保持 Validate → 动作权限 → CommandIdentity → receipt replay → Prepared.try_from → 七动作分派。权限仍先于回放；未新增任意事务错误后的恢复外壳。
- CommandIdentity 的 actor/id/action/trim(key)、w20-command-/w20-audit- 前缀和长度前缀 SHA-256 不变；command_fingerprint、confirmation_fingerprint、capability_update_fingerprint 的各自原输入/排序/serde fallback 不统一。
- replay_command 先匹配指纹，job_id Some 才从 support 原 owned find_by_id(NoTransaction) 读 job_no；缺 job 仍 None。
- persist_command_receipt 在原时点先构造 receipt，再构造 audit，实际 persist_receipt/MongoReceiptWrite 依次 domain receipt.create → audit.create → 可选 job_no 查询，全部使用收到的同一 Executor，await? 首错停止。
- 确认和能力更新仍仅原事务前回放，不增加事务错误恢复。确认回放 connection_version 取 frozen confirmation.connection_version.saturating_add(1)；能力更新回放仍完整读取当前 detail，非简化投影。
- Raw 分类命中的 CommandIdentity 实际仍为 connection_id/actor_id/action/idempotency_hash/fingerprint/receipt_id/audit_id 七字段；跨 crate 仅 pub(super)→pub，无 serde 派生。new 的摘要输入、trim、确定性 ID 和首次 required 校验保持原序。

## api_create_reference：连接创建及外部引用两次校验

- create_connection 保持 req.validate → PreparedSupplierConnectionCreate → supplier_accounts.find_by_id(NoTransaction) 仅存在性 → prepare_connection 内 next_id/实体/费率规则 → audit 构造 → 根事务 connection + 原空 capabilities 写 → audit。未添加供应商 active 校验。
- execute_reference/ReferenceCommand 实际生产链为 load_reference_target(NoTransaction，连接/版本/非 Active) → action 到引用 kind → registry.resolve(payload_reference,environment) → commit_reference_command 根事务重新读连接/版本/非 Active → binding → CAS → receipt/audit。
- 外部 registry.resolve 不接 Executor；preflight 失败不外呼，外呼失败不提交。reference_error 仍 BusinessLogicError(code + ': ' + summary)，不按新 failure enum 重分类。

## api_governance：连接治理、确认、能力及任务意图写序

- status 事务读取 connection/version → capabilities → confirmations → recent health → active offerings projection → open supplier orders count → active support catalog jobs；之后完整 impact.blockers 的第一项 → enable/disable → connection CAS → receipt/audit。
- 旧 connection_impact 拆成 owned_connection_impact 与 support count_active_supplier_catalog_jobs。供给投影读取仍先于订单 count；任务状态 pending/running/partially_succeeded、domain_job_type、domain_job_id、not-deleted 过滤保持。
- 确认事务仍 connection/version → capability/version → 确定性 confirmation ID/new(now) → touch connection/CAS → audit 构造 → confirmation create → audit create。prepare_business_confirmation 包含原第一个 connection CAS，未把 audit 提前。
- 能力变更保留 shape 构造先于 audit replay；事务内 connection/version/nonActive → caps → confirmations → classify/apply → 原更新列表逐 CAS/新增有序批写 → connection configuration/CAS → audit。写成功后完整 detail 查询；result.connection_version 取事务值，capabilities 取后续 detail。
- 健康/目录意图事务均连接/version → capabilities/blocker → BackgroundJob next_id/new。健康再 deterministic run/new(active capability snapshots)，随后 job create → run create → receipt/audit；目录不创建 run。未提前 ID/时钟或引入外部调用。
- 新增 BusinessConfirmationInput 只收纳原确认根已构造的 id/command/operation_id/idempotency_hash/fingerprint/actor_id，按值解包后运行原连接→能力→版本→确认实体→连接 CAS；结构不进行 I/O、时间/ID 求值，也无 serde 持久化合同。

## api_read：连接查询、权限、引用可见性

- 连接列表仍 domain page → capabilities-by-connections → suppliers → parties/current revisions → 原页序映射；actor 参数仍未用于列表内权限，不新增 name 缺失错误或额外排序。
- 详情仍 connection → 完整 governance context → reference metadata permission → capability confirm permission → capability update permission → 原治理投影 → action::all 原序逐权限，授权通过后才每次 registry.is_available/blockers。
- ReadService 默认 registry None 在原 is_available 位置映射 false，等价旧 UnavailableSupplierReferenceRegistry::is_available=false；已注入实例由 Process.reads/AppState 原 Arc 复用。旧完整 detail 的读取/首错并未因能力写后返回而跳过。
- connection_job 改 support.find_supplier_connection_job，保 id/domain_job_id/类型白名单和空白名单 None，调用 owned.find_one 的 not-deleted 语义不变。

## jobs：后台连接任务：事务退出、外呼及结果事务

- process_connection_job 仍 NoTransaction 读原 support owned job → missing NotFound → terminal return → 原两种 job_type 分发；未新增任务扫描、重试 worker 或外部配置查询。
- HealthExecution/start_health_job 的 root 完成后才 gateway.health_check，实际 execute runner 保 start 错误停止、outcome 原样传 finish；gateway 不接 Executor。单调时钟仅用于原 latency，业务 now 位置不变。
- 健康 start 事务为 job/Pending → run → connection → now → job.start/run.start → job CAS → run CAS。finish 事务为 job → run → connection → now；配置变化优先 ResultUnknown/TECHNICAL_CONFIG_CHANGED，写 W29 且不写 connection health。普通 Err 为 failure transitions → Failed health/touch → connection CAS → W29；Ok 为 progress/succeed/run.succeed → Healthy/touch → connection CAS。共同尾部 job CAS → run CAS → audit 构造/写。
- CatalogExecution 保持 start 事务前读 connection(NoTransaction)，开始事务只 job/Pending/now/start/CAS；gateway.catalog_sync 使用预读连接。finish 仅 job → now → Err/Ok settle → W29(失败) → job CAS → audit，不重读连接、不增加技术版本或 health guard。
- persist_health_failure_task 仍确定性 integration error ID/实体 → 原唯一 producer WorkItem(next_id,now) → 确定性 audit 构造 → task create → WorkItem create → audit，沿原 Executor。八类 failure 只经 integration_class/supplier_class 全分支机械映射；自动重试政策仍属于 integration。
- Unavailable API/registry 与显式模拟网关保持原失败关闭值；此次静态审查不构成真实外部系统调用成功证据。

## offering_qualification：Offering 唯一资格来源及六次读取

- MongoOfferingQualification → qualify 实际先 SKU.find/缺失 → SKU active → Product.find/缺失 → 原 Product.product_kind 显式四类 fact 映射；不读 SKU revision/category，也不加 Product active 校验。
- 随后 supplier ensure_offering_with_port 读 capability by required code → current revision pointer；ensure_qualified_with_port 再依次 supplier → capability revision → 按 revision.capability_code 第二次 capability。总计六次 DB 读取，第二次能力读取未缓存/合并。
- Supplier disabled 等纯规则仍在六读后才判定；原缺 SKU/商品/能力/指针/供应商/版本和资格不满足的 NotFound/BusinessLogicError 文案顺序保持。日期上下界和原停用资格注释保持。
- 所有六读复用传入的同一 Executor；旧公共 ensure_capability_qualified wrapper 仍传 NoTransaction，无新增事务。资格 foreign error 通过逐 variant bridge 原样进入 services，不转 Internal(string)。
- create 总在首 revision 构造后资格检查；revise 仅 next_status Active 使用新 revision.valid_from；Paused/Stopped、availability、exception、replay 均不查资格。

## offering_command：Offering 三命令的 ID/时钟/写入与任意错误恢复

- 三个 prepare 均先原 validate → raw JSON fingerprint → raw idempotency_key 查询；命中 ensure_replayable/replay_result 后返回，不读取当前对象、资格或新时间。存储 trim key 与查询 raw key 的原差异保留。
- create 保 offering ID/data/new → identity duplicate → optional source connection → terms/data → revision ID/new → qualification → received_at now/source time/availability data → availability ID/new → current pointer → result → command ID/new → audit → 根事务。
- revise 保原 raw id 查对象 → 最大 revision_no 查询 → next_no → terms/revision ID/new → next status/conditional qualification → status/pointer → next_persisted_version → audit。事务 revision insert → offering CAS → result → command ID/new/insert → audit。
- availability 保 fingerprint/replay 后才 typed trim id → offering → availability → optional expected version → now/source time/data/apply → next_persisted_version → audit。事务 availability CAS → result → command ID/new/insert → audit；CAS 只更新原版本/时间，不改变 result 所取 status/source time。
- 实际 MongoOfferingWrite/create_triple/append_revision 保 offering → revision → availability，或 revision → offering CAS；service write runner 随后原 command，process commit runner 最后 audit。每步相同 Executor，首错停止。
- resolve_command_result/resolve_written_result 实际委派 recovery::resolve：成功零读取；任意事务 Err 只查一次原 raw key，命中校验/解码，缺失返原错，重读/指纹/解码错误覆盖原错；不收窄为 DuplicateKey，不重试写事务。
- 新增 PreparedCreate/PreparedRevision/PreparedAvailability 仅把原根局部状态跨边界传递：Create 保留事务前已构造 command，Revision/Availability 保留原始 idempotency_key，command 仍在原写成功点后构造；三个结构无 serde，不改变数据库 payload。

## offering_exception_read：供给停止正式任务及列表事实

- complete_supply_exception_task 保 validate → path/decision trim match → nonempty task/subject → expected_task_version → CommandReceipt → committed_resource replay；RBAC 构造仍位于首次 replay 后。
- 实际事务 offering existence → raw WorkItem → type/object/id/reason → Open → task version → trimmed subject → ensure_domain_decision_access 同 Executor → now/complete → decision audit 构造 → receipt audit 构造 → WorkItem CAS → decision audit → receipt audit。只完成任务，不写供给或发布。
- 任意事务失败仅 fresh committed_resource_id 回读一次。replay 先 committed id/request task match，再 raw WorkItem NoTransaction，要求 Completed/当前 offering association；不新做授权/当前版本校验。
- 列表仅上移 SupplierOfferingReadRepository，原 normalized query、availability/keyword/product-code/SKU-code 筛选交叉、page find→count、revision/availability/display 六批次与行映射保持。原缺事实的 Option 展示、无历史 fallback 和单字段排序保持；HTTP cost permission 仍在列表读取成功后执行并只 redact 原七字段。

## fulfillment_dispatch：履约下单、售后和普通 dispatch

- submit_place 保 validate → fulfillment_order_no replay（不比新 payload、不外呼）→ ensure_placeable 原 connection/active/supplier/capability/revision/offering 两批读 → 原 ID/实体/now/items/action → inbox → audit。事务 order→items→PLACE action→inbox→audit；after_intent 真实用于根，只有提交成功才 gateway.dispatch。
- 售后共用 submit_after_sales_action：validate → order → order_no+action key replay/lines → connection/capabilities/action type → ensure_action_lines → action/逐 line ID → pending transition → inbox/audit。事务 action→逐条 line insert→order CAS→inbox→audit→commit 后 gateway。未添加 expected-version、active connection 或净余额守卫。
- 普通 Succeeded 保可选单号 update→Accepted→action Succeeded→message Processed(now)；缺单号仍照原成功。Rejected 先 action Failed，再仅 PLACE order Rejected，再 message Processed。ResultUnknown/Failed 分支在本域状态变化后才 message Failed/error task。
- Failed 的 integration can_auto_retry 仅从唯一 eight-way adapter 映射；可重试只 record_attempt(Some(now))，不写 response summary/order Exception。不可重试 action Failed/summary，仅 PLACE order Exception。
- 结果事务前原 WorkItem ID 调用点保留；事务 order CAS→action CAS→error task + inbox CAS（失败）或 inbox CAS→原 W26 active task 读取/刷新/创建→对应 audit。刷新仅同类型首任务 subject 不等于 CAS 后 order.version 时发生。普通写入没有新增事务错误回放。
- W26 工厂保大写 SUPPLIER_FULFILLMENT_ORDER、order ID/version，ResultUnknown 对应 IntegrationResultUnknown 其余 BusinessException；role-procurement/company、当前 actor、SystemRule/High/due None、原 reason/impact 保持。它不替换为 W29 工厂。

## fulfillment_callbacks：拒单和退款回调事实来源及写序

- reject 保 validate→order→connection/event replay→advance Rejected→history ID/received_at→latest PLACE query→action Failed→audit；真实 persist_reject order CAS→history create→action CAS→outer audit，同一 Executor，不加事务错误恢复。
- refund 保 validate→order→connection/refund_no/version replay→allocations read/return。未命中才两 sum（成本，已退金额）→累计上限→Refunded/Partial→inbox ID/now→fact ID→逐输入 allocation ID/sequence→validate_allocations→audit。
- build_refund_fact 只把完整 InboxMessage 输入换成 caller 显式 InboxMessageId(message.base.id)，其 source_event/refunded_at/原引用金额、APPLY、两 None 字段及 allocation 次序不变；没有读取新 finance 聚合。
- refund_writes 实际 provider 为 inbox create→domain order CAS→fact insert→allocations insert_many→audit。同 Executor 首错停止；原 replay 不新增 existing.order/path 或 payload 比较，未新增并发冲突自动恢复。

## fulfillment_investigation：调查 durable intent、外部结果冻结及最终结算

- object/task 入口保原 validate/fingerprint 次序；task 正版本解析仍先于 fingerprint。receipt replay 先于 stable evidence/key 与意图事务，命中不重验当前责任。
- ensure_investigation_intent 保 order/version→target/original association→task/raw/version/Open/object/subject/owner/no-op/真实授权（或 no active W26）→Replay safety→existing intent 验证/可选 prepared 解析，或 prepare_intent 内序列化/new→create action，同 Executor。
- 已有 durable prepared 返回后不进入 prepare/gateway；未命中 prepare 再 NoTransaction order/version/target/task/subject/no-op 或 no active task，QueryResult 当前权威终态短路、Replay safety，之后 connection active/capability→gateway；此处不重复正式 workflow 授权。
- 网关返回先 Unicode take(512) bounded，再独立 persist_prepared 事务 evidence→validate intent→已有 response 解析首次结果，否则 durable serde→Pending/summary/no next attempt→CAS。最终 CAS 失败不会删除 durable 事实或重开 gateway。
- 最终事务原 order/version→target→保存 investigated_order_version→object 无 active task→Replay safety→apply_prepared；仅变化才 order CAS，再仅变化才 target CAS；构造 result record→intent read/validate/parse durable/equality→evidence CAS；最后才 task/raw/W26 guards/旧 order version subject/no-op/真实授权→new subject/activity(now)/task CAS→receipt audit。
- 只有最终事务 Err 才一次 replay_investigation；intent、prepare/gateway、persist-prepared 错误原样 ?，不新增恢复范围。fresh read 错优先。replay 保 audit success/resource/action→message/receipt/fingerprint→task presence→evidence/parse→operation_id→order→audit.resource_id→response，未补 audit.actor 检查。
- Replay 与普通 dispatch 保独立：成功有单号 order update→Accepted→target success；成功无单号只 target ResultUnknown；Rejected 先 order 再 target；Failed 固定 record_attempt(None)→ResultUnknown，不调用 can_auto_retry。ensure_replay_safe 按最新 Query 序跳过非法并取首个匹配 target 的可解析记录，仅 VerifiedNoResult 可重放。

## fulfillment_complete：履约正式完成、回执及窄授权读事实

- complete_order_task 保 validate→正 task version→fingerprint/audit identity→receipt replay→stable terminal identity/key→RBAC 构造→事务；原 no-op 不是正式授权替代。
- 事务 task read→W26 version/Open/type/object/id/subject/owner→no-op→真实 workflow access→order/version→subject→evidence Query/Succeeded/schema/VerifiedTerminal→target/original association→当前权威终态一致；之后 terminal record serde/action new→completed_at 一次 now→activity/complete→terminal action insert→WorkItem CAS→receipt audit。
- 任何事务 Err 才一次 fresh replay；replay audit 身份/receipt→terminal action Query/Succeeded/schema→task id/resolution一致，不读取当前 WorkItem 或重做权限。文本 fp/e/o/t、fp/a/o/t/r、分隔符/重复 key/正版本/错误类别和四 schema 保持。
- W26 详情原 order/items/actions/histories/refunds/supplier name/blockers→trim task id→实际 WorkItemAuthorizationReadPort 调用位置不变；只映射原五字段，后续 active task 查询和有 Process 时 raw WorkItem 重读不合并。root/subject/type 的五字段比较及 ||false 保留。
- ensure_task_actor_eligible 原 let_ tuple/Ok 无操作实现唯一迁 RM fulfillment_access，Process 和 RM 原时点调用；它不被记作真实授权证明。窄 Port/adapter 独立证据见 supply16-work-item-authorization-review.json。

## settlement_source：结算权威来源、查询与首错

- record_source_evidence 保 validate→request_hash→request_id replay/hash→period/unique input items→latest period/version→source scope→完整性→逐请求行构造→source_as_of now→canonical hash→evidence next_id/new→audit→root evidence create→audit；任意事务错只一次原 request_id replay。
- build_source_evidence 仅将 actor 改稳定 actor_id、NoTransaction 改 caller 同 Executor；scope 缺订单/退款头首错、原 HashMap 遍历、漏行排序前 20、输入逐行关系/金额构造顺序不变。
- source_scope 保期间 refund facts(id asc)→union explicit/refund order ids→orders(id asc)→items(id asc)→必要时补读 item 的缺失 order→非空 refund ids 时 allocations。supplier/Shanghai 含首不含尾、COMPLETED 且 completed_at null 损坏行保留，未扩大到全历史。
- latest_for_period 保 supplier/period/policy id/version/deleted 及 source_version desc→created_at desc→id desc；latest_for_scope 不含 policy，created_at desc→source_version desc→id desc。request replay 先于最新来源版本检查。

## settlement_draft：结算创建、相同来源 no-op 与草稿物理替换

- create 保 validate/action/date/period→确定性 statement_no→existing replay（只核 supplier/period）→latest source→statement ID→from_source 逐 item/difference ID→now→statement/new/refresh/subject→fingerprint/audit→root statement insert→无条件 items insert_many→非空 differences insert_many→audit。
- refresh 保 validate/action/path→fingerprint/audit replay→statement/prepared actor/version/source hash→latest frozen policy source→old items→old differences。相同 hash 原 NoTransaction audit→UNCHANGED，不检查 editable、不写 statement。
- 不同 hash 先 snapshot IDs→now/refresh_snapshot/subject→receipt(version+1)→audit，再根重读 statement/version/source hash→replace→audit。没有把旧快照替换为事务前读缓存或新增 editable guard。
- 实际 replace_draft_snapshot→replace_snapshot/MongoDraftSnapshotStore：old_difference_ids 非空删补证→old_item_ids 非空删差异→无条件删本 statement items→statement CAS→无条件 insert new items→new differences 非空 insert；过滤器、物理 delete、同 Executor 和首错保持。
- 本条以 ed8015e28 的真实 command.rs 为依据，明确覆盖旧总合同第 5.2 的错误摘要；仓储没有 editable guard。补证/差异两个旧 ID 条件不得混写为同一条件。

## settlement_difference：正式差异决定、补证和作废

- 差异决定保 validate/path→typed conclusion→fingerprint/replay→difference/version→item/statement association→statement/version→prepared actor/editable→可选已登记补证集合→now/record_conclusion→all items/differences→替换该 difference→HasDifference/subject→root statement CAS→difference CAS→receipt/audit。
- 补证保 request hash/replay→statement editable statuses→difference/version→item association→evidence ID/now/new→audit；事务 EvidenceStore 真 provider 再 difference/version→item/association→statement/editable→statement CAS→evidence create→outer audit。没有新增 prepared-only 限制。
- 差异与补证根的原任意事务错误恢复和首错保持；补证 request replay 比较 hash/statement/difference，不改变结论。
- void prepare 保 validate→statement→Voided 立即返回（先于经办/editable/version）→原 guards/version→void_draft→audit→root statement CAS→audit→commit 后回填 caller entity。没有新增审计 receipt 或失败恢复。

## settlement_review：提交复核、正式决定和财务入账

- submit_review 保 validate/path/fingerprint/audit replay→statement/prepared actor/version→action/subject/cutoff→WorkItem ID/new；root statement reread/version/subject→prepared/editable→items nonempty→diffs/subject→submit_review→statement CAS→WorkItem create→receipt audit。未增加 reviewer eligibility 查询。
- decide_review 事务前保 validate/path→parsed reject reason→task version→action/fingerprint/replay→statement/version→WorkItem exists/version/subject/pending/type/object/role/company/owner→items→differences→resolved subject→now。
- Confirm 保 ensure_confirmable→payable_amount→finance account ID/new→entry ID/new→cost builder→record_review；Reject 回 Draft/HasDifference 后原 reason 检查→record_review，不调用 finance。之后 WorkItem activity/complete 才组根事务。
- 真实根固定 workflow authorization→prepared/reviewer separation→重新 items→重新 differences→resolved subject→statement CAS→WorkItem CAS→payable account/entry→逐 cost entry/allocations→receipt audit，同一 Executor。原 separation helper 的 db/item/executor 仅 let_ tuple，提取为纯 ensure_reviewer_separation 不遗漏读取。
- SettlementPayableSource 仅 statement_no/supplier_id/subject_hash/period_end 显式映射。account source_document_id 仍 statement_no；gross/invoiceable=amount，settled/invoiced=0；Entry Original/Increase、相同 amount、due period_end、supplier_settlement/statement_no/subject_hash/sequence1、posted_at frozen now。
- 成本 builder 仅 gross/net/tax 三者都零返回空；任一非零原 BusinessLogicError 文案保持。它仍发生在两个 payable 实体构造后、record_review/WorkItem mutation/事务前；未伪造 CostEntry 或加入额外 ID。零成本差额 Confirm 仍真实创建应付。
- persist_settlement_payable 复用原 PayableRepository account→entry；persist_settlement_costs 复用原 CostRepository entry→allocations，均不另开事务。
- 新增 Posting 结构借用原 db/actor/rbac/statement/task/payable/payable_entry/cost_entries，并拥有原 cost_delta/status/operation/fingerprint/audit/action；结构构造不执行 I/O、时钟或 ID，真实 MongoPosting 仍逐 Step 在同一 Executor 执行。

## settlement_receipts_read：结算各回执比较差异和详情事实

- digest_parts 保 UTF-8 段长 u64 BE + bytes，supplier-settlement-command-/command_sha256= 前缀、各 fingerprint 输入和 pipe receipt delimiter/正整数/金额解析/错误类别不变。
- refresh 要求当前 statement version/hash 精确等于 receipt；difference 要求 difference version 精确且非 pending，statement version 允许 >= receipt，响应仍冻结 receipt version；submit_review 允许 statement/task >= receipt，保 type/object/subject/role/company，不新增 Completed。
- review decision 要求 statement/task 精确版本、Completed/type/object/subject；Confirm 的 status/result/payable 和 Reject 的 Draft|HasDifference/rejected/no payable 各自精确校验。没有以通用宽松比较替代。所有带恢复根仍任意事务错只一次 fresh original-key 回读。
- 四个单域 GET 的 page find→count、同 filter stats 二次读、空 stats 零值、单字段排序及 difference page evidence 空保持。详情调用唯一 supply snapshot statement→items→非空 item ids differences→非空 difference ids evidence，原双 association 过滤与稳定组内顺序不变。
- 复核详情仍仅 pending-review 查 tasks、类型筛选后必须唯一；eligible=true 原显示占位保持。owned/eligible/separation 纯规则只换最小 bool 输入，不新增权限读。非零 delta 仍移除 Confirm/加缺成本 lineage blocker；review_processing_state 不合并全部 object blockers。

## wiring_errors：HTTP/构造注入、错误桥接和相邻事实

- HTTP 仅调用归属和 DTO import 调整：API 四写/三跨域读/能力单域读，Offering 四写/列表，履约八写/域列表/RM详情，结算八写/四域读/RM详情。route、permission、actor 提取、request body/path/query、ApiResponse 包装保持。
- API command await 返回后 Some(job_id) 的原 tokio::spawn 捕获和日志保持，调用唯一 supplier_connection_execution Process；AppState 外部 connectors/readiness 原字段与判定不变。新 domain 构造仅 db，上层复用原 registry/gateway/RBAC Arc。
- Supply Error 与 services/finance/supplier 错误桥接逐 variant 保持 Validation/Business/Conflict/Forbidden/Logic/Repository/OutcomeUnknown，optimistic/duplicate/transaction unknown 的原文和类别不以字符串重分类。
- Integration evidence_adapter 的履约退款正式事实读取仅改供给 Ext，原 discover/verify/known_result 分支、association 和同 Executor 不变。Catalog 查询 provider 由 A 独立审查，本报告不重复认定其语义。
- 本报告仅静态源代码核验，纯测试定义与 root 执行结果分离；真实 MongoDB 事务、回滚/并发/索引和真实外部供应商系统均未运行验证。

## 逐改变或提取的实际符号

| Before qualified symbol | After actual provider | 合同组 | SHA256 前 16 位 |
| --- | --- | --- | --- |
| `web_api::app_state::impl<AppState>::new_with_connectors` | `web_api::app_state::impl<AppState>::new_with_connectors` | wiring_errors | `01be54e298ccd5de` |
| `web_api::app_state::impl<AppState>::supplier_api_service` | `web_api::app_state::impl<AppState>::supplier_api_service` | wiring_errors | `01be54e298ccd5de` |
| `web_api::app_state::impl<AppState>::supplier_fulfillment_service` | `web_api::app_state::impl<AppState>::supplier_fulfillment_service` | wiring_errors | `01be54e298ccd5de` |
| `web_api::core::handler::supplier_api::service` | `web_api::app_state::impl<AppState>::supplier_api_service`<br>`web_api::app_state::impl<AppState>::supplier_api_governance_process`<br>`web_api::app_state::impl<AppState>::supplier_api_read_service` | wiring_errors | `01be54e298ccd5de`<br>`01be54e298ccd5de`<br>`01be54e298ccd5de` |
| `web_api::core::handler::supplier_api::supplier_api_connection_list` | `web_api::core::handler::supplier_api::supplier_api_connection_list` | wiring_errors | `f884662007ae94d4` |
| `web_api::core::handler::supplier_api::supplier_api_connection_detail` | `web_api::core::handler::supplier_api::supplier_api_connection_detail` | wiring_errors | `f884662007ae94d4` |
| `web_api::core::handler::supplier_api::supplier_api_connection_create` | `web_api::core::handler::supplier_api::supplier_api_connection_create` | wiring_errors | `f884662007ae94d4` |
| `web_api::core::handler::supplier_api::supplier_api_connection_command` | `web_api::core::handler::supplier_api::supplier_api_connection_command` | wiring_errors | `f884662007ae94d4` |
| `web_api::core::handler::supplier_api::supplier_api_business_capability_confirm` | `web_api::core::handler::supplier_api::supplier_api_business_capability_confirm` | wiring_errors | `f884662007ae94d4` |
| `web_api::core::handler::supplier_api::supplier_api_capabilities_update` | `web_api::core::handler::supplier_api::supplier_api_capabilities_update` | wiring_errors | `f884662007ae94d4` |
| `web_api::core::handler::supplier_api::supplier_api_connection_job_detail` | `web_api::core::handler::supplier_api::supplier_api_connection_job_detail` | wiring_errors | `f884662007ae94d4` |
| `web_api::core::handler::supplier_api::supplier_api_capability_list` | `web_api::core::handler::supplier_api::supplier_api_capability_list` | wiring_errors | `f884662007ae94d4` |
| `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_detail` | `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_detail` | wiring_errors | `5e28d83c37787395` |
| `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_investigation` | `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_investigation` | wiring_errors | `5e28d83c37787395` |
| `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_task_investigation` | `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_task_investigation` | wiring_errors | `5e28d83c37787395` |
| `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_task_completion` | `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_task_completion` | wiring_errors | `5e28d83c37787395` |
| `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_submit` | `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_submit` | wiring_errors | `5e28d83c37787395` |
| `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_cancel` | `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_cancel` | wiring_errors | `5e28d83c37787395` |
| `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_refund` | `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_refund` | wiring_errors | `5e28d83c37787395` |
| `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_reject` | `web_api::core::handler::supplier_fulfillment::supplier_fulfillment_order_reject` | wiring_errors | `5e28d83c37787395` |
| `web_api::core::handler::supplier_fulfillment::supplier_refund_fact_post` | `web_api::core::handler::supplier_fulfillment::supplier_refund_fact_post` | wiring_errors | `5e28d83c37787395` |
| `erp_finance::repository::payable::command::impl<< ' a > PayableRepository < ' a >>::create_payable_with_entry` | `erp_finance::repository::payable::command::impl<< ' a > PayableRepository < ' a >>::create_payable_with_entry` | settlement_review, settlement_receipts_read | `47ee7930dea693d1` |
| `erp_processes::supplier_connection_execution::catalog::impl<SupplierConnectionExecutionProcess>::start_background_job` | `erp_processes::supplier_connection_execution::catalog::impl<SupplierConnectionExecutionProcess>::start_background_job` | jobs | `90915cf28d14fe12` |
| `erp_processes::supplier_connection_execution::catalog::impl<SupplierConnectionExecutionProcess>::finish_catalog_job` | `erp_processes::supplier_connection_execution::catalog::impl<SupplierConnectionExecutionProcess>::finish_catalog_job` | jobs | `90915cf28d14fe12` |
| `erp_processes::supplier_connection_execution::failure::persist_health_failure_task` | `erp_processes::supplier_connection_execution::failure::persist_health_failure_task` | jobs | `9d27c753c19684a3` |
| `erp_processes::supplier_connection_execution::health::impl<SupplierConnectionExecutionProcess>::start_health_job` | `erp_processes::supplier_connection_execution::health::impl<SupplierConnectionExecutionProcess>::start_health_job` | jobs | `8129e7e6b6f2497d` |
| `erp_processes::supplier_connection_execution::health::impl<SupplierConnectionExecutionProcess>::finish_health_job` | `erp_processes::supplier_connection_execution::health::impl<SupplierConnectionExecutionProcess>::finish_health_job` | jobs | `8129e7e6b6f2497d` |
| `erp_processes::supplier_connection_execution::impl<SupplierConnectionExecutionProcess>::process_connection_job` | `erp_processes::supplier_connection_execution::impl<SupplierConnectionExecutionProcess>::process_connection_job` | jobs | `64faaaf0a4ef56b1` |
| `erp_read_models::supplier_center::fulfillment_detail::impl<SupplierFulfillmentDetailReadService>::supplier_fulfillment_order_detail` | `erp_read_models::supplier_center::fulfillment_detail::impl<SupplierFulfillmentDetailReadService>::supplier_fulfillment_order_detail` | fulfillment_complete | `c86627e8daab09da` |
| `erp_supplier::service::supplier::eligibility::ensure_capability_qualified` | `erp_supplier::service::supplier::eligibility::ensure_capability_qualified` | offering_qualification | `9608eb0c8c2e74bc` |
| `erp_supplier::service::supplier::eligibility::load_capability_revision` | `erp_supplier::service::supplier::eligibility::load_capability_revision` | offering_qualification | `9608eb0c8c2e74bc` |
| `database::repository::supplier_api::impl<< ' a > SupplierApiRepository < ' a >>::governance_job` | `erp_support::repository::owned::background_job::impl<< ' a > BackgroundJobRepository < ' a >>::find_by_id` | api_identity, api_create_reference, api_governance, api_read | `ad93571a13e5f569` |
| `database::repository::supplier_api::impl<< ' a > SupplierApiRepository < ' a >>::governance_audit` | `erp_audit::repository::owned::audit_log::impl<< ' a > AuditLogRepository < ' a >>::find_by_id` | api_identity, api_create_reference, api_governance, api_read | `816bc286ba938d2a` |
| `database::repository::supplier_api::impl<< ' a > SupplierApiRepository < ' a >>::connection_job` | `erp_support::repository::supplier_connection_job::impl<BackgroundJobRepository < ' _ >>::find_supplier_connection_job` | api_identity, api_create_reference, api_governance, api_read | `d05743c3d4a150d0` |
| `database::repository::supplier_api::impl<< ' a > SupplierApiRepository < ' a >>::governance_data` | `erp_supply::repository::supplier_api::impl<< ' a > SupplierApiRepository < ' a >>::governance_data` | api_identity, api_create_reference, api_governance, api_read | `799eb13f0aab45f5` |
| `database::repository::supplier_api::impl<< ' a > SupplierApiRepository < ' a >>::connection_impact` | `erp_supply::repository::supplier_api::impl<< ' a > SupplierApiRepository < ' a >>::owned_connection_impact`<br>`erp_support::repository::supplier_connection_job::impl<BackgroundJobRepository < ' _ >>::count_active_supplier_catalog_jobs` | api_identity, api_create_reference, api_governance, api_read | `799eb13f0aab45f5`<br>`d05743c3d4a150d0` |
| `database::repository::supplier_offering::impl<< ' a > SupplierOfferingDomainRepository < ' a >>::create_with_revision_and_availability` | `erp_supply::repository::supplier_offering::impl<< ' a > SupplierOfferingDomainRepository < ' a >>::create_with_revision_and_availability` | offering_qualification, offering_command, offering_exception_read | `8830d677c1932e80` |
| `database::repository::supplier_offering::impl<< ' a > SupplierOfferingDomainRepository < ' a >>::append_revision` | `erp_supply::repository::supplier_offering::impl<< ' a > SupplierOfferingDomainRepository < ' a >>::append_revision` | offering_qualification, offering_command, offering_exception_read | `8830d677c1932e80` |
| `database::repository::supplier_settlement::command::impl<< ' a > SupplierSettlementRepository < ' a >>::replace_draft_snapshot` | `erp_supply::repository::supplier_settlement::command::impl<< ' a > SupplierSettlementRepository < ' a >>::replace_draft_snapshot` | settlement_draft | `662dac367ba1437d` |
| `services::supplier_api::governance::command::impl<SupplierApiService>::confirm_business_capability_requirement` | `erp_processes::supply_governance::supplier_api::command::impl<SupplierApiGovernanceProcess>::confirm_business_capability_requirement` | api_identity, api_create_reference, api_governance, api_read | `91f2197cab5b20e0` |
| `services::supplier_api::governance::command::impl<SupplierApiService>::update_capabilities` | `erp_processes::supply_governance::supplier_api::command::impl<SupplierApiGovernanceProcess>::update_capabilities` | api_identity, api_create_reference, api_governance, api_read | `91f2197cab5b20e0` |
| `services::supplier_api::governance::command::impl<SupplierApiService>::execute_reference_command` | `erp_processes::supply_governance::supplier_api::reference::impl<SupplierApiGovernanceProcess>::execute_reference_command` | api_identity, api_create_reference, api_governance, api_read | `74f50069c31cf881` |
| `services::supplier_api::governance::command::impl<SupplierApiService>::commit_reference_command` | `erp_processes::supply_governance::supplier_api::reference::impl<SupplierApiGovernanceProcess>::commit_reference_command` | api_identity, api_create_reference, api_governance, api_read | `74f50069c31cf881` |
| `services::supplier_api::governance::command::impl<SupplierApiService>::execute_status_command` | `erp_processes::supply_governance::supplier_api::command::impl<SupplierApiGovernanceProcess>::execute_status_command` | api_identity, api_create_reference, api_governance, api_read | `91f2197cab5b20e0` |
| `services::supplier_api::governance::command::impl<SupplierApiService>::replay_command` | `erp_processes::supply_governance::supplier_api::command::impl<SupplierApiGovernanceProcess>::replay_command` | api_identity, api_create_reference, api_governance, api_read | `91f2197cab5b20e0` |
| `services::supplier_api::governance::command::impl<CommandIdentity>::new` | `erp_processes::supply_governance::supplier_api::impl<SupplierApiGovernanceProcess>::new`<br>`erp_read_models::supplier_center::supplier_api::impl<SupplierApiReadService>::new`<br>`erp_supply::service::supplier_api::impl<SupplierApiService>::new` | api_identity, api_create_reference, api_governance, api_read | `0836fe35b1f1b9d3`<br>`a75a92ae38eec635`<br>`c7c0156b10321900` |
| `services::supplier_api::governance::command::persist_command_receipt` | `erp_processes::supply_governance::supplier_api::receipt::persist_command_receipt` | api_identity, api_create_reference, api_governance, api_read | `f811b746a2cc100e` |
| `services::supplier_api::governance::context::impl<SupplierApiService>::governance_context` | `erp_read_models::supplier_center::supplier_api::context::impl<SupplierApiReadService>::governance_context` | api_identity, api_create_reference, api_governance, api_read | `139fffc459892209` |
| `services::supplier_api::governance::jobs::impl<SupplierApiService>::connection_job` | `erp_read_models::supplier_center::supplier_api::jobs::impl<SupplierApiReadService>::connection_job` | api_identity, api_create_reference, api_governance, api_read | `aa49a8d1508056b7` |
| `services::supplier_api::governance::jobs::impl<SupplierApiService>::create_health_job` | `erp_processes::supply_governance::supplier_api::jobs::impl<SupplierApiGovernanceProcess>::create_health_job` | api_identity, api_create_reference, api_governance, api_read | `27b207ae8a890ae9` |
| `services::supplier_api::governance::jobs::impl<SupplierApiService>::create_catalog_job` | `erp_processes::supply_governance::supplier_api::jobs::impl<SupplierApiGovernanceProcess>::create_catalog_job` | api_identity, api_create_reference, api_governance, api_read | `27b207ae8a890ae9` |
| `services::supplier_api::governance::query::impl<SupplierApiService>::connection_list_for_actor` | `erp_read_models::supplier_center::supplier_api::query::impl<SupplierApiReadService>::connection_list_for_actor` | api_identity, api_create_reference, api_governance, api_read | `5177e2a98d1c43db` |
| `services::supplier_api::governance::query::impl<SupplierApiService>::connection_detail_for_actor` | `erp_read_models::supplier_center::supplier_api::query::impl<SupplierApiReadService>::connection_detail_for_actor` | api_identity, api_create_reference, api_governance, api_read | `5177e2a98d1c43db` |
| `services::supplier_api::governance::query::impl<SupplierApiService>::connection_action_projection` | `erp_read_models::supplier_center::supplier_api::query::impl<SupplierApiReadService>::connection_action_projection` | api_identity, api_create_reference, api_governance, api_read | `5177e2a98d1c43db` |
| `services::supplier_api::impl<SupplierApiService>::new` | `erp_processes::supply_governance::supplier_api::impl<SupplierApiGovernanceProcess>::new`<br>`erp_read_models::supplier_center::supplier_api::impl<SupplierApiReadService>::new`<br>`erp_supply::service::supplier_api::impl<SupplierApiService>::new` | api_identity, api_create_reference, api_governance, api_read | `0836fe35b1f1b9d3`<br>`a75a92ae38eec635`<br>`c7c0156b10321900` |
| `services::supplier_api::impl<SupplierApiService>::with_reference_registry` | `erp_processes::supply_governance::supplier_api::impl<SupplierApiGovernanceProcess>::with_reference_registry`<br>`erp_read_models::supplier_center::supplier_api::impl<SupplierApiReadService>::with_reference_registry` | api_identity, api_create_reference, api_governance, api_read | `0836fe35b1f1b9d3`<br>`a75a92ae38eec635` |
| `services::supplier_api::impl<SupplierApiService>::create_connection` | `erp_processes::supply_governance::supplier_api::creation::impl<SupplierApiGovernanceProcess>::create_connection` | api_identity, api_create_reference, api_governance, api_read | `e0d522c3ff7ac79f` |
| `services::supplier_fulfillment::cancel::impl<SupplierFulfillmentService>::submit_after_sales_action` | `erp_processes::supply_execution::cancel::impl<SupplierFulfillmentProcess>::submit_after_sales_action` | fulfillment_dispatch, fulfillment_callbacks | `bcdab4ca863d8b29` |
| `services::supplier_fulfillment::complete::impl<SupplierFulfillmentService>::complete_order_task` | `erp_processes::supply_execution::complete::impl<SupplierFulfillmentProcess>::complete_order_task` | fulfillment_complete | `1788ab4d9f9acd32` |
| `services::supplier_fulfillment::complete::impl<SupplierFulfillmentService>::replay_task_completion` | `erp_processes::supply_execution::complete::impl<SupplierFulfillmentProcess>::replay_task_completion` | fulfillment_complete | `1788ab4d9f9acd32` |
| `services::supplier_fulfillment::investigate::impl<SupplierFulfillmentService>::execute_investigation` | `erp_processes::supply_execution::investigate::impl<SupplierFulfillmentProcess>::execute_investigation` | fulfillment_investigation, fulfillment_complete | `60808e3f0b99a288` |
| `services::supplier_fulfillment::investigate::impl<SupplierFulfillmentService>::ensure_investigation_intent` | `erp_processes::supply_execution::investigate::impl<SupplierFulfillmentProcess>::ensure_investigation_intent` | fulfillment_investigation, fulfillment_complete | `60808e3f0b99a288` |
| `services::supplier_fulfillment::investigate::impl<SupplierFulfillmentService>::persist_prepared_investigation` | `erp_processes::supply_execution::investigate::impl<SupplierFulfillmentProcess>::persist_prepared_investigation` | fulfillment_investigation, fulfillment_complete | `60808e3f0b99a288` |
| `services::supplier_fulfillment::investigate::impl<SupplierFulfillmentService>::replay_investigation` | `erp_processes::supply_execution::investigate::impl<SupplierFulfillmentProcess>::replay_investigation` | fulfillment_investigation, fulfillment_complete | `60808e3f0b99a288` |
| `services::supplier_fulfillment::investigate::investigation_result` | `erp_processes::supply_execution::investigate::investigation_result` | fulfillment_investigation, fulfillment_complete | `60808e3f0b99a288` |
| `services::supplier_fulfillment::impl<SupplierFulfillmentService>::new` | `erp_processes::supply_execution::impl<SupplierFulfillmentProcess>::new`<br>`erp_supply::service::supplier_fulfillment::impl<SupplierFulfillmentService>::new` | fulfillment_dispatch, fulfillment_callbacks | `b223639067a04d9a`<br>`1c5bb689d9e4af2c` |
| `services::supplier_fulfillment::place::impl<SupplierFulfillmentService>::submit_place` | `erp_processes::supply_execution::place::impl<SupplierFulfillmentProcess>::submit_place` | fulfillment_dispatch, fulfillment_callbacks | `b605f88c5a6675f7` |
| `services::supplier_fulfillment::place::impl<SupplierFulfillmentService>::settle_dispatch` | `erp_processes::supply_execution::place::impl<SupplierFulfillmentProcess>::settle_dispatch` | fulfillment_dispatch, fulfillment_callbacks | `b605f88c5a6675f7` |
| `services::supplier_fulfillment::place::impl<SupplierFulfillmentService>::apply_dispatch_outcome` | `erp_supply::service::supplier_fulfillment::place::impl<SupplierFulfillmentService>::apply_dispatch_outcome` | fulfillment_dispatch, fulfillment_callbacks | `5319762e1c0cb1c8` |
| `services::supplier_fulfillment::place::impl<SupplierFulfillmentService>::write_dispatch_result` | `erp_processes::supply_execution::place::impl<SupplierFulfillmentProcess>::write_dispatch_result` | fulfillment_dispatch, fulfillment_callbacks | `b605f88c5a6675f7` |
| `services::supplier_fulfillment::refund_result::impl<SupplierFulfillmentService>::record_refund_result` | `erp_processes::supply_execution::refund_result::impl<SupplierFulfillmentProcess>::record_refund_result` | fulfillment_dispatch, fulfillment_callbacks | `b655d5dc2b17027f` |
| `services::supplier_fulfillment::refund_result::impl<SupplierFulfillmentService>::build_refund_fact` | `erp_supply::service::supplier_fulfillment::refund_result::impl<SupplierFulfillmentService>::build_refund_fact` | fulfillment_dispatch, fulfillment_callbacks | `f77334a85b472daf` |
| `services::supplier_fulfillment::reject::impl<SupplierFulfillmentService>::record_reject` | `erp_processes::supply_execution::reject::impl<SupplierFulfillmentProcess>::record_reject` | fulfillment_dispatch, fulfillment_callbacks | `edab23d0bb6aabcb` |
| `services::supplier_offering::impl<SupplierOfferingService>::list` | `erp_read_models::supplier_center::offering::impl<SupplierOfferingReadService>::list` | offering_qualification, offering_command, offering_exception_read | `a68b128355e1609b` |
| `services::supplier_offering::impl<SupplierOfferingService>::create` | `erp_processes::supply_governance::offering::command::impl<SupplierOfferingProcess>::create` | offering_qualification, offering_command, offering_exception_read | `7a7beaf448fd2254` |
| `services::supplier_offering::impl<SupplierOfferingService>::revise` | `erp_processes::supply_governance::offering::command::impl<SupplierOfferingProcess>::revise` | offering_qualification, offering_command, offering_exception_read | `7a7beaf448fd2254` |
| `services::supplier_offering::impl<SupplierOfferingService>::update_availability` | `erp_processes::supply_governance::offering::command::impl<SupplierOfferingProcess>::update_availability` | offering_qualification, offering_command, offering_exception_read | `7a7beaf448fd2254` |
| `services::supplier_offering::impl<SupplierOfferingService>::complete_supply_exception_task` | `erp_processes::supply_governance::offering::exception::impl<SupplierOfferingProcess>::complete_supply_exception_task` | offering_qualification, offering_command, offering_exception_read | `d1e1fc89df2625b8` |
| `services::supplier_offering::impl<SupplierOfferingService>::ensure_identity_available` | `erp_supply::service::supplier_offering::impl<SupplierOfferingService>::ensure_identity_available` | offering_qualification, offering_command, offering_exception_read | `b39f87081a5b8dd8` |
| `services::supplier_offering::impl<SupplierOfferingService>::ensure_source_connection` | `erp_supply::service::supplier_offering::impl<SupplierOfferingService>::ensure_source_connection` | offering_qualification, offering_command, offering_exception_read | `b39f87081a5b8dd8` |
| `services::supplier_offering::impl<SupplierOfferingService>::ensure_qualified` | `erp_processes::supply_governance::offering::qualification::impl<QualificationPort for MongoOfferingQualification>::ensure_qualified` | offering_qualification, offering_command, offering_exception_read | `505a74e6b51de649` |
| `services::supplier_offering::impl<SupplierOfferingService>::current_revision_no` | `erp_supply::service::supplier_offering::impl<SupplierOfferingService>::current_revision_no` | offering_qualification, offering_command, offering_exception_read | `b39f87081a5b8dd8` |
| `services::supplier_offering::impl<SupplierOfferingService>::command_record` | `erp_supply::service::supplier_offering::impl<SupplierOfferingService>::command_record` | offering_qualification, offering_command, offering_exception_read | `b39f87081a5b8dd8` |
| `services::supplier_offering::impl<SupplierOfferingService>::resolve_command_result` | `erp_supply::service::supplier_offering::impl<SupplierOfferingService>::resolve_command_result` | offering_qualification, offering_command, offering_exception_read | `b39f87081a5b8dd8` |
| `services::supplier_offering::impl<SupplierOfferingService>::resolve_written_result` | `erp_supply::service::supplier_offering::impl<SupplierOfferingService>::resolve_written_result` | offering_qualification, offering_command, offering_exception_read | `b39f87081a5b8dd8` |
| `services::supplier_settlement::difference::impl<SupplierSettlementService>::decide_difference` | `erp_processes::supply_settlement::difference::impl<SupplierSettlementProcess>::decide_difference` | settlement_difference, settlement_receipts_read | `c970b335b896fc39` |
| `services::supplier_settlement::difference::impl<SupplierSettlementService>::replay_difference_decision` | `erp_processes::supply_settlement::difference::impl<SupplierSettlementProcess>::replay_difference_decision` | settlement_difference, settlement_receipts_read | `c970b335b896fc39` |
| `services::supplier_settlement::draft::impl<SupplierSettlementService>::create_statement` | `erp_processes::supply_settlement::draft::impl<SupplierSettlementProcess>::create_statement` | settlement_draft | `2316d521f4d04e85` |
| `services::supplier_settlement::draft::impl<SupplierSettlementService>::refresh_statement` | `erp_processes::supply_settlement::draft::impl<SupplierSettlementProcess>::refresh_statement` | settlement_draft | `2316d521f4d04e85` |
| `services::supplier_settlement::draft::impl<SupplierSettlementService>::draft_result` | `erp_supply::service::supplier_settlement::draft::impl<SupplierSettlementService>::draft_result` | settlement_draft | `288a7e16f0e66171` |
| `services::supplier_settlement::draft::impl<SupplierSettlementService>::replay_refresh` | `erp_processes::supply_settlement::draft::impl<SupplierSettlementProcess>::replay_refresh` | settlement_draft | `2316d521f4d04e85` |
| `services::supplier_settlement::dto::review::impl<SettlementReviewDecisionData>::parsed_reject_reason` | `erp_supply::dto::supplier_settlement::review::impl<SettlementReviewDecisionData>::parsed_reject_reason` | settlement_review, settlement_receipts_read | `72e58fb132f0148c` |
| `services::supplier_settlement::evidence::impl<SupplierSettlementService>::append_difference_evidence` | `erp_processes::supply_settlement::evidence::impl<SupplierSettlementProcess>::append_difference_evidence` | settlement_difference, settlement_receipts_read | `1faa44616648790b` |
| `services::supplier_settlement::impl<SupplierSettlementService>::load_statement_items` | `erp_supply::service::supplier_settlement::impl<SupplierSettlementService>::load_statement_items` | settlement_receipts_read, settlement_review | `549aba6f548d7872` |
| `services::supplier_settlement::impl<SupplierSettlementService>::load_statement` | `erp_supply::service::supplier_settlement::impl<SupplierSettlementService>::load_statement` | settlement_receipts_read, settlement_review | `549aba6f548d7872` |
| `services::supplier_settlement::query::impl<SupplierSettlementService>::supplier_settlement_statement_detail` | `erp_read_models::supplier_center::settlement::query::impl<SupplierSettlementReadService>::supplier_settlement_statement_detail` | settlement_receipts_read, settlement_review | `24ad428ff7add4e3` |
| `services::supplier_settlement::query::impl<SupplierSettlementService>::settlement_review_work_item_view` | `erp_read_models::supplier_center::settlement::query::impl<SupplierSettlementReadService>::settlement_review_work_item_view` | settlement_receipts_read, settlement_review | `24ad428ff7add4e3` |
| `services::supplier_settlement::query::settlement_object_actions` | `erp_supply::service::supplier_settlement::query::settlement_object_actions` | settlement_receipts_read, settlement_review | `b19e729949c8f40c` |
| `services::supplier_settlement::review::impl<SupplierSettlementService>::submit_review` | `erp_processes::supply_settlement::review::impl<SupplierSettlementProcess>::submit_review` | settlement_review, settlement_receipts_read | `f1eadb0034cd8185` |
| `services::supplier_settlement::review::impl<SupplierSettlementService>::decide_review` | `erp_processes::supply_settlement::review::impl<SupplierSettlementProcess>::decide_review` | settlement_review, settlement_receipts_read | `f1eadb0034cd8185` |
| `services::supplier_settlement::review::impl<SupplierSettlementService>::replay_review_submission` | `erp_processes::supply_settlement::review::impl<SupplierSettlementProcess>::replay_review_submission` | settlement_review, settlement_receipts_read | `f1eadb0034cd8185` |
| `services::supplier_settlement::review::impl<SupplierSettlementService>::replay_review_decision` | `erp_processes::supply_settlement::review::impl<SupplierSettlementProcess>::replay_review_decision` | settlement_review, settlement_receipts_read | `f1eadb0034cd8185` |
| `services::supplier_settlement::review::ensure_settlement_reviewer_eligible` | `erp_supply::service::supplier_settlement::review::ensure_reviewer_separation` | settlement_review, settlement_receipts_read | `30de897288cdd58e` |
| `services::supplier_settlement::review::settlement_review_access` | `erp_supply::service::supplier_settlement::review::settlement_review_access` | settlement_review, settlement_receipts_read | `30de897288cdd58e` |
| `services::supplier_settlement::review::build_settlement_payable` | `erp_finance::service::payable::supplier_settlement::build_settlement_payable` | settlement_review, settlement_receipts_read | `4baa064ae7dbb898` |
| `services::supplier_settlement::source::impl<SupplierSettlementService>::record_source_evidence` | `erp_processes::supply_settlement::source::impl<SupplierSettlementProcess>::record_source_evidence` | settlement_source | `f3504543b6100847` |
| `services::supplier_settlement::source::impl<SupplierSettlementService>::build_source_evidence` | `erp_supply::service::supplier_settlement::source::impl<SupplierSettlementService>::build_source_evidence` | settlement_source | `1dcfc5e884285e5d` |
| `services::supplier_settlement::void::impl<SupplierSettlementService>::void_statement` | `erp_processes::supply_settlement::void::impl<SupplierSettlementProcess>::void_statement` | settlement_difference, settlement_receipts_read | `cedddb7d9b16847c` |
| `services::supplier_settlement::void::impl<SupplierSettlementService>::update_statement_with_audit` | `erp_processes::supply_settlement::void::impl<SupplierSettlementProcess>::update_statement_with_audit` | settlement_difference, settlement_receipts_read | `cedddb7d9b16847c` |

## 静态报告与封存要求

- Raw 输入：`/private/tmp/erp-supply16-contract-sealed/missing-drift-report.json`；SHA256 `10857599382053d1b19c049878b2dec37aff1b07a4b01fca54ad396ba221c6e0`。
- Raw 每个改变名称均按来源路径映射真实 qualified 函数；不根据分类消失断言实现删除，不使用 scanner placeholder 作为业务完成证明。
- after 文件在生成前逐项检查当前字节；并发变更会拒绝生成，必须刷新和实际复核。指定 source-commit 时还核对每个 committed blob。
- 仓储/transaction/Executor/route 基础逐字节一致项见 JSON unchanged_foundations；提交不确定、并发、回滚、索引与外部网络行为不在本静态验证证据等级内。

- 本片 raw 实际来源命中：幂等 73 项，事务 90 项；是逐来源 occurrence 数，不是去重符号名称数。
- 其他阶段同名分类命中 10 项，涉及 5 个文件；输入、当前和源码提交整文件字节一致，见 JSON raw_unchanged_outside_scope。
- Money foundation 3 个符号由 A 独立 Catalog 审核覆盖；证据 `/private/tmp/supply16-catalog-semantic-review.json`，SHA256 `30107ef9405c7170561944b27c53e9035df4ddf98250645358278660b5374a23`，JSON pointer `/raw_foundation_money_disposition`。本记录核对其 source commit 与 sealed raw 均一致后引用，不重复归为 C 的独立审核。
- as_of、SellableSkuFilter、SellableSkuRow 的真实定义仍留 Catalog；A 记录构造主体和完整字段/serde token 一致，金额精度类型与日期求值点保持。静态采集 after money 来源集合遗漏本域合同叶，不等于符号删除。
