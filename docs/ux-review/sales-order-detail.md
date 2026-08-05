# UX 评审报告：/sales/orders/[salesOrderId]（销售单详情）

> 评审日期：2026-08-05
> 评审角色：产品经理 / UX 视角（纯分析，未改代码）
> 评审范围：
> - `features/sales-orders/sales-order-detail-page.tsx`
> - `features/sales-orders/acceptance-workspace.tsx`
> - `features/sales-orders/card-sales-approval-panel.tsx`
> - `features/sales-orders/close-conditions-card.tsx`
> - `features/sales-orders/procurement-rejection-card.tsx`
> - `features/sales-orders/revision-history-card.tsx`
> - `features/sales-orders/sales-order-paper-dialog.tsx`
> - 关联：`components/business/{document,workflow,feedback,values,page}.tsx`、`lib/ui-text.ts`、`features/sales-orders/{queries,api,acceptance-types}.ts`、`docs/ui-glossary.md`

---

## 一、页面概述

销售单详情是 W05 的「对象中心」页：单一叙事（当前要办）+ 多 Tab 分区（本单内容 / 采购未通过 / 卡券审批 / 客户验收 / 进度与结案 / 商城对接 / 历史版本），承载三种业务路径：

1. **审批路径**（卡券单）：领取 → 领导通过 → 运营通过并生效，弹窗逐环节确认（`card-sales-approval-panel.tsx`）；
2. **验收路径**（实物/服务单）：选择可验收履约批次 → 分配数量 → 填通过/短少/拒收 → 确认并完成验收，支持草稿与冲正（`acceptance-workspace.tsx`）；
3. **异常路径**（采购驳回）：改完再报 / 请领导批低毛利 / 作废三选一（`procurement-rejection-card.tsx`）。

另有「发起改单」（版本化修订）与只读结案说明（`close-conditions-card.tsx`）、版本时间线（`revision-history-card.tsx`）。纸质预览（`sales-order-paper-dialog.tsx`）仅从列表页进入。

整体架构优秀：Tab 切换用 `router.replace` 保留 `returnTo/from` 返回上下文（sales-order-detail-page.tsx:207-223），「当前要办」只保留一条叙事并限制主按钮出现位置（sales-order-detail-page.tsx:471-498），返回按钮按来源队列给出「返回采购确认/返回履约处理」等语境化文案（sales-order-detail-page.tsx:257-266）。以下问题集中在验收工作区与卡券审批的处理流。

---

## 二、易用性

**顺畅的部分**
- 状态流转入口集中且分层：Header 主按钮 → 聚焦 Alert「去处理」→ Tab 徽章「待办」三层均指向同一目标，无重复动作按钮。
- 卡券单「到期交付」与实物单「验收交付」的差异说明在 Header meta、验收 Tab、结案卡三处口径一致（sales-order-detail-page.tsx:403-413、826-836；close-conditions-card.tsx:52-58）。
- 采购驳回三种出路以三列卡片并列，与「领导批低毛利」进行中的状态分区块展示，路线清晰（procurement-rejection-card.tsx:389-526）。
- 冲正/改单/审批均带影响确认弹窗（状态变化 + 锁定字段 + 影响 + 下一责任部门），误操作防护到位。

**不顺畅的部分**
- **冲正流程被确认弹窗阻断**：点「冲正误录」会同时（1）打开确认弹窗、（2）在弹窗下层渲染「冲正理由」输入卡（acceptance-workspace.tsx:1271-1274 触发、1392-1432 弹窗、1434-1455 输入卡）。弹窗遮罩盖住输入框，用户首次确认必然因「请填写冲正理由」失败（1414-1422），必须关弹窗 → 补输入 → 重新触发，无任何提示。
- **审批驳回的「驳回说明」写了也白写**：表单校验要求 ≥4 字（card-sales-approval-panel.tsx:55-58），但提交只带 `reasonCode`，`comment` 从未进入 payload（card-sales-approval-panel.tsx:316-341）——用户花时间写的原因不会到达销售端。
- **验收退出无未保存防护**：「退出登记」直链跳走（acceptance-workspace.tsx:1300-1308），已填的数量/结果未保存即静默丢失（草稿仅显式保存时落服务端）。
- 卡券审批领取后无「释放」入口，领错只能刷新页面（刷新即丢处理权，文案有说明，属可接受但有感）。

---

## 三、信息密度

- **首屏**（详情页）：面包屑 + 返回 + 聚焦 Alert + DocumentHeader（标题/主状态/三轨状态/4 个操作按钮）+ 4 项金额指标 + 7 个 Tab，采用 compact 密度，整体偏满但分层清晰；Header 状态轨（交付/回款/开票）与指标条（已回款/待回款/已开票）存在轻度信息重叠（回款状态与金额各出现一次），可接受。
- **验收工作区** xl 双栏 62/38：左栏事实池（最大高 32rem 内部滚动）+ 右栏「本次验收」表单与「验收历史」——密度合理；底部 sticky 栏与右卡页脚重复展示「已选 N 个来源 / 总体结果 / 确认并完成验收」，同屏出现两套相同主动作（acceptance-workspace.tsx:1172-1192 与 1293-1328），显冗余。
- **关键信息突出度**：状态（Header + Tab 徽章）、金额（指标条）、客户（Header 标题）、进度（三轨 + 结案卡）均到位；验收工作区「待验收批次/待验收数量/交付进度/数据更新时间」四指标满足任务需要。
- 验收历史行（单号 + 徽章 + 总体结果 + 分数量 + factOnlyNotice + 冲正按钮）略密，但信息都有业务价值。

---

## 四、交互合理性

- **加载/空/错误状态**：详情页加载为纯文字（sales-order-detail-page.tsx:225-231）；验收区加载为双骨架卡（acceptance-workspace.tsx:602-613），且错误态提供「重试」（615-628）。但**详情页查询失败被渲染成「销售单不存在」**（sales-order-detail-page.tsx:233-245）：网络错误与「单据不存在」混淆，且无重试按钮——这是错误语义失真。
- **状态禁用逻辑**：`发起改单` 禁用时带 `title` 说明原因（455-460），正文区另有 blocker 文案（500-532）；验收表单按 `canPost/canSave` 禁用且不隐藏，符合「禁用不解释不如隐藏」的规避——合理。但验收区存在「演示：收回权限/恢复权限」按钮（759-788），真实用户可误触改变自己的权限状态。
- **反馈提示**：所有正式动作均有 `FormalActionResult`（成功/阻断/结果未知三态，未知态带「原任务号」可重试，符合术语表 §3.4）。
- **误操作防护**：数量守恒/超分配/原因必填校验 + `ValidationSummary` 定位跳转（feedback.tsx:549-604）做得扎实；`⌘S/⌘Enter` 快捷键存在但无提示入口（acceptance-workspace.tsx:389-405）。
- **回退路径**：详情页返回带队列上下文；验收「退出登记」直链返回；审批无释放入口（见上）。
- **数据保真问题（重点）**：「标记为服务不通过」勾选仅存在于客户端 `LineResultState`（acceptance-workspace.tsx:78-85），既不入草稿 payload（buildDraftLines，145-181）也不入提交 payload；刷新恢复草稿时被硬编码为 `serviceFail: false`（366）。前端展示总体结果「服务不通过」（deriveOverall，128-143），实际提交数据不含该标志——展示与落库不一致，正式验收记录失真。
- 分配数量一改，「通过数量」自动覆盖为分配合计（autoFillLineResult，256-277；setAllocQty，562-582）——用户手工填过的「通过数量」会被静默覆盖。

---

## 五、问题清单（按严重度）

### P0（阻断操作）

**P0-1 冲正理由输入框被确认弹窗遮罩，冲正首次提交必然失败**
- 位置：acceptance-workspace.tsx:1392-1432（弹窗）+ 1434-1455（理由输入卡）
- 现象：点「冲正误录」（1271-1274）同时打开弹窗并在遮罩下渲染理由输入卡；弹窗内必填校验（1414-1422）在理由为空时报「冲正失败：请填写冲正理由」，但此时输入框不可达。用户必须「关弹窗 → 填理由 → 重新触发」，且界面无任何提示说明理由在哪填。
- 影响：验收纠错（冲正）主路径第一步必败，属断链交互。

**P0-2 「服务不通过」标记不进入草稿/提交数据，验收结果失真**
- 位置：acceptance-workspace.tsx:145-181（buildDraftLines 无 serviceFail）、366（草稿恢复硬编码 false）、1093-1106（勾选控件）
- 现象：勾选「标记为服务不通过」后，前端把总体结果判为 SERVICE_FAIL（128-143）并在确认弹窗/结果展示为「服务不通过」，但提交的验收行只有通过/短少/拒收数量与原因，不含该标志；刷新恢复草稿后勾选丢失。
- 影响：正式验收记录无法表达「服务不通过」结论，展示与落库不一致，下游变更/异常流程拿不到正确信号。

### P1（明显阻碍效率）

**P1-1 详情查询失败渲染成「销售单不存在」且无重试**
- 位置：sales-order-detail-page.tsx:233-245
- 现象：`query.isError` 时 `order` 为 undefined，页面提示「未找到编号为 X 的销售单」。网络抖动/接口失败与单据不存在语义混淆，误导用户去核对单号；无重试按钮。
- 建议：区分 isError（系统加载失败 + 重试）与 404（单据不存在 + 返回列表）。

**P1-2 卡券审批「驳回说明」校验通过却从不提交**
- 位置：card-sales-approval-panel.tsx:55-58（schema 要求 comment ≥4 字）、316-341（onConfirm 只传 reasonCode）
- 现象：驳回说明必填、占位符引导「写清要销售改什么」，但提交 payload 不含 comment，销售端只能看到分类，看不到说明。
- 建议：把 `rejectPayload.comment` 传入 `completeMutation`（接口需支持 comment 字段）。

**P1-3 验收工作区未保存退出无任何确认**
- 位置：acceptance-workspace.tsx:1300-1308（退出登记）、1172-1192（保存草稿为显式动作）
- 现象：已录入的分配/数量/原因仅存客户端 state，草稿需手动保存；点「退出登记」或浏览器返回即静默丢失。验收为多行长表单，误退代价高。
- 建议：复用 `DiscardConfirmDialog`（components/business/feedback.tsx:1039-1066）做 dirty 检测（对比 form values + selected/lineResults 与已存草稿）。

**P1-4 验收确认弹窗泄漏内部实现词（字段名/表名/枚举）**
- 位置：acceptance-workspace.tsx:1348-1360
- 现象：lockedFields 含「销售单 lockVersion」；effects 含「形成 customer_acceptance / 验收行」「写入 acceptance_fulfillment_allocation（APPLY）」「更新销售履约数据」。违反 `docs/ui-glossary.md` §2 第 5 轮 P0 类别「字段名/枚举原值上屏必须清零」（lockVersion → 数据版本；表名 → 业务描述；APPLY → 删除）。
- 影响：术语门禁必拦；用户看到数据库实现细节。

**P1-5 修改分配数量会静默覆盖手工填写的「通过数量」**
- 位置：acceptance-workspace.tsx:256-277（autoFillLineResult）、562-582（setAllocQty）
- 现象：用户手工把「通过数量」改为非分配合计后，再调整任一来源的分配数，通过数量被自动重算覆盖（仅当无短少/拒收/服务不通过时）。
- 建议：仅在首次选中时自动填充一次，之后不覆盖用户修改（可加「已修改」标记）。

### P2（体验瑕疵）

**P2-1 演示控件混入正式操作区**
- 位置：acceptance-workspace.tsx:759-788（演示：收回权限/恢复权限）；procurement-rejection-card.tsx:340-385（领导通过/驳回（演示））
- 现象：与真实操作按钮并排，样式为普通按钮，误触会改变会话权限/直接完成审批。
- 建议：收进「演示模式」面板或加醒目演示徽标（术语表 G6 允许演示标记，但应视觉隔离）。

**P2-2 「确认并完成验收」同屏双按钮**
- 位置：acceptance-workspace.tsx:1172-1192 与 1293-1328
- 现象：右卡页脚与底部 sticky 栏各一套（含保存草稿），信息重复。
- 建议：sticky 栏保留为唯一动作区，或按滚动位置切换。

**P2-3 验收数据新鲜度显示「刚刚」为时间文本**
- 位置：acceptance-workspace.tsx:730-736
- 现象：`updatedAt="刚刚"` 但 `dateTime={view.freshness.factsUpdatedAt}`，渲染出「数据更新于 刚刚」，无具体时间；术语表 §3.5 推荐「数据更新于 {时间}」。
- 建议：传入格式化时间或省略该指标。

**P2-4 键盘快捷键 effect 无依赖数组**
- 位置：acceptance-workspace.tsx:389-405
- 现象：`⌘S / ⌘Enter` 监听每次渲染重新绑定，无清理说明，且快捷键无任何界面提示（易用性：不可发现）。
- 建议：补依赖数组（或 ref 化 handler），并在卡片说明中标注快捷键。

**P2-5 详情页加载态无骨架，仅为一行文字**
- 位置：sales-order-detail-page.tsx:225-231
- 现象：加载瞬间整页几乎空白，缺少结构反馈。
- 建议：参照验收区双卡骨架（acceptance-workspace.tsx:602-613）。

**P2-6 采购驳回「调整后含税单价」在多行明细下语义不清**
- 位置：procurement-rejection-card.tsx:57-63、116-135
- 现象：默认取 `lineItems[0]` 单价，单一单价字段无法表达改的是哪一行/是否全部行。
- 建议：明确标注「对全部明细统一调整」或按行展示。

**P2-7 「先保存改价」成功后无持久反馈**
- 位置：procurement-rejection-card.tsx:425-430
- 现象：保存成功后按钮文字不变，仅靠下方 FormalActionResult（成功提示可被后续滚动带离视区）；用户无法判断当前草稿是否已保存。
- 建议：参考 `DraftSaveIndicator`（feedback.tsx:470-533）加「已保存」态。

**P2-8 验收区内部词脚注**
- 位置：acceptance-workspace.tsx:863-865（来源 sales_order_line / 业务数据）、922-924（可验收量以系统净记录为准）、974-976（销售版本数据）
- 现象：「sales_order_line」为内表名直出；「系统净记录」「销售版本数据」接近实现词。
- 建议：删除或替换为「以系统记录为准 / 当前销售数据」。

**P2-9 「仅待验收/全部历史记录」筛选不随 URL 持久化**
- 位置：acceptance-workspace.tsx:290、311-321
- 现象：刷新后回默认「仅待验收」，与「URL 参数与界面控件一一对应」契约（AGENTS.md §5）不完全一致。
- 建议：筛选写入 `remainingOnly` 查询参数。

**P2-10 卡券审批领取后无释放处理权入口**
- 位置：card-sales-approval-panel.tsx:171-206
- 现象：领取后只有通过/驳回，无法交回；领错只能刷新（刷新即失效，靠会话内存）。
- 建议：增加「放弃本次处理」按钮（等效释放），符合只读角色文案规范。

---

## 六、改进建议（按优先级）

1. **（P0）修冲正交互**：把冲正理由输入并入确认弹窗内（弹窗内做 Textarea + 校验），或先弹输入再弹确认；最低限度在弹窗提示「理由在下方卡片填写」。
2. **（P0）服务不通过入数据**：`AcceptanceDraftLine` 与提交 payload 增加 `serviceFail` 字段，草稿恢复不再硬编码 false；前端展示与落库保持同源。
3. **（P1）错误态分级**：详情页区分「加载失败（重试）」与「单据不存在」。
4. **（P1）补驳回说明**：`completeCardSalesApproval` 增加 `comment` 透传；驳回卡片提示「说明将随驳回送达销售」。
5. **（P1）未保存防护**：验收区引入 dirty 追踪 + `DiscardConfirmDialog`。
6. **（P1）确认弹窗文案合规**：按术语表把 lockVersion/表名/APPLY 替换为业务表述（"数据版本"「生成验收记录」「按本次结果分配履约数量」）。
7. **（P1）自动填充只触发一次**：分配数变化不再覆盖手工修改的通过数量。
8. **（P2）演示控件隔离、双按钮去重、快捷键提示、URL 持久化**等按清单逐项消化。

---

### 附：统计

| 严重度 | 数量 |
| --- | --- |
| P0 | 2 |
| P1 | 5 |
| P2 | 10 |
| 合计 | 17 |
