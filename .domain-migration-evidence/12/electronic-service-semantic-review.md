# 阶段 12：电子交付与服务履约语义复核合同

## 1. 适用输入与判定

- 固定输入提交：`0a297ab00d29b28047e05f780b4815c0a5456a2f`。对比输入使用该提交的 Git 对象，不使用持续变化的工作树 HEAD。
- 封存输出提交：`de112614e03714bb6d1ca4ac9e25f580c5c273c8`；源文件哈希逐项与此提交的 Git 对象核对一致。
- 输入参考树：`/private/tmp/erp-domain-crate-11-procurement`；输出核对树：`/private/tmp/erp-domain-crate-12-fulfillment`。
- 范围：旧 `backend/services/src/fulfillment/{electronic_delivery.rs,electronic_delivery_crypto.rs,service_fulfillment.rs,service_fulfillment_crypto.rs,service_fulfillment_confirm.rs}` 的生产实现，以及本次迁移实际调用的领域规则、付款与销售事实 Provider、加密 Provider、附件、事务与 HTTP 补偿入口。
- 判定：本证据固定的源码快照内，未识别出本范围的业务语义漂移。原 35 个生产函数全部有目标符号；20 个主目标函数体在保留字面量、去除注释和空白后相同；其余 15 个已按下文展开真实实现。六个支持实体文件的 64 个生产函数体全部保持相同 token。原五叶的 13 个内联测试入口全部保留。
- 本判定是静态源码复核。未运行 Cargo、MongoDB、对象存储或外部发送；不证明真实数据库回滚、并发、重试或对象补偿已运行成功。阶段门禁由集成负责人单独记录。

## 2. 证据与核销要求

完整源文件 SHA-256、输入工作树与固定提交一致性、前后函数行号/函数体 SHA-256、目标辅助符号、保留测试清单见 [source-map JSON](/private/tmp/fulfillment12-electronic-service-source-map.json)。逐函数变化见 [body diffs](/private/tmp/fulfillment12-electronic-service-body-diffs.txt)，支持规则比较见 [supporting diffs](/private/tmp/fulfillment12-electronic-service-supporting-diffs.txt)。哈希覆盖的输入参考文件全部等于固定输入提交；采集结束时输出文件哈希未发生变化。

| 旧叶 | 生产函数数 | 必须保留的实际落点 |
| --- | ---: | --- |
| `electronic_delivery.rs` | 12 | `erp_fulfillment::service::electronic_delivery` 持有列表、详情、视图、本域读写；`erp_processes::fulfillment_execution::electronic_delivery` 持有创建注册与确认事务。 |
| `electronic_delivery_crypto.rs` | 2 | `erp_fulfillment::service::electronic_delivery_crypto` 持有指纹和草稿工厂；过程传入原 actor ID、原配置密钥。 |
| `service_fulfillment.rs` | 11 | `erp_fulfillment::service::service_fulfillment` 持有单域查询/写入；`erp_processes::fulfillment_execution::service_fulfillment` 持有注册、任务和审计事务。 |
| `service_fulfillment_crypto.rs` | 3 | `erp_fulfillment::service::service_fulfillment_crypto` 保留两种强类型指纹和草稿工厂。 |
| `service_fulfillment_confirm.rs` | 7 | 本域地点/确认事实工厂和状态写入归 `erp_fulfillment::service::service_fulfillment_confirm`；真实跨域执行归 `erp_processes::fulfillment_execution::service_confirm`；codec/元数据映射归 `service_crypto`。 |

新增代码不得以相同 Port 步骤名替代实际实现核对；本报告第 5 节已经展开 `MongoServiceConfirmation` 的每个生产方法。原 list/detail/from 函数体保持一致，三类创建/确认 DTO 字段与校验属性 token 保持一致。

## 3. 构造、密钥、时间与身份合同

1. [HTTP `process`](/private/tmp/erp-domain-crate-12-fulfillment/backend/apps/web-api/src/core/handler/fulfillment/mod.rs:51) 继续注入 `state.db()`、`state.config_snapshot().app.secret.as_bytes().to_vec()`、`state.sensitive_data()` 和 `state.approval_object_read()`。`FulfillmentProcess::new` 继续使用原共享 RBAC 构造与 `FailClosedObjectReadPort` 默认值；`with_object_read` 保留原替换语义。领域 `FulfillmentService::new(db)` 只保存 Database，没有读取、验证、ID 或时钟副作用。
2. 创建入口继续先 `req.validate()`，后草稿工厂，再持久化。草稿工厂顺序固定为请求 `occurred_at` 转换、`Instant::now()`、记录 `next_id()`、结构体字段逐项求值、`fact_no: next_id()`、领域工厂规则。`actor` 改为 `actor.id()` 投影不改变 `recorded_by`。
3. [电子草稿工厂](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-fulfillment/src/service/electronic_delivery_crypto.rs:49) 复制原不透明 `recipient_snapshot`，在原位置计算同 key、同字节的 HMAC；记录 ID 先于指纹，事实号后于指纹。不得添加电子发送或重新加密。
4. [服务草稿工厂](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-fulfillment/src/service/service_fulfillment_crypto.rs:69) 继续复制原不透明 recipient/location 值。`service_location_encrypted` 仍来自 `req.service_location.clone()`，地点指纹仍使用该输入值，不调用 codec 再次加密。开始/结束时间转换、说明、事实号与原位置相同。
5. 电子 recipient、服务 recipient、服务 location 强类型保持分离。`entity/fulfillment/fingerprint.rs` 的 HMAC、SHA-256、验证函数及常量保留原实现；原 golden 测试保留。真实 `erp_party::SensitiveDataCodec` 源文件与输入逐字节一致，无替代密钥或新加密实现。

## 4. 创建事务与电子确认合同

### 4.1 两类创建

`create_electronic_delivery` / `create_service_fulfillment` 保留原 clone、返回视图时点。`persist_created_*` 在根事务前构造原 action/resource/record ID 的审计对象；根事务内严格执行：

1. `register_created_*_document`：先按原 command/登记上下文生成 BusinessDocument；再验证对应 Policy 的 `NO_APPROVAL` 身份；验证 `SkipNoApproval` 和无 adapter；调用原 `services::workflow_compose::bind_published_definition_on_document_create`；检查无审批注册结果；登记文档。
2. 领域 `persist_created_*` 执行原单次 `db.electronic_deliveries().create` 或 `db.service_fulfillments().create`，使用调用者 Executor。
3. 原 `ensure_fulfillment_task`，传同一个记录和 Executor。
4. 原审计仓储 create。

没有新增 published definition 查询、审批实例启动或外部发送。原策略测试与无审批注册测试保留。

### 4.2 电子确认

[真实过程 `confirm_electronic_delivery`](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/electronic_delivery.rs:101) 保留一个 `with_transaction`。严格顺序为：本域记录读取 → 不存在 NotFound → `ensure_confirmable` 的 ConflictError → 来源采购单读取/NotFound → 采购状态 → PREPAY → 分配有效性 → 本域 `record.confirm()` / update → 完成履约任务 → 创建客户验收任务 → 审计构造与 create。

本域读取与状态写入分别由 `prepare_electronic_confirmation` / `persist_electronic_confirmation` 原位承接；没有提前读取或开启子事务。电子路径原来就无条件调用客户验收任务入口，输出继续如此；不得把服务的资格分支加到电子路径。Process 的重复确认仍先经过状态守卫；实体 `confirm` 自身的幂等行为没有取代该守卫。

## 5. 服务确认首错与九步真实 Provider 合同

### 5.1 根事务前

[过程入口](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-processes/src/fulfillment_execution/service_confirm.rs:83) 严格保留：`req.validate()` → `resolve_service_evidence_id` 内 `resolve_id` → `ensure_all_used` → 正式 evidence ID clone → [本域确认工厂](/private/tmp/erp-domain-crate-12-fulfillment/backend/crates/erp-fulfillment/src/service/service_fulfillment_confirm.rs:32) → 根持久化入口。

确认工厂严格保留 `ActualServiceLocation::parse` → 原 codec 加密规范化地点 → 对同一规范化明文计算原 HMAC → 时间戳转换 → `ServiceFulfillmentConfirmation::new`。确认实体内部顺序保持二元结果、完成说明、凭证主键、地点密文、指纹规范化/格式、正数量、时间窗。地点非法先于加密失败；加密失败先于后续确认字段规则失败。此段没有新增 ID、时钟或事务。

`ServiceLocationCryptoPort` 使用关联错误类型；真实 `ServiceCryptoAdapter::Error = services::Error`，`encrypt` 直接 `Ok(self.0.encrypt(plaintext)?)`。`erp_core` → 履约 Error → services Error 的转换保留原类型；未以字符串包装抹平 provider 错误。

### 5.2 事务内

旧入口 `services::fulfillment::service_fulfillment_confirm::confirm_service_fulfillment_in_transaction` 的完整业务块由以下生产组合承接：`erp_processes::fulfillment_execution::service_confirm::{confirm_service_fulfillment_in_transaction,execute_confirmation,MongoServiceConfirmation}`。Provider 构造只保存引用与值；不会预读或预写。`execute_confirmation` 的每个 await 均使用原传入的 `&mut dyn Executor` 并以 `?` 停止。

| 顺序 | 实际生产 Provider | 展开后的原操作与首错 | 目标源行 |
| --- | --- | --- | --- |
| 1 | `MongoServiceConfirmation::load` | `prepare_service_confirmation` 读取服务记录；不存在仍为“服务履约记录不存在”；`ensure_draft_version` 先草稿状态，后版本。 | `service_confirm.rs:309`；领域 `service_fulfillment_confirm.rs:60` |
| 2 | `purchase` | 读取 `record.purchase_order_id`；不存在仍为“来源采购单不存在”；采购 Effective/PartiallyExecuted 守卫；读取生效版本后检查 PREPAY，再读取付款净额。 | `service_confirm.rs:315`；`purchase_context.rs:32,61,95,126` |
| 3 | `allocation` | 原 allocation ID 与 sales line ID；读取分配、当前版本/版本行、关联销售版本行，再执行本域一致性规则。 | `service_confirm.rs:327`；`purchase_context.rs:177` |
| 4 | `evidence` | pending 包含正式 ID 则原位短路；否则同 Executor 查 file asset，不存在仍为“现场图片凭证不存在”，再执行证据政策。 | `service_confirm.rs:343,230` |
| 5 | `pending` | 原 `pending_assets.persist`；真实 `PendingFileAssets::persist` 先 `file_assets.create_many_ordered`，再 `audit_logs.create_many_ordered`，传同一 Executor。 | `service_confirm.rs:353`；`attachments/pending.rs:89` |
| 6 | `confirm` | 领域 `persist_service_confirmation` 原位执行 `apply_confirmation` → `confirm` → service record update。该处 clone 确认事实不产生 ID/clock/I/O。 | `service_confirm.rs:358`；领域 `service_fulfillment_confirm.rs:79` |
| 7 | `task` | 完成任务，传已更新记录、原 actor ID 和原 Executor。 | `service_confirm.rs:365` |
| 8 | `is_acceptance_eligible` / `acceptance` | 检查实际记录的 `status.is_acceptance_eligible() && result == Success`；仅满足时传 `po.sales_order_id` 与 `DeliveryAvailable` 创建客户验收任务。失败服务跳过该步骤，仍执行审计。 | `service_confirm.rs:375,379`；实体 `service_fulfillment.rs:594` |
| 9 | `audit` | 在原最后位置构造 `service_fulfillment.confirm` / `service_fulfillment` / 原 record ID 的审计，然后 create。 | `service_confirm.rs:390` |

真实九步 Provider 已展开，不能将同 Executor、首错或条件分支结论仅归于替身测试。新增替身源码包含非零大小 Executor 身份断言、各步注入失败后停止、失败服务跳过验收任务；本复核未运行这些测试。

## 6. 采购付款与销售来源 Provider 合同

- `ensure_prepay_gate` 继续先 `load_po_current_revision`，然后判断 `prepay_gate`。门槛为 false 时不读取付款净额。缺失生效版本指针与缺失版本记录的原不同错误保留。
- `effective_paid_amount` 继续应付子账 → 分录 → 过滤 `entry.source_document_id == po_id` → 付款核销分配 → 原遍历顺序累计 `APPLY - REVERSE`。没有加入付款状态筛选、改用聚合或提前求和；金额/比例/采购总额原值投影为履约窄事实。
- 实际财务 `PayableAccountRepository::list_payable_accounts_for_purchase_order` 保留 `source_document_id`、`source_type=PurchaseOrder`、未删除三个条件。新 owner 的 `find_many` 经过原 generic Repository，补入相同 deleted_at，仍调用原 `mongo_ops::find_many` 与默认 FindOptions；没有新增筛选/排序或 Executor 替换。
- 实际销售 `SalesOrderRevisionLineRepository::sales_revision_line_for_allocation` 保留 `id`、`sales_order_line_id`、未删除条件。新 owner 的 `find_one` 使用同 generic 路径，缺失关联继续 None。
- 状态、预付要求和分配只在进入履约规则时做字段投影；Process 原读取位置和领域规则原错误顺序保留。

## 7. 附件证据全枚举与提交未知补偿合同

### 7.1 全枚举映射

真实提供方为 `erp_support::{SensitivityClass,RetentionClass}`；`erp_processes::fulfillment_execution::service_crypto::evidence_metadata` 使用显式 match，无 wildcard：

| 提供方值 | 消费方事实 | 原政策结果 |
| --- | --- | --- |
| General | General | 拒绝，原敏感级别错误。 |
| Sensitive | Sensitive | 通过敏感级别检查。 |
| HighlySensitive | HighlySensitive | 通过敏感级别检查，不误拒绝更高等级。 |
| LongTerm | LongTerm | 通过保留周期检查。 |
| ThirtyDays | Other | 拒绝，原长期保留错误。 |
| SevenDays | Other | 拒绝，原长期保留错误。 |

政策顺序保持 MIME（JPEG/PNG/WebP）→ 敏感级别 → LongTerm → destroyed。Pending 与已存在资产都调用同一映射及政策。真实提供方 enum 定义和领域 `ServiceEvidencePolicy::validate` 已展开；领域政策函数体保持相同 token。

### 7.2 HTTP、pending 与补偿

完整 HTTP 时点保持：multipart 解析 → `validate_service_evidence_upload` 的文件/引用/MIME 检查 → 对象写入 `store_pending_asset_files` → `attachments::fulfillment::confirm_service_fulfillment_with_assets` 对上传 metadata 逐项政策校验 → `PendingFileAssets::prepare` → 服务请求验证/引用全消费/地点加密 → 业务事务。因此不得把过程内 `req.validate()` 描述为整个 HTTP 链的第一项校验。

`PendingFileAssets::prepare` 在事务前按原顺序验证临时引用和登记 metadata、分配文件 ID、构造资产与登记审计、组装引用集合。其整个文件与输入逐字节一致；实际 `persist` 在服务事务第 5 步写资产和登记审计。对象字节上传/删除仍在事务外。

`persistence_core::transaction` 与输入逐字节一致：原同会话提交重试用原标签/超时分类；结果未知保留 `CommitOutcomeUnknown`；callback 失败执行原 abort 尝试并返回原错误。根服务 `From<persistence_core::Error>` 继续转换为 `services::Error::OutcomeUnknown`。本域仓储传播经过 `erp_fulfillment::Error::OutcomeUnknown` 时，新增 `From<erp_fulfillment::Error>` 显式映回同一 services 变体。HTTP 仍将该错误原值传给 [should_compensate_pending_assets](/private/tmp/erp-domain-crate-12-fulfillment/backend/apps/web-api/src/core/handler/file_asset/mod.rs:524)：唯一排除条件仍是 `services::Error::OutcomeUnknown(_)`。因此提交未知分支保留已上传对象，其余原分类进入原删除补偿分支；没有增加事务内对象 I/O。

## 8. 独立复核边界与集成使用

- 本报告覆盖 D 叶与真实支持调用；履约任务/客户验收任务已追到入口、参数、分支与 Executor。任务内部责任/授权流程及客户验收 reverse 条件 reopen 不属于本报告的独立复核范围。任务迁移由本复核者在 E 分片实现，应由独立汇总者对照；辅助证据为 `/private/tmp/fulfillment12-task-evidence.json`。
- 13 个旧内联测试的“保留”只表示新路径有原入口；本报告不声明其通过。新增 Port/crypto/enum 测试同样只完成源码核对。
- 任一已记录源文件哈希变化后，集成负责人须重新核对该变化覆盖的符号与调用链，再引用本结论。不得以本文件替代阶段 12 公共门禁或真实数据库验证。

## 9. 固定提交封存记录

- Source commit：`de112614e03714bb6d1ca4ac9e25f580c5c273c8`。本报告 D 范围的 36 个 after 文件逐项对照固定提交：原采集 SHA-256 全部不变，工作树字节也全部相同，无需替换任何已记录源哈希。
- 任务证据单独重新执行原 35 个生产符号对比，仍为 33 个函数体同构与 2 个真实 Port 抽取入口；`task/command.rs` 的 6 个生产函数对照原生成脚本：先将提取的原 Rust 字符串写入 `/tmp` 并按仓库 rustfmt 配置格式化，再去除格式空白/注释/尾逗号后完全一致，import 集合相同。最终差异限于测试预期修正与 import/格式排序。原生成脚本只作为文本解析，没有执行其写源码操作。
- 测试修正已逐项核对真实 `WorkItem::{complete_open,record_activity}`：Complete 保留原 last_activity_at，形成完成字段；Activity 更新 last_activity_at，保留完成字段。任务证据已封存 8 个相关输出源文件哈希。
- 独立读取集成负责人完成的 `/private/tmp/erp-fulfillment12-lib-tests-2.log`：33 个测试结果组共 3440 passed / 0 failed / 68 ignored；四个 task command 测试及本 D 范围 13 个原 inline 测试均记录为 ok。日志 SHA-256 为 `d73266848b7cfc85bd7e3b5c220cdfe8047d1e60ce1589e56ff56872fd4b664c`。本复核者未运行 Cargo；该日志引用不扩大到真实数据库运行验证，也不替代其他公共门禁的独立记录。
