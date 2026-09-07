# 阶段 17 工作台独立只读核销

## 1. 接受范围与证据边界

- 固定 before source：`72a0c79a2261d33699b869329376e534edfb1ef4`；阶段 17 输入提交：`a537414eb8f78c43ebc383a3457a45c437dececc`。
- after 树：`/private/tmp/erp-domain-crate-17-cutover`。独立审核只读取 C/A 工作台 authority/provider/display 生产源码及 E 实际 ObjectFactPort 调用，不修改这些实现，不运行 Cargo、数据库或历史 tests。后续另经 root 授权实现导入路由 wrapper/export，独立性边界见补充节。
- 本核销覆盖 43 个 before 文件、46 个 after 文件。before 由冻结 Git blob 读取；after 在核销结束时重读 SHA，最终 source commit 已按末节逐 Git blob 重绑。
- 验收依据是原生产函数、实际调度和实际 provider 调用；测试名称/fixture 字符串不作为行为正确性的证明。

执行下列不变量并通过 root 统一门禁后，方可封存本报告。完整符号、前后行号和函数 SHA 见同名 JSON。

## 2. 命令调度与 Executor

唯一入口是 `authority/command.rs::WorkItemFactsReader::load`，实际调用 `authority/recipe.rs::load`。真实生产 `ORDER` 和 `CommandFactReads for WorkItemFactsReader` 的分派，与原 loader 调用顺序逐项对应：

`Sales → Purchase → Fulfillment → PurchaseChange → SalesChange → Receivable → Payable → CustomerReceipt → CustomerRefund → ReceiptReversal → SupplierPayment → SupplierRefund → PaymentReversal → Inventory → Settlement → LegacyImport → IntegrationError → Reconciliation → SupplierFulfillment → SupplierOffering`。

20 步串行 await，失败立即经 `?` 返回，传递同一个 Executor。原 procurement_confirmation 只有 `Ok(())`，删除无查询或事实副作用。未增加顶层空 keys 提前返回。

| 场景 | 必须保留的实际行为；本次已按原入口及新调用核对 |
| --- | --- |
| 整体 keys 为空 | 调度仍走 20 步；履约内采购入库、发货、电子交付、服务履约的四个仓储入口仍按原顺序接收空 IDs，来源单号 helper 保留自己的空输入护栏 |
| 销售/采购/变更、应收账户无实际头记录 | 原有 rows 空护栏保留；不增加后续提交/名称读取 |
| 客户回款无实际 receipts | 原提前返回保留；不补读审计 |
| 付款、退款、冲正无实际 rows | 保留按原请求 IDs 读取创建审计和原空来源读取；不机械套用回款的提前返回 |
| 应付无实际 accounts | 仍进入 supplier/来源采购号 recipe；来源采购仓储空输入调用保留 |
| 结算无实际 statements | 原 items/differences 来源调用保留，不新增提前返回 |
| 供给无实际 offerings | 仍按实际 offering IDs 调用 availability 仓储，即使该数组为空 |

24 个实际原子 source provider 均只执行一次原仓储方法，原样转交 `ids, executor`；没有新空输入护栏、排序、去重、事务、NoTransaction 或缓存。独立 JSON 逐 provider 记录方法与函数 SHA。reader 中不新增外部 I/O。

## 3. 命令与显示分别保留的四项差异

| 原差异 | 唯一来源及实际投影 | 核销要求 |
| --- | --- | --- |
| 采购全部原行与显示差异行 | `authority/purchase.rs::purchase_line_counts` 消费所有原行；rich `purchase_submission_brief_lines` 对同次 rows 形成 brief/state；display impact 继续用 `brief.lines.len()+more_count` | 命令计数不能改成显示计数。orders→submissions→sales numbers→lines 的显示插槽保持；命令不查询 sales numbers |
| 命令来源必须有名称，富来源可无名称 | `authority/funds/origins.rs` 的四个 counterpart-only mapper 继续 filter_map；rich 四个 `*_origins_from_rows` 保留原所有 rows，包括 counterparty=None，并从同次 rows 另算命令 map | 缺 counterpart 的首选回款/付款不进入命令 map；rich 仍优先已有首选来源，不因其无名称改选分录来源 |
| 供应商退款往来方 | authority 按 supplier→原付款→原应付分录回退；rich `load_supplier_refund_facts` 只把 supplier 名称赋给 display，来源只进入富简报 | display 的明确 None 不从 authority 回填；客户退款仍分别保留其命令与 rich 来源选择结果 |
| 卡券 flags 与 rich voucher line | `voucher_revision_ids` 只看销售修订的 category/expiry 两个 flags；rich 仍允许 voucher 行经 entry.or_default 创建显示条目 | authority 的影响文案使用 flags；display 文案使用 rich voucher 是否存在。两者均调用唯一文案 helper，不能交换 predicate |

上述四项均按生产入口、查询语句和实际 mapper 数据传递核对；没有先执行完整 command load 再执行 rich load。富显示不增加命令所需之外的重复头查询。

## 4. 权威字段、权限与显示

1. 权威类型直接采用原 `erp_workflow::ports::{ObjectFact,ObjectFactMap,SubjectBrief}`；该文件与冻结 before 字节相同。工作台只封装 `WorkbenchObjectFact { authority, display }`，不复制 root/creator/version 状态类型。
2. 销售、采购、变更、资金、库存和结算的 root/creator 由唯一 authority mapper 决定。采购所有提交保留 subject_briefs；仅 current submission 复制为对象默认覆盖。preferred submission、previous formal sequence 和上次正式相邻配对保持。
3. 只有供应商履约和供给两类约束 subject_versions。履约使用订单 base.version；供给按 offering 停止版本→availability 停止版本顺序，均未停止则无事实。其他对象继续 unrestricted；采购 submission ID 不变成锁版本。
4. `authorized_fields/authorized_item_fields` 保留 permission→participation→subject version 的首个失败条件；参与关系只读 authority.created_by/root，组织仍来自 WorkItem 的 owner_role/owner_organization_id。原显式负责人快捷分支保持。
5. `funds_fact_display(authority, None)` 明确清空显示名称，与未执行覆盖不同；显示应用只读取 display。`from_authority` 的初始复制不是运行时 None fallback。
6. 原 subject 可选字段仍按 frozen16 的 `subject.and_then(...).or_else(object display)` 回退。此行为与禁止 display→authority 回填是两项合同；不将 subject None 改成新的“抑制根默认值”业务信号。已有显式负责人任务的非空 impact 保留条件不变。

20 个完整生产函数体经明确 field qualification 后 token 相等，包含四个授权函数、两个显示应用函数、销售优先提交、采购正式前序、结算分组/文案和 W26/W21 实际版本 loader。该比较不归一业务条件、值或文案。

## 5. 来源名称、首错和窄 Port

- 创建人继续由原 HashSet→Vec 创建审计查询按仓储顺序取每 resource 第一个 actor；缺值为空串。
- party 名称继续 active parties→current revision→active revisions→trim 非空；账号缺法定名时回退业务编号。没有用这一显示 fallback 代替对象活性政策。
- `counterparty_numbers` 只接受 supplier/customer 字面值，保留每个输入 ID 的 find_by_id、重复读取及首错；不加入 status/can_login 过滤。is_active 仍只看返回 map 是否含 ID。
- Process 的前三个事实/名称方法消费唯一 reader，保留 RM→Process→原 Workflow 映射。
- **外部身份映射存在性必须保留独立错误路径**：reader 返回 `persistence_core::Result<bool>`，Process 在原 await 点直接 `WorkflowError::from`。不得经过范围更广的应用 duplicate-name 字典。原 find_by_id 参数、Executor 和 is_some 保持。
- reader 仅持 Database，不持 RBAC、授权服务、事务根或工作流写入 Port。assignment_separation_actors 仍在 Process 恒空；采购改派和 W29 写入不进入 reader。
- 供应商结算显示继续在 differences 与 supplier names 之间读取 difference_evidence→source_evidence。销购变更额外行查询、应收税/到期/券行查询均只留原显示位置。

## 6. 原测试、结论与封存动作

原 63 个测试定义按名称与次数全部保留，after 为 75 个，新增 12 个。JSON 列明各定义文件。新增录制 Port 测试连接真实 command recipe/往来编号 runner，纯 mapper 测试连接上述生产函数；本审核未执行这些测试，不把定义统计当作通过数。

本范围的生产阅读、20 项完整函数 token 比较、24 个 source provider 检查与 18 项独立语义核销未发现待处理的实质行为漂移。该结论仅覆盖本报告明确范围，不替代 root 全 workspace、Clippy、lib、权限、边界或数据库验证。

封存执行：

1. 运行 `/private/tmp/cutover17-workbench-independent-review.py`，校验当前 after 字节与已审核快照，生成同名 JSON。
2. root 冻结 source commit 后，逐项读取实际 Git blob，与 JSON after SHA 比较相等，并登记 after_source_commit。
3. 任一文件变化先查看真实 diff，复核受影响生产调用，再重捕获快照；不得只更新 hash 消除变化。
4. root 将统一门禁结果和本独立静态证据分别登记。当前提交待绑定，未声明 MongoDB 已验证。

当前 JSON SHA-256：`e5ae3d4aa25b1c705a7201554e010ab8faaa90591741c10ff1382b94a06e1e3f`。

## 补充：导入授权路由出口与独立性边界

本报告最初完成只读审核后，root 另授权 G 修复导入确认继承漂移。`workbench/dto/view.rs` 增加 `work_item_destination` 两字段投影，调用未修改的唯一 `handler_route`；`workbench/mod.rs` 只增加公开出口。authority/provider/display 调度均未修改，原 18 项独立核销仍按实际源码重核。两文件 after SHA 已重绑。

新增 wrapper 由本审核者实施，不宣称该 wrapper 经过独立自审；具体实现与原路由 table 的 00/16 对照归 `cutover17-import-view-repair.{md,json}`，交 root/C 独立审阅与统一门禁。

## 冻结源码与统一门禁观察

- 最终 source：`39c55021d8bb1d030eade4afb3e135e4b5a8a18b`。本报告 46 个 after 文件均从实际 Git blob 读取，原证据 SHA、当前工作树与冻结 blob 三方逐字节一致。
- root 执行全 fmt/check/strict Clippy/lib 并报告退出成功；G 读取并哈希封存日志，没有重跑门禁。库测试日志逐 package 汇总为 **3655 passed / 0 failed / 68 ignored**，3 个新增 Import 投影测试均有实际 `... ok` 行。
- 日志：`/private/tmp/erp-cutover17-lib-tests-sealed.log`、`erp-cutover17-clippy-sealed.log`、`erp-cutover17-check-sealed.log`；精确 SHA 与观察边界见 JSON 的 `source_freeze`。
- 本轮只更新 /tmp 证据；没有修改仓库源码、执行 Cargo/数据库或全仓扫描。
- 本 JSON SHA-256：`83e5b773dd2ec896c3214181d930ff43758e092cc0c5e10912ce22c905351a7d`。
