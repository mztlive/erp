# Backend Crates 开发索引

本目录包含 31 个 workspace crate：3 个共享基础 crate、19 个业务领域 crate、2 个跨域组合 crate 和 7 个技术及测试支持 crate。成员与依赖以 [workspace manifest](../Cargo.toml) 为准；开发约束执行 [backend/AGENTS.md](../AGENTS.md)。

## 职责分配合同

| 工作内容 | 实现位置 |
| --- | --- |
| 本域实体、DTO、规则、服务、仓储、集合访问器及索引 | 对应业务领域 crate |
| 跨域写入、审批动作、事务和外部 I/O 编排 | `erp-processes` |
| 混合查询、工作台、中心页、展示统计 | `erp-read-models` |
| 通用值对象、应用合同、持久化机制 | 三个共享基础 crate |
| HTTP 协议、路由、鉴权中间件和 worker 生命周期 | [apps/web-api](../apps/web-api/README.md) |
| 管理员初始化与密码重置的命令行装配 | [apps/cli](../apps/cli/README.md) |

## 依赖要求

1. 业务领域的 normal、build、dev 依赖均不得指向其他业务领域或组合层；外域事实使用消费方 Port 或稳定事实 DTO，实际适配器由组合层装配。
2. `erp-processes` 可以依赖 `erp-read-models`，禁止反向依赖。共享基础 crate 不得持有业务实体或依赖业务领域。
3. `bpm` 仅接收调用方给定的模型、资格、ID 和时间；ERP 政策及持久化适配由 `erp-workflow` 拥有。
4. Repository 通过调用方 `Executor` 参与事务，组合根只登记拥有领域的公开索引入口，保持现有逐集合顺序。
5. 不得恢复 `entities`、`database`、`services` 旧三层 crate 或兼容转发层；历史归档不得接入活动 Cargo target。
6. 新增 crate 时同步更新 workspace 成员、依赖、边界检查配置、本索引及该 crate 的 README。

## 共享基础合同

| Crate | 职责 |
| --- | --- |
| [application-core](application-core/README.md) | 共享应用合同 |
| [erp-core](erp-core/README.md) | ERP 共享内核 |
| [persistence-core](persistence-core/README.md) | MongoDB 基础设施合同 |

## 业务领域

| Crate | 职责 |
| --- | --- |
| [erp-audit](erp-audit/README.md) | 审计与命令回执查询 |
| [erp-catalog](erp-catalog/README.md) | 商品目录与规格 |
| [erp-contract](erp-contract/README.md) | 合同与归档修订 |
| [erp-customer](erp-customer/README.md) | 客户账户与分配 |
| [erp-finance](erp-finance/README.md) | 应收、应付与成本 |
| [erp-fulfillment](erp-fulfillment/README.md) | 收货、交付与客户验收 |
| [erp-identity](erp-identity/README.md) | 身份与访问控制 |
| [erp-import](erp-import/README.md) | 历史数据导入事实 |
| [erp-integration](erp-integration/README.md) | 集成消息与差异处理 |
| [erp-inventory](erp-inventory/README.md) | 库存与预占 |
| [erp-party](erp-party/README.md) | 往来主体与敏感资料 |
| [erp-procurement](erp-procurement/README.md) | 采购订单与采购责任 |
| [erp-returns](erp-returns/README.md) | 退货、退款与冲正 |
| [erp-sales](erp-sales/README.md) | 销售订单与销售变更 |
| [erp-supplier](erp-supplier/README.md) | 供应商账户与资质 |
| [erp-supply](erp-supply/README.md) | 供给、供应商连接、履约与结算 |
| [erp-support](erp-support/README.md) | 来源登记、批量任务与文件资产 |
| [erp-warehouse](erp-warehouse/README.md) | 仓库资料与 SKU 策略 |
| [erp-workflow](erp-workflow/README.md) | 审批集成与工作项 |

## 跨域组合层

| Crate | 职责 |
| --- | --- |
| [erp-processes](erp-processes/README.md) | 跨领域命令与事务编排 |
| [erp-read-models](erp-read-models/README.md) | 跨领域查询与展示投影 |

## 技术与测试支持

| Crate | 职责 |
| --- | --- |
| [bpm](bpm/README.md) | 纯流程模型与状态引擎 |
| [entity-core](entity-core/README.md) | 实体基础元数据 |
| [entity-macros](entity-macros/README.md) | 实体与 ID 过程宏 |
| [id-generator](id-generator/README.md) | 内部 ID 与业务编号 |
| [permission-macros](permission-macros/README.md) | HTTP 权限标注宏 |
| [storage](storage/README.md) | S3 对象存储 |
| [test-support](test-support/README.md) | 库测试辅助与历史数据库夹具 |

## 接入与验证执行顺序

1. 在上表确定职责归属，再阅读目标 crate 的 README 和 `src/lib.rs`。
2. 消费方通过 workspace 依赖接入，例如在 `Cargo.toml` 的适当依赖节添加 `erp-customer = { workspace = true }`；业务领域不得用此方式引入其他领域，`test-support` 只能放在 dev-dependencies。
3. 根据变更调整拥有领域的规则、仓储、服务，再更新必要的 Process、ReadModel 与应用调用方。
4. 运行目标 README 的定向检查。修改公共合同、依赖或业务行为后执行下列统一质量门禁。
5. 仅修改 README 时，核对 crate 覆盖、相对链接、公开入口、命令包名和 `git diff --check`；无需因文档编辑执行编译或业务回归。

## 质量门禁

以下命令均在 `backend/` 目录执行：

```bash
cargo fmt --all -- --check
cargo check --workspace --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
env -u ERP_TEST_MONGO_URI cargo test --workspace --lib --locked
./scripts/check-bpm-boundaries.sh
./scripts/check-domain-boundaries.sh --cutover
./scripts/check-permissions-drift.sh
git diff --check
```

测试仅运行库单元测试。不得新增、修改或执行集成测试，不运行 `--test`、`--include-ignored` 或真实 MongoDB、S3 等外部服务测试。上述命令是执行要求，不代表当前工作区已完成验证。
