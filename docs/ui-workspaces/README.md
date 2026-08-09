# ERP 工作面详细设计

本目录把 `erp-ui-design.md` 中的 W01–W30 工作面展开为可实施、可验收的页级设计契约。

## 文档边界

- `erp-ui-design.md`：全局视觉语言、应用壳、M1–M7 页面模式、权限和状态通用契约。
- 本目录的 `wxx-*.md`：单个工作面的布局、内容、操作、数据和验收标准。
- `erp-phase-1.md` / `erp-phase-2.md`：业务范围、状态机和职责边界。
- `erp-data-model.md`：实体、字段、不变量和查询投影。
- `erp-ui-flows.md`：跨工作面的高频完整流程。

W 文件不得重定义业务状态机，也不得为了版面方便改变金额、权限或来源口径。

## 编写规则

1. 一个 Wxx 只对应一个文件；内部可以包含多个路由、Tab、Sheet、Dialog 和状态视图。
2. 文件名固定为 `wxx-kebab-case.md`，W 编号使用两位数，不因名称调整而改变。
3. 所有可见字段必须注明数据来源；派生值必须注明计算方和新鲜度。
4. 所有可见操作必须注明权限、可用条件、成功结果和失败恢复。
5. W 文件正文仅得写入已确认的业务规则与数据/操作契约；禁止写入未确认规则，禁止设立「待确认事项」表或同类过程隔离区。
6. 页面实现完成不等于设计完成；必须通过文件内的验收清单。
7. **M4 对象中心页头契约**（与 `erp-ui-design.md` §4.5.1、代码 `components/business` 对齐，禁止漂移）：
   - 列表/工作台：`PageHeader variant="page"`（默认）+ 工作面 title。
   - 对象/批次详情：`PageHeader variant="object-chrome"`（仅面包屑 + 返回/刷新）+ `DocumentHeader density="compact"`（唯一身份 h1）。
   - **禁止** `PageHeader(title=工作面名)` 再叠 `DocumentHeader(title=对象名)`。
   - 可选指标：`MetricStrip density="compact"`；业务风险 `detailMode="inline"`，实现/口径旁白 `tooltip` 或 `none`。
   - 线框与区域表必须写明上述 variant/density，不得只写笼统的 `PageHeader` + `DocumentHeader`。
8. **用户可见文案契约**（与 `../ui-glossary.md` 对齐，禁止漂移）：
   - 界面上的按钮、状态、提示、空态、错误恢复一律使用业务语言。任务领取、动作命令、
     work_item、对象版本、幂等键等实现词只出现在本目录正文、代码注释和字段名里；
     租约、令牌、信封、指纹等分布式协议概念已从契约中移除。
   - **枚举原值**（`POSTED`、`BLOCKED`…）和**内部 ID**（`rsv_*`、`pla_*`…）不得进入界面；
     新增枚举必须同时提供中文映射。
   - 跨角色的措辞差异用**组件 props** 表达，不得修改 `components/business` 的默认文案 ——
     那会波及其它工作面。
   - W 文件写线框和文案示例时必须直接写最终用词；禁止先写实现术语再指望实现时翻译。
9. **URL 参数与界面控件必须一一对应**：任何被查询消费、却没有对应控件也无法清除的参数，
   都是用户改不动的隐形状态。要么补控件，要么从查询里摘掉。筛选契约表必须标注实现状态
   （已实现 / 未实现）；禁止将未实现项表述为已具备。

## 状态定义

| 状态 | 含义 |
| --- | --- |
| 样板 | 仅固定文档结构与详细度模板；业务契约不完整，禁止按此实施 |
| 草稿 | 已形成初步页级设计；未升为「已确认」前禁止按此实施 |
| 评审中 | 页级设计已提交确认；未升为「已确认」前禁止按此实施 |
| 已确认 | 布局、内容、操作和数据契约完整，必须按此实施 |
| 已实现 | 样板或正式页面已按契约落地；验收清单尚未全部通过 |
| 已验收 | 文档验收项与实现验证均通过；为当前有效实施依据 |

## 当前索引

| ID | 工作面 | 模式 | 主要路由 | 文档 | 状态 |
| --- | --- | --- | --- | --- | --- |
| W01 | 今日工作台 | M1 | `/workspace` | [w01-today-workspace.md](w01-today-workspace.md) | 已实现 |
| W02 | 待办队列（统一） | M3 | `/workspace/tasks` | [w02-unified-task-queue.md](w02-unified-task-queue.md) | 已实现 |
| W03 | 客户中心 | M4 | `/sales/customers` | [w03-customer-center.md](w03-customer-center.md) | 已实现 |
| W04 | 合同 | M2 + M4 | `/sales/contracts` | [w04-contracts.md](w04-contracts.md) | 已实现 |
| W05 | 销售单（统一） | M2 + M4 + M5 | `/sales/orders`（列表行点纸质预览，无侧栏 Sheet） | [w05-sales-orders.md](w05-sales-orders.md) | 已实现 |
| W06 | 客户验收 | 挂 W05 + 作业 | `/sales/orders/:salesOrderId?section=acceptance` | [w06-customer-acceptance.md](w06-customer-acceptance.md) | 已实现 |
| W07 | 二次确认队列 | M3 | `/procurement/confirm` | [w07-procurement-confirmation-queue.md](w07-procurement-confirmation-queue.md) | 已实现 |
| W08 | 采购单 | M2 + M4 + M5 | `/procurement/orders` | [w08-purchase-orders.md](w08-purchase-orders.md) | 已实现 |
| W10 | 库存台账 | M2 + M6 | `/inventory` | [w10-inventory-ledger.md](w10-inventory-ledger.md) | 已实现 |
| W11 | 客户往来 | M2 + M5 | `/finance/customer-accounts` | [w11-customer-receivables.md](w11-customer-receivables.md) | 已实现 |
| W12 | 供应商往来 | M2 + M5 | `/finance/supplier-accounts` | [w12-supplier-payables.md](w12-supplier-payables.md) | 已实现 |
| W13 | 卡券票款复核 | M3 | `/finance/card-funds-review` | [w13-card-funds-review.md](w13-card-funds-review.md) | 已实现 |
| W14 | 商品与 SKU（公司商品池）、商品分类、品牌、卡券类目、供应商与仓库 | M2 + M4 | `/master-data/:resource` | [w14-basic-data.md](w14-basic-data.md) | 已实现 |
| W15 | 客户经营质量 | M6 | `/analytics/customer-quality` | [w15-customer-business-quality.md](w15-customer-business-quality.md) | 已实现 |
| W16 | 实际经营盈亏 | M6 | `/analytics/profit-loss` | [w16-actual-profit-loss.md](w16-actual-profit-loss.md) | 已实现 |
| W17 | 商城同步与映射 | M7 | `/governance/mall-sync` | [w17-mall-sync-mapping.md](w17-mall-sync-mapping.md) | 已实现 |
| W18 | 导入与期初 | M7 | `/governance/imports` | [w18-import-opening.md](w18-import-opening.md) | 已实现 |
| W19 | 权限与审计 | M2 | `/system/access-audit` | [w19-permissions-audit.md](w19-permissions-audit.md) | 已实现 |
| W20 | API 供应商连接 | M2 + M4 | `/supplier-api/connections` | [w20-supplier-api-connections.md](w20-supplier-api-connections.md) | 已实现 |
| W21 | 供应商供给 | M2 + M4 | `/procurement/supplier-offerings` | [w21-supplier-offerings.md](w21-supplier-offerings.md) | 已实现 |
| W22 | 商品发布 | M2 + M4 | `/commerce/publications` | [w22-product-publication.md](w22-product-publication.md) | 已实现 |
| W23 | 执行投影 | M2 + M4 | `/commerce/execution-projections` | [w23-execution-projection.md](w23-execution-projection.md) | 已实现 |
| W25 | 商城消费订单 | M2 + M4 | `/commerce/consumption-orders` | [w25-mall-consumption-orders.md](w25-mall-consumption-orders.md) | 已实现 |
| W26 | 供应商订单 | M2 + M4 | `/supplier-api/orders` | [w26-supplier-orders.md](w26-supplier-orders.md) | 已实现 |
| W27 | API 结算 | M2 + M4 | `/supplier-api/settlements` | [w27-api-settlement.md](w27-api-settlement.md) | 已实现 |
| W28 | 卡券消费台账与经营分析 | M6 | `/analytics/card-business` | [w28-card-consumption-analytics.md](w28-card-consumption-analytics.md) | 已实现 |
| W29 | 接口错误与对账中心 | M7 | `/governance/integration-errors` | [w29-integration-error-reconciliation.md](w29-integration-error-reconciliation.md) | 已实现 |
| W30 | 历史消费回填 | M7 | `/governance/history-backfill` | [w30-historical-consumption-backfill.md](w30-historical-consumption-backfill.md) | 已实现 |

索引中的路由是工作面级导航契约。对象详情、编辑态、侧栏和弹窗的具体路径或状态由各 W 文件定义；若调整主路由，必须同步本索引、导航配置和所有跨 W 跳转。

> 注：主责迁移机制已取消（T 起商城停单、ERP 全面服务，见 erp-data-model.md §6.16）。
