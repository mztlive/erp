# 阶段 03：工作流与组合层

## 1. 阶段元数据

| 字段 | 值 |
| --- | --- |
| 阶段 | 03 |
| 状态 | 本地门禁通过 |
| 编制日期 | 2026-09-06 |
| 执行目录 | 仓库内 backend；源路径均相对此目录 |
| 目标 crate | `erp-workflow`, `erp-processes`, `erp-read-models` |
| 执行负责人 | 本阶段唯一集成负责人（分支 `chore/domain-crate-03-workflow-composition`） |
| 输入/输出提交 | 前序 `fead5bad842092ddbdcbf598ab7b3d00384ff226` / 实现提交 `1cb08731c8426db1c35dffa451c378d6c5cd095c`（证据见 `.domain-migration-evidence/03/metadata.json`） |
| 依据 | [设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)、[公共执行合同](execution-contract.md) |

## 2. 阶段目标

分离工作流状态、业务动作执行与跨领域查询，建立可供后续阶段复用的组合层。

## 3. 前置条件

- [阶段 02](02-identity-audit.md) 已验收，证据与提交完整。
- 先阅读 [README](README.md)、公共执行合同及本阶段完整内容。
- 对照 [source-map.tsv](source-map.tsv) 的 phase=03 行、[source-symbols.tsv](source-symbols.tsv) 和本节之后的符号拆分表核验当前输入；当前清单默认归属文件数为 135，其源文件哈希只是编制快照，不是迁移已完成证明。
- 前序阶段已改变公共类型时，以前序验收提交为实际输入，登记相应路径/签名变化；禁止自动重生成清单来隐藏未经审核的漂移。
- 进入专用 worktree；重叠未提交工作已明确归属。清单与当前源不符时先校正计划，不覆盖他人修改。

## 4. 不可变业务合同

- HTTP 路径/方法、DTO 字段及序列化、错误码/状态码、RBAC 和数据范围保持不变。
- collection、BSON 类型、索引名称/键/唯一性、金额/数量/时间、幂等键与回执保持不变。
- 跨领域原子流程复用同一个 Executor；领域事务内方法不得另开事务，外部 I/O 不得持有 session。
- 不创建事件总线、微服务、双写、兼容 façade 或第二份业务实现。
- 当前仓库只执行纯内联单元测试；历史 tests/ 原样保留，不作为迁移中的 Cargo target，不启动真实 MongoDB。
- 共用的资料、附件、审计、审批编排一旦迁入 processes，所有仍调用它的旧 services 外层用例必须在同阶段上移到对应命名流程；旧 services 只保留可被调用的单域/事务内接口，不得反向依赖 processes。
- 本阶段外的代码仅允许进行为已迁符号编译所必需的导入、类型和装配更新；新增业务变化必须独立处理。

## 5. 范围内与范围外事项

erp-workflow 拥有审批集成、Document Registry、WorkItem 状态/单域写入；业务动作注册归 processes，brief/工作台/客户中心归 read-models。

范围内文件由本阶段符号表、phase=03 的全量文件清单、必要调用方和已登记的注册文件构成。范围外包括新业务、数据回填、依赖版本升级、生产配置/凭据、发布推送，以及既有集成测试源码。

## 6. 源路径、符号、目标与调用方

| 当前源文件（相对 backend） | 主要符号/选择范围 | 目标文件 | 执行动作 |
| --- | --- | --- | --- |
| `services/src/approval/execution/runtime_service.rs` | `ApprovalRuntimeService` | `crates/erp-workflow/src/service/approval/execution/runtime_service.rs` | 保留注入 ApprovalDomainActionPort 的执行方式 |
| `services/src/approval_action_registry.rs` | `ApprovalActionRegistry；execute` | `crates/erp-processes/src/approval_dispatch/action_registry.rs` | 保留现有动作枚举映射和 Executor |
| `services/src/approval/business_adapter.rs` | `adapter_object_read_decision；require_wired_object_read` | `crates/erp-processes/src/approval_dispatch/object_read.rs` | 具体领域读取判定迁出；纯规格校验留 workflow |
| `entities/src/approval_integration/identity.rs` | `document_type_of_sales_business；subject_ref_for_sales_business` | `crates/erp-processes/src/approval_dispatch/sales_subject.rs` | 销售 BusinessType 适配迁出；通用 DocumentType↔ProcessKind 留 workflow |
| `services/src/transaction.rs` | `run_audited` | `crates/erp-processes/src/audit/transaction.rs` | 具体审计写入归组合层；基础事务保持通用 |
| `services/src/work_item/query.rs` | `work_item_list；work_item_detail` | `crates/erp-read-models/src/workbench/query.rs` | 授权后的分页与 brief 组装；保留已有过滤/游标算法 |
| `services/src/work_item/mod.rs` | `WorkItemService` | `crates/erp-workflow/src/service/work_item/mod.rs` | 命令服务留 workflow；查询服务拆为 WorkbenchReadService |
| `services/src/work_item/dto/view.rs` | `WorkItemView` | `crates/erp-read-models/src/workbench/dto/view.rs` | 跨领域视图迁出；命令 DTO 仍在 workflow |
| `services/src/customer/center.rs` | `CustomerCenterReadService` | `crates/erp-read-models/src/customer_center/service.rs` | 此阶段先迁组合；后续 05/09 替换其旧领域供应方 |
| `database/src/repository/customer_center_related.rs` | `customer_center_related` | `crates/erp-read-models/src/customer_center/repository/related.rs` | 由专属 CustomerCenterRepository 拥有跨域只读聚合 |
| `database/src/repository/work_item_fulfillment_queue.rs` | `FulfillmentQueueFilter` | `crates/erp-read-models/src/fulfillment_queue/repository.rs` | 跨域队列查询归读模型仓储 |


全量文件以 source-map.tsv 为默认目标；混合文件必须按上表及第 8 节拆符号。source-symbols.tsv 列出各源文件的类型、Trait、函数和声明行；未另行规定的符号名称与行为保持原样。禁止将该清单作为直接批量 mv 脚本。

本阶段还必须迁移 repository-types.tsv 中 owner_phase=03 的全部拥有仓储类型：其 prepare_at 文件由阶段 01 创建，迁到 final_type_definition，并同步 impl_sources 中的所有专用方法。本阶段开始时这些准备文件属于前序输出，不因编制时尚不存在而遗漏。

本阶段必须更新的入口调用方：

- `apps/web-api/src/app_state.rs`。
- `apps/web-api/src/core/handler/approval_instance/http.rs`。
- `apps/web-api/src/core/handler/approval_process/http.rs`。
- `apps/web-api/src/core/handler/customer/mod.rs`。
- `apps/web-api/src/core/handler/document_registry/mod.rs`。
- `apps/web-api/src/core/handler/work_item/finance_responsibility.rs`。
- `apps/web-api/src/core/handler/work_item/mod.rs`。
- `apps/web-api/src/core/routes/document_registry.rs`。
- `apps/web-api/src/core/routes/work_item.rs`。
- `apps/web-api/src/lib.rs`。
- `apps/web-api/src/main.rs`。

此外必须按 source-symbols.tsv 的选定符号用 rg 检索全部生产消费者，并复核 grouped use、类型别名、pub use、impl、宏与 cfg(test) 调用。具体跨域消费者按第 7、8 节处理；每个旧引用必须改为直接目标合同或组合入口，不能新增旧路径转发。

## 7. 目标依赖合同

workflow → bpm/基础；processes → workflow/身份/审计/旧 services；read-models → workflow/基础/旧 entities/database/services。所有领域及旧 services 均不得反向依赖 processes/read-models；旧入口需要组合逻辑时直接切换 Handler/CLI 到组合用例。

依赖检查覆盖 normal、build、dev 三类边；禁止只检查默认 cargo tree 的正常依赖。没有业务直接依赖也不代表运行时解耦已完成，跨域集合访问与事务调用必须独立检查。

## 8. 按顺序执行的任务清单

1. [x] 冻结审批固定类型/动作/快照、提交者与审批人分离、受阻取消、版本冲突、通知 outbox、WorkItem 责任与状态测试。bpm crate 不搬入 ERP 类型。

2. [x] 先定义并迁移 workflow 实体、DTO、ErrorCode 和消费方 Port。审批运行器继续消费 ApprovalDomainActionPort；授权使用注入的最小事实。ERP 集成 ID 使用 erp-core 中唯一类型，七类 BPM ID 保持原定义。

3. [x] 迁移 BPM/审批集成/DocumentRegistry/WorkItem 持久化及索引。对 WorkItem 中跨域责任查询与领域事实判断拆为 Port；只保留任务自身校验与写入。禁止直接把 SharedRbacService 或销售实体放入 workflow。

4. [x] 建立 erp-processes::approval_dispatch，迁移动作注册表、adapter_object_read_decision 中各领域分支、销售主题映射与 run_audited。具体适配可暂时依赖旧 services；workflow 本身不得依赖旧三层或 processes。

5. [x] 拆出 erp-read-models::workbench 与 customer_center。WorkItemView、分页列表/详情/统计、brief、party_names、fulfillment_queue 归读模型；WorkItem 的 create/reassign/close/write 仍为 workflow 命令。为保留 HTTP 返回字段，在 read-models 显式组合 workflow 查询事实与各业务最小事实。

6. [x] 将 customer_center_related 与履约队列从外部 Repository<T> 固有 impl 改为本 crate 拥有的专属只读仓储。customer_center_receivable 仅查询应收所属集合，暂留财务旧仓储，阶段 09 再迁入财务并接窄事实接口。

7. [x] 更新 AppState、approval_instance/approval_process/work_item/customer 的 Handler、后台通知 worker 与旧 services 调用方。AppState 作为 composition root 注入 Port；模块返回型引用拆开，防止 workflow→read-models 回边。

8. [x] 同步 check-bpm-boundaries.sh 的 process_kind.rs 路径、活动成员检查和 ID 位置；保留穷尽映射、无 I/O/无 ID 生成、唯一源等全部规则。旧模块声明/根 re-export 清零并运行门禁。

- [ ] 人工将状态改为已验收。本阶段执行者不得勾选；最高状态为本地门禁通过。

每个实体/仓储/服务迁移单元均按“特征测试 → 公开合同 → 实体 → 仓储 → 服务 → 流程/读模型 → 调用方 → 删除旧实现 → 边界 → 全量门禁”执行。其文件/符号范围取第 6 节与清单，测试取第 10 节，删除范围为已迁实现与旧注册，不包括历史 tests/ 档案。

## 9. 阶段内编译中断规则

- 中间状态只存在于专用 worktree，不合入共享工作分支；阶段结束时整个 workspace 必须恢复可编译。
- 原则上每完成一个可闭合迁移单元即执行对应 crate 与入口窄检查；编译错误必须按事实归属修正，不扩大 public 或把业务类型移入基础层消错。
- 需要改共享注册文件时由本阶段唯一集成负责人完成，不让并行 worktree 交叉修改。
- 日常窄检查命令在目标 crate 创建后执行：

```bash
cargo check -p erp-workflow -p erp-processes -p erp-read-models -p web-api -p cli --locked
```

## 10. 测试与质量门禁

- 审批：动作分发完整、未接线失败关闭、同一 Executor 贯穿动作和任务推进、失败不推进 WorkItem。
- 视图：既有逐条授权、服务端分页、排序和扫描补足策略不变，分页 count/has_more 与基线相同。
- outbox：通知键、租约、重试与失败恢复状态不变；纯替身验证外部发送发生在事务外。

纯测试必须验证行为、数据形态或失败语义；不以源码字符串包含检查替代领域行为断言。既有 include_str! 结构检查可以保留并更新正确路径。

从 backend 运行；先用一次受控 cargo check --workspace 更新本地 path crate 的锁文件，审查第三方版本未变，再执行 --locked 门禁。

```bash
set -e
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked
./scripts/check-bpm-boundaries.sh
./scripts/check-service-boundaries.sh
./scripts/check-domain-boundaries.sh
./scripts/check-permissions-drift.sh
git diff --check
```

公共门禁全部通过才允许进入“本地门禁通过”；填完证据并形成完整阶段提交后进入“已验收”。不得把未执行命令标为通过。

## 11. 事务与持久化验收边界

- 使用纯内联测试、Port 替身和调用记录验证同一 Executor、原写入顺序、错误传播、幂等判断及外部 I/O 分离。
- 使用内存内 JSON/BSON 序列化和索引定义比较，检查原字段类型、Decimal128、索引键/选项和唯一性合同。
- 本阶段不运行真实数据库或其他外部服务测试。证据中固定记录“真实数据库运行未验证”；不将编译或替身测试记作真实回滚/并发测试。

## 12. 暂停条件

迁移后的 workflow 仍导入销售/财务实体、SharedRbacService 或具体业务 Service；旧 services 通过调用 processes 形成环；审批映射遗漏或重复定义。

此外，任何业务/HTTP/权限/数据库/幂等行为变化、测试无法证明关键行为、来源不明的重叠修改或无法复用 Executor 都必须暂停。先保留失败证据并标记“阻塞”，不得关闭规则、扩大债务基线或跳过失败测试。

## 13. 回退步骤

1. 保存当前阶段文件差异、失败日志和输入提交；停止后续阶段。
2. 未合入阶段在专用 worktree 回到其输入版本或废弃该阶段分支，保留失败证据；不得对共享脏工作区执行 reset --hard/clean。
3. 已合入阶段以完整阶段提交为单位 git revert；阶段包含多个提交时一并回退，禁止只回退某一层或 Cargo 注册。
4. 恢复上一阶段所有权/阶段状态，重跑上一阶段公共门禁。外部环境和数据没有因本迁移被修改，不执行数据回填或恢复脚本。

## 14. 完成判据

- 第 8 节任务已完成；第 6 节及全量清单已核销，混合文件无遗漏符号或第二实现。
- 本阶段依赖约束及旧引用清零通过；必需入口、内联测试与公开错误映射均接入新实现。
- 第 10 节门禁与第 11 节范围内验证通过；第 15 节证据齐全，状态和提交一致。
- 适用的编译复测已完成并记录真实结果；最终性能阈值按阶段 17 判定，文档编制不构成阶段验收。

## 15. 结构化验收证据

| 证据 | 必填结果 | 初始状态 |
| --- | --- | --- |
| 输入基线 | 前序本地门禁通过 `fead5bad842092ddbdcbf598ab7b3d00384ff226`；分支 `chore/domain-crate-03-workflow-composition`；source-map phase=03 行 135；owned types 8 | 已采集 `.domain-migration-evidence/03/input.json` |
| 文件与符号 | 相对输入 521 路径变化（193 add / 129 delete / 199 modify）；8 owned EntityRepository 迁入 erp-workflow；CustomerCenterRepository 与 FulfillmentQueueRepository 专属只读仓储；历史 tests/ 字节不变 | 已采集 `.domain-migration-evidence/03/files.tsv` |
| 依赖 | 21 个成员；无 kind=test；erp-workflow 无 processes/read-models/旧三层回边；组合层允许依赖旧三层 | 已采集 `.domain-migration-evidence/03/boundary.log` |
| 旧实现清零 | unique-cut 删除旧 `services::approval/execution`、`document_registry`、`transaction` 与 owned dual repos；领域边界旧源清零规则已加载、当前不核销（尚无已验收阶段）；历史 tests/ 档案未改 | 已采集：见 `boundary.log` / `files.tsv` |
| 测试 | `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked`；3275 passed / 0 failed / 71 ignored；exit 0 | 已执行 `.domain-migration-evidence/03/unit-tests.log` |
| 协议与数据 | 369 条管理路由；21 个 ErrorCode；368 条索引；权限生成物与阶段 02 哈希相等 | 已采集 `.domain-migration-evidence/03/contract-comparison.json` |
| 事务合同 | Executor/NoTransaction、snapshot+majority、erp-processes run_audited 与 WorkflowAuditPort 同一 Executor；真实数据库运行未验证 | 已采集 `.domain-migration-evidence/03/transaction-contract.json` |
| 公共门禁 | fmt/check/clippy/test/bpm/service/domain/permissions/git-diff-check 全部 exit 0；状态为本地门禁通过 | 已执行 `.domain-migration-evidence/03/quality-gates.log` |
| 编译收益 | 适用场景原始样本、Fresh/Dirty、timings、中位数与改善率；不适用须写明 | 本阶段不适用；阈值在阶段 17 判定 |
| 阶段提交 | 实现提交 `1cb08731c8426db1c35dffa451c378d6c5cd095c`；证据目录 `.domain-migration-evidence/03/`；状态本地门禁通过；禁止标记已验收 | 已写入 `.domain-migration-evidence/03/metadata.json` |
