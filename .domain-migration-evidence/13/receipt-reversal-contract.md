# 阶段 13 回款冲正迁移执行合同

## 1. 输入、范围与证据等级

- Before 固定为提交 `de112614e03714bb6d1ca4ac9e25f580c5c273c8`。
- After 固定为 `/private/tmp/erp-domain-crate-13-returns` 当前未提交工作树；第 8 节 SHA256 是本报告静态核对时的文件内容。文件再变更必须刷新相应证据。
- 本分片删除 `backend/services/src/returns/receipt_reversal.rs`；只实施回款冲正命令、本域服务以及原有 `ReceiptReversalProcess`，新子模块均在 `receipt_reversal/` 内。
- 查询负责人已从 Before 提取 detail/View 到 `erp_read_models::returns_center`，确认读取完成后才删除旧源；删除前旧源与本分片保存的输入快照字节一致。
- 本报告属于静态符号、控制流与源码完整性证据。定向 rustfmt、include 路径核对和 `git diff --check` 已完成；**本分片没有运行 Cargo 或测试，不能将下列测试覆盖目标记作运行通过。真实数据库运行未验证。**
- DTO、实体、仓储、审批 adapter/start/cancel、根注册及 HTTP 由其他分片负责。本分片未修改其源码，报告仅引用当前真实 provider。没有修改历史 `tests/**`、Cargo、MongoDB 或提交 Git。

## 2. 唯一符号归属与接线

下表路径均相对 `backend/`；Before 行号定位原输入，After 行号定位本报告 SHA 快照。

| Before 实际符号/位置 | After 唯一位置 | 执行合同 |
| --- | --- | --- |
| `services/src/returns/receipt_reversal.rs:138 create_receipt_reversal` | `crates/erp-processes/src/reverse_flow/receipt_reversal/create.rs:38 ReturnsProcess::create_receipt_reversal` + `crates/erp-returns/src/service/receipt_reversal.rs:21 build_create` | 请求校验留 Process；ID、默认附件、本域构造及规范化下沉；注册/绑定/审计/根事务留 Process。 |
| 旧源 `:174 commit_receipt_reversal` | process `receipt_reversal/commit.rs:40` + domain `receipt_reversal.rs:42 build_commit` | 根回执、原回款读取及审批快照留 Process；命令编号与默认金额/经办复核/发生时间构造归本域。 |
| 旧源 `:331 submit_receipt_reversal` | process `receipt_reversal/approval.rs:51` + domain `receipt_reversal.rs:71 prepare_receipt_reversal_submit` | 先校验版本、后启动本域状态，再进入绑定与 BPM 回执读取。 |
| 旧源 `:361 cancel_receipt_reversal_approval` | process `receipt_reversal/approval.rs:80` + domain `:66 ensure_receipt_reversal_version` | 版本先于 adapter/binding/runtime；本域状态动作由共享 domain approval 提供。 |
| 旧源 `:392 dispatch_receipt_reversal_start` / `:465 persist_cancelled_receipt_reversal` | process `receipt_reversal/approval.rs:98` / `:170` | 保留绑定、clock、context、快照、normalize、prepare、audit 的原位置。 |
| 旧源 `:516 load_receipt_reversal` | domain `receipt_reversal.rs:95 ReturnsService::load_receipt_reversal` | 本域 repository、NoTransaction 及不存在文案保持。 |
| 旧源 `:54 prepare_receipt_reversal_post` | domain `receipt_reversal.rs:104` | 本域读取 → Reversed 首错 → InApproval 守卫；原固定 ReceiptReversalPost 分派等价为第二次同纯守卫。 |
| 旧源 `:76 validate_receipt_reversal_amount` / `:95 persist_posted_receipt_reversal` | domain `receipt_reversal.rs:124` / `:143` | 独立 posted total，排除当前 ID；财务成功后才 mark_posted 再本域 update。 |
| 旧源创建 insert / 启动撤回 update | domain `receipt_reversal.rs:153 persist_created_receipt_reversal` / `:163 persist_receipt_reversal` | 真实本域仓储写入；不另开事务，不增加 clock/ID，仓储原乐观锁行为保持。 |
| 旧源 `:382 reject_receipt_reversal_client_post` | domain `receipt_reversal.rs:85 ReturnsService::reject_receipt_reversal_client_post` | 返回 `erp_returns::Result<Infallible>`；原 ConflictError 文案逐字保留，HTTP 由 root 穷尽处理不可达 Ok。 |
| 旧源 `:646 load_receipt_reversal_context` / `:685 persist_bound_receipt_reversal_document` | process `receipt_reversal/context.rs:19` / `:36` | 财务读取与往来主体/可选客户保留上层；真实发布定义绑定及注册由 Workflow provider 执行。 |
| 既有 `crates/erp-processes/src/reverse_flow/receipt_reversal.rs` | 同路径 `ReceiptReversalProcess`、`apply_receipt_reversal_final_post`、`ReceiptReversalPosting`、`post` | 原最终过账根与已有 Port 保留；写后读取切唯一 ReturnsReadService。 |
| 原最终过账 `DatabasePosting::refresh_sales` | 同路径 `AffectedSales for DatabasePosting` + 子模块 `sales_refresh.rs:16 refresh_affected_sales` | 只抽取真实 fresh-load 和逐 sales runner；财务 provider 及销售 provider 不复制。 |

根注册必须保持 `reverse_flow::receipt_reversal::ReceiptReversalProcess` 导出与 domain `service::receipt_reversal` 公开模块。普通命令为 `reverse_flow::ReturnsProcess` 的多文件 impl；私有子模块登记已在本分片所拥有的 `receipt_reversal.rs` 完成。

## 3. 创建与一次提交的原顺序

### 3.1 创建

1. `req.validate()` 必须先于 domain `build_create`；构造内部先 `ReceiptReversalId::new(next_id())`，按原字段顺序组装 Data，再执行 `ReceiptReversal::new`。`occurred_at` 使用请求值，附件继续 None。
2. Process 读取原回款 context（NoTransaction）并取 `counterparty_party_id`；`customer_id` 允许 None，不额外拒绝。context 不检查 posted 状态，保持原创建行为。
3. 组装 BindPublishedDefinitionCommand → new_registered_document → create audit，随后创建根事务。
4. 同 session 固定执行：对象可读检查 → `services::workflow_compose::bind_published_definition_on_document_create` → 绑定非空守卫 → attach_published_binding → business_documents.create → domain reversal create → audit create。
5. 事务成功后调用 `self.reads().receipt_reversal_detail`。不在领域层读取 View 或审批绑定。

### 3.2 Commit

1. Validate → `CommandReceipt::from_payload` → `committed_resource_id`。prefix 保持 `receipt-reversal-commit-`，action 保持 `receipt_reversal.commit`，resource 保持 `receipt_reversal`；原完整请求参与指纹。
2. 同载荷回执命中直接读详情；否则原回款 NoTransaction 读取，缺失文案为 `原客户回款不存在`，捕获实际 ID/version。
3. `build_commit` 在原实体构造位置执行：先 next_id；编号继续 `CZ-` + SHA256(actor + `|` + trim(key)) 前 8 位；amount 为请求值或原回款全额；reviewed_by 为 `finance_reviewer`；occurred_at 在同字段构造位置调用 Instant::now。原 reason 改为借用请求后的同值 clone，不改变已冻结指纹、规范化或错误顺序。
4. adapter → start 本域审批 → subject → **再次读取**原回款 context → object readable → 第二次 Instant::now → snapshot → document → create/submit/command 三审计对象。不得以第 2 步缓存移除 context 读取。
5. 根事务内重新 load 原回款 → `ensure_posted_source(actual_version, expected_version, is_posted, ...)`。版本冲突必须先于非 Posted 的业务错误。
6. 同 session 固定执行：重验源 → binding/document → bound graph → start_input/prepare_start → domain reversal create → Apply 时 runtime → create audit → submit audit → command audit。保持原 commit 对 PreparedExecution 分支的处理，不插入普通 submit 的额外写入。
7. 事务成功使用新 ID；事务失败后仍只调用原 `committed_resource_id` 一次，命中返回已提交 ID，未命中返回原事务错误，回执读取错误仍原样传播。详情读取保持事务之外。

## 4. 普通 submit/cancel 与恢复差异

- `submit` 固定为 Validate → adapter → domain load → entity version → start_approval → subject → binding/require → now → context → snapshot → start command → object readable → graph → BPM start receipt → build input/prepare_start → persist_start → detail。
- 新 `prepare_receipt_reversal_submit` 只收拢原相邻的版本与状态检查；不增加查询、时间、ID 或写入。旧版且状态非法时先报原并发冲突，不改变实体或审批版本。
- `persist_receipt_reversal_start` 真实 provider 在 `reverse_flow/start_approval/receipt_reversal.rs`：Replay 直接返回；Apply 为 audit 构造 → 根事务 → BPM receipt 首写 → BusinessDocument start guard → runtime → domain update → audit。runtime 的实例/assignee/execution、snapshot ID、逐 WorkItem ID 仍在原 helper/原分支生成。
- `cancel` 固定为 Validate → load → version → adapter → binding/require → subject → load_cancel_runtime → now → normalize key → build input/prepare_cancel → domain action → audit 构造 → persist_cancel → detail。取消动作仍由 process adapter 分派到 `erp_returns::service::approval`，不把 Workflow 枚举引入 domain。
- **回款冲正 submit 在 Before 没有退款的 preflight replay 或 8 次 fresh recovery 根，After 同样不新增。8 次恢复是客户/供应商退款的独立合同。** Commit 的根命令回执与普通 submit 的 BPM 回执不得统一或互相替换。
- commit/source validator 的 `ensure_posted_source` 与 submit/version helper 都沿唯一 domain shared/version_conflict provider；本分片未引入新的异常转换文案。

## 5. 最终过账真实 provider 与同 Executor 顺序

1. `ReceiptReversalProcess::post_receipt_reversal` 拥有根事务；`post_receipt_reversal_in_transaction` 接受审批当前 ClientSession，不建立内层事务。两者调用同一 `apply_receipt_reversal_final_post`。
2. prepare：domain reversal find → NotFound → Reversed 拒绝 → InApproval/最终动作守卫 → finance `load_posted_receipt`（存在先于 Posted）→ domain `posted_reversal_total_by_receipt(original_id, current_reversal_id, executor)` → CumulativeAmountLimit。
3. 原 `post` Port 顺序固定为 `reverse_finance → post_reversal → audit → refresh_sales`。每个 await 使用同一个 `&mut dyn Executor`；首个错误直接返回并停止后续能力。
4. reverse_finance 唯一真实 provider 为 `erp_finance::service::receivable::receipt_reversal::reverse_receipt_allocations`，输入仍为 finance CustomerReceipt clone、reversal.amount、reversal.occurred_at、actor.id、同 Executor。该 provider 与 Before 逐字节一致。
5. 财务内部顺序保持：查 allocations → plan_reverse → seq range → batch entry/account facts → 逐 chunk revert_settlement（false 立即业务错误）→ 逐 reverse row next_id/new/create → 原 receipt transition Reversed/update。REVERSE 分配的 receipt/entry/reverses_allocation_id、occurred_at 和逐行 ID 时点保持。
6. domain post_reversal 在财务成功后 `mark_posted`，随后原 repository update；审计再构造 resource_log(`receipt_reversal.post`, `receipt_reversal`, reversal.base.id)，再写 audit。审计 ID/时间由原 AuditActor provider 在该时点生成。
7. `refresh_affected_sales` **只能在审计写入成功后**调用：真实 `receipt_allocation_sales_order_ids` 重新查询该原回款全部 allocations → offset facts → sorted/dedup SalesOrderId → 原顺序逐条 `order_to_cash::progress::update_sales_order_money_progress`。不得复用事前 snapshot。
8. 每个销售刷新仍传 `actor.id().to_string()` 和 fulfillment `None`，与所有此前写入共用 Executor；不并行，不先触碰后续销售单。新窄 Port 只把这个实际读取及逐条动作暴露给同一生产 runner。
9. domain 仅访问 returns repository；不依赖完整财务/销售/Workflow/Audit/身份聚合或旧三层。构造保留稳定 ID 关联，不要求 CustomerAccount 存在。

## 6. 测试清单与证明边界

原 10 项测试全部保留，新 6 项，总计本分片 16 项；名称完整性由静态提取核对，**没有执行**。

| 测试归属 | 数量 | 原断言与新增行为范围 |
| --- | --- | --- |
| `receipt_reversal/receipt_reversal_approval_tests.rs` | 原 8 | 原 create/对象读取/submit/final/cancel/旁路关闭/version/batch 测试名称保留；include_str 路径切唯一实际拆分文件。它们仍是原结构与局部行为证据，不充当新的运行证明。 |
| 主 `receipt_reversal.rs::tests` | 原 2 | 原 finance/status/audit/sales 顺序、同 Executor；每一失败步骤仅执行原前缀，原 ConflictError 传播。 |
| domain `receipt_reversal.rs::tests` | 新 3 | commit 原全额/显式部分金额和无 customer 参数；旧版+非法状态时版本首错且实体不变；正常 submit 只递增 subject version，客户端 post 原冲突不变。 |
| `receipt_reversal/sales_refresh.rs::tests` | 新 3 | 复用实际 `post` 与 `refresh_affected_sales`。替身在 audit 步更新 allocations，证明 fresh load 读取新 sales；load 失败不刷新；第二 sales 失败不访问第三。成功/中间失败都断言同一 Executor 地址。 |

新增过程替身的 Executor 为非零结构体 `TestExecutor { _identity: u8 }`。上述替身不能证明真实 MongoDB 回滚、并发、未知提交恢复或生产财务余额正确；这些保持为真实数据库运行未验证。

## 7. 已执行静态门禁与后续集成要求

- 8 个负责文件使用 `rustfmt --edition 2021 --config skip_children=true` 定向格式化；同集合 `--check` 退出 0。
- `git diff --check` 退出 0。
- 所有迁入 include_str 目标逐项检查存在；原 10 个测试名称全部存在且当前 16 个测试名称不重复。
- finance receipt_reversal provider 字节与 Before 一致；负责文件不含 `services::returns`、`entities::returns`、`database::ReturnsExt`、`crate::errors` 旧来源。
- root 必须统一执行 Cargo/check/clippy/lib-test 与边界脚本并记录真实结果。未执行项不能因本文静态结论标记通过；本分片不重复启动 Cargo。
- 根与其他分片若修改本文列出的共享 provider，必须逐符号核对查询/写序、首错、clock/ID 和同 Executor；若修改第 8 节负责文件，必须刷新哈希及对应行号。

## 8. 内容指纹

### 8.1 Before

| 路径 | SHA256 |
| --- | --- |
| `backend/services/src/returns/receipt_reversal.rs` | `605c6d7845d97b7c979d6d694e056154eef985a54d27338450b154695a21e76c` |
| `backend/crates/erp-processes/src/reverse_flow/receipt_reversal.rs` | `32687b4377fae42ef1a82e6bb13dbe566eda47403136d473f6136ebc56440140` |

### 8.2 After 负责文件

| 路径 | SHA256 |
| --- | --- |
| `backend/crates/erp-returns/src/service/receipt_reversal.rs` | `92d903fc6e9850dfcd2088ff6cc3adce0efbcc7ffc1292603a69580782e5dd60` |
| `backend/crates/erp-processes/src/reverse_flow/receipt_reversal.rs` | `57733e4753a67426d32bd50505398401327f7dab84f7b3ccd9bfb1cd7e19130b` |
| `backend/crates/erp-processes/src/reverse_flow/receipt_reversal/approval.rs` | `3d2964cac96de732ba1d3093202a3fcd728f82f5acc5cddc2b91e84aa50fe0cf` |
| `backend/crates/erp-processes/src/reverse_flow/receipt_reversal/commit.rs` | `10ac0bff7939a0837b4e57cbf516b1c6ee97f43bfa0aa0c5006fde5b1d65fff4` |
| `backend/crates/erp-processes/src/reverse_flow/receipt_reversal/context.rs` | `365a23df98461f09ff17df5ae9d68dbc026c3ab8ad700f9ffc3f126b6f185a56` |
| `backend/crates/erp-processes/src/reverse_flow/receipt_reversal/create.rs` | `e106ba5c4fb825636ad2508ceb2e48153e5d6d3ac779cd8271891f49b30c16dc` |
| `backend/crates/erp-processes/src/reverse_flow/receipt_reversal/receipt_reversal_approval_tests.rs` | `dd26bdf56a9062546726f7da9c6a60eb4f8afb981accc4c6b0f5469ab3da7512` |
| `backend/crates/erp-processes/src/reverse_flow/receipt_reversal/sales_refresh.rs` | `0e8ce12d5d484713d0a06e0fc7a29daf9b61b2954f5d0436be8e7320d553212b` |

### 8.3 未改变的真实财务 provider

- `backend/crates/erp-finance/src/service/receivable/receipt_reversal.rs`：`a4f46c57532009b3f302f687100bb123211bd2a8b481a7204b8b22c3e273de25`；与 Before 逐字节相同。
