# 公共执行合同

## 1. 适用范围与权威顺序

本合同适用于本目录 00–17 阶段。源路径均相对 `backend`，目标路径是计划落点，只有阶段验收后才表示实际存在。

当前用户要求与 `backend/AGENTS.md` 优先；设计契约采用 2026-09-06 修订口径；具体阶段的符号拆分表优先于全量文件清单的默认路径。差异必须写入阶段证据，不得靠猜测选择路径。

计划编制没有执行业务迁移。`plan-manifest.json` 中的 HEAD 和源哈希记录编制工作区，可能包含已有未提交工作；阶段 00 必须形成包含实际待迁移实现的可追溯输入基线。禁止以较旧 HEAD 的代码覆盖当前工作。

## 2. 文件与阶段所有权

1. 阶段 00 先通过独立地基修订登记本迁移专用的阶段/注册文件所有权；不借旧 P1/P2/P3 owns 规则越界修改冻结文件。
2. 每阶段只有一个集成负责人更新 `Cargo.toml`、锁文件、旧三层根模块、仓储工厂/索引注册、AppState、根路由、错误映射、权限构建与检查脚本。
3. 每阶段的领域实现与必要注册变更共同构成一次可验收迁移；中间不可编译提交只在专用 worktree 内，不合入共享分支。
4. 前序阶段移出了后续领域仍调用的共用编排时，当前阶段必须迁移该调用者的外层组合入口。内层单域实现可留旧 crate，Handler/CLI 直接切换到组合入口，禁止旧 `services` 依赖 `erp-processes`。
5. 其他源文件的准备性修改只能用于接口解耦、导入、类型、事务参数和调用方接线，必须单独列入阶段证据；不得同时改业务行为。
6. 不修改来源不明的重叠工作；只暂停有重叠的迁移，不丢弃他人修改。

阶段分支使用可辨识的 `chore/domain-crate-<阶段>-<主题>` 命名，由阶段 00 地基修订登记；不得把本计划分支伪装成旧业务阶段。

## 3. 全量清单使用规则

- `source-map.tsv`：旧三层 `src` 下每个 Rust 文件只有一个默认迁移阶段。`preparation=01` 表示仓储类型必须先完成基础改造；共享根文件标记跨阶段注册更新。
- `source-symbols.tsv`：源文件类型、Trait、函数和宏 ID 的词法索引，用于定位符号；不是 Rust 语义分析器，不证明依赖闭合。
- `repository-types.tsv`：专用固有 `impl Repository<实体>` 对应的拥有类型、阶段 01 准备路径与最终定义路径。领域文件分散的同一实体只能共享一个拥有类型。
- 阶段表中指定为跨域 Process/Read Model 的符号必须从默认领域目标中分离；同一文件可以拆成多个明确目标，禁止把全文件机械移走后再留下双实现。
- 当前只有注释的历史占位文件标为 `remove-comment-placeholder`，只删除其占位和旧模块声明，不创建空目标模块。
- 源文件移动后必须同步 `mod`、`pub use`、`#[path]`、`include_str!`、`include_bytes!`、私有可见性和测试中的相对路径。必须检查 grouped imports、别名、宏展开及 cfg(test) 内引用。
- 阶段开始时重新核对本阶段源文件与前序迁移结果。新增、删除、重命名的业务文件须先补充清单和符号归属；不得自动刷新哈希把未知变更变成“已核验”。

## 4. Repository 的 Rust 类型合同

当前专用仓储方法通过 `impl Repository<'a, Entity>` 实现。把 `Repository<T>` 定义移动到 `persistence-core` 后，其他 crate 不能继续给这个外部类型写固有方法，即使 `Entity` 在本 crate 中定义也不行。使用 type alias 同样不成立。[Rust E0116](https://doc.rust-lang.org/error_codes/E0116.html)

执行路径固定如下：

1. 阶段 01 把通用 CRUD/连接/事务机械实现迁入 `persistence-core`。
2. 在旧 `database/src/repository/owned/<entity>.rs` 定义领域拥有类型，例如 `CustomerAccountRepository<'a>`，其内部组合 `persistence_core::Repository<'a, CustomerAccount>`。
3. 原 `impl Repository<'a, CustomerAccount>` 改为 `impl CustomerAccountRepository<'a>`；公开业务方法、参数、返回事实、Executor 约束保持原语义。其他实体依 `repository-types.tsv` 执行。
4. 领域拥有类型只公开调用方需要的 CRUD/查询/命令，不使用 `Deref` 暴露底层 collection；专用查询仍只有这一份实现。
5. `CustomerExt::customer_accounts()` 等工厂返回新的拥有类型；Factory Trait 在领域 crate 本地定义，可以为 MongoDB `Database` 实现。本地类型的重命名不改变 collection 名、关联常量或字段。
6. 后续领域阶段把拥有类型定义与全部专用 impl 一起移动到目标 crate；改为本地模块，不保留旧转发类型。
7. 只含通用 CRUD、没有专用固有 impl 的工厂可以返回基础 `Repository<T>`；不得误把这类类型计入专用拥有类型数量。
8. 跨域只读聚合必须由本地 `CustomerCenterRepository`、`FulfillmentQueueRepository` 等读模型仓储拥有，不能对其他 crate 的领域仓储追加固有 impl。

组合委托通用 CRUD 是领域仓储的实现方式；保留旧导入路径或双份业务查询是禁止的兼容层。

## 5. 基础层与数据格式

`erp-core` 持有金额/数量/比率、业务时间、稳定共享 ID、稳定操作人类别和纯校验基元。`application-core` 持有分页、调用人数据、命令身份/回执算法和应用错误分类。`persistence-core` 持有连接、执行器、事务和通用存储机械能力。

金额编码按以下条款执行：

- `Amount`、`Quantity`、`UnitPrice`、`Rate` 保留当前精度、舍入、JSON 字符串与 BSON wire Decimal128 行为。
- 当前金额 serde 实现随类型移动到 `erp-core/src/money.rs`，仅保留现有 `bson::Decimal128` 编解码依赖；不引入 MongoDB 驱动或查询。
- 不在 `persistence-core` 为外部金额类型实现外部 serde Trait；不为规避孤儿规则改成浮点、普通字符串存储或改写所有业务字段。
- 业务 entity 模块不得直接访问 BSON/MongoDB；需要的持久化表达由 Repository 处理。
- 基础层不得接入完整 AccountCore、Role、AuditLog、WorkItem、SalesOrder 或某一领域频繁变化的错误枚举。

原 `services::Error` 不整体移动。`ErrorClass` 等稳定分类归应用合同；审批 ErrorCode 归 workflow；特定唯一索引的用户提示及各领域错误在领域/组合/HTTP 边界显式映射。每阶段保存原错误黄金值，不解析字符串判断错误类别。

## 6. 领域之间的调用规则

所有业务领域，包括 identity/audit/workflow/support，均不得互相直接依赖。共享操作人只有数据；具体授权、审计写入、文件确认、工作流状态变化均通过以下方式实现：

| 场景 | 确定落点 | 执行规则 |
| --- | --- | --- |
| 单域命令需要其他域事实 | 消费方 `ports` + 组合层 adapter | 最小事实 DTO；不返回外域完整聚合/Repository/Service DTO |
| 多域原子命令 | `erp-processes::<命名流程>` | 持有根事务和根回执，调用各域事务内接口 |
| 跨域列表/中心/统计 | `erp-read-models::<消费视图>` | 组装公开事实或明确只读仓储，不散布 collection 查询 |
| 审批业务动作 | `erp-processes::approval_dispatch` | runtime 仅调用 `ApprovalDomainActionPort`；registry 依赖具体域 |
| 单域审计 | 消费方审计 Port 或根组合用例 | 业务决定动作/资源/结果；审计与原业务写入仍同事务 |
| 外域历史字段 | 消费方持有的快照事实类型 | 显式转换，原字段/serde 值不变，不能改读实时主数据 |

Port 的实现位于拥有消费方与提供方依赖的上层。接口中如需表达事务，则保留 `&mut dyn Executor` 的同一借用；不得拷贝会话、改用 `NoTransaction` 或开启嵌套事务。

领域持久化快照中目前引用其他域枚举时，先登记所有序列化变体，再改为消费方最小快照/事实类型并建立显式映射与往返测试。不得把外域完整聚合复制一份或为消除编译错误全部放入 erp-core。

## 7. 迁移期无环接线

- 新基础和新领域均不得依赖旧 `entities/database/services`，包括 dev-dependency 的传递回边。
- 旧三层可以单向消费已迁基础和领域；Process/Read Model 可以暂时同时依赖新旧实现。
- `services::transaction::run_audited` 在阶段 03 移入 Process 后，原直接调用它的领域外层事务也须移动到对应流程；原 Service 留下显式 `executor` 的单域内部方法，入口切换到 Process。
- `PendingFileAssets` 在阶段 04 进入附件组合后，catalog/supplier/payable/fulfillment 等外层 `*_with_assets` 同阶段移出旧 services。`FileAsset` 与 `AuditLog` 不再通过旧 Service 返回跨 crate 的组合载荷。
- 对尚未迁入新领域的类型使用上层 adapter 提取必要事实；禁止新领域为方便直接依赖旧 entities。
- 原注入 `SharedRbacService` 的地方改为消费方授权 Port；不能通过一个包含全部 Service 的“大上下文”重新形成编译聚合点。
- `test-support → entities` 的现有依赖必须纳入检查。新领域纯测试使用本地 fixture，不依赖该共享数据库夹具；身份迁移时只更新夹具本身的必要类型导入。

每阶段记录准备性上移的外层方法及原/新调用方。注册为 Process 的每个模块必须有实际业务步骤，禁止只有 re-export 或透明转发的空流程。

## 8. 事务、幂等与外部 I/O

1. 单域独立单集合 CRUD 使用原 `NoTransaction`；本域多集合原子命令使用原事务语义。
2. 跨域根命令由 Process 管理事务；所有下级命令读取/写入共用传入 Executor。
3. 写入顺序、前置检查时点、幂等规范化、指纹版本、回执 ID/内容与原代码保持一致。
4. 唯一键竞争后的失败 session 不复用；瞬态冲突、未知提交结果与可重试性按既有错误模型保持。
5. 有效业务写入成功后才允许推进相关 WorkItem；下级失败不得被吞掉。
6. HTTP/S3/供应商调用继续采用“意图事务 → 事务外调用 → 结果事务”，不新增消息总线或分布式事务。

## 9. 测试与历史档案

当前规则仅运行纯内联单元测试，不新增、修改或运行集成测试，不启动真实 MongoDB。阶段 00 必须在成员 manifest 设置 `autotests = false`，并移除显式集成 `[[test]]` 注册；该配置不会关闭库内单元测试。[Cargo target 自动发现](https://doc.rust-lang.org/cargo/reference/cargo-targets.html#target-auto-discovery)

- 所有 `tests/` 源码保持原样。旧业务 crate 最终移除后，旧目录可仅保留这些历史档案，不再作为 workspace 成员。
- 新领域也显式关闭集成 target 自动发现；不得创建新的集成 target。
- `cargo clippy --all-targets` 仍检查所有活动 target；不能依赖已禁用的历史集成代码来证明当前编译通过。
- 运行 `env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked`，保留当前纯测试和原有环境门控；不得以设置该变量、运行 ignored 或建立真实服务补充验收。
- 源文件内依赖真实 MongoDB 的既有测试只保持其环境门控，不执行其外部 I/O；必要验证另以纯输入、Port 替身或序列化测试表达。
- JSON/BSON 编解码与索引描述比较在内存内完成；事务调用轨迹记录 Executor 身份、调用顺序与失败停止位置。
- 验收记录明确写“真实数据库运行未验证”。这不是失败豁免，也不能写成真实事务回滚、并发或部署已验证。

## 10. 依赖与代码边界检查脚本合同

阶段 00 新增的 `scripts/check-domain-boundaries.sh` 必须检查：

1. 从 Cargo metadata 解析活动成员与 normal/build/dev 依赖、依赖改名及 workspace 继承；图中禁止回边和普通领域直接边。
2. 生产源码覆盖 grouped `use`、别名、`pub use`、宏可达路径；源码词法扫描结果必须结合真实 Cargo 图，不能用一条正则判断所有依赖。
3. entity 不能访问 Repository/Service/HTTP/MongoDB/BSON；erp-core 的金额编码按明确技术职责检查。
4. Service/Process 不能拼原始 BSON/collection 查询；只读原始查询仅位于领域或读模型 Repository。
5. 领域 Repository 不访问外域集合。集合关联常量与索引保持同一权威源。
6. 每个已迁类型/业务方法只有一个实现源；不保留旧路径 façade、宽泛根 re-export 或双 feature 路由。
7. 按已验收阶段清单检查旧源/旧注册清零；计划中的源路径、历史档案不作为生产依赖命中。
8. BPM 保持无 ERP 业务、I/O、时钟/ID 生成；七类 ID 和双向 ProcessKind 映射仍唯一。
9. 原 Service 边界规则严格度保持；迁移后新领域不继承旧债务 allowlist。移除旧路径命中后必须相应减少旧基线。

每条规则必须带正/负夹具和非零退出断言。禁止增大允许命中数量或增加任意忽略路径使阶段通过。

## 11. 证据与完成记录

证据写入专用 worktree 内 `.domain-migration-evidence/<阶段>/`，不存配置全文、Token、密码、密钥或真实业务敏感数据。阶段文档记录证据文件相对路径、命令、退出码和归属提交；日志可作为独立验收附件，不批量纳入源码提交。

最少包含 `input.json`、`files.tsv`、`metadata.json`、`boundary.log`、`unit-tests.log`、`quality-gates.log`、`contract-comparison.json`、`transaction-contract.json`；适用阶段另含 `compile/` 原始报告。名称为交付要求，当前文档编制未伪造这些文件。

状态为“本地门禁通过”时不得自动认为“已验收”。必须同时核销路径/符号、记录契约比较、完成性能要求和阶段提交，再由阶段执行者记录验收结果。各阶段初始状态均为“未开始”。

## 12. 范围核验与最终删除

当前生产源码对应 19 个业务领域。商城历史 ID、错误兼容文案和注释不作为创建 `erp-commerce` 的理由；阶段 15 按活动类型、集合与路由核验，结果为无实现时保存证据即可。

阶段 17 清除旧三层生产 src、manifest、workspace 注册、活动代码/CI/Docker/开发脚本依赖。权限生成物必须与输入基线内容一致，不能仅依赖暂存后的空 git diff 判断兼容。历史 tests/ 档案和迁移清单允许保留原路径文本。

本次执行不包含部署、推送、数据库变更或恢复。完整阶段回退必须连同所有注册和调用方提交一起处理，不在共享脏工作区执行破坏性清理。
