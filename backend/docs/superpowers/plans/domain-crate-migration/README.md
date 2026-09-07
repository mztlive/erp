# ERP 后端领域 Crate 迁移执行计划

## 1. 目标与入口

本计划用于把当前 backend 的 entities/database/services 生产代码迁移为领域 crate，缩小日常增量编译范围。执行模式为逐阶段硬切；阶段 00–10 已达「本地门禁通过」，11 执行中，12–17 未开始。

- 架构约束：[设计契约](../../specs/2026-09-03-domain-crate-migration-design.md)。
- 执行入口：[00 基线与执行治理](00-baseline.md)；开始前必须阅读[公共执行合同](execution-contract.md)。
- 编译基线与复测：[测量执行合同](compile-measurement.md)及[三场景补丁](compile-probes.json)。
- 全量映射：[source-map.tsv](source-map.tsv)，共 922 个旧三层 src 下 Rust 文件；类型/函数索引：[source-symbols.tsv](source-symbols.tsv)。
- 仓储拥有类型：[repository-types.tsv](repository-types.tsv)，共 141 类专用 Repository 固有实现。
- 机器可读阶段清单：[plan-manifest.json](plan-manifest.json)；文档校验：[tools/verify_plan.py](tools/verify_plan.py)。

从仓库根执行文档/当前源码快照校验：

```bash
python3 backend/docs/superpowers/plans/domain-crate-migration/tools/verify_plan.py --sources
```

该命令只读，不实施迁移或编译。快照包含编制时已有未提交工作，进入阶段 00 时必须建立新的、可追溯的实际输入基线。

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

状态只允许：未开始 → 执行中 → 本地门禁通过 → 已验收；异常时进入阻塞，问题消除后回到执行中。只有前一阶段已验收，后一阶段才可开始。

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
| 13 | [退货与逆向流程](13-returns.md) | erp-returns | 执行中 |
| 14 | [外部集成](14-integration.md) | erp-integration | 未开始 |
| 15 | [商城范围核验](15-commerce-scope.md) | 无实现范围核验 | 未开始 |
| 16 | [供应链协同](16-supply.md) | erp-supply | 未开始 |
| 17 | [最终切换与编译收益验收](17-cutover.md) | 治理或最终验收 | 未开始 |

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
./scripts/check-service-boundaries.sh
./scripts/check-domain-boundaries.sh
./scripts/check-permissions-drift.sh
git diff --check
```

阶段 17 删除旧 services 后，新检查已覆盖同等规则才移除旧 Service 检查命令。纯内联测试和静态调用轨迹不代表真实数据库回滚/并发已经验证。

## 5. 编译收益验收

Customer、Sales、Finance 分别测量 check 和 build。每组独立预热，五次有效样本取中位数；check/build 各至少两个场景改善 30%，任一场景回退不超过 10%。Sales 修改不编译 Finance，Finance 修改不编译 Sales，无依赖关系的其他领域保持 Fresh。

应用入口和实际依赖组合层仍可能编译及链接。必须保留原始样本、日志、timings、Fresh/Dirty、工具链/profile/features/存储与后台负载；结构或耗时任一未通过，阶段 17 不得验收。
