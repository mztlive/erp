# 阶段 13 供应商退款与付款冲正分片执行合同

## 1. 输入与验证边界

- 唯一工作树：`/private/tmp/erp-domain-crate-13-returns`；实际输入提交 `de112614`。
- 本文核销本分片源码与生产调用图，不登记阶段验收、Cargo 通过或数据库行为验证。
- 本分片不运行 Cargo/MongoDB/外部服务，不修改历史 `tests/**`，不提交代码。真实数据库运行未验证。
- 旧 `services/src/returns/{supplier_refund,payment_reversal,offset_batch}.rs` 已在 G 确认完成输入提取后删除。
- 原 `reverse_flow/mod.rs` 的 PaymentReversalProcess 已先提取到 `payment_posting.rs`；根改写和唯一导出由 E 完成，本分片没有修改根。

## 2. 入口与所有权合同

| 调用位置 | 最终唯一入口 |
| --- | --- |
| 供应商退款 create/commit/submit/cancel/post standalone | `erp_processes::reverse_flow::ReturnsProcess` 上原同名方法，保原参数和 services::Result。 |
| 付款冲正 create/commit/submit/cancel | 同 ReturnsProcess 原同名方法。 |
| 付款冲正 post standalone / approval transaction | `erp_processes::reverse_flow::PaymentReversalProcess::{post_payment_reversal,post_payment_reversal_in_transaction}`；原 new(db,rbac) 保留。 |
| 供应商退款 approval final | E 根的 `finalize_approved_return_in_transaction` 调 `supplier_refund::apply_supplier_refund_final_post`，不另开事务。 |
| DTO | `erp_returns::dto::{Create/Commit/Submit/Cancel*Request}`；G 唯一所有权。 |
| query/View | `erp_read_models::returns_center::{ReturnsReadService,dto::*}`；Process 在原写后时点调用 `self.reads()`，不复制 query。 |
| HTTP 永拒绝 post | domain ReturnsService 的 `reject_supplier_refund_client_post`、`reject_payment_reversal_client_post` 返回 `Result<Infallible>`；只有原 ConflictError 构造。HTTP 不可达 Ok 分支由 root 消解，不引入 View、panic 或实际过账。 |
| 审批 shared/纯规则 | E 的 `service::{shared,approval,version_conflict}` 唯一实现；Process adapter 继续匹配 ApprovalDomainAction。 |

## 3. 逐符号责任与原行为核销

### 3.1 supplier_refund.rs

| 原符号/范围 | 当前生产实现与要求 |
| --- | --- |
| supplier_refund_detail / supplier_refund_view | G 读模型；旧命令文件中已删除。所有原调用用 `self.reads().supplier_refund_detail` 直接重接。 |
| create_supplier_refund | Process Validate → domain `new_supplier_refund` → 原 persist_created 根 → 原详情读取。构造数据、new ID、请求原字段及 evidence_attachment_id=None 保持。 |
| commit_supplier_refund | 原 CommandReceipt::from_payload/committed_resource_id 在 source 预读前；随后 finance payment NoTransaction → 捕获 version → domain `new_supplier_refund_commit`。最小输入只有 payment ID、supplier ID、amount；不传完整 SupplierPayment。 |
| new_supplier_refund_commit | 原新 ID→GTK 编号→来源字段→reason→amount.unwrap_or→handled actor/reviewed finance_reviewer→Instant::now→entity new 顺序。只为借用原请求克隆 reason，不改变 fingerprint 或后续 key。 |
| submit_supplier_refund | 原 validate/normalize key → preflight exact replay → 当前 load/version → start state → dispatch。旧版本重放保持优先于当前状态/版本。 |
| dispatch_supplier_refund_start | 保留原 binding/context/graph/snapshot/receipt/prepare_start 和 runtime 持久化调用。原被丢弃的 start_command_kind 调用保留；未使用展示常量项由 E/root 统一删除，真实 history cap 唯一归 G。 |
| recover_supplier_refund_start | 仍仅在 command_may_have_committed 后最多 8 次 fresh recovery；原 delay、非恢复错误与原错误返回保留。 |
| replay_supplier_refund_start / replay_supplier_refund_start_version | 原 fresh 事务；active actor → domain 同 session load → supplier/org → 两个 scope → binding → subject version → exact receipt。没有将 load 提前，也没有缓存 source 或合并两条恢复路径。 |
| persist_cancelled_supplier_refund | 原 prepare_cancel、本域动作、audit 构造及 E cancel runtime 调用保留；Replay 仍由 E 实际 adapter 执行 CAS+audit。 |
| load_supplier_refund | 迁 domain 方法，显式接同一 Executor；普通 submit/cancel 仍传 NoTransaction，fresh replay 仍传 fresh session。 |
| supplier_refund_responsible_org / load_supplier_refund_org_id | Process 仍查询供应商本域 provider；保持 NotFound 和 party_id 资格错误，未把 actor 组织作为替代。 |
| post_supplier_refund | 原 standalone root、actor/object_read 准备、内层 final_post 与写后详情时点保留。 |
| persist_created_supplier_refund / persist_bound_supplier_refund_document | 原组织预读→bind command/document/create audit→根内 object readable/binding/document→domain create→audit；不复用原付款绑定。 |
| validate_supplier_refund_source | 根内原 payment 重读→domain shared version-first/posted-second；原中文错误和 expected_version 来源不变。 |
| apply_supplier_refund_final_post | domain prepare（load/Reversed/首次 final guard）→Process 签署动作（原第二次 guard）→财务与本域累计检查→财务写入→domain mark_posted/CAS→原 audit。 |
| apply_supplier_refund_posting | domain original_payment_id（entry-only 原错误）→finance load Posted payment→domain posted_refund_total/上限→finance provider。只给财务 refund_id/amount/occurred_at。 |
| 原 5 个财务 helper | 移到 finance `service/payable/supplier_refund.rs`；实际生产函数保留原名称与规则，数据库/ID/日期由 MongoRefundPosting 实现真实窄 Port。 |

### 3.2 payment_reversal.rs 与原 PaymentReversalProcess

| 原符号/范围 | 当前生产实现与要求 |
| --- | --- |
| payment_reversal_detail / payment_reversal_view | G 读模型；命令文件不再保留两实现。 |
| create_payment_reversal | Process Validate→domain new→原 context/创建根→原 View。 |
| commit_payment_reversal | 原 receipt replay→原 payment NoTransaction/version→domain new commit（payment ID/amount 最小事实）→context 第二次原付款读取→supplier/org→snapshot/binding/runtime/root。根失败仍无条件查询 committed_resource_id，再返回原错或首次结果。 |
| submit_payment_reversal | 原 Validate→adapter→load/version→start；仍没有退款类的 preflight replay 或 8 次恢复。 |
| dispatch_payment_reversal_start | 原冻结读取、snapshot、receipt 和 prepare_start/persist_start 时点；不新增统一恢复器。 |
| cancel_payment_reversal_approval / persist_cancelled_payment_reversal | 原 version、binding/runtime、prepare_cancel、domain action、audit；交 E 实际 cancel adapter 使用同一个 Executor。 |
| load_payment_reversal | 本域显式 Executor 方法；保持原 NotFound。 |
| payment_reversal_context / load_payment_reversal_context | 原先付款后供应商的 NoTransaction 查询顺序，不以 commit 预读替代。 |
| persist_created_payment_reversal / persist_bound_payment_reversal_document | 原 context/doc/audit 构造以及根内 binding→document→domain create→audit，保持对象读取闸门。 |
| validate_payment_reversal_source | 原 source 重读与 version-first/posted-second 规则；来源文案保持“原供应商付款不存在/只有已过账的供应商付款才能发起冲正”。 |
| prepare_payment_reversal_post | 本域 load→Reversed 特定错误→首次 final guard；Process adapter 保留原第二次 final guard。 |
| PaymentReversalProcess::new/post/_in_transaction | 真实实现位于 payment_posting.rs，根只导出；没有第二份原子流程。 |
| 原 apply_payment_reversal_final_post | 同名内部入口实例化 MongoPaymentReversal，并调用生产 execute_final_post；Standalone 和审批外层 session 共用。 |
| 原 apply_payment_reversal_posting | 已由真实 Port 分别组合 payment provider、returns limit、finance 分段、task、returns posted/CAS/audit。 |
| 原 persist_reversal_offsets_and_mark_payment | finance `prepare_payment_reversal` + `PaymentReversalWrite::persist`；中间暴露原 HashSet 给 Process 保留任务插入点。 |
| 原 revert_payment_settlements | finance 同名 helper：batch facts→每 chunk 缺项/条件回冲→同一 HashSet；不排序、不复制到 BTreeSet、不在财务调用 task。 |
| 原 persist_reverse_allocations | finance 同名 helper：原 reverse_rows.zip(seqs)，逐 new ID/new/create；随后原 payment transition Reversed/CAS。 |

### 3.3 offset_batch.rs

- 唯一应付 loader 移入 `erp_finance::service::payable::offset_batch::load_payable_offset_facts`。
- 保留 first-seen entry 去重→一批 entries→按 required ID 建索引→first-seen account 去重→一批 accounts→缺项失败。
- `OffsetFacts`、unique/index 公共算法继续复用同 finance crate 内既有 receivable provider，不复制第二个算法实现。
- 原 7 条纯 batch 测试逐条保留；旧 services loader 与其根出口由迁移删除，不建兼容转发。

## 4. 财务与跨域原序

### 4.1 供应商退款

1. refund 存在/Reversed/InApproval/action。
2. original_payment_id→付款存在/Posted→returns 独立 posted refund total→CumulativeAmountLimit。
3. 全部 allocations→plan_reverse→next seq range→batch entries/accounts。
4. 每 chunk：required entry/account→account.supplier_id==payment.supplier_id→条件回冲。
5. 首次回冲成功后才生成减少分录 ID 与 BusinessDate::today；每 chunk 紧接生成 offset ID 并写 offset。
6. 全部 chunk 成功后才写唯一减少分录，再逐反向 allocation ID/new/create。
7. refund.mark_posted/CAS→audit。原 payment 不变为 Reversed；无付款 task 或销售进度写入。

财务真实 Port 的 production 与替身使用同一 `persist_refund`、`create_decrease_offsets` 和实体 plan_reverse。新增测试同时检查两条分配的反向金额和 offsets 金额为原退款 80、减少分录金额 80、seq 追加 3/4、来源 refund ID、发生时间以及原 payment 仍 Posted。

### 4.2 付款冲正

1. reversal guard/action→原 payment Posted→returns 独立累计 reversal 限额。
2. finance 读取 allocations→plan_reverse→seq range→batch facts→完成全部 settlement 回冲。
3. 将原 HashSet 移交 Process；按该 set 自身迭代顺序逐账户 sync_purchase_payment_task。
4. finance Plan::persist 原反向 allocation 写入→原 payment.Reversed/CAS。
5. returns reversal.mark_posted/CAS→audit。

不得将步骤 3 移到整个 finance persist 之后；不得排序 HashSet 或声称原账户首错按 ID 排序。最终 post 不新增 CommandReceipt；提交/审批原状态、分配规则、BPM receipt 保持各自职责。

## 5. 测试与静态证据

- 原入口 23 项：supplier command8、payment command8、offset batch7；逐文件原名称原顺序全部保留。
- 新增 5 项：PaymentReversalPostingPort 原序/逐步失败2；Supplier finance 真正 IO/ID/date/金额守恒、逐 IO 失败、跨供应商与条件回冲首错3；当前分片共28入口。
- 两种 RecordingPort 均使用非零 `TestExecutor { visits: usize }`，每步断言 data pointer；不使用零尺寸 NoTransaction 地址证明同一 Executor。
- 新测试不访问数据库，不修改或依赖历史 tests target。源码 fixture、include 路径和测试计数已静态核验；执行结果由 root 统一登记。
- 所有新 domain 叶经过外域 import 扫描，未发现旧三层或其他领域依赖；完整 normal/build/dev 依赖门禁由 root 执行。
- 定向 Rustfmt 与 `git diff --check` 通过。全 workspace check/clippy/lib tests 不由本分片运行。
