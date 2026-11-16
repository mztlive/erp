# FSY-11：第五阶段授权补证（R2 开发）

本轮接续 R1，实现 INT-AUTH-02 的剩余消费者回归。INT-AUTH-01、03 的原修复保留；独立 Review 检查本轮最终交付提交，任务的 done 仍以独立审查通过为前提。

## 版本与恢复依据

- 分支：`agent/agent/fsy-11-363c002c07f7`；本轮起点：`c640fcf8710e5f70d4febfe293e77547014dcd21`。
- 已重新读取本事项、父事项及最新评论，确认没有本事项未处理的独立 Review 问题；本轮继续处理 R1 保留的 INT-AUTH-02。
- 共享输入：FSY-9 的 S-01/S-02 提交 `f9eee0afbe7e32757764ec22899484e27d564156`，本分支承接为 `808de7e9f`；R1 已有 FSY-7 恒拒绝凭证夹具，本轮没有重复移植。
- 共享交付完整版本：`96843a78fda1a2b7151179a00921c0546bf6933c`。FSY-9 复审报告 `81a48652c9f05bf5981e40ae50ad1fef31738663` 明确核对 S-01 的真实 require/query/validate 夹具与 S-02 摘要符合本任务边界；该事项因另一个管理权限上限测试问题 IDN-03 退回分析，并不撤销已交付的共享能力。
- 引用的共享独立验证证据：identity 默认库 580 passed，test-support 库 593 passed，均为 FSY-9 在上述准确版本的实现和独立审查记录；本轮没有执行 identity 库测试，也不将其计入本域测试数量。
- 原一次唤醒已触发并自动关闭；本轮完成后没有继续等待同一依赖的需要。

## 实现与影响边界

新增 crate 内部 `repository/integration_ops/reader.rs::IntegrationReader`，将两类分页、历史处理人 ID、当前页最新决定及两类根事实读取放在同一个窄持久化边界。生产 `Database` 实现逐项委托原仓储；原仓储的模型/阶段守卫、过滤、分页和计数实现均保留。

两类公开列表仍由原入口开启快照事务、规范化参数并传入原 executor。内部步骤保持真实公共权限常量、`resolve_list_scope`、固定 Mapping、响应投影及实际 `AuthorizationService::validate`。差异页依然先补齐决定、再最终复验。公开的 `require_decide` 仍读取真实根 ID/base.version，构造精确 Task 后调用真实 require。

测试只在数据读取边界提供实体/投影行和故障。`IntegrationFacts` 的真实 owner/org/assignee/version 转换也执行生产实现，未由替身提供“允许”结论。连接替身 Facts 的工厂只在本 crate 的 cfg(test) 下可用；生产绑定始终使用 Database，没有增加对外注入接口。

本轮没有重建身份设施、复制权限算法、放宽 Builder 或改动全局政策；没有修改 server 装配、HTTP/worker 工厂、编排/读模型入口或第六阶段边界。公开 Rust 用例签名及 DTO、数据库结构、索引、权限分配均无新增变化。

## INT-AUTH-02 逐场景证据

本轮新增 17 项库单元测试；以下测试名均在 `erp-integration` 内，测试不连接真实 MongoDB。

| 场景 | 实际入口与断言 |
| --- | --- |
| 两类列表成功 | `service::error_task::authorization_tests::error_page_runs_real_query_and_validate_with_the_list_permission`、`service::reconciliation_difference::authorization_tests::difference_page_runs_real_query_projection_and_final_validate_with_list_permission`：只授予实际列表权限并绑定该查询，经真实 query，检查 owner 条件、模型、页/总数、范围版本、业务筛选传递及原凭证在交付后失效；差异历史筛选先于根页，最新决定在最终验证前读取。 |
| 详情权限不能代替列表资格；身份不匹配 | `error_list_rejects_detail_only_missing_permission_and_other_identity_without_page_reads`、`difference_list_rejects_detail_only_missing_permission_and_other_identity_before_reads`：无权限、只有 detail、其他账号分别返回 Forbidden，业务读取轨迹为空。 |
| 空正向范围 | `error_list_empty_role_scope_retains_a_deny_plan_despite_company_personal_limit`、`difference_list_empty_scope_keeps_deny_filter_and_final_validation`：个人 company 上限不能产生正向角色范围，原 CompiledAccess 保留恒假条件，仓储收到拒绝计划，返回空页且仍执行真实最终 validate。 |
| 失败停止及最终失效 | `error_list_discards_pages_on_storage_failure_or_read_stage_change`、`difference_list_discards_pages_on_page_or_latest_failure_and_changed_stage`：页/决定读取故障保留错误，后续步骤停止；阶段变化使真实 validate 拒绝整个响应。 |
| 范围版本变化 | `error_list_rejects_changed_scope_version_before_reading_a_page`、`difference_list_rejects_changed_scope_version_before_history_or_page_reads`：不匹配版本返回 DATA_SCOPE_CHANGED，历史/页读取均未启动。 |
| Facts/Mapping 一致 | `repository::authorization::tests::actual_owner_facts_and_query_mapping_agree_for_both_resources`、`actual_organization_facts_and_query_mapping_agree_for_both_resources`：两类真实实体经生产 Facts 转换，实际对象 require 对本人/组织内允许，对他人/组织外拒绝；真实 query 的字段条件对应同一归属，缺根拒绝。 |
| 精确差异决定允许 | `service::access::tests::direct_decision_uses_real_difference_identity_version_and_assignment`：实际决定步骤先读根，再通过真实 Facts 转换和 require 校验真实根版本 7、实际指派与决定资格。 |
| 精确差异的指派及功能拒绝 | `direct_decision_rejects_another_handler_even_with_company_scope`、`direct_decision_cannot_use_list_detail_or_another_actor_instead_of_decision_qualification`：company 范围不替代指派；list/detail 或其他账号不能代替决定资格；功能拒绝后不再加载任务 Facts。 |
| 精确差异缺事实/版本冲突/取证故障 | `direct_decision_rejects_missing_root_facts_binding_and_incomplete_facts`、`direct_decision_preserves_exact_version_conflict_and_facts_storage_error`：缺根、缺绑定、缺完整事实集、事实版本与根目标版本不符、事实仓储故障分别保留拒绝或原错误。 |

R1 已有的缺 Mapping/身份设施故障、仓储逐字段模型错配、其他 executor、写入阶段推进、事务重启、会话暴露与过期守卫回归继续运行。此前 5 项交付顺序替身测试也保留，本轮另增加实际 AuthorizationService 的消费者证据。

## 回归敏感性及验证记录

临时变异仅用于核对新测试确实保护生产调用，完成后逐字节恢复源码。变异命令均为 `cargo test -p erp-integration --lib --locked <下表测试过滤>`，同时取消 ERP_TEST_MONGO_URI 并设置本轮独立 CARGO_TARGET_DIR。三次执行均退出 101，由实际测试断言失败，脚本在每次执行后检查源码与原字节一致。编译失败不计作变异被行为断言捕获。

| 临时变异 | 测试过滤 | 实际结果 |
| --- | --- | --- |
| 错误任务列表改传 ERROR_TASK_DETAIL | `service::error_task::authorization_tests::error_page_runs_real_query_and_validate_with_the_list_permission` | 0 passed / 1 failed / 207 filtered；真实入口因缺相应 Mapping 拒绝，成功页断言捕获错误。 |
| 精确差异目标版本改为 stale | `service::access::tests::direct_decision_uses_real_difference_identity_version_and_assignment` | 0 passed / 1 failed / 207 filtered；真实 require 返回“任务版本已改变”，成功授权断言捕获错误。 |
| AuthorizedPage 的最终 validate 改为直接成功 | `service::reconciliation_difference::authorization_tests::` | 3 passed / 3 failed / 202 filtered；成功页、空范围交付及阶段变化测试分别捕获复验缺失。 |

本轮最初库测试为 206 passed、2 failed：两项差异测试把 `QueryIds` 的输入写成数组，按现有字符串合同修正。共享全局 Cargo 目标目录还被其他 worktree 的旧 identity 产物覆盖，使第一次变异尝试在编译阶段找不到新夹具；该结果被拒绝计入证据。后续使用 APFS 依赖缓存副本作为本轮独立构建目录，并更新本 worktree 源码 mtime 以重新编译当前 workspace 依赖，避免交叉覆盖。

本轮只运行 `env -u ERP_TEST_MONGO_URI cargo test -p erp-integration --lib --locked` 及其库测试过滤。使用独立目录时仅额外设置 CARGO_TARGET_DIR，没有运行其他 crate 的测试、集成、examples、真实 MongoDB/S3/外部服务测试、server/workspace 编译、clippy all-targets 或 lib 通过后的 cargo check。

最终恢复版完整库测试：**208 passed、0 failed、0 ignored、0 filtered out，退出 0**。`cargo fmt --all -- --check` 与 `./scripts/check-domain-boundaries.sh --cutover` 均退出 0，后者含领域及审计边界。最初边界门禁发现 Service 测试里的 BSON 断言违反层次约束，已将其移至仓储测试辅助模块，没有放宽门禁。

全仓体积门禁仍退出 1：**59 项已有超限，涉及 54 个文件**。逐文件核对全部与本轮起点字节相同；本轮 14 个变更 Rust 文件按原门禁检查均 0 项问题，未提高阈值或增加排除项。完整测试、变异、格式、边界、体积及基线差异记录作为验证附件交付。已有 proc-macro-error2 的 future-incompatibility 提醒保留。

## 证据限制与问题处置

内存读取返回可控的已分页数据，断言仓储实际收到生产编译的授权计划、业务过滤及原执行器；它不执行 Mongo 聚合引擎，不证明真实数据库分页结果、事务回滚、并发或事务外撤权。正向/拒绝对象检查、目标分派、Facts 转换、授权编译与交付验证执行真实生产代码。测试中两次读取之间改变版本仅验证精确目标冲突的错误传播，不宣称真实快照会观察到事务外版本变化。

- INT-AUTH-01：原修复保留，本轮补充了真实 query/validate 的成功和失效交付证据。
- INT-AUTH-02：共享输入已可读取，剩余列表/事实/精确决定矩阵补齐，供独立审查核销。
- INT-AUTH-03：R1 已删除的未使用白名单 helper 保持删除，无新增调用方。

开发交付后应按本事项约定进入 `in_review`，交后端审查读取准确提交；唤醒时 blocked 步骤显示的通用 done 命令不能代替事项明确要求的独立 Review。无需数据库迁移、合并或部署。
