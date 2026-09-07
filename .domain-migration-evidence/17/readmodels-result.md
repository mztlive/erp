# 阶段 17 非工作台读模型消费者交付合同

## 1. 输入、范围与实际改动

- 唯一实施树：`/private/tmp/erp-domain-crate-17-cutover`。
- 输入提交：`a537414eb8f78c43ebc383a3457a45c437dececc`；其业务源码承接 `72a0c79a2261d33699b869329376e534edfb1ef4`。
- after 已绑定最终源码提交 `39c55021d8bb1d030eade4afb3e135e4b5a8a18b`。49 个被编辑文件在写入前逐一匹配 input 的真实 commit blob；冻结后再次逐一读取 after commit blob，49/49 与原审查 after SHA 相同。原文件另保存在 `/private/tmp/cutover17-readmodels-before`。
- 本片只编辑非 `workbench` 的 RM 消费者，不编辑 C 的 `workbench/**` 或 root 的 `lib.rs`、`errors.rs`、`test_indexes.rs`、Cargo/HTTP/Process。
- 共 49 个文件、63 处已锁路径替换：33 个 `services::`，24 个 `crate::errors::`，6 个 `database::ensure_indexes`。未新增 facade、业务方法、状态、查询或错误类型。
- 各文件 before/after SHA、每处替换原文和行号见 `/private/tmp/cutover17-readmodels-result.json`；允许替换日志另存 `/private/tmp/cutover17-readmodels-edits.json`。

## 2. 新唯一入口

| 原入口 | 新入口 | 保持项 |
| --- | --- | --- |
| `services::{Error, Result}`、`services::Error::...` | `crate::{Error, Result}`、`crate::Error::...` | 使用 root 新真实错误模型，原 From/明确构造位置保持。 |
| `crate::errors::{Error, Result}`、`crate::errors::Error::...` | crate 根真实 `Error/Result` | 不保留旧 services 别名；测试中的精确错误匹配一并切换。 |
| 六个 ignored 仓储测试中的 `database::ensure_indexes` | `crate::test_indexes::ensure_indexes` | root 拥有唯一 cfgtest 索引组合；阶段 17 按批准修复为 27 次窄调用覆盖 19 领域，并恢复阶段 00 的 161 步原序。六个消费者仅换调用路径，不新增 RM→Process/test-support 业务依赖。 |
| 原领域实体/仓储/provider | 原目标领域直接路径 | 本片没有旧 `entities::` 业务类型；不得改回中转层。 |

非 root/C 全量 RM 源码的词法扫描结果：活动 `services::`、`database::`、`entities::` 引用均为 0。统计排除注释/字符串，明确排除 root/C 的所有权文件；整个 crate Cargo normal/build/dev 图仍由 root 统一验证。

## 3. 不可变业务与特殊错误合同

以下八处仍先执行原 `find_approval_binding(..., NoTransaction)`，再通过 root Error 的 typed From 转换；仅 `NotFound` 返回 None，其他错误保持传播：

1. `sales_center/review/query.rs::load_change_binding`
2. `finance/receivable/customer_receipt.rs::customer_receipt_view`
3. `purchase_center/query.rs` 的单据详情绑定读取
4. `purchase_center/change/query.rs::load_change_binding`
5. `returns_center/customer_refund.rs` 的详情绑定读取
6. `returns_center/supplier_refund.rs` 的详情绑定读取
7. `returns_center/receipt_reversal.rs` 的详情绑定读取
8. `returns_center/payment_reversal.rs` 的详情绑定读取

- `sales_center/order/approval_query.rs` 和 `purchase_center/approval_query.rs` 原显式 `ValidationError(error.to_string())` 保持，不改用其他 From 分类。
- `sales_center/order/query.rs` 原 `.map_err(Error::Logic)` 保持；finance 权限解析 Internal、账户默认项冲突 BusinessLogicError、回执/责任 ConflictError 与原错误文案均保持。
- 各仓储已有 `persistence_core::{Error, Result}` 和其 EntityMetadataOutOfRange/OptimisticLockingError/DatabaseError 不改为应用错误。
- `WorkItemAuthorizationReadPort::authorize` 仍返回 `erp_workflow::Result<AuthorizedTaskFact>`，不新增 Executor、不提前调用。16 阶段固定的五个事实字段与工作流枚举保持。
- `supplier_center/fulfillment_access::ensure_task_actor_eligible` 只改变 Result 所属，其 `let _ = (db,item,actor_id,executor); Ok(())` 原唯一实现保持。详情仍在原位置授权、重读 task、调用该 helper；不增加资格规则或第二份 stub。
- 49 个文件没有改变 DTO 字段/serde 属性、查询过滤/排序、ID/clock、业务或审批动作顺序；无事务新增或转移。

## 4. 静态语义与验证证据

执行 `/private/tmp/audit-cutover17-readmodels-result.py` 得到：

- 49/49 文件在只规范化上述允许路径、剔除 `use` 语句排序后，**完整非 import token 序列逐项相等**。该比较覆盖函数、类型、字段、调用、控制流、常量、字符串、所有现存测试及 include 路径。
- 49/49 文件的注释 token 多重集相等；没有遗漏、改写或删除原注释。
- 本片原测试源码不删不增，49 个编辑文件中静态识别 83 个测试定义及 30 个 ignored 标记。因为完整非 import token 相等，测试属性、ignore 文案、断言和 include 字符串保持。
- 定向 `rustfmt --edition 2021 --config skip_children=true <49 files>` exit 0；仅对 49 owned 文件的 `git diff --check` exit 0。
- 本 worker 未运行 Cargo、数据库或历史 `tests/**`。root 的错误 From 细节与最终 Cargo 依赖闭合必须通过 root 的统一 check/strict Clippy/lib/边界/权限门禁接受；上述 token 证明与运行证据分开登记。root 封存日志已观察到库测试 3655 passed、0 failed、68 ignored，以及 check/Clippy 完成记录，日志 SHA 保存在同名 JSON 的 root_gate_observation。

## 5. 接收与冻结

- 后续只处理 root 统一门禁中归本片的必要编译/类型诊断；行为变更不在本片范围。
- 已按每个 after 路径实际读取 `39c55021d8bb1d030eade4afb3e135e4b5a8a18b` 的 commit blob，与本片 SHA 逐一相等后重绑同名 JSON/报告；绑定清单同时记录 blob OID 与 SHA-256。
- 16 阶段结算证据已独立绑定其源码提交；本报告不回写 16 源码、不将 17 类型切换当作 16 业务修复。
