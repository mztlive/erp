# 阶段 17 工作项权威事实单一化实施记录

- 输入提交：`a537414eb8f78c43ebc383a3457a45c437dececc`。
- 实施树：`/private/tmp/erp-domain-crate-17-cutover`。
- 实施范围：旧 `services/src/work_item/**` 与 `erp-read-models/src/workbench/**`。E 持有流程 adapter，root 持有全局错误、注册、入口及 Cargo 门禁。
- 状态：46 个最终来源文件已逐字节绑定源码提交 `39c55021d8bb1d030eade4afb3e135e4b5a8a18b`；335 个原生产函数、63 个原测试定义全部核销。
- 本记录仅为实际源码、纯测试定义与静态合同证据。**真实数据库运行未验证**。未运行 Cargo、MongoDB、外部 gateway 或历史 tests。

## 1. 唯一公开合同

唯一 reader 为 `erp_read_models::workbench::authority::WorkItemFactsReader`。构造仅保存 `Database`，不执行 I/O，不持有 RBAC、授权服务、工作流写入 Port 或事务根。

| 实际方法 | 返回 | 执行约束 |
| --- | --- | --- |
| `new(Database)` | reader | 不读库 |
| `load(&HashSet<ObjectFactKey>, &mut dyn Executor)` | RM `Result<ObjectFactMap>` | 固定步骤串行读取；首错立即返回 |
| `counterparty_numbers(kind, ids, executor)` | RM `Result<HashMap<String,String>>` | 原逐 ID `find_by_id`；保留重复读取 |
| `counterparty_is_active(kind, id, executor)` | RM `Result<bool>` | 仍只判断上述编号结果包含 ID，不增加 status 或 can_login 判断 |
| `external_identity_map_exists(id, executor)` | persistence-core `Result<bool>` | 原外部映射 `find_by_id` 与 `is_some` |

权威类型直接使用现存公开出口 `erp_workflow::ports::{ObjectFactKey,ObjectFactMap,ObjectFact,ObjectKind,SubjectBrief}`。没有新的同形 ObjectFact、SubjectBrief 或 ProcessObjectFacts 实体。

E 的 `erp-processes/src/adapters/workflow/object_facts.rs` 四个只读方法调用上述 reader；前三个组合读取的返回值通过正式 RM Error → Process Error → 原 WorkflowError 映射；原子 external_identity_map_exists 保留 persistence-core Error，由 adapter 直接执行原 WorkflowError::from，避免提前套用组合层的较广索引文案。采购责任改派、W29 prepare/persist、政策和审计仍归 E 的实际流程；reader 没有这些写方法。

## 2. 命令真实执行顺序

`authority/command.rs::WorkItemFactsReader::load` 调用 `authority/recipe.rs::load`。后者使用真实 `CommandFactReads`，其生产实现逐步调用原 loader，同一个 Executor 传到每个实际仓储读取。

顺序为 Sales → Purchase → Fulfillment → PurchaseChange → SalesChange → Receivable → Payable → CustomerReceipt → CustomerRefund → ReceiptReversal → SupplierPayment → SupplierRefund → PaymentReversal → Inventory → Settlement → LegacyImport → IntegrationError → Reconciliation → SupplierFulfillment → SupplierOffering。

Fulfillment 步骤内部继续按采购入库 → 发货 → 电子交付 → 服务履约读取。其对应 ID 为空时仍按原入口调用仓储；来源单号 helper 自身的空输入护栏保持。没有给整体 `load` 增加空 keys 返回。

原 `load_procurement_confirmation_facts` 仅返回 `Ok(())`，不读取、不修改事实，已删除两个无操作声明与调用。原 `assignment_separation_actors` 保留在流程 adapter，仍为无 I/O 的空集合。

## 3. 来源与投影归属

| 原事实范围 | 唯一 authority 实现 | 显示实际装配 |
| --- | --- | --- |
| 销售单 | `authority/sales.rs`：提交读取、preferred_submission、sales_order_fact | `sales_order_brief.rs` 在原提交读取后追加行读取；复用权威 mapper |
| 采购单 | `authority/purchase.rs`：订单/提交/行来源、全部行计数、上次正式提交、权威 subject overlay | `purchase_review_brief.rs` 保留 submissions → sales numbers → lines 的原插槽，再单独组装 diff/付款/物流简报 |
| 销售/采购变更 | `authority/changes.rs`：两个根/创建人/名称 fallback mapper；`authority/sources.rs` 共享头、基准与提交原子读取 | `change_order_brief.rs` 在原头/基准/提交后追加原两侧行与 diff |
| 履约四对象 | `authority/fulfillment.rs`：完整原同形读取与标题规则 | `fulfillment_operation_brief.rs` 将同一已加载 map 转为 display envelope，不重新读取 |
| 库存调整 | `authority/inventory.rs` 的 stock_adjustment_fact 与共享头读取 | `inventory_settlement_brief.rs` 保留原仓库、调整行、SKU 简报查询 |
| 供应商结算 | `authority/inventory.rs` 的分组、review instruction 和 settlement_fact；`authority/sources.rs` 的单/行/差异原子读取 | 显示在差异与名称之间保留 difference_evidence → source_evidence 原插槽 |
| 导入/集成/对账 | `authority/command.rs`：唯一原事实与两种 integration impact | 原 facts.rs 只追加已读实体的纯富简报，不新增命令读取 |
| 供应商履约/供给 | `authority/command.rs`：原版本约束/缺事实规则 | 显示复用此单一读取与投影，没有另一份版本判断 |
| 账户、回付款、退款、冲正 | `authority/funds/{accounts,receipts,payments,query,origins,sources,mapping}.rs` | 原 `funds_document_brief/**` 保留额外金额、核销、税、到期、凭证显示 |
| 金额格式 | `authority/amount.rs` 指向实际 presentation/brief 纯函数 | 不复制第二金额格式实现 |

`authority/sources.rs` 的 read_sales_orders/read_purchase_orders/read_sales_revisions/read_purchase_revisions 等只完成原仓储读取，不添加空输入护栏、排序、去重、过滤或缓存。每条 recipe 保留原始查询参数构造、HashSet 迭代顺序和失败位置。

## 4. 权威与显示隔离

`facts.rs::WorkbenchObjectFact` 仅包含现存 workflow `ObjectFact` 与 `WorkbenchObjectDisplay`。显示对象保留 label、counterparty_label、impact_summary、brief_source 和独立 subject 显示 map；`from_authority` 显式建立初始显示，随后每类显示 mapper 按原语义覆盖。

`access.rs` 的 authorized_fields/authorized_item_fields 保留原 permission → participation → subject_versions 判断顺序。root_document_id、created_by、subject_versions 只读取 `fact.authority`。组织判断仍使用 WorkItem 的 owner_organization_id，未把客户、供应商、往来主体或来源单据 ID 当作组织。

`apply_object_display/apply_subject_display` 只读取 `fact.display`；根业务对象 ID 读取 authority。subject 显示缺值时原有 subject → display root fallback 保持，不增加 display → command fallback。已有显式负责人任务 impact 保留条件未改变。

以下四项原差异分别保留，不以共享来源为由统一：

1. 应收命令卡券判定仅来自 sales revision 两个 flags；显示仍允许券行扩张 vouchers map。共享初始 flags，但两个 predicate 分别传入原影响文案。
2. 客户退款命令 origin map 用 filter_map 丢弃缺名称条目；rich origin map 保留无名称来源记录。两个 map 从同次原 rows 分别投影，因此 receipt-origin/entry-origin fallback 仍可能不同。
3. 供应商退款 display counterparty 保持仅 supplier display name；命令可回退 original payment/entry 名称。display None 不被 authority 反填。
4. 采购命令影响统计全部原提交行；富显示影响使用原差异/可见简报行及 more_count。两个投影从同次查询的原行分别生成。

除供应商履约订单和供给两类，权威 subject_versions 保持 unrestricted。采购提交 ID 的 subject_briefs key 不转成锁版本。供给停止顺序仍 offering:version → availability:version；两者未停止则不返回事实。供应商履约使用订单 base.version，未换成 WorkItem 版本。

## 5. 名称与首错合同

- 创建人审计保留原批量查询和每 resource 首个 actor；缺 actor 返回空串。
- party legal names 保留 active parties → current pointer → active revisions → trim 非空名称，以及原空集合护栏。
- customer/supplier display names 保留账号 → party 名称，缺名称回退业务编号。
- counterparty_numbers 保留 supplier/customer 字面判断与逐 ID 查询，未知字面返回空 map；相同 ID 输入多次仍读取多次。
- 所有 reader 返回原 typed Error，不把读取失败合并为对象不存在，不进行字符串错误分类。
- 所有 command/display 仓储调用传入原 Executor；reader 不创建 NoTransaction 或新事务。工作台原最外层 `object_facts_for_rows` 仍使用原 NoTransaction。

## 6. 纯测试与源码核销

原 63 个测试定义按名称与出现次数全部保留，当前 75 个：C 新增 7 个，A 新增 5 个。测试定义与执行结果必须分别登记。

C 的真实生产测试覆盖：固定 20 步调度和同一个非 ZST Executor、各步骤首错停止、往来编号重复 ID/未知 kind/首错、display None 不回填命令名称、subject 显示与命令影响分离、authority 根/创建人和任务组织参与判断。

A 的真实生产测试覆盖：审计首 actor、两种 origin map 成员差异、客户退款 fallback 差异、供应商退款 display None、卡券显示条件不覆盖命令条件。

旧 15 个 `services/work_item` 源逐文件核对输入提交字节后删除；没有修改或迁移历史 tests。核销记录：`/private/tmp/cutover17-work-item-deletions.json`。原字节副本：`/private/tmp/cutover17-c-before/**`。

完整 41 个 before 文件、335 个 before 生产函数及实际 after provider、来源哈希、原/新增测试定义清单见 `/private/tmp/cutover17-workflow-facts-implementation.json`。该 JSON 同时记录 workflow 唯一类型与 E adapter 实际来源，不用 scanner placeholder 代替 provider。

## 7. 统一门禁与封存

本分片已执行定向 rustfmt 与 `git diff --check`；未执行 Cargo。root 已报告冻结源码上的 fmt、workspace check、strict Clippy、lib、domain/BPM/permission 门禁全部 exit 0，33 个 package 的库测试为 3655 passed / 0 failed / 68 ignored。执行者与原始门禁日志由 root 的共享证据登记，本分片不重复执行。

最终封存必须重读每个 after 源并绑定 root 提供的 source commit blob。并发修复导致来源哈希变化时，先复核实际差异再刷新。根统一门禁通过、独立 G 审核完成与性能验收完成分别由各自证据证明；本静态记录不替代其中任何一项。

- G 独立审核：`/private/tmp/cutover17-workbench-independent-review.{md,json}`。已核销 18 项生产语义、20 项完整主体比较与 24 个真实原子读取，未发现未解决实质漂移；WB18 已确认 external-identity 原持久化→WorkflowError 边界恢复。该报告已绑定同一源码提交 `39c55021d8bb1d030eade4afb3e135e4b5a8a18b`。
- A 资金逐来源证据：`/private/tmp/cutover17-funds-result.{md,json}`；主 JSON 按实际文件 SHA256 引用，不把静态展开标记为数据库执行。

## 8. 最终绑定与新增路由投影核验

- 源码提交：`39c55021d8bb1d030eade4afb3e135e4b5a8a18b`；46 个当前 source 文件均逐字节等于对应提交 blob；旧 work_item 子树在当前树与提交树中均不存在。
- 41 个输入文件、335 个原生产函数、395 个当前生产函数、63 个原测试与75 个当前测试由配套 JSON 实际登记。唯一新增路由 wrapper 计入当前函数，未改变原 DTO 字段/serde 或原路由函数。
- `workbench/dto/view.rs` 去除经授权新增的 `work_item_destination` 文档及函数后，全部剩余字节与阶段16输入一致；其他4个 DTO 文件仍逐字节一致。
- 独立窄核验：`/private/tmp/cutover17-route-wrapper-independent-review.json`。handler_route、document_approval_route、w18_confirmation_scope 的完整定义 token 同阶段00及16；wrapper只转发真实type/object/role，保原typed错误并返回原handler/destination。Import授权成功分支调用它，raw/read-only分支仍原W18常量，无新增I/O、时钟、ID或写入。
- A资金19个文件原审核SHA/当前SHA/提交blobSHA全部一致；G独审与该独立路由核验均绑定同一提交。引用文件哈希由主JSON记录。
- 主 JSON SHA-256：`3d5640483748c44ae3ff544cb29f0cc60532df6678d595d3c5289ad00fbc6c66`。
- 仅静态绑定与来源/合同审查完成；真实性边界不变：真实数据库运行未验证，性能阈值由独立测量证据判定。
