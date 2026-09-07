# 阶段 17：启动、附件补偿与审批错误边界静态审查合同

## 1. 输入与判定

- 唯一审查树：`/private/tmp/erp-domain-crate-17-cutover`；业务输入固定 `a537414eb8f78c43ebc383a3457a45c437dececc`；after 为 root 冻结源码提交 `39c55021d8bb1d030eade4afb3e135e4b5a8a18b`。既有 after 清单已逐实际 commit blob 绑定，并与当前文件 SHA 相等。
- 原执行合同：`/tmp/cutover17-entry-wiring-contract.md`，SHA-256 `8534d9174310c6d5dad4eff7ad04a14518f3fea076c40b0072677372ef3dbc8b`。合同准备期源码为阶段 16 的 `72a0c79a...`；本审查全部生产 before 已重取实际输入 `a537414eb8f78c43ebc383a3457a45c437dececc`，不直接复用准备期源指纹。
- 索引全局顺序另以阶段 00 实际提交 `400ab4f7855255b284fe8a8e1caffe27acc96083` 为权威。阶段 17 按授权修复阶段 16 已继承的注册顺序漂移；16→17 索引登记存在有意的次序变化，不能表述为全量原序不变。
- 静态采集时间：`2026-09-07T06:42:01.635466+00:00`。全部登记 after 文件 SHA 在报告写入前再次与当前源码匹配。
- 判定：52 个非索引生产符号中，51 个函数体在仅替换批准的命名空间后 token 一致；审批列表 prepare 的 1 个差异仅为同一匹配臂表达式增加花括号，已逐式核对。未发现未处理的生产语义漂移。
- 索引判定：四个实际组合根各 27 次调用覆盖同一 19 域，展开为阶段 00 原 30 组、161 步（158 次集合 create_indexes + 3 次原 reconcile）；四根均逐步匹配。IndexModel 构造与 Mongo 调用 helper 保留，详见第 4 节。
- 本片只读源码、写 `/tmp`，未修改源码、未执行 Cargo、Rust 测试、应用或数据库。新增 RM 转接桥亦完成 typed 载荷与唯一分派核对；32 项原内联测试定义保留不代表运行通过；真实 MongoDB 未验证。root 提供的 sealed 动态门禁按 observer=root 单列于末节，不冒认为本 worker 执行。

## 2. 审查范围与实际 provider

- 独占范围为 AppState、CLI runtime/error/main、Web main、索引入口、附件补偿 helper 全部 11 个生产引用、WorkItemActionError 原两个 From、新增 ReadModel Error 转接桥与 reject_approval_task、ApprovalHttpError 的 from_service/From/响应封装、审批列表 422 特例。
- 两应用完整错误枚举转换、通用 HTTP 错误转换、7 个 duplicate 提示 helper、21 个审批码全面核销及新增 HTTP 黄金测试由父分片负责，本报告不复制该审查。Process errors 文件仅作为本片所需 Support/Workflow typed 转换和 code() 的真实 provider 指纹。

| 原 provider | 新真实 provider | 本片消费合同 |
|---|---|---|
| `services::identity_compose::shared_rbac_service` | `erp_processes::adapters::identity::shared_rbac_service` | 原 Database clone→MongoIdentityAudit→共享 RBAC；AppState 只创建一次并复用 Arc。 |
| `services::workflow_compose::{WorkflowAuth,workflow_auth,workflow_audit,workflow_object_facts,work_item_service}` | `erp_processes::adapters::workflow` 同名真实符号；WorkflowAuth 定义在 authorization 叶并由根导出 | 参数、Arc 与 with_ports 的实参顺序保持。 |
| `services::support_audit::MongoSupportAudit` | `erp_processes::adapters::support_audit::MongoSupportAudit` | 文件、批量任务、来源注册的原审计 port。 |
| `services::support_documents::MongoBusinessDocument` | `erp_processes::adapters::support_documents::MongoBusinessDocument` | 文件与批量任务的原单据 port。 |
| `services::Error` / `services::ErrorCode` | `erp_processes::Error` / `erp_workflow::ErrorCode` | Workflow 的 Coded、Support 的 OutcomeUnknown 与原 payload 仍直接保留；本片先识别 typed 结构再走 HTTP。 |

## 3. 启动顺序执行合同

AppState::new 保持 fail_closed 外部连接器并委派 new_with_connectors；new_with_connectors 的构造序为：

1. 用配置快照 secret 建立 SensitiveDataCodec。
2. 用 db.clone() 建立 shared RBAC。
3. 用同一 RBAC Arc 建立 ApprovalActionRegistry。
4. 构造 ApprovalRuntimeService::with_ports，顺序为 db、workflow_auth(db,同一 RBAC Arc)、action port、ProcessObjectRead、ProcessUpgradeSubject(db)、workflow_audit(db)、workflow_object_facts(db)。
5. 构造 ApprovalNotificationOutboxPort。
6. 构造 MongoIntegrationEvidenceAuthority。
7. 构造 MongoCatalogSupplyQuery，最后装配原字段。

- file_asset_service 与 bulk_job_service 的实参仍为 db→audit port→document port；source_registry_service 仍为 db→audit port。approval_runtime_service 与 rbac getter 仍只 clone 原 Arc，未新增重建或鉴权。
- Web main 保持 tracing 配置→init_tracing→SafeConfig::from_args→读取端口/记录日志→start。Box<dyn Error + Send + Sync> 返回边界不改。
- Web start 保持 cfg.snapshot→build_storage→connect→AppState→ensure_transaction_support→完整索引序列→固定 policy 校验→root role→predefined roles→config watcher→outbox worker→run_app→worker.stop→返回 run_app 原结果。索引失败仍阻止后续 policy/角色/worker 初始化；run_app 的 Err 仍先经过 worker.stop。
- ensure_registered_approval_policies 的函数体保持 ALL_DOCUMENT_TYPES 原序逐项 policy_of?；唯一允许的签名变化为 services::Result 型边界改为 erp_workflow::Result<()>。Web 外层 Box Error 与调用时点保持。
- CLI AdminRuntime::connect 保持 Config::from_file→connect→ensure_transaction_support→完整索引→shared RBAC→AdminService::new。CLI main 保持 init_tracing→Cli::parse→命令分派；失败仍 stderr Display 并 exit(1)。
- CLI Error 仅删除原无生产构造/传播者的 Service variant：对输入 CLI 源码的引用核对只找到该声明。Config、Database、Identity、Usage 及透明 Display/#from 保留；删去该块后的枚举 token 与 after 相等。

## 4. 索引全局顺序修复

- 权威 before：`400ab4f7855255b284fe8a8e1caffe27acc96083:backend/database/src/indexes/mod.rs` 的实际 30 组，及其原叶/真实常量定义。phase16 输入 `a537414eb8f78c43ebc383a3457a45c437dececc` 的 19-root 全序只保留为中间证据；其第 3 步是 casbin_rules，而阶段 00 第 3 步应为 audit_logs，证明继承漂移确实存在。
- after 使用 identity 两个窄入口，及 workflow/support/finance 的单一叶 ensure reexport；没有复制 IndexModel 定义。四个根分别为 Web indexes.rs、CLI indexes.rs、Process test_indexes.rs、ReadModel test_indexes.rs。后两者只在本次最终顺序一致性核对中纳入，不运行其测试。
- Web/CLI 各 27 个实际调用均为同一 db 引用、串行 await?，返回 persistence_core::Result<()>。组合展开后的尾调用仍返回原 Result；不存在 join、吞错或并发注册。

| 序号 | 四根一致的实际调用 |
|---:|---|
| 1 | `erp_identity::indexes::ensure_accounts_and_roles(db).await?` |
| 2 | `erp_audit::indexes::ensure(db).await?` |
| 3 | `erp_identity::indexes::ensure_authorization(db).await?` |
| 4 | `erp_workflow::indexes::ensure_approval_integration(db).await?` |
| 5 | `erp_workflow::indexes::ensure_bpm(db).await?` |
| 6 | `erp_support::indexes::ensure_bulk_job(db).await?` |
| 7 | `erp_catalog::indexes::ensure(db).await?` |
| 8 | `erp_contract::indexes::ensure(db).await?` |
| 9 | `erp_finance::indexes::ensure_cost(db).await?` |
| 10 | `erp_customer::indexes::ensure(db).await?` |
| 11 | `erp_workflow::indexes::ensure_document_registry(db).await?` |
| 12 | `erp_support::indexes::ensure_file_asset(db).await?` |
| 13 | `erp_fulfillment::indexes::ensure(db).await?` |
| 14 | `erp_integration::indexes::ensure(db).await?` |
| 15 | `erp_inventory::indexes::ensure(db).await?` |
| 16 | `erp_import::indexes::ensure(db).await?` |
| 17 | `erp_party::indexes::ensure(db).await?` |
| 18 | `erp_finance::indexes::ensure_payable(db).await?` |
| 19 | `erp_procurement::indexes::ensure(db).await?` |
| 20 | `erp_finance::indexes::ensure_receivable(db).await?` |
| 21 | `erp_returns::indexes::ensure(db).await?` |
| 22 | `erp_sales::indexes::ensure(db).await?` |
| 23 | `erp_support::indexes::ensure_source_registry(db).await?` |
| 24 | `erp_supplier::indexes::ensure(db).await?` |
| 25 | `erp_supply::indexes::ensure(db).await?` |
| 26 | `erp_warehouse::indexes::ensure(db).await?` |
| 27 | `erp_workflow::indexes::ensure_work_item(db).await?` |

以下按阶段 00 的 30 组列出真实集合写入序；完整 161 步的 before/after provider、builder、传播方式与源码 SHA 在索引 JSON 中逐项列出。

| 组序 | 阶段 00 组 | 集合 create_indexes 原序 | 额外原步骤 |
|---:|---|---|---|
| 1 | `access_control` | `accounts` → `roles` → `audit_logs` → `casbin_rules` → `permissions` → `user_roles` → `data_scopes` → `audit_events` | — |
| 2 | `approval_integration` | `approval_subject_snapshots` → `approval_notification_outbox` | — |
| 3 | `bpm` | `approval_process_definitions` → `approval_node_definitions` → `approval_transition_definitions` → `approval_process_instances` → `approval_node_executions` → `approval_instance_assignees` → `approval_command_receipts` | — |
| 4 | `bulk_job` | `bulk_selection_snapshots` → `bulk_selection_items` → `background_jobs` → `background_job_items` | — |
| 5 | `catalog` | `product_categories` → `product_brands` → `unit_of_measures` → `sku_attributes` → `sku_attribute_values` → `product_category_attributes` → `products` → `product_revisions` → `product_revision_medias` → `skus` → `sku_revisions` → `sku_revision_attribute_values` → `voucher_category_profile_revisions` | — |
| 6 | `contract` | `contracts` → `contract_revisions` | — |
| 7 | `cost` | `cost_entries` → `cost_allocations` | — |
| 8 | `customer` | `customer_accounts` → `customer_assignments` → `customer_profile_commands` | — |
| 9 | `document_registry` | `business_documents` → `document_relations` → `document_participants` → `workflow_actions` | — |
| 10 | `file_asset` | `file_assets` → `document_attachments` | — |
| 11 | `fulfillment` | `purchase_receipts` → `purchase_receipt_lines` → `deliveries` → `delivery_lines` → `electronic_deliveries` → `service_fulfillments` → `customer_acceptances` → `customer_acceptance_lines` → `acceptance_fulfillment_allocations` | — |
| 12 | `integration_ops` | `inbox_messages` → `integration_error_tasks` → `reconciliation_differences` → `reconciliation_difference_resolutions` | — |
| 13 | `inventory` | `stock_movements` → `stock_balances` → `stock_reservations` → `stock_reservation_entries` → `stock_adjustments` → `stock_adjustment_lines` | `reconcile_stock_reservation_source_indexes` |
| 14 | `legacy_import` | `legacy_import_batches` → `legacy_import_rows` → `legacy_import_confirmations` | — |
| 15 | `party` | `parties` → `party_revisions` → `party_contacts` → `party_addresses` → `party_tax_profiles` → `party_bank_accounts` | — |
| 16 | `payable` | `payable_accounts` → `payable_entries` → `payable_entry_offsets` → `supplier_payments` → `payment_allocations` → `purchase_invoice_allocations` | — |
| 17 | `procurement_responsibility` | `procurement_responsibility_rules` | — |
| 18 | `purchase_order` | `purchase_orders` → `purchase_order_submissions` → `purchase_order_submission_lines` → `purchase_order_revisions` → `purchase_order_revision_lines` → `purchase_line_sales_allocations` → `purchase_change_orders` → `purchase_change_submissions` → `purchase_change_submission_lines` | `reconcile_purchase_order_no_index` |
| 19 | `receivable` | `receivable_accounts` → `receivable_entries` → `receivable_funds_reviews` → `receivable_entry_offsets` → `customer_receipts` → `receipt_allocations` → `invoices` → `sales_invoice_allocations` | — |
| 20 | `returns` | `sales_return_cases` → `sales_return_lines` → `purchase_return_orders` → `purchase_return_lines` → `customer_refunds` → `supplier_refunds` → `receipt_reversals` → `payment_reversals` | — |
| 21 | `sales_order` | `sales_orders` → `sales_order_lines` → `sales_order_working_copies` → `sales_order_working_copy_lines` → `sales_order_submissions` → `sales_order_submission_lines` → `sales_order_revisions` → `sales_order_revision_lines` → `sales_order_goods_service_line_revisions` → `sales_order_voucher_line_revisions` | — |
| 22 | `sales_review` | `sales_change_orders` → `sales_change_submissions` → `sales_change_submission_lines` | — |
| 23 | `source_registry` | `source_systems` → `external_identity_maps` → `external_identity_targets` | — |
| 24 | `supplier` | `supplier_accounts` → `supplier_commercial_profile_revisions` → `supplier_capabilities` → `supplier_capability_revisions` → `supplier_qualifications` → `supplier_qualification_revisions` → `supplier_qualification_capabilities` → `supplier_rating_revisions` → `supplier_profile_commands` | — |
| 25 | `supplier_api` | `supplier_api_connections` → `supplier_api_capabilities` → `supplier_api_business_capability_confirmations` → `supplier_api_health_check_runs` → `supplier_api_connection_command_receipts` | — |
| 26 | `supplier_offering` | `supplier_offerings` → `supplier_offering_revisions` → `supplier_offering_availabilities` → `supplier_offering_commands` | — |
| 27 | `supplier_fulfillment` | `supplier_fulfillment_orders` → `supplier_fulfillment_items` → `supplier_order_actions` → `supplier_order_action_lines` → `supplier_order_status_histories` → `supplier_refund_facts` → `supplier_refund_allocations` | — |
| 28 | `supplier_settlement` | `supplier_settlement_statements` → `supplier_settlement_items` → `supplier_settlement_differences` → `supplier_settlement_source_evidence` → `supplier_settlement_difference_evidence` | — |
| 29 | `warehouse` | `warehouses` → `warehouse_revisions` → `warehouse_sku_policies` | — |
| 30 | `work_item` | `work_items` → `finance_responsibility_rules` | `reconcile_open_object_type_index` |

- 相对次序必须连同 reconcile 保留：第 63 步 stock_reservations reconcile 位于 balance 创建后、reservation 创建前；第 84 步 purchase_orders reconcile 位于采购单索引创建前；第 159 步 work_items reconcile 位于 work item 两个集合索引创建前。三函数的原正文 token 均一致，不能将条件 drop/list 操作从创建序列中省略。
- 完整计数已与 F 的独立解析逐位置核对为 161。早期 129 的不完整口径作废：其解析漏 29 个 builder 尾逗号调用、1 个 supplier_offering 尾部 await 返回，以及 inventory/purchase_order 的 2 个 reconcile；不能把该初稿与最终全量并列。
- 158 个实际 builder 的函数体与阶段 00 同位原函数 token 一致；95 个实际 Mongo create_indexes 与 named/unique/partial 构造 helper 的函数体也一致。由此保留键顺序、name、unique、partial 及原未设置 options；实际库中的索引状态仍未验证。
- 对阶段 16→17 的 48 个领域索引文件进一步核对：所有既有非 ensure 函数体均保留。改动限于 identity ensure 拆分、窄索引公开/导出与相应根登记；本次不把文件 SHA 不同误判为 IndexModel 改动。
- 独立索引证据：`/tmp/cutover17-index-sequence-review.json`，SHA-256 `ce175486c31602a0ae1473db1c320304b6ea7884185f0f3f8361cb7c9dd69150`。该文件通过真实代码递归展开、解析集合常量取得 161 步，不使用 root 提供的列表代替实际展开。root 修复清单仅作为辅助输入并单独记录 SHA。

## 5. 附件补偿矩阵与 11 个引用

helper 仍只匹配顶层 OutcomeUnknown；不得替换成 command_may_have_committed。后者还包含 ReceiptDuplicate 与 TransientTransaction，会改变当前合同。

| Process Error variant | 是否补偿本次 pending 对象 |
|---|---|
| OutcomeUnknown | 否，保留对象等待同一命令核对。 |
| ReceiptDuplicate、TransientTransaction | 是，保持原补偿分支。 |
| RepositoryError（包括其 payload 的任何持久化分类） | 是，helper 不按内部 payload 重新分类。 |
| Internal、NotFound、ValidationError、BusinessLogicError、ConflictError、Forbidden、Unauthenticated、Logic、Rbac、Coded | 是，保持原分支。 |

| 生产符号 | before→after 位置 | 结果/清理次序 |
|---|---|---|
| `contract_upload` | `backend/apps/web-api/src/core/handler/contract/mod.rs:93` → `:93` | 原 object_key；helper 为 true 时单对象 delete，忽略清理错误后返回原业务错误。 |
| `product_brand_create_with_assets` | `backend/apps/web-api/src/core/handler/catalog/mod.rs:236` → `:236` | 成功原样返回；失败先 helper/cleanup 再转换原错误。 |
| `product_brand_update_with_assets` | `backend/apps/web-api/src/core/handler/catalog/mod.rs:295` → `:295` | 成功原样返回；失败先 helper/cleanup 再转换原错误。 |
| `product_create_with_assets` | `backend/apps/web-api/src/core/handler/catalog/product.rs:117` → `:117` | 成功原样返回；失败先 helper/cleanup 再转换原错误。 |
| `product_update_with_assets` | `backend/apps/web-api/src/core/handler/catalog/product.rs:175` → `:175` | 成功原样返回；失败先 helper/cleanup 再转换原错误。 |
| `service_fulfillment_confirm` | `backend/apps/web-api/src/core/handler/fulfillment/mod.rs:525` → `:525` | 成功原样返回；失败先 helper/cleanup 再转换原错误。 |
| `supplier_profile_create_with_assets` | `backend/apps/web-api/src/core/handler/supplier/mod.rs:54` → `:54` | 成功且 !assets_committed 先清理本次 pending 再返回原视图；失败先 helper/cleanup 再转换原错误。 |
| `supplier_profile_update_with_assets` | `backend/apps/web-api/src/core/handler/supplier/mod.rs:111` → `:111` | 成功且 !assets_committed 先清理本次 pending 再返回原视图；失败先 helper/cleanup 再转换原错误。 |
| `file_asset_upload` | `backend/apps/web-api/src/core/handler/file_asset/mod.rs:158` → `:158` | 原 Support 结果先转 Process Error→helper→cleanup→Err 转 HTTP；document_id Some/None 的注册分支不改。 |
| `should_compensate_pending_assets` | `backend/apps/web-api/src/core/handler/file_asset/mod.rs:524` → `:524` | 唯一 helper 定义；仅 OutcomeUnknown 返回 false。 |
| `supplier_payment_commit` | `backend/apps/web-api/src/core/handler/payable/mod.rs:230` → `:230` | 成功且 !assets_committed 先清理本次 pending 再返回原视图；失败先 helper/cleanup 再转换原错误。 |

- 计数为 10 个生产调用 + 1 个唯一 helper 定义；imports 与测试引用不冒认为额外生产入口。全部 11 个函数体在批准命名空间替换后 token 一致。原对象上传、pending.clone、actor/ID/清理参数、时点、逐项清理及错误忽略方式均随原函数保留。

## 6. 审批错误、UUID 与请求头合同

- 新增 WorkItemActionError::from(ReadModelError) 只有 erp_processes::Error::from(error).into()。Process 的实际 from_domain!(erp_read_models,Rbac,Coded) 将 14 个 variant/payload 逐个移动；桥内没有 UUID、HTTP 转换或第二次 special-code 分派，仍只进入既有 Process From 的一次审批保护判断。
- 该新桥处理 Applied 分支写命令完成后的 work_item_detail(...).await? 错误；Conflict 分支的详情读取仍使用 await.ok()，错误被保留为 None，不进入新桥，也不改变原冲突响应。
- WorkItemActionError::from(WorkflowError) 仍先把相同 variant/payload 送入应用 Error，再委派第二个 From。第二个 From 先检查 ApprovalGenericWorkItemMutationForbidden；命中后才 new_v4、构造 ApprovalProtected，未命中才 HttpError::from。该保护码仍为 409，使用 workflow 唯一 ErrorCode；无请求头路径仍生成新 UUID。
- reject_approval_task 仅在“不是 DocumentApproval 且 approval_node_execution_id 为 None”时放行。DocumentApproval 或携带 node execution ID 的任务均在原分支拒绝；拒绝分支才调用 correlation_id(headers)，继而构造同一保护码。
- ApprovalHttpError::from_service 的顺序保持 error.into→correlation_id(headers)→error.code()→有码直接 coded；无码才 HttpError::from→from_http。不得先走通用 HTTP 再尝试恢复审批码。From<ProcessError> 仍传空 HeaderMap；From<HttpError> 仍在进入 from_http 前 new_v4。
- correlation_id 仍取 X-Trace-Id 的有效 UTF-8 非空值，保持原字符串且不 trim、不验证 UUID 格式；无效 UTF-8、空值或缺失才 new_v4。三个实际 UUID 生成符号及其分支位置均保留：correlation_id、WorkItem From<ProcessError>、Approval From<HttpError>。
- coded 仍按状态计算隐藏规则：403/404 删除输入 data。IntoResponse 保持 field_errors=None、原 retryable 与 success=false；仅 409 由 conflict_data 合并 correlation_id，非对象 data 放入 payload，原对象中的同名 correlation_id 被当前值覆盖。此层不新增响应 Header 写入。
- ok_json 仍先 serde_json::to_value，再成功 ApiResponse；失败把相同诊断字符串放入 Process Internal 并通过同一 Approval From 路径生成关联 ID。decision_command 仍先 parse expected_task_version，后判 Reject 空原因；首错顺序、actor 注入与原 idempotency_key 不改。

## 7. 审批列表 422 特殊分支

1. PreparedInstanceListQuery::from_request_parts 先 clone 原 headers，再做 Axum Query 提取；提取失败在原位置映射 unprocessable。
2. prepare 先 decode_cursor，失败在原位置映射 unprocessable；成功后保存 view，再以原字段次序调用 RuntimeInstanceListQuery::prepare。
3. 返回错误先经 Process typed conversion；仅 ValidationError(message) 直接进入 ApprovalHttpError::unprocessable(message,headers)，保留 422。其它错误继续 from_service(error,headers)。
4. 该分支 after 只因 rustfmt 将相同 unprocessable 调用包装为无分号表达式块；表达式、参数、求值次数及返回值不变。不能把本特例挪入普通 HTTP ValidationError→400 分支。

## 8. 52 个生产符号核销

命名空间归一化仅按执行合同替换真实 qualified token；字符串值不参与路径重写。下表“同序”表示函数体 token 一致或明确登记的单表达式块适配，不表示已运行。

| 符号 | before 位置 | after 位置 | 函数体核对 |
|---|---|---|---|
| `new` | `backend/apps/web-api/src/app_state.rs:153` | `backend/apps/web-api/src/app_state.rs:153` | namespace 归一 token 一致 |
| `new_with_connectors` | `backend/apps/web-api/src/app_state.rs:158` | `backend/apps/web-api/src/app_state.rs:158` | namespace 归一 token 一致 |
| `rbac` | `backend/apps/web-api/src/app_state.rs:226` | `backend/apps/web-api/src/app_state.rs:226` | namespace 归一 token 一致 |
| `file_asset_service` | `backend/apps/web-api/src/app_state.rs:231` | `backend/apps/web-api/src/app_state.rs:231` | namespace 归一 token 一致 |
| `bulk_job_service` | `backend/apps/web-api/src/app_state.rs:240` | `backend/apps/web-api/src/app_state.rs:240` | namespace 归一 token 一致 |
| `source_registry_service` | `backend/apps/web-api/src/app_state.rs:249` | `backend/apps/web-api/src/app_state.rs:249` | namespace 归一 token 一致 |
| `approval_runtime_service` | `backend/apps/web-api/src/app_state.rs:267` | `backend/apps/web-api/src/app_state.rs:267` | namespace 归一 token 一致 |
| `approval_outbox_port` | `backend/apps/web-api/src/app_state.rs:275` | `backend/apps/web-api/src/app_state.rs:275` | namespace 归一 token 一致 |
| `work_item_authorization` | `backend/apps/web-api/src/app_state.rs:313` | `backend/apps/web-api/src/app_state.rs:313` | namespace 归一 token 一致 |
| `main` | `backend/apps/web-api/src/main.rs:47` | `backend/apps/web-api/src/main.rs:49` | namespace 归一 token 一致 |
| `start` | `backend/apps/web-api/src/main.rs:114` | `backend/apps/web-api/src/main.rs:116` | namespace 归一 token 一致 |
| `ensure_registered_approval_policies` | `backend/apps/web-api/src/main.rs:148` | `backend/apps/web-api/src/main.rs:150` | namespace 归一 token 一致 |
| `build_storage` | `backend/apps/web-api/src/main.rs:165` | `backend/apps/web-api/src/main.rs:167` | namespace 归一 token 一致 |
| `spawn_config_watcher` | `backend/apps/web-api/src/main.rs:190` | `backend/apps/web-api/src/main.rs:192` | namespace 归一 token 一致 |
| `run_app` | `backend/apps/web-api/src/main.rs:318` | `backend/apps/web-api/src/main.rs:320` | namespace 归一 token 一致 |
| `main` | `backend/apps/cli/src/main.rs:26` | `backend/apps/cli/src/main.rs:28` | namespace 归一 token 一致 |
| `run` | `backend/apps/cli/src/main.rs:40` | `backend/apps/cli/src/main.rs:42` | namespace 归一 token 一致 |
| `init_tracing` | `backend/apps/cli/src/main.rs:59` | `backend/apps/cli/src/main.rs:61` | namespace 归一 token 一致 |
| `connect` | `backend/apps/cli/src/runtime.rs:27` | `backend/apps/cli/src/runtime.rs:27` | namespace 归一 token 一致 |
| `reject_approval_task` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:225` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:231` | namespace 归一 token 一致 |
| `work_item_action_response` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:240` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:246` | namespace 归一 token 一致 |
| `into_response` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:145` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:151` | namespace 归一 token 一致 |
| `new` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:38` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:38` | namespace 归一 token 一致 |
| `coded` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:67` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:67` | namespace 归一 token 一致 |
| `from_service` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:88` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:88` | namespace 归一 token 一致 |
| `from_http` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:105` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:105` | namespace 归一 token 一致 |
| `unprocessable` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:121` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:121` | namespace 归一 token 一致 |
| `bad_request` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:133` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:133` | namespace 归一 token 一致 |
| `into_response` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:185` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:185` | namespace 归一 token 一致 |
| `correlation_id` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:208` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:208` | namespace 归一 token 一致 |
| `conflict_data` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:263` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:263` | namespace 归一 token 一致 |
| `status_of` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:283` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:283` | namespace 归一 token 一致 |
| `from_request_parts` | `backend/apps/web-api/src/core/handler/approval_instance/http.rs:59` | `backend/apps/web-api/src/core/handler/approval_instance/http.rs:59` | namespace 归一 token 一致 |
| `prepare` | `backend/apps/web-api/src/core/handler/approval_instance/http.rs:73` | `backend/apps/web-api/src/core/handler/approval_instance/http.rs:73` | 相同匹配臂表达式块适配，同序 |
| `runtime_service` | `backend/apps/web-api/src/core/handler/approval_instance/mod.rs:285` | `backend/apps/web-api/src/core/handler/approval_instance/mod.rs:285` | namespace 归一 token 一致 |
| `ok_json` | `backend/apps/web-api/src/core/handler/approval_instance/mod.rs:292` | `backend/apps/web-api/src/core/handler/approval_instance/mod.rs:292` | namespace 归一 token 一致 |
| `decision_command` | `backend/apps/web-api/src/core/handler/approval_instance/mod.rs:313` | `backend/apps/web-api/src/core/handler/approval_instance/mod.rs:313` | namespace 归一 token 一致 |
| `WorkItemActionError::from(WorkflowError)` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:115` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:115` | namespace 归一 token 一致 |
| `WorkItemActionError::from(ProcessError)` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:128` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:134` | namespace 归一 token 一致 |
| `ApprovalHttpError::from(ProcessError)` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:162` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:162` | namespace 归一 token 一致 |
| `ApprovalHttpError::from(HttpError)` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:175` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:175` | namespace 归一 token 一致 |
| `contract_upload` | `backend/apps/web-api/src/core/handler/contract/mod.rs:93` | `backend/apps/web-api/src/core/handler/contract/mod.rs:93` | namespace 归一 token 一致 |
| `product_brand_create_with_assets` | `backend/apps/web-api/src/core/handler/catalog/mod.rs:236` | `backend/apps/web-api/src/core/handler/catalog/mod.rs:236` | namespace 归一 token 一致 |
| `product_brand_update_with_assets` | `backend/apps/web-api/src/core/handler/catalog/mod.rs:295` | `backend/apps/web-api/src/core/handler/catalog/mod.rs:295` | namespace 归一 token 一致 |
| `product_create_with_assets` | `backend/apps/web-api/src/core/handler/catalog/product.rs:117` | `backend/apps/web-api/src/core/handler/catalog/product.rs:117` | namespace 归一 token 一致 |
| `product_update_with_assets` | `backend/apps/web-api/src/core/handler/catalog/product.rs:175` | `backend/apps/web-api/src/core/handler/catalog/product.rs:175` | namespace 归一 token 一致 |
| `service_fulfillment_confirm` | `backend/apps/web-api/src/core/handler/fulfillment/mod.rs:525` | `backend/apps/web-api/src/core/handler/fulfillment/mod.rs:525` | namespace 归一 token 一致 |
| `supplier_profile_create_with_assets` | `backend/apps/web-api/src/core/handler/supplier/mod.rs:54` | `backend/apps/web-api/src/core/handler/supplier/mod.rs:54` | namespace 归一 token 一致 |
| `supplier_profile_update_with_assets` | `backend/apps/web-api/src/core/handler/supplier/mod.rs:111` | `backend/apps/web-api/src/core/handler/supplier/mod.rs:111` | namespace 归一 token 一致 |
| `file_asset_upload` | `backend/apps/web-api/src/core/handler/file_asset/mod.rs:158` | `backend/apps/web-api/src/core/handler/file_asset/mod.rs:158` | namespace 归一 token 一致 |
| `should_compensate_pending_assets` | `backend/apps/web-api/src/core/handler/file_asset/mod.rs:524` | `backend/apps/web-api/src/core/handler/file_asset/mod.rs:524` | namespace 归一 token 一致 |
| `supplier_payment_commit` | `backend/apps/web-api/src/core/handler/payable/mod.rs:230` | `backend/apps/web-api/src/core/handler/payable/mod.rs:230` | namespace 归一 token 一致 |

## 9. 原测试定义与运行边界

- 在本片检查的入口/错误/上传文件中，32 个原内联测试全部保留；函数体在批准命名空间替换后 token 一致，原 ignore 状态保持。此处核对定义与断言存在性，不把这些函数定义或源码匹配结果登记成测试通过。
- 索引首次失败位置、应用启动、真实 MongoDB 索引创建/修正以及对象存储补偿均未执行。本报告也未运行已有 IntoResponse 测试；root 提供的 sealed check/clippy/lib test 结果与日志指纹已按 observer=root 单列，不构成真实 MongoDB 验证。

| 原测试 | before→after 位置 | token / ignore |
|---|---|---|
| `readiness_requires_every_external_port` | `backend/apps/web-api/src/app_state.rs:542` → `:542` | 一致 / 非 ignored |
| `version_parser_only_accepts_canonical_positive_decimal_strings` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:342` → `:342` | 一致 / 非 ignored |
| `policy_not_registered_is_internal_error` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:364` → `:364` | 一致 / 非 ignored |
| `conflict_response_includes_stable_code_and_correlation_id` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:373` → `:373` | 一致 / 非 ignored |
| `forbidden_code_does_not_leak_versions` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:390` → `:390` | 一致 / 非 ignored |
| `policy_not_registered_response_is_500` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:405` → `:405` | 一致 / 非 ignored |
| `stock_adjustment_update_hidden_failures_have_identical_http_projection` | `backend/apps/web-api/src/core/handler/approval_instance/error.rs:422` → `:422` | 一致 / 非 ignored |
| `decision_request_denies_forbidden_fields` | `backend/apps/web-api/src/core/handler/approval_instance/http.rs:348` → `:350` | 一致 / 非 ignored |
| `resume_cancel_upgrade_deny_unknown_fields` | `backend/apps/web-api/src/core/handler/approval_instance/http.rs:383` → `:385` | 一致 / 非 ignored |
| `list_query_reuses_service_enums_and_denies_unknown_fields` | `backend/apps/web-api/src/core/handler/approval_instance/http.rs:411` → `:413` | 一致 / 非 ignored |
| `list_cursor_only_encodes_and_decodes_protocol_shape` | `backend/apps/web-api/src/core/handler/approval_instance/http.rs:430` → `:432` | 一致 / 非 ignored |
| `history_cursor_parses_execution_no` | `backend/apps/web-api/src/core/handler/approval_instance/http.rs:498` → `:500` | 一致 / 非 ignored |
| `instance_list_query_extractor_returns_422_for_all_invalid_boundaries` | `backend/apps/web-api/src/core/handler/approval_instance/mod.rs:443` → `:443` | 一致 / 非 ignored |
| `instance_list_query_extractor_keeps_stable_error_envelope` | `backend/apps/web-api/src/core/handler/approval_instance/mod.rs:474` → `:474` | 一致 / 非 ignored |
| `instance_list_query_extractor_accepts_valid_values` | `backend/apps/web-api/src/core/handler/approval_instance/mod.rs:493` → `:493` | 一致 / 非 ignored |
| `decision_injects_actor_and_rejects_empty_reject_reason` | `backend/apps/web-api/src/core/handler/approval_instance/mod.rs:503` → `:503` | 一致 / 非 ignored |
| `resume_injects_path_id_and_actor` | `backend/apps/web-api/src/core/handler/approval_instance/mod.rs:536` → `:536` | 一致 / 非 ignored |
| `recover_alias_payload_is_rejected` | `backend/apps/web-api/src/core/handler/approval_instance/mod.rs:556` → `:556` | 一致 / 非 ignored |
| `supported_image_names_keep_normalized_extension` | `backend/apps/web-api/src/core/handler/file_asset/mod.rs:786` → `:786` | 一致 / 非 ignored |
| `unsupported_or_extensionless_names_stay_unchanged` | `backend/apps/web-api/src/core/handler/file_asset/mod.rs:798` → `:798` | 一致 / 非 ignored |
| `asset_file_rejects_spoofed_pdf_and_accepts_valid_header` | `backend/apps/web-api/src/core/handler/file_asset/mod.rs:807` → `:807` | 一致 / 非 ignored |
| `unknown_commit_outcome_keeps_uploaded_object` | `backend/apps/web-api/src/core/handler/file_asset/mod.rs:824` → `:824` | 一致 / 非 ignored |
| `version_conflict_uses_409_stable_code_and_safe_tombstone` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:464` → `:470` | 一致 / 非 ignored |
| `responsibility_conflict_has_distinct_stable_code` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:482` → `:488` | 一致 / 非 ignored |
| `approval_generic_mutation_maps_to_stable_409` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:498` → `:504` | 一致 / 非 ignored |
| `document_approval_is_personal_approval` | `backend/apps/web-api/src/core/handler/work_item/mod.rs:510` → `:516` | 一致 / 非 ignored |
| `database_change_should_require_restart` | `backend/apps/web-api/src/main.rs:382` → `:384` | 一致 / 非 ignored |
| `jwt_secret_change_should_remain_hot_reloadable` | `backend/apps/web-api/src/main.rs:393` → `:395` | 一致 / 非 ignored |
| `registered_approval_policies_are_exhaustive_at_startup` | `backend/apps/web-api/src/main.rs:399` → `:401` | 一致 / 非 ignored |
| `missing_environment_flag_is_disabled` | `backend/apps/web-api/src/main.rs:404` → `:406` | 一致 / 非 ignored |
| `otel_exporter_requires_endpoint_and_enabled_sdk` | `backend/apps/web-api/src/main.rs:410` → `:412` | 一致 / 非 ignored |
| `storage_change_should_require_restart` | `backend/apps/web-api/src/main.rs:419` → `:421` | 一致 / 非 ignored |

## 10. 源码指纹与证据交付

- 完整 before/after 符号、函数片段 SHA、11 引用与入口清单：`/tmp/cutover17-startup-compensation-review.json`，SHA-256 `df161d401d17d853746144beed447b251233c6c7032ee1d44c2deab361e417b7`。
- 入口与辅助 provider 源快照：`/tmp/cutover17-startup-compensation-sources`；索引阶段 00、阶段 16 及当前源码快照：`/tmp/cutover17-index-sequence-sources`。这些都是只读输入副本，不是新增实现。
- 下表 82 个文件分别标记输入 blob 与 after 的真实 SHA；“—”表示该边不存在（例如旧实现迁删或新入口）。索引附证另记录阶段 00 的实际 blob、常量定义来源及每步 provider。

| 源路径 | before SHA-256 | after SHA-256 |
|---|---|---|
| `backend/apps/cli/src/error.rs` | `b6f0b212bbd005ec63a84494b22fcc5e5fcb795a8daa978752a47e43a3d0b28b` | `46555a54a75ab6e4798e291a5a6805b591edbd2322ac30b332310765c843e1d4` |
| `backend/apps/cli/src/indexes.rs` | — | `66a10320e93980ae8164af7e3fb2a161244580d805d99f8f6ad4657998b8dfe0` |
| `backend/apps/cli/src/main.rs` | `030441ae13cdafde466e51f7c322b3e2d0aa5c9e611938d0d5cd26e3ddbef26c` | `041d059d75c4d884955c6e9fc099a489834ae467a4e0ea23d8c06eb380d78a2a` |
| `backend/apps/cli/src/runtime.rs` | `cbc1f12486836efd164287e6efbb1ab7b5d6825a59e204611d3aa052a733bc86` | `620d3e0735e8cab3799a3d2bd9ebf7c84f5cccb5ff3125a009b033c2c5c62981` |
| `backend/apps/web-api/src/app_state.rs` | `01be54e298ccd5dec552a93fa2d9615a894359a14223e8ac22ed7fa0789d2715` | `deb91d3318253fbad8a587e2bb99ab470ab4548d15db460b289e6b06cab274e1` |
| `backend/apps/web-api/src/core/handler/approval_instance/error.rs` | `c2b6ff741035adede727ea250511e289a03c8c959bfff4a2c8dc41e4e073902d` | `c95b0df27ed96517d8f0146f082cc4012058f8d1d90989526c69e5a85ac7cc8c` |
| `backend/apps/web-api/src/core/handler/approval_instance/http.rs` | `3d7c1fd64c10d9ce820bfad73da5ced4f12343f66221a9606273f38827682441` | `15f372f27132bcfe288cc466f7ff680704ace2934bead2709d395562edf5fdf5` |
| `backend/apps/web-api/src/core/handler/approval_instance/mod.rs` | `efca66f4c8343e55bf1eba6190bc1c2c7e088eb57db0f1c0715ca1854c7dfa10` | `bcbb5c27018ad006f01eea276e0477539f6aa85d4bc2ea38ea8b1090463d7b0c` |
| `backend/apps/web-api/src/core/handler/catalog/mod.rs` | `e7dc1b22402b4964331f4159dba8383cc90d49bfcf7c271247402d04f7b917ea` | `e7dc1b22402b4964331f4159dba8383cc90d49bfcf7c271247402d04f7b917ea` |
| `backend/apps/web-api/src/core/handler/catalog/product.rs` | `d5cc8c86a26a284deda7db110389943d227a5e9ce54af82fb4346e2b76b5b217` | `d5cc8c86a26a284deda7db110389943d227a5e9ce54af82fb4346e2b76b5b217` |
| `backend/apps/web-api/src/core/handler/contract/mod.rs` | `90c61830eb8d925124f16870e177f47745a1be0bc78bef7c3a081ddbb14713b8` | `90c61830eb8d925124f16870e177f47745a1be0bc78bef7c3a081ddbb14713b8` |
| `backend/apps/web-api/src/core/handler/file_asset/mod.rs` | `5878651364cf2bc241a3cd6082eef6e51370b442cdc059d524b3bd913aa571d0` | `fc71136b68507a172915ed0417d49ed13efc52ffb79df20c888c6d53724bcda6` |
| `backend/apps/web-api/src/core/handler/fulfillment/mod.rs` | `a2a3208a3d757812f17ea2cf7660cfa9c8d2ba65b2d9aa54b2649799b66c4ac3` | `bb5b43cbc3b63f5daba2cc5fb162b07e17bb01f0f9daa4c511b1308adf4e793c` |
| `backend/apps/web-api/src/core/handler/payable/mod.rs` | `3a435e5d899a7cc8930384e112e4d96af367b096f5ddcc4e6f00e4846c0a2072` | `3a435e5d899a7cc8930384e112e4d96af367b096f5ddcc4e6f00e4846c0a2072` |
| `backend/apps/web-api/src/core/handler/supplier/mod.rs` | `e761159b582ba4f3cfac98d5b50f502a4b1ca9be0c9633d365e9063cf6609209` | `e761159b582ba4f3cfac98d5b50f502a4b1ca9be0c9633d365e9063cf6609209` |
| `backend/apps/web-api/src/core/handler/work_item/mod.rs` | `c82e6302423b34ac4d662a5840c94822fbf2e067ffd3829a0153504709897584` | `dce88fe14edd55ca58ac0504104146d8d255d8a8ec551b5466a5964ef11391e4` |
| `backend/apps/web-api/src/core/response.rs` | `877356c5e95e6a3cf07e0ba2dc66caa354cdfbaca7df1a902a4e07dbce082bc1` | `877356c5e95e6a3cf07e0ba2dc66caa354cdfbaca7df1a902a4e07dbce082bc1` |
| `backend/apps/web-api/src/indexes.rs` | — | `66a10320e93980ae8164af7e3fb2a161244580d805d99f8f6ad4657998b8dfe0` |
| `backend/apps/web-api/src/main.rs` | `d14bd4725b387a92cb506059479501be17e0416e5afa005dc76f8b157d6cc4ed` | `ebad5d1b62ab1c45ca1c8bcb1ad1163e66f3dcb185594fda33df13d1140c3a20` |
| `backend/crates/erp-audit/src/indexes.rs` | `5383551e5e9a1aac01578e5c982b2257a75b282f65d961b62b9b0a254c93d67b` | `5383551e5e9a1aac01578e5c982b2257a75b282f65d961b62b9b0a254c93d67b` |
| `backend/crates/erp-catalog/src/indexes/catalog.rs` | `941879f526338dd3de2bf059d63a31464a88b2cb91588a757beeeed68013872c` | `941879f526338dd3de2bf059d63a31464a88b2cb91588a757beeeed68013872c` |
| `backend/crates/erp-catalog/src/indexes/mod.rs` | `8031be644486ae57fcde79e12cb81113fd4bf496ad75ce2d6414980cbd04bec6` | `8031be644486ae57fcde79e12cb81113fd4bf496ad75ce2d6414980cbd04bec6` |
| `backend/crates/erp-contract/src/indexes/contract.rs` | `8a43a5c99f0eea50527bd7533718292024cbc294660499c147afae4214a1bd3e` | `8a43a5c99f0eea50527bd7533718292024cbc294660499c147afae4214a1bd3e` |
| `backend/crates/erp-contract/src/indexes/mod.rs` | `dd2eac95653bf7005989b47d02c00250b19a63de100914a4c909b792fa05b484` | `dd2eac95653bf7005989b47d02c00250b19a63de100914a4c909b792fa05b484` |
| `backend/crates/erp-customer/src/indexes/customer.rs` | `8ad0c6f7bec860ed46d85c4043748d5d907804bec280a569ecef3061a8fd0d6c` | `8ad0c6f7bec860ed46d85c4043748d5d907804bec280a569ecef3061a8fd0d6c` |
| `backend/crates/erp-customer/src/indexes/mod.rs` | `7527fe50427766d8f705e5a82a3b289c5de4414ad2e61ccb58da941c6dcb220e` | `7527fe50427766d8f705e5a82a3b289c5de4414ad2e61ccb58da941c6dcb220e` |
| `backend/crates/erp-finance/src/indexes/cost.rs` | `4fbc5c02ec23be476a5655b866f9f7e5f1c6c72441946fcfca8797fce85cd468` | `4095449e533088185776459dbfab704a9f539b1b45584311543635774316fbfe` |
| `backend/crates/erp-finance/src/indexes/mod.rs` | `b725e65129222793f07c83d91bddb9dd91a5f8d5bd5d988d14512dda1358364d` | `5c842aed976201015689d3e909c7205fed6f950c4ee51d8bf648f6e0a4d42ba7` |
| `backend/crates/erp-finance/src/indexes/payable.rs` | `d371e8b914f609aba970776d175792d4a43747cb0972758e676e4dc32948637a` | `c15e34c466089ee02ac9a5d606fed9a87c48303b6469eb37fcf7b052e145483c` |
| `backend/crates/erp-finance/src/indexes/receivable.rs` | `f820de8d681cb15189a14de659c6651c808011ecf7a6b0ead4da40c748d617c6` | `87c5cc4420d1b1b7101255cf93798afe393b7083d9016e732a3310a254492f6c` |
| `backend/crates/erp-fulfillment/src/indexes/fulfillment.rs` | `72456f6adb4e0e0e70915847fba9d973388342b57262308ddbc7ae307ffe8379` | `72456f6adb4e0e0e70915847fba9d973388342b57262308ddbc7ae307ffe8379` |
| `backend/crates/erp-fulfillment/src/indexes/mod.rs` | `a7f02fc863c9d10460666911a5d6b3f2170542f8c752457b98b3bc41f05502a7` | `a7f02fc863c9d10460666911a5d6b3f2170542f8c752457b98b3bc41f05502a7` |
| `backend/crates/erp-identity/src/indexes.rs` | `ce1c4c305e9754de76a54f723ce5eec5974841f873ed7ca4b0ae05ce7098d331` | `dd1b373f1818c11662fdb89da22b076fc1d8fc017fc1ac225589b8aa1693c9c5` |
| `backend/crates/erp-import/src/indexes/legacy_import.rs` | `6763db3aee23ff6c428af4bb702ed77135dd27624299d4297653773c57e348f8` | `6763db3aee23ff6c428af4bb702ed77135dd27624299d4297653773c57e348f8` |
| `backend/crates/erp-import/src/indexes/mod.rs` | `b15f88cdf58723a0f1ed70801842d7282e6b9d008ea39d480ba8227696c6b5ec` | `b15f88cdf58723a0f1ed70801842d7282e6b9d008ea39d480ba8227696c6b5ec` |
| `backend/crates/erp-integration/src/indexes/integration_ops.rs` | `f89ed3ff401eeec32331d3dc2ccbd1c5fc7492c2c109a83582d5d842f61474b2` | `f89ed3ff401eeec32331d3dc2ccbd1c5fc7492c2c109a83582d5d842f61474b2` |
| `backend/crates/erp-integration/src/indexes/mod.rs` | `55c57830d68374a7cf5d42a41f8601ec59169bf54b92f4be4738b0c39e4ef103` | `55c57830d68374a7cf5d42a41f8601ec59169bf54b92f4be4738b0c39e4ef103` |
| `backend/crates/erp-inventory/src/indexes/inventory.rs` | `e8a34573b71a03b63c5ce410a368599417ce6297929a846803daaadec94703be` | `e8a34573b71a03b63c5ce410a368599417ce6297929a846803daaadec94703be` |
| `backend/crates/erp-inventory/src/indexes/mod.rs` | `2dfb528099d89c7330d8630d807ae35f75d830035268e96905f02ef7de98903c` | `2dfb528099d89c7330d8630d807ae35f75d830035268e96905f02ef7de98903c` |
| `backend/crates/erp-party/src/indexes/mod.rs` | `1ebb3244dbb31ebc8236fe06846200d36803367d73e1d945f654074d82c4df5b` | `1ebb3244dbb31ebc8236fe06846200d36803367d73e1d945f654074d82c4df5b` |
| `backend/crates/erp-party/src/indexes/party.rs` | `0f5c824ecab98f3b8a48ca6e0d1337bbfc70e8ea4db7b86b3740fe0baeeea4e5` | `0f5c824ecab98f3b8a48ca6e0d1337bbfc70e8ea4db7b86b3740fe0baeeea4e5` |
| `backend/crates/erp-processes/src/adapters/identity.rs` | — | `a275adc12d3895c71eee5d9072c4419b45ecd306e36a85ea1c304c83d40b5af7` |
| `backend/crates/erp-processes/src/adapters/support_audit.rs` | — | `51aae492e7bcccbf73ea39c192122e4089c3f0fe6b6d1f347ea71ee75ee153de` |
| `backend/crates/erp-processes/src/adapters/support_documents.rs` | — | `c68f9b952ac623e29727f06378ffd7f871708e05eb19828f20570aa4bd1c36fc` |
| `backend/crates/erp-processes/src/adapters/workflow/authorization.rs` | — | `2a26535fbb1403152944d0a951ab2588ca084747ab7eeb0eef6bc02dafbd3cdc` |
| `backend/crates/erp-processes/src/adapters/workflow/mod.rs` | `d3d303e7fd6fa4b892178e00fe8aba03a0788f4ebb04e72933af1e2b364e8863` | `d75552bdd8f06027703f2c4342ed14bc9ede710547e9929b4baf997305b06b09` |
| `backend/crates/erp-processes/src/errors.rs` | — | `5e83a431a3fb40660a22e32821e4ba85ae4996dd890ec06316655b3f436608bc` |
| `backend/crates/erp-processes/src/test_indexes.rs` | — | `66a10320e93980ae8164af7e3fb2a161244580d805d99f8f6ad4657998b8dfe0` |
| `backend/crates/erp-procurement/src/indexes/mod.rs` | `bdf79a885376c477b42281bc7b7f128ff64c34095d5c87f78fa25c4165a33864` | `bdf79a885376c477b42281bc7b7f128ff64c34095d5c87f78fa25c4165a33864` |
| `backend/crates/erp-procurement/src/indexes/procurement_responsibility.rs` | `32c8870e8069afffed9c8005b36fd2ed0358dcba430328c49e713f09dc8d71bf` | `32c8870e8069afffed9c8005b36fd2ed0358dcba430328c49e713f09dc8d71bf` |
| `backend/crates/erp-procurement/src/indexes/purchase_order.rs` | `6c9f3ee4bd79d3e6881a1a83408e6796259359e0e11f49de0ee1d402c0e30650` | `6c9f3ee4bd79d3e6881a1a83408e6796259359e0e11f49de0ee1d402c0e30650` |
| `backend/crates/erp-read-models/src/test_indexes.rs` | — | `66a10320e93980ae8164af7e3fb2a161244580d805d99f8f6ad4657998b8dfe0` |
| `backend/crates/erp-returns/src/indexes/mod.rs` | `bc0b42777b02354e1db022f2b48bf03752d577af7035b92f78448c483aeabfbf` | `bc0b42777b02354e1db022f2b48bf03752d577af7035b92f78448c483aeabfbf` |
| `backend/crates/erp-returns/src/indexes/returns.rs` | `9cf53d1cb99da2b5e4da88fbc67c7ab8d778d6bf64ab17ce77db1638d3d6ae15` | `9cf53d1cb99da2b5e4da88fbc67c7ab8d778d6bf64ab17ce77db1638d3d6ae15` |
| `backend/crates/erp-sales/src/indexes/mod.rs` | `92658a86aaafc14709383b1f69ee00678320bf430b047848d75fcda4e9c1b96d` | `92658a86aaafc14709383b1f69ee00678320bf430b047848d75fcda4e9c1b96d` |
| `backend/crates/erp-sales/src/indexes/sales_order.rs` | `db16be5e6c17f152dba2f6c16255e64843b8a5a1ef04250047e56f5d50564fc2` | `db16be5e6c17f152dba2f6c16255e64843b8a5a1ef04250047e56f5d50564fc2` |
| `backend/crates/erp-sales/src/indexes/sales_review.rs` | `aa582fa885344a16bef845adb8e6475a12f8fb73160a294971ac0c4964be4ebe` | `aa582fa885344a16bef845adb8e6475a12f8fb73160a294971ac0c4964be4ebe` |
| `backend/crates/erp-supplier/src/indexes/mod.rs` | `426cb93bbe16791e29f99f55c1ddb977467393db5966763545f599e399f6f6b5` | `426cb93bbe16791e29f99f55c1ddb977467393db5966763545f599e399f6f6b5` |
| `backend/crates/erp-supplier/src/indexes/supplier.rs` | `35d732d0e017f8eb8565563ca5be284335417ffa21db4561e73ba4aa44810495` | `35d732d0e017f8eb8565563ca5be284335417ffa21db4561e73ba4aa44810495` |
| `backend/crates/erp-supply/src/indexes/mod.rs` | `090add2baa843987f300824d5f70fa581ec57ca693b427384f714e85f032e30c` | `090add2baa843987f300824d5f70fa581ec57ca693b427384f714e85f032e30c` |
| `backend/crates/erp-supply/src/indexes/supplier_api.rs` | `445e5f4114e2f5a740b614e2f07b88def312eec985802c765d720b3aec049b8b` | `445e5f4114e2f5a740b614e2f07b88def312eec985802c765d720b3aec049b8b` |
| `backend/crates/erp-supply/src/indexes/supplier_fulfillment.rs` | `4a1b5c5ea0a266e6f21c0aa158d0d9af8be7330becafb05da264aeb3ccbf6a5c` | `4a1b5c5ea0a266e6f21c0aa158d0d9af8be7330becafb05da264aeb3ccbf6a5c` |
| `backend/crates/erp-supply/src/indexes/supplier_offering.rs` | `9d1d89fda5ca475d22a64e1ec498dcffb5ea5e6f4e65a3b6b359e0ec749d06f3` | `9d1d89fda5ca475d22a64e1ec498dcffb5ea5e6f4e65a3b6b359e0ec749d06f3` |
| `backend/crates/erp-supply/src/indexes/supplier_settlement.rs` | `75c5168994c784e3a716540360f5608b008dfa1ba62a500717e074f5b7dbde17` | `75c5168994c784e3a716540360f5608b008dfa1ba62a500717e074f5b7dbde17` |
| `backend/crates/erp-support/src/indexes/bulk_job.rs` | `80905fd70f27ba2fe07a657f7d2dc473277cc37085011361f960d97f5e21a52a` | `e9a1f955061b60d0f8e4fa3eb17d3a008a8727bf79b7c559cce1b700f460af81` |
| `backend/crates/erp-support/src/indexes/file_asset.rs` | `5077f14b129cd02789ef195a0ee9d80321018f4ad0e0ce698c20f2841af9650f` | `7d6d2cca1c844e0e2af829661792f4b02391b9b2f931ae4a40f3e3ef0d8f6ccb` |
| `backend/crates/erp-support/src/indexes/mod.rs` | `401cbe2d8bfc0b36d08950bdbaf352179d1cbaf53dbf26c0dbfa93ed53f5371a` | `9b97b55f8cdd36e352f8fb70015f1012ab8928ef113e5d65ae501d127189b7bb` |
| `backend/crates/erp-support/src/indexes/source_registry.rs` | `18f2db99340ee9ebcfe02632d6334268e5ca5bc7c123f34b76214293522cf0ca` | `dd01be1bb3c095149bff1877b3547083622ed0503b7efa859e2ed8ac1eb12f94` |
| `backend/crates/erp-warehouse/src/indexes/mod.rs` | `a44b65a3b40e7686d2e6b6cb8f3bfd2b226913a2e5002ec2c3b41f9290c82d7c` | `a44b65a3b40e7686d2e6b6cb8f3bfd2b226913a2e5002ec2c3b41f9290c82d7c` |
| `backend/crates/erp-warehouse/src/indexes/warehouse.rs` | `8c80ebfdbd87f9fe27cb3d606fb3a8bed73e2472aa7045aeb506abbe5b370b28` | `8c80ebfdbd87f9fe27cb3d606fb3a8bed73e2472aa7045aeb506abbe5b370b28` |
| `backend/crates/erp-workflow/src/error.rs` | `63fe39952aca61e1b56d288cc4522da37e36e7c68bde54a4c6f240548745ac83` | `4ee5b991a9fcc41a7727b1d47c65469c6d3417a3faace7d15bf3eda4ffc200bf` |
| `backend/crates/erp-workflow/src/indexes/approval_integration.rs` | `a06bdeb03d0c0a5e8fe6eb7365358fac4951fa3c81500fc059b0d6993e0c443f` | `a06bdeb03d0c0a5e8fe6eb7365358fac4951fa3c81500fc059b0d6993e0c443f` |
| `backend/crates/erp-workflow/src/indexes/bpm.rs` | `ab222dce574ad9f6f6a090e765a3414255d93fa82754df67cd132d5e3b01a485` | `ab222dce574ad9f6f6a090e765a3414255d93fa82754df67cd132d5e3b01a485` |
| `backend/crates/erp-workflow/src/indexes/document_registry.rs` | `280df3ca4f93fbce40265aea4d79a14cad6763c4e5be97f8ee84504868d3f036` | `280df3ca4f93fbce40265aea4d79a14cad6763c4e5be97f8ee84504868d3f036` |
| `backend/crates/erp-workflow/src/indexes/mod.rs` | `1e502ee1f1f051c59dc364d7cde152962a4686249620a8e7e0c46d56c2ca6e13` | `85ac0298c9f1d7e18528009d7beaa1d42e0136347bcfc3fde3d7e1bdd85cdfeb` |
| `backend/crates/erp-workflow/src/indexes/work_item.rs` | `defee8bf4f6c512bf5558c1b5499ef87a5115ca01d23f1aba9021219fd7189c7` | `defee8bf4f6c512bf5558c1b5499ef87a5115ca01d23f1aba9021219fd7189c7` |
| `backend/database/src/indexes/mod.rs` | `cf8a022673baacf0411e2484fd571b9ec04f83f763a910826b1a8b72f8358a0f` | — |
| `backend/services/src/errors.rs` | `7666307ce6617948104ad9683c09beabe861268caaf98bb34a60269f26db7e82` | — |
| `backend/services/src/identity_compose.rs` | `6017bcceb790186370c27d41c4a387d0f2852dbade3c0754193aa844e7012e70` | — |
| `backend/services/src/support_audit.rs` | `dbf3700f91d6ca51668d5db89a4c3bf6918b507627964a79fc3860d389c386fd` | — |
| `backend/services/src/support_documents.rs` | `c68f9b952ac623e29727f06378ffd7f871708e05eb19828f20570aa4bd1c36fc` | — |
| `backend/services/src/workflow_compose.rs` | `e88d2a5a93165fe244f23859f2e13b332056b3ea058186eda982680911cd9e3a` | — |

## 11. 冻结源码绑定与 root 动态观察

- after 固定 `39c55021d8bb1d030eade4afb3e135e4b5a8a18b`；before 仍为阶段 16 输入 `a537414eb8f78c43ebc383a3457a45c437dececc`，索引顺序权威仍为阶段 00 `400ab4f7855255b284fe8a8e1caffe27acc96083`。
- 仅使用既有 82 条主清单中的 76 个实际 after 文件、索引已登记的 52 个 after 路径及既有常量 provider，去重共 107 个源码路径。通过 git cat-file batch 逐个读取真实 commit blob，均满足旧清单 SHA = commit blob SHA = 当前文件 SHA；0 漂移，无需重算 token，也未新扫描源码。
- 每路径的 Git blob OID、SHA 与 verified 记录见同名 JSON、索引 JSON 及 `/tmp/cutover17-startup-final-blob-binding.json`（SHA-256 `26e3e594953f1578b1a8421f529818a904f8895f00c25a785072eee809301790`）。已报告 ready 并停止所有源码 I/O；本轮后续只更新 `/tmp` 元数据和引用。
- 动态结果观察者为 root：fmt/check/clippy 通过；lib 为 3655 passed、0 failed、68 ignored；HTTP 黄金测试 10 项通过，覆盖 494 个实际响应 case。该结果由 root 提供，本 worker 只读取 sealed 日志 SHA，未执行或独立解析测试输出。真实 MongoDB 未验证。

| root sealed 日志 | SHA-256 | observer |
|---|---|---|
| `/tmp/erp-cutover17-lib-tests-sealed.log` | `2fb05cc9803e1b527afb67067c28f606d2d785c58965ea75a9e89cd30fa54753` | root |
| `/tmp/erp-cutover17-clippy-sealed.log` | `afe2e2111e7e30fdd3ecf2a38664b90e9f5b2b23e63940cb0020a2fca5cd7481` | root |
| `/tmp/erp-cutover17-check-sealed.log` | `90380e0beeef1e7c0b8674e4011583d27231dd2726b97657a713241706df7266` | root |

- 已最终绑定的 F 索引独立工件：`/tmp/cutover17-index-order-review.json`，SHA-256 `9f787cd29b0d21b8ca2c4c6447cb1fc41219479423cc990d9cafdc678261f632`；本报告只刷新引用，不重新运行其解析。
- 主 JSON 最终 SHA-256：`df161d401d17d853746144beed447b251233c6c7032ee1d44c2deab361e417b7`；索引 JSON 最终 SHA-256：`ce175486c31602a0ae1473db1c320304b6ea7884185f0f3f8361cb7c9dd69150`。
