# 自动化落地计划

## 1. 实施顺序

### 阶段 A：领域模型与测试钩子

1. 建立采购责任规则实体、仓储、索引和服务，先补规则优先级、唯一命中和资格校验单元/集成测试。
2. 建立共享的销售行采购覆盖计算，先补状态矩阵、零/超量和作废释放测试。
3. 注册 `PROCUREMENT_ORDER_CREATION` 任务类型、展示策略和责任范围唯一性。
4. 为关键前端交互增加 `testability-contract.md` 约定的稳定 `data-testid`。

### 阶段 B：销售提交与生效任务

1. 接入销售草稿责任预览。
2. 在销售提交审批前执行唯一责任人门禁。
3. 在最终审批生效路径中同步派发具体到人的采购任务。
4. 补非末级采购审批、最终生效、重复生效和失败回滚的服务层测试。

### 阶段 C：多采购单与进度

1. 删除“已有采购单则整单不可建”的判断。
2. 按销售提交行汇总有效采购覆盖数量并返回剩余数量。
3. 创建请求支持选择行和输入本次数量；事务内重新校验，防止并发超采。
4. 销售详情、工作台和创建依据复用同一口径；全部覆盖完成任务，释放后恢复任务。
5. 补 Rust、前端模型/组件测试。

### 阶段 D：真实端到端

1. 扩展现有 E2E reset/fixture，通过真实 API 或页面准备责任规则与供给。
2. 新增 `e2e/tests/flow-procurement-responsibility-multi-po.spec.ts`。
3. 完成桌面主流程：责任预览 → 两级审批 → 任务出现 → 4+6 两次建单 → 两张采购单 → 任务完成。
4. 使用移动视口验证采购进度、任务和继续建单入口，并实际执行一项业务操作。
5. 增加无规则、无供给、超量和跨责任范围等高价值异常断言。

### 阶段 E：回归、报告和基线合入

1. 执行 Rust 格式、静态检查、单元/集成测试。
2. 执行前端 lint、类型检查、单元测试和生产构建。
3. 执行目标 Playwright 测试，并用浏览器工具人工回归相关路由和移动端。
4. 更新 `automation-matrix.md` 为真实状态，生成执行报告。
5. 将已实现行为合入 `docs/prd` 基线；只有全部验收标准和完成门禁均满足后，才把增量 PRD 状态改为 `merged`。

## 2. 测试数据策略

- 使用仓库现有 E2E reset 保留账号/RBAC/基础资料的方式清理业务数据。
- 规则、商品/SKU、分类、供应商供给、审批定义和销售单通过真实管理 API/页面创建；禁止 mock 前端接口。
- 每次运行使用唯一前缀，例如 `E2E-MPO-<timestamp>`，避免与并发套件冲突。
- 主流程固定销售数量 10，第一次采购 4，第二次采购 6，便于跨页面断言。
- 并发集成测试使用隔离数据库和屏障；不依赖开发机已有记录。

## 3. 计划测试资产

| 层级 | 计划落点 | 主要覆盖 |
| --- | --- | --- |
| Rust unit | 采购责任规则、创建依据与剩余数量模块内测试 | 优先级、分类回退、状态覆盖、数量边界、任务完成判断 |
| Rust service/API integration | `backend/services/tests` 或仓库既有集成测试目录 | 资格权限、销售生效任务、事务回滚、并发超采、作废/减量恢复、审计 |
| Frontend unit | `features/sales-orders`、`features/purchase-orders`、`features/work-items` | 责任显示、进度模型、入口可见性、数量输入和业务文案 |
| Playwright E2E | `e2e/tests/flow-procurement-responsibility-multi-po.spec.ts` | P0 跨层真实用户链路 |
| Performance | 独立测试脚本/ignored Rust 测试 | 批量规则、大量依据、并发和稳定性 |

## 4. 执行命令

最终以仓库实际脚本为准，优先使用统一入口。计划至少执行：

```bash
cd backend && cargo fmt --all -- --check
cd backend && cargo clippy --workspace --all-targets --all-features -- -D warnings
cd backend && cargo test --workspace --all-features

cd erp-client && npm run lint
cd erp-client && npm run test
cd erp-client && npm run build

cd e2e && npx playwright test tests/flow-procurement-responsibility-multi-po.spec.ts
```

若仓库使用 Bun/pnpm 或统一 `scripts/test.sh`，执行时改用仓库已有命令并在报告中记录实际命令。

## 5. CI 分层

1. 快速门禁：Rust fmt/clippy、前端 lint/typecheck、纯单元测试。
2. 数据库门禁：责任、任务、剩余数量和并发服务层集成测试。
3. 构建门禁：前端生产构建、后端 workspace 编译。
4. 浏览器门禁：主 P0 Playwright 流程。
5. 定时门禁：大数据量和并发性能测试。

## 6. 验收与停止条件

- P0 可达用例没有真实 E2E 证据时，不声明功能完成。
- 同一失败连续修复三轮仍失败时，记录现象、尝试和风险并停止该项。
- 浏览器发现相关路由回归、桌面或移动端操作不可达时，修复后重新执行完整相关路径。
- 执行报告必须区分通过、失败、跳过、blocked 和人工验证，不能以单一命令通过替代全部用例结论。
