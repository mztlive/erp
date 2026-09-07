# 阶段 16：DTO／索引 parser 限制复核合同

输入：`/private/tmp/erp-domain-crate-15-commerce-scope`，source commit `ed8015e28d36e5b9323b87e1a1fd5f63c122eb70`。候选：`/private/tmp/erp-domain-crate-16-supply`，source commit：72a0c79a2261d33699b869329376e534edfb1ef4。

原始报告：`/tmp/erp-supply16-contract-sealed/missing-drift-report.json`，SHA256 `10857599382053d1b19c049878b2dec37aff1b07a4b01fca54ad396ba221c6e0`。保留 `status=drift_detected`、实际 `exit 1`；changed 1、missing 0、added 1、needs_review 50。本报告不修改 raw、parser 或 allowlist。

## 1. 核销范围

本报告覆盖 47 个实际 needs_review：两侧共 46 个 tuple/index 限制项，另含新增 SupplierFailureClass 的无 side 项。changed DispatchOutcome 和 added SupplierFailureClass 的完整 raw 条目另存 JSON。三个 foundation 项（money、idempotency、transaction）按原条目排除，不用此报告代替事务／金额语义审查。

所有普通限制项均定位当前源码并与对侧唯一配对；9 个原路径文件真实字节相同。索引调用链另外展开至 trait 常量、Database 实现、域聚合器、全局聚合器、web-api 启动和 Cargo manifest。

## 2. DTO 逐符号约束

| 符号 | 必须保留的源码序列化合同 |
| --- | --- |
| `UserID` | derived Deserialize only；Derived newtype String deserialization; no Serialize implementation is declared for this extractor. |
| `Account` | derived Deserialize only；Derived newtype String deserialization; the tuple field is String, not AccountCore. No Serialize implementation is declared for this extractor. |
| `CommandFingerprint` | derived transparent Serialize plus manual Deserialize；Deserialize reads String then Self::parse; v1 prefix and 64 hexadecimal characters are required, accepted digest is lowercased. |
| `IdempotencyKey` | derived transparent Serialize plus manual Deserialize；Deserialize reads String, calls Self::parse, then requires parsed value equal persisted value; noncanonical whitespace is rejected instead of silently normalized. |
| `CommandScope` | derived transparent Serialize only；Source declares no Deserialize implementation; preserve this directionality. v3 constructor and namespace remain in the byte-identical source. |
| `CommandDigest` | derived transparent Serialize only；Source declares no Deserialize implementation; preserve this directionality. Distinct digest namespace remains in the byte-identical source. |
| `ParticipantId` | derived newtype Serialize plus manual Deserialize；Deserialize reads String then Self::new; empty and overlength identifiers are rejected without trimming. |
| `ElectronicRecipientFingerprint` | derived newtype Serialize and Deserialize；Derived Deserialize wraps the String directly; constructor validation is not an added deserialization requirement. |
| `ServiceRecipientFingerprint` | derived newtype Serialize and Deserialize；Derived Deserialize wraps the String directly; retain the distinct recipient newtype. |
| `ServiceLocationFingerprint` | derived newtype Serialize and Deserialize；Derived Deserialize wraps the String directly; retain the distinct location newtype. |
| `ContentHmac` | derived newtype Serialize and Deserialize；Derived Deserialize wraps the String directly; the separate parse constructor is not invoked by the derive. Preserve the existing contract. |

## 3. DispatchOutcome 与新增失败事实

原 `services/src/supplier_fulfillment/gateway.rs` 的 `DispatchOutcome::Failed.error_class` 引用 integration `ErrorClass`；新 `erp-supply/src/ports/supplier_gateway.rs` 引用本域 `SupplierFailureClass`。该 Rust 类型归属变化与新增枚举必须保留为 raw changed/added，不得通过改名或放宽 parser 隐藏。

两种失败枚举均采用相同的 derived Serialize/Deserialize 与 `rename_all="snake_case"`，八个 unit variant、顺序和属性等价。DispatchOutcome 的 `type` 内部标签、SCREAMING_SNAKE_CASE 名称、其他三个 variant 及其字段完全相同；Failed 的 required error_class／summary 保持。未知分类和缺失 required 字段没有增加 default／other 处理。

| Rust variant | 两侧 wire 值 |
| --- | --- |
| `CapabilityGap` | `capability_gap` |
| `MappingError` | `mapping_error` |
| `BusinessRejected` | `business_rejected` |
| `TransientFailure` | `transient_failure` |
| `ResultUnknown` | `result_unknown` |
| `AuthSignature` | `auth_signature` |
| `RateLimited` | `rate_limited` |
| `OutOfOrder` | `out_of_order` |

Process 中 `supplier_failure::{integration_class,supplier_class}` 各有八个显式一对一 match 分支，无 wildcard。已有内联 `all_failure_classes_round_trip_with_identical_wire_codes` 直接断言两域 serde_json 值相等并做往返映射；本次只读该测试源码，没有执行它。serde／serde_derive／serde_json／bson 的两侧 lock 版本与 checksum 已核对一致。

**证据等级：源码序列化合同等价；不声称已执行 Rust 序列化或 BSON 测试。** 真实 raw 类型差异、完整 enum 源码、转换函数及依赖记录均保存在 JSON 的 `dispatch_schema_review`。

## 4. 索引迁移与注册合同

| 域 | 实际 collection 数 | 实际 index 数 | 文件去向 |
| --- | ---: | ---: | --- |
| `supplier_api` | 5 | 11 | `database/src/indexes/supplier_api.rs` → `crates/erp-supply/src/indexes/supplier_api.rs` |
| `supplier_offering` | 4 | 10 | `database/src/indexes/supplier_offering.rs` → `crates/erp-supply/src/indexes/supplier_offering.rs` |
| `supplier_fulfillment` | 7 | 16 | `database/src/indexes/supplier_fulfillment.rs` → `crates/erp-supply/src/indexes/supplier_fulfillment.rs` |
| `supplier_settlement` | 5 | 12 | `database/src/indexes/supplier_settlement.rs` → `crates/erp-supply/src/indexes/supplier_settlement.rs` |

共 39 个当前函数、21 个 collection、49 个索引已展开复核。四组顺序必须为 supplier_api → supplier_offering → supplier_fulfillment → supplier_settlement；新 erp_supply 聚合器在旧全局四调用原位置展开，`.await?` 首错传播保持。

四组索引文件只调整 repository import；API／Fulfillment ensure 的可见性扩为 pub。函数本体、命名字符串、键顺序、方向、unique Option 与部分筛选保持。所有 named_index 的 unique 为 None，不把 parser 的 false 布尔值误说成 Some(false)；未增加 sparse／TTL。四个部分唯一筛选分别保持 external_order_no 为 string、allocation_action 为 REVERSE、external_bill_no 为 string、结算 status 为 CONFIRMED。

四组 extension 的实际迁移路径均为 `database/src/repository/extensions/<group>.rs` → `crates/erp-supply/src/repository/extensions/<group>.rs`。API／Fulfillment／Settlement extension 全字节相同。Offering 只移除展示查询相关 reexport／associated type 三处声明；collection 常量、Database 默认值和访问器保持。本报告不据此替代 Offering 跨域查询语义审核。

原 12 个 parser 限制索引保持采购责任规则 3 个与 work_items 9 个。work_items 同名旧索引调和继续仅在现有键／unique／partial 不一致时删除；采购 selector 的 active＋deleted_at i64 0 筛选保持。完整键顺序、partial 文本与每个 builder 源码见 JSON，不用只比较索引名称的结论代替。

## 5. 原始 needs_review 逐项处置

| raw 序号 | 类别／side | 符号 | 处置与 JSON 证据 |
| ---: | --- | --- | --- |
| 0 | dto／before | `UserID` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.UserID`；raw needs_review 保留 |
| 1 | dto／before | `Account` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.Account`；raw needs_review 保留 |
| 2 | dto／before | `CommandFingerprint` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.CommandFingerprint`；raw needs_review 保留 |
| 3 | dto／before | `IdempotencyKey` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.IdempotencyKey`；raw needs_review 保留 |
| 4 | dto／before | `CommandScope` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.CommandScope`；raw needs_review 保留 |
| 5 | dto／before | `CommandDigest` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.CommandDigest`；raw needs_review 保留 |
| 6 | dto／before | `ParticipantId` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.ParticipantId`；raw needs_review 保留 |
| 7 | dto／before | `ElectronicRecipientFingerprint` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.ElectronicRecipientFingerprint`；raw needs_review 保留 |
| 8 | dto／before | `ServiceRecipientFingerprint` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.ServiceRecipientFingerprint`；raw needs_review 保留 |
| 9 | dto／before | `ServiceLocationFingerprint` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.ServiceLocationFingerprint`；raw needs_review 保留 |
| 10 | indexes／before | `idx_procurement_responsibility_list` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.idx_procurement_responsibility_list`；raw needs_review 保留 |
| 11 | indexes／before | `idx_procurement_responsibility_owner` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.idx_procurement_responsibility_owner`；raw needs_review 保留 |
| 12 | indexes／before | `uk_procurement_responsibility_active_selector` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_procurement_responsibility_active_selector`；raw needs_review 保留 |
| 13 | dto／before | `ContentHmac` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.ContentHmac`；raw needs_review 保留 |
| 14 | indexes／before | `idx_work_items_document_approval_owner_page` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.idx_work_items_document_approval_owner_page`；raw needs_review 保留 |
| 15 | indexes／before | `idx_work_items_document_approval_owner_type_page` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.idx_work_items_document_approval_owner_type_page`；raw needs_review 保留 |
| 16 | indexes／before | `idx_work_items_fulfillment_queue` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.idx_work_items_fulfillment_queue`；raw needs_review 保留 |
| 17 | indexes／before | `uk_work_items_open_sales_invoice_execution_object` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_work_items_open_sales_invoice_execution_object`；raw needs_review 保留 |
| 18 | indexes／before | `uk_work_items_open_object_type` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_work_items_open_object_type`；raw needs_review 保留 |
| 19 | indexes／before | `uk_work_items_open_fulfillment_object` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_work_items_open_fulfillment_object`；raw needs_review 保留 |
| 20 | indexes／before | `uk_work_items_open_customer_acceptance_object` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_work_items_open_customer_acceptance_object`；raw needs_review 保留 |
| 21 | indexes／before | `uk_work_items_open_payment_execution_object` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_work_items_open_payment_execution_object`；raw needs_review 保留 |
| 22 | indexes／before | `uk_work_items_approval_execution` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_work_items_approval_execution`；raw needs_review 保留 |
| 23 | dto／after | `UserID` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.UserID`；raw needs_review 保留 |
| 24 | dto／after | `Account` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.Account`；raw needs_review 保留 |
| 25 | dto／after | `CommandFingerprint` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.CommandFingerprint`；raw needs_review 保留 |
| 26 | dto／after | `IdempotencyKey` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.IdempotencyKey`；raw needs_review 保留 |
| 27 | dto／after | `CommandScope` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.CommandScope`；raw needs_review 保留 |
| 28 | dto／after | `CommandDigest` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.CommandDigest`；raw needs_review 保留 |
| 29 | dto／after | `ParticipantId` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.ParticipantId`；raw needs_review 保留 |
| 30 | dto／after | `ElectronicRecipientFingerprint` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.ElectronicRecipientFingerprint`；raw needs_review 保留 |
| 31 | dto／after | `ServiceRecipientFingerprint` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.ServiceRecipientFingerprint`；raw needs_review 保留 |
| 32 | dto／after | `ServiceLocationFingerprint` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.ServiceLocationFingerprint`；raw needs_review 保留 |
| 33 | indexes／after | `idx_procurement_responsibility_list` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.idx_procurement_responsibility_list`；raw needs_review 保留 |
| 34 | indexes／after | `idx_procurement_responsibility_owner` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.idx_procurement_responsibility_owner`；raw needs_review 保留 |
| 35 | indexes／after | `uk_procurement_responsibility_active_selector` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_procurement_responsibility_active_selector`；raw needs_review 保留 |
| 36 | dto／after | `ContentHmac` | source_reviewed_no_observed_wire_or_index_drift；`dto_definitions.ContentHmac`；raw needs_review 保留 |
| 37 | indexes／after | `idx_work_items_document_approval_owner_page` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.idx_work_items_document_approval_owner_page`；raw needs_review 保留 |
| 38 | indexes／after | `idx_work_items_document_approval_owner_type_page` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.idx_work_items_document_approval_owner_type_page`；raw needs_review 保留 |
| 39 | indexes／after | `idx_work_items_fulfillment_queue` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.idx_work_items_fulfillment_queue`；raw needs_review 保留 |
| 40 | indexes／after | `uk_work_items_open_sales_invoice_execution_object` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_work_items_open_sales_invoice_execution_object`；raw needs_review 保留 |
| 41 | indexes／after | `uk_work_items_open_object_type` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_work_items_open_object_type`；raw needs_review 保留 |
| 42 | indexes／after | `uk_work_items_open_fulfillment_object` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_work_items_open_fulfillment_object`；raw needs_review 保留 |
| 43 | indexes／after | `uk_work_items_open_customer_acceptance_object` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_work_items_open_customer_acceptance_object`；raw needs_review 保留 |
| 44 | indexes／after | `uk_work_items_open_payment_execution_object` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_work_items_open_payment_execution_object`；raw needs_review 保留 |
| 45 | indexes／after | `uk_work_items_approval_execution` | source_reviewed_no_observed_wire_or_index_drift；`index_definitions.uk_work_items_approval_execution`；raw needs_review 保留 |
| 49 | dto／无 side | `SupplierFailureClass` | actual_new_domain_type_source_reviewed_wire_schema_equivalent；`dispatch_schema_review`；raw needs_review 保留 |
| 46 | foundations | money source or symbol hashes changed | 本报告排除，交独立语义审核；raw 原项保留 |
| 47 | foundations | idempotency source or symbol hashes changed | 本报告排除，交独立语义审核；raw 原项保留 |
| 48 | foundations | transaction source or symbol hashes changed | 本报告排除，交独立语义审核；raw 原项保留 |

## 6. 绑定与使用门禁

当前指纹：Rust 73 个、manifest 5 个、Cargo.lock 2 个；Rust 与当前 raw 库存全部匹配：`true`。每个输入侧文件已经核对真实 ed8015e28 blob；候选最终 blob 绑定以 `after_source_frozen` 为准。

最终 sealed raw 生成后，使用同一复核脚本传 `--raw <sealed目录> --raw-exit-code <真实退出码> --after-source-commit <最终提交>`。必须先保存当前 working 报告再重绑；新 raw 有新增项目或源码变化时重新核对对应语义，禁止直接复制旧结论。不得因语义审查等价而改写 exit 1 为 exit 0／2。

本报告仅写 /private/tmp 证据；未改业务源码、Cargo 配置或历史 tests/，未运行 Cargo、数据库、真实序列化或业务程序。
