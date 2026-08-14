# 开发业务数据重置执行合同

## 1. 适用范围

本合同用于清空开发环境中的旧销售单、客户、合同及其完整交易链，并为不兼容的审批工作流模型提供空库起点。

本工具不得用于生产环境。执行人必须对目标 MongoDB 和业务停写状态负责。

入口：

```bash
backend/scripts/reset-dev-business-data.sh
```

工具只读取指定 TOML 文件的 `database.uri` 和 `database.db_name`。工具不得输出 URI、用户名、密码或配置文件中的其他密钥。
`mongosh` 必须以 `--norc` 启动，禁止加载用户级 `.mongoshrc.js` 并引入合同外操作。

## 2. 强制前置条件

执行写操作前必须同时满足：

1. 已停止 web-api、worker、商城同步、回填任务和所有其他写入方；
2. 已完成一次默认预览并核对目标数据库、非零集合、共享 Party 和悬挂引用；
3. 已确认该数据库允许丢弃旧交易数据；
4. 已提供 `--execute`；
5. 已提供与配置完全一致的 `--confirm-db <db_name>`；
6. 非 loopback MongoDB 已额外提供 `--allow-remote`。
7. 远程库名含 `dev`/`test`/`stage`/`sandbox`/`local` 的独立边界标记；
8. 远程 URI 中每个主机都已在 `ERP_RESET_ALLOWED_REMOTE_HOSTS` 精确白名单中，不得使用通配符。

缺少任一执行参数时，工具不得进入写分支。

## 3. 删除合同

### 3.1 必须 drop 的不兼容集合

下列集合必须整体 drop，以同时移除旧文档和旧索引：

```text
work_items
approval_step_instances
approval_instances
approval_step_definitions
approval_definitions
```

### 3.2 必须整体重置的交易链

工具按叶子到根的顺序 drop 下列集合。完整、可执行的唯一清单由 `reset-dev-business-data.mongosh.js` 的 `DROP_GROUPS` 固定。

```text
# 通用集成、批处理与注册表
reconciliation_difference_resolutions reconciliation_differences
integration_error_tasks inbox_messages
supplier_api_health_check_runs supplier_api_connection_command_receipts
background_job_items background_jobs bulk_selection_items bulk_selection_snapshots
legacy_import_confirmations legacy_import_rows legacy_import_batches
document_attachments workflow_actions document_participants document_relations business_documents

# 商城、卡实例与同步
mall_snapshot_reapply_operations
mall_balance_restoration_allocations mall_balance_restorations
mall_refund_allocations mall_refund_lines mall_refunds
mall_after_sales_request_lines mall_after_sales_requests
mall_consumption_backfill_items mall_consumption_backfill_jobs
mall_sales_reconciliation_items mall_sales_reconciliation_jobs
mall_sales_order_snapshots mall_sales_sync_cursors mall_sales_sync_jobs master_mapping_tasks
mall_card_instance_corrections mall_balance_snapshots mall_card_instances mall_consumption_cutovers
mall_consumption_cost_assessments mall_item_funding_allocations mall_consumption_entries
mall_payment_sources mall_order_items mall_orders
mall_order_completion_facts mall_order_cancel_facts mall_order_facts

# 商品发布运行链；保留商品、SKU 与供应商供给主数据
system_safety_pause_operations product_publication_deliveries
product_publication_revision_media product_publication_revisions product_publications

# 供应商交易运行数据；不含供应商主数据
supplier_settlement_difference_evidence supplier_settlement_source_evidence
supplier_settlement_differences supplier_settlement_items supplier_settlement_statements
supplier_refund_allocations supplier_refund_facts
supplier_order_action_lines supplier_order_actions supplier_order_status_histories
supplier_fulfillment_items supplier_fulfillment_orders

# 退货、资金与成本
payment_reversals receipt_reversals supplier_refunds customer_refunds
purchase_return_lines purchase_return_orders sales_return_lines sales_return_cases
sales_invoice_allocations receipt_allocations receivable_entry_offsets receivable_funds_reviews
receivable_entries customer_receipts invoices receivable_accounts
purchase_invoice_allocations payment_allocations payable_entry_offsets payable_entries
supplier_payments payable_accounts cost_allocations cost_entries

# 履约与完整库存账本
acceptance_fulfillment_allocations customer_acceptance_lines customer_acceptances
service_fulfillments electronic_deliveries delivery_lines deliveries
purchase_receipt_lines purchase_receipts
stock_reservation_entries stock_reservations stock_adjustment_lines stock_adjustments
stock_movements stock_balances

# 采购、销售投影、销售复核和销售核心
purchase_change_submission_lines purchase_change_submissions purchase_change_orders
purchase_line_sales_allocations purchase_order_revision_lines purchase_order_revisions
purchase_order_submission_lines purchase_order_submissions purchase_orders
sales_order_projection_deliveries sales_order_projection_revisions sales_order_projections
sales_change_reviews sales_change_submission_lines sales_change_submissions sales_change_orders
low_margin_manager_confirmations
procurement_confirmation_lines procurement_confirmations sales_order_reviews
sales_order_goods_service_line_revisions sales_order_voucher_line_revisions
sales_order_revision_lines sales_order_revisions
sales_order_submission_lines sales_order_submissions
sales_order_working_copy_lines sales_order_working_copies sales_order_lines sales_orders

# 合同与客户角色
contract_revisions contracts
customer_assignments customer_profile_commands customer_accounts
```

库存采用完整账本重置。不得只删除销售预留而保留受其影响的移动、余额或调整事实。

### 3.3 必须按条件删除的共享数据

`external_identity_targets` 必须删除内部对象类型为 `customer`、`contract` 或 `sales_order` 的记录，以及指向待删映射的记录。`external_identity_maps` 必须删除 `object_type` 为 `customer`、`contract` 或 `sales_order` 的记录。`source_systems` 必须保留。

`supplier_api_connections` 必须保留；但其健康检查运行事实和命令回执被重置后，工具必须把仍带健康缓存的连接置为 `disabled`，并清空 `last_health_at`、`last_health_result`、`last_healthy_technical_config_version`。不得保留无法追溯来源的“最近健康”或继续启用状态。

删除交易链前必须固化全部待删 Party 候选：`customer_accounts.party_id`，合同、合同版本、销售单、销售草稿、销售提交、销售变更提交的 `settlement_party_id`，以及应收账户、收款和发票的往来主体 ID。满足下列任一条件的 Party 必须保留：

- 被 `supplier_accounts.party_id` 引用；
- 被 `supplier_commercial_profile_revisions.signing_entity_party_id` 引用；
- 被 `supplier_commercial_profile_revisions.payment_entity_party_id` 引用。

其余客户及结算链专属 Party 必须按 `party_bank_accounts`、`party_tax_profiles`、`party_addresses`、`party_contacts`、`party_revisions`、`parties` 的顺序删除。
删除这些 Party 前，工具必须删除指向它们的 `external_identity_targets`；只有在候选映射已无任何其他目标时，才可删除对应 `external_identity_maps`。指向供应商共享 Party 的目标和映射必须保留。

Party 专属链必须在 drop 客户、合同、销售和应收来源集合前删除。
该顺序用于保证中断后幂等重跑：若程序在 Party 删除后、来源集合 drop 前中断，
下次运行仍能从尚存的来源引用重建同一候选集并继续清理。
此中断窗口内可暂时存在指向已删 Party 的待重置引用，因此整个重置期间必须持续停止应用写入，
并在全部 drop 和后置校验完成前禁止重启应用。

## 4. 明确保留项

工具必须保留：

- 账号、角色、权限、数据范围和审计记录；
- `supplier_accounts` 及供应商资料、能力、资质、目录和接口配置；
- 商品、SKU、仓库、供应商供给和其他基础资料；商品发布运行链按 §3.2 重建；
- `source_systems`；
- `file_assets` 和对象存储对象；
- `document_number_counters`。

合同 PDF 和业务附件所引用的文件资产仅在摘要中计数。本工具不删除文件元数据或对象存储对象，避免在无法证明跨域无引用时造成不可恢复的数据丢失。后续文件销毁必须走独立、可审计的文件治理流程。

## 5. 执行程序

### 5.1 预览

从仓库根目录执行：

```bash
backend/scripts/reset-dev-business-data.sh
```

使用其他配置：

```bash
backend/scripts/reset-dev-business-data.sh --config /absolute/path/to/config.toml
```

预览只允许执行 `ping`、集合枚举、计数、`distinct` 和聚合校验，不得调用 `drop`、`deleteMany` 或其他写命令。

### 5.2 本地执行

```bash
backend/scripts/reset-dev-business-data.sh \
  --execute \
  --confirm-db <database.db_name>
```

### 5.3 远程开发库执行

```bash
backend/scripts/reset-dev-business-data.sh \
  --execute \
  --confirm-db <database.db_name> \
  --allow-remote
```

执行前必须在当前 shell 设置精确白名单：

```bash
export ERP_RESET_ALLOWED_REMOTE_HOSTS='mongo-dev-1.example.internal,mongo-dev-2.example.internal'
```

`--allow-remote` 只解除拓扑保护，不构成生产环境授权。脚本还必须验证开发库命名和全部远程主机精确白名单。

## 6. 验收合同

执行前必须输出：

- 每组待 drop 集合的文档计数和全部非零集合；
- customer/contract/sales 外部映射计数与待删客户及结算链专属 Party 目标计数；
- 客户账户 Party、客户/合同/销售结算链 Party、供应商共享 Party 与待删专属 Party 计数；
- 保留文件资产候选计数；
- 关键引用的悬挂数量。

执行后必须满足：

1. 全部 reset 集合不存在；
2. customer/contract/sales 外部映射、映射目标及任何指向已删映射的目标为零；
3. 客户及结算链专属 Party 及其子记录、指向该 Party 的外部映射目标为零；
4. 保留的供应商 Party 引用和外部映射引用，其悬挂数量不得高于执行前基线；
5. 工具以零状态退出并输出后置条件通过。

随后必须重启应用，以重建 MongoDB 索引和代码注册的审批定义。最后执行应用级销售、合同、客户及审批冒烟验收。

## 7. 失败处置

集合 drop 不在单一 MongoDB 事务内。执行中断时必须继续保持停写，并以完全相同的命令幂等重跑。不得手工跳过后置校验，不得在半清理状态恢复应用写入。
