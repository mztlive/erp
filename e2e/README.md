# ERP 一期 E2E 测试（Playwright）

基于 `docs/erp-phase-1.md` 业务流程编写的端到端测试，独立于前后端目录，自成体系。

## 结构

```
e2e/
├── playwright.config.ts        # 单 worker、长超时、失败截图/trace
├── helpers/                    # 共享工具（账号、登录、API、UI）
├── scripts/
│   ├── ensure-services.sh      # 检查/启动前后端（已启动则复用）
│   ├── stop-backend.sh         # 停止 web-api（重置前停写）
│   ├── reset-db.sh             # 数据库重置（清业务数据，保留账号/主数据，不填充种子）
│   ├── restart-backend.sh      # 重启 web-api（--build 时先编译）
│   ├── publish-approval-definitions.mjs  # 发布 12 个 PROCESS_REQUIRED 类型审批定义
│   └── run-flow.sh             # 单流程编排：服务→reset→重启→发布定义→跑 spec
└── tests/                      # 每个 .spec.ts 文件 = 一个业务流程
```

## 运行

```bash
# 安装依赖（首次）
npm install
npx playwright install chromium

# 运行单个流程（推荐：自动 reset + 重启 + 发布定义）
E2E_RESET=1 E2E_ALLOW_REMOTE_RESET=1 bash scripts/run-flow.sh tests/flow-01-sales-warehouse.spec.ts

# 依序运行全部流程
E2E_ALLOW_REMOTE_RESET=1 bash scripts/run-flow.sh all

# 跳过 reset 快速调试（数据不复位，谨慎）
E2E_RESET=0 bash scripts/run-flow.sh tests/flow-01-sales-warehouse.spec.ts
```

### 环境变量

| 变量 | 默认 | 说明 |
|------|------|------|
| `E2E_RESET` | `1` | `1`：跑 spec 前 stop → reset DB → restart → 发布审批定义；`0`：跳过，适合快速复跑 |
| `E2E_ALLOW_REMOTE_RESET` | （无） | reset 远端/共享库时需显式设为 `1`，防止误清 |
| `E2E_HEADED` | `0` | `1`：打开真实浏览器窗口；窗口默认最大化，页面 viewport 跟随实际可用尺寸 |
| `E2E_SLOW_MO` | （空） | 每个动作间隔毫秒数，写入 `playwright.config.ts` 的 `launchOptions.slowMo`。Playwright Test CLI 没有 `--slow-mo`，不要把它当命令行参数传 |

### 观察浏览器操作

默认无头（`headless: true`）。所有 flow、浏览器 probe 与 `repro-*.mjs`
统一使用 `E2E_HEADED=1` 打开最大化窗口。需要看见点击、填表、跳转时：

```bash
# 有界面观察（仍走完整 reset 编排）
E2E_HEADED=1 E2E_ALLOW_REMOTE_RESET=1 bash scripts/run-flow.sh tests/flow-01-sales-warehouse.spec.ts

# 有界面 + 慢动作（500ms/动作，更容易跟）
E2E_HEADED=1 E2E_SLOW_MO=500 E2E_ALLOW_REMOTE_RESET=1 bash scripts/run-flow.sh tests/flow-01-sales-warehouse.spec.ts

# 跳过 reset + 有界面（数据已就绪时快速复看）
E2E_RESET=0 E2E_HEADED=1 bash scripts/run-flow.sh tests/flow-01-sales-warehouse.spec.ts
```

其它调试方式（不经 `run-flow.sh`，需自行保证服务与数据就绪）：

```bash
# Playwright UI 模式：可视化选用例、看时间线
npx playwright test tests/flow-01-sales-warehouse.spec.ts --ui

# 逐步调试（Playwright Inspector）
npx playwright test tests/flow-01-sales-warehouse.spec.ts --debug

# 只跑某个 test 标题（grep）
npx playwright test tests/flow-10-stock-adjustment.spec.ts -g "盘亏调整单" --headed --workers=1
```

## 关键约定

- **每个流程从 0 开始**：run-flow.sh 在每个 spec 前执行数据库 reset。
  reset 清理客户/合同/销售单/采购单/票款/库存/审批实例等业务数据，
  **保留账号/RBAC（4 个测试账号密码均为 123456）、供应商/商品/仓库主数据、
  source_systems、file_assets、审计、编号计数器**；不填充任何种子数据。
- **账号**：xiaoshou(销售) / caigou(采购) / yunying(运营) / caiwu(财务) / lisiyong(销售领导) / admin(超级管理员)，密码均为 `123456`。所有 `flow-*` 流程在同一个浏览器页面内串行切换账号，不为账号额外创建窗口。
- **审批定义**：reset 会删除全部审批定义，按合同必须先创建并发布定义才能开单。
  `publish-approval-definitions.mjs` 幂等补齐 12 个 PROCESS_REQUIRED 类型的定义。
- **岗位分离**：提交人不得审批自己的单据，各类型审批人按部门职责分配（见发布脚本）。
- **服务**：优先复用已启动的前后端（后端 :10001、前端 :3000），未启动才拉起。
- **修复后的重启**：若测试暴露代码问题并修复，对应服务会重启（后端 `restart-backend.sh --build`；前端 next dev 热更新，必要时重启）。

## 文档与代码不一致时的处理

以代码为准。测试中发现文档与实现不一致时，在 spec 头部注释和本目录 README 的
「文档-代码差异记录」中登记，并在最终汇报中列出风险。
