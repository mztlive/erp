# 第五阶段授权审查修改清单

## 1. 文书地位

本文件是第五阶段审查后的修改清单。执行人按条目修改代码，并按该条目的验收条件核对。

第六阶段事项见第 6 节。本清单不要求提前切换 HTTP、worker、Application 工厂、CLI 或前端。

路径从 `backend/` 起。行号以写入时源码为准。验收命令一律为：

```text
env -u ERP_TEST_MONGO_URI cargo test -p <crate> --lib
```

禁止 `cargo test --workspace`。禁止 `--test`、`tests/`、`--include-ignored`、examples，以及任何连接真实 MongoDB 的命令。禁止把 `erp-server` 列为第五阶段验收命令。

依据：`backend/crates/domains/erp-identity/docs/authorization-service-improvement.md`；`backend/AGENTS.md`。

## 2. 必须修改

授权结果错误、写入前未授权、目标形态不匹配、事务边界破坏，按本节修改。

### M-01 银行账号揭示的目标形态

- 现状：`crates/foundation/erp-permissions/src/payable.rs` 第 15 行 `PAYMENT_RECIPIENT_REVEAL` 的编码是 `party_bank_account:reveal`。`crates/domains/erp-identity/src/entity/access_control/scope_spec/catalog.rs` 第 358 行把该常量登记为 `ActionKind::Task`。`crates/domains/erp-identity/src/service/authorization/rules.rs` 第 198–203 行对无记录条件的 Task 只允许 `TargetForm::Task`；第 539 行 `task_supported` 包含该常量。`crates/orchestration/erp-master-data/src/customer_profile/sensitive.rs` 的 `reveal_snapshot`（第 50 行）在第 60–62 行对 `field_permissions` 的每一项执行 `require(..., AccessTarget::Operation)`。`field_permissions`（第 200–205 行）在 `BankAccountNumber` 上使用 `party::BANK_ACCOUNT_DETAIL` 与 `payable::PAYMENT_RECIPIENT_REVEAL`。`crates/orchestration/erp-master-data/src/customer_profile/display.rs` 第 15 行把同一常量放入功能资格列表，第 53–55 行 `reveal` 与第 79 行 `REVEAL_BANK_ACCOUNT` 按功能资格判断可见。银行账号揭示因此在事务内被拒绝：目标形态与固定处理分支不一致。
- 依据：重写方案 §3.2.3、§3.6.1。
- 修改要求：保持 `party_bank_account:reveal`。该动作只要求功能资格：在 identity 的 scope catalog 把 `payable::PAYMENT_RECIPIENT_REVEAL` 固定为 `ActionKind::Function`，并从 `task_supported` 删除该常量，使 `rules.rs` 的 Function 分支允许 `Operation`。`sensitive.rs` 第 60–62 行继续对该常量 `require(Operation)`。`display.rs` 继续用同一常量做功能资格可见性。禁止新造第二个权限编码，禁止改成 `supplier_payment:reveal_recipient`。补 `BankAccountNumber` 路径的授权行为测试：允许的功能资格在快照事务内通过；无该资格拒绝；非快照执行器拒绝。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-master-data --lib
```

### M-02 供应商敏感揭示与命令回执缺少对象范围

- 现状：`crates/orchestration/erp-master-data/src/supplier_profile/sensitive.rs` 的 `reveal_snapshot` 只对 `supplier::SENSITIVE_REVEAL` 执行 `require(Operation)`，不对令牌中的供应商执行对象范围。`commands.rs` 的 `command_result_snapshot` 对 `SUPPLIER_PROFILE_COMMAND_READ` 执行 `Operation` 后直接读取结果，不核对关联供应商的详情范围。客户揭示在 `customer_profile/sensitive.rs` 第 57–63 行的顺序是：先功能资格，再字段资格，再 `customer::DETAIL`。
- 依据：重写方案 §3.5.1、§3.6.1。对照客户揭示的既有对象检查。
- 修改要求：供应商揭示在同一快照事务内先检查字段资格，再对令牌中的供应商执行 `supplier::DETAIL`。命令回执在读取结果之后，核对关联供应商的详情范围。不得新造权限编码。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-master-data --lib
```

### M-03 供应商命令用功能预检并在创建回放调用创建入口

- 现状：`crates/orchestration/erp-master-data/src/supplier_profile/commands.rs` 的 `imported_result`、`command_qualified`、`authorize_command` 调用 `has_permission`。创建回放调用 `require_create`。
- 依据：重写方案 §3.5.1、§3.6.1。`has_permission` 只证明功能资格，不产生本次对象授权。
- 修改要求：命令分支改为 `AuthorizationService::require`，目标形态与该命令既有写入口一致。创建回放执行原创建命令的 `require`，不得改走其他入口。同一快照事务内完成授权后再读、再写。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-master-data --lib
```

### M-04 供给详情用列表权限且补齐后未复验

- 现状：`crates/read-models/erp-read-models/src/supplier_center/offering/detail.rs` 第 26 行 `detail`，第 36 行 `require_offering(..., supplier_offering::LIST, ...)`。补齐后未 `validate`。
- 依据：重写方案 §3.6.1、§3.6.5。
- 修改要求：详情入口使用详情权限常量。同一快照事务内先 `require` 或 `query`，读取并补齐后 `validate`。列表权限不得作为详情通过条件。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib
```

### M-05 开展新业务时用客户详情权限

- 现状：销售单新建、草稿保存、首次提交在绑定或更换客户时调用 `customer::DETAIL`。已确认调用点：`crates/orchestration/erp-order-to-cash/src/sales_order/command/create/persist.rs` 的 `related_order`；`command/create.rs` 的 `load_customer`；`command/save.rs` 的 `related_order`；`draft_working_copy.rs` 的 `related_order`；`command/contract.rs` 的 `related`；`command/no_contract.rs` 的 `load_customer`；`start_approval.rs` 中 `SalesOrderStartPersistInput::validate` 的 `related_order`。`crates/orchestration/erp-master-data/src/contract_import/archive.rs` 的 `persist_archive` 在 `require_create`（`internal::CUSTOMER_SELECT`）之后再拼接 `customer::DETAIL`。已有单据只读取已绑定客户的详情入口不在本条。
- 依据：重写方案 §3.5.1、§3.6.1。开展新业务使用 `internal::CUSTOMER_SELECT` 的 `require_current`，关闭历史补充。读取已有单据继续 `customer::DETAIL`。
- 修改要求：上述绑定或更换客户的调用改为 `internal::CUSTOMER_SELECT` 的 `require_current`。合同归档在已执行 `CUSTOMER_SELECT` 后删除额外的 `customer::DETAIL`。不得把已有单据详情读取改成 `CUSTOMER_SELECT`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-order-to-cash --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-master-data --lib
```

### M-06 采购单发送邮件拼接详情授权并拆成两个事务

- 现状：`crates/orchestration/erp-procure-to-pay/src/purchase_correspondence/mod.rs` 的 `send`（第 73 行）先调用带详情授权的公开读取，再开第二个事务做发送授权。
- 依据：重写方案 §3.1.1、§3.5.3。
- 修改要求：改为同一写事务。事务内先 `require` 采购单发送邮件权限和真实采购单记录，再读取、渲染、写入。不得另行拼接详情授权。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-procure-to-pay --lib
```

### M-07 结算差异补证只检查操作资格

- 现状：`crates/orchestration/erp-supply-settlement/src/supply_settlement/evidence.rs` 的 `append_difference_evidence` 与 `append_evidence` 只对差异决定权限执行 `AccessTarget::Operation`。
- 依据：重写方案 §3.5.1、§3.6.3。与 `SettlementAction::AppendEvidence` 的对象范围一致。
- 修改要求：同一事务内增加结算单对象范围检查，检查口径与 `SettlementAction::AppendEvidence` 相同。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply-settlement --lib
```

### M-08 结算作废、刷新、差异决定的重放降级授权或伪造操作人

- 现状：`crates/orchestration/erp-supply-settlement/src/supply_settlement/void.rs` 的 `void_statement_apply` 先做 `Operation`，已作废路径在领域 `prepare_void` 提前返回。`draft.rs` 的 `replay_refresh` 使用 `AuditActor::new(..., AccountKind::Admin)`。`difference.rs` 的 `recover_difference_decision`、`decide_difference_apply`、`replay_difference_decision` 使用 `Operation` 或伪造 Admin。
- 依据：重写方案 §3.5.1、§3.6.3。
- 修改要求：传入调用方真实 `AuditActor`。重放与首次写入使用同一 `SettlementAccess::require_action`。禁止在重放路径构造 Admin。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply-settlement --lib
```

### M-09 付款、开票、履约写命令在写入之后才授权

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

### M-10 商品导入写入口未授权

- 现状：`crates/orchestration/erp-data-import/src/product_import/submit.rs` 的 `submit`（第 40 行）解析并创建任务，无 `AuthorizationService::require`。`product_import/direct_upload.rs` 的 `init_direct_upload`（第 41 行）、`direct_upload_part_url`（第 77 行）、`submit_direct_upload`（第 112 行）、`abort_direct_upload`（第 155 行）无 actor、无 `require`。同 crate 的 `brand_import/mod.rs` 第 86 行对 `catalog::PRODUCT_BRAND_CREATE` 执行 `require(Operation)`；`supplier_import/submit.rs` 对 `internal::SUPPLIER_IMPORT_SUBMIT` 执行 `require`。商品导入权限常量已是 `catalog::PRODUCT_PRODUCT_CREATE` 的别名。路由层 `with_permission` 不作为本阶段已授权依据。
- 依据：重写方案 §3.5.1、§3.6.1。
- 修改要求：提交、直传初始化、分片地址、完成、取消在快照事务内先按商品创建资格 `require(Operation)`，再解析或签发上传地址。直传方法增加 `actor` 参数。HTTP 处理函数传入已认证 actor，属于本条调用方改动。不得删除路由层 `with_permission`。后台逐行执行仍按现有候选创建规则重验提交人。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-data-import --lib
```

本阶段不运行 `erp-server`。HTTP 处理函数只改传入 actor 所需的调用签名。

### M-11 采购数据范围端口先读单据再按动作名授权

- 现状：`crates/domains/erp-procurement/src/ports/data_scope.rs` 的 `qualify`、`require`、`resolve` 接收 `action: &str`。`service/purchase_order/access.rs` 的 `authorized_order` 先 `qualify`，再装载采购单，再 `scope.require`。`require_loaded` 把已装载单据交给 `action: &str` 的 `require`。生产路径仍走该端口：`erp-procure-to-pay` 的变更、运费豁免与授权装配，以及 `erp-read-models` 的采购中心与退货列表。`apps/erp-server/src/wiring/purchase.rs` 与 `reverse_flow.rs` 注入该端口。
- 依据：重写方案 §3.5.1、§3.5.3、§3.6.1。
- 修改要求：端口持有 `Arc<AuthorizationService>`。入口接收 `&Permission` 常量。先 `require` 再读单据。删除以 `&str` 动作名先读后授权的路径。旧 `wiring/purchase_data_scope.rs` 不在本条删除，见 P-02。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-procurement --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-procure-to-pay --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib
```

### M-12 供应商连接详情与目录在 query 与 validate 之间推进读取阶段

- 现状：`crates/read-models/erp-read-models/src/supplier_center/supplier_api/query.rs` 的 `connection_detail_for_actor`（第 166–177 行）先 `query(CONNECTION_DETAIL)`，随后 `detail_view` 经 `context.rs` 的 `connection_permissions` 调用 `authorization.permissions`，再 `validate`。`catalog.rs` 的 `catalog`（第 76–115 行）先 `query(CONNECTION_CATALOG_READ)`，再 `has_permission(SUPPLIER_OFFERING_COST_DETAIL)`，读取后 `validate`。`require`、`query`、`permissions`、`has_permission` 都会推进读取阶段，导致 `validate` 返回 `DATA_SCOPE_CHANGED`。
- 依据：重写方案 §3.6.5。
- 修改要求：同一快照事务内先完成展示资格查询，再 `query`，再读取，最后 `validate`。`query` 与 `validate` 之间不得再调用会推进读取阶段的授权入口。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib
```

### M-13 访问检查未走完整资格与权限来源解析

- 现状：`crates/domains/erp-identity/src/service/access_control/inspection.rs` 第 56 行 `AccessInspectionService::inspect`。第 63–65 行只 `require` 检查动作本身。第 80–85 行对可配置范围调用 `DataScopeService::resolve`。`DataScopeService::resolve` 在 `service/access_control/resolve.rs` 第 172 行，`resolve_boundary` 第 183 行，`batch` 第 137 行；`resolve/batch.rs` 第 69 行实现批量边界。生产调用方只有 `inspection.rs` 第 81 行。`facilities.rs` 使用的 `DataScopeService::qualified` 保留。
- 依据：重写方案 §3.3.4、§3.6.1、§3.7.3。
- 修改要求：`inspect` 先 `qualify` 完整功能资格。Scope 动作（含 `ScopeSource::Permission`）用本次合格角色解析范围。无对象上下文时不得报告对象授权通过。删除已无调用方的 `resolve`、`resolve_boundary`、`batch` 及 `resolve/batch.rs`。保留 `qualified`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib
```

### M-14 审批定义授权未强制快照且到期错误分类错误

- 现状：`crates/domains/erp-identity/src/service/authorization/definitions.rs` 第 54 行 `require_definition`。第 68 行只读取 `executor.boundary()`，不要求快照事务。第 73–76 行到期返回 `ValidationError("审批定义授权目标或执行阶段已失效")`。`require.rs` 第 33–44 行的快照守卫不覆盖定义候选。
- 依据：重写方案 §3.5.2、§3.6.5。
- 修改要求：只读定义候选强制快照事务边界。到期返回 `DATA_SCOPE_CHANGED`，不得返回 `ValidationError`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib
```

### M-15 供应商品与履约单转交未检查新归属

- 现状：`crates/domains/erp-supply/src/service/supplier_offering/handover.rs` 的 `persist_handover` 只通过 `require_offering` 检查原记录。`supplier_fulfillment/handover.rs` 的 `persist_handover` 只通过 `require_order` 检查原履约单。两处都未使用 `AccessTarget::OwnershipChange`。
- 依据：重写方案 §3.5.1、§3.6.3。
- 修改要求：更新前对原记录与新归属执行 `AccessTarget::OwnershipChange`。原记录检查与新归属检查在同一快照事务内、写入之前完成。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib
```

### M-16 结算作废在已作废分支于授权前返回

- 现状：`crates/domains/erp-supply/src/service/supplier_settlement/void.rs` 第 23 行 `prepare_void`。第 31 行先 `load_statement`。第 32–33 行已作废时直接返回。第 35 行才 `require_action`。
- 依据：重写方案 §3.5.1、§3.5.3。
- 修改要求：授权移到读取与幂等判断之前。已作废回放执行与首次作废相同的 `require_action(Void)`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib
```

### M-17 客户创建用功能预检

- 现状：`crates/domains/erp-customer/src/service/customer/access.rs` 第 291 行 `CustomerAccess::require_create` 对 `customer::CREATE` 调用 `has_permission`。
- 依据：重写方案 §3.5.1、§3.6.1。
- 修改要求：改为 `AuthorizationService::require`，目标为服务端构造的 `Candidate`。禁止继续用 `has_permission` 作为创建通过条件。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-customer --lib
```

### M-18 创建商品凭证未检查创建候选

- 现状：`crates/domains/erp-catalog` 的 `voucher_category_create` 路径上，`voucher.rs` 第 480 行 `new_voucher_product` 创建商品时未调用 `require_create(Candidate)`。
- 依据：重写方案 §3.5.1、§3.6.1。
- 修改要求：创建商品前在同一快照事务内 `require_create`，目标为服务端 `Candidate`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-catalog --lib
```

### M-19 商品查重跳过快照边界并用单条越权中断整批

- 现状：`crates/domains/erp-catalog` 的 `CatalogPortalService::duplicate_candidates`（`portal/query.rs` 第 274 行）用 `ensure_writable`（`has_permission`）。候选为空时跳过快照边界。范围外商品返回 `Forbidden` 并中断整次查重。
- 依据：重写方案 §3.5.2、§3.6.1。
- 修改要求：入口先校验快照边界，再按商品读取资格授权。空候选仍走该边界。单条范围外商品不使整次查重失败，按既有查重结果排除该商品。禁止用 `has_permission` 代替本次 `require` 或 `query`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-catalog --lib
```

### M-20 采购自反关系每次新建映射实例

- 现状：`crates/domains/erp-procurement/src/authorization/mapping.rs` 第 47 行 `relations`、第 85 行 `relation` 每次为 `self_purchase` 新建不同 `Arc`。`authorization/facts.rs` 第 65 行 `PurchaseFacts::load` 在第 76 行插入 `self_purchase`。全仓规则不使用该关系；仅 mapping、facts 与第六阶段 `wiring/purchase_data_scope.rs` 出现该名字。
- 依据：重写方案 §3.4.1。
- 修改要求：删除 `self_purchase` 关系及 facts 中的膨胀节点。`relations` 与 `relation` 不再登记该键。禁止保留空实现或按资源名返回另一份新建 `Arc`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-procurement --lib
```

### M-21 仓储前置失败未推进读取阶段

- 现状：`crates/foundation/persistence-core/src/repository/base.rs` 第 367 行 `Repository::update`、第 401 行 `soft_delete`、第 432 行 `restore` 在 `ensure_active_for_cas` 失败时返回，尚未执行会调用 `next_read` 的写入。`rotation.rs` 第 51 行 `advance` 在第 59 行快照检查失败时返回，第 62 行 `update_document` 失败时也可在 `mongo_ops::update_one` 之前返回。
- 依据：重写方案 §3.5.2。
- 修改要求：上述函数在入口立即 `executor.next_read()`，再执行前置检查与写入。前置失败不得留下未推进的读取阶段。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p persistence-core --lib
```

### M-22 审批定义在事务外预读并在退役时写回事务外节点

- 现状：`crates/domains/erp-workflow/src/service/approval/definition/command.rs` 第 95 行 `require_graph` 用 `NoTransaction` 装载定义图。`retire.rs` 第 35 行 `retire_definition` 在第 50 行事务外取得图，第 76 行事务使用该图。`replace.rs` 的 `replace_definition_nodes`、`publish.rs` 的 `publish_definition` 同样先事务外装载。
- 依据：重写方案 §3.5.2、§3.5.3。`backend/AGENTS.md`「事务」：决定写入的读取在事务内执行。
- 修改要求：`replace`、`publish`、`retire` 在唯一 `with_transaction` 内先授权再读取定义图。退役不得把事务外节点写入事务。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-workflow --lib
```

### M-23 集成差异列表、决定与范围时点

- 现状：`crates/domains/erp-integration/src/service/reconciliation_difference.rs` 第 51 行 `difference_page` 在第 76 行 `validate`，第 78 行才 `project_difference_rows`。`access.rs` 第 92 行 `require_decide` 第 98–103 行先 `find_by_id`，第 109 行用库内 `row.base.version` 构造 `Task`。`scope.rs` 第 33 行 `ScopedIntegrationList::from_access` 第 36 行用 `Instant::now()` 覆盖时点；`CompiledAccess::as_of` 在 `repository/authorization/query.rs` 第 271 行。
- 依据：重写方案 §3.5.2、§3.6.4、§3.6.5。
- 修改要求：`difference_page` 在补水之后 `validate`。`require_decide` 使用调用方给出的任务标识与版本，先授权再读库；库内版本不得回填成授权目标。`from_access` 使用 `access.as_of()`，禁止 `Instant::now()`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-integration --lib
```

### M-24 任务资格由调用方手写，结算复核执行子集为空

- 现状：`crates/orchestration/erp-supply-settlement/src/supply_settlement/reviewers.rs` 第 27 行 `qualified_reviewer` 手写 `STATEMENT_DETAIL`、`STATEMENT_REVIEW_DECIDE`、`work_item::DETAIL` 并调用 `permissions`。`crates/orchestration/erp-procure-to-pay/src/procure_to_pay/responsibility/eligibility.rs` 第 42 行 `assignment_owner` 第 43 行伪造 Admin，并手写询价查看、保存反馈、发布权限。`crates/orchestration/erp-order-to-cash/src/sales_inquiry/execution/apply.rs` 第 172 行 `Reassign` 调用 `ensure_reassignment_target`。`crates/domains/erp-identity/src/service/authorization/work_items.rs` 第 43 行 `READ_PERMISSIONS` 已登记 `SUPPLIER_SETTLEMENT_REVIEW` 的读取权限 `STATEMENT_DETAIL`。第 79 行起的 `EXECUTION` 不含该种类。第 258–266 行 `execution_requirements` 未命中时返回空切片，第 245–254 行 `requirements` 因此只加入读取权限。
- 依据：重写方案 §3.3.2、§3.6.3。
- 修改要求：已有任务改派改为 `WORK_ITEM_RECIPIENT` 加 `AccessTarget::Task { id, version }`。任务形成前的候选改为 `WORK_ITEM_EXECUTION_QUALIFICATION`。在 `EXECUTION` 为 `SUPPLIER_SETTLEMENT_REVIEW` / `supplier_settlement_statement` 增加执行子集，内容为现有 `STATEMENT_REVIEW_DECIDE`。不得新造权限编码。不得写成该种类的读取权限未登记。`assignment_owner` 删除 Admin 构造，使用调用方账号与统一任务资格入口。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply-settlement --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-procure-to-pay --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-order-to-cash --lib
```

### M-25 空文件列表跳过快照边界

- 现状：`crates/domains/erp-support/src/authorization/files.rs` 第 52 行 `require_files`。第 59–64 行在 `ids` 为空时调用 `permissions(actor, &[], executor)` 后返回，不检查快照边界。
- 依据：重写方案 §3.5.2。
- 修改要求：函数入口先校验快照事务边界，再按文件集合授权。空列表不得绕过该边界。同步修改把空列表放在 `NoTransaction` 上仍期望成功的测试。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-support --lib
```

## 3. 同批修改

事务拆分、授权前读取、仓储执行缺口、错误分类被吞没，与第 2 节同一批修改。本节省不得为搬迁改变授权结果。

### B-01 一个写用例拆成多个事务

- 现状：`crates/domains/erp-sales/src/service/sales_selection/publish.rs` 第 40 行 `publish` 在第 50 行使用 `NoTransaction` 做回放与装载，再进入后续提交事务。`crates/domains/erp-workflow/src/service/approval/execution/runtime_service/resume_apply.rs` 第 54 行 `resume_current_approver` 依次调用 `replay_resume`（第 104 行，独立事务）、`prepare_resume_runtime`、`commit_resume`（第 86 行，另一事务）。`crates/domains/erp-party/src/service/party/company.rs` 第 110 行 `create_company`、第 159 行 `update_company` 经 `company_snapshot`（第 210 行）与后续写入、回读各开事务。
- 依据：`backend/AGENTS.md`「事务」：一个用例一个 `with_transaction`。
- 修改要求：每个入口只保留一个 `with_transaction`。回放、准备、写入、回读接收该执行器。禁止在入口下再开事务。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-sales --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-workflow --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-party --lib
```

### B-02 回款、开票申请、付款申请与销售单提交拆事务或在事务外裸读

- 现状：客户回款提交与撤回在事务外用 `NoTransaction` 读取审批绑定或单据，再开写入事务；重放降为详情权限。开票申请撤回先用 `NoTransaction` 准备，再开第二个事务。付款申请提交、撤回、关闭用 `NoTransaction` 读取已提交回执；`close` 在 `payment_request/cancel.rs`，不存在单独的 `close.rs`。销售单 `submit_sales_order`（`command/submit.rs` 第 198 行）在事务外用 `ReadExecutor` 读取，并多次 `with_transaction`。
- 依据：`backend/AGENTS.md`「事务」：一个用例一个事务；决定写入的读取在事务内。重写方案 §3.6.1：重放校验原写命令权限。
- 修改要求：上述每个命令收敛为一个 `with_transaction`。事务外裸读移入该事务，且位于授权之后。重放执行原写命令权限，不得降为 `DETAIL`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-order-to-cash --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-finance-posting --lib
```

### B-03 列表、验收取号与寻源取号脱离授权事务

- 现状：`crates/orchestration/erp-supply-settlement/src/supply_settlement/list.rs` 的 `statement_list` 在授权事务之外用 `NoTransaction` 做名称匹配与后续读取。客户验收创建在授权前调用 `next_customer_acceptance_no`。`sourcing_create.rs` 在事务内调用不接收 executor 的 `next_delivery_no(db)`。
- 依据：重写方案 §3.5.3。`backend/AGENTS.md`「事务」：决定写入的读取使用同一 executor。
- 修改要求：读取与取号移入授权通过后的原事务，函数接收该 executor。禁止在授权前消耗单据号。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply-settlement --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-fulfillment-execution --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-procure-to-pay --lib
```

### B-04 授权前读取受保护对象

- 现状：
  - `crates/domains/erp-supply/src/service/supplier_settlement/difference.rs` 第 113 行 `prepare_difference_decision` 第 121 行先 `find_by_id`，第 142 行才 `require_action`。`draft.rs` 第 129 行 `prepare_refresh` 第 136 行先 `load_statement`，第 137 行才 `require_action`。
  - `crates/domains/erp-workflow/src/service/work_item/abandon_rework.rs` 第 73 行 `apply_abandon_rework` 第 83 行先 `find_work_item`，第 90 行才 `require`。
  - `runtime_service/query.rs` 第 324 行 `recovery_options_stage` 第 330 行先装载主体，第 331 行才检查读取资格。
  - `cancel_blocked.rs` 第 413 行 `load_cancel_source` 第 421 行先 `find_instance_by_id`，第 424 行才 `ensure_cancel_blocked_authorized`。
  - `query.rs` 第 156 行 `instance_list` 第 162 行调用 `ensure_active_instance_reader`；该方法第 434–435 行用 `NoTransaction` 调用 `has_permission`。
- 依据：重写方案 §3.5.3、§3.6.1。
- 修改要求：授权移到受保护对象读取之前。`instance_list` 删除事务外 `has_permission` 预检，在列表自己的快照事务内授权后再查询。`apply_abandon_rework` 使用请求中的任务标识与版本完成 `require`，再读任务。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-workflow --lib
```

### B-05 先读受保护事实再授权的详情入口

- 现状：`crates/read-models/erp-read-models/src/fulfillment_center/access.rs` 第 75 行 `AcceptanceReadService::detail` 在第 84–86 行 `load_customer_acceptance`，第 87–93 行才 `require_object`。`returns_center/purchase_return.rs` 第 278–280 行先 `find_by_id`，第 283 行才 `resolve(..., "detail")`。
- 依据：重写方案 §3.5.3、§3.6.1。
- 修改要求：先完成对象授权，再读取验收单或采购退货单。采购退货详情改为 `&Permission` 常量，不得再传 `"detail"` 字符串。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib
```

### B-06 查询执行缺少当前边界或查询模型核对

- 现状：`crates/domains/erp-finance/src/repository/supplier_deposit_query.rs` 第 52 行 `account_page` 只核对集合名，不调用 `ensure_current`，不核对资源与输出粒度。`crates/domains/erp-warehouse/src/repository/directory.rs` 第 143 行 `authorized_rows_exist` 直接使用 `stages()`；第 161 行 `authorized_ids_in` 只 `ensure_current`。`WarehouseQuery::ensure_current`（`authorization.rs` 第 245 行）只转发 `compiled.ensure_current`。`crates/domains/erp-supplier/src/authorization.rs` 第 80 行 `CompiledScope::ensure_current` 同样只转发边界，不核对资源、集合与输出粒度。
- 依据：重写方案 §3.6.4。
- 修改要求：执行查询前核对资源、集合、输出粒度与当前边界。任一不符返回校验错误并停止读取。业务过滤不得改写授权阶段已固定的字段。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-finance --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-warehouse --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier --lib
```

### B-07 列表拆成两个快照，对象检查自开空事务

- 现状：客户列表、供应商列表、合同列表各打开列表快照后再打开版本快照。`erp-customer` 的 `CustomerAccess::require`（`service/customer/access.rs` 第 206 行）、供应商 `access.rs` 的 `require`、合同 `authorization.rs` 的 `require` 自开事务且只返回 `()`。
- 依据：`backend/AGENTS.md`「事务」：一个用例一个事务；复用时传入原 executor。
- 修改要求：每个列表收敛为一个 `with_transaction`。删除只返回 `()` 的自开事务 `require`。调用方把原执行器传入对象检查。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-customer --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-contract --lib
```

### B-08 销售选品服务保存请求级操作人

- 现状：`crates/domains/erp-sales/src/service/sales_selection/mod.rs` 在结构体字段保存 `Option<(SelectionAccess, AuditActor)>`。`new` 只接收数据库，`with_authorization` 再写入该对。
- 依据：重写方案 §3.6.1。`backend/AGENTS.md`「逻辑归属」：多步写入留在 Service 公开入口，入口显式接收操作人。
- 修改要求：构造器注入 `AuthorizationService`。公开方法显式接收 `actor`。删除请求级 `(SelectionAccess, AuditActor)` 字段。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-sales --lib
```

### B-09 无授权且内部使用 NoTransaction 的公开方法

- 现状：下列领域方法无生产调用方，内部使用 `NoTransaction`：`SalesReviewService::sales_change_order_list`；`SalesSelectionService` 的 `booklet_detail`、`proposal_detail`、`admin_session`、`booklet`；`FulfillmentService::customer_acceptance_detail`；`SupplierService::load_supplier`；`ensure_capability_qualified` 的服务层包装；`WorkItemService::load`；`DocumentRegistryService::approval_binding`。编排与 HTTP 使用各自已授权入口，不调用这些方法。
- 依据：重写方案 §3.6.1。`backend/AGENTS.md`「事务」：禁止为有无事务复制实现。
- 修改要求：无生产调用方的删除。若删除前发现仍被已授权事务复用，改为接收 `executor: &mut dyn Executor` 并收窄可见性，不得保留无 actor 的公开包装。实体上同名的能力判断不删除。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-sales --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-fulfillment --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-workflow --lib
```

### B-10 数据转移在授权前读取任务或解密文件

- 现状：`crates/orchestration/erp-data-import` 的 `DataTransferProcess` 在 `status`、`context`、`result` 中，于授权前读取任务或解密私有文件。`context` 先 `owned_job` / `transfer_job` 再授权。
- 依据：重写方案 §3.5.3、§3.6.1。
- 修改要求：先在快照事务内授权，再读取任务和解密。解密不得发生在授权失败的路径上。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-data-import --lib
```

### B-11 交付前范围未复验，覆盖与导入批次脱离原执行器

- 现状：`crates/read-models/erp-read-models/src/purchase_center/change/query.rs` 第 62 行 `change_order_list` 经快照读取后返回，交付前未按 §3.6.4 复验范围。`catalog_center/delivery_coverage.rs` 第 188 行 `sku_delivery_inputs` 为公开函数，第 194、220、222 行使用 `NoTransaction`。`import_center.rs` 第 25 行 `batch_names`、第 45 行 `batch_name` 硬编码 `NoTransaction`。
- 依据：重写方案 §3.6.4、§3.6.5。
- 修改要求：`change_order_list` 在交付前 `validate`。`sku_delivery_inputs`、`batch_names`、`batch_name` 接收调用方原执行器，在该执行器上读取并 `validate`。`sku_delivery_inputs` 收窄为调用方所在模块可见，禁止保留公开的 `NoTransaction` 入口。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-read-models --lib
```

### B-12 预存款读取把多种错误吞成内部错误

- 现状：`crates/orchestration/erp-supplier-orders/src/supplier_deposits/read_access.rs` 第 36 行 `SupplierDepositReadAccess::authorize`。第 43–49 行映射 `Forbidden`、`NotFound`、`RepositoryError`、`BusinessLogicError`，其余含 `ConflictError`、`Unauthenticated`、`ValidationError` 进入 `_`，变成 `Internal`。
- 依据：重写方案 §4.1.5。
- 修改要求：按原错误分类映射 `ConflictError`、`Unauthenticated`、`ValidationError`。禁止落入 `Internal`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier-orders --lib
```

### B-13 合同映射对纯数据库关联做空准备

- 现状：`crates/domains/erp-contract/src/repository/authorization.rs` 第 120 行 `preparation` 返回 `Some(self)`。第 201 行 `MappingPreparation::prepare` 只读取组织状态并返回组织准备数据，合同关联本身来自数据库字段。`ports/participant.rs` 第 42 行 `ContractParticipantPort::revision` 有接线实现，生产读取使用第 76 行 `history` 的 `document_ids_in`，不调用 `revision`。
- 依据：重写方案 §3.3.2。
- 修改要求：删除 `MappingPreparation` 实现，`preparation` 使用默认的 `None`。删除无调用的 `revision` 及其实现。保留 `document_ids_in`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-contract --lib
```

### B-14 采购事实按动作名决定历史参与

- 现状：`apps/erp-server/src/wiring/authorization/commerce.rs` 第 162 行 `PurchaseObjects::load`。第 169 行用 `context.permission.action()` 是否为 `"list"` 或 `"detail"` 决定是否查询 `document_participants`。`wiring/authorization/catalog.rs` 不存在。范围定义在 `crates/domains/erp-identity/src/entity/access_control/scope_spec/catalog.rs`：`purchase_order::LIST` 与 `DETAIL`（第 226–227 行）为 Company 且 `allows_history = true`；同段其余采购动作 `allows_history = false`。`scope` 的参数顺序见 `scope_spec.rs` 第 54–60 行。
- 依据：重写方案 §3.4.1、§3.6.4。
- 修改要求：事实加载不得读取动作名字符串。是否装载历史参与由该权限的 `allows_history` 决定。`allows_history` 为 false 时不得查询 `document_participants`。Company 范围已无条件允许、历史参与不改变结果集时，不得生成该跨集合 lookup。
- 验收：本文件位于 `apps/erp-server`。本阶段不执行 `erp-server` 测试。采购映射与事实的库测试为：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-procurement --lib
```

`commerce.rs` 的分支删除由代码核对。构造编译见 P-03，本条不提前执行。

### B-15 跨域只读查询与单域查询的落点

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

## 4. 规范与测试

只改本清单触及的代码。不得为通过全仓体积检查拆未触及的历史方法。

### N-01 函数体内的完整模块路径

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

### N-02 公开方法的文档注释

- 现状：`crates/domains/erp-warehouse/src/service/warehouse/mod.rs` 的 `warehouse_fulfillment_handler_options` 接收 `actor: &AuditActor`，其 `# 参数` 写「无。」。本次修改新增 `actor` 或 `authorization` 参数的公开方法，文档未写这三项或把参数写成「无。」的，一并修改。
- 依据：`backend/AGENTS.md`「编码约定」：公共方法含 `# 参数`、`# 返回`、`# 错误`。
- 修改要求：补全这三段。新增的 actor 与授权服务写入 `# 参数`。无错误时 `# 错误` 写「无。」，有错误时写实际分类。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-warehouse --lib
```

其余 crate 随该公开方法所在条目的验收命令。

### N-03 本次修改的生产方法长度

- 现状：`submit_sales_order`（`command/submit.rs` 第 198 行起）有效行超过 50。本清单其他条目改动后，生产方法有效行也可能超过 50。
- 依据：`backend/AGENTS.md`「编码约定」：本次新增或修改的生产方法硬上限 50 有效行；禁止为通过全仓体积检查拆未触及的历史方法。
- 修改要求：本清单改动的生产方法超过 50 有效行时拆成私有函数。不拆未改动的历史方法。不运行 `./scripts/check-rust-size.sh` 作为本清单验收。
- 验收：随 B-02 的 `erp-order-to-cash` 命令，以及其他被拆方法所在 crate 的 `--lib` 命令。

### N-04 单测把内置权限写成字符串

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

### N-05 授权测试固定睡眠两秒

- 现状：`crates/domains/erp-identity/src/repository/authorization/phase5_tests.rs` 第 295 行、`service/authorization/tests/approval_management.rs` 第 358 行调用 `tokio::time::sleep(Duration::from_secs(2))`。
- 依据：`backend/AGENTS.md`「测试」：行为测试执行生产逻辑并断言具体结果。到期必须由测试控制。
- 修改要求：改为可控时钟或确定性到期时点。删除这两处两秒睡眠。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib
```

### N-06 工作区依赖与自动测试开关

- 现状：根 `Cargo.toml` 已有 `[workspace.dependencies]`，其中没有 `roxmltree`、`zeroize`。`crates/domains/erp-contract/Cargo.toml` 直接写 `roxmltree = "0.21.1"`。`crates/domains/erp-supply/Cargo.toml` 直接写 `zeroize`。`crates/domains/erp-notification/Cargo.toml` 的包头没有 `autotests = false`。
- 依据：`backend/AGENTS.md`「质量门禁」与工作区依赖约定：版本集中在 workspace；领域 crate 关闭自动集成测试发现。
- 修改要求：在根 `[workspace.dependencies]` 增加 `roxmltree` 与 `zeroize`（保留 supply 现有的 `serde` feature）。两个领域包改为 `workspace = true`。`erp-notification` 增加 `autotests = false`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-contract --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-notification --lib
```

### N-07 单 crate 测试缺少 test-support

- 现状：`erp-import` 的 dev-dependencies 只有 `erp-identity` 的 `test-support`，生产代码使用 `persistence_core::testing`。`erp-supply` 的 dev-dependencies 只有 `erp-identity` 的 `test-support`。`erp-warehouse` 的 dev-dependencies 没有 `erp-identity` 的 `test-support`，测试调用 `erp_identity::testing::unavailable_authorization`。
- 依据：`backend/AGENTS.md`「测试」：单 crate `--lib` 必须能编译。
- 修改要求：为上述三个包补齐实际用到的 `test-support` feature。`erp-import` 与 `erp-supply` 增加 `persistence-core` 的 `test-support`。`erp-warehouse` 增加 `erp-identity` 的 `test-support`。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-import --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-supply --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-warehouse --lib
```

### N-08 读取入口的授权行为测试

- 现状：`erp-supplier`、`erp-warehouse`、`erp-party` 的读取入口缺少允许、拒绝、非快照执行器拒绝三类行为测试。第 2 节各条的验收已覆盖该条自己的行为测试。
- 依据：`backend/AGENTS.md`「测试」：每个改动覆盖成功路径与权限拒绝；行为测试执行生产逻辑。
- 修改要求：为这三个 crate 的读取入口补允许、拒绝、非快照执行器拒绝。非快照用例使用 `NoTransaction` 或等价的非快照执行器，断言拒绝。禁止连接真实 MongoDB。
- 验收：

```text
env -u ERP_TEST_MONGO_URI cargo test -p erp-supplier --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-warehouse --lib
env -u ERP_TEST_MONGO_URI cargo test -p erp-party --lib
```

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
