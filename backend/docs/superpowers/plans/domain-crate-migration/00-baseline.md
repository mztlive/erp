# 阶段 00：基线与执行治理

## 1. 阶段元数据

| 字段 | 值 |
| --- | --- |
| 阶段 | 00 |
| 状态 | 未开始 |
| 编制日期 | 2026-09-06 |
| 执行目录 | 仓库内 backend；源路径均相对此目录 |
| 目标 crate | 治理/范围核验/最终验收，不创建空业务 crate |
| 执行负责人 | 进入执行中前登记；该阶段只有一个共享注册文件集成负责人 |
| 输入/输出提交 | 前序验收提交 / 本阶段验收后填写 |
| 依据 | [设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)、[公共执行合同](execution-contract.md) |

## 2. 阶段目标

建立可复现编译与行为基线、迁移文件所有权及可执行门禁，使阶段 01 有确定输入。

## 3. 前置条件

- 本计划及当前源码核对完成。
- 先阅读 [README](README.md)、公共执行合同及本阶段完整内容。
- 对照 [source-map.tsv](source-map.tsv) 的 phase=00 行、[source-symbols.tsv](source-symbols.tsv) 和本节之后的符号拆分表核验当前输入；当前清单默认归属文件数为 0，其源文件哈希只是编制快照，不是迁移已完成证明。
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

本阶段只建立治理、测量及检查工具；业务三层代码不搬迁。当前未提交工作必须先由其所有者形成可识别基线，不得用旧 HEAD 覆盖工作区。

范围内文件由本阶段符号表、phase=00 的全量文件清单、必要调用方和已登记的注册文件构成。范围外包括新业务、数据回填、依赖版本升级、生产配置/凭据、发布推送，以及既有集成测试源码。

## 6. 源路径、符号、目标与调用方

| 当前源文件（相对 backend） | 主要符号/选择范围 | 目标文件 | 执行动作 |
| --- | --- | --- | --- |
| `AGENTS.md` | `共享注册文件冻结清单；测试期望` | `AGENTS.md` | 独立地基修订：登记本迁移专用所有权及统一单元测试边界 |
| `Jenkinsfile` | `P0 质量门禁；P0 集成测试` | `Jenkinsfile` | 使用 --lib；移除真实 MongoDB 集成阶段；保留其他质量门禁 |
| `Cargo.toml` | `[workspace]；workspace.dependencies；profile.dev` | `Cargo.toml` | 记录原配置；只登记治理工具，不混入优化参数变化 |
| `services/Cargo.toml` | `package；dev-dependencies` | `services/Cargo.toml` | 与其他现有成员 manifest 一起关闭集成 target 自动发现 |
| `scripts/check-bpm-boundaries.sh` | `require_single_definition；PROCESS_KIND_FILE` | `scripts/check-bpm-boundaries.sh` | 记录规则基线；后续阶段只更新实际权威路径 |
| `scripts/check-service-boundaries.sh` | `rules；baseline；negative_samples` | `scripts/check-domain-boundaries.sh` | 新增领域边界入口；保留旧 Service 检查及其严格度 |


全量文件以 source-map.tsv 为默认目标；混合文件必须按上表及第 8 节拆符号。source-symbols.tsv 列出各源文件的类型、Trait、函数和声明行；未另行规定的符号名称与行为保持原样。禁止将该清单作为直接批量 mv 脚本。

本阶段还必须迁移 repository-types.tsv 中 owner_phase=00 的全部拥有仓储类型：其 prepare_at 文件由阶段 01 创建，迁到 final_type_definition，并同步 impl_sources 中的所有专用方法。本阶段开始时这些准备文件属于前序输出，不因编制时尚不存在而遗漏。

本阶段必须更新的入口调用方：

- `apps/cli/src/main.rs`。
- `apps/web-api/src/app_state.rs`。
- `apps/web-api/src/core/errors.rs`。
- `apps/web-api/src/lib.rs`。
- `apps/web-api/src/main.rs`。

此外必须按 source-symbols.tsv 的选定符号用 rg 检索全部生产消费者，并复核 grouped use、类型别名、pub use、impl、宏与 cfg(test) 调用。具体跨域消费者按第 7、8 节处理；每个旧引用必须改为直接目标合同或组合入口，不能新增旧路径转发。

## 7. 目标依赖合同

保持当前业务依赖图；本阶段不创建空业务 crate。后续新领域 forbidden edge 集合先作为可执行检查合同落盘，尚无新领域时不得伪造领域隔离通过证据。

依赖检查覆盖 normal、build、dev 三类边；禁止只检查默认 cargo tree 的正常依赖。没有业务直接依赖也不代表运行时解耦已完成，跨域集合访问与事务调用必须独立检查。

## 8. 按顺序执行的任务清单

1. [ ] 从仓库根记录 git status --short、git diff --stat、暂存/未暂存差异和当前 HEAD。建立涵盖现有修改的可追溯迁移基线提交；不得自动 stash、reset 或提交不属于本任务的代码。重叠文件未有明确归属时只暂停重叠的代码迁移。

2. [ ] 在独立地基修订中，为本迁移登记冻结文件的唯一集成负责人、00–17 阶段映射和阶段分支规则。旧 P1/P2/P3 owns 规则不用于绕过共享文件冻结；以后每阶段的注册变更与该阶段业务迁移一起验收。

3. [ ] 用 cargo metadata 枚举所有成员 manifest。在 [package] 设置 autotests = false，并移除显式 [[test]] 注册；不修改既有 tests/ 文件，不关闭 [lib]、内联测试、二进制或检查规则。再次用 metadata 验证没有 kind=test 的集成目标。同步 Jenkinsfile 的测试命令。

4. [ ] 新增 scripts/check-domain-boundaries.sh 及其解析实现。输入为 cargo metadata 的真实依赖图、已验收阶段清单和 Rust 源码；必须覆盖依赖重命名、grouped use、pub use、路径别名与条件编译。新增至少一条对应每条规则的正/负夹具；纯 grep 计数不能作为依赖闭合证明。

5. [ ] 新增 scripts/measure-incremental.py，严格实现 compile-measurement.md 的命令行、源文件恢复、无修改检查、预热、五次独立有效样本、Fresh/dirty 单元和中位数输出。脚本先在临时最小工作区验证失败恢复；不启动 web-api 或数据库。

6. [ ] 在专用基线 worktree，分别保存 Customer/Sales/Finance 的 check 与 build 样本。每组记录真实 rustc/cargo 版本、features、profile、存储路径、负载、两种补丁内容及哈希；保存构建日志和 timings 报告。无修改仍触发业务 crate 编译时先解决 fingerprint 原因。

7. [ ] 按 execution-contract.md 记录 HTTP 路由/权限、JSON/BSON、索引定义、命令指纹、错误码及关键事务调用轨迹。为缺失的行为边界补纯内联测试，再执行公共门禁；将基线提交和完整证据登记到本阶段证据表。

每个实体/仓储/服务迁移单元均按“特征测试 → 公开合同 → 实体 → 仓储 → 服务 → 流程/读模型 → 调用方 → 删除旧实现 → 边界 → 全量门禁”执行。其文件/符号范围取第 6 节与清单，测试取第 10 节，删除范围为已迁实现与旧注册，不包括历史 tests/ 档案。

## 9. 阶段内编译中断规则

- 中间状态只存在于专用 worktree，不合入共享工作分支；阶段结束时整个 workspace 必须恢复可编译。
- 原则上每完成一个可闭合迁移单元即执行对应 crate 与入口窄检查；编译错误必须按事实归属修正，不扩大 public 或把业务类型移入基础层消错。
- 需要改共享注册文件时由本阶段唯一集成负责人完成，不让并行 worktree 交叉修改。
- 日常窄检查命令在目标 crate 创建后执行：

```bash
cargo check -p web-api -p cli --locked
```

## 10. 测试与质量门禁

- 测量脚本：命令失败与 SIGINT 后源文件哈希恢复；同一文件两种补丁各只匹配一次；输出缺少任一有效样本时必须失败。
- 边界脚本：新领域→旧业务、领域→领域、基础→业务、BPM→I/O、Service 原始 Mongo 操作分别有负向夹具。
- Cargo target：全部成员不存在集成 test target，纯内联测试仍被执行；CI 无真实数据库测试入口。

纯测试必须验证行为、数据形态或失败语义；不以源码字符串包含检查替代领域行为断言。既有 include_str! 结构检查可以保留并更新正确路径。

阶段 00 先完成检查脚本与 target 治理，再执行本组命令。

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

没有包含当前未提交工作的新基线；无修改构建持续重编译且原因不明；需要禁用纯单元测试或修改历史测试源码才能建立门禁。

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
| 输入基线 | 前序已验收 commit；阶段分支；源映射核对 | 未采集 |
| 文件与符号 | 新增/修改/删除列表；source-map 核销；跨域符号归属 | 未采集 |
| 依赖 | metadata/tree；normal/build/dev 边；无环与旧引用结果 | 未采集 |
| 旧实现清零 | 源目录、根导出、#[path]/include 路径、调用方搜索命令及结果 | 未采集 |
| 测试 | 内联测试命令、退出码、通过/失败/忽略数量、关键断言 | 未执行 |
| 协议与数据 | HTTP/DTO/错误/权限、JSON/BSON、索引对比 | 未采集 |
| 事务合同 | 同一 Executor、调用顺序、失败传播、I/O 边界；真实数据库运行未验证 | 未采集 |
| 公共门禁 | 每条命令、工具版本、退出码和日志路径 | 未执行 |
| 编译收益 | 适用场景原始样本、Fresh/Dirty、timings、中位数与改善率；不适用须写明 | 未执行 |
| 阶段提交 | commit hash、范围、验收日期及验收人 | 未提交 |
