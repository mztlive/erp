# 阶段 09 财务迁移语义核验记录

## 核验范围与裁定

- 基线 B：提交 `6778959016426ecd808f97db6bcb37955bd7b11b`，原工作区完整输入快照。
- 候选 A：实现提交 `7d94672e4c90445c6763ed57076bdc1c1d8a010f`；本记录为只读源码核验，不包含 Cargo 或数据库执行。
- 输入：`/private/tmp/erp-finance09-contract-final/missing-drift-report.json` 中 2 个幂等符号、17 个事务符号；两项幂等符号包含于 17 项事务符号。
- 核验项目：prepare/replay 顺序；ID 和时间生成位置；验证及逐行首错；真实生产适配器的写入/任务/审计顺序；调用方 Executor 传递；错误分类及传播。
- 裁定：本记录覆盖的实际函数与提取后调用链中，未发现上述合同的实质漂移。哈希变化可定位到根事务上移、财务片段提取、只读结果路由或文件候选集合变化。
- 证据等级：源码逐段差异与生产调用链复核。**真实数据库运行未验证**。本裁定不证明真实 MongoDB 事务提交/回滚、并发唯一键竞争、驱动重试或提交结果未知恢复，也不替代全量门禁记录。

## 符号覆盖表

每行裁定仅覆盖本行列出的源码行为。B/A 链接指向核验时对应工作树的实际函数。

| # | 报告符号 | 基线与候选落点 | 核验事实 | 源码结论 |
|---|---|---|---|---|
| 1 | `apply_effective_change` | `67789590:backend/services/src/sales_review/sales_change_order.rs:519` → `7d94672e:backend/crates/erp-processes/src/sales_change/mod.rs:37` | 变更单读取与动作闸门→原计划准备→审计构造→根事务。事务内正式版本→应收差额及任务→变更状态→成功审计；传入事务入口继续复用原 session。 | 合同保持；未发现实质漂移 |
| 2 | `commit_customer_receipt` | `67789590:backend/services/src/receivable/customer_receipt.rs:233` → `7d94672e:backend/crates/erp-processes/src/finance_posting/receivable/customer_receipt.rs:118` | 请求校验→命令指纹与已提交回放→prepare→新回款 ID/构造→根事务；原草稿版本检查、绑定/冻结核销/审批写入顺序保留。事务错误仅在命令收据回查成功时转为已提交结果，否则传播原错误。 | 合同保持；未发现实质漂移 |
| 3 | `create_cost_entry` | `67789590:backend/services/src/cost/mod.rs:139` → `7d94672e:backend/crates/erp-processes/src/finance_posting/cost/mod.rs:53` | 请求/商城范围检查→销售存在性→金额/尾差计划→成本 ID/构造→逐分配 ID/构造→审计构造→事务内成本与分配→审计。 | 合同保持；未发现实质漂移 |
| 4 | `create_delivery_draft_for_order` | `67789590:backend/services/src/purchase_order/review.rs:460` → `7d94672e:backend/services/src/purchase_order/review.rs:313` | 函数体相同；保留原履约编号获取、草稿字段和任务持久化顺序，仍由采购正式化副作用在财务/付款任务/成本成功后调用。 | 合同保持；未发现实质漂移 |
| 5 | `create_payable_account` | `67789590:backend/services/src/payable/account.rs:322` → `7d94672e:backend/crates/erp-processes/src/finance_posting/payable/account.rs:101` | 请求校验→采购来源读取→账户 ID→分录 ID→账户构造→入账时间及分录构造→审计构造→同事务账户/分录→审计。 | 合同保持；未发现实质漂移 |
| 6 | `create_receivable_account` | `67789590:backend/services/src/receivable/account.rs:433` → `7d94672e:backend/crates/erp-processes/src/finance_posting/receivable/account.rs:46` | 请求校验→销售来源→复核状态/当前版本闸门→账户与分录 ID/构造→审计构造→账户/分录→初始卡券复核任务→开票任务→审计。业务类型采用显式双分支事实映射。 | 合同保持；未发现实质漂移 |
| 7 | `create_service_fulfillment_draft_for_order` | `67789590:backend/services/src/purchase_order/review.rs:539` → `7d94672e:backend/services/src/purchase_order/review.rs:392` | 函数体相同；冻结责任分支、服务草稿指纹/创建时点以及任务调用顺序相同。 | 合同保持；未发现实质漂移 |
| 8 | `formalize_approved_order` | `67789590:backend/services/src/purchase_order/review.rs:50` → `7d94672e:backend/crates/erp-processes/src/procure_to_pay/mod.rs:36` | 原 prepare_formalized_order 保留；准备与 ID 生成仍在根事务外，审批入口继续使用传入 session；订单、财务、任务、履约、审计具体写序见采购核验条目。 | 合同保持；未发现实质漂移 |
| 9 | `formalize_approved_submission` | `67789590:backend/services/src/sales_order/formalize.rs:89` → `7d94672e:backend/crates/erp-processes/src/order_to_cash/mod.rs:50` | 已正式化短路、状态闸门、最新提交与采购责任计划保持；授权策略事务/普通根事务分支保留；订单/提交→首次应收→任务→审计。 | 合同保持；未发现实质漂移 |
| 10 | `issue_red_invoice` | `67789590:backend/services/src/receivable/red_invoice.rs:52` → `7d94672e:backend/crates/erp-processes/src/finance_posting/receivable/red_invoice.rs:49` | 重复红票身份守卫及红冲计划保持；红票创建→必要时原票红冲状态→额度逆转→逆向分配→审计→销售侧任务/销售进度。 | 合同保持；未发现实质漂移 |
| 11 | `persist_effective_writes` | `67789590:backend/services/src/sales_review/sales_change_order.rs:1171` → `7d94672e:backend/crates/erp-processes/src/sales_change/posting.rs:61` | 根事务所有权上移到 SalesChangeProcess；生产 post 顺序为 revision→receivable_and_tasks→change→audit，各步接收同一 executor 并使用 ? 短路。 | 合同保持；未发现实质漂移 |
| 12 | `persist_formalized_order` | `67789590:backend/services/src/purchase_order/review.rs:304` → `7d94672e:backend/crates/erp-processes/src/procure_to_pay/mod.rs:36` | 旧私有根事务包装被流程入口吸收；prepare 结果仍在事务前生成，事务内使用新 persist_formalized_order_write；此符号同时触发幂等哈希变化。 | 合同保持；未发现实质漂移 |
| 13 | `persist_formalized_submission` | `67789590:backend/services/src/sales_order/formalize.rs:227` → `7d94672e:backend/crates/erp-processes/src/order_to_cash/mod.rs:121` | 准备及根事务移到流程入口；本函数接收既有 session，先订单正式化写，再首次应收/任务，最后成功审计；无内层 with_transaction。 | 合同保持；未发现实质漂移 |
| 14 | `post_customer_receipt` | `67789590:backend/services/src/receivable/customer_receipt.rs:699` → `7d94672e:backend/crates/erp-processes/src/finance_posting/receivable/customer_receipt.rs:585` | 根事务与审批事务入口保持；状态闸门→核销准备/额度条件更新→回款过账/分配→审计→去重销售进度。 | 合同保持；未发现实质漂移 |
| 15 | `post_payment_reversal` | `67789590:backend/services/src/returns/payment_reversal.rs:496` → `7d94672e:backend/crates/erp-processes/src/reverse_flow/mod.rs:49` | 冲正状态与最终通过检查提取为 ReturnsService 准备方法；原付款与额度校验→逆向副作用/任务→冲正过账状态→成功审计，均沿同一 session。 | 合同保持；未发现实质漂移 |
| 16 | `register_card_funds_invoice` | `67789590:backend/services/src/receivable/card_funds_register.rs:253` → `7d94672e:backend/crates/erp-processes/src/finance_posting/receivable/card_funds_register.rs:248` | 指纹/审计键/稳定发票号和事务首部 replay 保留；财务额度更新/发票/分配提取 helper，原业务审计、命令回执与销售进度顺序保留；同时触发幂等哈希变化。 | 合同保持；未发现实质漂移 |
| 17 | `register_purchase_invoice` | `67789590:backend/services/src/payable/invoice.rs:56` → `7d94672e:backend/crates/erp-processes/src/finance_posting/payable/invoice.rs:50` | 命令收据回放→供应商来源→发票 ID/构造→事务内号码查重→分配 ID/计划→账户/供应商逐账户首错→额度→发票/分配→审计→命令收据。失败后原收据恢复保留。 | 合同保持；未发现实质漂移 |

## 幂等合同

1. 采购正式化：`67789590:backend/services/src/purchase_order/review.rs:81` 与 `7d94672e:backend/services/src/purchase_order/review.rs:39` 的函数体相同；候选仅增加公开可见性。旧 `persist_formalized_order` 自身只持有根事务，未自行新增或校验幂等指纹。流程入口仍先准备再进入事务，审批运行时入口仍在准备后使用传入 session。履约草稿的冻结责任字段、指纹及编号调用由两个未改函数体继续提供。

2. 历史卡券开票：`7d94672e:backend/crates/erp-processes/src/finance_posting/receivable/card_funds_register.rs:248` 保留 `req.validate`→任务版本解析→`CardFundsRegistrationReceipt::payload_fingerprint`→action/actor/key 审计 ID→规范发票号码或稳定号码。事务进入后先 `7d94672e:backend/crates/erp-processes/src/finance_posting/receivable/card_funds_receipt.rs:45`；命中相同命令即返回已登记结果，未继续生成发票与分配 ID。未命中才进行任务身份/上下文及分配校验、号码查重、发票 ID/构造/登记、资料注册。随后 `7d94672e:backend/crates/erp-finance/src/service/receivable/card_funds_register.rs:164` 依次执行账户 `apply_invoicing`→发票 create→分配 ID→分配 create。helper 使用发票已冻结的 gross/net/tax，分别来自原请求及已验证分配总额。任务同步→业务审计→命令回执审计→销售金额进度顺序保持。

3. `customer_receipt`/普通 `invoice`：`7d94672e:backend/crates/erp-processes/src/finance_posting/receivable/customer_receipt.rs:118` 与 `7d94672e:backend/crates/erp-processes/src/finance_posting/receivable/invoice.rs:105` 保留请求校验后先构造并查询 `CommandReceipt`、再调用 `prepare` 的顺序。新资源 ID 只在 New 分支生成；Existing 保留期望版本检查。事务失败后查询已提交收据；有收据返回原资源，无收据返回事务原错误。`7d94672e:backend/crates/erp-finance/src/service/receivable/customer_receipt_commit.rs:89` 与 `7d94672e:backend/crates/erp-finance/src/service/receivable/invoice_commit.rs:120` 的 New/Existing 组合校验、金额/分配转换和返回计划顺序与基线对应函数体一致（忽略模块限定路径）。

4. `card_funds`：登记回款的 `register_card_funds_receipt`、`load_card_funds_registration_context`，以及 `card_funds_identity` 和 `card_funds_receipt` 的身份锁定、replay 与错误映射函数，和基线对应生产函数体一致（忽略模块限定路径）。财务 `7d94672e:backend/crates/erp-finance/src/service/receivable/card_funds_register.rs:36`、`7d94672e:backend/crates/erp-finance/src/service/receivable/card_funds_register.rs:91` 保持分配计划及落库输入顺序；未引入新的幂等键字段或宽松回放分支。

## 准备、ID 与入账时点

- 成本：`7d94672e:backend/crates/erp-finance/src/service/cost.rs:368` 只承接原请求与已停用商城检查；流程完成销售存在性后才调用 `7d94672e:backend/crates/erp-finance/src/service/cost.rs:377`。先金额/净额/尾差计划，后成本 ID 与构造，再逐条分配 ID 与构造，随后流程生成成功审计对象并进入事务。无验证提前到错误优先级不同的位置。
- 应付手工建账：`7d94672e:backend/crates/erp-finance/src/service/payable/account.rs:15` 在采购来源确认后生成账户 ID、分录 ID，维持零核销/零收票、默认可收票 gross、分录 posted_at 的原构造位置。流程在准备完成后生成审计对象，账户/分录与审计共用根 session。
- 应收手工建账：复核状态显式映射 `BusinessType::{GoodsService,Voucher}` 到同名 `SalesBusinessTypeFact`，初始状态解算仍映射为 ValidationError，卡券当前生效版本不符仍在 ID 生成前返回 ConflictError。账户、分录、任务和审计顺序见覆盖表。
- 销售首次应收：`67789590:backend/services/src/sales_order/formalize.rs:479` → `7d94672e:backend/crates/erp-finance/src/service/receivable/initial_account.rs:50` / `7d94672e:backend/crates/erp-finance/src/service/receivable/initial_account.rs:66` 保持账户 ID→分录 ID→账户构造→`BusinessDate::today`→分录构造。账户 `account_seq=1`、`system` 创建人、Original/Increase、source_sequence=1、销售单/正式修订来源、零核销/零开票和冻结 gross/posted_at 相同；两处实体构造均显式保留 `Error::Logic`。流程在财务写入后依次推进初始卡券复核任务与开票任务。
- 销售变更：`7d94672e:backend/services/src/sales_review/sales_change_order.rs:1019` 保留提交/提交行/销售单/基准版本检查；`7d94672e:backend/services/src/sales_review/sales_change_order.rs:1054` 的 now、版本号读取、正式修订 ID、应收差额计划生成位置保留。审计构造从计划末尾移到流程收到计划后的下一步；两者间只有同步结构返回，没有新增查询、ID 或时钟获取。原准备子查询中的 `NoTransaction` 仍存在，包括调用方已有审批 session 的路径；此为基线行为，不应将本记录写成所有准备读取均在事务内。

## 正式化与财务写入顺序

- 销售正式化：`7d94672e:backend/services/src/sales_order/formalize.rs:184` 保留采购来源复验→采购工作项持久化→业务文档正式化→销售正式版本/订单→采购任务同步→提交状态写回。`7d94672e:backend/crates/erp-processes/src/order_to_cash/mod.rs:121` 紧接首次应收、卡券/开票任务和审计。审计对象仍在写入事务之前构造，策略 revision 存在时仍走原授权事务入口。
- 采购正式化：`7d94672e:backend/crates/erp-processes/src/procure_to_pay/mod.rs:71` 在事务内先构造审计，然后 `7d94672e:backend/services/src/purchase_order/review.rs:505` 按原顺序执行来源复验→当前销售分配准备→有效修订创建→销售分配写入→订单正式化→提交 review（原 `Instant::now` 位置）→提交更新→订单更新。流程随后依次写应付/分录、付款任务、输入顺序的成本条目、冻结责任对应履约草稿、成功审计。`7d94672e:backend/services/src/purchase_order/review.rs:571` 保留原分支。
- 销售变更：`7d94672e:backend/crates/erp-processes/src/sales_change/posting.rs:26` 的生产适配器实际调用 `persist_revision`→`write_receivable_delta`→`persist_change`→audit；`7d94672e:backend/crates/erp-processes/src/sales_change/posting.rs:74` 内为分录 create→账户 update→卡券复核任务→开票任务。每一步都传递调用方 executor，并通过 `?` 停止后续步骤。采购变更文件中同名 `apply_effective_change`、`persist_effective_writes` 的函数体未变：`7d94672e:backend/services/src/purchase_order/change/effect.rs:416`。
- 回款过账：`7d94672e:backend/crates/erp-finance/src/service/receivable/customer_receipt_posting.rs:19` 保留已有分配读取、核销 ledger、排序去重分录/账户批量查询、分配 ID 生成、单次 allocated_at、输入顺序缺失分录→缺失账户→往来主体检查、ledger 应用、条件额度写入及余额拒绝。成功后才 mark_posted/update 回款并批量插入分配。流程随后写审计，再按排序去重销售单更新金额进度。
- 普通销项开票：`7d94672e:backend/crates/erp-processes/src/finance_posting/receivable/invoice_posting.rs:58` 的生产顺序为任务执行记录→财务→业务审计→任务同步→销售进度→可选命令收据。`7d94672e:backend/crates/erp-finance/src/service/receivable/invoice_posting.rs:18` 保留任务执行记录之后才生成分配 ID、构建计划与账户校验；条件额度写入拒绝在 invoice.mark_registered/update 和分配插入之前。MongoInvoicePosting 逐方法转传同一 executor；普通 post 无收据，commit 保持收据最后写入。
- 红票：`7d94672e:backend/crates/erp-finance/src/service/receivable/red_invoice_posting.rs:23` 保留红票 create→全额时原票 mark_red_invoiced/update→合并逆转 delta→按销售/采购分支条件回退额度→拒绝检查→逐行逆向分配 ID/构造→批量插入。流程之后写业务审计，再在销售侧按账户同步任务及按销售单更新进度。红冲计划及合并 delta 函数体与基线相同。
- 进项开票：`7d94672e:backend/crates/erp-finance/src/service/payable/invoice.rs:161` 保留号码查重先于分配 ID/计划、账户批量读取。流程批量加载供应商后仍按账户 delta 首次出现顺序解释“账户不存在→供应商不存在→主体不一致”。`7d94672e:backend/crates/erp-finance/src/service/payable/invoice.rs:216` 保留条件额度→拒绝检查→发票 mark_registered/create→分配插入；业务审计与命令收据在后。
- 付款冲正：`7d94672e:backend/services/src/returns/payment_reversal.rs:57` 逐句保留旧 final_post 开头的读取/已冲正拒绝/最终通过/领域动作闸门。`7d94672e:backend/crates/erp-processes/src/reverse_flow/mod.rs:85` 在其后执行原冲正副作用、mark_posted、冲正单更新、审计构造与写入；原付款/超额验证和核销逆转、付款任务、逆向分配、原付款状态顺序未变。

## 错误传播与基础设施边界

- `7d94672e:backend/crates/erp-finance/src/error.rs:1` 将原校验、领域、并发与仓储错误保持在对应变体；`7d94672e:backend/services/src/errors.rs:236` 显式逐变体映射回 services::Error，未以字符串匹配错误。原 services 唯一索引映射没有财务专属提示，财务唯一索引保持原通用冲突消息；乐观锁、瞬态事务及提交结果未知保留原变体。
- 已检查的提取财务 helper 没有自行调用 `with_transaction`；流程生产适配器及仓储调用继续使用传入的 session/executor。普通根事务入口和审批已有事务入口分别保留。此结论来自调用链源码，不证明数据库实际执行器身份或回滚效果。
- 纯替身测试的定义可在候选生产编排文件内看到；本次未执行这些测试，也未将其当作真实 MongoDB 证据。历史 `tests/**` 不在本次执行范围。

| 逐字节比较的基础文件 | 基线/候选 | 候选 SHA-256 |
|---|---|---|
| `backend/crates/persistence-core/src/executor.rs` | 相同 | `d7f18681920bb0f7356687953043ea004f2c96b0a3219f4cb8a46242a7303a2d` |
| `backend/crates/persistence-core/src/transaction.rs` | 相同 | `3fa042ed3d5baaa9914f55566fd961c40d58ebcd9a7f58f0e67497bdabdb47ed` |
| `backend/crates/application-core/src/command.rs` | 相同 | `606633a4c45560ad8b8994aa6f6d933a4264e1558b6f7ec2111b8e5837adfa73` |

## 记录适用条件

本记录仅消解输入报告列出的 2/17 项符号哈希变化的源码语义疑点，不将报告中其他 DTO、间接索引解析或静态采集限制自动置为通过。并行修改后如上述生产函数发生新变更，应按相应路径重新复核。
