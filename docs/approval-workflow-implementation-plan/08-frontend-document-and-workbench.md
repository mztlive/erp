# 阶段 08：前端单据审批区与工作台

> 阶段性质：P4 前端工作包；按试点和 DocumentType 拆分页面接入子阶段
>
> 阶段目标：用统一服务端审批事实替换卡券专用审批和受阻重试交互
>
> 允许状态：可按阶段 06 DTO 开发；试点所需后端共享入口由 P0-B 完成，W24 与前端权限生成由 P0-C 完成，两者都必须在 P6-PILOT 前合并

## 1. 文件责任

新增通用 feature：

```text
erp-client/features/approval-workflow/
├── api.ts
├── types.ts
├── queries.ts
├── components/definition-binding-card.tsx
├── components/submission-route-confirmation.tsx
├── components/runtime-summary.tsx
├── components/decision-dialog.tsx
├── components/execution-history.tsx
├── components/resume-approver-dialog.tsx
├── components/reassign-dialog.tsx
├── components/upgrade-binding-dialog.tsx
├── components/cancel-approval-dialog.tsx
└── **/*.test.tsx
```

修改：

- `erp-client/features/work-items/{api,types,queries,display}.ts*`；
- `erp-client/features/workspace/**`；
- 试点单据及其余 DocumentType 的创建结果、详情和提交确认工作面；每个业务页面域一个独立 P4 子阶段。

删除：

- `erp-client/features/unified-task-queue/**`：列表查询、筛选、排序、分页和路由跳转能力先并入 `workspace`，再整目录删除；
- `erp-client/app/(workspace)/workspace/tasks/page.tsx` 的独立页面实现，替换为到 `/workspace` 的永久重定向。

本阶段不得一次性跨多个业务页面域提交一个 PR。接入顺序必须与阶段 04 相同：通用组件、试点 `StockAdjustment`、`P6-PILOT`、其余 11 个 `PROCESS_REQUIRED` 类型逐批接入、`P6-FINAL`。8 个 `NO_APPROVAL` 类型不接入审批区。

## 2. 单据创建与详情

任何需要审批的单据创建成功后必须只读展示服务端返回的绑定：流程名、版本和有序节点/审批人。页面不得提供选择定义、增删节点、排序或换人的控件。

详情页面统一嵌入：

- 未提交：`definition-binding-card`；
- 提交确认：`submission-route-confirmation`；
- 运行中/终态：`runtime-summary + execution-history`；
- 当前本人有开放审批任务：显示决定入口；
- 当前用户有运行管理权限且实例受阻：运行管理区只显示服务端 `recovery_options` 授权的恢复、改派或受阻取消入口；
- 未提交且服务端允许升级绑定：显示“更新审批流程版本”入口；
- 实例运行中，或因人员失效受阻，且原提交人拥有服务端撤回动作：显示“撤回审批”入口；具备类型运行管理权的管理员按服务端授权显示“应急撤回审批”。非人员一致性 blocker 不显示这两个入口。

各类型页面必须使用相同组件和响应结构。允许业务页面决定放置位置，不得复制审批状态推导逻辑。页面不得仅凭单据状态显示升级、撤回或决定按钮，必须读取服务端 `allowed_actions`。

前端不得感知 `bpm` crate、`ProcessKind`、`SubjectRef`、`TransitionPlan` 或 BPM 内部事件。页面所需单据类型、路由、责任、中文文案和允许动作必须全部来自 ERP Service/HTTP 投影。

## 3. 决定交互

决定弹窗只能提交 `work_item_id`、`APPROVE|REJECT`、原因、`expected_task_version` 和幂等键。

规则：

- 驳回原因必填；通过原因按合同可选；
- 请求进行中禁用重复提交；
- 幂等键在同一次用户意图重试时保持不变，用户修改决定或原因后生成新键；
- 成功后用响应事实更新实例、单据和任务缓存；
- `409` 时刷新任务和实例，展示“责任或版本已变化”；不得自动重放决定；
- 页面不得提交下一节点、驳回目标或下一审批人。

## 4. 运行摘要与历史

运行摘要必须直接展示：审批状态、当前轮次、当前节点、当前审批人、最近驳回和驳回原因。实例 `BLOCKED` 必须使用独立受阻样式，不得伪装为普通待办。

历史使用 `useInfiniteQuery` 调用游标端点，按 `round_no` 分组、按 `execution_no` 排序。已结束执行只读；同一节点跨轮次必须显示多条记录，不得按 `node_key` 去重。详情首屏只消费受控 `recent_history`，不得假设服务端返回全量历史。

## 5. 统一工作台（W01 与 W02 合并）

按合同 §16.4，原 W01 今日工作台与 W02 统一待办队列合并为唯一页面。

### 5.1 路由与目录

| 项 | 固定值 |
| --- | --- |
| 唯一路由 | `/workspace` |
| 旧路由 | `/workspace/tasks` 保留为永久重定向到 `/workspace`，不保留第二套页面代码 |
| 目标目录 | `erp-client/features/workspace/**` |
| 删除目录 | `erp-client/features/unified-task-queue/**`；其列表查询、筛选、排序、分页和路由跳转能力先并入 `workspace`，再整目录删除 |
| TaskTabs 身份 | 仍为 `workspace:today:{userId}`，不新增第二个待办页签 |

### 5.2 布局

固定为**列表 + 详情主从**，页内连续处理：

```text
┌───────────────────────┬──────────────────────┐
│[待我处理 12][超期 3][受阻 1][我发起的 5]      │
├───────────────────────┼──────────────────────┤
│▸销售单审批 SO-0031  ● │ SO-2026-0031         │
│ 采购确认   SO-0044    │ 客户A·12.8万·8行     │
│ 库存调整   IA-0011    │ 流程 v3 · 第 2 轮    │
│ 回款复核   RC-0203    │ 当前节点：销售审核   │
│ 资质到期   SUP-0007   │ 上轮驳回：王五 / …   │
│                       │ ────────────────────  │
│                       │ [通过][驳回][打开单据]│
└───────────────────────┴──────────────────────┘
```

1. 指标带是筛选器：点击写入 URL 查询参数并筛选左列，不打开第二个页面、不跳路由；
2. 左列是唯一待办列表，跨领域混排，按任务类型页签收敛，承接原 W02 的全部查询能力；
3. 右侧详情按任务类型分派渲染：审批任务渲染通用审批区（第 3、4 节），非审批任务渲染业务摘要与「打开单据」；
4. 审批任务在详情内直接提交通过或驳回，成功后自动选中列表下一条实现连续处理；
5. 非审批任务的正式动作仍在对应业务工作面提交，本页面不提供通用完成表单；
6. 详情区不得提供批量通过、批量驳回或批量转交；
7. 移动端与窄屏下详情降级为全屏抽屉，返回后保留列表筛选与滚动位置。

### 5.3 分区口径

页面只有一个「待我处理」口径，**不存在团队分区**：

1. 待我处理：`OPEN + owner_user_id=当前用户`，审批与非审批任务同一口径；
2. 类型页签（审批 / 履约 / 财务 / 集成……）只筛选，不改变数量口径；
3. 我发起的审批：实例 `started_by`；
4. 受阻：关联实例或执行为 `BLOCKED`，使用独立受阻样式；
5. 管理视图：仅有运行管理权限时显示，与本人口径分开计数。

当前责任、轮次和下一步全部读取 API 字段。不得根据业务状态、`workflow_action` 或前端角色推断。指标数量必须来自服务端，不得对已加载条目求和。

“开始处理”“退回团队”“领取”已按合同 §13.2 从全系统删除，任何任务类型都不得显示这些动作；审批任务另外不得显示“通用转交”或“关闭”。

## 6. 人员恢复、管理员改派与受阻取消

将 `blocked-approval-list.tsx` 的“重试当前步骤”替换为服务端恢复动作。页面只读取 `recovery_options` 和 `allowed_actions`：原审批人已恢复时显示“恢复当前审批人”，原审批人仍失效且存在合格目标时显示“改派当前审批人”，非人员一致性 blocker 只对运行管理员显示“取消受阻审批”。

恢复当前审批人弹窗不得选择用户，只提交 instance/execution/assignment/closed-task 版本和幂等键。成功后必须展示新执行和新 OPEN 任务；不得重新打开旧任务或重放原决定。

改派弹窗必须：

- 显示原定义审批人和当前实例审批人；
- 通过服务端资格搜索选择具体用户；
- 要求非空原因；
- 提交 instance/execution/assignment 版本、可空的已关闭 task 版本和幂等键；
- 成功后展示新的当前责任人和恢复状态；
- `409` 时刷新事实并要求重新确认。

不得向用户暴露 `RETRY_CURRENT_STEP` 或“自动换成角色池”的选择。受阻取消必须展示 blocker 分类、取消后的业务状态和不可恢复提示，并调用专用受阻取消端口；不得调用通用 WorkItem close。

## 7. 绑定升级与撤回

### 7.1 更新未提交单据流程版本

管理员入口必须展示当前绑定版本、目标当前发布版本、路线差异和影响说明。表单使用 `useAppForm + Zod`，提交原因、业务版本、绑定版本和幂等键，不允许选择任意历史定义。409 时保留输入、刷新两种版本并要求重新确认。

### 7.2 撤回审批

原提交人的撤回入口只在服务端 `allowed_actions` 允许时显示。撤回原因一律必填；运行管理员应急撤回还必须具备该类型运行管理权并显示应急代办提示，前端不得以“管理员”角色名自行放行。确认框必须说明当前节点和撤回后的业务状态，提交业务接口要求的实例/执行版本、可空任务版本、原因和幂等键；只接受 `RUNNING` 或人员失效类别的 `BLOCKED`，`BLOCKED` 无开放任务时任务版本必须为空。非人员一致性 blocker 只显示运行管理员受阻取消入口。不得调用通用 WorkItem close，也不得在前端先修改单据状态。

## 8. 旧卡券审批清除

销售单 P4 子阶段完成通用组件接入时必须删除：

- `erp-client/features/sales-orders/api/card-approval.ts`；
- `erp-client/features/sales-orders/components/card-sales-approval-dialogs.tsx`；
- `card-sales-approval-forms.tsx`；
- `card-sales-approval-panel.tsx`；
- `hooks/use-card-approval-model.ts`；
- `hooks/use-card-approval-actions.ts` 及对应旧语义测试。

销售单详情改用通用审批 feature。`TERMINATE_APPROVAL`、`START_PROCESSING` 和运营审批 `POOL` 文案/类型必须清零。采购二次确认不再有独立业务任务工作面：它是 `SalesOrder` 定义中的普通审批节点，只走通用审批区。

## 9. 查询与缓存

使用 TanStack Query 建立稳定键：

```text
approval.instance(instanceId)
approval.document(documentType, documentId)
approval.history(instanceId, cursor)
approval.instances("mine", filters, cursor)
approval.instances("started", filters, cursor)
approval.instances("managed", filters, cursor)
approval.instances("blocked", filters, cursor)
```

Mutation 成功后精确更新/失效工作项、实例和对应业务单据。背景刷新使用现有数据占位，避免列表切换整页闪烁。页面筛选必须进入 URL 状态并由服务端查询，不得前端全量过滤 DataScope。

所有请求只能通过 TanStack Query；业务 API 函数使用现有 `ResultAsync` envelope。所有业务组件使用 `"use client"`，不得通过 RSC/SSR 取数；不得使用 `useEffect` 手写请求。

## 10. 用户术语

统一使用：

```text
审批流程
审批节点
当前轮次
当前审批人
通过
驳回
受阻
改派当前审批人
待我处理
待我审批
我发起的审批
```

不得显示内部 `instance`、`execution`、`assignee binding`、`POOL`、`DIRECT`、`retry` 等技术词。

新增函数、hook 和组件必须有 JSDoc。枚举必须显式映射中文；内部 ID 不得作为用户文案。所有文案必须通过 `docs/ui-glossary.md`。

## 11. 测试

必须覆盖：

- 创建后有绑定展示但无待办；
- 提交确认的路径与固定驳回说明；
- 决定请求白名单和幂等键生命周期；
- 驳回后轮次递增且历史不覆盖；
- 审批任务没有通用工作项动作；
- 受阻与普通待审批视觉和动作不同；
- 管理员改派的版本冲突处理；
- 原审批人恢复后创建新执行和新任务，不出现无任务死路；
- ACTIVE 或结构性 blocker 不显示改派；
- 非人员一致性 blocker 只显示受阻取消，不显示恢复或改派；
- 更新未提交绑定的路线差异、请求白名单和版本冲突；
- 撤回只调用业务资源接口且遵循 `allowed_actions`；
- 工作台无团队分区：指标带筛选不跳页，审批决定可在详情内连续提交；
- `/workspace/tasks` 重定向到 `/workspace`，不存在第二套待办页面；
- 无对象读取权时不渲染敏感业务字段；
- 卡券旧枚举、API 和组件引用全部消失。

## 12. 阶段验收

- [ ] 试点先通过 P6-PILOT；其余 DocumentType 分批通过统一组件展示服务端事实，并在 P6-FINAL 完成全量验收。
- [ ] 前端不存在审批责任、当前节点或下一节点推导。
- [ ] 前端不存在 `ProcessKind`、`SubjectRef`、`TransitionPlan` 或 BPM 内部事件模型。
- [ ] 工作台只有「待我处理」口径，无团队待处理分区；`/workspace/tasks` 重定向到 `/workspace`。
- [ ] 驳回只表达“从第一节点开始下一轮”。
- [ ] 人员受阻只提供“恢复当前审批人”或“改派当前审批人”，两者都创建新执行和新任务，不提供通用重试。
- [ ] 非人员一致性受阻只提供受阻取消，不提供换人或继续推进。
- [ ] 未提交单据升级和运行中撤回均有明确入口、版本和冲突处理。
- [ ] API 使用 ResultAsync，Query/Form 遵循仓库约定，新增方法具备 JSDoc。
- [ ] 目标 feature 测试、类型检查和 lint 通过；真实浏览器验收由阶段 11 执行。
