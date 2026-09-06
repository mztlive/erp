# 阶段 06：商品、仓库与合同

## 1. 阶段元数据

| 字段 | 值 |
| --- | --- |
| 阶段 | 06 |
| 状态 | 本地门禁通过 |
| 编制日期 | 2026-09-06 |
| 执行目录 | 仓库内 backend；源路径均相对此目录 |
| 目标 crate | `erp-catalog`, `erp-warehouse`, `erp-contract` |
| 执行负责人 | chore/domain-crate-06-catalog-warehouse-contract integrator |
| 输入/输出提交 | 前序 `88c386d071a72052b3c0d061de26a9c4ed5e76e9` / 实现提交 `b3f9ea487d63bc36e943c2d94a997bccf83fac59`（证据见 `.domain-migration-evidence/06/metadata.json`） |
| 依据 | [设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)、[公共执行合同](execution-contract.md) |

## 2. 阶段目标

迁移商品、仓库与合同，保持商品池筛选、修订快照与责任配置行为。

## 3. 前置条件

- [阶段 05](05-party-customer-supplier.md) 已验收，证据与提交完整。
- 先阅读 [README](README.md)、公共执行合同及本阶段完整内容。
- 对照 [source-map.tsv](source-map.tsv) 的 phase=06 行、[source-symbols.tsv](source-symbols.tsv) 和本节之后的符号拆分表核验当前输入；当前清单默认归属文件数为 75，其源文件哈希只是编制快照，不是迁移已完成证明。
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

商品及 SKU/规格/上下架、仓库主数据与责任人、合同及修订；涉及其他领域的文件和资质事实通过 Port 获取。

范围内文件由本阶段符号表、phase=06 的全量文件清单、必要调用方和已登记的注册文件构成。范围外包括新业务、数据回填、依赖版本升级、生产配置/凭据、发布推送，以及既有集成测试源码。

## 6. 源路径、符号、目标与调用方

| 当前源文件（相对 backend） | 主要符号/选择范围 | 目标文件 | 执行动作 |
| --- | --- | --- | --- |
| `services/src/catalog/sellable.rs` | `sellable` | `crates/erp-catalog/src/service/catalog/sellable.rs` | 保留公司商品池查询与服务端分页 |
| `services/src/catalog/product_workflow.rs` | `Product` | `crates/erp-catalog/src/service/catalog/product_workflow.rs` | 商品修订规则留本域；附件原子编排迁 processes |
| `database/src/repository/catalog/sellable.rs` | `sellable` | `crates/erp-catalog/src/repository/catalog/sellable.rs` | 保留管道、规格过滤与排序 |
| `services/src/warehouse/mod.rs` | `WarehouseService` | `crates/erp-warehouse/src/service/warehouse/mod.rs` | 仓库与履约责任配置使用身份事实 Port |
| `services/src/contract/mod.rs` | `ContractService` | `crates/erp-contract/src/service/contract/mod.rs` | 合同主体与附件事实通过 Port |
| `entities/src/catalog/specification.rs` | `compute_specification_signature` | `crates/erp-catalog/src/entity/catalog/specification.rs` | 规格签名与精确匹配保持原算法 |


全量文件以 source-map.tsv 为默认目标；混合文件必须按上表及第 8 节拆符号。source-symbols.tsv 列出各源文件的类型、Trait、函数和声明行；未另行规定的符号名称与行为保持原样。禁止将该清单作为直接批量 mv 脚本。

本阶段还必须迁移 repository-types.tsv 中 owner_phase=06 的全部拥有仓储类型：其 prepare_at 文件由阶段 01 创建，迁到 final_type_definition，并同步 impl_sources 中的所有专用方法。本阶段开始时这些准备文件属于前序输出，不因编制时尚不存在而遗漏。

本阶段必须更新的入口调用方：

- `apps/web-api/src/app_state.rs`。
- `apps/web-api/src/core/handler/catalog/mod.rs`。
- `apps/web-api/src/core/handler/catalog/product.rs`。
- `apps/web-api/src/core/handler/contract/mod.rs`。
- `apps/web-api/src/core/handler/warehouse/mod.rs`。
- `apps/web-api/src/core/routes/catalog.rs`。
- `apps/web-api/src/core/routes/contract.rs`。
- `apps/web-api/src/core/routes/warehouse.rs`。
- `apps/web-api/src/lib.rs`。
- `apps/web-api/src/main.rs`。

此外必须按 source-symbols.tsv 的选定符号用 rg 检索全部生产消费者，并复核 grouped use、类型别名、pub use、impl、宏与 cfg(test) 调用。具体跨域消费者按第 7、8 节处理；每个旧引用必须改为直接目标合同或组合入口，不能新增旧路径转发。

## 7. 目标依赖合同

catalog/warehouse/contract 互不依赖；销售/采购/客户中心读取适配位于 processes/read-models；领域只依赖基础及消费方 Port。

依赖检查覆盖 normal、build、dev 三类边；禁止只检查默认 cargo tree 的正常依赖。没有业务直接依赖也不代表运行时解耦已完成，跨域集合访问与事务调用必须独立检查。

## 8. 按顺序执行的任务清单

1. [x] 固定可售 SKU 过滤、属性组合、启停、分页、商品/仓库/合同版本与快照测试。每个领域先保存相关 JSON/BSON 与索引定义。

2. [x] 逐域迁入实体、DTO、拥有仓储、extensions 和 indexes；source-map.tsv 的 catalog/warehouse/contract 文件全部归位。

3. [x] 把 catalog 和 contract 的 PendingFileAssets/审计根编排切到已有 processes 模块；新领域不直接依赖 support 或 audit。供应资格、合同主体、仓库候选责任人的事实由消费方 Port 注入。

4. [x] 更新销售创建/商品池、采购创建依据、客户中心及仓库责任配置中的调用方；读取修订快照的地方继续读取原历史事实，禁止改成实时主数据覆盖。

5. [x] 逐域切换 Handler 和 registry，删除对应旧模块。将 include_str! 与内联查询测试指向实际目标文件，执行目标 crate 与入口窄检查和公共门禁。

每个实体/仓储/服务迁移单元均按“特征测试 → 公开合同 → 实体 → 仓储 → 服务 → 流程/读模型 → 调用方 → 删除旧实现 → 边界 → 全量门禁”执行。其文件/符号范围取第 6 节与清单，测试取第 10 节，删除范围为已迁实现与旧注册，不包括历史 tests/ 档案。

## 9. 阶段内编译中断规则

- 中间状态只存在于专用 worktree，不合入共享工作分支；阶段结束时整个 workspace 必须恢复可编译。
- 原则上每完成一个可闭合迁移单元即执行对应 crate 与入口窄检查；编译错误必须按事实归属修正，不扩大 public 或把业务类型移入基础层消错。
- 需要改共享注册文件时由本阶段唯一集成负责人完成，不让并行 worktree 交叉修改。
- 日常窄检查命令在目标 crate 创建后执行：

```bash
cargo check -p erp-catalog -p erp-warehouse -p erp-contract -p web-api -p cli --locked
```

## 10. 测试与质量门禁

- 商品：规格签名稳定、SKU匹配/过滤组合、分页边界和稳定排序；金额数量不用浮点。
- 仓库：候选人资格、停用、组织范围、责任人版本冲突。
- 合同：修订不可变、历史主体快照、附件确认和授权失败保持原结果。

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

商品池变成客户端切片；规格签名或价格口径改变；历史修订改读实时主数据；跨领域实体进入目标依赖。

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
| 输入基线 | 前序本地门禁通过 `88c386d071a72052b3c0d061de26a9c4ed5e76e9`；分支 `chore/domain-crate-06-catalog-warehouse-contract`；source-map phase=06 行 75；owned types 17 | 已采集 `.domain-migration-evidence/06/input.json` |
| 文件与符号 | 相对输入 206 路径变化（49 add / 1 delete / 65 modify / 91 rename）；17 owned EntityRepository 迁入 erp-catalog/erp-warehouse/erp-contract；商品附件与合同上传根用例迁入 erp-processes；历史 tests/ 字节不变 | 已采集 `.domain-migration-evidence/06/files.tsv` |
| 依赖 | 28 个成员；无 kind=test；erp-catalog/erp-warehouse/erp-contract 互不依赖且无旧三层回边；组合层允许依赖旧三层 | 已采集 `.domain-migration-evidence/06/boundary.log` |
| 旧实现清零 | unique-cut 删除旧 `entities/database/services` catalog/warehouse/contract 专属实现；领域边界旧源清零规则已加载、当前不核销（尚无已验收阶段）；历史 tests/ 档案未改 | 已采集：见 `boundary.log` / `files.tsv` |
| 测试 | `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked`；3303 passed / 0 failed / 68 ignored；exit 0 | 已执行 `.domain-migration-evidence/06/unit-tests.log` |
| 协议与数据 | 369 条管理路由；21 个 ErrorCode；368 条索引；权限生成物与阶段 05 哈希相等 | 已采集 `.domain-migration-evidence/06/contract-comparison.json` |
| 事务合同 | Executor/NoTransaction、snapshot+majority、erp-processes run_audited 与 catalog/warehouse/contract 根事务同一 Executor；真实数据库运行未验证 | 已采集 `.domain-migration-evidence/06/transaction-contract.json` |
| 公共门禁 | fmt/check/clippy/test/bpm/service/domain/permissions/git-diff-check 全部 exit 0；review_approved=true；状态为本地门禁通过 | 已执行 `.domain-migration-evidence/06/quality-gates.log` |
| 编译收益 | 适用场景原始样本、Fresh/Dirty、timings、中位数与改善率；不适用须写明 | 本阶段不适用；阈值在阶段 17 判定 |
| 阶段提交 | 实现/集成提交 `b3f9ea487d63bc36e943c2d94a997bccf83fac59`；证据目录 `.domain-migration-evidence/06/`；review_approved=true；状态本地门禁通过；禁止标记已验收 | 已写入 `.domain-migration-evidence/06/metadata.json` |
