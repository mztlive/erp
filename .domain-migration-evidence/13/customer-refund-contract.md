# 阶段 13 客户退款迁移合同

## 1. 输入、范围和验收边界

- 输入提交必须为 `de112614e03714bb6d1ca4ac9e25f580c5c273c8`；After 为 `/private/tmp/erp-domain-crate-13-returns` 当前未提交工作树。
- 原文件为 `backend/services/src/returns/customer_refund.rs`。命令、领域及财务写入由本分片迁移；查询、View、请求 DTO 定义分别由统一读模型及 DTO 分片迁移，禁止留下旧 Service 转发层。
- 本合同记录源级核对及实际生产算法的替身测试边界。不得将测试源码存在或静态检查通过表述为 MongoDB 事务运行证明；统一 Cargo、Clippy 和 lib 测试结果由根执行者单独登记。
- 回款冲正必须遵守 `/private/tmp/returns13-receipt-reversal-contract.md`，不得把客户退款的重放恢复规则复制给冲正。

## 2. 唯一符号归属

| 原符号或代码段 | 唯一迁后归属 | 调用合同 |
| --- | --- | --- |
| `create_customer_refund` / `commit_customer_refund` / `submit_customer_refund` / `cancel_customer_refund_approval` / `post_customer_refund` | `erp_processes::reverse_flow::ReturnsProcess`；叶 `customer_refund.rs` | 保持原请求、审计人及 View 返回；写后经 `self.reads()` 唯一读取详情 |
| 创建请求校验、退款 `next_id/new` | `erp_returns::service::ReturnsService::prepare_customer_refund` | 所有原 DTO 字段、None 默认及领域错误保持 |
| 一体提交客户缺项检查、退款 `next_id/new` | `ReturnsService::prepare_committed_customer_refund` | 接收请求引用及 `CustomerRefundSourceFact`；不提前检查来源 Posted |
| 本域 load/create/update | `ReturnsService::{load_customer_refund,create_customer_refund_in_transaction,persist_customer_refund}` | 实例方法；接受调用方 `&mut dyn Executor`；内部不新建事务 |
| 最终状态检查和 CAS | `ReturnsService::{prepare_customer_refund_final_post,persist_posted_customer_refund}` | Reversed 特定错误先于 InApproval 检查；财务成功后才 mark_posted/update |
| 原来源 ID 与累计额度 | `ReturnsService::{customer_refund_receipt_id,validate_customer_refund_amount}` | 独立读取退款累计，排除当前退款；不合并回款冲正累计 |
| 来源事务复验 | `erp_returns::service::customer_refund::ensure_customer_refund_source` | 源 version → Posted → customer 缺项；错误类别、文案保持 |
| `reject_client_post` | `ReturnsService::reject_client_post` | 返回 `erp_returns::Result<Infallible>`，唯一原 ConflictError；HTTP 成功分支静态不可达 |
| 启动、8 次恢复、授权回放、撤回编排 | `processes/reverse_flow/customer_refund/start.rs` | 原 `dispatch_*` / `recover_*` / `replay_*` / `persist_cancelled_*` 语义；根入口需要者仅 `pub(super)` |
| `apply_customer_refund_final_post` | `processes/reverse_flow/customer_refund.rs` | 同一 Executor；本域 prepare → 原签署动作分派 → Finance → Refund → Audit |
| 退款资金原回款预读/最终读取 | `erp_finance::service::receivable::customer_refund::{load_customer_refund_source,load_posted_refund_receipt}` | 保留两处不同的 NotFound/Posted 文案和各自查询时点 |
| `persist_refund_offsets_and_reversals` 及其全部实际财务算法 | `erp_finance::service::receivable::customer_refund` | 只接财务实体及 `CustomerRefundPosting`；无 returns 聚合依赖 |
| `customer_refund_list/detail/view` | `erp_read_models::returns_center::ReturnsReadService` | 唯一查询装配实现；详情缺 registry 的兼容分支、列表批量 registry 读取保持 |
| 请求 DTO | `erp_returns::dto::*` | 原字段、serde、normalize/validator 不变 |
| 列表/Query/View/Approval DTO | `erp_read_models::returns_center::dto::*` | JSON 合同不变；命令仅以结果类型使用 View |

`ReturnsProcess` 根构造器、shared helpers、审批 adapter、start/cancel runtime 由 E 分片维护。所有本域写入调用均指向真实 `erp-returns` 服务，不通过旧 services 的透明委托。

## 3. 消费方事实合同

### 3.1 退款消费的原回款事实

`erp_returns::service::customer_refund::CustomerRefundSourceFact` 必须只含：

| 字段 | 类型 | 语义 |
| --- | --- | --- |
| `id` | `CustomerReceiptId` | 原资金身份 |
| `version` | `u64` | 原查询得到的实体版本 |
| `amount` | `Amount` | 未填写退款金额时的原默认金额 |
| `customer_id` | `Option<CustomerAccountId>` | 必须保留可空事实，原位置才报缺项 |
| `is_posted` | `bool` | 流程对财务 `CustomerReceiptStatus::Posted` 显式比较 |

实际 provider 必须为 `erp-finance` 的原回款仓储读取。Process 在每个原查询位置投影 facts，禁止额外读、缓存替代事务复验、提前执行 Posted 检查。

### 3.2 财务消费的退款事实

`erp_finance::service::receivable::customer_refund::CustomerRefundPosting` 必须只含 `refund_id: String`、`amount: Amount`、`occurred_at: Instant`。来源类型固定 `customer_refund`；source_document_id/source_revision_id 同用 refund_id，source_sequence 固定 1。

## 4. 创建与一次提交顺序

### 4.1 普通创建

执行序必须为：请求校验 → 原退款 ID/new → 客户责任组织读取 → 构造绑定命令/注册单据/审计 → 开根事务 → 本地对象读权限 → 绑定发布定义 → 注册单据写入 → 退款 create → 审计 create → 提交后详情读取。普通创建不得增加 commit 才有的来源版本或 Posted 门禁。

### 4.2 一次提交

1. 校验请求；使用原前缀 `customer-refund-commit-`、动作 `customer_refund.commit`、资源类型和完整请求构造 `CommandReceipt`。
2. 在来源读取前查询已提交资源；命中直接读取其详情。
3. 非事务读取原回款，保存来源 ID/version。缺客户必须在生成退款 ID 和实体校验前失败。
4. 在原位置生成退款 ID、`return_command_no("TK", actor, key)` 和 `occurred_at=Instant::now()`。金额为空采用原金额；经办人 actor、复核人 `finance_reviewer`、evidence None 保持。
5. 创建 adapter，执行 start guard，读取组织并判断本地对象可读；再调用独立 `Instant::now()` 构造审批快照。不得把两个 now 合并。
6. 构造 bind/document/create audit/submit audit/command audit 后，进入根事务；按来源重读 → version/Posted/customer 门禁 → 定义绑定与注册 → 同 Executor 定义图读取 → `prepare_start(receipt=None)` → 退款 create → runtime/snapshot/tasks → create audit → submit audit → command audit 顺序执行。
7. 任意事务错误都必须重新查询 command receipt；已提交则返回已提交详情，否则返回原错误。不得缩窄为只有 `command_may_have_committed()` 才查询。

## 5. 普通提交、授权回放和恢复

普通提交必须按 Validate → 幂等键 normalize → 授权精确回放 → 命中则详情 → adapter → 当前退款读取 → expected_version 检查 → start guard → dispatch 执行。版本/当前状态门禁不得移到回放之前。

回放事务必须先验证 actor active，再读取退款及客户，随后组织权限 → frozen binding → 当前及下一 subject_version 的精确回执查找。恢复与回放均保留调用方事务 Executor。

Dispatch 必须在原位置读取绑定、取 now、读组织、构造快照/start command、求值 `start_approval_command_kind`、校验本地可读、读定义图和现有回执，然后 prepare/persist。原 `RECENT_HISTORY_LIMIT` 的无效常量读取已随展示归属删除，实际函数求值保留。

只有 `persist_customer_refund_start` 的 `command_may_have_committed()` 错误才进入 8 次 fresh-session 恢复。每次必须使用原固定 subject_version、actor/key/binding；非可恢复错误立即返回，最后一次后不 sleep，耗尽8次返回原错误。不得改成单次查找、无限重试或跳过 actor/组织授权。

## 6. 最终过账和真实财务 provider

根事务必须按以下顺序执行，任意失败只传播原错误并停止后续步骤：

1. `ReturnsService::prepare_customer_refund_final_post`：读取退款，Reversed 特定错误，InApproval guard。
2. 原 `execute_customer_refund_domain_action(CustomerRefundPost)` 分派。
3. `execute_refund_posting` 的 Finance：来源 ID 门禁 → 原回款读取和 Posted 门禁 → 退款专属已过账累计查询 → 原上限规则 → `erp-finance::persist_refund_offsets_and_reversals`。
4. Refund：`refund.mark_posted()` → 退款原版本条件 update。
5. Audit：在此处构造 `customer_refund.post` 审计并写入。

财务入口必须实际调用 `post_with_store(MongoRefundStore)`；仓储替身与生产使用同一算法：

| 步骤 | 实际 provider/构造 | 必须保持的语义 |
| --- | --- | --- |
| 分配读取 | `receipt_allocations.find_allocations_by_receipts` | 原回款过滤与同 Executor |
| 逆向规划 | `ReceiptAllocation::plan_reverse`、`next_allocation_seq_range` | 原 Apply/Reverse 累计算法和顺序，不自行聚合替代 |
| 批读事实 | 原唯一 `receipt_reversal::load_receivable_offset_facts` | entry IDs、account IDs、批读缺项语义保持 |
| 逐 chunk 门禁 | entry → account → counterparty 相等 | 不提前校验后续 chunk |
| 原子回冲 | `receivable_accounts.revert_settlement` | false 返回原 `退款冲减超过已核销金额` |
| 唯一减少分录构造 | 首 chunk 成功回冲之后 `next_id/new` | 第一账户、全退款金额；BusinessDate::today 仍在此处 |
| 逐 chunk 抵销写入 | `ReceivableEntryOffset::new(next_id())` → create | **每条 offset 必须先于减少分录入库**；sequence=原 offset index+1 |
| 减少分录入库 | 全 chunks 成功后 `receivable_entries.create` | 仅一条；amount 全退款金额；posted_at 原 occurred_at |
| 反向核销写入 | reverse_rows zip seqs；每行 `next_id/new/create` | 原 allocation reverse link、seq、amount、occurred_at；逐行构造，不提前生成后续 ID |

本流程不把原 receipt 标记 Reversed，不新增销售 progress 刷新，不新增任务；这种差异必须与回款冲正分别保留。

## 7. 测试与证据边界

原客户文件 8 个测试必须全部保留：7 个在 process 命令叶，`list_batches_document_bindings_and_keeps_missing_registry_rows` 在唯一读模型。结构测试仅把 include/assert 定位到实际迁后实现，不代替行为测试。

新增 9 个行为测试：

| 文件 | 新测试数 | 实际覆盖 |
| --- | ---: | --- |
| returns service/customer_refund.rs | 3 | source version→Posted→customer 首错；commit 原全额默认/字段与延后门禁；缺客户早于无效金额 |
| finance receivable/customer_refund/tests.rs | 4 | 真实两 chunk offsets→decrease→reverse 顺序及字段；每个仓储失败停止；缺entry/account/跨主体/CAS false；第二次部分退款消费前次 reverse 及超可逆余额拒绝 |
| process reverse_flow/customer_refund.rs | 2 | 生产 runner Finance→Refund→Audit；每步失败传播原错误并停止 |

所有仓储/步骤替身必须用非零大小 `TestExecutor { _identity: u8 }`，验证每次 trait-object data address 为原 Executor。Finance 试验入口必须为生产 `post_with_store`，Process 必须为生产 `execute_refund_posting`，禁止另写影子算法。财务测试首轮 fixture 采用乱序输入 Apply(40 seq1)、Apply(30 seq2)，退款50；第二次退款10，sequence5，再退11必须在分配读取后、事实批读前失败。

静态验收已经确认：原8个测试名全部找到；owned include_str 文件都存在；git diff --check 通过。当前并未由本分片运行 Cargo、MongoDB 或历史 tests。真实数据库的事务回滚/索引竞争、8次恢复延迟和运行时完整权限流须由根统一门禁及后续授权运行验证，不得从替身推断已运行。

## 8. 快照校验

以下 SHA256 仅绑定本分片负责的 After 文件。若 Cargo 修复或后续格式修改改变文件，交付者必须刷新此表。

| 文件 | SHA256 |
| --- | --- |
| `backend/crates/erp-returns/src/service/customer_refund.rs` | `80655e85bcb7998d0546a69c38a8aa4662582e8899a915400079ad1cfe96dfd5` |
| `backend/crates/erp-processes/src/reverse_flow/customer_refund.rs` | `0647a478fd713e5ae8dcc811376ed1d0ce0ee7967b86717c3b62c63d5ab6df76` |
| `backend/crates/erp-processes/src/reverse_flow/customer_refund/start.rs` | `862c4135055c1bdee22299ff178755ab14c50fc25c002d664e0a92f6fb88f4d9` |
| `backend/crates/erp-finance/src/service/receivable/customer_refund.rs` | `7bb5e8a619475aa01271197bcbbec7d9d4cc62411af8c183e80e3fa400c2c633` |
| `backend/crates/erp-finance/src/service/receivable/customer_refund/tests.rs` | `7c4bf63d28b289456fd927136c4e88470f60a355924c32de52e50ccd43bd175e` |
