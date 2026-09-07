# 阶段 17 parser / schema / index 审核合同

## 1. 证据与分工

候选 source commit：`39c55021d8bb1d030eade4afb3e135e4b5a8a18b`。本报告仅确认当前真实源码和静态捕获，不执行 Cargo、Rust serde、MongoDB 或业务运行。错误、authority、事务、幂等与金额语义必须使用各自独立审核。

| 比较 | raw exit | changed | missing | added | needs_review |
| --- | ---: | ---: | ---: | ---: | ---: |
| step | 1 | 1 | 2 | 0 | 48 |
| cumulative | 1 | 10 | 24 | 14 | 65 |

raw 原项、顺序、计数、状态与退出码必须保留；语义核销不得回写 scanner、allowlist 或原捕获。JSON 每个条目保存原始索引及对应源码证据指针。

## 2. 索引注册与定义

00 全局 30 组按真实 ensure / reexport 展开为 158 次集合 create_indexes 与 3 次 inventory / purchase_order / work_item reconcile。阶段 16 聚合顺序继承偏移；17 在 web、CLI、process test、read-model test 四根恢复 00 顺序。本次独立递归复核四根 161 操作逐位置相同；279 组原子构造及辅助函数去注释 token 相同。该顺序修复是真实行为修复，必须与 parser 限制分开登记。

完整顺序、原子提供方、键构造、reconcile 及每条源码 SHA 见 `/tmp/cutover17-index-order-review.json`。工作捕获早于修正时不得宣称其指纹已包含修正；须用 fresh sealed capture 绑定最终提交。

累计 missing 20 个 access-control 索引仍在 identity/audit 的 src/indexes.rs。scanner 要求路径分段包含 indexes，故未抓到 indexes.rs。accounts/roles 常量也因该门槛遗漏。casbin_rules/casbin_policy_state 原已存在；移入 repository/ 才被捕获。policy 仅为 Casbin policy-state 文档 _id，禁止当作新 collection。

采购责任规则 3 个、work_items 9 个和累计 Casbin 2 个限制项均按真实 trait 常量、构造函数、键顺序、unique Option、partial 和注册调用核销；named_index 的 unique 为 None，不能写成 Some(false)。

## 3. 累计 DTO 差异约束

| 符号 | 必须采用的处置 |
| --- | --- |
| DispatchOutcome / SupplierFailureClass | 保留实际类型归属变化与新增 raw。Failed.error_class 的八个 snake_case 值与原 integration ErrorClass 相同。 |
| SupplierDetailView / Party*Fact | 对照原 Party*View 与实体状态 enum；字段、空值、masked 字段与显式状态映射保持。不得仅按相同字段名推断。 |
| ImportExecutionResult / ImportJobStatus | 六个后台状态同名 snake_case 输出；新事实只提供 Serialize，原消费者为响应视图。 |
| ImportBusinessConfirmationWorkItemView | WorkItemStatus 三值输出保持；最终 Process 复合 DTO 直接保留 workflow 的 WorkItemType / WorkItemStatus，实际 item 值原样映射；17 修复 16 的单值窄化，step 差异须保留。 |
| WorkItemConflict / HttpWorkItemConflict | workflow 内部冲突只保留跳过序列化的 kind/id；HTTP DTO 输出 current_work_item，包括 None 时的显式 null。 |
| WorkItemFields / WorkItemMutationOutcome | 原、新类型均非 Serialize；区分内部字段拆分与真正 WorkItemView 输出。不得把原始类型变化称为 wire 字段移除。 |
| WorkItemHttpView | flatten 内部改为由真实 WorkItemView 转出的 JSON Value；WorkItemView 的实际 serde 属性保持。命令后重查询时点、失败与权限另审。 |
| SalesBusinessTypeFact | 原 sales BusinessType 提供 Voucher/GoodsService，显式映射保留 VOUCHER/GOODS_SERVICE。 |

## 4. raw 逐项处置

### step

| raw 列表/序号 | 类别 | 符号或提示 | 处置 |
| --- | --- | --- | --- |
| changed/0 | dto | `ImportBusinessConfirmationWorkItemView` | intentional_correction_of_inherited_import_enum_boundary；step_schema_differences.ImportBusinessConfirmationWorkItemView |
| missing/0 | dto | `ImportWorkItemStatus` | intentional_correction_of_inherited_import_enum_boundary；step_schema_differences.ImportWorkItemStatus |
| missing/1 | dto | `ImportWorkItemType` | intentional_correction_of_inherited_import_enum_boundary；step_schema_differences.ImportWorkItemType |
| needs_review/0 | dto | `UserID` | tuple_newtype_current_definition_verified；tuple_definitions.step.UserID |
| needs_review/1 | dto | `Account` | tuple_newtype_current_definition_verified；tuple_definitions.step.Account |
| needs_review/2 | dto | `CommandFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.step.CommandFingerprint |
| needs_review/3 | dto | `IdempotencyKey` | tuple_newtype_current_definition_verified；tuple_definitions.step.IdempotencyKey |
| needs_review/4 | dto | `CommandScope` | tuple_newtype_current_definition_verified；tuple_definitions.step.CommandScope |
| needs_review/5 | dto | `CommandDigest` | tuple_newtype_current_definition_verified；tuple_definitions.step.CommandDigest |
| needs_review/6 | dto | `ParticipantId` | tuple_newtype_current_definition_verified；tuple_definitions.step.ParticipantId |
| needs_review/7 | dto | `ElectronicRecipientFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.step.ElectronicRecipientFingerprint |
| needs_review/8 | dto | `ServiceRecipientFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.step.ServiceRecipientFingerprint |
| needs_review/9 | dto | `ServiceLocationFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.step.ServiceLocationFingerprint |
| needs_review/10 | indexes | `idx_procurement_responsibility_list` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_procurement_responsibility_list |
| needs_review/11 | indexes | `idx_procurement_responsibility_owner` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_procurement_responsibility_owner |
| needs_review/12 | indexes | `uk_procurement_responsibility_active_selector` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_procurement_responsibility_active_selector |
| needs_review/13 | dto | `ContentHmac` | tuple_newtype_current_definition_verified；tuple_definitions.step.ContentHmac |
| needs_review/14 | indexes | `idx_work_items_document_approval_owner_page` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_work_items_document_approval_owner_page |
| needs_review/15 | indexes | `idx_work_items_document_approval_owner_type_page` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_work_items_document_approval_owner_type_page |
| needs_review/16 | indexes | `idx_work_items_fulfillment_queue` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_work_items_fulfillment_queue |
| needs_review/17 | indexes | `uk_work_items_open_sales_invoice_execution_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_sales_invoice_execution_object |
| needs_review/18 | indexes | `uk_work_items_open_object_type` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_object_type |
| needs_review/19 | indexes | `uk_work_items_open_fulfillment_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_fulfillment_object |
| needs_review/20 | indexes | `uk_work_items_open_customer_acceptance_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_customer_acceptance_object |
| needs_review/21 | indexes | `uk_work_items_open_payment_execution_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_payment_execution_object |
| needs_review/22 | indexes | `uk_work_items_approval_execution` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_approval_execution |
| needs_review/23 | dto | `UserID` | tuple_newtype_current_definition_verified；tuple_definitions.step.UserID |
| needs_review/24 | dto | `Account` | tuple_newtype_current_definition_verified；tuple_definitions.step.Account |
| needs_review/25 | dto | `CommandFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.step.CommandFingerprint |
| needs_review/26 | dto | `IdempotencyKey` | tuple_newtype_current_definition_verified；tuple_definitions.step.IdempotencyKey |
| needs_review/27 | dto | `CommandScope` | tuple_newtype_current_definition_verified；tuple_definitions.step.CommandScope |
| needs_review/28 | dto | `CommandDigest` | tuple_newtype_current_definition_verified；tuple_definitions.step.CommandDigest |
| needs_review/29 | dto | `ParticipantId` | tuple_newtype_current_definition_verified；tuple_definitions.step.ParticipantId |
| needs_review/30 | dto | `ElectronicRecipientFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.step.ElectronicRecipientFingerprint |
| needs_review/31 | dto | `ServiceRecipientFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.step.ServiceRecipientFingerprint |
| needs_review/32 | dto | `ServiceLocationFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.step.ServiceLocationFingerprint |
| needs_review/33 | indexes | `idx_procurement_responsibility_list` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_procurement_responsibility_list |
| needs_review/34 | indexes | `idx_procurement_responsibility_owner` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_procurement_responsibility_owner |
| needs_review/35 | indexes | `uk_procurement_responsibility_active_selector` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_procurement_responsibility_active_selector |
| needs_review/36 | dto | `ContentHmac` | tuple_newtype_current_definition_verified；tuple_definitions.step.ContentHmac |
| needs_review/37 | indexes | `idx_work_items_document_approval_owner_page` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_work_items_document_approval_owner_page |
| needs_review/38 | indexes | `idx_work_items_document_approval_owner_type_page` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_work_items_document_approval_owner_type_page |
| needs_review/39 | indexes | `idx_work_items_fulfillment_queue` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_work_items_fulfillment_queue |
| needs_review/40 | indexes | `uk_work_items_open_sales_invoice_execution_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_sales_invoice_execution_object |
| needs_review/41 | indexes | `uk_work_items_open_object_type` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_object_type |
| needs_review/42 | indexes | `uk_work_items_open_fulfillment_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_fulfillment_object |
| needs_review/43 | indexes | `uk_work_items_open_customer_acceptance_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_customer_acceptance_object |
| needs_review/44 | indexes | `uk_work_items_open_payment_execution_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_payment_execution_object |
| needs_review/45 | indexes | `uk_work_items_approval_execution` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_approval_execution |
| needs_review/46 | foundations | `idempotency source or symbol hashes changed` | out_of_scope_preserved；独立 owner 审核 |
| needs_review/47 | foundations | `transaction source or symbol hashes changed` | out_of_scope_preserved；独立 owner 审核 |

### cumulative

| raw 列表/序号 | 类别 | 符号或提示 | 处置 |
| --- | --- | --- | --- |
| changed/0 | dto | `DispatchOutcome` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.DispatchOutcome |
| changed/1 | dto | `ImportExecutionResult` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.ImportExecutionResult |
| changed/2 | dto | `SupplierDetailView` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.SupplierDetailView |
| changed/3 | dto | `WorkItemConflict` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.WorkItemConflict |
| changed/4 | dto | `WorkItemFields` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.WorkItemFields |
| changed/5 | dto | `WorkItemHttpView` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.WorkItemHttpView |
| changed/6 | dto | `WorkItemMutationOutcome` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.WorkItemMutationOutcome |
| changed/7 | errors | `Error::class` | out_of_scope_preserved；独立 owner 审核 |
| changed/8 | errors | `ErrorClass::as_str` | out_of_scope_preserved；独立 owner 审核 |
| changed/9 | errors | `ErrorCode::class` | out_of_scope_preserved；独立 owner 审核 |
| missing/0 | collections | `accounts` | existing_collection_hidden_by_indexes_rs_path_predicate；index_review.missing_collection_resolution.accounts |
| missing/1 | collections | `roles` | existing_collection_hidden_by_indexes_rs_path_predicate；index_review.missing_collection_resolution.roles |
| missing/2 | indexes | `idx_accounts_kind_active_created` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.idx_accounts_kind_active_created |
| missing/3 | indexes | `idx_audit_events_actor_created` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.idx_audit_events_actor_created |
| missing/4 | indexes | `idx_audit_events_created` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.idx_audit_events_created |
| missing/5 | indexes | `idx_audit_events_object_created` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.idx_audit_events_object_created |
| missing/6 | indexes | `idx_audit_events_request_id` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.idx_audit_events_request_id |
| missing/7 | indexes | `idx_audit_logs_active_created` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.idx_audit_logs_active_created |
| missing/8 | indexes | `idx_casbin_ptype_value0` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.idx_casbin_ptype_value0 |
| missing/9 | indexes | `idx_casbin_ptype_value1` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.idx_casbin_ptype_value1 |
| missing/10 | indexes | `idx_data_scopes_scope_type` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.idx_data_scopes_scope_type |
| missing/11 | indexes | `idx_permissions_disabled` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.idx_permissions_disabled |
| missing/12 | indexes | `idx_roles_active_enabled` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.idx_roles_active_enabled |
| missing/13 | indexes | `idx_user_roles_user_effective` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.idx_user_roles_user_effective |
| missing/14 | indexes | `uk_accounts_account` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.uk_accounts_account |
| missing/15 | indexes | `uk_accounts_id` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.uk_accounts_id |
| missing/16 | indexes | `uk_audit_events_id` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.uk_audit_events_id |
| missing/17 | indexes | `uk_audit_logs_id` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.uk_audit_logs_id |
| missing/18 | indexes | `uk_data_scopes_subject_scope` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.uk_data_scopes_subject_scope |
| missing/19 | indexes | `uk_permissions_resource_action` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.uk_permissions_resource_action |
| missing/20 | indexes | `uk_roles_id` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.uk_roles_id |
| missing/21 | indexes | `uk_user_roles_active` | existing_index_hidden_by_indexes_rs_path_predicate；index_review.missing20_real_provider_resolution.uk_user_roles_active |
| missing/22 | errors | `ErrorClass::class` | out_of_scope_preserved；独立 owner 审核 |
| missing/23 | errors | `ErrorClass::retryable` | out_of_scope_preserved；独立 owner 审核 |
| added/0 | collections | `casbin_policy_state` | existing_collection_new_capture_visibility；index_review.collection_resolution |
| added/1 | collections | `casbin_rules` | existing_collection_new_capture_visibility；index_review.collection_resolution |
| added/2 | collections | `policy` | document_id_false_positive_not_collection；index_review.collection_resolution |
| added/3 | dto | `AddressTypeFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.AddressTypeFact |
| added/4 | dto | `EffectiveRecordStatusFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.EffectiveRecordStatusFact |
| added/5 | dto | `HttpWorkItemConflict` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.HttpWorkItemConflict |
| added/6 | dto | `ImportJobStatus` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.ImportJobStatus |
| added/7 | dto | `PartyAddressFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.PartyAddressFact |
| added/8 | dto | `PartyBankAccountFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.PartyBankAccountFact |
| added/9 | dto | `PartyContactFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.PartyContactFact |
| added/10 | dto | `PartyStatusFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.PartyStatusFact |
| added/11 | dto | `PartyTaxProfileFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.PartyTaxProfileFact |
| added/12 | dto | `SalesBusinessTypeFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.SalesBusinessTypeFact |
| added/13 | dto | `SupplierFailureClass` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.SupplierFailureClass |
| needs_review/0 | dto | `UserID` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.UserID |
| needs_review/1 | dto | `Account` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.Account |
| needs_review/2 | dto | `IdempotencyKey` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.IdempotencyKey |
| needs_review/3 | dto | `CommandScope` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.CommandScope |
| needs_review/4 | dto | `CommandDigest` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.CommandDigest |
| needs_review/5 | dto | `ParticipantId` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.ParticipantId |
| needs_review/6 | indexes | `idx_casbin_ptype_value0` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_casbin_ptype_value0 |
| needs_review/7 | indexes | `idx_casbin_ptype_value1` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_casbin_ptype_value1 |
| needs_review/8 | indexes | `idx_procurement_responsibility_list` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_procurement_responsibility_list |
| needs_review/9 | indexes | `idx_procurement_responsibility_owner` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_procurement_responsibility_owner |
| needs_review/10 | indexes | `uk_procurement_responsibility_active_selector` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_procurement_responsibility_active_selector |
| needs_review/11 | indexes | `idx_work_items_document_approval_owner_page` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_work_items_document_approval_owner_page |
| needs_review/12 | indexes | `idx_work_items_document_approval_owner_type_page` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_work_items_document_approval_owner_type_page |
| needs_review/13 | indexes | `idx_work_items_fulfillment_queue` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_work_items_fulfillment_queue |
| needs_review/14 | indexes | `uk_work_items_open_sales_invoice_execution_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_sales_invoice_execution_object |
| needs_review/15 | indexes | `uk_work_items_open_object_type` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_object_type |
| needs_review/16 | indexes | `uk_work_items_open_fulfillment_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_fulfillment_object |
| needs_review/17 | indexes | `uk_work_items_open_customer_acceptance_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_customer_acceptance_object |
| needs_review/18 | indexes | `uk_work_items_open_payment_execution_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_payment_execution_object |
| needs_review/19 | indexes | `uk_work_items_approval_execution` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_approval_execution |
| needs_review/20 | dto | `CommandFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.CommandFingerprint |
| needs_review/21 | dto | `ContentHmac` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.ContentHmac |
| needs_review/22 | dto | `ElectronicRecipientFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.ElectronicRecipientFingerprint |
| needs_review/23 | dto | `ServiceRecipientFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.ServiceRecipientFingerprint |
| needs_review/24 | dto | `ServiceLocationFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.ServiceLocationFingerprint |
| needs_review/25 | dto | `UserID` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.UserID |
| needs_review/26 | dto | `Account` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.Account |
| needs_review/27 | dto | `CommandFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.CommandFingerprint |
| needs_review/28 | dto | `IdempotencyKey` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.IdempotencyKey |
| needs_review/29 | dto | `CommandScope` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.CommandScope |
| needs_review/30 | dto | `CommandDigest` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.CommandDigest |
| needs_review/31 | dto | `ParticipantId` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.ParticipantId |
| needs_review/32 | dto | `ElectronicRecipientFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.ElectronicRecipientFingerprint |
| needs_review/33 | dto | `ServiceRecipientFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.ServiceRecipientFingerprint |
| needs_review/34 | dto | `ServiceLocationFingerprint` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.ServiceLocationFingerprint |
| needs_review/35 | indexes | `idx_procurement_responsibility_list` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_procurement_responsibility_list |
| needs_review/36 | indexes | `idx_procurement_responsibility_owner` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_procurement_responsibility_owner |
| needs_review/37 | indexes | `uk_procurement_responsibility_active_selector` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_procurement_responsibility_active_selector |
| needs_review/38 | dto | `ContentHmac` | tuple_newtype_current_definition_verified；tuple_definitions.cumulative.ContentHmac |
| needs_review/39 | indexes | `idx_work_items_document_approval_owner_page` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_work_items_document_approval_owner_page |
| needs_review/40 | indexes | `idx_work_items_document_approval_owner_type_page` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_work_items_document_approval_owner_type_page |
| needs_review/41 | indexes | `idx_work_items_fulfillment_queue` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.idx_work_items_fulfillment_queue |
| needs_review/42 | indexes | `uk_work_items_open_sales_invoice_execution_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_sales_invoice_execution_object |
| needs_review/43 | indexes | `uk_work_items_open_object_type` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_object_type |
| needs_review/44 | indexes | `uk_work_items_open_fulfillment_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_fulfillment_object |
| needs_review/45 | indexes | `uk_work_items_open_customer_acceptance_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_customer_acceptance_object |
| needs_review/46 | indexes | `uk_work_items_open_payment_execution_object` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_open_payment_execution_object |
| needs_review/47 | indexes | `uk_work_items_approval_execution` | actual_collection_keys_unique_and_partial_source_resolved；index_review.resolved_parser_index_definitions.uk_work_items_approval_execution |
| needs_review/48 | foundations | `money source or symbol hashes changed` | out_of_scope_preserved；独立 owner 审核 |
| needs_review/49 | foundations | `idempotency source or symbol hashes changed` | out_of_scope_preserved；独立 owner 审核 |
| needs_review/50 | foundations | `transaction source or symbol hashes changed` | out_of_scope_preserved；独立 owner 审核 |
| needs_review/51 | collections | `casbin_policy_state` | existing_collection_new_capture_visibility；index_review.collection_resolution |
| needs_review/52 | collections | `casbin_rules` | existing_collection_new_capture_visibility；index_review.collection_resolution |
| needs_review/53 | collections | `policy` | document_id_false_positive_not_collection；index_review.collection_resolution |
| needs_review/54 | dto | `AddressTypeFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.AddressTypeFact |
| needs_review/55 | dto | `EffectiveRecordStatusFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.EffectiveRecordStatusFact |
| needs_review/56 | dto | `HttpWorkItemConflict` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.HttpWorkItemConflict |
| needs_review/57 | dto | `ImportJobStatus` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.ImportJobStatus |
| needs_review/58 | dto | `PartyAddressFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.PartyAddressFact |
| needs_review/59 | dto | `PartyBankAccountFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.PartyBankAccountFact |
| needs_review/60 | dto | `PartyContactFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.PartyContactFact |
| needs_review/61 | dto | `PartyStatusFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.PartyStatusFact |
| needs_review/62 | dto | `PartyTaxProfileFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.PartyTaxProfileFact |
| needs_review/63 | dto | `SalesBusinessTypeFact` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.SalesBusinessTypeFact |
| needs_review/64 | dto | `SupplierFailureClass` | actual_type_ownership_or_internal_shape_change_preserved_source_contract_reviewed；cumulative_schema.SupplierFailureClass |

## 5. 最终绑定门禁

已登记 331 个真实文件 SHA。所有已读输入侧文件逐一比对实际 source commit blob；candidate 当前 SHA 与 raw inventory 分开保存。整份 raw source_inventory 的当前一致性也逐文件核对，结果见 JSON `full_raw_inventory_binding`。

最终必须保留 working 报告并用 source commit / fresh sealed raw 重绑。任何新增差异须重新审核对应代码，不得复制旧数量、替换真实退出码或补造运行证据。
