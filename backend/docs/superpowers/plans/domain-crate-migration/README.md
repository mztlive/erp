# ERP 后端领域 Crate 迁移执行计划

## 1. 目标与入口

本计划用于把 backend 的 entities/database/services 生产代码迁入领域 crate，并删除旧三层的生产源码、manifest 和全部活动依赖。阶段 00–17 均已达到「本地门禁通过」。

按 2026-09-07 用户指令，本轮以逻辑正确迁移和旧三层清零为交付目标；停止编译指标测量，性能验收后置，不作为本轮完成条件。不得将后置状态写成性能达标。

- 架构约束：[设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)。
- 执行入口：[00 基线与执行治理](00-baseline.md)；开始前必须阅读[公共执行合同](execution-contract.md)。
- 后续编译复测：[测量执行合同](compile-measurement.md)及[三场景补丁](compile-probes.json)；本轮不执行。
- 全量映射：[source-map.tsv](source-map.tsv)，共 922 个旧三层 src 下 Rust 文件；类型/函数索引：[source-symbols.tsv](source-symbols.tsv)。
- 历史档案：[旧三层归档合同](../../../archive/legacy-crates/README.md)；测试和旧文档保留原字节，三个旧根目录必须删除。
- 仓储拥有类型：[repository-types.tsv](repository-types.tsv)，共 141 类专用 Repository 固有实现。
- 机器可读阶段清单：[plan-manifest.json](plan-manifest.json)；文档校验：[tools/verify_plan.py](tools/verify_plan.py)。

从已包含阶段代码及证据提交的专用 worktree 根执行交付证据校验：

```bash
python3 backend/docs/superpowers/plans/domain-crate-migration/tools/verify_plan.py --execution
```

该命令只读，核验 18 阶段文档、代码提交、证据提交及已记录门禁。原 `--sources` 仅用于迁移前编制快照，不适用于旧源已删除的最终工作区。仅回填文件、保留原 HEAD 的工作区须与已核验提交逐字节比对，不以其旧 HEAD 冒充最终证据提交。

## 2. 不可变合同

- HTTP 路径/方法、DTO 字段及序列化、错误码/状态码、RBAC 和数据范围保持不变。
- collection、BSON 类型、索引名称/键/唯一性、金额/数量/时间、幂等键与回执保持不变。
- 跨领域原子流程复用同一个 Executor；领域事务内方法不得另开事务，外部 I/O 不得持有 session。
- 不创建事件总线、微服务、双写、兼容 façade 或第二份业务实现。
- 当前仓库只执行纯内联单元测试；历史 tests/ 原样保留，不作为迁移中的 Cargo target，不启动真实 MongoDB。
- 共用的资料、附件、审计、审批编排一旦迁入 processes，所有仍调用它的旧 services 外层用例必须在同阶段上移到对应命名流程；旧 services 只保留可被调用的单域/事务内接口，不得反向依赖 processes。
- 本阶段外的代码仅允许进行为已迁符号编译所必需的导入、类型和装配更新；新增业务变化必须独立处理。

当前有实现的目标为 19 个业务领域 crate、3 个基础 crate 和 2 个组合 crate；保留 bpm 等现有技术 crate。阶段 15 核验到无商城实现时不创建空 erp-commerce，不能把历史领域编号当成新增功能任务。

Rust 类型边界、金额序列化、消费方 Port、旧调用方上移与历史测试处理，以公共执行合同中的确定规则执行。

## 3. 阶段顺序与状态

状态只允许：未开始 → 执行中 → 本地门禁通过 → 已验收；异常时进入阻塞，问题消除后回到执行中。连续执行必须以前序完整代码及证据提交为输入；执行者最高登记「本地门禁通过」，不自行登记人工「已验收」。

| 阶段 | 执行文档 | 主要目标 | 状态 |
| --- | --- | --- | --- |
| 00 | [基线与执行治理](00-baseline.md) | 治理或最终验收 | 本地门禁通过 |
| 01 | [公共基础与仓储类型解耦](01-foundations.md) | erp-core, application-core, persistence-core | 本地门禁通过 |
| 02 | [身份与审计](02-identity-audit.md) | erp-identity, erp-audit | 本地门禁通过 |
| 03 | [工作流与组合层](03-workflow-composition.md) | erp-workflow, erp-processes, erp-read-models | 本地门禁通过 |
| 04 | [通用支撑](04-support.md) | erp-support | 本地门禁通过 |
| 05 | [主体、客户与供应商](05-party-customer-supplier.md) | erp-party, erp-customer, erp-supplier | 本地门禁通过 |
| 06 | [商品、仓库与合同](06-catalog-warehouse-contract.md) | erp-catalog, erp-warehouse, erp-contract | 本地门禁通过 |
| 07 | [导入任务](07-import.md) | erp-import | 本地门禁通过 |
| 08 | [库存](08-inventory.md) | erp-inventory | 本地门禁通过 |
| 09 | [财务](09-finance.md) | erp-finance | 本地门禁通过 |
| 10 | [销售](10-sales.md) | erp-sales | 本地门禁通过 |
| 11 | [采购](11-procurement.md) | erp-procurement | 本地门禁通过 |
| 12 | [履约](12-fulfillment.md) | erp-fulfillment | 本地门禁通过 |
| 13 | [退货与逆向流程](13-returns.md) | erp-returns | 本地门禁通过 |
| 14 | [外部集成](14-integration.md) | erp-integration | 本地门禁通过 |
| 15 | [商城范围核验](15-commerce-scope.md) | 无实现范围核验 | 本地门禁通过 |
| 16 | [供应链协同](16-supply.md) | erp-supply | 本地门禁通过 |
| 17 | [最终切换与逻辑迁移验收](17-cutover.md) | 旧三层清零、装配与行为合同 | 本地门禁通过 |

每阶段使用专用 worktree 与可追溯输入提交；共享注册变更由唯一集成负责人管理。阶段输出必须包含代码、调用方切换、旧实现删除、边界检查、完整门禁、证据与阶段提交。

## 4. 公共门禁

从 backend 执行。阶段 00 必须先创建领域边界脚本并统一 Cargo target/CI 测试规则。新增 path crate 的锁文件经一次受控 cargo check 更新、审查后才执行 --locked。

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

新领域检查已承接旧 Service 的七项原始查询规则，并覆盖领域依赖与最终旧三层清零；旧 Service 检查已删除。删除前两种检查的实际成功记录归档于阶段 17。纯内联测试和静态调用轨迹不代表真实数据库回滚/并发已经验证。

## 5. 本轮交付与后续性能事项

本轮必须满足：19 个领域、3 个基础、2 个组合 crate 具备真实实现；旧三层生产 src、manifest、workspace 注册与 normal/build/dev 依赖全部清零；入口切换、公开合同复核和完整质量门禁通过；历史测试档案保持原字节。阶段结果及证据入口见 [17-cutover.md](17-cutover.md)。

编译耗时、Fresh/Dirty 传播及改善率的性能验收后置。后续启动性能工作时执行测量合同，保留原始样本与失败记录；本轮不声明已达到性能阈值。
