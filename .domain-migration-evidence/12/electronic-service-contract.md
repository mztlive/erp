# 阶段 12 电子交付与服务履约分片执行合同

## 1. 输入、所有权与验证边界

- 唯一输入树：`/private/tmp/erp-domain-crate-12-fulfillment`，输入提交 `0a297ab0`。
- 迁移旧源：`services/src/fulfillment/{electronic_delivery,electronic_delivery_crypto,service_fulfillment,service_fulfillment_crypto,service_fulfillment_confirm,mod}.rs`。六个旧文件已删除，不保留旧路径转发。
- 共享根仅按 root 授权维护：新 `erp-fulfillment/src/service/mod.rs`、`ports/mod.rs`、`erp-processes/src/fulfillment_execution/mod.rs`。其他分片的叶实现不归本分片。
- 调用方改动限 `erp-processes/src/attachments/fulfillment.rs`；HTTP/全局注册/Cargo 由唯一集成人处理。
- 本分片未运行 Cargo、MongoDB、外部服务或历史 `tests/**`。下文“保留/新增”指源码与调用图核对，不能登记为测试通过。真实数据库运行未验证。

## 2. 统一构造与 DTO 合同

| 符号 | 最终合同 |
| --- | --- |
| `erp_fulfillment::service::FulfillmentService::new` | 仅 `Database`；结构仅持 `db`。 |
| `erp_processes::fulfillment_execution::FulfillmentProcess::new` | 原 `(Database, Vec<u8>, Arc<SensitiveDataCodec>)`；保持启动期 codec 共享、原 `shared_rbac_service` 与 `FailClosedObjectReadPort` 默认。 |
| `FulfillmentProcess::with_object_read` | 原 `Arc<dyn ApprovalObjectReadPort>` 注入，返回同一配置实例。 |
| `FulfillmentProcess::domain` | 兄弟叶内部从同一 db 构造领域服务，不重新生成密钥/codec/RBAC。 |
| 电子/服务请求、列表参数、View/PageView/SortDir | 唯一来源 `erp_fulfillment::dto::*`；DTO 分片负责字段及序列化，电子/服务单域 `From<Entity>` 保留在领域 service 叶。 |
| `ServiceLocationCryptoPort` | 位于领域 `ports::service_crypto`；只接已规范化地点，关联 Error 承接领域错误；Process `ServiceCryptoAdapter` 调原 party codec 并保留原 typed error。 |

领域不依赖 party/support/workflow/identity/procurement/sales/finance/inventory 或旧三层。Process 只把进入领域规则的内容映射为最小事实；组合内部仍可持各提供方实体。

## 3. 逐符号迁移与生产消费者

| 原源/符号 | 最终归属 | 生产消费与保留要求 |
| --- | --- | --- |
| `electronic_delivery::{electronic_delivery_list,electronic_delivery_detail}` | domain `service/electronic_delivery.rs`、同名 FulfillmentService 方法 | Validate → normalized → 原 filter/query；分页 row 顺序和 View 字段不变，详情 NoTransaction 原首错不变。 |
| `From<ElectronicDelivery> for ElectronicDeliveryView` | 同领域叶 | create/confirm 用原已写实体转 View，不新增重读。 |
| `create_electronic_delivery` | Process 同名方法 | validate → 领域草稿构造 → 原创建根 → 原实体 View。 |
| `electronic_delivery_draft_from_request` | domain `service/electronic_delivery_crypto.rs` | 只把 AuditActor 参数收窄为 actor_id；occurred_at → recorded_at clock → record ID → recipient fingerprint → fact ID 的原求值顺序保留。 |
| `electronic_recipient_fingerprint` | 同领域 crypto 叶 | 原 HMAC/强类型格式校验及 golden 测试。 |
| `confirm_electronic_delivery` | Process 同名方法 | 单一根事务；采购、任务及审计仍在组合层。 |
| 原电子确认 load/ensure_confirmable、confirm/update | domain `prepare_electronic_confirmation`、`persist_electronic_confirmation` | 同 Executor；原 NotFound、状态 Conflict、实体 Logic/仓储错误保留。 |
| 原电子 create 文档 policy/bind/register/persist helpers | Process `electronic_delivery.rs`，保原函数名 | NO_APPROVAL/no adapter → binding 返回无绑定 → document → 本域 create → execution task → audit。 |
| `service_fulfillment::{service_fulfillment_list,service_fulfillment_detail}` | domain `service/service_fulfillment.rs`、同名 FulfillmentService 方法 | 原过滤/排序/分页/NoTransaction 与 From 映射不变。 |
| `From<ServiceFulfillment> for ServiceFulfillmentView` | 同领域叶 | 原字段；create/confirm 不引入写后新读取。 |
| `create_service_fulfillment` | Process 同名方法 | validate → 领域双指纹草稿构造 → 原创建根 → 原实体 View。 |
| `service_fulfillment_draft_from_request` | domain `service/service_fulfillment_crypto.rs` | actor_id 收窄；原草稿不透明地点与 recipient 字段不重新加密，ID/两个指纹/时钟位置保持。 |
| `service_recipient_fingerprint`、`service_location_fingerprint` | 同领域 crypto 叶 | 强类型不可混用，算法/密钥版本 golden 测试保留。 |
| 原服务 create 文档 helpers | Process `service_fulfillment.rs`，保原名 | 原无审批校验与 binding/register/root 顺序，领域仅持久化本域记录。 |
| `confirm_service_fulfillment`、`confirm_service_fulfillment_with_assets` | Process `service_confirm.rs` 同名方法 | 无新文件时仍 `EmptyPendingAttachments`；普通与 multipart 共用一个确认根。 |
| `service_confirmation_from_request` | domain `service/service_fulfillment_confirm.rs` | 原地点规范化、同明文加密与 fingerprint、confirmation 构造；只替换 codec 参数为窄 Port。 |
| 原服务 record load/draft-version、apply_confirmation/confirm/update | domain `prepare_service_confirmation`、`persist_service_confirmation` | 分别在原事务初始及 pending persist 后调用；同 Executor、同首错。 |
| `persist_confirmed_service_fulfillment`、`confirm_service_fulfillment_in_transaction` | Process `service_confirm.rs` | 原 standalone root 和内层确认函数保留，由真实 MongoServiceConfirmation 适配器执行统一步骤。 |
| `resolve_service_evidence_id` | Process `service_confirm.rs` | 原 resolve_id → ensure_all_used → 返回正式 ID；保持请求中 ID 替换位置。 |
| `ensure_service_evidence_asset_in_transaction` | Process `service_confirm.rs` | 本批 ID 仍直接认可；否则同 Executor 查 file asset → 缺失错误 → 元数据事实显式映射 → 本域 policy。 |
| `confirm_service_fulfillment_with_assets` 附件组合入口 | `processes/src/attachments/fulfillment.rs` | 首参改 FulfillmentProcess；资产请求 metadata 检查 → PendingFileAssets::prepare/shared → 同一流程根。其余参数及共享 pending 实例不变。 |

## 4. 原事务与首错约束

### 4.1 电子确认

1. 领域读取 electronic record → 原 ensure_confirmable。
2. 原采购读取/状态 → PREPAY → allocation/current lines/sales association。
3. 本域 confirm/update → complete_fulfillment_task → ensure_customer_acceptance_task → audit。
4. 所有步骤复用根 session，返回原 confirmed View。当前没有电子发送能力，本迁移不新增发送或外部调用。

### 4.2 服务确认准备

1. DTO Validate。
2. pending resolve_id → ensure_all_used。
3. ActualServiceLocation::parse；空白/占位先失败。
4. 原 SensitiveDataCodec.encrypt，同一规范化明文计算原服务地点 fingerprint。
5. ServiceFulfillmentConfirmation::new 校验原时间、数量、结果与凭证。
6. 加密错误必须先于后续 confirmation 字段错误返回；关联 Error 不转换为错误字符串。

### 4.3 服务确认事务

真实生产 `execute_confirmation` 顺序固定如下，Mongo 适配器和纯替身共同调用，不存在只供测试使用的平行步骤实现。

1. `load`：领域 record 存在/draft/version。
2. `purchase`：原采购存在/状态/PREPAY。
3. `allocation`：原分配及版本/销售关联。
4. `evidence`：既有或本批 pending 图片资格。
5. `pending`：同 Executor 执行原 PendingAttachmentBatch::persist。
6. `confirm`：本域 apply_confirmation → confirm → update。
7. `task`：完成当前履约工作项。
8. 仅已确认 record.is_acceptance_eligible() 为真时 `acceptance`。
9. `audit`：原 actor/resource_log 构造时点与审计写入。

不得把 pending persist 提到 evidence 前；不得在确认写入前推进任务；任一步错误直接传播并终止后续步骤。当前实现只对确认参数作内存 Clone，未生成额外 ID、时钟或持久化事实。

### 4.4 附件与补偿

- multipart 上传仍在 HTTP 进入业务根之前完成；Process pending 仅登记已有对象元数据。
- 原 HTTP `should_compensate_pending_assets` 判定及 `delete_pending_asset_objects` 调用由 root 接线时保持；本分片未修改其未知提交处理。
- `OutcomeUnknown` 通过原 typed conversion 到 services/HTTP，不在领域或 Process 中吞并，不在同一失败 session 重跑业务。
- EvidenceSensitivity 对 General/Sensitive/HighlySensitive 显式一一映射；RetentionClass LongTerm→LongTerm，ThirtyDays/SevenDays→Other。领域校验顺序仍 MIME→敏感→保留→destroyed。

## 5. 测试核销与证据

- 原内联测试 13 项：电子4、电子 crypto1、服务4、服务 crypto1、服务确认3；全部随对应生产符号保留。电子/服务各有两个同名测试，按文件与函数共同计数，不能按名称全局去重。
- 新增 7 项：服务确认真实 Port 全序/每步失败/无资格跳验收3；领域地点首错与加密失败顺序2；附件 provider 枚举完整映射2。
- 当前源码合计 20 个测试入口；所有 `include_str!` 已核对指向实际存在的新实现。
- Executor 替身为非零 `TestExecutor { visits: usize }`；每次调用校验 data pointer 并累计访问，不能使用零尺寸 NoTransaction 地址作为同一实例证明。
- Rustfmt 仅限定本分片文件，未全 workspace fmt；`git diff --check` 静态检查通过。完整 check/clippy/lib tests 由 root 统一执行并登记真实结果。
