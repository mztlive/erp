# 阶段 17 资金工作项事实交付合同

输入提交必须为 `a537414eb8f78c43ebc383a3457a45c437dececc`。实施树为 `/private/tmp/erp-domain-crate-17-cutover`。本文件与配套 JSON 绑定当前 16 个本分片文件和 3 个实际共用 helper/source 文件的 SHA-256；最终源码提交已绑定 `39c55021d8bb1d030eade4afb3e135e4b5a8a18b`。

## 1. 交付与所有权

本分片交付真实 `WorkItemFactsReader` 资金来源、命令读取和唯一权威映射，并将现有工作台显示接到相同原子来源与映射。旧 `services/work_item` 源文件删除由 finance_persistence 负责。不得恢复旧 facade、复制第二份 ObjectFact、将完整命令 load 加到富显示 load 前。

公共权威类型使用 `erp_workflow::ports::{ObjectFact,ObjectFactMap,ObjectKind,SubjectBrief}`；不得引用私有 `ports::object_facts` 模块。资金方法可见性为 `pub(in crate::workbench)`，不扩大整个模块树。

| 实际文件 | 接口和执行职责 |
|---|---|
| `authority/funds/sources.rs` | 10 个 `read_*`，各只执行原仓储 `list_active_by_ids(ids, executor)`，保留空输入调用。 |
| `authority/funds/accounts.rs` | `load_receivable_account_facts`、`load_payable_account_facts`。 |
| `authority/funds/receipts.rs` | 回款、客户退款、回款冲正 3 个命令 loader。 |
| `authority/funds/payments.rs` | 付款、供应商退款、付款冲正 3 个命令 loader。 |
| `authority/funds/mapping.rs` | 8 个唯一 authority factory、`voucher_revision_ids`、`receivable_account_impact`。 |
| `authority/funds/origins.rs` | 4 个命令来源 recipe 与 4 个已读行 counterpart-only projection。 |
| `authority/funds/query.rs` | 唯一审计创建人、主体当前法定名、客户/供应商编号 fallback、应付来源单号和命令卡券修订查询。 |
| `authority/amount.rs` | 仅复用已有 presentation/brief 唯一纯 helper；原金额与采购摘要 2 个测试随实际 helper 导入。 |
| `funds_document_brief{.rs,/**}` | 原显示查询插槽、富简报与显式 display overlay；不自行构造 authority root/creator/version。 |
| `party_names.rs` | 文件字节保持输入一致；原 6 个测试保留。 |

## 2. 查询顺序与首错

以下每一步必须使用入口传入的同一 Executor，逐次 await，失败立即返回。不新增事务、NoTransaction、并发查询、缓存、排序或重试。共同来源无新增空输入护栏。命令与富显示入口所有 keys 空护栏均保持；实际 rows 空提前返回仅保留原有位置。

| 对象 | 命令查询顺序 | 富显示额外查询位置 |
|---|---|
| 应收账户 | accounts → 实际空 accounts 返回 → party names → revision flags | 原 sales number、revision/line/voucher、tax/current BusinessDate、due 查询及顺序保留。 |
| 应付账户 | accounts → supplier names → purchase numbers | purchase numbers 后原 due dates。 |
| 客户回款 | receipts → 实际空 receipts 返回 → create audits → party names | names 后 receipt allocations。 |
| 客户退款 | refunds → audits → customer names → receipt origins → entry origins | origins 内保留原金额、核销行、销售号、到期等显示读取。 |
| 回款冲正 | reversals → audits → receipt origins | 原 receipt origins 内 names 后 allocations。 |
| 供应商付款 | payments → audits → supplier names | names 后 payment allocations。 |
| 供应商退款 | refunds → audits → supplier names → payment origins → entry origins | origins 内原核销行、采购号、到期等读取。 |
| 付款冲正 | reversals → audits → payment origins | 原 payment origins 内 names 后 allocations。 |

扩展实际 reader 后，16 条入口从函数开始到映射循环之前的 token 全部与输入一致。12 个名称/来源 helper 或显示专属查询的完整 body 同样一致。4 个富 origins 的查询前缀和完整原 projection 保持一致；4 个命令 origins 的查询前缀与 filter_map 保持一致，仅消费值改为借用行后 clone ID。

## 3. 权威字段与显示差异

| 类型 | 参与根 | 创建人 | 版本约束 |
|---|---|
| ReceivableAccount | `account.sales_order_id` | `account.stable.created_by` | `unrestricted` |
| PayableAccount | `account.source_document_id` | `account.stable.created_by` | `unrestricted` |
| CustomerReceipt | `receipt.base.id` | `create audit first actor or empty` | `unrestricted` |
| SupplierPayment | `payment.base.id` | `create audit first actor or empty` | `unrestricted` |
| CustomerRefund | `refund.base.id` | `create audit first actor or empty` | `unrestricted` |
| SupplierRefund | `refund.base.id` | `create audit first actor or empty` | `unrestricted` |
| ReceiptReversal | `reversal.base.id` | `create audit first actor or empty` | `unrestricted` |
| PaymentReversal | `reversal.base.id` | `create audit first actor or empty` | `unrestricted` |

审计查询保留 HashSet→Vec 的既有迭代次序、原 repository filter 和每 resource 的第一个 actor；缺 actor 为空串，不使用实体 created_by 代替。主体名称仍读取 active party 当前 revision 并 trim，缺修订或空法定名不入表；客户/供应商账号缺法定名时仍回退原编号。

必须保留下列 4 个原差异：

1. 命令 origin map 使用 filter_map 丢弃无名称条目；富 origin map 保留条目并保存 None。
2. 客户退款首选原回款存在但无名称时，命令可回退分录名称；显示仍选择已存在的原回款 brief。
3. 供应商退款显示仅取直接 supplier 名称；命令仍可回退 origin。显式 display None 不得被 authority 自动回填。
4. 命令应收卡券只认修订 2 个 flags；显示可由券行 entry().or_default() 扩展。唯一 classifier 供两者初始化使用，命令集合不接收券行扩展。

资金标题、影响文本、格式化 Amount 的唯一实现已按 JSON 的 8 项 authority_mapping_contract 核对。金额无转换浮点数；原 due date 和 BusinessDate::today 求值位置保留。新增数据均为短生命周期只读内部结构，不新增 serde/BSON/持久化 schema。

## 4. 静态核销与测试

- 14 个输入源文件逐字匹配输入 git blob，82 个原函数逐项登记实际目的文件、符号、行号及 body token SHA-256。
- 10 个本分片 raw source 和 3 个父分片共享 raw source 均展开检查真实单次仓储调用。
- 16 条 recipe 前缀、12 个 helper、4 条 rich origin split、4 条 command origin split 无未解释的 token 差异。
- 原 12 个 tests（旧 amount 2、rich mapping 4、party_names 6）全部保留，完整原 test body token 相同。
- 新增 5 个纯测试均调用实际生产 helper/mapper：audit first actor、无名称原单双 map、客户退款双来源、供应商退款显式 None、真实销售修订 flags 与显示 impact 的区别。
- 定向 rustfmt 已通过；本分片未执行 Cargo、Mongo 或历史 tests。`ERP_TEST_MONGO_URI` 仅检查为 unset，未输出值。

运行验收必须以 root 最终 workspace/clippy/lib 日志为准。本文不声明非 ZST Executor 指针逐步骤注错测试已执行；当前同 Executor 与失败后短路证据来自真实生产 body 静态核对。不得将此静态证据改写为数据库运行或完整故障注入通过。

## 5. 封存输入

`/private/tmp/cutover17-funds-result.json` 是逐文件、逐符号与每项比较结果的结构化记录。`/private/tmp/review-cutover17-funds.py` 和 `/private/tmp/finalize-cutover17-funds-evidence.py` 仅静态读当前树、输入 git blob 并写 /tmp 证据，不执行 Cargo/数据库。最终源码若变动，必须重新运行并核对 original_tests、static_issues、after_hashes_stable_at_end，再绑定最终提交。

## 6. 最终提交绑定

- 源码提交：`39c55021d8bb1d030eade4afb3e135e4b5a8a18b`。
- 16 个资金文件与 3 个共用 helper/source 文件全部满足：原审核 SHA-256 = 当前文件 SHA-256 = 提交 blob SHA-256。没有生产或测试字节变化，不需要替换既有语义结论。
- 配套 JSON SHA-256：`ee61b4913b7e491587a97b7ac122b906cd15ad17067ec89954695d87844d8796`。
- 本次仅执行 Git blob/文件比较；不运行 Cargo、数据库或历史测试。真实数据库运行未验证。
