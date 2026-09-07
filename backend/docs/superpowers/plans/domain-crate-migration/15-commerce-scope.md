# 阶段 15：商城范围核验

## 1. 阶段元数据

| 字段 | 值 |
| --- | --- |
| 阶段 | 15 |
| 状态 | 本地门禁通过 |
| 编制日期 | 2026-09-06 |
| 执行目录 | 仓库内 backend；源路径均相对此目录 |
| 目标 crate | 治理/范围核验/最终验收，不创建空业务 crate |
| 执行负责人 | Codex；唯一共享注册集成负责人 |
| 输入/输出提交 | 输入 `f5269ee9`；实现 `f5269ee9ab2277c87cb435cd1bf89adfc7483d64`；证据以本文件所属提交为准 |
| 依据 | [设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)、[公共执行合同](execution-contract.md) |

## 2. 阶段目标

核验历史商城领域在当前迁移基线中没有生产实现，形成明确范围结论。

## 3. 前置条件

- [阶段 14](14-integration.md) 已验收，证据与提交完整。
- 先阅读 [README](README.md)、公共执行合同及本阶段完整内容。
- 对照 [source-map.tsv](source-map.tsv) 的 phase=15 行、[source-symbols.tsv](source-symbols.tsv) 和本节之后的符号拆分表核验当前输入；当前清单默认归属文件数为 0，其源文件哈希只是编制快照，不是迁移已完成证明。
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

本阶段默认不创建 erp-commerce。历史 ID、DocumentType、错误索引消息或注释不等于已实现商城聚合。

范围内文件由本阶段符号表、phase=15 的全量文件清单、必要调用方和已登记的注册文件构成。范围外包括新业务、数据回填、依赖版本升级、生产配置/凭据、发布推送，以及既有集成测试源码。

## 6. 源路径、符号、目标与调用方

| 实际核验对象 | 固定结果与归属 | 执行动作 |
| --- | --- | --- |
| CardInstance / MallOrder / MallAfterSales / MallBackfill / ProductPublication | 00与15输入均无生产聚合、ID newtype、专用仓储/Service、活动集合或HTTP路由 | 不创建erp-commerce；精确命令、分类和哈希保存在scope-audit.json |
| erp-core稳定枚举与标识 | 仅保留现有来源、消息和单据分类，不含上述五类ID；DocumentType中的VoucherSalesOrder归销售 | 保持原符号、序列化及调用方 |
| services/src/errors.rs的历史publication/采购确认索引提示 | 现有兼容提示，不代表活动聚合 | 保持原行为，17由HTTP边界接续 |
| erp-sales卡券销售与erp-finance资金/成本规则 | 现有生产行为归原领域 | 不按商城文本改归属 |
| supplier_offering三处publication查询与catalog一处修订查询 | 实际只读本域集合 | 保留当前公开合同，16按供给和catalog原归属接线 |


以下为阶段 17 执行检查适配时补充的非 owned 范围复核合同，不计入阶段 15 迁移归属，不声称阶段 15 原始提交已包含本表。阶段 15 的 source-map 行与 repository owner 行均须保持为 0。

| 归属判定 | 历史源路径 | 实际保留目标 | 固定执行约束 |
| --- | --- | --- | --- |
| 非 owned 范围复核 | `entities/src/ids.rs` | `crates/erp-core/src/ids.rs` | 阶段 01 已迁稳定 ID；不据历史标识创建商城聚合。 |
| 非 owned 范围复核 | `services/src/errors.rs` | `apps/web-api/src/core/errors.rs` | 历史索引提示保持兼容；阶段 17 由 HTTP 错误边界接续。 |
| 非 owned 范围复核 | `entities/src/sales_order/entity/order.rs` | `crates/erp-sales/src/entity/sales_order/entity/order.rs` | 卡券销售仍归阶段 10 的销售域，不改归商城。 |


全量文件以 source-map.tsv 为默认目标；混合文件必须按上表及第 8 节拆符号。source-symbols.tsv 列出各源文件的类型、Trait、函数和声明行；未另行规定的符号名称与行为保持原样。禁止将该清单作为直接批量 mv 脚本。

本阶段还必须迁移 repository-types.tsv 中 owner_phase=15 的全部拥有仓储类型：其 prepare_at 文件由阶段 01 创建，迁到 final_type_definition，并同步 impl_sources 中的所有专用方法。本阶段开始时这些准备文件属于前序输出，不因编制时尚不存在而遗漏。

本阶段必须核验AppState、lib/main、路由与Handler的实际注册链；范围核验确认没有商城入口，不为满足清单形式修改这些文件。全部生产目标和调用方路径以scope-audit.json所列真实metadata及源码结果为准。

此外必须按 source-symbols.tsv 的选定符号用 rg 检索全部生产消费者，并复核 grouped use、类型别名、pub use、impl、宏与 cfg(test) 调用。具体跨域消费者按第 7、8 节处理；每个旧引用必须改为直接目标合同或组合入口，不能新增旧路径转发。

## 7. 目标依赖合同

本基线不新增依赖边；不创建空 erp-commerce。发现真实已有实现时采用与普通领域相同的无直接领域依赖规则。

依赖检查覆盖 normal、build、dev 三类边；禁止只检查默认 cargo tree 的正常依赖。没有业务直接依赖也不代表运行时解耦已完成，跨域集合访问与事务调用必须独立检查。

## 8. 按顺序执行的任务清单

1. [x] 读取阶段 00 基线和 source-map.tsv，枚举生产 Cargo targets、实体定义、仓储工厂、路由注册及其调用方。

2. [x] 对 CardInstance/MallOrder/MallAfterSales/MallBackfill/ProductPublication 的真实类型定义和活动集合访问进行核验；区分注释、历史测试、ID newtype 与生产实现。

3. [x] 当前基线没有对应聚合、Service、Repository 或 HTTP 实现时，填写“无既有实现；不创建 crate”，保存精确搜索范围、命令、输出及目标 crate 清单。

4. [x] 如在执行基线中实际出现此前未纳入的生产实现，先更新本阶段文件/符号/调用方映射及设计的有实现 crate 清单，再按既有业务行为迁移；不把新增功能作为本次重构任务。

5. [x] 验证此前已迁的卡券销售、资金登记与供应链事实仍在原所属领域；执行公共门禁，范围证据完整后本阶段可验收。

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

- 范围核验：无遗漏的活动类型、路由、集合；仅有历史标识时目标数保持 19。
- 既有卡券销售和资金登记的纯单元测试结果不变。

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

为了凑满 20 个 crate 创建空模块；将现有销售或供应链功能按名字强行归类；发现真实实现却没有源路径和契约记录。

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
| 输入基线 | 前序 f5269ee9；专用阶段 worktree | .domain-migration-evidence/15/input.json |
| 文件与符号 | 0 个清单源、0 个 owned 准备文件；五类商城对象无生产实现 | source-clearance.json、scope-audit.json、scope-contract.md |
| 依赖 | 无新增依赖；真实metadata与静态目标一致，范围不新增erp-commerce | domain-dependencies.json、metadata.json、boundary.log |
| 测试 | 3510 passed、0 failed、68 ignored；31 个历史 tests 档案逐字不变 | test-summary.json、unit-tests.log、historical-tests.json |
| 协议与数据 | changed/missing/added 均为0；原始46条needs_review逐项复核；权限生成物无漂移 | contract-review.json、parser-limitations-review.json、permissions.log |
| 事务合同 | 生产源逐字不变，沿用14已核验合同；本阶段没有事务实现变更；真实数据库运行未验证 | production-byte-identity.json、前序14事务合同 |
| 公共门禁 | fmt/check/严格clippy/lib tests/BPM/service/domain/permissions/git diff 全部exit0；516个第三方包版本和校验和不变 | quality-gates.log、dependency-lock.json |
| 编译证据 | 范围阶段全部公共门禁重跑通过；性能统一在17判定 | compile-applicability.json、domain-dependencies.json |
| 阶段提交 | 实现f5269ee9；证据以本文件提交记录为准；状态最高本地门禁通过 | input.json |

表内未写目录前缀的证据均位于 `.domain-migration-evidence/15/`。原始扫描保留exit2与needs_review；必须结合逐项复核证据使用，不得标为扫描器直接通过。

## 16. 固定调用与证据边界

- CardInstance、MallOrder、MallAfterSales、MallBackfill、ProductPublication在00基线与实际15输入均无生产聚合、ID newtype、专用Service/Repository、活动集合调用或HTTP路由。结论为无既有实现；不创建erp-commerce。
- source-map的phase=15与repository-types的owner_phase=15均为0行；不改清单生成不存在的迁移任务。原计划示例ID不得作为当前存在的事实。
- 卡券销售、资金登记、来源枚举和商城消费成本禁写规则仍归原领域；W29商城缺失类别保持正式差异代码。四个publication查询保留supplier_offering与catalog归属，不据命名新建发布聚合。
- 历史publication和采购确认索引提示维持原兼容行为，最终仅在17拆解错误出口，不在15新增商城业务。
- 实际00 Cargo metadata中的历史test targets如实保留在基线证据；当前metadata无kind=test集成目标。本阶段没有运行任何tests目录或真实MongoDB。
- 生产源、历史tests、脚本、Cargo配置及权限生成物与14输入逐字一致；协议原始扫描changed/missing/added均为0，46条needs_review保留并以同源证明和14源码复核覆盖。
- 纯内联与静态门禁不构成真实数据库证明；真实数据库运行未验证。
