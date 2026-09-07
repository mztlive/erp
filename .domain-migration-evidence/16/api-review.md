# 阶段 16 API 分片交付合同

## 1. 输入与验收边界

- 唯一输入：`ed8015e28d36e5b9323b87e1a1fd5f63c122eb70`。
- 工作树：`/private/tmp/erp-domain-crate-16-supply`；After 已逐 actual blob 绑定冻结源码提交 `72a0c79a2261d33699b869329376e534edfb1ef4`，50 个文件 SHA-256 全部相等。
- 旧 source-map 18 文件、owned 5 文件共 23 文件已删除。删除前逐文件与输入 Git 内容比较，确认没有他人编辑后执行。阶段 14 的 execution 5 文件保留并改接供给 provider。
- 本分片不修改共享 Cargo/根模块/错误/HTTP/AppState，不执行 Cargo、MongoDB、历史 tests/**、commit；只对拥有的 Rust 文件执行 rustfmt。
- `/private/tmp/supply16-api-implementation.json` 记录输入 28 文件、557 个源码符号的 hash 和目的地、每个既有测试的前后 hash、50 个 After 文件及 hash。符号计数按该脚本实际扫描 fn/struct/enum/trait/const/type，不替代 root 的全阶段 source-map 核销器。
- 60 个既有测试全部有新目的地；After 68 个测试，新增 8，ignore 仍为 0。此处是源码扫描事实，测试通过数由 root 的统一门禁登记。

## 2. 公开构造与命令

| 责任 | 唯一出口 |
| --- | --- |
| 单域连接/能力列表 | `erp_supply::service::supplier_api::SupplierApiService::new(Database)` 的 `connection_list` / `capability_list` |
| 创建、固定命令、业务确认、能力更新 | `erp_processes::supply_governance::supplier_api::SupplierApiGovernanceProcess::new(Database)` 的 `create_connection` / `execute_connection_command` / `confirm_business_capability_requirement` / `update_capabilities` |
| 连接分页动作/详情/后台任务 | `erp_read_models::supplier_center::supplier_api::SupplierApiReadService::new(Database)` 的 `connection_list_for_actor` / `connection_detail_for_actor` / `connection_job` |
| 后台真实执行 | 原 `erp_processes::supplier_connection_execution::SupplierConnectionExecutionProcess::new(Database, Arc<dyn SupplierApiGateway>)` 和 `process_connection_job` |
| API 请求与本域 DTO | `erp_supply::dto::supplier_api::*`；保留原字段、serde、Validator、分页与排序 |
| 唯一外域 Job View | `erp_read_models::supplier_center::supplier_api::dto::SupplierConnectionJobView` |
| API Gateway / ClassifiedError | `erp_supply::ports::supplier_api_gateway::*` |
| 引用注册表/引用种类/解析事实 | `erp_supply::ports::supplier_reference_registry::*` |
| 默认失败关闭 adapters | `erp_processes::adapters::supplier_api::{UnavailableSupplierApiGateway,UnavailableSupplierReferenceRegistry}` |
| 共用八类失败事实 | `erp_supply::entity::failure::SupplierFailureClass` |
| 唯一双向 integration 映射 | `erp_processes::adapters::supplier_failure::{integration_class,supplier_class}` |

Process 与 ReadService 的 `with_rbac`、`with_reference_registry` 保留原注入方式。domain 只持数据库。Process/RM 暂用 `services::Result`，domain 使用 `erp_supply::Result`；本分片未新增错误类或兼容 Service。

ReadService 无外部 registry 注入时 `is_available=false`，RBAC 未注入时权限为 false；Process 默认引用解析仍调用实际失败关闭 adapter。连接详情在每个已授权动作之后才求 registry availability；不引入额外调用或提前求值。

## 3. 本域实际责任

| 文件 | 实际入口与责任 |
| --- | --- |
| `service/supplier_api/creation.rs` | `prepare_connection` 在供应商存在性确认后生成 ID / new；`persist_created_connection` 原 connection→capabilities；`persist_connection`、`persist_health_run` 原 CAS |
| `capability.rs` | `apply_capability_changes` 接收按值 CapabilityChangeSet；connection/version/非Active→capabilities→confirmations→classify→原 apply helper→更新逐项 CAS / creates ordered batch→connection 配置版本/CAS |
| `confirmation.rs` | `BusinessConfirmationInput` 只携本域命令、ID、摘要、actor ID；prepare 方法 connection/version→capability/version→new(now)→connection touch/CAS；persist 方法仅 confirmation create |
| `reference.rs` | `load_reference_target` 原 NoTransaction 预检；`apply_reference` 在 caller Executor 中重读/version/非Active，再原引用变更与 CAS |
| `status.rs` | `prepare_status_target` 先 connection/version 与全部本域影响读取；`apply_status_change` 接真实 support count，按 first blocker 求值并 enable/disable/CAS |
| `intent.rs` | `prepare_job_target` 任务 ID 之前的 connection/version/capability/blockers；`prepare_health_run` 在 support job new 之后构造原健康运行；原 create 和 receipt new/create 独立接口 |
| `command.rs` / `context.rs` | 保留三种原指纹、CommandIdentity、版本/形态错误、能力内存应用、固定权限名与安全投影；无 RBAC/审计/任务外域实体 |

原创建只要求供应商存在，不新增启用判断。原引用与能力的两个不同“连接启用”错误文案分别保留。原 deterministic ID、next_id、Instant::now 位置保持；没有增加事务失败恢复循环。

## 4. 生产组合顺序

| 原符号 | 必须保持的生产顺序与新链 |
| --- | --- |
| `create_connection` | validate→Prepared shape→supplier exists NoTransaction→domain new(ID/金额策略)→audit 构造→txn domain create connection/capabilities→audit→View |
| `execute_connection_command` | validate→permission→CommandIdentity→receipt replay→Prepared shape→原 7 动作分派 |
| `execute_reference_command` / `commit_reference_command` | `reference::execute_reference` 实际消费 `ReferenceCommand`：preflight 的 domain NoTransaction load/version/非Active→kind→registry.resolve 无 Executor→commit txn domain 再读/version/非Active→绑定/CAS→receipt |
| `execute_status_command` | txn domain prepare(connection/version/caps/confirmations/health/offering/order)→support jobs count→domain first blocker/state/CAS→receipt |
| `create_health_job` | txn domain资格→support job new(ID)→domain health run new→support job create→domain run create→receipt |
| `create_catalog_job` | txn 同原资格读取→support job new(ID)→job create→receipt；无 run |
| `confirm_business_capability_requirement` | validate→permission→fingerprint/key→NoTransaction confirmation replay；txn domain prepare 完成 connection CAS→audit 构造→confirmation create→audit create→原 result |
| `update_capabilities` | validate→permission→shape→fingerprint/auditID→NoTransaction audit replay；未命中则 txn domain classification/能力批量写/connection CAS→audit 构造/create→提交→完整 readmodel detail |
| `replay_command` | receipt 查询→fingerprint 比较→仅 job_id Some 时 NoTransaction support find_by_id→原 receipt result |
| `persist_command_receipt` | receipt new→audit new→`receipt::persist_receipt` 实际 `MongoReceiptWrite` receipt create→audit create→可选 job_no 同 Executor 查询 |

能力更新两处完整 detail 读取均保留。replay 的 connection_version 取当前 detail；正常结果的 version 取事务返回值，capabilities 取提交后 detail。确认 replay 仍使用原 saturating_add(1)，不重新加载连接。

## 5. 仓储 provider 分解

- Supply `SupplierApiRepository` 保留连接/能力/confirmation/health/receipt 的原专用查询及写入。全部原五集合名与 11 个索引键序/唯一性不变。
- `governance_job` 删除：Process 两根均调用 support `background_jobs().find_by_id`，原查询的 session/NoTransaction 保留。
- `governance_audit` 删除：能力幂等检查调用 audit `audit_logs().find_by_id`。
- `connection_job` 外域查询唯一放在 support `BackgroundJobRepository::find_supplier_connection_job`；空 whitelist 先返回 None，原 id/domain_job_id/type in + owned not-deleted 过滤不变。
- 原 connection_impact 分为供给 `owned_connection_impact` 和 support `count_active_supplier_catalog_jobs`。供给仍先读取 active offering projection，再 count open fulfillment；外层随后读取 pending/running/partially_succeeded catalog jobs。
- `SupplierConnectionOwnedImpact::with_active_sync_jobs` 消费真实 count，不伪造 0。ReadService 与 status Process 共用同一 support 查询，未复制过滤器。

## 6. 读模型与前序 execution

- list：domain page→capabilities-by-connections→supplier accounts→party/current revisions→原页面映射；actor 仍未使用，不新增列表内部授权。
- detail：connection→caps→confirmations→health(50)→offering→order→support jobs→metadata permission→confirmation permission→capability update permission→capability projections→逐动作 permission→registry availability/blocker→原 View。
- `supplier_connection_execution` 5 叶仅重接 supply/support/failure provider。health 原启动及结果事务、配置变化优先分支、connection 与 W29 交错、job/run/audit 写序保持。
- catalog 仍在启动事务前 load connection，结果事务不重读 connection、不增加技术配置校验或 health run。
- W29 failure 原 task→work item→audit 的 `MongoFailureWrite` 保留；实际 task class 与 owner_role 参数均经唯一 integration_class 映射。Supply enum 不拥有重试政策。

## 7. 测试与证据消费

保留 55 个原 API 内联测试及前序 execution 5 个真实 Port 测试，名称/hash 对照见 JSON。新增 8 个测试：

1. `all_failure_classes_round_trip_with_identical_wire_codes`：8 类双向映射、原 JSON wire 值。
2. `connection_job_filter_keeps_identity_and_type_scope`：support 真查询使用的 filter。
3. `active_catalog_filter_keeps_original_states_and_deleted_guard`：support 真计数使用的 filter。
4. `receipt_audit_and_job_read_share_executor_in_original_order`：生产策略三步同一个非零 Executor、原值传递。
5. `receipt_failure_stops_before_each_later_write_or_read`：各步失败停止、首错保留。
6. `synchronous_receipt_never_reads_a_background_job`：无 job_id 不触发第三查询。
7. `reference_resolution_stays_between_preflight_and_commit_revalidation`：生产 runner 三段值传递与时序。
8. `reference_failures_stop_before_resolve_or_commit_and_preserve_first_error`：三段逐一失败短路。

root 已报告统一 lib tests exit 0：36 packages、3594 passed / 0 failed / 68 ignored；strict clippy、domain boundary、permissions drift 均通过。上述执行者为 root，本分片未自行运行 Cargo。JSON 已记录报告来源和冻结提交逐 blob 对照。独立语义审核由 C 检查真实 provider 内部，不可仅凭替身轨迹认定数据库事务已验证。当前分片证据不声称真实 MongoDB、索引创建、并发恢复或外部供应商调用已验证。
