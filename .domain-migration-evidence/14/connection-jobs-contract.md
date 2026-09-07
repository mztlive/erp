# 阶段 14 C 实施验收合同

## 1. 固定输入与证据位置

- 实施树：`/private/tmp/erp-domain-crate-14-integration`。
- 输入：`453cd48082793b8f40e5afa37d5d225a747fd1b0`。
- 准备合同：`/private/tmp/integration14-jobs-contract.md` 与 `.json`。
- 准备合同 15 个源 SHA-256 与正式输入全部一致，无前序漂移。
- 函数对照：`/private/tmp/integration14-jobs-semantic-comparison.json`。
- 本分片不运行 Cargo、MongoDB、外部供应商请求，不提交，不修改历史 tests/**。统一门禁由 root 记录。

## 2. 实际文件与唯一入口

| 文件 | 实际责任 |
| --- | --- |
| `backend/crates/erp-processes/src/supplier_connection_execution/mod.rs` | `SupplierConnectionExecutionProcess::new(Database, Arc<dyn SupplierApiGateway>)`；唯一 `process_connection_job` 执行根；NoTransaction 加载、terminal 返回、原 job type 路由。 |
| 同目录 `health.rs` | 健康 start/finish 原事务函数；实际 HealthExecution adapter。 |
| 同目录 `catalog.rs` | 目录 start/finish 原事务函数；实际 CatalogExecution adapter。 |
| 同目录 `execution.rs` | 实际执行泛型 `execute` 与消费方 ConnectionJobExecutionPort；两类生产 adapter 都通过该函数完成 start→invoke→finish。 |
| 同目录 `failure.rs` | 原 settle_health_failure、原 task/WorkItem/audit 构造；实际 FailureWritePort 与 MongoFailureWrite 三写 adapter。 |
| `services/src/supplier_api/governance/jobs.rs` | 仅保留 connection_job、create_health_job、create_catalog_job、job_view。 |
| `services/src/supplier_api/mod.rs` | Service::new(Database)，删除已迁出的 gateway 字段；原 registry/RBAC 构造和注入不变；ErrorClass 新出口；digest 单符号 reexport。 |
| `governance/context.rs` 与 `governance/mod.rs` | load_connection 原体窄公开；digest 原体经两级单符号 reexport 公开；不公开整个子树。 |
| `governance/command.rs` | 原执行方法 rustdoc 链接指向流程层说明，原命令代码保持。 |

root 已有 AppState/HTTP 接线由 root 单独负责；C 不改这些共享文件。实际 type/method 与准备合同一致。新模块不增 RBAC/registry/object-read 参数。

## 3. 逐符号核销结果

下列 10 个原函数经忽略空白后，函数体与输入完全相同：

- 原位保留：connection_job、create_health_job、create_catalog_job、job_view。
- 迁入流程：process_connection_job、start_health_job、finish_health_job、start_background_job、finish_catalog_job、settle_health_failure。

另 3 个原函数按真实调用边界抽取：

| 原函数 | 唯一实际调用链及保留项 |
| --- | --- |
| process_health_job | execute(HealthExecution) → start_health_job 完整返回 → invoke 内 MonotonicInstant::now → 原 gateway.health_check → 原 elapsed u64 饱和换算 → finish_health_job。未提前读取时钟或把网关放进事务。 |
| process_catalog_job | execute(CatalogExecution) → 原 domain_job_id 缺失错误 → Service::load_connection(NoTransaction) → start_background_job 返回 → 原 gateway.catalog_sync → finish_catalog_job。配置只在原时点读取。 |
| persist_health_failure_task | 原确定性 task ID/字段构造 → B producer::error_work_item → 原确定性 WorkItem audit 构造 → persist_failure(MongoFailureWrite) 的 error task create → WorkItem create → audit create。三个原数据对象与同一个 executor 原位传入。 |

生产 MongoFailureWrite 不创建 session/transaction，三个方法分别仅执行原 repository create；策略内每步 `?` 保持首错停止。task/WorkItem/audit 的构造都仍在三写之前。

## 4. 外部消费者核销

下列 7 个文件中全部原函数，经过旧 integration 路径替换与忽略空白后，函数体与输入完全相同：

- services/src/work_item/facts.rs。
- services/src/workflow_compose.rs。
- services/src/supplier_fulfillment/gateway.rs。
- services/src/supplier_fulfillment/place.rs。
- services/src/supplier_fulfillment/cancel.rs。
- services/src/supplier_fulfillment/refund_result.rs。
- crates/erp-read-models/src/workbench/facts.rs。

仅更新 `erp_integration::entity::integration_ops`、`erp_integration::repository::IntegrationOpsExt` 的生产/测试导入。W29 close 仍由旧 workflow 自有 adapter 实现；W26 原工厂与 task 关联对象不替换为 B W29 工厂。没有旧 services→processes 依赖。

## 5. 测试保留与新增入口

C 相邻原测试 14 个全部保留，名字与准备机器合同相同；原 AppState readiness 测试 1 个由 root 单独记账。A/B 的原 116 个迁移测试不在此重复计数。

新增 5 个纯内联生产 Port 测试：

| 测试 | 验证合同 |
| --- | --- |
| gateway_is_between_committed_start_and_new_result_transaction | 启动提交后才 invoke，结果事务在 invoke 后开启；actor 与已启动事实传递保持。 |
| classified_gateway_failure_still_reaches_result_transaction_unchanged | ResultUnknown 分类、code、summary 原样交 finish，不由执行策略提前返回。 |
| execution_preserves_first_transaction_error_without_later_steps | start 失败不 invoke/finish；finish 失败保持 Conflict 文案及既有前缀轨迹。 |
| w29_failure_writes_share_executor_in_task_work_item_audit_order | 非零 TestExecutor(visits)；每步 data pointer 相同；原三写顺序与载荷值。 |
| w29_failure_stops_at_each_write_preserving_first_error | 三个位置分别失败；只出现截至失败步骤的调用前缀；保留首错。 |

HealthExecution/CatalogExecution 是执行策略的实际生产实现；MongoFailureWrite 是三写策略的实际生产实现。健康三分支与目录差异的证据为原 finish 事务函数体一致性核销，不把上述替身测试声称为数据库分支运行验证。

## 6. 已完成静态检查与验收限制

- 所有本分片 Rust 文件已定向 rustfmt。
- `git diff --check` 已通过。
- C 文件搜索旧 `entities::integration_ops`、`database::IntegrationOpsExt`、旧 producer 导入均为零；旧 SupplierApiService 无 process_connection_job 方法。
- 唯一 W29 工厂实际消费 `crate::integration_resolution::producer::error_work_item`，B 的 pub(crate) 模块与函数已落盘。
- 后续编译、clippy、全 lib 测试和边界脚本结果由 root 的真实日志填写。本文件不声明这些命令已通过。
- 真实数据库运行未验证；未执行外部供应商请求，不声明真实回滚、并发或外部成功。
