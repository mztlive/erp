# 阶段 16：供应链协同

## 1. 阶段元数据

| 字段 | 值 |
| --- | --- |
| 阶段 | 16 |
| 状态 | 本地门禁通过 |
| 编制日期 | 2026-09-06 |
| 执行目录 | 仓库内 backend；源路径均相对此目录 |
| 目标 crate | `erp-supply` |
| 执行负责人 | Codex；唯一共享注册集成负责人 |
| 输入/输出提交 | 输入 `ed8015e2`；实现 `72a0c79a2261d33699b869329376e534edfb1ef4`；证据以本文件所属提交为准 |
| 依据 | [设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)、[公共执行合同](execution-contract.md) |

## 2. 阶段目标

将供应供给、API 能力、供货履约与结算迁入统一供应链协同 crate。

## 3. 前置条件

- [阶段 15](15-commerce-scope.md) 已验收，证据与提交完整。
- 先阅读 [README](README.md)、公共执行合同及本阶段完整内容。
- 对照 [source-map.tsv](source-map.tsv) 的 phase=16 行、[source-symbols.tsv](source-symbols.tsv) 和本节之后的符号拆分表核验当前输入；当前清单默认归属文件数为 88，其源文件哈希只是编制快照，不是迁移已完成证明。
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

erp-supply 内的 supplier_offering/api/fulfillment/settlement 可按实际依赖协作；主体/供应商主数据、财务与工作流仍为外域。

范围内文件由本阶段符号表、phase=16 的全量文件清单、必要调用方和已登记的注册文件构成。范围外包括新业务、数据回填、依赖版本升级、生产配置/凭据、发布推送，以及既有集成测试源码。

## 6. 源路径、符号、目标与调用方

| 当前源文件（相对 backend） | 主要符号/选择范围 | 目标文件 | 执行动作 |
| --- | --- | --- | --- |
| `services/src/supplier_offering/mod.rs` | `SupplierOfferingService` | `crates/erp-supply/src/service/supplier_offering/mod.rs` | 资格事实通过供应商 Port 获取 |
| `services/src/supplier_api/governance/command.rs` | `command` | `crates/erp-processes/src/supply_governance/command.rs` | 批任务/集成/能力变更根编排 |
| `services/src/supplier_fulfillment/gateway.rs` | `SupplierGateway` | `crates/erp-supply/src/ports/supplier_gateway.rs` | 保持外部协议与失败关闭合同 |
| `services/src/supplier_fulfillment/place.rs` | `place` | `crates/erp-processes/src/supply_execution/place.rs` | 外部意图/调用/结果编排 |
| `services/src/supplier_settlement/source.rs` | `source` | `crates/erp-supply/src/service/supplier_settlement/source.rs` | 本供应链域内事实归属与证据 |
| `entities/src/supplier_settlement/source_evidence.rs` | `Evidence` | `crates/erp-supply/src/entity/supplier_settlement/source_evidence.rs` | 结算来源证据唯一语义 |
| `apps/web-api/src/app_state.rs` | `ExternalConnectorPorts；ExternalConnectorReadiness` | `apps/web-api/src/app_state.rs` | 注入新 gateway，保持 readiness 与 FailClosed |


全量文件以 source-map.tsv 为默认目标；混合文件必须按上表及第 8 节拆符号。source-symbols.tsv 列出各源文件的类型、Trait、函数和声明行；未另行规定的符号名称与行为保持原样。禁止将该清单作为直接批量 mv 脚本。

本阶段还必须迁移 repository-types.tsv 中 owner_phase=16 的全部拥有仓储类型：其 prepare_at 文件由阶段 01 创建，迁到 final_type_definition，并同步 impl_sources 中的所有专用方法。本阶段开始时这些准备文件属于前序输出，不因编制时尚不存在而遗漏。

本阶段必须更新的入口调用方：

- `apps/web-api/src/app_state.rs`。
- `apps/web-api/src/core/handler/supplier_api/mod.rs`。
- `apps/web-api/src/core/handler/supplier_fulfillment/mod.rs`。
- `apps/web-api/src/core/handler/supplier_offering/mod.rs`。
- `apps/web-api/src/core/handler/supplier_settlement/mod.rs`。
- `apps/web-api/src/core/routes/supplier_api.rs`。
- `apps/web-api/src/core/routes/supplier_fulfillment.rs`。
- `apps/web-api/src/core/routes/supplier_offering.rs`。
- `apps/web-api/src/core/routes/supplier_settlement.rs`。
- `apps/web-api/src/lib.rs`。
- `apps/web-api/src/main.rs`。

此外必须按 source-symbols.tsv 的选定符号用 rg 检索全部生产消费者，并复核 grouped use、类型别名、pub use、impl、宏与 cfg(test) 调用。具体跨域消费者按第 7、8 节处理；每个旧引用必须改为直接目标合同或组合入口，不能新增旧路径转发。

### 6.1 必须一并关闭的实际额外消费者

- 商品混合查询由 `erp-catalog::ports::CatalogSupplyQueryPort` 定义事实合同；真实 Mongo 聚合唯一迁到 `erp-processes::adapters::catalog_supply_query::repository`，查询准备与结果投影留在商品领域，商品列表入口装配 `erp-read-models::catalog_center`。销售精确 SKU 资格查询复用同一 Port，保持原 Executor 和日期。
- W13 应收详情及供应履约详情通过 `erp-read-models::ports::work_item_authorization` 接受原授权结果；Process adapter 调用原正式任务授权服务。读模型不得依赖 Process，不复制权限政策。
- 供应商资格的六次实际读取通过 OfferingQualificationPort 及唯一 supplier eligibility provider 协作；结算复核的应付/成本准备和持久化使用 finance 的最小事实入口。
- 支撑任务查询提供方、前序连接执行流程、采购创建依据和工作台事实只更新必要的实际提供方路径，保持原查询顺序及过滤。
- 网关失败分类 `SupplierFailureClass` 由供应链域唯一拥有，八个 snake_case 值保持不变；integration 重试政策仍只在 integration。逐分支双向映射必须穷尽，不使用兜底分支。

## 7. 目标依赖合同

supply 内部四组模块允许按实际方向调用；supply 不依赖 supplier/finance/integration/workflow/support 或旧三层；processes 负责跨域协作。

依赖检查覆盖 normal、build、dev 三类边；禁止只检查默认 cargo tree 的正常依赖。没有业务直接依赖也不代表运行时解耦已完成，跨域集合访问与事务调用必须独立检查。

## 8. 按顺序执行的任务清单

1. [x] 冻结供应商供给匹配、资质排除、API能力变更、下单/取消/拒绝/退款与结算证据/差异测试。

2. [x] 迁入四组实体、DTO、拥有仓储、扩展与索引。供给和结算可调用同一 erp-supply 内本域能力；对 erp-supplier 主数据的读取改为资格/账期事实 Port。

3. [x] 把 supplier_api 的 integration/bulk_job 协作迁 processes::supply_governance；把 supplier_fulfillment 的工作项推进及外部调用三段流程迁 processes::supply_execution。

4. [x] 保持 SupplierGateway/SupplierApiGateway/SupplierReferenceRegistry 的协议和错误分类；在 AppState 注入现有实现或原失败关闭实现，不更改外部地址、凭据、重试规则或 readiness 语义。

5. [x] 结算 source/projection/source_scope 的查询按实体归属限定在本供应链域；跨财务结算结果由 process 协调。保持草稿证据快照、差异判定与明细总额守恒。

6. [x] 更新四组 Handler、工作台供应协同摘要和其他领域 Port adapter，清空最后的旧供应链业务目录，执行门禁。

每个实体/仓储/服务迁移单元均按“特征测试 → 公开合同 → 实体 → 仓储 → 服务 → 流程/读模型 → 调用方 → 删除旧实现 → 边界 → 全量门禁”执行。其文件/符号范围取第 6 节与清单，测试取第 10 节，删除范围为已迁实现与旧注册，不包括历史 tests/ 档案。

## 9. 阶段内编译中断规则

- 中间状态只存在于专用 worktree，不合入共享工作分支；阶段结束时整个 workspace 必须恢复可编译。
- 原则上每完成一个可闭合迁移单元即执行对应 crate 与入口窄检查；编译错误必须按事实归属修正，不扩大 public 或把业务类型移入基础层消错。
- 需要改共享注册文件时由本阶段唯一集成负责人完成，不让并行 worktree 交叉修改。
- 日常窄检查命令在目标 crate 创建后执行：

```bash
cargo check -p erp-supply -p web-api -p cli --locked
```

## 10. 测试与质量门禁

- 供给：资质排除、SKU匹配、分页排序与重复供应供给。
- 外部协作：失败关闭、重复回调、幂等退款、租约/结果重放、外部调用位于事务外。
- 结算：来源证据、重复引用、差异/复核原因、作废与总额守恒。

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

供应商主数据被复制为第二事实源；失败 gateway 返回伪成功；结算证据或退款幂等语义改变；事务内外部调用。

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

| 证据 | 已核验结果 | 证据路径（仓库根） |
| --- | --- | --- |
| 输入基线 | 前序 ed8015e2；专用阶段 worktree | .domain-migration-evidence/16/input.json |
| 文件与符号 | 88 个清单旧路径、21 个 owned 准备文件已清零；跨域符号按固定提供方拆分 | source-clearance.json、files.tsv及逐符号报告 |
| 依赖 | normal/build/dev 全依赖闭包无其他业务域或旧三层 | domain-dependencies.json、metadata.json、boundary.log |
| 测试 | 3594 passed、0 failed、68 ignored；31 个历史 tests 档案逐字不变 | test-summary.json、unit-tests.log、historical-tests.json |
| 协议与数据 | raw changed=1、missing=0、added=1；类型归属变化与八个wire值逐项复核等价；原始50条needs_review逐项复核；权限生成物无漂移 | contract-review.json、parser-limitations-review.json、permissions.log |
| 事务合同 | 实际生产调用链、同Executor、原首错/写入顺序和I/O边界逐项复核；真实数据库运行未验证 | idempotency-transaction-review.json及各分片语义报告 |
| 公共门禁 | fmt/check/严格clippy/lib tests/BPM/service/domain/permissions/git diff 全部exit0；516个第三方包版本和校验和不变 | quality-gates.log、dependency-lock.json |
| 编译证据 | 本阶段边界与全workspace编译通过；最终三个性能场景统一在阶段17计时判定 | compile-applicability.json、domain-dependencies.json |
| 阶段提交 | 实现72a0c79a；证据以本文件提交记录为准；状态最高本地门禁通过 | input.json |

表内未写目录前缀的证据均位于 `.domain-migration-evidence/16/`。原始扫描保留exit1、类型声明差异与needs_review；必须结合逐项复核证据使用，不得标为扫描器直接通过。

## 16. 固定调用与证据边界

- API 命令保持校验、权限、身份与回执顺序；外部引用解析位于事务外，事务内原复验保持。能力更新回放及提交后的完整详情两次读取不得缩减。
- Offering 资格按 SKU、产品、当前能力指针、供应商、revision、第二能力的原顺序读取；命令恢复和任务/审计 ID 时点保持。列表精确SKU条件被后续集合条件覆盖的既有语义保持。
- 履约意图事务结束后才能调用网关，结果在第二事务持久化。普通派发、回放、未知结果和退款各自原恢复差异保持；W26 工厂唯一，不补原不存在的退款校验。
- 结算草稿替换保持有条件删除补证、有条件删除差异、无条件删除 items、statement CAS、无条件插入 items、有条件插入 differences。复核授权与岗位分离先于重读事实与写入，statement、任务、应付账户/分录、成本、回执的同Executor顺序保持。
- 非零成本差额仍在 record_review 与根事务前失败；零成本差额仍真实创建应付。各种回执的版本等于或大于等于条件分别保留，不合并为统一政策。
- 商品跨Supply聚合只有实际Process仓储实现；domain保查询准备、纯投影和唯一上架表达式。销售精确refs走同一Port，不新增排序、去重、日期或Executor替换。
- W13 与供应履约详情通过窄授权Port调用原WorkItem授权，构造器不新增查询，读模型不依赖Process。
- SupplierFailureClass拥有八个snake_case事实，integration重试政策仍归integration。原始扫描exit1/changed1/added1如实保留，结合穷尽映射和序列化测试核销，不能写成扫描器直接通过。
- 仅运行无真实Mongo环境的纯内联库测试；历史tests档案、原ignore以及第三方锁文件保持。真实数据库运行未验证。
