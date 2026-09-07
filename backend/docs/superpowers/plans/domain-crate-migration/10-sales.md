# 阶段 10：销售

## 1. 阶段元数据

| 字段 | 值 |
| --- | --- |
| 阶段 | 10 |
| 状态 | 本地门禁通过 |
| 编制日期 | 2026-09-06 |
| 执行目录 | 仓库内 backend；源路径均相对此目录 |
| 目标 crate | `erp-sales` |
| 执行负责人 | Codex；唯一共享注册集成负责人 |
| 输入/输出提交 | 输入 `abf36f41`；实现 `07da7863ec0fb975d2093e4e97a43c7e0813e607`；编译/证据以本文件所属证据提交为准 |
| 依据 | [设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)、[公共执行合同](execution-contract.md) |

## 2. 阶段目标

迁移销售单、销售变更与正式修订，完成 Sales/Finance 双向编译隔离。

## 3. 前置条件

- [阶段 09](09-finance.md) 已验收，证据与提交完整。
- 先阅读 [README](README.md)、公共执行合同及本阶段完整内容。
- 对照 [source-map.tsv](source-map.tsv) 的 phase=10 行、[source-symbols.tsv](source-symbols.tsv) 和本节之后的符号拆分表核验当前输入；当前清单默认归属文件数为 69，其源文件哈希只是编制快照，不是迁移已完成证明。
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

销售规则与历史销售事实归 erp-sales；正式化、应收差额及采购协作根流程归 processes。

范围内文件由本阶段符号表、phase=10 的全量文件清单、必要调用方和已登记的注册文件构成。范围外包括新业务、数据回填、依赖版本升级、生产配置/凭据、发布推送，以及既有集成测试源码。

## 6. 源路径、符号、目标与调用方

| 当前源文件（相对 backend） | 主要符号/选择范围 | 目标文件 | 执行动作 |
| --- | --- | --- | --- |
| `services/src/sales_order/formalize.rs` | `formalize` | `crates/erp-processes/src/order_to_cash/formalize.rs` | 正式化跨域根流程；销售本域写入留 sales |
| `services/src/sales_review/sales_change_order.rs` | `write_receivable_delta` | `crates/erp-processes/src/order_to_cash/sales_change.rs` | 应收差额及发票/资金审核任务移交 finance/workflow |
| `services/src/sales_order/query.rs` | `load_sales_procurement_coverage` | `crates/erp-read-models/src/sales_center/query.rs` | 跨采购覆盖读模型迁出 |
| `services/src/sales_review/sourcing.rs` | `采购推荐方案已随采购二次确认删除` | 删除生产实现 | 删除历史注释占位；禁止恢复已删除的推荐入口 |
| `entities/src/sales_order/content_hash.rs` | `hash` | `crates/erp-sales/src/entity/sales_order/content_hash.rs` | 保持指纹和创建/提交/正式版本区别 |
| `services/src/sales_order/mod.rs` | `SalesOrderService` | `crates/erp-sales/src/service/sales_order/mod.rs` | 导出稳定单域用例和事务内接口 |


全量文件以 source-map.tsv 为默认目标；混合文件必须按上表及第 8 节拆符号。source-symbols.tsv 列出各源文件的类型、Trait、函数和声明行；未另行规定的符号名称与行为保持原样。禁止将该清单作为直接批量 mv 脚本。

本阶段还必须迁移 repository-types.tsv 中 owner_phase=10 的全部拥有仓储类型：其 prepare_at 文件由阶段 01 创建，迁到 final_type_definition，并同步 impl_sources 中的所有专用方法。本阶段开始时这些准备文件属于前序输出，不因编制时尚不存在而遗漏。

本阶段必须更新的入口调用方：

- `apps/web-api/src/app_state.rs`。
- `apps/web-api/src/core/handler/sales_order/mod.rs`。
- `apps/web-api/src/core/handler/sales_review/mod.rs`。
- `apps/web-api/src/core/routes/sales_order.rs`。
- `apps/web-api/src/core/routes/sales_review.rs`。
- `apps/web-api/src/lib.rs`。
- `apps/web-api/src/main.rs`。

此外必须按 source-symbols.tsv 的选定符号用 rg 检索全部生产消费者，并复核 grouped use、类型别名、pub use、impl、宏与 cfg(test) 调用。具体跨域消费者按第 7、8 节处理；每个旧引用必须改为直接目标合同或组合入口，不能新增旧路径转发。

## 7. 目标依赖合同

sales 与 finance 无任何 normal/build/dev 直接依赖；processes 可组合两者；销售不依赖旧采购，由上层 adapter 提供事实。

依赖检查覆盖 normal、build、dev 三类边；禁止只检查默认 cargo tree 的正常依赖。没有业务直接依赖也不代表运行时解耦已完成，跨域集合访问与事务调用必须独立检查。

## 8. 按顺序执行的任务清单

1. [x] 冻结草稿/working copy/提交快照/正式修订、审批资格、价格精度和商品规格事实测试；保持历史引用不被当前主数据覆盖。

2. [x] 迁入 sales_order/sales_review 的实体、DTO、拥有仓储、索引；消除对 finance/procurement/catalog 的实体或服务导入，改为消费方窄事实及快照转换。

3. [x] 在 processes::order_to_cash 迁入正式化、应收初建/差额与资金/开票任务的根编排。write_receivable_delta 拆为 finance 差额接口与 workflow 任务接口；根回执及原 Executor 仍由组合层持有。

4. [x] 将销售资金进度更新中的跨财务读取由 finance 公开事实接口适配提供；销售纯进度规则和写入留 sales。替换阶段 09 中暂时指向旧销售的 adapter，清除旧销售业务依赖。

5. [x] 将采购覆盖、客户/合同中心摘要移入 read-models::sales_center；当前仍存在的销售对采购协作由明确组合用例接入旧采购事务内接口。sourcing.rs 只有删除说明，直接删除其占位，不创建对应流程或恢复旧推荐功能。

6. [x] 切换 sales_order/sales_review Handler、审批分发、履约和退货的调用方；删除旧目录、导出、#[path] 和测试源路径。执行完整门禁与 Sales/Finance 两组增量复测。

每个实体/仓储/服务迁移单元均按“特征测试 → 公开合同 → 实体 → 仓储 → 服务 → 流程/读模型 → 调用方 → 删除旧实现 → 边界 → 全量门禁”执行。其文件/符号范围取第 6 节与清单，测试取第 10 节，删除范围为已迁实现与旧注册，不包括历史 tests/ 档案。

## 9. 阶段内编译中断规则

- 中间状态只存在于专用 worktree，不合入共享工作分支；阶段结束时整个 workspace 必须恢复可编译。
- 原则上每完成一个可闭合迁移单元即执行对应 crate 与入口窄检查；编译错误必须按事实归属修正，不扩大 public 或把业务类型移入基础层消错。
- 需要改共享注册文件时由本阶段唯一集成负责人完成，不让并行 worktree 交叉修改。
- 日常窄检查命令在目标 crate 创建后执行：

```bash
cargo check -p erp-sales -p web-api -p cli --locked
```

## 10. 测试与质量门禁

- 销售：草稿/提交/正式快照不混用，商业内容、版本、取消和关闭状态一致。
- 销售变更：应收差额与开票任务一致、超额/负额/重复正式化失败边界。
- 编译：修改 Sales 时 Finance 保持 Fresh；修改 Finance 时 Sales 保持 Fresh；记录真实反向依赖闭包。

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

Sales/Finance 出现直接或通过旧 crate 的回边；重放指纹变化；正式历史快照被当前值覆盖；范围仅搬目录未解循环调用。

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
| 输入基线 | 前序 abf36f41；chore/domain-crate-10-sales 专用分支 | .domain-migration-evidence/10/input.json |
| 文件与符号 | source-map 69 个旧路径全部清零；13 个 owned 仓储完成迁移；销售/销售变更实体与持久化归销售，跨域命令归 processes | source-clearance.json、files.tsv、persistence-file-map.json、semantic-review.md |
| 依赖 | normal/build/dev 全依赖闭包中，业务包仅 erp-sales 与基础 erp-core；无其他领域或旧三层依赖 | metadata.json、sales-dependencies.json、boundary.log |
| 旧实现清零 | 旧销售两模块、仓储/索引注册、生产消费者与 include 路径切换 | source-clearance.json、files.tsv、boundary.log |
| 测试 | 3351 passed、0 failed、68 ignored；31 个历史 tests 档案逐字节不变 | unit-tests.log、historical-tests.json |
| 协议与数据 | 捕获合同 changed/missing/added 均为0；48条原始 needs_review 经字节及逐符号复核核销；DTO/金额/BSON/索引与权限生成物不变 | contract-comparison.json、missing-drift-report.json、contract-review.json、parser-limitations-review.json、permissions.log |
| 事务合同 | 正式化、资金进度、验收、回款冲正的实际生产Port/helper验证同一非零执行器、原步骤与逐步失败停止；真实数据库运行未验证 | semantic-review.md、transaction-contract.json、unit-tests.log |
| 公共门禁 | fmt/check/严格clippy/lib tests/BPM/service/domain/permissions/git diff 全部exit0；第三方依赖版本未变 | quality-gates.log、dependency-lock.json |
| 编译收益 | Sales/Finance各check/build、每组5样本、同Kingston介质、恢复与noop证明齐全；逐项中位数和Fresh闭包见原始报告 | compile/phase-summary.json及各组目录 |
| 阶段提交 | 实现07da7863；证据以本文件提交记录为准；状态最高本地门禁通过 | input.json |

表内未写目录前缀的证据均位于 `.domain-migration-evidence/10/`。原始静态扫描退出码为2，必须与复核证据共同使用，不得记为扫描器直接通过。

## 16. 固定调用与证据边界

- `erp-sales` 拥有销售/销售变更实体、仓储、索引、纯规则和单域写入；财务余额由财务事实接口按原逐账户方式提供，销售不引用财务类型。
- `order_to_cash`、`sales_change`、`fulfillment_execution::customer_acceptance`、`reverse_flow::ReceiptReversalProcess` 持有跨域根编排；HTTP及审批回调全部接入同一真实实现。
- 首次正式化保持采购复验、工作项、业务单据、销售版本、任务同步、提交、应收、资金/开票任务、审计的原10步顺序。发生错误时停止后继步骤。
- 客户验收投影为空时不读取财务余额；回款冲正在审计成功后仍重新读取核销分配，再逐单刷新销售进度。客户端直接冲正过账的拒绝入口保持。
- 真实数据库运行未验证；纯测试与静态复核不构成实际MongoDB回滚、并发或未知提交恢复证明。
- 编译报告为阶段中间态；最终至少两个场景改善30%、任何场景回退不超过10%的阈值统一在阶段17判定。
