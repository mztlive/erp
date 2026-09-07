# 阶段 17 错误边界与入口语义核验合同

## 1. 输入、范围与证据状态

- 源码树：`/private/tmp/erp-domain-crate-17-cutover`。
- 冻结输入：`a537414eb8f78c43ebc383a3457a45c437dececc`。错误及入口默认按此实际 commit blob 比较；索引注册顺序另外按阶段 00 `400ab4f7855255b284fe8a8e1caffe27acc96083` 的真实全局展开顺序验收。
- 本审查只读取源码并写 `/private/tmp` 工件；不修改业务源码、共享根、Cargo 或历史测试，不执行 Cargo、HTTP 服务或 MongoDB。
- 最终业务源码：`39c55021d8bb1d030eade4afb3e135e4b5a8a18b`。主审 32 个实际文件全部逐 Git blob 匹配原审查 after SHA；49 个原 RM 消费者与 14 个 Import 补审文件也已按各自清单重绑。
- 错误主审记录位于同名 JSON：32 个实际文件的 before/after SHA-256、80 项检查，其中 73 项为去注释 Rust token 等价检查。当前全部满足。
- AppState、CLI、Web 启动、附件补偿、WorkItem 特殊响应与索引展开的独立子审位于 `/private/tmp/cutover17-startup-compensation-review.{md,json}`。最终源提交已逐 blob 核对，root 封存运行日志已独立观察并保存 SHA；源码静态结论与运行结果分别登记。

## 2. 应用错误的唯一拥有与载荷合同

`erp_processes::{Error, Result}` 与 `erp_read_models::{Error, Result}` 均实际定义于各自私有 `errors` 模块，由 crate 根显式导出。二者不是旧 services 或彼此的别名。Process 通过自己的穷尽 `From<erp_read_models::Error>` 接收读模型错误，读模型无反向依赖。

各应用边界的 14 个变体与旧 Services 声明逐 token 等价，唯一允许的类型名称变化为旧本地 `ErrorCode` 改为 `erp_workflow::ErrorCode`。原 `#[error]` 字面、载荷类型、`#[source]` 和 `#[from]` 全保留。特别地，`RepositoryError(persistence_core::Error)` 原来没有 `#[source]`，迁后仍没有；不得以补充错误链为理由改变这个可观察合同。

19 个领域 Error 声明对输入保持 token 等价：17 个域各 12 个共同变体，identity 另有 `Rbac`，workflow 另有 `Rbac` 和 `Coded`。原逐域转换与新 `from_domain!` 均按相同变体移动原载荷。该宏不采用字符串分类、不丢弃 typed persistence error，也没有通配分支掩盖新变体。Process/RM 两个实际宏的 token 相同。

| 应用变体 | HTTP 分类 | typed/source 与恢复要求 |
| --- | --- | --- |
| `Internal(String)` | 500 `INTERNAL_ERROR` | 外部固定安全文案 |
| `NotFound(String)` | 404 `NOT_FOUND` | 保原业务消息过滤 |
| `ValidationError(String)` | 400 `INVALID_REQUEST` | 字符串，不产生 `fieldErrors` |
| `BusinessLogicError(String)` | 422 `BUSINESS_RULE_BLOCKED` | 保业务语义 |
| `ConflictError(String)` | 409 `CONFLICT` | 普通冲突不启动回执恢复 |
| `ReceiptDuplicate(persistence_core::Error)` | 409 `CONFLICT` | 原 typed source；Process 可恢复 |
| `TransientTransaction(persistence_core::Error)` | 409 `CONFLICT` | 原 typed source；Process 可恢复 |
| `Forbidden(String)` | 403 `PERMISSION_DENIED` | 保权限分支 |
| `Unauthenticated(String)` | 401 `UNAUTHENTICATED` | 固定重新登录说明 |
| `Logic(erp_core::Error)` | 422 `BUSINESS_RULE_BLOCKED` | 不借用 domain `class()` 降成其他状态 |
| `Rbac(String)` | 500 `INTERNAL_ERROR` | 不暴露策略细节 |
| `OutcomeUnknown(persistence_core::Error)` | 500 `OUTCOME_UNKNOWN` | 原 typed source；Process 可恢复 |
| `RepositoryError(persistence_core::Error)` | 通常 500 `INTERNAL_ERROR` | 仅第 4 节三项精确历史 DuplicateKey 例外 |
| `Coded(erp_workflow::ErrorCode)` | 原稳定码与分类 | 仅此变体委托稳定码 `class()` |

`Process::Error::command_may_have_committed` 与旧 Services 同符号 token 相同，仅 `OutcomeUnknown / ReceiptDuplicate / TransientTransaction` 返回 true。RM 不新增命令恢复能力。

## 3. 21 个工作流稳定码

唯一真实类型为 `erp_workflow::ErrorCode`。其 enum、完整 impl（包括 `ALL` 的 21 项顺序）、`as_str`、`class`、`retryable` 和 Display 与冻结输入的 Services 定义 token 相同。旧 `From<erp_workflow::ErrorCode> for services::ErrorCode` 的 21 个分支都是同名恒等映射，因此直接使用唯一 workflow 类型不会改变稳定码载荷。

HTTP 只在 `Coded` 分支使用 `class()`。`NotFound`、`Unauthenticated`、`Logic` 等直接变体继续按原 HTTP 404、401、422 映射，不以领域层的归类替换这些协议行为。

## 4. 唯一索引消息与三个历史例外

七个领域各自通过一个 `known_duplicate_index_message(&str) -> Option<&'static str>` 生产固定消息。所有 helper 均为精确 `match`，未知名称返回 None；不做 trim、大小写、前缀、后缀或子串匹配。各原私有 formatter 恰调用一次该 helper 并保留通用 fallback，领域 `From<persistence_core::Error>` 本身与输入 token 相同。

| 消息拥有域 | 活动索引数量 |
| --- | ---: |
| party | 4 |
| supplier | 2 |
| supply | 1 |
| contract | 1 |
| procurement | 2 |
| customer | 2 |
| workflow | 2 |

以上 14 项与 HTTP 下列 3 项合并，名称和消息逐项等于旧 Services 的 17 项表。完整名称及消息冻结在 JSON `helpers[].mapping` 与检查记录中。

- `uk_procurement_confirmation_lines_confirmation_line`
- `uk_procurement_confirmation_lines_active_confirmation_line`
- `uk_product_publication_revisions_publication_revision`

这三项没有活动索引/仓储生产者。当前实际 Rust 精确字面扫描仅发现：6 项应用 typed 载体判别、3 项 HTTP 历史消息拥有、10 项内联测试 fixture。原始 `rg` 命令、exit、输出和逐命中分类全部保存于 JSON，不将注释、相近名称或 fixture 误报为生产实现。

应用边界直接收到三项历史 DuplicateKey 时保留 `RepositoryError(original DuplicateKey)`，使 HTTP 能在最终边界提供历史 409 文案。此处是明确的载体调整，不得假称内部变体完全未变；普通回执恢复仍为 false。HTTP 对 Process/RM 的 RepositoryError 同时验证真实 `DuplicateKey` 变体与精确历史名称，满足二者才转交 HTTP 直接 persistence 转换。

其他应用 RepositoryError 一律保持 500，包括已包装的 active/unknown/nameless DuplicateKey、OptimisticLockingError、TransientTransactionConflict、CommitOutcomeUnknown 和 DatabaseError。19 个直接领域 RepositoryError 也保持 500，包含同名历史 DuplicateKey。直接 `From<persistence_core::Error>` 则按原独立合同映射 duplicate/optimistic/transient/unknown，不经过 Process 或 RM 中转。

## 5. HTTP 响应与安全消息

实际 `backend/apps/web-api/src/core/errors.rs` 的以下原实现与输入 token 相同：`IntoResponse::into_response`、`http_status`、`error_code`、`user_message`、`field_errors`、`retryable`、`user_message_or`、`user_message_is_safe`、`is_internal_code_token`、`TECHNICAL_MESSAGE_TERMS` 和 `USER_ACTION_MARKERS`。`Error` enum 仅 Coded 的拥有路径变化；String、&str、io 错误仍进入 Internal。

响应继续由实际 `ApiResponse` 产生 `status / errorMessage / code / retryable / data / success`；`fieldErrors` 仅在直接 HTTP validator 错误时存在。内部和底层技术消息不穿透安全过滤。瞬态事务的原技术消息仍在 HTTP 过滤成既有业务冲突说明。

RateLimit 四分支保持原响应：可给出等待时间的三项为 429 并设置实际 `Retry-After`；Unavailable 为 500、仍使用 `RATE_LIMITED` 且不设置该头。不得把统一 `retryable` 值重写为事务恢复分类。

## 6. 实际 HTTP 测试定义与计数

被审文件为 `backend/apps/web-api/src/core/errors/cutover_tests.rs`，由真实 errors 根以 `#[cfg(test)] mod cutover_tests` 注册。10 个异步测试只构造内存错误；每个 case 经实际 `From` 后进入 `assert_response`，调用实际 `error.into_response()`，断言 HTTP status、Content-Type、Retry-After 及反序列化后的完整 JSON 信封。黄金预期是独立字面值，不反向调用生产分类或文案方法计算预期。

| 实际测试 | 完整执行时响应 cases |
| --- | ---: |
| `all_nineteen_domain_common_variants_keep_http_contract` | 228 |
| `process_and_read_model_common_variants_keep_http_contract` | 24 |
| `rbac_errors_remain_internal_at_every_real_provider` | 4 |
| `all_workflow_codes_keep_http_contract_across_three_boundaries` | 63 |
| `duplicate_indexes_keep_seventeen_messages_and_exact_name_fallbacks` | 75 |
| `historical_repository_wrappers_are_special_only_at_application_boundaries` | 63 |
| `other_application_repository_wrappers_remain_internal` | 26 |
| `direct_persistence_non_duplicate_errors_keep_http_contract` | 4 |
| `validator_fields_are_only_exposed_for_direct_http_validation` | 3 |
| `rate_limit_variants_keep_body_status_and_retry_after` | 4 |
| 合计 | 494 |

494 是真实循环展开后的响应断言数量，不是 Rust test harness 的测试函数数量。Process/RM 的另外内联测试覆盖全部共同载荷、typed source、三个历史载体、普通近似名、validator 字符串，Process 另有 14 变体恢复分类和 RM bridge 验证。旧 Services 的 14 个原内联测试也全部保留原函数 token：13 个在 Process errors，历史 publication 文案测试移到实际 HTTP 文案拥有者。以上测试定义不替代 root 的执行日志。

## 7. 入口、补偿与索引验收

AppState 注入、Web 与 CLI 启动、专用审批响应与 WorkItemActionError 已由独立子审核到 52 个实际生产符号：51 项仅命名空间替换后 token 相同，另 1 项保留相同表达式的 rustfmt 匹配臂包装经明确核对；32 个原内联测试静态保留。详情及逐符号 token/hash 见子审工件。附件补偿的唯一 helper 及 10 个调用点保持原条件：仅 OutcomeUnknown 阻止补偿；ReceiptDuplicate 和 TransientTransaction 不因 Process 回执恢复能力而自动阻止补偿。两者属于不同合同，不能合并。

新增 `From<erp_read_models::Error> for WorkItemActionError` 只经 Process 的 14 变体穷尽桥，再进入原唯一 `From<erp_processes::Error>`。`ApprovalGenericWorkItemMutationForbidden` 仍只在原特殊分支产生一次 UUID 并构造 ApprovalHttpError，其余走统一 HTTP 错误。真实 `work_item_action_response` 的 Applied 写后详情 `await?` 使用该桥；Conflict 详情读取保持 `await.ok()`，继续忽略读取错误，不改变冲突响应。

索引注册必须按阶段 00 的原始 collection 级串行展开顺序核验。阶段 16 将 audit 等调用按领域聚合造成了继承顺序漂移，阶段 17 已有意通过 27 个窄 ensure 调用覆盖 19 个域进行修复。因此不得以原 19 个域根列表或“与阶段 16 同序”作为最终验收。独立核验已完成：阶段 00 实际 30 组展开为 158 次集合 create_indexes 和 3 个原 reconcile 步骤，共 161 步；Web、CLI、Process 测试和 RM 测试四个实际根的 27 次调用均逐步等于该 161 步顺序。158 个同位 IndexModel builder、95 个实际 create_indexes/named/unique/partial helper 及 3 个 reconcile 函数体保持 token 相同，协调步骤位置不变。阶段 16 的首个偏差发生在第 3 步，原 audit_logs 被 casbin_rules 占位；此项已恢复。两套独立解析结果还在 before00 及 after17 四入口逐位置核对 operation/collection/builder，全部 161 步一致；另一审查最初的 129 步为解析器漏尾逗号、尾 await 和两个 reconcile 的非完整计数，已明确作废。独立机器证明为 `/private/tmp/cutover17-index-sequence-review.json`，root 修复输入记录为 `/private/tmp/cutover17-index-order-correction.json`。

## 8. 交付与最终重绑要求

复验依次执行 `python3 /private/tmp/audit-cutover17-boundary-errors.py` 与 `python3 /private/tmp/aggregate-cutover17-boundary-error-review.py`。前者读取工作树及冻结输入并重建主审 JSON，后者验证并汇总独立入口及索引子审工件的哈希。两者只写 `/private/tmp`。最终绑定执行器为 `/private/tmp/bind-cutover17-b-reviews.py`。本次 32 个主审文件的实际 commit blob SHA-256 已与原记录相符，并记录 blob OID；不是仅替换报告提交号。

当前未发现错误边界的非授权生产语义漂移。阶段 00 索引展开、最终 commit blob 绑定及 root 封存日志观察均已闭合。索引顺序修复作为有意的前序漂移修复保留。Import 补充独立报告为 `/private/tmp/cutover17-import-projection-review.{md,json}`：11 个修复文件及 3 个直接 provider、32 项完整 token 检查全部满足；直接 Workflow 类型与动态路由的授权恢复单独登记。


## 9. 封存运行观察

本审查者未运行 Cargo、应用或数据库。以下为 root 提供并由本审查者读取的封存日志：

| 门禁 | 实际日志 | SHA-256 | 观察结果 |
| --- | --- | --- | --- |
| 库测试 | `/tmp/erp-cutover17-lib-tests-sealed.log` | `2fb05cc9803e1b527afb67067c28f606d2d785c58965ea75a9e89cd30fa54753` | 33 个 harness 汇总为 3655 passed / 0 failed / 68 ignored；10 个 HTTP 黄金测试及 3 个 Import 新投影测试均有实际 ok 行。 |
| Clippy | `/tmp/erp-cutover17-clippy-sealed.log` | `afe2e2111e7e30fdd3ecf2a38664b90e9f5b2b23e63940cb0020a2fca5cd7481` | root 报告 exit 0，日志有完成记录。 |
| Check | `/tmp/erp-cutover17-check-sealed.log` | `90380e0beeef1e7c0b8674e4011583d27231dd2726b97657a713241706df7266` | root 报告 exit 0，日志有完成记录。 |

494 个 HTTP case 由已审且已通过的 10 个真实测试循环展开计算；日志没有将它们冒记为 494 个测试函数。真实 MongoDB 验证仍为未执行。
