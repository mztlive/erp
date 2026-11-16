FSY-10 第 1 轮分析：erp-import 第五阶段授权实现检查。结论为 **1 项已复现的库测试编译问题、1 项授权行为验收证据缺口**，均待后端开发处理；尚未独立 Review。本轮仅提交分析报告，不修改业务、依赖声明或共享装配。

## 版本、目标与边界

| 项目 | 本轮记录 |
| --- | --- |
| 日期 / 环节 | 2026-11-16，todo / 后端分析，第 1 轮 |
| 分支 | `agent/agent/fsy-10` |
| 实际分析与测试基准 | `77f96e3fd6a225d2b67aaa6ba8c299412eaaee45` |
| 事项最初说明中的基线 | `cc248977e7d6b2b73cc7a935f3897b93ea389a4f`；本轮以实际工作树基准为准 |
| 报告交付提交 | 完整 SHA 记在 FSY-10 本轮最终交接评论；本文件随该提交交付 |
| 方案 | `backend/crates/domains/erp-identity/docs/authorization-service-improvement.md` §3.1、§3.2、§3.3、§3.5、§3.6、§3.7.4、§4.1.5 |
| 规范 | 已读仓库根 `AGENTS.md`、`backend/AGENTS.md`、本 crate README、`rust-coding-standards` 及其测试审查要求 |

目标是验证历史导入批次和行读取的入口资格、事务交付、仓储过滤、后台任务事实依赖及完整装配责任，并追踪写入和内部导入能力的实际拥有方。第五阶段的入口和完整业务绑定属于本次范围；HTTP、worker、Application、服务工厂切换及 server 全量编译属于第六阶段。

不修改跨域正式导入、工作项授权、共享政策、恢复合同或数据库模型。必要修复限于恢复本 crate 的独立库测试能力，以及补齐本域授权行为证据。任何确有必要的共享测试设施改动，先核对 FSY-8 / FSY-9 的最新交付，按下文协作边界处理。

## 已确认的执行合同

1. 本域只有三个受保护的 Service 读取入口：`batch_list`、`batch_detail`、`row_list`。`LegacyImportService::new` 仅保存数据库、后台任务端口和共享 `Arc<AuthorizationService>`，不读取数据、不授予权限，见 `backend/crates/domains/erp-import/src/service/legacy_import/mod.rs:16`、`:35`。
2. 三个权限属于 identity 的 **Function** 固定动作，不是按人员或记录授权过滤的查询。依据是 `backend/crates/domains/erp-identity/src/entity/access_control/scope_spec/catalog/functions.rs:179`、`:181`、`:184`，以及 `scope_spec/catalog.rs:410` 的 Function 登记。`rules.rs:197` 只允许 Operation 目标；`require.rs:33` 拒绝没有快照事务边界的 Operation；`resolve.rs:48` 查询当前身份和启用角色，缺完整资格返回 Forbidden。
3. 方案 §3.5.2 明确规定，纯 RBAC 读取执行 `require(Operation) → 同一快照内完整读取 → 事务成功后交付`，无需 Mapping，也不重复读取身份资格。本域没有 `CompiledAccess`，不能机械要求补 `query/validate`、对象 Facts、人员范围或动态依赖修订。
4. 三个公开入口均通过 `Transactional::with_transaction` 调用私有快照步骤，结果在事务成功后返回。基础事务使用 `ReadConcern::snapshot()`，回调成功后先提交再返回结果，见 `backend/crates/foundation/persistence-core/src/transaction.rs:49`、`:146`。本轮仅静态检查该设施，未执行真实数据库事务。
5. 功能资格允许读取满足业务筛选的全体导入记录；代码没有“仅本人批次”或“必须同时拥有详情菜单资格”的规则。不得自行新增上述限制或把无人员范围过滤判为绕过。

下表中未带 crate 前缀的 `service/`、`repository/`、`ports/`、`entity/` 路径均相对于 `backend/crates/domains/erp-import/src/`。行号均对应本轮源代码基准。

## 逐入口检查表

| 入口 / 实际调用方 | 权限、目标 | 授权与交付位置 | Facts / Mapping / 装配 | 测试及结论 |
| --- | --- | --- | --- | --- |
| `LegacyImportService::batch_list`，`service/legacy_import/query.rs:33`；HTTP `http/handler/legacy_import/mod.rs:37` 是待切换调用方 | 公共 `legacy_import::BATCH_LIST`，`legacy_import_batch:list`，Operation | `query.rs:41` 开事务；`:57` 参数校验；`:58` require；`:63` 查询并计数；`:66` 组装结果；外层等待事务成功 | 仅当前身份设施；不需要对象 Facts / Mapping。共享授权实例由服务构造参数注入 | 现有设施故障单测覆盖此私有步骤；没有正常允许、缺权和非快照行为证明。静态合同符合，证据缺口 FSY10-02 |
| `LegacyImportService::batch_detail`，`query.rs:80`；HTTP `http/handler/legacy_import/mod.rs:74` | 公共 `legacy_import::BATCH_DETAIL`，`legacy_import_batch:detail`，Operation；ID 仅用于后续业务读取 | `:84` 开事务；`:100` require 先于 `:103` 查批次；不存在返回 NotFound；`:109` 在同 executor 内补后台任务 ID | `BulkJobFactsPort` 是领域消费端口，不是 identity `AccessFactsPort`。真实 adapter 位于 server `wiring/import.rs:49`，只返回后台任务 ID | 故障测试证明身份设施失败后未探测批次；允许返回、NotFound 顺序和后台端口传播尚未由本域授权行为单测证明，归 FSY10-02 |
| `LegacyImportService::row_list`，`query.rs:125`；HTTP `http/handler/legacy_import/mod.rs:137` | 公共 `legacy_import::ROW_LIST`，`legacy_import_row:list`，Operation | `:135` 开事务；`:152` 校验参数；`:153` require；`:156` 校验批次存在；`:159` 固定 batch_id；`:169` 查行及计数；外层事务成功后返回 | 无独立授权 Facts / Mapping；使用原 executor 读取批次、页数据和总数 | 当前故障测试覆盖私有步骤；正常缺权、错误权限替代、非快照及结果传播待补，归 FSY10-02 |
| `LegacyImportService::new`，`service/legacy_import/mod.rs:35` | 无独立权限，构造器不构成业务读取 | 无 I/O；持有传入的共享服务，未自建临时授权服务 | `Arc<AuthorizationService>`、`Arc<dyn BulkJobFactsPort>` | 不适用入口鉴权测试；应用工厂注入切换属于第六阶段 |
| `batch_view_of`，`query.rs:207` | 非公开业务入口，`pub(super)`；依赖调用它的已授权读取 | 只有本域已授权详情调用；后台任务查询沿用原 executor | 原端口返回错误即传播；缺少任务记录可正常返回 None，不把端口故障降级成 None | `FailClosedBulkJobFacts` 单测已覆盖未装配错误，见 `ports/bulk_job.rs:62`；同 executor 成功链尚未覆盖 |

本域没有公开的独立计数、汇总、导出或流式导出 Service，因此没有额外遗漏的交付入口。分页数据与总数统一由 `repository/legacy_import/projected.rs:28` 执行：`:43` 以业务 filter 查询投影，`:44` 用同一 filter、同一 executor 计数。批次和行分别由 `repository/legacy_import/mod.rs:235`、`:412` 接入。行列表的批次约束不会被外部筛选覆盖。上述内容是静态代码证据，不代表真实 MongoDB 分页已验证。

## 仓储、纯规则及跨域调用责任

仓储是可组合的数据访问能力，不是自动完成授权的公共业务入口。以下能力必须由拥有用例的调用方先完成授权，并传入原 executor；不向每个仓储追加 identity 查询，也不恢复旧政策登记层。

| 本域能力 | 实际调用方 / 授权前置条件 | 本轮判断 |
| --- | --- | --- |
| `search_legacy_import_batches`、`search_legacy_import_rows`、通用 `find_by_id` | 本域三个 Service 入口，见上表 | 已按同事务纯功能读取合同接入 |
| `search_legacy_import_confirmations`、`find_by_work_item`、`find_by_batch_scope_trial`、`list_by_batch`，`repository/legacy_import/mod.rs:563` 起 | `erp-data-import::ImportApplyProcess` 的确认读取、创建、完成、回放；`confirmation_query.rs:65` 先 require CONFIRMATION_LIST，`:78` 后查确认并在原 executor 补姓名及工作项 | 本域负责过滤和事实；编排中的任务投影、当前责任和回放由 FSY-25 负责，本轮未验证其全部行为 |
| `create_batch_with_rows`，`repository/legacy_import/mod.rs:735` | 唯一业务调用位于 `erp-data-import/src/import_apply/create_batch.rs:113`；入口 `:71` 开事务，`:86` 先 require BATCH_CREATE | 不另建本域写 Service；仓储本身不构成事务边界，文档 `:720` 明确必须收到事务 executor |
| `find_by_batch_no`、`find_rows_by_batch_ids`、通用批次 / 行 CRUD | 创建、应用与执行编排，分别在 BATCH_CREATE、BATCH_APPLY、EXECUTION_COMMAND 检查后读写 | 保留原状态、版本、幂等与 CAS 职责；完整写入验证属于 FSY-25 |
| `apply_row_scope`，`repository/legacy_import/apply_scope.rs:49` | `erp-data-import/src/import_apply/batch.rs:111` 先 require BATCH_APPLY，`:127` 读取运行任务，`:129` 再读取指定批次内行 | 本域强制行 ID 与 batch_id 同时匹配；缺失 ID 由编排拒绝。无独立授权 Mapping |
| `list_failed_by_batch`、`persist_failed_retry_rows`，`repository/legacy_import/failed_retry.rs:35`、`:61` | `erp-data-import/src/import_apply/execution.rs:125` 入口授权后的命令步骤，`:241`、`:247` 调用 | 固定批次、失败状态、未删除过滤；写回用同 executor 的 CAS，不另开事务 |
| `persist_invalidated_confirmations`，`repository/legacy_import/supersede_batch.rs:35` | `erp-data-import/src/import_apply/supersede.rs:94` 及执行命令链 | 仅写本轮 replacement_id 对应的失效确认；政策与原子边界归编排 |
| `ImportCommandReceiptExt::import_command_receipts`，`repository/command_receipt.rs:20` | `erp-data-import/src/import_apply/complete.rs`、`execution.rs`；回放分别先检查 CONFIRMATION_COMPLETE / EXECUTION_COMMAND，见 `complete.rs:91`、`execution.rs:97` | 独立业务回执，不从展示审计恢复。纯回执身份 / 结果规则不是账号授权依据 |
| 批次事实 / 工作台摘要读取 | `erp-read-models/src/workbench/authority/command.rs:85`、`workbench/operational_brief.rs:36`，仅按调用方目标和 executor 读取事实 / 展示字段 | 调用方的工作项授权和输出粒度由 FSY-38、FSY-21 负责；本轮未验证完整工作台查询，不将事实 loader 自身无 actor 判为缺陷 |
| 对账关联对象编号读取 | `erp-read-models/src/integration_center/repository/object_labels.rs:24`，只处理已授权 DifferenceView 并沿用 executor，`:79` 取导入批次编号 | 属于跨域投影的前置授权合同，完整可见性由 FSY-38 / FSY-11 检查，本轮不作已通过结论 |
| DTO、实体、索引入口 | 纯输入规范化、状态机、确认矩阵、稳定幂等身份、结果不变式及数据库结构登记 | 不需要强行增加 actor 或 identity 授权；这些方法不对外授予账号访问能力 |

本域未发现业务侧 `AuthorizationService::new`、旧 PolicyRegistry、独立查询编译、调用方 AccessSubject 选择、动态解析内置权限或重复动作白名单。该项以本域源码检索和三个完整调用链阅读为依据，不扩大为全仓清理已完成的结论。

## 权限、事实、映射与装配核对

| 权限 / 能力 | 固定规则及资格来源 | Facts / Mapping / 绑定责任 |
| --- | --- | --- |
| BATCH_LIST、BATCH_DETAIL、ROW_LIST | `erp-permissions/src/legacy_import.rs:5`、`:11`、`:20` 公共常量；identity Function；普通资格使用当前权限自身，`authorization/rules.rs:62` | 无对象 Facts、无查询 Mapping、无动态准备、无查询凭证；不得补空实现 |
| BATCH_CREATE、BATCH_APPLY、EXECUTION_COMMAND | 同模块 `:8`、`:14`、`:17`；identity `scope_spec/catalog/functions.rs:180`、`:182`、`:183`；由 ImportApplyProcess 在原事务 require Operation | 无须 legacy_import 独立授权绑定；正式业务写入归 erp-data-import，FSY-25 检查 |
| CONFIRMATION_LIST、CONFIRMATION_CREATE、CONFIRMATION_COMPLETE | 同模块 `:23`、`:26`、`:29`；identity Function；编排在读取、写入和回放前检查各自功能资格 | 基础功能检查无对象绑定；任务身份、版本、指派和来源的附加业务合同仍由工作项能力处理，不能以本表替代该检查 |
| 内部工作项办理 `internal::WORK_ITEM_EXECUTE`，真实类型 IMPORT_BUSINESS_CONFIRMATION / LEGACY_IMPORT_BATCH | `erp-identity/src/service/authorization/work_items.rs:44` 将读取资格映射到已有 `legacy_import::BATCH_DETAIL`；真实 Task 目标由统一工作项合同约束 | 复用 `work_item` Facts；`erp-server/src/wiring/authorization/mod.rs:97` 至 `:105` 绑定 TaskFacts 和任务 Mapping，内部装入 `erp-workflow::service::work_item::authorization_facts`；本域不另建 legacy_import Facts |
| 详情中的后台任务身份 | 已通过 BATCH_DETAIL 的完整读取步骤 | 领域端口 `ports/bulk_job.rs:10` → `erp-server/src/wiring/import.rs:49` → `erp-support::background_jobs().find_by_request_id(request_id, executor)`；只补 ID，不冒充后台任务详情授权 |

Builder 对不需要 Facts 的纯功能资源直接拒绝事实绑定（`erp-identity/src/service/authorization/builder.rs:45`）；完整性检查 `:111` 按 Function 及实际 query 能力判断必要依赖。`rules.rs:337` 采用显式查询权限集合，而不是把所有 list/detail 动作都当成 Mapping 查询。因此本域在 `wiring/authorization/` 没有独立 legacy_import 绑定是设计结果，不是漏装配。

完整共享构造入口是 `backend/apps/erp-server/src/wiring/authorization/mod.rs:36`：先登记真实 Facts / Mapping，再执行 `builder.build()`；本轮只读装配代码，没有运行 server 单测或声称全量生产构造通过。

本 crate 没有自行定义内部导入编码。商品、供应商、品牌、询价导入的派生资格位于其他领域 / 编排，核对到的对应关系为：

- `internal::PRODUCT_IMPORT_JOB_READ` → `catalog::PRODUCT_PRODUCT_LIST`；
- `internal::SUPPLIER_IMPORT_JOB_READ`、`SUPPLIER_IMPORT_SUBMIT` → `supplier::IMPORT_SUPPLIER_IMPORT`；
- `internal::BRAND_IMPORT_JOB_READ` → `catalog::PRODUCT_BRAND_CREATE`；
- `internal::SALES_INQUIRY_IMPORT_CREATE` → 同角色具有销售询价创建、导入、可售商品查询资格。

前三组定义见 `erp-identity/src/service/authorization/rules/records.rs:117`；最后一组见 `rules.rs:52`。其中商品任务查询绑定位于 server `wiring/authorization/mod.rs:111`；供应商提交是 Function，不能要求账号获授派生内部编码。实际任务读取的本人关系、文件来源和候选范围归 FSY-25 / FSY-19 / 相应领域验证，不在本轮扩大修复，也未把静态映射核对写成已执行的行为验证。

## 待处理问题

### FSY10-01 · P2 · 本 crate 独立库测试缺少测试 feature 声明（已复现）

- 位置：`backend/crates/domains/erp-import/Cargo.toml:24`、`src/service/legacy_import/query.rs:320`；被导入模块的开关位于 `backend/crates/foundation/persistence-core/src/lib.rs:14`。
- 触发：在 backend 执行规定命令 `env -u ERP_TEST_MONGO_URI cargo test -p erp-import --lib --locked`，不额外选择其他包或 feature。
- 预期：本 crate 的内联授权测试能够独立编译和执行，不依赖其他 workspace 包偶然启用 feature。
- 实际：退出码 101，E0432，`persistence_core::testing` 被 `test-support` feature 隔离；本 crate 只在开发依赖中启用 `erp-identity/test-support`。identity 自己的 dev-dependency 不会作为下游普通依赖的测试设施自动启用。
- 影响：任务规定的独立库测试门禁无法执行；这是测试依赖声明缺失，不证明生产授权绕过或生产代码无法编译。
- 已完成定位验证：不改文件，显式添加 `--features persistence-core/test-support` 后 80 个库测试全部通过。这支持缺少 feature 声明的根因，不替代原命令的失败结论。
- 最小修复：仅在 erp-import 的 `[dev-dependencies]` 增加 `persistence-core = { workspace = true, features = ["test-support"] }`；保留普通依赖默认 feature，不修改 workspace 全局依赖或 persistence-core 生产封装。已有本地 crate manifest 可作写法参考。
- 验收：修复提交上，原规定命令不加任何 feature 参数即可退出 0；只跑本 crate 库单元测试，不通过全 workspace feature 合并掩盖问题。

### FSY10-02 · P2 · 三个读取入口缺少关键授权行为证据（已确认测试缺口）

- 位置：`backend/crates/domains/erp-import/src/service/legacy_import/query.rs:308` 至 `:364`；错误转换位于 `src/error.rs:77`，现有错误测试从 `:97` 开始。
- 现有证据：`import_reads_require_identity_before_querying_batches_or_rows` 使用 `unavailable_authorization()` 和 SnapshotExecutor，断言三个私有快照步骤返回 Internal“测试授权设施不可用”；这可证明设施故障前置，没有正常 RBAC 资格和完整成功读取。另一测试只验证 page_view 组装。错误测试覆盖唯一索引和乐观锁，未覆盖 identity 拒绝分类转换。
- 触发风险：即使资格被错误替换、正常缺权被吞掉、非快照执行器被放行、成功路径漏掉某一读取结果或端口错误，现有故障设施测试不一定能够检出。这里陈述的是回归覆盖缺失，不宣称这些生产错误已经发生。
- 预期：通过真实 AuthorizationService 的 require API 和实际生产读取编排，验证允许、正常 Forbidden、设施错误、原快照约束及结果 / 错误传播。不能用源码位置、方法名或字符串计数断言替代。
- 最小实现方向：保留三个公开入口、固定权限和事务包装；优先复用 FSY-8 / FSY-9 已交付的测试设施。若缺少可控制身份资格的 feature 测试实例，在核对协作后增加最小、仅 `test-support` 暴露的身份设施替身，仍执行真实 require / qualify / target 检查，禁止生产 Builder 放宽完整性。
- 若为了无 MongoDB 验证成功 / 拒绝后的顺序，需要小幅抽取本域私有“先 require、再调用传入读取步骤”的函数，应使三个实际入口都复用它；读取回调接收同一个 executor，单测用窄替身返回业务值或错误。不能仅在测试中复制生产步骤，也不为每个 Repository 建一层测试专用接口。
- 至少补证：三个动作各自有资格时进入读取、只授另一动作或没有资格时拒绝且零业务读取；NoTransaction / ReadExecutor 或已撤销快照边界时拒绝；身份设施错误及读取 / 端口错误传播；成功页的 items / total / page / page_size 和详情后台任务 ID 正确交付。身份错误转换断言至少覆盖 Forbidden、Unauthenticated、ValidationError、ConflictError、设施错误的稳定分类。
- 限制：真实数据库查询、外层事务提交 / 回滚、并发撤权及 MongoDB 分页仍不在允许执行的验证范围，必须明确保留未验证项。若单测只能覆盖共享编排步骤，交接中必须说明公开事务包装及数据库路径仍是静态验证，不能称完整 HTTP 或真实数据库端到端通过。

## 最小实现顺序与影响范围

1. 开发先读取本报告交付提交，核对本轮源基准和后续新增评论；保留 FSY10-01、FSY10-02 编号。无新证据不扩展成授权政策重写。
2. 修复 FSY10-01 的本域 dev-dependency。该操作不需要数据库、网络服务或数据迁移，不属于 external blocker。
3. 对照 FSY-8 的 FSY8-03 和 FSY-9 的最新实现，复用一次共享测试设施；必要的共享改动在父事项记录具体文件、负责人和准确 SHA，避免复制一套新的测试服务工厂。
4. 在本域补 FSY10-02 的内联行为测试及必要的最小私有编排抽取。优先改 `service/legacy_import/query.rs` 和 `error.rs`；若拆文件，按仓库规则保持本域责任和私有可见性。保持正常输入、筛选、DTO、权限编码和错误语义。
5. 在 backend 按规定原命令验证 erp-import 库测试。只有共享测试支持本身确有改动时，补受影响 crate 的相应库单测；不能用全仓检查或真实服务测试代替。lib 测试通过后不重复 cargo check。
6. 提交完整实现和测试，记录分支、源基准、交付 SHA、逐问题处置及命令结果，再进入 in_review。独立审查必须读取准确实现 SHA；尚有问题退回分析，全部关闭后才可 done。

| 文件 / 模块 | 预计改动 | 协作边界 |
| --- | --- | --- |
| `backend/crates/domains/erp-import/Cargo.toml` | 开发依赖启用 persistence-core/test-support | FSY-10 本域修复 |
| `backend/crates/domains/erp-import/src/service/legacy_import/query.rs` | 授权行为单测，必要的私有步骤抽取 | FSY-10；保留公开入口及事务语义 |
| `backend/crates/domains/erp-import/src/error.rs` | identity 错误分类传播单测 | FSY-10；无需改变错误枚举或共享恢复合同 |
| identity 的 `src/testing.rs` / `service/authorization/` feature 模块（仅如确需） | 最小可控资格测试设施 | 先核对 FSY-8 / FSY-9；复用其已交付版本，不改生产权限分支、Builder 或固定范围定义 |
| `erp-data-import` / `erp-workflow` / `erp-read-models` | 本轮只列出调用和责任，不要求本任务修改 | 分别由 FSY-25 / FSY-21 / FSY-38 独立检查 |
| server `wiring/authorization/` / `wiring/import.rs` / HTTP | 本域无缺失的必要授权 Facts / Mapping；本轮不要求改动 | 完整共享装配检查保留；应用注入和 actor 入参切换留第六阶段 |

兼容性：计划不改数据库字段、集合、索引、既有权限字符串、DTO 序列化或公开业务方法签名，无数据迁移和回填要求。dev-dependency 的 feature 不应通过普通依赖流入生产。若为测试抽取私有步骤，主要风险是改变参数验证、缺权和 NotFound 的先后顺序，必须保留目前“参数校验可先于鉴权、业务读取必须后于鉴权”的合同，并确保后台端口仍接收原 executor。

## 可执行验收矩阵

| 编号 | 场景 / 期望 | 证据方式与责任 |
| --- | --- | --- |
| A01 / FSY10-01 | 原独立命令无需额外 feature，库测试全部通过 | 开发与独立审查各自对准确交付版本执行规定的 erp-import --lib 命令 |
| A02 / FSY10-02 | BATCH_LIST、BATCH_DETAIL、ROW_LIST 各有资格且持有快照边界时进入各自读取步骤 | 真实 require + 生产读取编排 + 无数据库替身，断言业务结果和进入次数 |
| A03 / FSY10-02 | 缺当前动作资格、只授另一动作、身份失效时返回 Forbidden；不探测批次、行或后台端口 | 同上，断言具体错误及零读取；不以始终失败设施模拟正常缺权 |
| A04 / FSY10-02 | 非快照执行器及快照证明被撤销时，在业务读取前返回 ValidationError | 本域私有步骤 / 真实 require 的行为测试；SnapshotExecutor 只证明设施合同 |
| A05 / FSY10-02 | 身份设施、仓储步骤和后台端口错误保持失败；无结果交付，不降级放行 | 实际编排传播测试及 ErrorClass 断言 |
| A06 / FSY10-02 | 成功页保留 items、total、page、page_size；详情保留 background_job_id；缺失任务 ID 的 None 与端口错误区分 | 生产结果构造和实际被调用步骤的内联单测；数据库内容正确性仍未验证 |
| A07 | 列表 / 计数沿用同 filter、同 executor；明细读取先 require 后检查存在；端口沿用原 executor | 本轮已静态追踪；最小编排抽取后以替身验证可测试的顺序 / 入参，不冒充数据库事务证明 |
| A08 | 不增加 legacy_import 无用 Facts / Mapping，不复制权限政策，不从内部编码授予入口资格 | 独立审查固定规则、Builder 责任及最终 diff；本域无 Mapping 凭证失效测试需求 |
| A09 | 完整准确提交可读取；两个问题分别关闭或保留未解决理由，未执行项如实标记 | FSY-10 最终交接评论 + 分支 / SHA + 独立 Review 结论 |

缺失对象 Facts / Mapping、Missing 与 Known(None)、动态准备版本、来源份额和 CompiledAccess 跨事务 / 跨阶段复用，对本域三个 Function 读取不适用，因为它们不生成查询凭证，也不读取对象授权事实。相应统一引擎和任务目标能力归 FSY-9 / FSY-21，不在本域复制全套测试。本域仍需覆盖非快照拒绝和拒绝后零业务读取。

## 已执行验证与明确未验证项

工作目录为 `backend/`，两次均基于 `77f96e3fd6a225d2b67aaa6ba8c299412eaaee45`，本轮没有修改生产或测试源码。

| 命令 | 实际结果 |
| --- | --- |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-import --lib --locked` | **失败，退出码 101**；E0432：`query.rs:320` unresolved import `persistence_core::testing`，feature 被关闭；没有执行任何测试用例 |
| `env -u ERP_TEST_MONGO_URI cargo test -p erp-import --lib --locked --features persistence-core/test-support` | **诊断验证通过，退出码 0；80 passed、0 failed、0 ignored、0 filtered out**。这是显式 feature 下的结果，原独立命令仍待修复 |

诊断运行中第三方 proc-macro-error2 产生 future-incompatibility 提示，没有导致失败。现有授权故障测试使用未部署数据库的本地端口句柄，在身份设施错误后提前返回；未读取真实数据库。FailClosedBulkJobFacts 测试也不执行数据库操作。

没有新增、修改或执行集成测试、examples、真实 MongoDB / S3 / 其他外部服务测试；没有执行 cargo check、server / workspace 全量构建或其他 crate 测试。报告文档变更以提交前 `git diff --cached --check` 校验；结果记入最终交接评论。

未验证：真实快照 / 提交 / 回滚 / 并发行为、真实数据库分页投影和计数、生产身份设施及完整 server 构造、HTTP / worker 的最终接线。静态阅读到的 identity 内部导入和任务测试未在本轮执行，不计入上面的 80 个通过用例。禁止以本轮诊断单测通过代替这些行为已验证。

## 同族任务协作与交接

已读父事项 FSY-2 的创建范围、三 agent 闭环指令、FSY-3 和 FSY-8 协作结论；交付前再次扫描并展开新增的 FSY-5 / FSY-6 / FSY-7 协作记录。已核对父事项全部 37 个子项。FSY-10 没有前轮评论或待保留的 Review 问题编号。

- FSY-8 已交开发，报告提交 `0fe9cdbea3070501ac3e1fd19d9724fd6abaac76`，分支 `agent/agent/fsy-8`；其 FSY8-03 同样需要纯功能读取的允许 / 拒绝证据。已扫描并展开其唯一分析交接线程，尚未读到测试设施的实现交付。若后续已交付可复用测试支持，应直接复用，不能重写。
- FSY-9 核对时为 todo、后端分析，评论扫描为空；共享 identity 测试设施变更需再次核对其最新工作。状态和评论为空都不能证明另一任务没有运行。
- FSY-7 报告提交 `ad6fb89cabbd79aa84541c8d24b6cbc7980814a7` 也记录了测试支持需求，建议由 FSY-9 / FSY-39 统一最小夹具；FSY-5 的 FSY5-03 有同类需求。本域只需要可控制纯功能资格的测试能力，无须为了本任务增加查询凭证签发 API。共享设施应复用一次交付，不在多个任务各建工厂。FSY-6 的客户 / 选品问题不涉及本域，不扩大本轮修复。
- FSY-25 核对时为 todo、后端分析，评论扫描为空；已读任务范围。ImportApplyProcess 的确认列表、写入、回放、文件来源和内部导入执行由该任务负责，本轮不重复实现，也不修改 `erp-data-import/src/errors.rs`。
- 父事项 FSY-3 的 AUD-02 可能机械适配各编排的 `errors.rs` 及 server 共享错误转换。本任务仅补 erp-import 本域 Error 转换测试，保留其交付，不修改共享恢复语义。
- 本域无必要共享装配修复。把上述测试设施复用和责任边界作为最终协作结论记录在 FSY-2，不改父事项状态、不额外 mention 或 wakeup 下一 agent。

本轮分析交付完成后，通过 FSY-10 的 todo → in_progress 命令交后端开发。测试依赖问题属于可修复代码问题，继续正常闭环，不标记环境 blocked；本轮没有独立审查通过、合并或部署结论。
