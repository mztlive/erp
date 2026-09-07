# 阶段 17：最终切换与逻辑迁移验收

## 1. 阶段元数据

| 字段 | 值 |
| --- | --- |
| 阶段 | 17 |
| 状态 | 本地门禁通过 |
| 编制日期 | 2026-09-06 |
| 执行目录 | 仓库内 backend；源路径均相对此目录 |
| 目标 crate | 治理/范围核验/最终验收，不创建空业务 crate |
| 执行负责人 | Codex；唯一共享注册集成负责人，分片所有权按实际输入证据登记 |
| 输入/输出提交 | 输入 `a537414eb8f78c43ebc383a3457a45c437dececc`；实现 `39c55021d8bb1d030eade4afb3e135e4b5a8a18b`；证据以本文件所属提交为准 |
| 依据 | [设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)、[公共执行合同](execution-contract.md) |

## 2. 阶段目标

完成逻辑正确的领域迁移，删除旧 entities/database/services 的生产源码、manifest、workspace 注册与全部活动依赖，完成入口装配及业务合同验证。

按 2026-09-07 用户指令，编译指标测量已停止，性能验收后置，不作为本轮完成条件。依赖结构和编译正确性仍须通过，不得将性能后置记为性能达标。

## 3. 前置条件

- [阶段 16](16-supply.md) 本地门禁通过，代码及证据提交完整。
- 先阅读 [README](README.md)、公共执行合同及本阶段完整内容。
- 对照 [source-map.tsv](source-map.tsv) 的 phase=17 行、[source-symbols.tsv](source-symbols.tsv) 和本节之后的符号拆分表核验当前输入；当前清单默认归属文件数为 7，其源文件哈希只是编制快照，不是迁移已完成证明。
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

旧三层生产入口/manifest、最终装配、脚本/CI/Docker 路径与逻辑合同核验；历史 tests/ 档案保留原样且不进入 Cargo。编译耗时、Fresh/Dirty 传播和改善率验收后置。

范围内文件由本阶段符号表、phase=17 的全量文件清单、必要调用方和已登记的注册文件构成。范围外包括新业务、数据回填、依赖版本升级、生产配置/凭据、发布推送，以及既有集成测试源码。

## 6. 源路径、符号、目标与调用方

| 当前源文件（相对 backend） | 主要符号/选择范围 | 目标文件 | 执行动作 |
| --- | --- | --- | --- |
| `Cargo.toml` | `entities；database；services` | `Cargo.toml` | 删除旧成员与 workspace.dependencies；登记真实有实现的目标集合 |
| `entities/src/lib.rs` | `pub mod` | 删除生产实现 | 删除旧生产源码与 manifest；保留 tests/ 档案 |
| `database/src/lib.rs` | `ensure_indexes` | `apps/web-api/src/app_state.rs` | 最终索引初始化只在组合根登记领域索引 |
| `services/src/errors.rs` | `Error；ErrorCode` | `apps/web-api/src/core/errors.rs` | 保留稳定协议错误映射；清除旧错误依赖 |
| `services/src/lib.rs` | `pub mod` | 删除生产实现 | 删除旧生产源码与 manifest；不保留 services facade |
| `Dockerfile` | `COPY database；COPY entities；COPY services` | `Dockerfile` | 移除旧目录 COPY；release 构建仍使用既有 LLVM/stable |
| `crates/test-support/Cargo.toml` | `entities` | `crates/test-support/Cargo.toml` | 清除旧依赖；纯单元测试新域仍不用此共享数据库夹具 |
| `scripts/check-service-boundaries.sh` | `rules` | `scripts/check-domain-boundaries.sh` | 新检查覆盖同等语义后移除旧入口 |


全量文件以 source-map.tsv 为默认目标；混合文件必须按上表及第 8 节拆符号。source-symbols.tsv 列出各源文件的类型、Trait、函数和声明行；未另行规定的符号名称与行为保持原样。禁止将该清单作为直接批量 mv 脚本。

本阶段还必须迁移 repository-types.tsv 中 owner_phase=17 的全部拥有仓储类型：其 prepare_at 文件由阶段 01 创建，迁到 final_type_definition，并同步 impl_sources 中的所有专用方法。本阶段开始时这些准备文件属于前序输出，不因编制时尚不存在而遗漏。

本阶段必须更新的入口调用方：

- `apps/cli/src/main.rs`。
- `apps/web-api/src/app_state.rs`。
- `apps/web-api/src/core/errors.rs`。
- `apps/web-api/src/lib.rs`。
- `apps/web-api/src/main.rs`。

此外必须按 source-symbols.tsv 的选定符号用 rg 检索全部生产消费者，并复核 grouped use、类型别名、pub use、impl、宏与 cfg(test) 调用。具体跨域消费者按第 7、8 节处理；每个旧引用必须改为直接目标合同或组合入口，不能新增旧路径转发。

### 6.1 实际额外切换范围

- 原五个 identity/support/workflow 组合适配器迁入实际 Process adapter；授权、采购改派和 W29 关闭仍按原 Port 方法调用领域提供方。唯一事实读模型不得承担跨域写入。
- 原 `services/work_item` 与工作台的共享权威事实合并到 `workbench::authority::WorkItemFactsReader`。命令最小事实与显示详情分别保持原查询次数、读取顺序和缺字段行为；不得以完整显示查询替换权限事实查询。
- Process 与 ReadModel 各自拥有真实 Error/Result，直接消费19领域错误，Process可接收ReadModel错误。审批码只使用 workflow 的唯一定义，禁止旧错误别名或读模型反向依赖Process。
- 活动唯一键提示由七个拥有领域提供窄函数；三个历史索引提示只由HTTP保留。应用边界仅对这三项保留typed DuplicateKey，HTTP仅对这三项重新分类，其余RepositoryError维持Internal。
- Web、CLI及两个组合crate的原ignore库测试分别登记阶段00原逐集合索引顺序。只复用领域公开索引，不移动或运行历史tests。
- 索引组合根使用27个调用保留19领域的原30组索引操作；身份账号/角色、审计日志、身份授权索引按原交错位置执行，workflow/support/finance子组不得以领域聚合入口重排。单个集合内的索引键、选项与创建调用不变。
- 导入确认的三个复合响应归Process，直接使用workflow唯一定义的任务类型与状态；投影必须保留实际关联任务值，禁止将所有类型固定成IMPORT_BUSINESS_CONFIRMATION。
- 已有测量工具及探针修正保留；不继续采样或判定性能阈值。原00固定生成的事务字段保留历史原件并另附真实性勘误。

## 7. 目标依赖合同

最终只允许入口→领域/组合层，组合层→领域，领域→基础，workflow→bpm，id-generator→persistence-core；所有 legacy 依赖为 0。

依赖检查覆盖 normal、build、dev 三类边；禁止只检查默认 cargo tree 的正常依赖。没有业务直接依赖也不代表运行时解耦已完成，跨域集合访问与事务调用必须独立检查。

## 8. 按顺序执行的任务清单

1. [x] 从 metadata 检查 19 个有实现业务域、3 个基础、2 个组合及保留技术 crate。processes/read-models 中对旧三层的临时依赖已替换为目标 API。

2. [x] 按 source-map.tsv、阶段 01 的仓储类型清单与各阶段符号拆分表核销生产实现。旧 entities/database/services 的 src 和 Cargo.toml 已删除；原 tests/ 档案保留，不改写、不移动、不运行历史集成测试。

3. [x] 更新 Cargo.lock、AppState、CLI、全局错误映射、索引初始化与后台 worker 装配；活动源码不再引用旧crate。历史迁移记录的源路径保留，不计入活动依赖。

4. [x] 更新 Dockerfile 的 COPY、构建与开发脚本、CI 和有效运行说明；未执行发布/推送/远程部署。保持稳定版 release 与本地 nightly/Cranelift 的既有配置边界。

5. [x] 确认 check-domain-boundaries.sh 覆盖旧 Service 原始查询规则以及新领域依赖规则后移除旧 Service 检查；BPM 纯度、权限漂移和全部现有行为门禁通过。

6. [x] 核验最终业务合同、同 Executor 调用路径、写入顺序、错误传播、索引顺序与公开响应；修复前序索引组重排和导入确认复合响应的偏差。

7. [x] 执行本轮完整质量门禁，归档实际命令、退出码、原始比较结果及逐项复核证据；原始扫描 exit1 不得改写为扫描器直接通过。

8. [x] 停止编译指标测量并恢复测量补丁，登记性能验收后置；填写阶段提交和证据，README 与 manifest 状态统一为「本地门禁通过」。后续性能事项必须另按 compile-measurement.md 执行，不纳入本轮完成清单。

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

- 依赖：普通领域互不直接依赖，无旧三层或 dev 回边；根 crate 是装配层。
- 行为：所有纯内联测试、HTTP/DTO/错误/权限、JSON/BSON/索引对比通过；真实数据库运行未验证如实记录。
- 性能：本轮后置；已停止测量且源码已恢复，不声明任何性能阈值通过。

纯测试必须验证行为、数据形态或失败语义；不以源码字符串包含检查替代领域行为断言。既有 include_str! 结构检查可以保留并更新正确路径。

从 backend 运行；先用一次受控 cargo check --workspace 更新本地 path crate 的锁文件，审查第三方版本未变，再执行 --locked 门禁。 旧 Service 检查只有在新领域检查覆盖同等规则后才从此命令组移除；删除它是本阶段明确任务。

```bash
set -e
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked
./scripts/check-bpm-boundaries.sh
./scripts/check-domain-boundaries.sh --cutover
./scripts/check-permissions-drift.sh
git diff --check
```

公共门禁全部通过、证据归档并形成完整阶段提交后，执行者最高登记“本地门禁通过”，不得自行登记人工“已验收”。不得把未执行命令标为通过。

## 11. 事务与持久化验收边界

- 使用纯内联测试、Port 替身和调用记录验证同一 Executor、原写入顺序、错误传播、幂等判断及外部 I/O 分离。
- 使用内存内 JSON/BSON 序列化和索引定义比较，检查原字段类型、Decimal128、索引键/选项和唯一性合同。
- 本阶段不运行真实数据库或其他外部服务测试。证据中固定记录“真实数据库运行未验证”；不将编译或替身测试记作真实回滚/并发测试。

## 12. 暂停条件

仍有旧生产源或 facade；丢失历史测试档案；实际依赖结构不符合合同；质量门禁未通过；以关闭规则、扩大 allowlist 或篡改证据制造通过。性能后置不阻断本轮逻辑迁移。

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
- 已停止编译指标测量并记录性能后置；不以文档状态代替实际质量门禁、代码提交或旧三层清零证据。

## 15. 结构化验收证据

| 证据 | 已核验结果 | 证据文件 |
| --- | --- | --- |
| 输入基线 | 阶段16 `a537414e`；专用切换 worktree；实现 `39c55021` | input.json、files.tsv |
| 文件与符号 | 阶段17清单7行已核销，全部141个仓储准备路径缺失且最终目标存在；跨域事实、写入和适配器归属已复核 | source-clearance.json、workflow-facts-implementation.json、process-result.json、adapters-implementation-evidence.json |
| 依赖 | 34成员；19领域、3基础、2组合；normal/build/dev无旧三层和禁止依赖边 | metadata.json、domain-dependencies.json、boundary.log |
| 旧实现清零 | 旧三层3个src及3个manifest缺失，活动源码、注册和依赖为0；31个历史测试逐字不变 | legacy-final-audit.json、historical-tests.json |
| 测试 | 33个库目标；3655 passed、0 failed、68 ignored，比阶段16新增61项通过 | test-summary.json、unit-tests.log |
| 协议与数据 | 10个HTTP测试覆盖494组实际响应；21错误码、17索引提示；4个组合根的161项索引操作顺序与00一致；导入确认保留真实关联任务值及动态路由 | http-error-expectations.json、index-sequence-review.json、import-projection-review.json、permissions.log |
| 契约扫描 | 16→17：changed1/missing2/added0/needs48；00→17：changed10/missing24/added14/needs65；均保留原始exit1并附逐项复核 | contract-comparison.json、missing-drift-report.json、cumulative/、contract-review.json、parser-review.json |
| 事务合同 | 源码调用链及内联测试核验同Executor、顺序、首错、幂等恢复和I/O分离；真实数据库运行未验证 | transaction-contract.json、transaction-review.json、boundary-error-review.json、startup-compensation-review.json |
| 公共门禁 | fmt/check/严格clippy/lib tests/BPM/domain/permissions/diff均exit0；旧Service检查删除前已通过并由新规则承接；516个第三方完整锁定项不变 | quality-gates.log、dependency-lock.json |
| 编译指标 | 按用户指令停止，性能验收后置；候选与基线源码已恢复，不声明性能阈值通过 | compile-deferred.json |
| 阶段提交 | 实现 `39c55021d8bb1d030eade4afb3e135e4b5a8a18b`；证据提交以本文件Git记录为准；执行人Codex，2026-09-07 | input.json |

表内证据文件均位于仓库根 `.domain-migration-evidence/17/`。只读扫描存在解析范围限制，必须结合 `contract-review.json` 的逐项处置与独立源码复核使用；原始比较产物不改写。真实数据库运行未验证，性能验收后置，状态最高登记「本地门禁通过」。

18阶段执行证据的只读核验结果归档于 `.domain-migration-evidence/final-execution.json`；该结果绑定已落库的阶段代码与证据，不表示人工验收。
