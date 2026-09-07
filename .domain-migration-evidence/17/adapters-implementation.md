# 阶段 17 组合适配器实施验收合同

## 1. 输入和文件核销

实施树为 `/private/tmp/erp-domain-crate-17-cutover`，授权输入为 `a537414eb8f78c43ebc383a3457a45c437dececc`。原五个 adapter 与代码提交 `72a0c79a2261d33699b869329376e534edfb1ef4`、只读合同快照的实际字节一致；删除前再次逐 SHA-256 核对。输入重绑见 [input-rebind](/private/tmp/cutover17-adapters-input-rebind.json)。

E 已实际迁入 10 个 owned 叶：四个 identity/support adapter，workflow 的 authorization/audit/object_facts/purchase_responsibility/w29_close，以及原 work_item_authorization 的真实 import。五个旧 `services/src/{identity_audit,identity_compose,support_audit,support_documents,workflow_compose}.rs` 已删除。集成负责人写入 workflow 根及注册、错误边界；C 的唯一 WorkItemFactsReader 已接线。E 未修改共享根、Cargo/lib/HTTP 或历史 `tests/**`。

[当前逐符号证据](/private/tmp/cutover17-adapters-implementation-evidence.json) 覆盖 **78 个原生产函数**：65 个原函数体 token 相同；12 个因真实 reader/provider/流程函数委派或共享 ErrorCode 调整；原 map_workflow_code 的 21 项等值转换由 root 收敛为直接共享 workflow ErrorCode。原五源没有 inline test 或 ignore，原测试迁入数量为 0。

当前 10 个 E 叶和 1 个 root workflow 根的 hash、行号和快照已保存，供后续 gate 修复及 sourcecommit 封存逐字核对；最终源码已绑定 `39c55021d8bb1d030eade4afb3e135e4b5a8a18b`：10 个 E 叶和 1 个 root workflow 根均逐字验证 commit blob = 当前源码 = 原 after-snapshot，零文件漂移。另 10 个实际共享装配/错误/reader/入口依赖均已核对 commit blob = 当前源码，未宣称此前未捕获的共享文件与旧快照等价。

## 2. 已实现的生产路径

| 公开入口 | 真实目标 |
| --- | --- |
| `adapters::identity::shared_rbac_service` | 构造原 MongoIdentityAudit，共享身份域原 RBAC 服务。 |
| `adapters::{identity_audit,support_audit,support_documents}` | 原 Prepared 元数据/审计写入、文档登记读取及各域错误转换。 |
| `adapters::workflow::WorkflowAuth` | 原完整身份权限、组织覆盖、policy session 与错误处理，不重新实现身份事务。 |
| `adapters::workflow::WorkflowAudit` | 原审计写入、命令回执读取及资源审计投影。 |
| `adapters::workflow::WorkflowObjectFacts` | 四项读取委派 C 唯一 authority reader；职责分离仍为空结果；采购/W29 委派本流程真实 runner。 |
| `workflow::purchase_responsibility::{purchase_order_fulfillment_scope,reassign_purchase_order_owner}` | MongoPurchaseResponsibility → 实际 scope/reassign → 原采购实体规则及 owned update。 |
| `workflow::w29_close::{prepare_w29_close,persist_w29_close}` | 原 W29CloseDecision → MongoW29Close → 实际 persist 两分支及原集成实体。 |
| `workflow::work_item_authorization::WorkItemAuthorizationAdapter` | 原实现保留，仅接本模块 factory/WorkflowAuth，仍只向 RM 暴露五项授权事实。 |

原四个 workflow factory、account_fact、bind/attach helper 已由 root 应用候选文本。所有本片带 Executor 的方法继续使用原 caller Executor；未新增 NoTransaction、事务、ID、now 或外部发送。W29CloseInput 仅聚合原借用参数和 passed closed_at，构造无 I/O。

## 3. 真实 runner 与 provider 验证合同

采购 Port 的 load_order/open_tasks/persist_order 分别直接调用原 purchase_orders.find_by_id、work_items.list_open_fulfillment_by_responsibility_key、purchase_orders.update。生产 ObjectFactPort 两方法与新增替身测试均调用同一 scope/reassign；不是独立模拟步骤列表。

W29 Port 的 replacement/error_task/persist_task/difference/latest/persist_resolution 分别调用原 work_item 查询、集成错误任务查询/CAS、差异查询、最近决定查询、决定 insert。difference 只将原已读完整实体映成 Option<()>，缺失判断仍在原 runner 位置；没有新增读取或变化政策。

三个实际 runner 的全部函数体，经过**显式将原物理仓储调用替换为对应生产 Port 方法**后，均与输入 token 相同。W29 仅额外解构 W29CloseInput，rustfmt 添加的尾逗号已单独规范。该展开结果、预期/实际 token hash 保存在 evidence 的 explicit_runner_expansion_checks；原 helper/map/domain 调用、分支、首错、ID 和时间逻辑没有被重新编写。

load、counterparty_numbers、counterparty_is_active 三个读取的完整错误路线是 RM 原变体 → root `From<erp_read_models::Error>` → 原 map_service → WorkflowError。external_identity_map_exists 保留输入唯一例外：reader 返回 `persistence_core::Result<bool>`，adapter 直接 `.map_err(erp_workflow::Error::from)`；不经过更广的 RM/Process duplicate-index 字典。该例外的原查询、当前 provider 与 SHA-256 已单独记录。RepositoryError 仍经过 WorkflowError::from 做原二次分类；已分类的 ReceiptDuplicate/TransientTransaction/OutcomeUnknown 保留原 persistence error。ErrorCode 直接共享 workflow 唯一 21 值，新增测试穷举 ALL。

Identity/Support persist 仍先原 AuditLog::new 再回填完整 Prepared BaseModel；WorkflowAudit 的 Prepared 本来不含 BaseModel，仍在 persist 构造时间。原两种时间合同没有人为合并。PolicyTxnError 的 caller/identity/persistence/application 分支、Display/Debug/Error 与真实 identity policy transaction 顺序保持。

## 4. 测试入口与验证界限

新增 **15 个**源码可达测试：

| 数量 | 所属叶 | 必须验证的实际行为 |
| ---: | --- | --- |
| 4 | identity_audit、support_audit | 原 Prepared 完整元数据和业务字段重建；无效字段首错；带 persistence error 的类型转换。 |
| 3 | workflow/authorization | 14 类错误及 21 code；原 caller error 包装合同；角色/用户覆盖交集、Company 与空覆盖。 |
| 3 | workflow/purchase_responsibility | 同一非 ZST Executor；scope 后 reassign 的事务内重读；原实体责任变更/CAS；逐 provider 失败停止；缺失/终态/责任不符/集合变化/空目标。 |
| 5 | workflow/w29_close | 全 8 ErrorClass 的任务类型派生；原 closed_at；两种 resolution action、连续序号及固定回执派生 ID；replacement 与两分支每步失败停止；缺失/terminal/type mismatch/非法证据与不支持对象。 |

纯错误包装测试不等于执行真实 RBAC policy 事务；源码已追至身份真实事务 provider，Mongo 事务、并发与取消行为由本次静态保真核对，未运行数据库。

E 已执行 10 个 owned 叶 `rustfmt --check --config skip_children=true` 和 owned `git diff --check`，均 exit 0；10+1 当前文件 SHA-256 全部与记录一致。集成负责人实际执行并报告最终 fmt/check/严格 clippy/lib 门禁通过。E 核对原始日志并汇总所有 test result 块为 **3655 passed / 0 failed / 68 ignored**，15 个本片新增测试在原日志逐一匹配 ok。Cargo 门禁没有由 E 执行；真实 Mongo 验证仍未执行。

## 5. 后续封存要求

1. 统一 gate 发现真实错误时只修改对应 owned 叶，并刷新原 symbol/runner/test 映射；不得改生产业务以满足错误测试期望。
2. root/C/G 更改共享根或 reader 后，记录依赖实际 hash 并核对接口，不能重新复制其实现。
3. 最终 sourcecommit 给定后逐项 git show commit:path 比较当前文件与既有快照；hash 改变必须展开实际差异，再填写提交绑定。
4. 门禁报告分别记录静态检查、纯替身测试、编译，以及未运行的 Mongo 验证边界。不得把 source-map 当作真实数据库事务验证。

## 6. 最终根门禁日志封存

以下日志均由集成负责人执行，E 仅读取原始字节并记录 SHA-256。

| 日志 | SHA-256 |
| --- | --- |
| `erp-cutover17-lib-tests-sealed.log` | `2fb05cc9803e1b527afb67067c28f606d2d785c58965ea75a9e89cd30fa54753` |
| `erp-cutover17-clippy-sealed.log` | `afe2e2111e7e30fdd3ecf2a38664b90e9f5b2b23e63940cb0020a2fca5cd7481` |
| `erp-cutover17-check-sealed.log` | `90380e0beeef1e7c0b8674e4011583d27231dd2726b97657a713241706df7266` |

最终 sourcecommit、逐 blob 结果、10 个额外共享依赖、15 项测试原日志匹配及零漂移 diff 文件统一登记于 implementation-evidence.json。后续性能测量期间 E 暂停 Cargo 与全树重扫描；本次封存没有修改仓库源码。
