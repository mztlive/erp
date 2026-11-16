FSY-10 第 1 轮实现交接。范围为恢复 erp-import 的独立库测试，以及补齐三个纯功能读取入口的授权行为证据。独立 Review 结论以事项中的审查交接为准。

## 版本与修复范围

| 项目 | 记录 |
| --- | --- |
| 实现分支 | `agent/agent/fsy-10-3a63dbae5548` |
| 本轮起始提交 | `e792c9bb629497328fe5c8692b60cb36c50d421f` |
| 分析基准 | `77f96e3fd6a225d2b67aaa6ba8c299412eaaee45` |
| 分析报告原提交 | `359397dac35d67388d7834706619531887571c89` |
| 本分支承接分析报告 | `186a53df0f2f91260e17aeb813eeb765ab688f0c`，仅引入本事项的分析文档 |
| 本域实现与测试提交 | `7c373711c3144909a83ddd79f3fcac8d092ca8e9` |
| 共享账号限制扩展 | `327c1c7f4b4eaf7a821b57a2b04068de73b13cdf` |
| 最终交付 | 包含本实现报告的完整 SHA 见 FSY-10 最终交接评论；该文档提交不改变已测代码 |

本轮核对了分析报告、父事项协作记录和同族交付。实现开始时 erp-import、identity 和 persistence-core 的相关代码与分析基准没有差异。本域没有前轮独立 Review 遗留问题。

## 逐问题处置

**FSY10-01 / P2 · 已修复并验证**：在 erp-import 的 dev-dependencies 显式启用 `persistence-core/test-support`。普通依赖、workspace 依赖配置和生产 Executor 封装保持原合同；独立命令无需依赖其他包偶然合并测试 feature。

**FSY10-02 / P2 · 已补齐行为证据**：将三个查询的授权、筛选和结果构造保留在私有生产步骤中，公开入口仍通过 `with_transaction` 调用这些步骤。步骤只允许替换仓储 I/O 回调，单测执行真实 `AuthorizationService::require(Operation)`，不在测试中复制授权或业务编排，不为 Repository 增加一层公共接口。

详情步骤直接调用已有 `BulkJobFactsPort`。它在成功找到批次之后才查询任务 ID；`None` 与端口失败分别验证，避免以缺省任务 ID 掩盖错误。原先依靠不可用数据库地址的授权故障测试改为完全内存的窄替身，无须构造 MongoDB Client。

本域错误转换新增具体 identity 错误的分类和消息断言。分类沿用既有合同：Forbidden / Unauthenticated → Forbidden；ValidationError → BusinessRule；ConflictError → Conflict；Internal / Rbac → Internal。没有修改生产错误枚举或跨域恢复语义。

## 共享测试支持的准确来源

先核对 FSY-9 的 S-01 方案、FSY-5 / FSY-7 的正式共享交付及同族运行。为避免依赖其他 worktree 未提交内容，本轮采用先发布的 FSY-5 夹具，而非复制 FSY-9 的开发文件；必要扩展已先在父事项 FSY-2 的 FSY-10 线程记录边界。

| 来源 | 本分支承接 |
| --- | --- |
| FSY-7 `6e25bc4c532e9bc478e363531c46f1a2df539f69`，经 `6cb24a596d01bcd534d84a3dbf07b63931bbd516` 复用 | `e7cab5a17db20909f9291cde3b57a999b82a2463`，保留恒拒绝的仓储守卫夹具；本域成功授权不使用它 |
| FSY-5 `12b1cf081bc7e0c42e478d290e40c7ec17f79ce4`，经 `1c144a9bbd95f2b31b64d03e6445c8fdedd90abd` 复用 | `ce2c130d353b566f926766e84f756ea454da0b6c`，AuthorizationFixture、expire_access 与既有范围摘要 |
| 本任务的最小扩展 | `327c1c7f4b4eaf7a821b57a2b04068de73b13cdf`，仅给既有 AuthorizationFixture 增加可选 `.actor(...)`，不另造授权工厂 |

原夹具允许任意调用账号，不能模拟身份设施返回 None。`.actor(...)` 只收紧可解析账号的 ID 和 AccountKind，其他账号返回身份缺失；缺省行为兼容已有消费者。共享内联测试通过真实 permissions / require 验证匹配账号允许、错误 ID 或账号类型拒绝。本域三个入口分别使用该选项测试身份缺失后的零业务读取。真实账号 / 角色存储行为仍由生产设施负责，本轮不宣称对真实数据库身份失效做了验证。

测试夹具与账号限制全部位于既有 test-support 隔离之内；未放宽生产 Builder 或引入生产默认放行。保留 FSY-5 / FSY-7 的原有交付；FSY-9 的 IDN-01/02 和 S-01 后续整合仍由其任务负责。

## 逐入口行为证据

下表中的私有函数与同名公开方法位于 `src/service/legacy_import/query.rs`。公开方法只负责事务及真实仓储回调接线；表中行为测试执行其调用的私有生产步骤。

| 入口 / 固定权限 | 允许路径 | 拒绝、边界与失败路径 |
| --- | --- | --- |
| batch_list / BATCH_LIST | 只授当前动作即可返回批次投影、total、page、page_size；检查归一化筛选、排序和原 executor | 缺权、只授详情或行列表、身份未登记、身份设施故障、NoTransaction、ReadExecutor、撤销快照时回调零调用；读取失败传播；空页保留 total |
| batch_detail / BATCH_DETAIL | 只授详情即可读取批次，再按 batch_no 读取任务 ID；任务可存在或不存在；批次和任务使用同一 executor | 缺权不探测批次；非快照拒绝；批次不存在或读取失败不调用任务端口；端口失败不交付半成品详情 |
| row_list / ROW_LIST | 只授行列表即可先核对批次，再读取固定 batch_id 的页；验证筛选、返回投影及分页，两个步骤沿用原 executor | 缺权不探测批次或行；非快照拒绝；批次缺失 / 读取失败停止后续行查询；行查询失败传播；空页保留 total |

两个列表测试分页校验先于身份取证，三个读取都测试身份故障先于业务读取。测试中的 ExecutionBoundary 来自回调使用的 executor，并在后续步骤及调用结束时对调用方原 executor 核验，防止替换执行器或在步骤之间推进阶段而没有被测试察觉。

对应内联单测包括：

- `missing_or_other_read_permission_never_reaches_business_reads`、`unresolved_actor_never_reaches_business_reads`、`identity_failure_never_reaches_business_reads`。
- `non_snapshot_and_revoked_executors_are_rejected_before_reads`。
- `batch_list_delivers_page_and_filter_using_the_original_executor`、`rows_deliver_page_with_batch_filter_and_the_original_executor`。
- `detail_delivers_job_identity_or_none_using_the_batch_executor`、`detail_missing_batch_and_read_failure_stop_before_job_lookup`、`job_port_failure_is_propagated_instead_of_returning_an_incomplete_detail`。
- `row_reads_stop_when_batch_is_missing_or_batch_lookup_fails`、`list_read_failures_are_propagated`、`out_of_range_pages_keep_totals_and_requested_paging`、`invalid_paging_stops_before_identity_and_reads`。
- `error::tests::identity_failures_keep_error_classes_and_messages`。

## 授权、装配与责任边界

三个入口仍直接引用公共 legacy_import 常量并使用 Operation 目标。它们是固定 Function，不产生 CompiledAccess，也没有对象范围或动态准备依赖，继续执行方案 §3.5.2 的 `require → 原快照完整读取 → 事务成功后交付`，无须额外 query / validate 或 legacy_import Facts / Mapping。

真实仓储、分页及计数继续使用传入 executor；没有移动授权、归一化、NotFound 和任务查询的先后顺序。`with_transaction` 在回调成功后先提交再交付结果；该项本轮通过代码阅读核对，未执行真实事务。

写入、确认、内部导入编排仍归 FSY-25，工作项复用 work_item 绑定。共享错误恢复合同仍归 FSY-3；本轮没有修改其文件。HTTP、worker、Application、服务工厂和 server 全量构建继续归第六阶段。

本轮不改变公开业务方法签名、权限编码、DTO 序列化、数据库集合 / 字段 / 索引或既有授权政策，无数据迁移要求。

## 验证结果

所有编译和行为测试限于获准的库单元测试。首次开发编译因尚未承接共享 API 的 E0432 退出 101，测试未运行；承接夹具后首轮运行 92 通过、1 失败，原因是新测试错误地假定 MongoError::custom 的 Display 包含自定义源。改用驱动的 get_custom 检查原始源后，完整原命令通过；该修正未改变业务错误处理。

| 命令（backend 下） | 结果 |
| --- | --- |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-import --lib --locked` | 退出 0，93 passed、0 failed、0 ignored |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-identity --lib --locked --features test-support` | 退出 0，586 passed、0 failed、0 ignored；覆盖复用夹具与本轮 actor 限制 |
| `cargo fmt --all -- --check` | 退出 0 |
| `env -u ERP_TEST_MONGO_URI cargo clippy -p erp-import -p erp-identity --lib --features erp-identity/test-support --locked -- -D warnings` | 退出 0；只检查两个库，未使用 all-targets |
| `PYTHONDONTWRITEBYTECODE=1 ./scripts/check-domain-boundaries.sh --cutover` | 退出 0；领域 / 审计边界通过 |
| `./scripts/check-rust-size.sh --json` | 退出 1，59 项既有超限；均位于与起始提交字节一致的文件，本轮修改文件无命中 |
| 门禁原函数 `rust_size_gate.scan_source` 对三个修改的 Rust 文件逐个检查 | 退出 0，无体积问题、无解析问题 |
| `git diff --check` / `git diff --cached --check` | 退出 0 |

两个库测试及 lib Clippy 均只有第三方 proc-macro-error2 的 future-incompatibility 提示，未导致失败。原始验证输出随事项交接附件交付。

未新增、修改或执行集成测试、examples、真实 MongoDB / S3 / 外部服务测试；未执行 cargo check、server / workspace 编译或全量 clippy。真实数据库分页、总数、快照 / 提交 / 回滚、并发撤权、生产身份设施和最终 HTTP 接线不在本轮验证结论内。

## 审查接续

审查应读取最终交接评论给出的准确提交；该版本包含必要共享测试支持、本域实现和测试、原分析报告及本实现报告，不依赖本 worktree 的未提交文件。Review 请逐项复核 FSY10-01 / FSY10-02；如有待处理问题，按既有工作流退回分析后继续修复。本轮交实现成果，不作独立 Review 已通过、PR 已合并或已部署声明。
