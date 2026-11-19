# 第五阶段授权审查修改清单

## 1. 文书地位

本文件是第五阶段审查后的修改清单。执行人按条目修改代码，并按该条目的验收条件核对。

完成状态：M、B、N 共 48 项均已解决。最终组合验收覆盖 32 个实际受影响的库，执行 `--lib --locked`：7056 passed、0 failed、54 ignored。领域依赖、BPM、权限生成物一致性、修改文件格式及 `git diff --check` 均通过。最终尺寸复验覆盖 290 个生产 Rust 文件，其中 641 个方法体发生变更，最大 49 有效行，无超限方法。验收限于库单元测试和静态检查，不包含真实 MongoDB、外部服务或服务端运行。

第六阶段事项见第 6 节。本清单不要求提前切换 HTTP、worker、Application 工厂、CLI 或前端。

路径从 `backend/` 起。行号以写入时源码为准。验收命令一律为：

```text
env -u ERP_TEST_MONGO_URI cargo test -p <crate> --lib
```

禁止 `cargo test --workspace`。禁止 `--test`、`tests/`、`--include-ignored`、examples，以及任何连接真实 MongoDB 的命令。禁止把 `erp-server` 列为第五阶段验收命令。

依据：`backend/crates/domains/erp-identity/docs/authorization-service-improvement.md`；`backend/AGENTS.md`。

## 2. 必须修改

授权结果错误、写入前未授权、目标形态不匹配、事务边界破坏，按本节修改。

### M-01 银行账号揭示的目标形态（已解决）

- 现状：`crates/foundation/erp-permissions/src/payable.rs` 第 15 行 `PAYMENT_RECIPIENT_REVEAL` 的编码是 `party_bank_account:reveal`。`crates/domains/erp-identity/src/entity/access_control/scope_spec/catalog.rs` 第 358 行把该常量登记为 `ActionKind::Task`。`crates/domains/erp-identity/src/service/authorization/rules.rs` 第 198–203 行对无记录条件的 Task 只允许 `TargetForm::Task`；第 539 行 `task_supported` 包含该常量。`crates/orchestration/erp-master-data/src/customer_profile/sensitive.rs` 的 `reveal_snapshot`（第 50 行）在第 60–62 行对 `field_permissions` 的每一项执行 `require(..., AccessTarget::Operation)`。`field_permissions`（第 200–205 行）在 `BankAccountNumber` 上使用 `party::BANK_ACCOUNT_DETAIL` 与 `payable::PAYMENT_RECIPIENT_REVEAL`。`crates/orchestration/erp-master-data/src/customer_profile/display.rs` 第 15 行把同一常量放入功能资格列表，第 53–55 行 `reveal` 与第 79 行 `REVEAL_BANK_ACCOUNT` 按功能资格判断可见。银行账号揭示因此在事务内被拒绝：目标形态与固定处理分支不一致。
- 依据：重写方案 §3.2.3、§3.6.1。
- 修改要求：保持 `party_bank_account:reveal`。该动作只要求功能资格：在 identity 的 scope catalog 把 `payable::PAYMENT_RECIPIENT_REVEAL` 固定为 `ActionKind::Function`，并从 `task_supported` 删除该常量，使 `rules.rs` 的 Function 分支允许 `Operation`。`sensitive.rs` 第 60–62 行继续对该常量 `require(Operation)`。`display.rs` 继续用同一常量做功能资格可见性。禁止新造第二个权限编码，禁止改成 `supplier_payment:reveal_recipient`。补 `BankAccountNumber` 路径的授权行为测试：允许的功能资格在快照事务内通过；无该资格拒绝；非快照执行器拒绝。
- 修复结果：权限目录已固定为 `Function`，精确任务支持已移除该权限。客户字段揭示和展示继续使用原权限。付款任务及付款资料揭示使用 `Operation`，付款任务保留当前责任人校验，付款和开票继续使用精确任务目标。已补快照内功能资格通过、缺资格拒绝、非快照拒绝、非法记录或任务目标拒绝，以及银行字段展示和付款责任人行为测试。
- 实际验收：`erp-identity --lib` 582 passed；`erp-master-data --lib` 34 passed；`erp-finance-posting --lib` 125 passed，均为 0 failed、0 ignored。付款编排首次验收被并行修改中的依赖签名错误阻断，签名修复后复跑通过。共享 M-11 的 `erp-read-models --lib` 最终验收：703 passed、0 failed、30 ignored，日志为外置盘 `m11-tests-final-r2.log`。本项改动文件的 `rustfmt --check` 与 `git diff --check` 通过。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-master-data --lib
```

### M-02 供应商敏感揭示与命令回执缺少对象范围（已解决）

- 现状：`crates/orchestration/erp-master-data/src/supplier_profile/sensitive.rs` 的 `reveal_snapshot` 只对 `supplier::SENSITIVE_REVEAL` 执行 `require(Operation)`，不对令牌中的供应商执行对象范围。`commands.rs` 的 `command_result_snapshot` 对 `SUPPLIER_PROFILE_COMMAND_READ` 执行 `Operation` 后直接读取结果，不核对关联供应商的详情范围。客户揭示在 `customer_profile/sensitive.rs` 第 57–63 行的顺序是：先功能资格，再字段资格，再 `customer::DETAIL`。
- 依据：重写方案 §3.5.1、§3.6.1。对照客户揭示的既有对象检查。
- 修改要求：供应商揭示在同一快照事务内先检查字段资格，再对令牌中的供应商执行 `supplier::DETAIL`。命令回执在读取结果之后，核对关联供应商的详情范围。不得新造权限编码。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-master-data --lib
```

- 实际验证：上述命令通过，34 个单元测试通过。新增回归覆盖供应商详情授权失败时停止敏感数据读取和回执返回，以及空回执；范围内放行与范围外拒绝的完整链路未验证。仅对本项两个 Rust 文件执行 `rustfmt --check`，并通过 `git diff --check`；未连接真实 MongoDB。

### M-03 供应商命令用功能预检并在创建回放调用创建入口（已解决）

- 现状：`crates/orchestration/erp-master-data/src/supplier_profile/commands.rs` 的 `imported_result`、`command_qualified`、`authorize_command` 调用 `has_permission`。创建回放调用 `require_create`。
- 依据：重写方案 §3.5.1、§3.6.1。`has_permission` 只证明功能资格，不产生本次对象授权。
- 修改要求：命令分支改为 `AuthorizationService::require`，目标形态与该命令既有写入口一致。创建回放执行原创建命令的 `require`，不得改走其他入口。同一快照事务内完成授权后再读、再写。
- 修复结果：创建及恢复保留原请求维护人，统一使用 `PROFILE_CREATE` 与维护人、当前有效主属组织候选执行 `require`；修订及恢复使用 `HANDOVER` 与原供应商记录。所有回放先授权后读取命令，不再使用 `has_permission` 或 `require_create`。导入按名称共享命令键，须先授权当前导入候选，返回回执前再以关联供应商的维护候选执行原创建权限 `require`，不得替换为详情入口。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-master-data --lib
```

- 实际验证：上述命令通过，36 个单元测试通过。新增回归覆盖原创建维护人及修订目标保持、候选与记录授权的快照要求、授权设施失败先于修订回执读取。仅对本项四个 Rust 文件执行 `rustfmt --check`，并通过 `git diff --check`。未连接真实 MongoDB；完整创建、导入回放及范围内放行、范围外拒绝链路未验证。

### M-04 供给详情用列表权限且补齐后未复验（已解决）

- 现状：`crates/read-models/erp-read-models/src/supplier_center/offering/detail.rs` 第 26 行 `detail`，第 36 行 `require_offering(..., supplier_offering::LIST, ...)`。补齐后未 `validate`。
- 依据：重写方案 §3.6.1、§3.6.5。
- 修改要求：详情入口使用详情权限常量。同一快照事务内先 `require` 或 `query`，读取并补齐后 `validate`。列表权限不得作为详情通过条件。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib
```

- 修复合同：详情先以独立 `supplier_offering::DETAIL` 编译完整范围；单 ID 根查询、商品和供应商资料、当前条款及维护人名称均在同一快照内读取，补齐后使用原凭证 `validate` 才交付。非快照入口直接拒绝。
- 必要接入：原目录没有独立 `DETAIL`，`READ_DETAIL` 为 `LIST` 别名；现已新增详情常量和权限目录项，将 `READ_DETAIL` 指向 `DETAIL`，补齐固定范围规则、查询登记及现有授权 Builder 的 Mapping 绑定。详情完整功能资格仅要求 `DETAIL`，不以 `LIST` 代替。
- 实际验证：`erp-read-models --lib` 通过，703 passed、30 ignored；`erp-identity --lib supplier_offering_detail` 通过，2 passed。行为回归覆盖非快照及授权失败先于业务读取、仅列表资格拒绝详情、仅详情资格放行、详情 Mapping 缺失拒绝构造，以及交付凭证过期拒绝。权限生成物一致性、所改 Rust 文件 `rustfmt --check` 和 `git diff --check` 均通过。未执行真实 MongoDB、HTTP 路由或 server 编译验收。

### M-05 开展新业务时用客户详情权限（已解决）

- 现状：销售单新建、草稿保存、首次提交在绑定或更换客户时调用 `customer::DETAIL`。已确认调用点：`crates/orchestration/erp-order-to-cash/src/order_to_cash/command/create/persist.rs` 的 `related_order`；`command/create.rs` 的 `load_customer`；`command/save.rs` 的 `related_order`；`draft_working_copy.rs` 的 `related_order`；`command/contract.rs` 的 `related`；`command/no_contract.rs` 的 `load_customer`；`start_approval.rs` 中 `SalesOrderStartPersistInput::validate` 的 `related_order`。`crates/orchestration/erp-data-import/src/contract_import/archive.rs` 的 `persist_archive` 在 `require_create` 之后拼接 `customer::DETAIL`；当前 `require_create` 只检查合同创建候选，不执行客户选择检查。已有单据只读取已绑定客户的详情入口不在本条。
- 依据：重写方案 §3.5.1、§3.6.1。开展新业务使用 `internal::CUSTOMER_SELECT` 的 `require_current`，关闭历史补充。读取已有单据继续 `customer::DETAIL`。
- 修改要求：新建、首次提交及绑定或更换关系使用 `internal::CUSTOMER_SELECT` 的 `require_current`。合同归档保留合同创建候选检查，客户检查使用 `require_current`。已有单据只保存非客户字段、未变更关系时保留 `customer::DETAIL`；已有单据详情读取不得改成 `CUSTOMER_SELECT`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-order-to-cash --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-data-import --lib
```

- 实际验收：在统一外置 Cargo、target、TMPDIR 环境下执行 `env -u ERP_TEST_MONGO_URI cargo test -p erp-order-to-cash -p erp-data-import --lib --locked`，`erp-order-to-cash` 114 通过，`erp-data-import` 244 通过、1 忽略；修改的三份源码定向 `rustfmt --check` 与 `git diff --check` 通过。未执行集成测试或真实 Mongo 验收。

### M-06 采购单发送邮件拼接详情授权并拆成两个事务（已解决）

- 现状：`crates/orchestration/erp-procure-to-pay/src/purchase_correspondence/mod.rs` 的 `send`（第 73 行）先调用带详情授权的公开读取，再开第二个事务做发送授权。
- 依据：重写方案 §3.1.1、§3.5.3。
- 修改要求：改为同一写事务。事务内先 `require` 采购单发送邮件权限和真实采购单记录，再读取、渲染、写入。不得另行拼接详情授权。
- 处理：发送入口在原写事务内使用固定发送权限授权真实采购单，沿原执行器读取当前内容、校验确认版本、渲染正文并写入确定性发件占用。事务提交后执行外部邮件传输；同键重放及提交未知恢复只读取原结果，不再次发送。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-procure-to-pay --lib
```

- 实际验收：统一外置 Cargo、target、TMPDIR 环境下执行 `env -u ERP_TEST_MONGO_URI cargo test -p erp-procure-to-pay --lib --locked`，145 passed、0 failed、0 ignored；三份 Rust 源码的定向格式检查与本项差异检查通过。
- 验证范围：行为单测执行真实发送准备与授权读取步骤，覆盖固定发送权限、原执行器、授权拒绝和设施错误前置停止、渲染占用成功、过期版本、缺失生效内容及重放身份隔离。未执行真实 MongoDB、集成或外部邮件服务验收；单测不证明数据库回滚、并发提交或实际送达。

### M-07 结算差异补证只检查操作资格（已解决）

- 现状：`crates/orchestration/erp-supply-settlement/src/supply_settlement/evidence.rs` 的 `append_difference_evidence` 与 `append_evidence` 只对差异决定权限执行 `AccessTarget::Operation`。
- 依据：重写方案 §3.5.1、§3.6.3。与 `SettlementAction::AppendEvidence` 的对象范围一致。
- 修改要求：同一事务内增加结算单对象范围检查，检查口径与 `SettlementAction::AppendEvidence` 相同。
- 处理：首次执行和结果恢复均在各自原事务内复用 `SettlementAction::AppendEvidence` 检查结算单对象范围；检查通过后才读取幂等补证结果。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply-settlement --lib
```

- 实际验收：上述库单测 35 passed、0 failed、5 ignored；`evidence.rs` 格式检查与 `git diff --check` 通过。
- 验证范围：5 项真实 MongoDB 单测未执行；本项未独立新增对象放行与越界拒绝行为测试，结算对象检查直接复用既有 `AppendEvidence` 授权实现。

### M-08 结算作废、刷新、差异决定的重放降级授权或伪造操作人（已解决）

- 现状：`crates/orchestration/erp-supply-settlement/src/supply_settlement/void.rs` 的 `void_statement_apply` 先做 `Operation`，已作废路径在领域 `prepare_void` 提前返回。`draft.rs` 的 `replay_refresh` 使用 `AuditActor::new(..., AccountKind::Admin)`。`difference.rs` 的 `recover_difference_decision`、`decide_difference_apply`、`replay_difference_decision` 使用 `Operation` 或伪造 Admin。
- 依据：重写方案 §3.5.1、§3.6.3。
- 修改要求：传入调用方真实 `AuditActor`。重放与首次写入使用同一 `SettlementAccess::require_action`。禁止在重放路径构造 Admin。
- 修复：作废在准备和已作废返回前核验 `Void`；刷新和差异决定在首次执行、回执读取及恢复时统一核验对应 `require_action`，全程传递真实 `AuditActor`。保留作废、差异决定的既有功能权限检查。差异回执必须归属本次已授权的结算单。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply-settlement --lib
```

- 实际验收：上述库单测 38 passed、0 failed、5 ignored；三个命令的失败关闭测试执行真实首次执行、恢复或重放入口。四个修改源码文件的格式检查与 `git diff --check` 通过。
- 验证范围：5 项真实 MongoDB 单测未执行；对象责任人放行与越界矩阵复用既有 identity 政策，本项新增测试不证明真实数据库提交、并发或生产行为。

### M-09 付款、开票、履约写命令在写入之后才授权（已解决）

- 现状：
  - `crates/orchestration/erp-finance-posting/src/finance_posting/payable/payment.rs` 第 396 行 `execute`：第 398–405 行已提交回执直接返回付款，未 `require`；第 420–422 行先 `persist_document`、`persist_payment`，再 `finish_payment`。
  - `crates/orchestration/erp-finance-posting/src/finance_posting/receivable/invoice_commit.rs` 第 45 行 `execute`：第 46–50 行已提交标识在 `post_invoice_apply` 之前返回。
  - `crates/orchestration/erp-finance-posting/src/finance_posting/receivable/invoice_evidence.rs` 第 30 行 `complete_invoice_with_assets` 用 `&mut NoTransaction` 查已提交标识；第 93 行 `execute` 的重放分支在授权前返回。
  - `crates/orchestration/erp-fulfillment-execution/src/fulfillment_execution/delivery.rs` 第 87 行 `update_delivery`：事务内第 112 行 `prepare_delivery_update`、第 120–122 行 `persist_delivery`，第 123 行才 `record_fulfillment_activity`。
  - `purchase_receipt.rs` 第 83 行 `update_purchase_receipt`：第 89 行在事务外 `prepare_purchase_receipt_update`，事务内第 100 行先持久化，第 102 行才记录任务。
  - `delivery_posting.rs` 第 57 行 `post_delivery`：事务内第 83 行先 `prepare_delivery_posting`，第 107 行 `execute_posting` 先写库存与履约。
  - `purchase_receipt_posting.rs` 第 68 行 `post_purchase_receipt`：事务内第 102 行先 `prepare_purchase_receipt_posting`。
  - `electronic_confirm.rs` 第 100 行 `confirm`：第 110 行先 `prepare_electronic_confirmation`，第 122–124 行写入后第 125 行才 `finish`。
  - `service_confirm.rs` 第 302 行 `execute` 先 `execute_confirmation_steps`（第 314 行起加载并确认）。
  - `task/command.rs` 第 56 行 `authorize` 用 `AuditActor::new(actor_id, actor_id, AccountKind::Admin)` 再 `require_assignee`。
- 依据：重写方案 §3.5.1、§3.5.3、§3.6.5。`backend/AGENTS.md`「事务」：决定写入的读取与授权重验使用同一 executor。
- 修改要求：授权放在事务闭包的第一条业务语句，先于加载受保护单据和持久化。发票与付款的重放分支执行同一入口授权，禁止提前返回跳过授权。`task/command.rs` 的 `authorize` 接收并传递调用方真实 `AuditActor`，删除 Admin 构造。直传调用方把已认证 actor 传入上述入口。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-finance-posting --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-fulfillment-execution --lib
```

- 修复合同：付款、票面开票与附件开票命令须在原事务先核验真实操作人的当前任务资格，再读取命令回执及业务单据。回执重放执行相同授权；首次执行分支才解析并消费上传。首次执行仍按请求中的期望任务版本校验业务状态。
- 履约写入口须在原事务先按业务身份核验开放任务，再准备或持久化单据。采购收货草稿读取复用原执行器。任务活动、任务完成及直系调用方透传完整 `AuditActor`，不得按账号 ID 补造 Admin 身份。审计入口登记须与发货更新步骤和付款命令位置一致。
- 实际验收：`erp-finance-posting --lib` 125 项通过；`erp-fulfillment-execution --lib` 111 项通过。真实账号及请求关联透传、入口授权失败停止、服务确认授权先于单据读取的行为测试通过。自有 Rust 文件格式检查、`git diff --check`、`check-domain-boundaries.sh --cutover`（含审计登记检查）通过。
- 验证边界：未执行真实 MongoDB、集成测试或 `erp-server` 编译；窄 Port 调用轨迹不作为数据库回滚或并发正确性的证据。

### M-10 商品导入写入口未授权（已解决）

- 现状：`crates/orchestration/erp-data-import/src/product_import/submit.rs` 的 `submit`（第 40 行）解析并创建任务，无 `AuthorizationService::require`。`product_import/direct_upload.rs` 的 `init_direct_upload`（第 41 行）、`direct_upload_part_url`（第 77 行）、`submit_direct_upload`（第 112 行）、`abort_direct_upload`（第 155 行）无 actor、无 `require`。同 crate 的 `brand_import/mod.rs` 第 86 行对 `catalog::PRODUCT_BRAND_CREATE` 执行 `require(Operation)`；`supplier_import/submit.rs` 对 `internal::SUPPLIER_IMPORT_SUBMIT` 执行 `require`。商品导入权限常量已是 `catalog::PRODUCT_PRODUCT_CREATE` 的别名。路由层 `with_permission` 不作为本阶段已授权依据。
- 依据：重写方案 §3.5.1、§3.6.1。
- 修复要求：提交、直传初始化、分片地址、完成、取消先在短快照内执行 `internal::PRODUCT_IMPORT_SUBMIT` 的 `require(Operation)`，固定依赖 `catalog::PRODUCT_PRODUCT_CREATE` 功能资格；快照成功后再解析或调用对象存储。商品创建权限只支持候选目标，不得直接改成 `Operation`。任务登记和唯一竞争回放在各自数据库事务内重验同一资格。直传初始化、分片地址、取消接收 HTTP 传入的已认证 actor；完成接口沿用真实 actor。保留路由层 `with_permission` 和后台逐行候选创建重验。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-data-import --lib
```

- 实际验收：2026-11-17 执行上述命令，248 通过、0 失败、1 忽略。新增行为测试覆盖入口功能资格允许与拒绝、授权设施故障、非快照拒绝、候选创建范围拒绝及唯一键错误分类保留。受影响的提交、直传及 HTTP 文件通过 `rustfmt --check`；本条改动通过 `git diff --check`。
- 验收边界：未运行 `erp-server`，未连接真实 MongoDB 或对象存储。HTTP 只同步 actor 参数；数据库并发提交、S3 直传和 HTTP 运行结果不计为本次已验证。

### M-11 采购数据范围端口先读单据再按动作名授权（已解决）

- 现状：`crates/domains/erp-procurement/src/ports/data_scope.rs` 的 `qualify`、`require`、`resolve` 接收 `action: &str`。`service/purchase_order/access.rs` 的 `authorized_order` 先 `qualify`，再装载采购单，再 `scope.require`。`require_loaded` 把已装载单据交给 `action: &str` 的 `require`。生产路径仍走该端口：`erp-procure-to-pay` 的变更、运费豁免与授权装配，以及 `erp-read-models` 的采购中心与退货列表。`apps/erp-server/src/wiring/purchase.rs` 与 `reverse_flow.rs` 注入该端口。
- 依据：重写方案 §3.5.1、§3.5.3、§3.6.1。
- 修改要求：端口持有 `Arc<AuthorizationService>`。入口接收 `&Permission` 常量。先 `require` 再读单据。删除以 `&str` 动作名先读后授权的路径。旧 `wiring/purchase_data_scope.rs` 不在本条删除，见 P-02。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-procurement --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-procure-to-pay --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib
```
- 修复结果：生产采购端口固定注入共享 `Arc<AuthorizationService>`，组织筛选复用 `OrganizationDirectoryService`。入口只接收 `&Permission`；持久化采购单必须先按真实 ID 完成统一 `require`，成功后才沿原执行器加载单据；授权拒绝不得加载业务对象。删除字符串 `qualify`、`require` 和 `require_loaded` 路径。采购变更事务内重验使用对应中央固定来源入口；创建并提交保留候选事实检查。
- 实际验收（2026-11-17）：在规定外置环境执行 `env -u ERP_TEST_MONGO_URI cargo test -p erp-procurement -p erp-procure-to-pay -p erp-read-models --lib --quiet`，退出 0。`erp-procurement` 413 项通过，`erp-procure-to-pay` 142 项通过，`erp-read-models` 703 项通过、30 项忽略。行为覆盖授权成功后加载、拒绝及身份设施失败时零加载、原执行器传递、创建候选和其他资源入口拒绝。本条 Rust 文件定向体积检查及 `git diff --check` 通过。
- 阶段边界：本阶段仅修改采购相关 server 注入源码，不编译或测试 `erp-server`，不连接真实 MongoDB。`Application` 共享服务总切换与旧 `wiring/purchase_data_scope.rs` 删除按第六阶段执行；本条未修改采购 Mapping 的自反关系。

### M-12 供应商连接详情与目录在 query 与 validate 之间推进读取阶段（已解决）

- 核实：原连接详情与目录使用非事务 `ReadExecutor`，展示资格在根 `query` 后读取，不满足先展示资格、再最终查询且全部读取共享同一快照的合同。当前 `permissions`、`has_permission` 未推进读取阶段，不认定这两个调用必现 `DATA_SCOPE_CHANGED`；当前跨读取阶段复用凭证按 `ValidationError` 拒绝。
- 依据：重写方案 §3.6.5。
- 修改要求：同一快照事务内先完成展示资格查询，再 `query`，再读取，最后 `validate`。`query` 与 `validate` 之间不得再调用会推进读取阶段的授权入口。
- 修复：两个公开入口通过 `with_transaction` 创建原快照执行器。展示资格在最终查询前读取，详情权限显式传入 `detail_view`，成本资格显式传入目录投影；连接、健康记录、结算资料、商品、任务及筛选项全部补齐后保留最终 `validate`。内部入口拒绝非快照执行器。
- 验收：2026-11-17 执行下列库单元测试，707 passed、0 failed、30 ignored；新增 4 项行为回归通过，覆盖展示资格允许与拒绝、展示权限不能绕过查询资格、非快照拒绝及新查询使旧凭证失效。自有 4 个 Rust 文件 `rustfmt --check` 与 `git diff --check` 通过。未执行真实 MongoDB、完整仓储成功读取或应用运行验证。

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib --quiet
```

### M-13 访问检查未走完整资格与权限来源解析（已解决）

- 现状：`crates/domains/erp-identity/src/service/access_control/inspection.rs` 第 56 行 `AccessInspectionService::inspect`。第 63–65 行只 `require` 检查动作本身。第 80–85 行对可配置范围调用 `DataScopeService::resolve`。`DataScopeService::resolve` 在 `service/access_control/resolve.rs` 第 172 行，`resolve_boundary` 第 183 行，`batch` 第 137 行；`resolve/batch.rs` 第 69 行实现批量边界。生产调用方只有 `inspection.rs` 第 81 行。`facilities.rs` 使用的 `DataScopeService::qualified` 保留。
- 依据：重写方案 §3.3.4、§3.6.1、§3.7.3。
- 修改要求：`inspect` 先 `qualify` 完整功能资格。Scope 动作（含 `ScopeSource::Permission`）用本次合格角色解析范围。无对象上下文时不得报告对象授权通过。删除已无调用方的 `resolve`、`resolve_boundary`、`batch` 及 `resolve/batch.rs`。保留 `qualified`。
- 修复：检查目标通过统一授权的完整资格取证后解析范围；全部 Scope 动作按固定来源和本次合格角色计算，人员范围说明读取实际来源配置。无对象上下文时保留 review，不返回对象授权通过结论。删除旧 resolve、resolve_boundary、batch 及 batch.rs；保留 qualified 和范围版本摘要测试。
- 验收：2026-11-17 执行下列库单元测试，589 passed、0 failed、0 ignored；新增 6 项行为回归通过。自有文件 rustfmt --check 与 git diff --check 通过。未执行真实 MongoDB、应用运行或 erp-server 编译。

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib --quiet
```

### M-14 审批定义授权未强制快照且到期错误分类错误（已解决）

- 现状：`crates/domains/erp-identity/src/service/authorization/definitions.rs` 第 54 行 `require_definition`。第 68 行只读取 `executor.boundary()`，不要求快照事务。第 73–76 行到期返回 `ValidationError("审批定义授权目标或执行阶段已失效")`。`require.rs` 第 33–44 行的快照守卫不覆盖定义候选。
- 依据：重写方案 §3.5.2、§3.6.5。
- 修改要求：只读定义候选强制快照事务边界。到期返回 `DATA_SCOPE_CHANGED`，不得返回 `ValidationError`。
- 修复：`require_definition` 必须取得快照事务边界后才能读取资格；读取结束时必须复验原阶段和候选有效期，失效统一返回 `ConflictError(DATA_SCOPE_CHANGED)`。
- 验收记录（2026-11-17）：`erp-identity --lib --quiet` 594 项全部通过。行为测试覆盖只读定义候选拒绝非快照及无边界执行器、正常快照通过、候选到期及取证中阶段变化；限定文件格式检查与 `git diff --check` 通过。验收不含真实 MongoDB 事务。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib
```

### M-15 供应商品与履约单转交未检查新归属（已解决）

- 现状：`crates/domains/erp-supply/src/service/supplier_offering/handover.rs` 的 `persist_handover` 只通过 `require_offering` 检查原记录。`supplier_fulfillment/handover.rs` 的 `persist_handover` 只通过 `require_order` 检查原履约单。两处都未使用 `AccessTarget::OwnershipChange`。
- 依据：重写方案 §3.5.1、§3.6.3。
- 修改要求：更新前对原记录与新归属执行 `AccessTarget::OwnershipChange`。原记录检查与新归属检查在同一快照事务内、写入之前完成。
- 修复：供给与履约交接必须先应用实体交接规则，再从拟保存实体生成维护人或跟进人及业务组织事实；通过 `OwnershipChange` 同时重验原记录和新归属后，才允许使用原执行器写入。省略组织时保留原业务组织，沿用既有公司范围政策。
- 验收：`env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib --locked` 通过，624 passed；本条新增 6 个真实授权行为测试，覆盖新旧责任人及组织分别拒绝、公司范围跨组织成功、组织省略保留和非快照拒绝。4 个修改的 Rust 文件通过 `rustfmt --check`，差异检查通过。未执行真实 MongoDB、集成或 server 验收。

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib
```

### M-16 结算作废在已作废分支于授权前返回（已解决）

- 现状：`crates/domains/erp-supply/src/service/supplier_settlement/void.rs` 第 23 行 `prepare_void`。第 31 行先 `load_statement`。第 32–33 行已作废时直接返回。第 35 行才 `require_action`。
- 依据：重写方案 §3.5.1、§3.5.3。
- 修改要求：授权移到读取与幂等判断之前。已作废回放执行与首次作废相同的 `require_action(Void)`。
- 修复：`prepare_void` 在原执行器完成 `require_action(Void)` 后才加载结算单并判断幂等。首次作废继续校验状态与版本；已作废回放保留忽略旧版本、返回原实体及 `already_voided=true` 的合同。
- 组合验收：加载步骤返回显式 `Send` boxed future，支持编排事务回调。`erp-supply` 与 `erp-supply-settlement` 合并 `--lib` 通过，分别为 629 passed、0 ignored 与 38 passed、5 ignored，均为 0 failed；日志为外置盘 `m16-integration-retest.log`。
- 验收记录：2026-11-17 执行下列命令，628 passed、0 failed、0 ignored。新增 3 项行为测试覆盖首次与回放缺少功能权限或非制单人时零业务读取、授权后的首次作废与旧版本回放，以及首次作废的版本和状态守卫；测试使用真实授权逻辑及内存加载替身，不连接 MongoDB。受改文件 `rustfmt --check` 与 `git diff --check` 通过。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib
```

### M-17 客户创建用功能预检（已解决）

- 现状：`crates/domains/erp-customer/src/service/customer/access.rs` 第 291 行 `CustomerAccess::require_create` 对 `customer::CREATE` 调用 `has_permission`。
- 依据：重写方案 §3.5.1、§3.6.1。
- 修改要求：改为 `AuthorizationService::require`，目标为服务端构造的 `Candidate`。禁止继续用 `has_permission` 作为创建通过条件。
- 修复：`require_create` 必须复用服务端创建候选检查。首条负责人固定取真实操作人，组织及最近变化边界由原执行器读取；功能资格、归属范围、个人上限、候选有效期和快照边界统一交由 `AuthorizationService::require` 校验。持久化前仍须以实际客户 ID 复验，不接收 HTTP 提交的负责人或授权事实。
- 实际验收：`env -u ERP_TEST_MONGO_URI cargo test -p erp-customer --lib` 通过，102 passed、0 failed。新增行为测试执行真实授权服务，覆盖本人及组织范围允许、缺少权限或正向范围、组织越界、个人上限、组织缺失、事实过期和非快照拒绝。仅验证无数据库授权步骤；真实组织目录读取、Mongo 事务及持久化未执行。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-customer --lib
```

### M-18 创建商品凭证未检查创建候选（已解决）

- 现状：`crates/domains/erp-catalog` 的 `voucher_category_create` 路径上，`voucher.rs` 第 480 行 `new_voucher_product` 创建商品时未调用 `require_create(Candidate)`。
- 依据：重写方案 §3.5.1、§3.6.1。
- 修改要求：创建商品前在同一快照事务内 `require_create`，目标为服务端 `Candidate`。
- 修复：卡券商品使用原事务解析的维护人及主属组织构造并校验 `Product`，随后执行 `CatalogAccess::require_create`；通过 `PRODUCT_PRODUCT_CREATE` 的 `Candidate` 完整授权后才返回商品创建草稿。
- 验收：`env -u ERP_TEST_MONGO_URI cargo test -p erp-catalog --lib --quiet` 通过，313 passed。行为测试执行生产候选构造与真实授权规则，覆盖允许、维护人及组织范围拒绝、缺少商品创建权限和非快照拒绝。未执行真实 MongoDB 或运行时验收。

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-catalog --lib
```

### M-19 商品查重跳过快照边界并用单条越权中断整批（已解决）

- 现状：`crates/domains/erp-catalog` 的 `CatalogPortalService::duplicate_candidates`（`portal/query.rs` 第 274 行）用 `ensure_writable`（`has_permission`）。候选为空时跳过快照边界。范围外商品返回 `Forbidden` 并中断整次查重。
- 依据：重写方案 §3.5.2、§3.6.1。
- 修改要求：入口先校验快照边界，再按商品读取资格授权。空候选仍走该边界。单条范围外商品不使整次查重失败，按既有查重结果排除该商品。禁止用 `has_permission` 代替本次 `require` 或 `query`。
- 修复：查重必须在同一快照中执行 `PRODUCT_PRODUCT_DETAIL` 查询授权及交付前 `validate`，空候选不得跳过。商品读取复用 `authorized_product` 的完整查询条件，无权或缺失商品按空结果排除；设施、数据库及凭证失效错误原样返回。实际匹配写入继续执行商品更新授权。
- 验收：2026-11-17 执行下列命令通过，319 项通过、0 失败。新增 6 项行为测试覆盖快照优先拒绝、空集读取资格、已过滤候选继续返回、身份设施故障、候选设施与数据库故障传播、读取阶段失效。仅格式化本次 Rust 文件，`rustfmt --check` 与本次文件 `git diff --check` 通过。
- 验收边界：行为测试使用固定内存授权设施、真实商品 Mapping 和候选读取替身；未执行真实 MongoDB、集成测试或 server 编译。

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-catalog --lib
```

### M-20 采购自反关系每次新建映射实例（已解决）

- 现状：`crates/domains/erp-procurement/src/authorization/mapping.rs` 第 47 行 `relations`、第 85 行 `relation` 每次为 `self_purchase` 新建不同 `Arc`。`authorization/facts.rs` 第 65 行 `PurchaseFacts::load` 在第 76 行插入 `self_purchase`。全仓规则不使用该关系；仅 mapping、facts 与第六阶段 `wiring/purchase_data_scope.rs` 出现该名字。
- 依据：重写方案 §3.4.1。
- 修改要求：删除 `self_purchase` 关系及 facts 中的膨胀节点。`relations` 与 `relation` 不再登记该键。禁止保留空实现或按资源名返回另一份新建 `Arc`。
- 修复：删除采购 Mapping 的自反关系登记及新建来源实例，所有来源关系请求统一拒绝；采购 Facts 只返回根对象。第六阶段 `wiring/purchase_data_scope.rs` 保持不变。
- 实际验收：`erp-procurement --lib` 通过，414 个测试通过；覆盖采购模型无来源关系、已删关系及未知关系拒绝、根事实保留身份且无来源节点。修改文件格式检查及 `git diff --check` 通过。未执行第六阶段接线、真实 Mongo 或运行时验收。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-procurement --lib
```

### M-21 仓储前置失败未推进读取阶段（已解决）

- 现状：`crates/foundation/persistence-core/src/repository/base.rs` 第 367 行 `Repository::update`、第 401 行 `soft_delete`、第 432 行 `restore` 在 `ensure_active_for_cas` 失败时返回，尚未执行会调用 `next_read` 的写入。`rotation.rs` 第 51 行 `advance` 在第 59 行快照检查失败时返回，第 62 行 `update_document` 失败时也可在 `mongo_ops::update_one` 之前返回。
- 依据：重写方案 §3.5.2。
- 修改要求：上述函数在入口立即 `executor.next_read()`，再执行前置检查与写入。前置失败不得留下未推进的读取阶段。
- 处理结果：`Repository::{update, soft_delete, restore}` 与 `rotation::advance` 均在入口推进读取阶段；生命周期、版本范围、BSON 编码及快照检查失败时，旧读取凭证均失效。公开方法文档已声明此行为。
- 实际验证：`env -u ERP_TEST_MONGO_URI cargo test -p persistence-core --lib` 通过，52 passed、0 failed。新增内联测试执行实际仓储方法，覆盖上述前置失败，断言旧凭证失效、当前执行模式保留及实体元数据不变；未连接真实 MongoDB。两个修改源文件的定向 `rustfmt --check` 与本项 `git diff --check` 通过。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p persistence-core --lib
```

### M-22 审批定义在事务外预读并在退役时写回事务外节点（已解决）

- 现状：`crates/domains/erp-workflow/src/service/approval/definition/command.rs` 第 95 行 `require_graph` 用 `NoTransaction` 装载定义图。`retire.rs` 第 35 行 `retire_definition` 在第 50 行事务外取得图，第 76 行事务使用该图。`replace.rs` 的 `replace_definition_nodes`、`publish.rs` 的 `publish_definition` 同样先事务外装载。
- 依据：重写方案 §3.5.2、§3.5.3。`backend/AGENTS.md`「事务」：决定写入的读取在事务内执行。
- 修改要求：`replace`、`publish`、`retire` 在唯一 `with_transaction` 内先授权再读取定义图。退役不得把事务外节点写入事务。
- 授权前提：写动作必须使用真实 `subject_type` 的 `Candidate`；原修改要求未说明该类型须先从持久化事实取得。事务内先核验当前命令入口资格，只投影目标定义的 `process_kind`，再完成真实类型的 `require`；完整授权通过前禁止加载节点、连线或业务图，不得放宽为 `Operation` 或虚构类型。
- 修复：三个入口取消事务外读图；图读取、草稿状态与版本校验、发布记录读取、收据、写入、审计及结果补齐共用原事务执行器。退役只更新定义文档，节点和连线取自原事务。异常提交回读在新快照按真实定义 ID 重新取证，不复用失败事务的类型政策。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-workflow --lib --locked --quiet
556 passed; 0 failed; 3 ignored
./scripts/check-bpm-boundaries.sh
BPM 边界检查通过
```

- 补充检查：本项六个 Rust 文件的格式检查、生产方法 50 有效行检查及 `git diff --check` 通过。行为测试执行三个生产事务步骤和恢复授权步骤，验证非快照、入口缺失及授权设施失败均先于定义事实、收据和版本读取拒绝。
- 验收边界：未运行真实 MongoDB；未验证实际会话回滚、并发及提交结果未知的数据库恢复。

### M-23 集成差异列表、决定与范围时点（已解决）

- 现状：`crates/domains/erp-integration/src/service/reconciliation_difference.rs` 第 51 行 `difference_page` 在第 76 行 `validate`，第 78 行才 `project_difference_rows`。`access.rs` 第 92 行 `require_decide` 第 98–103 行先 `find_by_id`，第 109 行用库内 `row.base.version` 构造 `Task`。`scope.rs` 第 33 行 `ScopedIntegrationList::from_access` 第 36 行用 `Instant::now()` 覆盖时点；`CompiledAccess::as_of` 在 `repository/authorization/query.rs` 第 271 行。
- 依据：重写方案 §3.5.2、§3.6.4、§3.6.5。
- 修改要求：`difference_page` 在补水之后 `validate`。`require_decide` 先按真实差异对象授权，再进入业务读取；固定规则必须核对决定资格与当前处理人。`from_access` 使用 `access.as_of()`，禁止 `Instant::now()`。
- 无任务约束：现有调用方为 `DirectReconciliationCommand`，不包含正式任务；`expected_difference_version` 表示最新决定序号。决定授权必须使用 `AccessTarget::record`，不得把不可变差异的 `BaseModel.version` 或决定序号伪装成任务版本。新命令继续在原执行器校验调用方决定版本；回放重新检查当前资格、处理人和无正式任务关联，不套用原命令的业务版本。
- 修复：删除 `require_decide` 的授权前仓储读取；在 identity 固定对象规则登记 `DIFFERENCE_DECISION` 的 `AssignedTo` 条件及必需事实。列表在最新决定补水完成后复验原凭证，元数据复用该凭证的冻结时点。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-integration --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib
```

- 实际验证：`erp-integration --lib` 182 通过、0 失败、0 忽略；共享 `erp-identity --lib` 582 通过、0 失败、0 忽略，包含本项两条真实 `require` 行为测试。覆盖身份拒绝先于业务读取、当前处理人允许和拒绝、缺失处理人事实、撤权后零事实读取、伪任务目标拒绝及非快照拒绝。受影响文件格式检查与 `git diff --check` 通过；未执行真实 Mongo、集成测试或应用编译。

### M-24 任务资格由调用方手写，结算复核执行子集为空（已解决）

- 现状：`crates/orchestration/erp-supply-settlement/src/supply_settlement/reviewers.rs` 第 27 行 `qualified_reviewer` 手写 `STATEMENT_DETAIL`、`STATEMENT_REVIEW_DECIDE`、`work_item::DETAIL` 并调用 `permissions`。`crates/orchestration/erp-procure-to-pay/src/procure_to_pay/responsibility/eligibility.rs` 第 42 行 `assignment_owner` 第 43 行伪造 Admin，并手写询价查看、保存反馈、发布权限。`crates/orchestration/erp-order-to-cash/src/sales_inquiry/execution/apply.rs` 第 172 行 `Reassign` 调用 `ensure_reassignment_target`。`crates/domains/erp-identity/src/service/authorization/work_items.rs` 第 43 行 `READ_PERMISSIONS` 已登记 `SUPPLIER_SETTLEMENT_REVIEW` 的读取权限 `STATEMENT_DETAIL`。第 79 行起的 `EXECUTION` 不含该种类。第 258–266 行 `execution_requirements` 未命中时返回空切片，第 245–254 行 `requirements` 因此只加入读取权限。
- 依据：重写方案 §3.3.2、§3.6.3。
- 修改要求：已有任务改派改为 `WORK_ITEM_RECIPIENT` 加 `AccessTarget::Task { id, version }`。任务形成前的候选改为 `WORK_ITEM_EXECUTION_QUALIFICATION`。在 `EXECUTION` 为 `SUPPLIER_SETTLEMENT_REVIEW` / `supplier_settlement_statement` 增加执行子集，内容为现有 `STATEMENT_REVIEW_DECIDE`。不得新造权限编码。不得写成该种类的读取权限未登记。`assignment_owner` 删除 Admin 构造，使用调用方账号与统一任务资格入口。
- 修复：结算复核与询价任务形成前统一使用服务端任务种类及来源候选；账号身份使用当前目录的真实账号类型。结算执行子集登记现有 `STATEMENT_REVIEW_DECIDE`，保留既有读取登记。询价改派使用原任务的精确身份及版本验证接收者，并保留一人一组责任行与有效主属组织检查。询价责任预览允许嵌套统一授权推进读取阶段，交付结果前必须保持原执行器的快照事务证明。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply-settlement --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-procure-to-pay --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-order-to-cash --lib
```

- 实际验证：共享 `erp-identity --lib` 589 通过、0 失败、0 忽略；`erp-supply-settlement --lib` 38 通过、0 失败、5 忽略；`erp-procure-to-pay --lib` 142 通过、0 失败、0 忽略；`erp-order-to-cash --lib` 114 通过、0 失败、0 忽略。新增三条真实授权入口测试，覆盖结算形成前执行子集、结算接收者同角色完整资格及精确版本、询价形成前完整同角色资格。责任预览测试覆盖嵌套读取阶段推进与快照撤销拒绝。受影响文件格式检查与 `git diff --check` 通过；未执行真实 Mongo、集成测试或应用编译。

### M-25 空文件列表跳过快照边界（已解决）

- 修复：`crates/domains/erp-support/src/authorization/files.rs` 的 `require_files` 必须先校验执行器为快照事务，再读取账号资格或文件事实。空集合与非空集合均拒绝 `NoTransaction`；空集合在快照内仍检查账号有效性。
- 依据：重写方案 §3.5.2。
- 修改要求：函数入口先校验快照事务边界，再按文件集合授权。空列表不得绕过该边界。同步修改把空列表放在 `NoTransaction` 上仍期望成功的测试。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-support --lib
```

- 实际验收：上述命令通过，171 项通过、0 项失败。行为测试覆盖两个公开入口的空集合与非空集合拒绝、快照内成功及身份设施失败；使用内存授权设施和快照执行器替身，不证明真实 MongoDB 事务行为。

## 3. 同批修改

事务拆分、授权前读取、仓储执行缺口、错误分类被吞没，与第 2 节同一批修改。本节省不得为搬迁改变授权结果。

### B-01 一个写用例拆成多个事务（已解决）

- 现状：`crates/domains/erp-sales/src/service/sales_selection/publish.rs` 第 40 行 `publish` 在第 50 行使用 `NoTransaction` 做回放与装载，再进入后续提交事务。`crates/domains/erp-workflow/src/service/approval/execution/runtime_service/resume_apply.rs` 第 54 行 `resume_current_approver` 依次调用 `replay_resume`（第 104 行，独立事务）、`prepare_resume_runtime`、`commit_resume`（第 86 行，另一事务）。`crates/domains/erp-party/src/service/party/company.rs` 第 110 行 `create_company`、第 159 行 `update_company` 经 `company_snapshot`（第 210 行）与后续写入、回读各开事务。
- 依据：`backend/AGENTS.md`「事务」：一个用例一个 `with_transaction`。
- 修改要求：每个入口只保留一个 `with_transaction`。回放、准备、写入、回读接收该执行器。禁止在入口下再开事务。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-sales --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-workflow --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-party --lib
```

- 修复：发布、原审批人恢复、公司创建与更新各由公开入口开启唯一事务；回放、准备、当前授权、写入和回包读取必须传递该入口的原执行器。发布精确修订资格复核经目录 Port 透传原执行器，保持显式 actor、请求哈希、幂等作用域及公司同内容回读合同。恢复竞争或未知提交时只在独立只读快照查证收据和已持久化结果，禁止重执行业务写入；通知仍仅写 outbox。
- 实际验收（2026-11-17）：统一外置环境执行 `erp-sales`、`erp-workflow`、`erp-party` 的 `--lib --locked`，分别通过 496、560、132 项（workflow 忽略 3 项，party 忽略 2 项）；直接下游 `erp-order-to-cash --lib --locked` 通过 116 项。新增或加强原执行器透传、授权失败先停止、非快照拒绝及目录失败不重试的行为断言。BPM 边界、所改文件格式检查及 `git diff --check` 通过。未执行真实 Mongo 提交、回滚与并发验收。

### B-02 回款、开票申请、付款申请与销售单提交拆事务或在事务外裸读（已解决）

- 现状：客户回款提交与撤回在事务外用 `NoTransaction` 读取审批绑定或单据，再开写入事务；重放降为详情权限。开票申请撤回先用 `NoTransaction` 准备，再开第二个事务。付款申请提交、撤回、关闭用 `NoTransaction` 读取已提交回执；`close` 在 `payment_request/cancel.rs`，不存在单独的 `close.rs`。销售单 `submit_sales_order`（`command/submit.rs` 第 198 行）在事务外用 `ReadExecutor` 读取，并多次 `with_transaction`。
- 依据：`backend/AGENTS.md`「事务」：一个用例一个事务；决定写入的读取在事务内。重写方案 §3.6.1：重放校验原写命令权限。
- 修改要求：上述每个命令收敛为一个 `with_transaction`。事务外裸读移入该事务，且位于授权之后。重放执行原写命令权限，不得降为 `DETAIL`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-order-to-cash --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-finance-posting --lib
```

- 修复：上述命令的授权、单据与审批准备、精确回执、写入及结果读取使用入口的同一个事务执行器。销售稳定行、工作副本旧行与采购责任预检接收原执行器；下层持久化步骤不得另开事务。
- 重放：付款申请提交、撤回、关闭执行各自原写动作权限，不得使用详情资格。确定性失败直接返回；仅可能已提交的错误允许独立只读查证完整命令回执，不得重跑业务写入。开票撤回保留已提交回执证据；外部副作用继续通过既有事务外步骤或 outbox 执行。
- 验收记录（2026-11-17）：外置构建环境执行 `env -u ERP_TEST_MONGO_URI cargo test -p erp-finance-posting -p erp-order-to-cash --lib --quiet`，分别通过 126、114 项，失败与忽略均为 0。付款命令授权拒绝先于单据读取；回款回放覆盖同一快照执行器、当前资格拒绝及载荷冲突。19 个触及源码文件的体积扫描无超限；`git diff --check` 通过。未执行真实 Mongo、集成、服务端或生产验证。

### B-03 列表、验收取号与寻源取号脱离授权事务（已解决）

- 核实结论：B-15 已将结算列表迁至 `erp-read-models/src/supplier_center/settlement/list.rs`，迁后名称匹配与开放复核任务读取仍在授权事务外。客户验收草稿及原子登记均存在提前取号。寻源仓发取号已在供给任务授权后，但采购候选的创建、提交范围授权仍在仓发取号后。
- 列表合同：必须按列表授权、名称与任务筛选、根查询与统计、授权复验的顺序执行；全部读取使用原事务 executor。领域查询必须校验凭证属于固定 `STATEMENT_LIST` 动作，禁止挪用同资源的其他动作凭证。
- 验收编号合同：草稿创建与原子登记必须在原事务完成创建权限、销售来源及凭证转授授权后取号；原子登记还必须先通过当前办理任务资格。已有草稿与已完成命令回放不得重新取号。
- 寻源编号合同：库存仓发及采购写入前，必须先按原候选构造逻辑校验完整采购拆单批次的创建、提交对象范围；单笔实际写入保留原范围重验。任一发起人命令、对象或任务授权失败必须在取号前终止。
- 编号事务边界：`id-generator/README.md` §4 规定计数器独立自动提交且永不回收，原要求计数器加入同 executor 的方案与该合同冲突，不予采用。业务授权、事实及写入使用原事务 executor；计数器保持独立提交，不得用无效 executor 参数宣称计数器原子回滚。取号后的目标履约责任人任职配置失败或其他业务失败允许留下空号。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply-settlement --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-fulfillment-execution --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-procure-to-pay --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib
```

- 验收记录（2026-11-17）：统一外置构建环境实跑通过：`erp-fulfillment-execution` 113 通过、0 失败、0 忽略；`erp-procure-to-pay` 最后修复后重跑 147 通过、0 失败、0 忽略；`erp-read-models` 713 通过、0 失败、30 忽略；`erp-supply` 629 通过、0 失败、0 忽略。共享 `m16-integration-retest.log` 中 `erp-supply-settlement` 为 38 通过、0 失败、5 忽略。新增行为测试覆盖拒绝后零取号、授权先于筛选、原 executor、凭证动作与边界错配、完整候选批次及拒绝停止；仅格式化本条源码，`rustfmt --check` 与 `git diff --check` 通过。未执行真实 Mongo、集成、服务端或生产验证；替身轨迹不证明真实数据库回滚与并发正确性。

### B-04 授权前读取受保护对象（已解决）

- 现状：
  - `crates/domains/erp-supply/src/service/supplier_settlement/difference.rs` 第 113 行 `prepare_difference_decision` 第 121 行先 `find_by_id`，第 142 行才 `require_action`。`draft.rs` 第 129 行 `prepare_refresh` 第 136 行先 `load_statement`，第 137 行才 `require_action`。
  - `crates/domains/erp-workflow/src/service/work_item/abandon_rework.rs` 第 73 行 `apply_abandon_rework` 第 83 行先 `find_work_item`，第 90 行才 `require`。
  - `runtime_service/query.rs` 第 324 行 `recovery_options_stage` 第 330 行先装载主体，第 331 行才检查读取资格。
  - `cancel_blocked.rs` 第 413 行 `load_cancel_source` 第 421 行先 `find_instance_by_id`，第 424 行才 `ensure_cancel_blocked_authorized`。
  - `query.rs` 第 156 行 `instance_list` 第 162 行调用 `ensure_active_instance_reader`；该方法第 434–435 行用 `NoTransaction` 调用 `has_permission`。
- 依据：重写方案 §3.5.3、§3.6.1。
- 修改要求：授权必须先于受保护对象业务读取。`instance_list` 必须在列表自己的快照事务内授权后查询。`apply_abandon_rework` 必须先按请求任务 ID 完成原 `MANAGE` 对象授权，再读取任务、仲裁回执并核验首次执行的请求版本。
- 修复：结算差异决定、草稿刷新、返工放弃、恢复选项、受阻取消首次执行及回放均已提前执行原对象授权；构造授权目标只使用请求 ID。实例列表已删除事务外 `has_permission` 预检，复用各列表原快照中的 `query`、业务读取及 `validate`。
- 授权合同：`MANAGE` 是范围权限，不支持精确 `Task` 目标；不得额外追加 `DETAIL` 菜单资格。闭任务回放继续按当前 `MANAGE` 资格、原回执和真实终态核验；首次执行的请求版本继续在同一事务中执行 CAS 校验，不要求闭任务版本等于原请求版本。
- 验收记录：`cargo test -p erp-supply -p erp-workflow --lib` 通过，分别为 625 passed 和 560 passed、3 ignored；新增结算准备、返工放弃、恢复选项、四类实例列表、受阻取消及回放的授权失败先停行为测试全部通过。`./scripts/check-bpm-boundaries.sh`、本条源码格式与 `git diff --check` 通过。仅验证库逻辑和静态边界；未执行真实 Mongo、集成、server 编译或生产验收。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-workflow --lib
```

### B-05 先读受保护事实再授权的详情入口（已解决）

- 现状：`crates/read-models/erp-read-models/src/fulfillment_center/access.rs` 第 75 行 `AcceptanceReadService::detail` 在第 84–86 行 `load_customer_acceptance`，第 87–93 行才 `require_object`。`returns_center/purchase_return.rs` 第 278–280 行先 `find_by_id`，第 283 行才 `resolve(..., "detail")`。
- 依据：重写方案 §3.5.3、§3.6.1。
- 修改要求：先完成对象授权，再读取验收单或采购退货单。采购退货详情改为 `&Permission` 常量，不得再传 `"detail"` 字符串。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib
```

- 修复：验收单和采购退货详情必须先取得对应来源的完整 `DETAIL` 查询凭证，再按目标主键连接实际来源完成对象授权。授权查询仅投影验收身份或退货身份及采购来源身份；命中后才装载业务主单、明细和分配，并在原快照交付前执行 `validate`。聚合、BSON 管道、最小身份投影及对应管道用例必须放在现有 ReadModel `repository` 模块，上层只向窄仓储方法传入原凭证和执行器，不得直接操作原始集合。采购退货使用 `purchase_order::DETAIL` 常量，保留来源版本和双快照重验合同；不得改变 K-01 的仅功能资格政策。
- 实际验收：仓储下沉后使用外置缓存和构建目录重跑上述 `--lib` 命令通过，713 passed、0 failed、30 ignored。四个内联回归测试全部通过，覆盖详情资格不足或授权设施故障先于业务读取、实际来源关联及完整授权先于身份投影。`./scripts/check-domain-boundaries.sh --cutover` 通过，夹具 errors=0、工作区 errors=0；不得新增边界豁免。仅格式化所改 Rust 文件，格式检查和限定文件的 `git diff --check` 通过。未执行真实 MongoDB、集成测试或 `erp-server` 构建，未验证 MongoDB 聚合实际执行及真实事务并发。

### B-06 查询执行缺少当前边界或查询模型核对（已解决）

- 现状：`crates/domains/erp-finance/src/repository/supplier_deposit_query.rs` 第 52 行 `account_page` 只核对集合名，不调用 `ensure_current`，不核对资源与输出粒度。`crates/domains/erp-warehouse/src/repository/directory.rs` 第 143 行 `authorized_rows_exist` 直接使用 `stages()`；第 161 行 `authorized_ids_in` 只 `ensure_current`。`WarehouseQuery::ensure_current`（`authorization.rs` 第 245 行）只转发 `compiled.ensure_current`。`crates/domains/erp-supplier/src/authorization.rs` 第 80 行 `CompiledScope::ensure_current` 同样只转发边界，不核对资源、集合与输出粒度。
- 依据：重写方案 §3.6.4。
- 修改要求：执行查询前核对资源、集合、输出粒度与当前边界。任一不符返回校验错误并停止读取。业务过滤不得改写授权阶段已固定的字段。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-finance --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-warehouse --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier --lib
```

- 修复：预存款账户、仓库和供应商查询执行前必须核对固定资源、集合、`id` 行身份、完整对象输出及当前读取边界；模型错配、跨执行器或跨读取阶段返回 `ValidationError`。仓库存在性和候选身份读取必须保留授权错误分类，不得转为数据库错误。业务筛选只追加匹配条件，不改写授权阶段固定字段。
- 实际验收：`env -u ERP_TEST_MONGO_URI cargo test -p erp-finance -p erp-warehouse -p erp-supplier --lib` 通过；finance 524 项通过、13 项忽略，warehouse 62 项通过，supplier 185 项通过。新增测试覆盖固定模型、受限输出、跨边界拒绝及完整授权阶段保留；本条源码 `rustfmt --check`、`git diff --check` 通过。
- 验收边界：仅执行库单元测试；未执行真实 MongoDB、集成或 erp-server 验收。

### B-07 列表拆成两个快照，对象检查自开空事务（已解决）

- 现状：客户列表、供应商列表、合同列表各打开列表快照后再打开版本快照。`erp-customer` 的 `CustomerAccess::require`（`service/customer/access.rs` 第 206 行）、供应商 `access.rs` 的 `require`、合同 `authorization.rs` 的 `require` 自开事务且只返回 `()`。
- 依据：`backend/AGENTS.md`「事务」：一个用例一个事务；复用时传入原 executor。
- 修改要求：每个列表收敛为一个 `with_transaction`。删除只返回 `()` 的自开事务 `require`。调用方把原执行器传入对象检查。
- 修复：客户、供应商与合同列表各由公开入口开启一次事务；分页、范围指纹、客户端版本核对及交付版本复核必须共用原快照执行器，内部读取不得另开事务。三类访问器删除自开事务的 `require`；对象检查保留接收原执行器的 `require_with`。B-15 迁移后的客户资料查询及当前库调用方继续在原事务内执行授权检查，原权限常量和拒绝语义保持不变。
- 实际验收：`env -u ERP_TEST_MONGO_URI cargo test -p erp-customer -p erp-supplier -p erp-contract --lib` 通过；三个领域新增的列表阶段测试均执行真实生产列表阶段，覆盖非快照执行器与缺失授权绑定的先行拒绝。直接调用方 `erp-master-data --lib` 31 项通过；同批 `erp-read-models --lib` 713 项通过、30 项忽略。本条源码 `rustfmt --check`、`git diff --check` 通过。
- 验收边界：仅执行库单元测试；未执行真实 MongoDB、集成或 `erp-server` 验收。旧 HTTP helper 的入口切换归第六阶段，本条不得提前扩展 handler 事务链。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-customer --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-contract --lib
```

### B-08 销售选品服务保存请求级操作人（已解决）

- 修复前：`crates/domains/erp-sales/src/service/sales_selection/mod.rs` 在结构体字段保存 `Option<(SelectionAccess, AuditActor)>`。`new` 只接收数据库，`with_authorization` 再写入该对。
- 依据：重写方案 §3.6.1。`backend/AGENTS.md`「逻辑归属」：多步写入留在 Service 公开入口，入口显式接收操作人。
- 修改要求：构造器注入 `AuthorizationService`。公开方法显式接收 `actor`。删除请求级 `(SelectionAccess, AuditActor)` 字段。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-sales --lib
```

- 修复：`new` 必须注入共享 `Arc<AuthorizationService>`，服务只保存数据库和共享授权依赖。发布、准备、删项与链接生命周期命令显式接收 `&AuditActor`；事务与发布重放均传递本次操作人，授权和领域留痕使用同一身份。删除 `with_authorization`，同步 `erp-order-to-cash` 与 `erp-read-models` 的直接构造及调用。
- 复用合同：创建、复制、改价和密码维护属于调用方同一事务内的已授权步骤；调用方必须先用同一执行器核验相应对象或候选。步骤中的操作人 ID 仅用于领域留痕及幂等作用域。
- 验收记录（2026-11-17）：`erp-sales --lib` 496 通过；最终选品服务定向复验 30 通过。共享实例依次使用负责人、外部账号、负责人执行真实统一授权，结果为通过、拒绝、通过。直接调用方 `erp-order-to-cash --lib` 114 通过；本轮共享验收 `erp-read-models --lib` 707 通过、30 忽略。`git diff --check` 通过；未执行 server、集成测试或真实 MongoDB 验收。

### B-09 无授权且内部使用 NoTransaction 的公开方法（已解决）

- 现状：下列领域方法无生产调用方，内部使用 `NoTransaction`：`SalesReviewService::sales_change_order_list`；`SalesSelectionService` 的 `booklet_detail`、`proposal_detail`、`admin_session`、`booklet`；`FulfillmentService::customer_acceptance_detail`；`SupplierService::load_supplier`；`ensure_capability_qualified` 的服务层包装；`WorkItemService::load`；`DocumentRegistryService::approval_binding`。编排与 HTTP 使用各自已授权入口，不调用这些方法。
- 依据：重写方案 §3.6.1。`backend/AGENTS.md`「事务」：禁止为有无事务复制实现。
- 修改要求：无生产调用方的删除。若删除前发现仍被已授权事务复用，改为接收 `executor: &mut dyn Executor` 并收窄可见性，不得保留无 actor 的公开包装。实体上同名的能力判断不删除。
- 修复：核查所有 Rust 调用后，删除本条列出的十个无生产调用包装；同时删除无调用的 `approval_binding_with`、销售变更查询模块及其专用归一化类型。删除服务层资格包装的根导出，保留实体资格判断、供给资格规则委派和真实执行器读取入口。
- 复用合同：选品视图、验收详情和审批绑定读取必须沿现有已授权调用链复用原执行器；HTTP 继续使用当前编排与 ReadModel 入口，不得恢复旧包装。
- 验收记录（2026-11-17）：`erp-sales --lib --locked` 496 通过，`erp-fulfillment --lib --locked` 185 通过，`erp-supplier --lib --locked` 186 通过，`erp-workflow --lib --locked` 560 通过、3 忽略。清理销售变更专用归一化代码后重新执行 `erp-sales --lib --locked`，496 通过。仅对本条修改文件执行 `rustfmt --check` 和 `git diff --check`；未执行 server、集成测试或真实 MongoDB 验收。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-sales --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-fulfillment --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-workflow --lib
```

### B-10 数据转移在授权前读取任务或解密文件（已解决）

- 现状：`crates/orchestration/erp-data-import` 的 `DataTransferProcess` 在 `status`、`context`、`result` 中，于授权前读取任务或解密私有文件。`context` 先 `owned_job` / `transfer_job` 再授权。
- 依据：重写方案 §3.5.3、§3.6.1。
- 修改要求：在同一快照内完成当前资格、任务及文件元数据读取。对象存储下载和解密在事务成功后执行；初始资格拒绝不得读取或解密私有文件，交付前必须复验当前资格和原任务版本。
- 修复：`status` 先校验真实后台任务记录资格，再读取任务；`context`、`result` 先校验持久化分类对应的业务功能和真实询价/采购任务目标，再读取完整任务及私有文件元数据。共享导出保留当前身份授权，非本人上下文继续脱敏。门户本人关系查询下沉至 `erp-support` 窄仓储，并沿用原执行器。
- 历史兼容：缺少独立上下文的本人任务先通过真实任务记录资格，源文件仅用于恢复业务分类，结果解密前补验当前业务资格；非本人历史任务缺少独立上下文时拒绝，不解密识别类型。原结果摘要与业务范围检查继续执行。
- 实际验收：`env -u ERP_TEST_MONGO_URI cargo test -p erp-data-import --lib` 通过，253 项通过、1 项依赖真实 MongoDB 的既有测试忽略。新增 5 项行为测试覆盖同快照授权后读取、拒绝后零受保护读取、非快照拒绝以及任务版本/身份变化拒绝交付。10 个本条修改源码文件体积检查、定向格式检查与 `git diff --check` 通过。未执行真实 MongoDB、S3 下载解密或 HTTP 验收。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-data-import --lib
```

### B-11 交付前范围未复验，覆盖与导入批次脱离原执行器（已解决）

- 现状：`crates/read-models/erp-read-models/src/purchase_center/change/query.rs` 第 62 行 `change_order_list` 经快照读取后返回，交付前未按 §3.6.4 复验范围。`catalog_center/delivery_coverage.rs` 第 188 行 `sku_delivery_inputs` 为公开函数，第 194、220、222 行使用 `NoTransaction`。`import_center.rs` 第 25 行 `batch_names`、第 45 行 `batch_name` 硬编码 `NoTransaction`。
- 依据：重写方案 §3.6.4、§3.6.5。
- 修改要求：`change_order_list` 在交付前 `validate`。`sku_delivery_inputs`、`batch_names`、`batch_name` 接收调用方原执行器，在该执行器上读取并 `validate`。`sku_delivery_inputs` 收窄为调用方所在模块可见，禁止保留公开的 `NoTransaction` 入口。
- 修复：采购变更列表继续使用 `read_change_list` 末尾已有的 `scope.validate(executor)`，校验发生在全部读取与投影之后、同一事务交付之前。配送覆盖的 `sku_delivery_inputs`、`restricted_skus` 改为私有步骤并沿原执行器读取；跨 crate 入口只生成后台重算所需的最终覆盖记录，待重算批次、事实读取和条件保存共用调用方执行器。导入批次由 ReadModel 组合入口持有快照，领域批次读取、后台任务补齐与来源名称读取共用该执行器；名称补齐步骤不再公开。
- 授权合同：导入批次保持既有 `require(Operation)` 功能资格，领域步骤拒绝非快照执行器；不为功能资格新增 Mapping 或无意义 `validate`。后台覆盖重算保持原 worker 入口，本条不新增身份、权限或系统放行路径。读取型范围凭证继续由拥有它的入口在原执行器上 `validate`。
- 实际验收：`env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models -p erp-import -p erp-supplier-catalog --lib --locked` 通过：分别为 713 项通过、30 项既有测试忽略；81 项通过；189 项通过。新增行为测试验证批次列表与详情在非快照时于身份及业务读取前拒绝；既有拒绝测试改为执行当前公开领域步骤。5 个修改源码文件的定向格式检查与 `git diff --check` 通过。未执行真实 MongoDB、worker 运行或 HTTP 验收；server 调用仅完成源码适配。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib
```

### B-12 预存款读取把多种错误吞成内部错误（已解决）

- 现状：`crates/orchestration/erp-supplier-orders/src/supplier_deposits/read_access.rs` 第 36 行 `SupplierDepositReadAccess::authorize`。第 43–49 行映射 `Forbidden`、`NotFound`、`RepositoryError`、`BusinessLogicError`，其余含 `ConflictError`、`Unauthenticated`、`ValidationError` 进入 `_`，变成 `Internal`。
- 依据：重写方案 §4.1.5。
- 修改要求：按原错误分类映射 `ConflictError`、`Unauthenticated`、`ValidationError`。禁止落入 `Internal`。
- 修复：`SupplierDepositReadAccess::authorize` 统一调用 `From<erp_supplier_orders::Error> for erp_read_models::Error`；转换须穷尽保留同构错误分类、原载荷、字段校验明细及持久化来源，禁止兜底降为 `Internal`。新增三项生产转换行为测试。单包测试依赖须启用 `erp-identity/test-support`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier-orders --lib
```

- 实际验收（2026-11-17）：154 项单元测试通过，0 失败；本文件对应 Rust 源文件格式检查及限定文件 `git diff --check` 通过。`./scripts/check-domain-boundaries.sh --cutover` 复验通过，夹具与工作区均为 `errors=0`。验收不包含真实 MongoDB、服务端编译或运行验证。

### B-13 合同映射对纯数据库关联做空准备（已解决）

- 现状：`crates/domains/erp-contract/src/repository/authorization.rs` 原先在编译前只读取组织状态。合同字段、组织主属关系和历史参与均通过数据库条件关联。新 Mapping 路径的对象历史事实只调用 `document_ids_in`；`ContractParticipantPort::revision` 仅被第六阶段待迁移的旧接线调用。
- 依据：重写方案 §3.3.2。
- 修改要求：合同 Mapping 只构建数据库内条件，不预读组织状态。删除无调用的 `revision` 及其实现。保留 `document_ids_in`。
- 修复：合同 Mapping 使用静态集合和字段描述，删除准备能力、组织准备数据和仅供准备使用的数据库句柄。组织条件继续使用统一授权时点关联真实主属关系；对象事实继续保留负责人组织、到期边界及本次目标的历史参与。参与 Port 只保留 `document_ids_in`，删除各接线和测试替身中的 `revision` 实现。
- 阶段边界：旧 `contract_data_scope` 接线归 P02／第六阶段；其修订读取直接调用工作流既有 `participant_access::revision`，保留原读取／写入选择和错误分类。本条仅调整 server 必要源码，不执行 server 编译或测试。
- 验收：以下库测试实际通过，117 passed，0 failed。测试覆盖默认无准备、合同字段与组织条件、历史来源关联、目标内历史读取、未接线读取失败和对象组织到期边界。仅格式化本条修改的 Rust 文件；这些文件的格式检查与本条改动的 `git diff --check` 均通过。

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-contract --lib --locked
```

### B-14 采购事实按动作名决定历史参与（已解决）

- 现状：`apps/erp-server/src/wiring/authorization/commerce.rs` 第 162 行 `PurchaseObjects::load`。第 169 行用 `context.permission.action()` 是否为 `"list"` 或 `"detail"` 决定是否查询 `document_participants`。`wiring/authorization/catalog.rs` 不存在。范围定义在 `crates/domains/erp-identity/src/entity/access_control/scope_spec/catalog.rs`：`purchase_order::LIST` 与 `DETAIL`（第 226–227 行）为 Company 且 `allows_history = true`；同段其余采购动作 `allows_history = false`。`scope` 的参数顺序见 `scope_spec.rs` 第 54–60 行。
- 依据：重写方案 §3.4.1、§3.6.4。
- 修改要求：事实加载不得读取动作名字符串。是否装载历史参与由该权限的 `allows_history` 决定。`allows_history` 为 false 时不得查询 `document_participants`。Company 范围已无条件允许、历史参与不改变结果集时，不得生成该跨集合 lookup。
- 修复：采购 `LIST/DETAIL` 保留 `allows_history = true` 及任务来源的历史读取、显式记录读取补充。采购事实通过 identity 的 `AccessContext::allows_history` 读取固定权限规格；false 分支不得查询参与集合。规则仅在本次解析范围含 Company 且没有个人上限时省略冗余历史条件，受限来源和个人上限继续执行原条件。保留 M-20 删除 `self_purchase` 来源关系的结果。
- 验收：本文件位于 `apps/erp-server`。本阶段不执行 `erp-server` 测试。采购映射与事实的库测试为：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity -p erp-procurement --lib
```

`commerce.rs` 的动作名判断删除、规格分支及参与集合读取顺序由代码核对。构造编译见 P-03，本条不提前执行。

- 验收结果：上述联合库测试通过，identity 601 项通过、procurement 417 项通过，均为 0 失败。原三项任务来源历史、显式记录补充及 query／require 一致性测试保留并通过；新增回归核验 Company 无个人上限时无需历史事实、Company 加个人上限不跳过历史及上限、Company 任务根不放宽受限采购来源，以及四个采购 Company 查询不生成跨集合 lookup。指定文件 `rustfmt --check`、`git diff --check` 和 `commerce.rs` 源码核对通过；日志为 `/Volumes/Kingston/codex-erp-phase5-review/b14-recheck-lib.log`。未执行 server 构造、编译、测试或真实数据库验收。

### B-15 跨域只读查询与单域查询的落点（已解决）

- 现状：跨域只读仍在编排层：`CustomerProfileProcess::detail`（`crates/orchestration/erp-master-data` 的客户资料查询）、`SupplierSettlementProcess::statement_list`、`SupplierFulfillmentProcess::supplier_fulfillment_order_list`、`InquiryReadService`（销售询价读取）、销售选品册与方案的列表和详情（编排层 `sales_selection`）。单域查询仍在 `erp-read-models`：`returns_center/sales_return.rs` 的 `load_sales_return_case_view` 只读取退货域表。
- 依据：`backend/AGENTS.md`「架构与依赖」：跨域读取放 `erp-read-models`；单域查询留在拥有领域的 Service 或 Repository。
- 修改要求：上列跨域读取移入 `erp-read-models` 的对应中心。`load_sales_return_case_view` 移回退货领域的查询服务。搬迁保持现有授权结果、权限常量与事务边界。本条不改变 M-05、K-01 已规定的授权条件。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-master-data --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply-settlement --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-order-to-cash --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-returns --lib
```

- 修复：客户资料详情改由 `customer_center::profile::CustomerProfileReadService` 读取；供应商结算列表与履约列表改由 `supplier_center` 读模型读取；询价读取、选品册与方案列表及详情改由 `sales_center::{inquiry, selection}` 读取。单域销售退货列表、详情、DTO 与原 Executor 结果装配全部归 `erp-returns`。调用方必须使用新入口；原权限常量、授权结果与事务边界保持。
- 实际验收：外置共享构建环境中，`erp-master-data` 31、`erp-order-to-cash` 114、`erp-read-models` 703、`erp-returns` 118、`erp-supply-settlement` 38、`erp-reverse-flow` 97 项库单测通过，失败为 0；读模型 30 项、结算 5 项真实 MongoDB 测试保持忽略。`check-domain-boundaries.sh --cutover`（含审计登记）、本项 49 份 Rust 文件的体积扫描与 `git diff --check` 通过。
- 验收范围：仅库单测与静态边界；未执行 `erp-server`、集成测试或真实 MongoDB 验收。

## 4. 规范与测试

只改本清单触及的代码。不得为通过全仓体积检查拆未触及的历史方法。

### N-01 函数体内的完整模块路径（已解决）

- 现状：下列位置在函数体内写完整路径。`crates/domains/erp-catalog/src/sellable.rs` 不存在，不在本条。
  - `erp-party` 的 `party/mod.rs`、`contact.rs`、`bank_account.rs` 体内使用 `crate::entity::party::`、`crate::dto::party::` 或 `erp_core::Result`。
  - `erp-sales` 的 `sales_selection/query.rs`、`publish.rs` 体内使用 crate 全路径。
  - `erp-read-models` 的 `supplier_api/dto.rs`、`supplier_api/query.rs`、`supplier_settlement/query.rs`、`source.rs`、`customer_refund.rs`、`supply_settlement/list.rs`。
  - `erp-order-to-cash` 的 `handover.rs`、`draft_working_copy.rs`、`red_invoice.rs`、`customer_receipt_submit.rs`、销售单 `query.rs`。
  - `erp-finance-posting` 的 `payable/account.rs`、`finance/payable/payment.rs`。
  - `erp-procure-to-pay` 的 `change/submit.rs`、`place.rs`。
  - `erp-supply` 的供应交接 `handover.rs`。
  - `erp-supplier` 的 `funds_scope/authorization.rs`。
  - `erp-workflow` 的退货审批 `runtime.rs`、`approval_runtime/query.rs`。
  - `apps/erp-server/src/wiring/authorization/sales.rs`。
  - `erp-identity` 的 `rules/records.rs`、`rules.rs` 第 533 行在已有顶部导入时仍写 `erp_permissions::supplier_offering::COMPLETE_SUPPLY_EXCEPTION_TASK`，以及检查 DTO 体内的 `crate::Permission::parse`。
- 依据：`backend/AGENTS.md`「编码约定」：禁止在代码中写完整路径；顶部 `use` 后用短名；`<Type as Trait>::item` 除外。动态权限输入继续 `Permission::parse`，只把路径提到顶部。
- 修改要求：上述仍存在的完整路径改为文件顶部 `use`。本条不改授权结果。`apps/erp-server` 的导入随第六阶段编译，本阶段只改源码。
- 验收：对每个被改 crate 执行 `env -u ERP_TEST_MONGO_URI cargo test -p <crate> --lib`。不跑 `erp-server`。

- 修复记录（2026-11-17）：已将指定现存范围内 29 个源码文件的完整路径移至顶部 `use`，同名错误类型使用别名。动态检查输入继续调用 `Permission::parse`；保留 `<Type as Trait>::item` 消歧、可见性路径及宏卫生路径。不得借本条改变授权规则或业务行为。
- 路径核实：选品跨域读取位于 `erp-read-models/sales_center/selection`；销售单查询位于 `sales_center/order/query.rs`；结算来源位于 `erp-supply/service/supplier_settlement`；红票与客户收款提交位于 `erp-finance-posting`；供应下单位于 `erp-fulfillment-execution`；资金范围和审批运行查询位于 `erp-read-models`。不存在的 `erp-catalog/src/sellable.rs` 不处理。
- 实际验收：受改文件 `rustfmt --check`、`git diff --check` 通过。`cargo test --lib` 通过：`erp-finance-posting` 126、`erp-fulfillment-execution` 113、`erp-order-to-cash` 116、`erp-party` 132（2 ignored）、`erp-procure-to-pay` 147、`erp-read-models` 713（30 ignored）、`erp-sales` 496、`erp-supply` 629。所有测试均取消 `ERP_TEST_MONGO_URI` 并使用指定外置 Cargo 缓存与产物目录；未执行集成测试、真实 MongoDB 或 `erp-server` 编译。
- 组合复验：共享 B-14 最终 `erp-identity --lib` 结果，601 passed、0 failed、0 ignored，包含任务来源历史及显式读取补充回归；日志为外置盘 `b14-recheck-lib.log`。服务端导入仅完成源码和格式检查。

### N-02 公开方法的文档注释（已解决）

- 现状：`crates/domains/erp-warehouse/src/service/warehouse/mod.rs` 的 `warehouse_fulfillment_handler_options` 接收 `actor: &AuditActor`，其 `# 参数` 写「无。」。本次修改新增 `actor` 或 `authorization` 参数的公开方法，文档未写这三项或把参数写成「无。」的，一并修改。
- 依据：`backend/AGENTS.md`「编码约定」：公共方法含 `# 参数`、`# 返回`、`# 错误`。
- 修改要求：补全这三段。新增的 actor 与授权服务写入 `# 参数`。无错误时 `# 错误` 写「无。」，有错误时写实际分类。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-warehouse --lib
```

其余 crate 随该公开方法所在条目的验收命令。

- 修复：按当前 diff 检查含 actor 或 authorization 的新增、变更可见签名，补齐真实参数名称；将询价注释中的旧 action 名称改为 permission。仓库候选、回款、财务任务和发票附件入口写明实际拒绝、读取及未知提交错误；无错误的构造方法明确写「无。」。本项仅修改文档注释。
- 验收记录：本项触及的 18 个 Rust 文件定向 `rustfmt --edition 2024 --config skip_children=true --check` 通过，`git diff --check` 通过。依 `backend/AGENTS.md` 注释变更规则未重复编译或测试；仓库行为验收沿用 N-08，相关库由统一复验覆盖。

### N-03 本次修改的生产方法长度（已解决）

- 现状：`submit_sales_order` 已按 B-02 拆分，入口为 21 有效行，相关新增私有方法最大 42 有效行。本项无新增生产源码调整。
- 依据：`backend/AGENTS.md`「编码约定」：本次新增或修改的生产方法硬上限 50 有效行；禁止为通过全仓体积检查拆未触及的历史方法。
- 修改要求：本清单改动的生产方法超过 50 有效行时拆成私有函数。不拆未改动的历史方法。不运行 `./scripts/check-rust-size.sh` 作为本清单验收。
- 尺寸验收：复用仓库尺寸解析器，只扫描当前 Git diff 的 287 个 Rust 生产文件，排除测试、`build.rs` 和测试专用设施。671 个方法变更候选中，639 个方法体变更的方法最大 49 有效行；唯一超过 50 的候选 `supplier_settlement/source.rs::build_source_line` 仅将参数完整路径改为导入名，方法体与 HEAD 一致，不纳入拆分。B-11、B-14 稳定后的生产改动已覆盖，未发现本清单新增或实际修改的超限方法。证据：`/Volumes/Kingston/codex-erp-phase5-review/n03-size-report.json`。
- 库验收：共享 B-01 的最终 `env -u ERP_TEST_MONGO_URI cargo test -p erp-order-to-cash --lib --locked`，116 通过、0 失败；N-01 导入调整后同库仍为 116 通过。日志：`/Volumes/Kingston/codex-erp-phase5-review/b01-final-otc.log`、`/Volumes/Kingston/codex-erp-phase5-review/n01-lib-remaining.log`。本项只追加验收记录，限定文档 `git diff --check` 通过。

### N-04 单测把内置权限写成字符串（已解决）

- 现状：单测对内置权限调用 `Permission::parse` 字符串：`erp-procurement` 的 `authorization/mapping.rs` 中 `"purchase_order:list"`；`erp-warehouse` 的 `authorization.rs` 中 `"warehouse:list"`；库存授权测试中 `"stock_balance:list"`；`erp-customer` 的 facts 测试中 `"sales_selection_booklet:create"` 以及 `format!("customer:{action}")`；`erp-sales` 选品授权测试中 `format!("{resource}:list")`；`erp-support` 的 `background.rs` 中 `format!("background_job:{action}")`。
- 依据：重写方案 §3.6.1。内置权限使用常量。动态外部输入才使用 `Permission::parse`。
- 修改要求：上述内置权限改为对应常量。按动作拼接的测试改为直接引用常量。`background_job:unknown` 这类故意非法编码保留 `Permission::parse`。检查请求里的动态资源与动作保留 `Permission::parse`，只按 N-01 整理导入。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-procurement --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-warehouse --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-customer --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-sales --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-support --lib
```

库存授权测试所在包为 `erp-inventory`：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-inventory --lib
```

- 修复：采购、仓库、库存、客户、选品及后台任务单测的合法内置权限直接引用 `erp-permissions` 常量；动作组合测试使用常量集合。保留 `background_job:unknown` 的解析以验证未登记权限拒绝，保留外部动态请求解析。
- 实际验收（2026-11-17）：按统一外置盘环境执行六包 `cargo test --lib --locked`，采购 417、仓库 62、库存 131、客户 103、销售 496、支持 171 个单测全部通过。六个修改源文件通过 `rustfmt --check`，本项修改通过 `git diff --check`。日志：`/Volumes/Kingston/codex-erp-phase5-review/n04-tests.log`。

### N-05 授权测试固定睡眠两秒（已解决）

- 原问题：`crates/domains/erp-identity/src/repository/authorization/phase5_tests.rs` 与 `service/authorization/tests/approval_management.rs` 通过固定两秒睡眠等待授权到期。
- 依据：`backend/AGENTS.md`「测试」：行为测试执行生产逻辑并断言具体结果。到期必须由测试控制。
- 修改要求：改为可控时钟或确定性到期时点。删除这两处两秒睡眠。
- 修复：授权证明的截止检查使用仅在 `cfg(test)` 启用的任务局部时钟。静态 Mapping 测试执行真实 `query/validate`，分别验证截止前允许交付及到达截止时拒绝；审批测试在最后一个来源的异步重验结束时推进时钟，执行真实 `require` 并断言已签发来源证明到期。生产继续使用原 `Instant::now()`，测试间不共享时钟状态。
- 实际验收（2026-11-17）：`cargo test -p erp-identity --lib deadline` 的 4 项测试全部通过，测试执行耗时 0.04 秒。联合复验 `cargo test -p erp-identity -p erp-procurement --lib` 中 identity 601 项全部通过，两项修改后的期限测试均通过；日志为 `/Volumes/Kingston/codex-erp-phase5-review/b14-recheck-lib.log`。三个修改源文件通过 `rustfmt --check`，本项修改通过 `git diff --check`。验收不访问真实 MongoDB。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib
```

### N-06 工作区依赖与自动测试开关（已解决）

- 现状：根 `Cargo.toml` 已有 `[workspace.dependencies]`，其中没有 `roxmltree`、`zeroize`。`crates/domains/erp-contract/Cargo.toml` 直接写 `roxmltree = "0.21.1"`。`crates/domains/erp-supply/Cargo.toml` 直接写 `zeroize`。`crates/domains/erp-notification/Cargo.toml` 的包头没有 `autotests = false`。
- 依据：`backend/AGENTS.md`「质量门禁」与工作区依赖约定：版本集中在 workspace；领域 crate 关闭自动集成测试发现。
- 修改要求：在根 `[workspace.dependencies]` 增加 `roxmltree` 与 `zeroize`（保留 supply 现有的 `serde` feature）。两个领域包改为 `workspace = true`。`erp-notification` 增加 `autotests = false`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-contract --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-notification --lib
```

- 完成：根工作区登记 `roxmltree = "0.21.1"`、`zeroize = "1.8.2"`；contract、supply 改用 `workspace = true`，supply 保留 `serde` feature；notification 设置 `autotests = false`。
- 实际验证：上述三个 `--lib` 命令分别通过 116、618、7 项测试，均为 0 失败、0 忽略。`./scripts/check-domain-boundaries.sh --cutover` 通过，夹具与工作区均为 `errors=0`，Cargo metadata 无 `kind=test` 集成目标。`git diff --check` 通过。

### N-07 单 crate 测试缺少 test-support（已解决）

- 现状：`erp-import` 的 dev-dependencies 只有 `erp-identity` 的 `test-support`，生产代码使用 `persistence_core::testing`。`erp-supply` 的 dev-dependencies 只有 `erp-identity` 的 `test-support`。`erp-warehouse` 的 dev-dependencies 没有 `erp-identity` 的 `test-support`，测试调用 `erp_identity::testing::unavailable_authorization`。
- 依据：`backend/AGENTS.md`「测试」：单 crate `--lib` 必须能编译。
- 修改要求：为上述三个包补齐实际用到的 `test-support` feature。`erp-import` 与 `erp-supply` 增加 `persistence-core` 的 `test-support`。`erp-warehouse` 增加 `erp-identity` 的 `test-support`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-import --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-warehouse --lib
```

- 修复结果：`erp-import`、`erp-supply` 在 dev-dependencies 启用 `persistence-core/test-support`；`erp-warehouse` 在 dev-dependencies 启用 `erp-identity/test-support`。
- 实际验收：`erp-import --lib --locked` 通过，80 passed；`erp-supply --lib` 通过，618 passed；`erp-warehouse --lib --locked` 通过，56 passed。三包均为 0 failed、0 ignored，执行前均移除 `ERP_TEST_MONGO_URI`。
- 依赖门禁：`./scripts/check-domain-boundaries.sh --cutover` 通过，夹具与工作区均为 errors=0，审计边界检查通过。

### N-08 读取入口的授权行为测试（已解决）

- 现状：`erp-supplier`、`erp-warehouse`、`erp-party` 的读取入口缺少允许、拒绝、非快照执行器拒绝三类行为测试。第 2 节各条的验收已覆盖该条自己的行为测试。
- 依据：`backend/AGENTS.md`「测试」：每个改动覆盖成功路径与权限拒绝；行为测试执行生产逻辑。
- 修改要求：为这三个 crate 的读取入口补允许、拒绝、非快照执行器拒绝。非快照用例使用 `NoTransaction` 或等价的非快照执行器，断言拒绝。禁止连接真实 MongoDB。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-warehouse --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-party --lib
```

- 完成：增加仅在测试中使用的固定内存身份、范围和对象事实设施；查询使用领域实际 Mapping，并执行生产 `AuthorizationService::require/query/validate`。生产 Builder 的完整绑定校验保持不变。
- 行为覆盖：供应商列表授权编译、本人详情允许、其他负责人拒绝、目标缺失及无权限拒绝；仓库经办候选读取、修订和策略筛选准备；主体列表、详情及修订筛选准备。上述读取均增加 `NoTransaction` 拒绝测试；仓库和主体拒绝路径执行实际读取事务体，并在业务查询前停止。
- 实际验收：合并执行 `env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier -p erp-warehouse -p erp-party --lib --quiet`，分别通过 183、60、130 项，主体既有 2 项忽略测试未执行；`erp-identity --lib --quiet` 通过 595 项。`./scripts/check-domain-boundaries.sh --cutover` 通过，夹具与工作区均为 `errors=0`；本项文件格式及 `git diff --check` 通过。
- 验收边界：允许路径覆盖真实授权步骤、输入准备及仓库既有身份窄 Port 的返回组装。未执行真实 MongoDB 查询、公开入口的事务开启或快照隔离验证；主体、仓库修订和策略列表的允许路径不据此声明数据库结果正确。

## 5. 维持现状

不得按审查报告原文把下列入口改成补数据范围、改权限编码或提前切换调用链。

### K-01 逆向单据列表与详情

`sales_return`、`customer_refund`、`supplier_refund`、`receipt_reversal`、`payment_reversal` 的列表与详情在 `crates/domains/erp-identity/src/service/authorization/functions.rs` 登记为仅功能资格。重构前无行级范围。重写方案要求保留既有授权条件。

继续按 Function 在同一快照事务内 `require(Operation)` 后读取。不得另行加数据范围。`purchase_return` 不在本条；其详情读取顺序按 B-05 修改。

### K-02 开票申请额度读取

`internal::INVOICE_REQUEST_AMOUNTS_READ` 的常量说明写明沿用申请列表功能资格，且不签发应收对象凭证。identity 将其登记为 Function。读模型在快照事务内 `require(Operation)` 后读取。路由 `apps/erp-server/src/http/routes/receivable.rs` 使用 `sales_invoice_request:list`。

保持仅功能资格的快照事务读取。不得补应收对象范围。HTTP handler 仍调用无 actor 的旧方法，按 P-06 处理，不在本阶段改 handler。

### K-03 销售单列表的范围变化

审查报告中销售单列表必现 `DATA_SCOPE_CHANGED` 的描述，当前源码不能确认。不得把 `sales_order_list` 列入必须修改。已确认的供应商连接详情与目录按 M-12 修改。

### K-04 已有单据的客户详情

读取已有销售单、合同或其他已绑定客户单据时，继续使用 `customer::DETAIL`。不得按 M-05 改成 `CUSTOMER_SELECT`。M-05 只覆盖开展新业务时的绑定与更换客户，以及合同归档里多出来的详情拼接。

### K-05 路由层功能门

本阶段保留路由 `with_permission`、`RbacAuthorizeLayer`、`RbacAuthorizeService`。M-10 只增加服务内授权和 actor 参数。删除路由门见 P-04。

## 6. 第六阶段事项

本清单不实施本节。只记录事项，不写修改步骤。

### P-01 应用与业务工厂

Application 与业务工厂切换到 `wiring::authorization::build`。

### P-02 旧装配模块

删除仍引用已删除类型的旧 wiring 模块，包括 `apps/erp-server/src/wiring/purchase_data_scope.rs`、`wiring/contract_data_scope.rs`、`wiring/inventory.rs`。这些文件仍引用 `AuthorizationRegistry`、`AccessPolicy`、`AccessMode`、`AccessRegistration`、`DataAccess`。M-11 只改仍在生产路径上的 `PurchaseDataScopePort`，不在第五阶段删除这些文件。

### P-03 授权装配构造测试

编译并执行 `wiring/authorization` 的不读数据库完整构造测试。

### P-04 路由功能门

删除 `with_permission`、`RbacAuthorizeLayer`、`RbacAuthorizeService`，并同步路由合同检查。

### P-05 进程外入口

worker、CLI、前端人员范围与权限检查接口切换到授权服务。

### P-06 HTTP 调用已授权入口

HTTP handler 改为调用已授权的领域或组合入口。包括审计日志列表，以及开票额度读取（当前 `http/handler/receivable/invoice_request.rs` 调用无 actor 的 `invoice_request_amounts`）。K-02 的功能资格条件保持不变。
