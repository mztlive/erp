# 阶段 13 退款与冲正幂等/事务合同核验记录

输入提交：`5b83e18ef01d4a36cd1c6862aa0ac38402a0e908`。候选工作树：`/private/tmp/erp-domain-crate-13-returns`。
候选源码提交：`453cd48082793b8f40e5afa37d5d225a747fd1b0`。
采集时间：`2026-09-07T04:38:56.444016+00:00`。

核验结果：本记录覆盖的六类创建、四类 commit/submit/cancel、退款授权回放/有界恢复及退款/冲正最终过账，未发现实质语义漂移。原 24 个 transaction 差异符号均逐 qualified symbol 核销；归档 JSON 保存 before/after 函数与文件 SHA-256、实际 provider 的同级证据。

**证据边界：真实数据库运行未验证。** 本次只读源码复核，未执行 Cargo、Mongo、历史 tests 或 provider 测试。源码顺序、Executor 传递和错误分支核验不得当作并发、回滚或未知结果恢复的数据库运行证明。

本记录对 39 个原 source-map 文件和 3 个已有 process/finance 输入固定 before 副本；258 个原生产函数建立实际落点，其中 80 个改变/抽取函数逐实现复核，178 个原函数的对应生产 body token 相同。查询与 DTO 的完整独立核验仍由读模型分片记录承担。

## CREATE — 六类创建及 NO_APPROVAL

1. 四资金事实：请求校验和实体 ID/构造先完成，随后外层读取组织或资金上下文、构造绑定命令、注册行及 create audit；事务内绑定/注册行 → 本域 create → audit；事务结束后调用对应 detail。普通 create 未新增 commit 专属来源版本和 Posted 门禁。
2. 销售/采购退货仍只构造 req.lines[0]，头 ID/实体早于首行 ID/实体；NO_APPROVAL 政策/无 adapter/空绑定校验 → 注册行 → 头和首行 → 审计。新 MongoCreation 调用的 persist_creation 正是实际创建事务路径；未新增库存、原销售/采购或额度读取。

## COMMIT — 四类一次提交的命令回执与原写序

1. 顺序为 req.validate → CommandReceipt::from_payload(完整 req) → committed_resource_id；已提交立即 detail，未命中才 NoTransaction 来源查询并冻结版本 → 新业务 ID/实体/发生时间 → adapter/start → 组织与对象读取校验 → 独立 now/审批快照/注册行/三条审计构造。
2. 事务内来源重读后先版本再 Posted，客户退款再缺客户校验；随后绑定/注册行 → 同 Executor 定义图 → prepare_start(receipt None) → 本域 create → Apply runtime → create audit → submit audit → command audit。未把 commit 改为 ordinary submit 的 receipt-first 写序。
3. 任何事务错误均执行原 command receipt 回读：命中返回既有 ID，否则原错误；不是仅 OutcomeUnknown 回读。四前缀 customer-refund-commit-/supplier-refund-commit-/receipt-reversal-commit-/payment-reversal-commit-、action/resource 及全请求 payload 保持。
4. TK/GTK/CZ/PCZ 由 SHA256(actor|trim(key)) 前 8 hex 生成；amount None 仍取原金额；ID/occurred_at now 保留原构造位置，snapshot now 仍是另一次时钟读取。

## REFUND_REPLAY — 普通退款提交与授权回放/恢复

1. 客户/供应商 submit 均先校验、规范化 key、授权回放，再 adapter/load/version/start/dispatch；回放未被移到当前状态和版本检查之后。
2. 每个回放事务先检查 actor active 再退款读取，再 customer/supplier 组织事实、active/action DataScope/read DataScope、冻结 binding、严格 subject/version/key/actor/binding identity receipt 与 runtime 匹配；当前版本 > 0 时先当前，再 checked_add(1) 下一版本，溢出返回原 Conflict。
3. ordinary persist-start 错误只在 command_may_have_committed 时进入恢复；最多 8 次 fresh-session 事务，失败可恢复类或 None 才继续，非恢复错误立即返回，延迟在事务外，耗尽返回原错误。客户 helper 内循环，供应商 helper 复用版本回放；未统一两条原有控制结构。

## REVERSAL_SUBMIT — 普通冲正提交

1. receipt/payment reversal：validate → adapter → 原 NoTransaction load → expected_version → start → subject/binding → now → context/snapshot → start command/object read → graph → existing receipt → prepare_start → persist-start → detail。未新增 refund 的预先授权 replay 或 8 次恢复。
2. receipt_reversal_context 旧方法仅转调 load_receipt_reversal_context；新调用在同一位置直接调用该唯一 loader。仍查询原回款后检查 counterparty party，再返回可选 customer；now 仍先于 context 读取。

## START — 普通审批启动事务

1. 四 persist_*_start 在 PreparedExecution::Replay 返回原准备记录，不写库；Apply 构造原 submit audit 后开启唯一根事务。
2. Apply：insert_command_receipt 并调用 map_receipt_first_write_error → mark_approval_started/缺注册行冲突 → runtime 的实例/assignee/execution/snapshot/task → 本域 CAS → submit audit；同一个 session 传入所有仓储。构造 ID 的原 start input 和 runtime helper token 保持，未预分配到更早阶段。

## CANCEL — 撤销审批根、Replay 与审批回调

1. 四 cancel 根仍 validate → NoTransaction load → expected_version → adapter/binding/subject/runtime → now/key/input/prepare_cancel → domain cancel → audit → persist_cancel → detail；原因与 normalized key 使用位置不变。
2. 实际 cancel_write::persist/execute：Apply 才 close_all_for_approval_cancellation → persist_cancelled_runtime → persist_cancelled_approval_tasks；之后 Apply 与 Replay 均执行本域 CAS → audit。MongoCancelWrite 的 record enum 逐类映射到对应原 CAS，未删除 Replay 的既有写入。
3. 审批 cancel_approval_in_transaction 按 document type 读取实体 → execute domain cancel → 对应 CAS → returns.cancel_approval audit；使用调用方 Executor，不开内层事务。

## CUSTOMER_FINAL — 客户退款最终过账及财务 provider

1. refund load → Reversed → InApproval guard → 原 action 再次 guard → original_receipt_id 必需 → receipt exists/Posted → refund-only posted total(排除自己) → 累计上限 → finance → refund.mark_posted/CAS → customer_refund.post audit。entry-only 来源保持原 BusinessLogic 文案。
2. finance MongoRefundStore：allocations → plan_reverse → sequence range → 批量 entries/accounts → 每块 entry/account/同 counterparty 检查 → conditional revert_settlement → 首块才 next_id/today 构造减少分录 → 每块 next_id/create offset → 全块完成才 create 减少分录 → 按 reverse rows 顺序 next_id/create reverse allocations。保留原 occurred_at、source_document_id/source_revision_id=refund ID；不改原回款状态、不刷新销售/任务。
3. 事务中的真实 execute_refund_posting/MongoRefundPosting 依 Finance → Refund → Audit 调用，首错传播；新 store 每个方法直接将同 Executor 传入原 financial repo。

## SUPPLIER_FINAL — 供应商退款最终过账及财务 provider

1. refund load/Reversed/InApproval/action → original_payment_id → payment exists/Posted → refund-only posted total(排除自己) → 累计上限 → finance → mark_posted/CAS → supplier_refund.post audit。
2. finance MongoRefundPosting：allocations → plan/seqs → facts → 每块 entry/account/supplier 一致性 → conditional settlement revert → 首块 ID/today 减少分录 → 每块 ID/offset → entry create → reverse allocations ID/create。next_id/today 的生产 Port 分别直接调用 id_generator::next_id 和 BusinessDate::today，调用位置保留。原付款状态与付款任务均未加入退款路径。

## RECEIPT_FINAL — 回款冲正最终过账与销售刷新

1. 原最终流程继续 prepare returns(Reversed/InApproval/重复 action guard) → load posted receipt → reversal-only posted total/限额 → finance reverse allocations/原 receipt Reversed CAS → returns mark_posted/CAS → audit → 重新查询受影响 sales → 按原排序/去重结果逐单 money refresh，fulfillment=None。
2. refresh_affected_sales 是实际 DatabasePosting::refresh_sales 调用的生产 helper；其 load_sales provider 在 audit 之后调用原 receipt_allocation_sales_order_ids；refresh_sale 原 progress API 与同 Executor 保持。未复用逆转准备阶段分配快照。
3. 原财务 receipt_reversal 算法 token 保持，仅开放被 payable offset_batch 复用的索引 helper；其 reverse rows ID/create 与 receipt state CAS 未被移时。

## PAYMENT_FINAL — 付款冲正最终过账与任务时点

1. 实际 execute_final_post/MongoPaymentReversal：returns prepare(Reversed/InApproval) → action 原重复 guard → payment exists/Posted → reversal-only累计上限 → allocations/plan/seqs → 全部 settlement revert → 原 HashSet 各账户 payment task → reverse allocations ID/create → payment Reversed/CAS → reversal mark_posted/CAS → audit。
2. finance prepare_payment_reversal 只返回原 payment/reverse rows/seqs/amount/occurred_at 与原 HashSet；PaymentReversalWrite::persist 在任务全部成功之后才分配 reverse allocation ID 并写入，之后才 transition/update payment。没有排序 HashSet 或将任务移到反向分配后。
3. 根 post_payment_reversal 与审批 in_transaction 共用 apply_payment_reversal_final_post；根在事务完成后 detail，审批回调直接沿原 session 调用。

## GUARDS — 纯审批规则、来源门禁与错误

1. 新领域 approval/shared 仍执行原 draft→InApproval、cancel→draft、final InApproval 守卫；adapter 仅按原 action 分发并 typed map_err(Error::from)。原无效 action ValidationError 文案保留。
2. 新增 erp_returns::Error 与 erp_finance::Error 转 services::Error 均逐 variant 保留 message/source；OutcomeUnknown、TransientTransaction、ReceiptDuplicate 仍为对应原 typed variant。原 command_may_have_committed 匹配三类未改变；未转换为字符串再判断。
3. 四客户端 post 拒绝函数由原无成功路径泛型返回改为 Infallible，仍恒返回同一 ConflictError 文案；HTTP 的 Ok(never) 不引入生产成功路径。

## READS — 写后详情调用与迁移 token 对照边界

1. 本记录仅核对原写根返回详情的位置，以及旧 list/detail/approval projection 函数迁移后的生产 body token；查询/DTO 的完整独立审查由读模型所有者负责。body token 相同是静态等价证据，不是数据库分页或授权运行证明。

## SUPPORT — 共享装配及批量来源 helper

1. 原绑定/图/收据/审批列表投影及 batch offset 索引函数逐实际符号迁移，原生产 body token 保持；函数位置变化未改变调用根写序。批量 offset 仍先去重 IDs → entries → accounts，first-seen 顺序及错误位置保留。

## 原 24 个事务差异符号逐项核销

表中路径均相对 backend；每项完整 qualified symbol 与全部 SHA-256 见同名 JSON 的 symbols/raw_transaction_coverage。

| 原符号及输入位置 | 候选实现位置 | 适用合同 | 结论 |
|---|---|---|---|
| `services::returns::cancel_approval::persist_customer_refund_cancel`<br>`services/src/returns/cancel_approval.rs:204` | `crates/erp-processes/src/reverse_flow/cancel_approval.rs:202` | CANCEL | 逐实际 provider 核对，无漂移 |
| `services::returns::cancel_approval::persist_supplier_refund_cancel`<br>`services/src/returns/cancel_approval.rs:324` | `crates/erp-processes/src/reverse_flow/cancel_approval.rs:319` | CANCEL | 逐实际 provider 核对，无漂移 |
| `services::returns::cancel_approval::persist_receipt_reversal_cancel`<br>`services/src/returns/cancel_approval.rs:444` | `crates/erp-processes/src/reverse_flow/cancel_approval.rs:436` | CANCEL | 逐实际 provider 核对，无漂移 |
| `services::returns::cancel_approval::persist_payment_reversal_cancel`<br>`services/src/returns/cancel_approval.rs:564` | `crates/erp-processes/src/reverse_flow/cancel_approval.rs:553` | CANCEL | 逐实际 provider 核对，无漂移 |
| `services::returns::customer_refund::impl<ReturnsService>::commit_customer_refund`<br>`services/src/returns/customer_refund.rs:208` | `crates/erp-processes/src/reverse_flow/customer_refund.rs:91` | COMMIT | 逐实际 provider 核对，无漂移 |
| `services::returns::customer_refund::impl<ReturnsService>::recover_customer_refund_start`<br>`services/src/returns/customer_refund.rs:501` | `crates/erp-processes/src/reverse_flow/customer_refund/start.rs:73` | REFUND_REPLAY | 逐实际 provider 核对，无漂移 |
| `services::returns::customer_refund::impl<ReturnsService>::replay_customer_refund_start`<br>`services/src/returns/customer_refund.rs:578` | `crates/erp-processes/src/reverse_flow/customer_refund/start.rs:147` | REFUND_REPLAY | 逐实际 provider 核对，无漂移 |
| `services::returns::customer_refund::impl<ReturnsService>::post_customer_refund`<br>`services/src/returns/customer_refund.rs:733` | `crates/erp-processes/src/reverse_flow/customer_refund.rs:305` | CUSTOMER_FINAL | 逐实际 provider 核对，无漂移 |
| `services::returns::customer_refund::persist_created_customer_refund`<br>`services/src/returns/customer_refund.rs:829` | `crates/erp-processes/src/reverse_flow/customer_refund.rs:421` | CREATE | 逐实际 provider 核对，无漂移 |
| `services::returns::payment_reversal::impl<ReturnsService>::commit_payment_reversal`<br>`services/src/returns/payment_reversal.rs:147` | `crates/erp-processes/src/reverse_flow/payment_reversal.rs:97` | COMMIT | 逐实际 provider 核对，无漂移 |
| `services::returns::payment_reversal::persist_created_payment_reversal`<br>`services/src/returns/payment_reversal.rs:551` | `crates/erp-processes/src/reverse_flow/payment_reversal.rs:435` | CREATE | 逐实际 provider 核对，无漂移 |
| `services::returns::purchase_return::persist_created_purchase_return_order`<br>`services/src/returns/purchase_return.rs:370` | `crates/erp-processes/src/reverse_flow/purchase_return.rs:242` | CREATE | 逐实际 provider 核对，无漂移 |
| `services::returns::receipt_reversal::impl<ReturnsService>::commit_receipt_reversal`<br>`services/src/returns/receipt_reversal.rs:174` | `crates/erp-processes/src/reverse_flow/receipt_reversal/commit.rs:40` | COMMIT | 逐实际 provider 核对，无漂移 |
| `services::returns::receipt_reversal::persist_created_receipt_reversal`<br>`services/src/returns/receipt_reversal.rs:588` | `crates/erp-processes/src/reverse_flow/receipt_reversal/create.rs:63` | CREATE | 逐实际 provider 核对，无漂移 |
| `services::returns::sales_return::persist_created_sales_return_case`<br>`services/src/returns/sales_return.rs:402` | `crates/erp-processes/src/reverse_flow/sales_return.rs:246` | CREATE | 逐实际 provider 核对，无漂移 |
| `services::returns::start_approval::customer_refund::persist_customer_refund_start`<br>`services/src/returns/start_approval/customer_refund.rs:262` | `crates/erp-processes/src/reverse_flow/start_approval/customer_refund.rs:261` | START | 逐实际 provider 核对，无漂移 |
| `services::returns::start_approval::payment_reversal::persist_payment_reversal_start`<br>`services/src/returns/start_approval/payment_reversal.rs:263` | `crates/erp-processes/src/reverse_flow/start_approval/payment_reversal.rs:262` | START | 逐实际 provider 核对，无漂移 |
| `services::returns::start_approval::receipt_reversal::persist_receipt_reversal_start`<br>`services/src/returns/start_approval/receipt_reversal.rs:264` | `crates/erp-processes/src/reverse_flow/start_approval/receipt_reversal.rs:263` | START | 逐实际 provider 核对，无漂移 |
| `services::returns::start_approval::supplier_refund::persist_supplier_refund_start`<br>`services/src/returns/start_approval/supplier_refund.rs:262` | `crates/erp-processes/src/reverse_flow/start_approval/supplier_refund.rs:261` | START | 逐实际 provider 核对，无漂移 |
| `services::returns::supplier_refund::impl<ReturnsService>::commit_supplier_refund`<br>`services/src/returns/supplier_refund.rs:140` | `crates/erp-processes/src/reverse_flow/supplier_refund.rs:98` | COMMIT | 逐实际 provider 核对，无漂移 |
| `services::returns::supplier_refund::impl<ReturnsService>::replay_supplier_refund_start`<br>`services/src/returns/supplier_refund.rs:456` | `crates/erp-processes/src/reverse_flow/supplier_refund.rs:394` | REFUND_REPLAY | 逐实际 provider 核对，无漂移 |
| `services::returns::supplier_refund::impl<ReturnsService>::replay_supplier_refund_start_version`<br>`services/src/returns/supplier_refund.rs:523` | `crates/erp-processes/src/reverse_flow/supplier_refund.rs:459` | REFUND_REPLAY | 逐实际 provider 核对，无漂移 |
| `services::returns::supplier_refund::impl<ReturnsService>::post_supplier_refund`<br>`services/src/returns/supplier_refund.rs:673` | `crates/erp-processes/src/reverse_flow/supplier_refund.rs:595` | SUPPLIER_FINAL | 逐实际 provider 核对，无漂移 |
| `services::returns::supplier_refund::persist_created_supplier_refund`<br>`services/src/returns/supplier_refund.rs:748` | `crates/erp-processes/src/reverse_flow/supplier_refund.rs:625` | CREATE | 逐实际 provider 核对，无漂移 |

## 其余改变/抽取符号逐项核销

每项所引用合同包含实际生产 provider；对应 provider 的路径、行号、原文函数哈希与 body token 哈希均固定在 JSON contracts 中。

- `services::returns::adapter::customer_refund::execute_customer_refund_domain_action`（`services/src/returns/adapter/customer_refund.rs:227`）→ `erp_processes::reverse_flow::adapter::customer_refund::execute_customer_refund_domain_action`（`crates/erp-processes/src/reverse_flow/adapter/customer_refund.rs:189`）。合同 `GUARDS`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::adapter::payment_reversal::execute_payment_reversal_domain_action`（`services/src/returns/adapter/payment_reversal.rs:229`）→ `erp_processes::reverse_flow::adapter::payment_reversal::execute_payment_reversal_domain_action`（`crates/erp-processes/src/reverse_flow/adapter/payment_reversal.rs:193`）。合同 `GUARDS`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::adapter::receipt_reversal::execute_receipt_reversal_domain_action`（`services/src/returns/adapter/receipt_reversal.rs:229`）→ `erp_processes::reverse_flow::adapter::receipt_reversal::execute_receipt_reversal_domain_action`（`crates/erp-processes/src/reverse_flow/adapter/receipt_reversal.rs:193`）。合同 `GUARDS`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::adapter::supplier_refund::execute_supplier_refund_domain_action`（`services/src/returns/adapter/supplier_refund.rs:227`）→ `erp_processes::reverse_flow::adapter::supplier_refund::execute_supplier_refund_domain_action`（`crates/erp-processes/src/reverse_flow/adapter/supplier_refund.rs:191`）。合同 `GUARDS`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::customer_refund::impl<ReturnsService>::create_customer_refund`（`services/src/returns/customer_refund.rs:169`）→ `erp_processes::reverse_flow::customer_refund::impl<ReturnsProcess>::create_customer_refund`（`crates/erp-processes/src/reverse_flow/customer_refund.rs:70`）。合同 `CREATE`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::customer_refund::impl<ReturnsService>::submit_customer_refund`（`services/src/returns/customer_refund.rs:364`）→ `erp_processes::reverse_flow::customer_refund::impl<ReturnsProcess>::submit_customer_refund`（`crates/erp-processes/src/reverse_flow/customer_refund.rs:233`）。合同 `REFUND_REPLAY`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::customer_refund::impl<ReturnsService>::cancel_customer_refund_approval`（`services/src/returns/customer_refund.rs:404`）→ `erp_processes::reverse_flow::customer_refund::impl<ReturnsProcess>::cancel_customer_refund_approval`（`crates/erp-processes/src/reverse_flow/customer_refund.rs:273`）。合同 `CANCEL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::customer_refund::impl<ReturnsService>::dispatch_customer_refund_start`（`services/src/returns/customer_refund.rs:435`）→ `erp_processes::reverse_flow::customer_refund::start::impl<ReturnsProcess>::dispatch_customer_refund_start`（`crates/erp-processes/src/reverse_flow/customer_refund/start.rs:8`）。合同 `REFUND_REPLAY`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::customer_refund::impl<ReturnsService>::load_customer_refund`（`services/src/returns/customer_refund.rs:699`）→ `erp_returns::service::customer_refund::impl<ReturnsService>::load_customer_refund`（`crates/erp-returns/src/service/customer_refund.rs:90`）。合同 `REFUND_REPLAY`；原 load 内置 NoTransaction 改为显式 Executor；submit/cancel 原位置仍传 NoTransaction，refund replay/recovery 原事务查询位置传已有 session。对应 find_by_id 与 NotFound 文案保持。
- `services::returns::customer_refund::apply_customer_refund_final_post`（`services/src/returns/customer_refund.rs:792`）→ `erp_processes::reverse_flow::customer_refund::apply_customer_refund_final_post`（`crates/erp-processes/src/reverse_flow/customer_refund.rs:329`）。合同 `CUSTOMER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::customer_refund::validate_customer_refund_source`（`services/src/returns/customer_refund.rs:899`）→ `erp_processes::reverse_flow::customer_refund::validate_customer_refund_source`（`crates/erp-processes/src/reverse_flow/customer_refund.rs:493`）。合同 `COMMIT`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::customer_refund::apply_customer_refund_posting`（`services/src/returns/customer_refund.rs:960`）→ `erp_processes::reverse_flow::customer_refund::apply_customer_refund_posting`（`crates/erp-processes/src/reverse_flow/customer_refund.rs:548`）。合同 `CUSTOMER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::customer_refund::persist_refund_offsets_and_reversals`（`services/src/returns/customer_refund.rs:991`）→ `erp_finance::service::receivable::customer_refund::persist_refund_offsets_and_reversals`（`crates/erp-finance/src/service/receivable/customer_refund.rs:142`）。合同 `CUSTOMER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::customer_refund::create_decrease_offsets`（`services/src/returns/customer_refund.rs:1030`）→ `erp_finance::service::receivable::customer_refund::create_decrease_offsets`（`crates/erp-finance/src/service/receivable/customer_refund.rs:187`）。合同 `CUSTOMER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::customer_refund::revert_customer_refund_settlement`（`services/src/returns/customer_refund.rs:1071`）→ `erp_finance::service::receivable::customer_refund::revert_customer_refund_settlement`（`crates/erp-finance/src/service/receivable/customer_refund.rs:222`）。合同 `CUSTOMER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::customer_refund::build_customer_refund_decrease_entry`（`services/src/returns/customer_refund.rs:1092`）→ `erp_finance::service::receivable::customer_refund::build_customer_refund_decrease_entry`（`crates/erp-finance/src/service/receivable/customer_refund.rs:239`）。合同 `CUSTOMER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::customer_refund::persist_customer_refund_decrease_offset`（`services/src/returns/customer_refund.rs:1117`）→ `erp_finance::service::receivable::customer_refund::persist_customer_refund_decrease_offset`（`crates/erp-finance/src/service/receivable/customer_refund.rs:263`）。合同 `CUSTOMER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::cancel_approval_in_transaction`（`services/src/returns/mod.rs:190`）→ `erp_processes::reverse_flow::cancel_approval_in_transaction`（`crates/erp-processes/src/reverse_flow/mod.rs:121`）。合同 `CANCEL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::payment_reversal::impl<ReturnsService>::prepare_payment_reversal_post`（`services/src/returns/payment_reversal.rs:57`）→ `erp_returns::service::payment_reversal::impl<ReturnsService>::prepare_payment_reversal_post`（`crates/erp-returns/src/service/payment_reversal.rs:105`）。合同 `PAYMENT_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::payment_reversal::impl<ReturnsService>::create_payment_reversal`（`services/src/returns/payment_reversal.rs:111`）→ `erp_processes::reverse_flow::payment_reversal::impl<ReturnsProcess>::create_payment_reversal`（`crates/erp-processes/src/reverse_flow/payment_reversal.rs:75`）。合同 `CREATE`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::payment_reversal::impl<ReturnsService>::submit_payment_reversal`（`services/src/returns/payment_reversal.rs:299`）→ `erp_processes::reverse_flow::payment_reversal::impl<ReturnsProcess>::submit_payment_reversal`（`crates/erp-processes/src/reverse_flow/payment_reversal.rs:244`）。合同 `REVERSAL_SUBMIT`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::payment_reversal::impl<ReturnsService>::cancel_payment_reversal_approval`（`services/src/returns/payment_reversal.rs:329`）→ `erp_processes::reverse_flow::payment_reversal::impl<ReturnsProcess>::cancel_payment_reversal_approval`（`crates/erp-processes/src/reverse_flow/payment_reversal.rs:277`）。合同 `CANCEL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::payment_reversal::impl<ReturnsService>::dispatch_payment_reversal_start`（`services/src/returns/payment_reversal.rs:360`）→ `erp_processes::reverse_flow::payment_reversal::impl<ReturnsProcess>::dispatch_payment_reversal_start`（`crates/erp-processes/src/reverse_flow/payment_reversal.rs:298`）。合同 `REVERSAL_SUBMIT`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::payment_reversal::impl<ReturnsService>::load_payment_reversal`（`services/src/returns/payment_reversal.rs:479`）→ `erp_returns::service::payment_reversal::impl<ReturnsService>::load_payment_reversal`（`crates/erp-returns/src/service/payment_reversal.rs:72`）。合同 `REVERSAL_SUBMIT`；原 load 内置 NoTransaction 改为显式 Executor；submit/cancel 原位置仍传 NoTransaction，refund replay/recovery 原事务查询位置传已有 session。对应 find_by_id 与 NotFound 文案保持。
- `services::returns::payment_reversal::validate_payment_reversal_source`（`services/src/returns/payment_reversal.rs:630`）→ `erp_processes::reverse_flow::payment_reversal::validate_payment_reversal_source`（`crates/erp-processes/src/reverse_flow/payment_reversal.rs:516`）。合同 `COMMIT`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::purchase_return::impl<ReturnsService>::create_purchase_return_order`（`services/src/returns/purchase_return.rs:106`）→ `erp_processes::reverse_flow::purchase_return::impl<ReturnsProcess>::create_purchase_return_order`（`crates/erp-processes/src/reverse_flow/purchase_return.rs:42`）。合同 `CREATE`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::receipt_reversal::impl<ReturnsService>::prepare_receipt_reversal_post`（`services/src/returns/receipt_reversal.rs:54`）→ `erp_returns::service::receipt_reversal::impl<ReturnsService>::prepare_receipt_reversal_post`（`crates/erp-returns/src/service/receipt_reversal.rs:104`）。合同 `RECEIPT_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::receipt_reversal::impl<ReturnsService>::persist_posted_receipt_reversal`（`services/src/returns/receipt_reversal.rs:95`）→ `erp_returns::service::receipt_reversal::impl<ReturnsService>::persist_posted_receipt_reversal`（`crates/erp-returns/src/service/receipt_reversal.rs:143`）。合同 `RECEIPT_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::receipt_reversal::impl<ReturnsService>::create_receipt_reversal`（`services/src/returns/receipt_reversal.rs:138`）→ `erp_processes::reverse_flow::receipt_reversal::create::impl<ReturnsProcess>::create_receipt_reversal`（`crates/erp-processes/src/reverse_flow/receipt_reversal/create.rs:38`）。合同 `CREATE`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::receipt_reversal::impl<ReturnsService>::submit_receipt_reversal`（`services/src/returns/receipt_reversal.rs:331`）→ `erp_processes::reverse_flow::receipt_reversal::approval::impl<ReturnsProcess>::submit_receipt_reversal`（`crates/erp-processes/src/reverse_flow/receipt_reversal/approval.rs:51`）。合同 `REVERSAL_SUBMIT`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::receipt_reversal::impl<ReturnsService>::cancel_receipt_reversal_approval`（`services/src/returns/receipt_reversal.rs:361`）→ `erp_processes::reverse_flow::receipt_reversal::approval::impl<ReturnsProcess>::cancel_receipt_reversal_approval`（`crates/erp-processes/src/reverse_flow/receipt_reversal/approval.rs:80`）。合同 `CANCEL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::receipt_reversal::impl<ReturnsService>::dispatch_receipt_reversal_start`（`services/src/returns/receipt_reversal.rs:392`）→ `erp_processes::reverse_flow::receipt_reversal::approval::impl<ReturnsProcess>::dispatch_receipt_reversal_start`（`crates/erp-processes/src/reverse_flow/receipt_reversal/approval.rs:98`）。合同 `REVERSAL_SUBMIT`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::receipt_reversal::impl<ReturnsService>::receipt_reversal_context`（`services/src/returns/receipt_reversal.rs:528`）→ `erp_processes::reverse_flow::receipt_reversal::context::load_receipt_reversal_context`（`crates/erp-processes/src/reverse_flow/receipt_reversal/context.rs:19`）。合同 `REVERSAL_SUBMIT`；原 self 方法仅转调 load_receipt_reversal_context；after 调用者直接转调同一 loader，无查询/now 位移。
- `services::returns::receipt_reversal::validate_receipt_reversal_source`（`services/src/returns/receipt_reversal.rs:662`）→ `erp_processes::reverse_flow::receipt_reversal::commit::validate_receipt_reversal_source`（`crates/erp-processes/src/reverse_flow/receipt_reversal/commit.rs:171`）。合同 `COMMIT`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::sales_return::impl<ReturnsService>::create_sales_return_case`（`services/src/returns/sales_return.rs:107`）→ `erp_processes::reverse_flow::sales_return::impl<ReturnsProcess>::create_sales_return_case`（`crates/erp-processes/src/reverse_flow/sales_return.rs:46`）。合同 `CREATE`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::start_approval::prepare::ensure_return_start_actor_active`（`services/src/returns/start_approval/prepare.rs:24`）→ `erp_processes::reverse_flow::start_approval::prepare::ensure_return_start_actor_active`（`crates/erp-processes/src/reverse_flow/start_approval/prepare.rs:20`）。合同 `REFUND_REPLAY`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::start_approval::prepare::ensure_return_start_replay_authorized`（`services/src/returns/start_approval/prepare.rs:43`）→ `erp_processes::reverse_flow::start_approval::prepare::ensure_return_start_replay_authorized`（`crates/erp-processes/src/reverse_flow/start_approval/prepare.rs:30`）。合同 `REFUND_REPLAY`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::supplier_refund::impl<ReturnsService>::create_supplier_refund`（`services/src/returns/supplier_refund.rs:101`）→ `erp_processes::reverse_flow::supplier_refund::impl<ReturnsProcess>::create_supplier_refund`（`crates/erp-processes/src/reverse_flow/supplier_refund.rs:76`）。合同 `CREATE`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::supplier_refund::impl<ReturnsService>::submit_supplier_refund`（`services/src/returns/supplier_refund.rs:292`）→ `erp_processes::reverse_flow::supplier_refund::impl<ReturnsProcess>::submit_supplier_refund`（`crates/erp-processes/src/reverse_flow/supplier_refund.rs:243`）。合同 `REFUND_REPLAY`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::supplier_refund::impl<ReturnsService>::cancel_supplier_refund_approval`（`services/src/returns/supplier_refund.rs:332`）→ `erp_processes::reverse_flow::supplier_refund::impl<ReturnsProcess>::cancel_supplier_refund_approval`（`crates/erp-processes/src/reverse_flow/supplier_refund.rs:283`）。合同 `CANCEL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::supplier_refund::impl<ReturnsService>::dispatch_supplier_refund_start`（`services/src/returns/supplier_refund.rs:363`）→ `erp_processes::reverse_flow::supplier_refund::impl<ReturnsProcess>::dispatch_supplier_refund_start`（`crates/erp-processes/src/reverse_flow/supplier_refund.rs:301`）。合同 `REFUND_REPLAY`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::supplier_refund::impl<ReturnsService>::load_supplier_refund`（`services/src/returns/supplier_refund.rs:639`）→ `erp_returns::service::supplier_refund::impl<ReturnsService>::load_supplier_refund`（`crates/erp-returns/src/service/supplier_refund.rs:80`）。合同 `REFUND_REPLAY`；原 load 内置 NoTransaction 改为显式 Executor；submit/cancel 原位置仍传 NoTransaction，refund replay/recovery 原事务查询位置传已有 session。对应 find_by_id 与 NotFound 文案保持。
- `services::returns::supplier_refund::validate_supplier_refund_source`（`services/src/returns/supplier_refund.rs:818`）→ `erp_processes::reverse_flow::supplier_refund::validate_supplier_refund_source`（`crates/erp-processes/src/reverse_flow/supplier_refund.rs:697`）。合同 `COMMIT`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::supplier_refund::apply_supplier_refund_final_post`（`services/src/returns/supplier_refund.rs:873`）→ `erp_processes::reverse_flow::supplier_refund::apply_supplier_refund_final_post`（`crates/erp-processes/src/reverse_flow/supplier_refund.rs:752`）。合同 `SUPPLIER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::supplier_refund::apply_supplier_refund_posting`（`services/src/returns/supplier_refund.rs:908`）→ `erp_processes::reverse_flow::supplier_refund::apply_supplier_refund_posting`（`crates/erp-processes/src/reverse_flow/supplier_refund.rs:782`）。合同 `SUPPLIER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::supplier_refund::persist_refund_offsets_and_reversals`（`services/src/returns/supplier_refund.rs:939`）→ `erp_finance::service::payable::supplier_refund::persist_refund_offsets_and_reversals`（`crates/erp-finance/src/service/payable/supplier_refund.rs:52`）。合同 `SUPPLIER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::supplier_refund::create_decrease_offsets`（`services/src/returns/supplier_refund.rs:978`）→ `erp_finance::service::payable::supplier_refund::create_decrease_offsets`（`crates/erp-finance/src/service/payable/supplier_refund.rs:98`）。合同 `SUPPLIER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::supplier_refund::revert_supplier_refund_settlement`（`services/src/returns/supplier_refund.rs:1018`）→ `erp_finance::service::payable::supplier_refund::revert_supplier_refund_settlement`（`crates/erp-finance/src/service/payable/supplier_refund.rs:141`）。合同 `SUPPLIER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::supplier_refund::build_decrease_entry`（`services/src/returns/supplier_refund.rs:1039`）→ `erp_finance::service::payable::supplier_refund::build_decrease_entry`（`crates/erp-finance/src/service/payable/supplier_refund.rs:161`）。合同 `SUPPLIER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `services::returns::supplier_refund::persist_decrease_offset`（`services/src/returns/supplier_refund.rs:1064`）→ `erp_finance::service::payable::supplier_refund::persist_decrease_offset`（`crates/erp-finance/src/service/payable/supplier_refund.rs:187`）。合同 `SUPPLIER_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `erp_processes::reverse_flow::apply_payment_reversal_final_post`（`crates/erp-processes/src/reverse_flow/mod.rs:88`）→ `erp_processes::reverse_flow::payment_posting::apply_payment_reversal_final_post`（`crates/erp-processes/src/reverse_flow/payment_posting.rs:83`）。合同 `PAYMENT_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `erp_processes::reverse_flow::apply_payment_reversal_posting`（`crates/erp-processes/src/reverse_flow/mod.rs:112`）→ `erp_processes::reverse_flow::payment_posting::execute_final_post`（`crates/erp-processes/src/reverse_flow/payment_posting.rs:128`）。合同 `PAYMENT_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `erp_processes::reverse_flow::persist_reversal_offsets_and_mark_payment`（`crates/erp-processes/src/reverse_flow/mod.rs:140`）→ `erp_finance::service::payable::payment_reversal::prepare_payment_reversal`（`crates/erp-finance/src/service/payable/payment_reversal.rs:55`）；`erp_finance::service::payable::payment_reversal::impl<PaymentReversalWrite>::persist`（`crates/erp-finance/src/service/payable/payment_reversal.rs:82`）。合同 `PAYMENT_FINAL`；拆为 prepare_payment_reversal 与 PaymentReversalWrite::persist；外层在二者之间同步原 affected_accounts 任务，ID/分配写/原付款状态迁移保留原时点。
- `erp_processes::reverse_flow::revert_payment_settlements`（`crates/erp-processes/src/reverse_flow/mod.rs:165`）→ `erp_finance::service::payable::payment_reversal::revert_payment_settlements`（`crates/erp-finance/src/service/payable/payment_reversal.rs:103`）。合同 `PAYMENT_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `erp_processes::reverse_flow::receipt_reversal::impl<ReceiptReversalPosting for DatabasePosting < ' _ >>::post_reversal`（`crates/erp-processes/src/reverse_flow/receipt_reversal.rs:134`）→ `erp_processes::reverse_flow::receipt_reversal::impl<ReceiptReversalPosting for DatabasePosting < ' _ >>::post_reversal`（`crates/erp-processes/src/reverse_flow/receipt_reversal.rs:147`）。合同 `RECEIPT_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。
- `erp_processes::reverse_flow::receipt_reversal::impl<ReceiptReversalPosting for DatabasePosting < ' _ >>::refresh_sales`（`crates/erp-processes/src/reverse_flow/receipt_reversal.rs:146`）→ `erp_processes::reverse_flow::receipt_reversal::impl<ReceiptReversalPosting for DatabasePosting < ' _ >>::refresh_sales`（`crates/erp-processes/src/reverse_flow/receipt_reversal.rs:159`）。合同 `RECEIPT_FINAL`；已对照原函数及新实际 provider；抽取/分域/typed error 映射保持引用合同中的查询、时点和首错顺序。

## 基础实现与集成入口证据

| 路径 | 前后状态 | 核验要求 |
|---|---|---|
| `crates/application-core/src/command.rs` | 字节一致 | unchanged_source |
| `crates/erp-audit/src/service.rs` | 字节一致 | unchanged_source |
| `crates/erp-workflow/src/service/approval/execution/idempotency.rs` | 字节一致 | unchanged_source |
| `crates/persistence-core/src/executor.rs` | 字节一致 | unchanged_source |
| `crates/persistence-core/src/transaction.rs` | 字节一致 | unchanged_source |
| `services/src/errors.rs` | 抽取或接线变化 | reviewed typed error conversion / root provider relocation; same ordered production path |
| `crates/erp-finance/src/error.rs` | 字节一致 | unchanged_source |
| `crates/erp-processes/src/approval_dispatch/action_registry.rs` | 抽取或接线变化 | reviewed typed error conversion / root provider relocation; same ordered production path |
| `apps/web-api/src/core/handler/returns/mod.rs` | 抽取或接线变化 | reviewed typed error conversion / root provider relocation; same ordered production path |
| `crates/erp-returns/src/error.rs` | 抽取或接线变化 | new domain error converts repository classes and maps every variant back to services without text matching |

## 提交前复验约束

1. 对 JSON `after_files` 与 `foundation_evidence` 的候选文件 SHA-256 逐项复验；有变化时重新检查涉及函数或 provider。
2. 保留静态源码核验与根集成者执行的纯测试、真实数据库证据的独立状态；不得用本记录覆盖未运行项目。
3. 不得根据 scanner 的幂等分类是否命中判断退款回执链成立；本记录以四 commit、四 submit、退款 replay/recovery 和真实 runtime/provider 源码为准。
4. 任何后续改动不得重排来源校验、ID/时钟、任务、审计，或新增内层事务/NoTransaction 替换原写 Executor。
