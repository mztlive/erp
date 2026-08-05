# UX 评审报告：商城消费订单详情 / 处理中心（/commerce/consumption-orders/[mallOrderId]）

- 评审视角：产品经理 / UX
- 涉及代码：
  - `erp-client/features/mall-consumption-orders/consumption-order-center-page.tsx`（详情中心页）
  - `erp-client/features/mall-consumption-orders/consumption-order-preview-panel.tsx`（列表预览面板）
  - `erp-client/features/mall-consumption-orders/types.ts` / `queries.ts` / `api.ts` / mock 种子
  - `erp-client/components/business/{document,values,domain,feedback}.tsx`（共享组件）
  - `erp-client/app/(workspace)/commerce/consumption-orders/[mallOrderId]/page.tsx`
- 参考基线：`docs/ui-glossary.md`（v1.2，P0/P1/P2 禁用词表）
- 结论：页面是**只读追溯视图**，整体结构清晰、空/错/加载三态齐全、异常引导链路（W29/W26 跳转）设计完整；主要问题集中在**权限打码契约被绕过**、**成本口径数据前端写死**与**枚举原值/内部词上屏**三类。

---

## 1. 页面概述

- 定位：W25 商城消费订单的「对象中心」详情页，展示一单消费订单从支付、归集、履约、成本到售后的全链路**只读**追溯；明示"不提供修改商城订单、补支付记录、编辑分摊或旁路重试供应商动作"（`consumption-order-center-page.tsx:556-564`）。
- 信息架构：页头（面包屑 + 返回列表/刷新）→ 单据头（订单号/主状态）→ 状态条 → 条件 Alert → 9 个 Tab（概览 / 关键记录 / 商品明细 / 支付与分摊 / 来源追溯 / 供应商履约 / 成本口径 / 售后结果 / 审计）。
- 双入口：列表行点击打开预览面板（`ConsumptionOrderPreviewPanel`，半屏摘要）；「打开中心」进入本页完整视图（`consumption-orders-list-page.tsx:536-547`）。导出能力仅在列表页。

## 2. 易用性

**顺畅的部分**
- 异常订单的处理路径是闭环的：`paymentOccurredAlert` 直接给出「打开供应商子订单 / 打开接口错误差异」按钮（`consumption-order-center-page.tsx:508-554`）；供应商缺单时给出「打开接口错误与对账中心」入口（`consumption-order-center-page.tsx:1030-1045`）；审计 tab 也带 W29 入口。异常用户基本不需要去别处找路。
- 权限态、无数据态、错误态三态齐全；无数据态带「返回列表」按钮（`consumption-order-center-page.tsx:364-383`）。
- Tab 切换用 `router.replace` 保持 URL 与控件一致（`consumption-order-center-page.tsx:316-326`），刷新/分享可恢复 `section`/`fact` 状态，符合 AGENTS.md 的 URL 契约。

**不顺的部分**
- 本页**没有任何操作**（只读），但页头仍摆「返回列表 + 刷新」两个次级按钮，且所有跳转都用「打开 X」的机制式措辞——对首次使用者来说，页面是"看"还是"办"的定位要靠大段 Alert 才能读懂；「处理中心」的路由名与"本页只读、处理都在别的页"的实际能力有落差。
- 错误态只有「重试」，没有「返回列表」，用户在本页失败后缺少显式回退路径（见 P2-2）。
- 「返回列表」是裸链接到列表根路径，丢失进入本页前的筛选/分页/指标上下文（见 P2-3）。

## 3. 信息密度

- 概览 tab 是 12 个字段的三列网格（`consumption-order-center-page.tsx:581-668`），结构规整，但**实付金额与其余金额视觉同级**，没有用字号/颜色强调"最终结果"，用户在首屏找"这单到底付了多少钱"要扫读整块网格（P2-13）。
- **同一条状态被渲染两遍**：`DocumentHeader.statuses`（关键记录/归集）与其正下方 `StatusTrackSummary`（关键记录/履约链/归集）重叠展示"归集"与"关键记录"（`consumption-order-center-page.tsx:460-506`），首屏重复信息约一屏高。
- 「关键记录」tab 内同一批事实同时以 FactCard 网格与 AuditTimeline 两套列表呈现（`consumption-order-center-page.tsx:720-756`），信息冗余，页面很长。
- 常驻信息条过多：`paymentOccurredAlert` + 「记录追溯边界」Alert + 履约链告警在异常单上可叠出 3~4 条 Alert 再开始正文（`consumption-order-center-page.tsx:508-564, 669-684`），首屏有效内容被挤压。
- 成本 tab 的 `CostCoverageNotice` 展示"成本覆盖率"进度条与三分列，但数据是前端写死的（见 P1-4），给用户造成"系统权威口径"的错觉。

## 4. 交互合理性

- 加载态为骨架屏（`consumption-order-center-page.tsx:333-341`），良好。
- 「刷新」按钮：`refetch()` 不绑定 `isFetching`，点击后无进行中反馈、可连点（P2-1）。
- 敏感字段：概览按 `fieldPermissions` 打码（`consumption-order-center-page.tsx:604-609`），但**页头标题绕过打码直接渲染 `customerLabel`**（见 P0-1），权限契约失效。
- 「地址仅短暂显示，需授权并记录审计；离开页面后立即清除」的文案（`consumption-order-center-page.tsx:708-709`）描述了一个本页不存在的能力——`revealAllowed`（`types.ts:419`）从未被消费，页面始终只显示掩码摘要（P2-6）。
- 售后 tab 的「供应商退款」卡显示"见履约区供应商退款摘要"却无跳转链接（`consumption-order-center-page.tsx:1315-1318`），需要用户手动切 tab（P2-11）。
- 误操作防护：本页无写操作，无需二次确认；阻断原因在审计 tab 完整列出（`consumption-order-center-page.tsx:1395-1414`），但以 mono 内部码呈现（P1-7）。

## 5. 问题清单（按严重度）

### P0（契约级缺陷 / 信息泄露）

**P0-1 客户打码契约被页头绕过**
- 位置：`consumption-order-center-page.tsx:449`（`title={view.identity.mallName · view.customer.customerLabel}`）vs `:604-609`（`fieldPermissions.customer === "masked"` 时显示 `****（打码）`）
- 问题：本页明示"敏感字段（按权限打码）"，但页头标题直接渲染完整客户名。无权限用户仍能从标题拿到客户名称，打码形同虚设；同一页两种口径自相矛盾。预览面板（`consumption-order-preview-panel.tsx:118-122`）正确做了判断，说明这是中心页漏处理而非产品设计。

### P1（明显阻碍效率 / 契约违反）

**P1-1 成本口径枚举原值上屏（ACTUAL / STANDARD / NONE）**
- 位置：`types.ts:86-90`（`COST_BASIS_LABEL` 原样返回枚举值）；渲染于 `consumption-order-center-page.tsx:1216-1217`、`consumption-order-preview-panel.tsx:348-349, 373-374`；共享组件 `domain.tsx:925-940` 也以 "ACTUAL · 实际成本" 形式上屏
- 问题：违反术语表 §1.3 规则 7「枚举原值禁止直接渲染」；成本口径是财务高频核对字段，英文原值增加阅读成本。业务用户应看到「实际成本 / 标准成本 / 无可用成本」。

**P1-2 供应商「取消 / 退款」状态原值上屏（NONE / PARTIAL）**
- 位置：`consumption-order-center-page.tsx:1060-1061`（`履约 X · 取消 {so.cancelStatus} · 退款 {so.refundStatus}`）、`consumption-order-preview-panel.tsx:308-309`；mock 数据 `mock/mall-consumption-orders.ts:315-316`（`"NONE"`/`"PARTIAL"`）
- 问题：同一处展示「履约」用了中文映射（`SUPPLIER_STATUS_LABEL`），旁边却裸渲染英文枚举，口径割裂。

**P1-3 成本覆盖率/成本口径占比为前端写死数据**
- 位置：`consumption-order-center-page.tsx:1156-1194`——`coveragePercent` 硬编码 `0/100/70`（`:1158-1164`），`breakdown` 硬编码 `"100%"/"0%"/"未覆盖"`（`:1179-1183`）
- 问题：`MallConsumptionOrderView`（`types.ts:381-440`）没有任何覆盖率字段，页面按 `costBasisPrimary` 分支伪造百分比：有 ACTUAL 就报 100%、有 STANDARD 就报 70%。当订单同时含 ACTUAL 与 STANDARD 条目、或部分条目 NONE 时，展示的"成本覆盖率"与真实数据不符。财务口径类数字不允许占位伪造——要么接口给真实聚合，要么删除百分比只留条目级明细。

**P1-4 空态文案泄露内部词 + 原始内部 ID**
- 位置：`consumption-order-center-page.tsx:370`（`稳定身份 ${mallOrderId} 不存在或无权访问。`）
- 问题：「稳定身份」是内部实现词（术语表禁止），且把 `mallOrderId` 原始 ID 直接展示给用户。应改为「订单 {外部单号} 不存在或无权访问」——注意：订单不存在时外部单号可能同样取不到，至少应把「ERP 稳定 ID」换成业务说法。

**P1-5 「记录追溯边界」文案包含工作面编号 W25**
- 位置：`api.ts:38-39`（`BOUNDARY_NOTICE` 以 "W25 是由不可变关键记录形成…" 开头）、`mock/mall-consumption-orders.ts:353-354`；渲染于 `consumption-order-center-page.tsx:556-563`
- 问题：术语表 §2 明确「Wxx 工作面编号禁止出现在面向业务用户的提示」，且「不可变关键记录」是事实（fact）概念的内部翻译残留。这段文案是本页最高频的用户可见信息（每单必现），应当按业务说法改写（如「本页由商城支付/退款等结果记录汇总而来」）。

**P1-6 审计区动作码/命令名原样上屏**
- 位置：`consumption-order-center-page.tsx:1400-1412`——`b.code` 以 mono 渲染（mock 为 `FACT_TRACE_READONLY`、`USE_W26`，见 `mock/mall-consumption-orders.ts:333-344`），`b.action` 原样渲染（`EDIT_MALL_ORDER` 等）
- 问题：违反术语表 §1.3 规则 7「不把命令名/字段名当文案」。审计 tab 面向排障用户，内部码也许有用，但应改为「动作说明 + 中文阻断原因」双栏，而非把命令码当正文。

### P2（体验瑕疵）

- **P2-1 刷新无进行中反馈、可连点**：`consumption-order-center-page.tsx:434-442` 的刷新按钮不绑定 `isFetching`，无 spinner/禁用，快速连点产生重复请求；刷新成功也无"数据已更新"反馈（`freshnessText` 未用）。
- **P2-2 错误态缺少「返回列表」回退**：`consumption-order-center-page.tsx:343-361` 只有「重试」，与空态的「返回列表」（`:372-378`）不一致。
- **P2-3 「返回列表」丢失列表筛选上下文**：`consumption-order-center-page.tsx:425-433` 直接 `Link` 到 `/commerce/consumption-orders`，进入前的指标/筛选/分页状态全部丢失。
- **P2-4 状态双轨重复展示**：`DocumentHeader.statuses`（关键记录/归集，`:460-478`）与紧随的 `StatusTrackSummary`（关键记录/履约链/归集，`:479-506`）信息重叠，建议合并为一处。
- **P2-5 关键记录双重列表**：`consumption-order-center-page.tsx:720-729`（FactCard 网格）+ `:730-756`（AuditTimeline）同一批事实渲染两遍；建议卡片承载细节、时间线承载时序，二选一或合并。
- **P2-6 敏感字段揭示文案与实现不符**：`consumption-order-center-page.tsx:708-709` 声称"地址仅短暂显示、需授权"但 `revealAllowed`（`types.ts:419`）从未被消费，本页无任何揭示能力；文案应改与实现一致或补揭示交互。
- **P2-7 两视图时间格式不一致**：预览面板用 `monthDayIntl`（`consumption-order-preview-panel.tsx:128,134,176`），详情页用 `default`（`consumption-order-center-page.tsx:614,620`），同一数据两种日期格式。
- **P2-8 预览面板把三个敏感字段挤进一行**：`consumption-order-preview-panel.tsx:185-188` 以 `收货地址 x · 手机号 y · 支付引用 z` 单行拼装，可读性差且无字段标签对齐；详情页用 `DocumentSummary` 三列（`:687-707`），两处应一致。
- **P2-9 「卡实例」与「卡券」术语不一致**：`consumption-order-center-page.tsx:75, 885, 1005` 用「卡实例」，`sourceColumnTitle` 的 Badge（`:76-78`）与支付 tab Badge（`:872-875`）用「卡券」；同一域内两种叫法。
- **P2-10 支付矩阵注脚裸露英文枚举**：`consumption-order-center-page.tsx:227`（"支付来源仅 CARD / WECHAT…"），应按术语表写「卡券 / 微信」。
- **P2-11 售后 tab 跨区指引无链接**：`consumption-order-center-page.tsx:1317`「见履约区供应商退款摘要」无跳转，用户需手动切 tab。
- **P2-12 Tabs 缺 TabsContent 关联**：`consumption-order-center-page.tsx:566-577` 只渲染 `TabsList`，内容在外部按 `section` 条件渲染，Radix 的 `aria-controls`/内容区域关联缺失，影响无障碍与键盘用户定位。
- **P2-13 实付金额无视觉强调**：概览 12 字段网格（`consumption-order-center-page.tsx:582-668`）中实付与其他金额视觉同级，关键结果未突出；「守恒：差异」（`:646-651`）也无警示色，与矩阵区 `destructive` 的提示（`:106-112`）不一致。
- **P2-14 详情页无导出/复制能力**：列表页有导出（`consumption-orders-list-page.tsx`），详情页连单号复制都没有；处理人员常需把订单摘出去对账。
- **P2-15 页面标题固定**：`page.tsx:7` `title: "消费订单"` 不含单号/客户，多页签场景无法区分。
- **P2-16 常驻 Alert 堆叠**：异常单首屏最多可叠「记录追溯边界 + 支付已发生 + 履约告警」3~4 条（`consumption-order-center-page.tsx:508-564, 669-684`），建议告警按优先级收敛、追溯边界说明折叠到审计 tab。
- **P2-17 「ERP 稳定 ID」半内部词标签**：`consumption-order-center-page.tsx:594-596` 直接展示 `mallOrderId` 原值；排障场景或可保留，但标签应换成业务说法（如「ERP 订单 ID」），与术语表"内部 ID 不裸露"原则对齐。

## 6. 改进建议（按优先级）

1. **修复打码绕过（P0-1）**：`DocumentHeader` 标题在 `fieldPermissions.customer === "masked"` 时用「客户（已打码）」占位；建议抽一个 `customerTitle(view)` 工具函数，预览面板与中心页共用，避免再次分叉。
2. **消灭枚举/内部词上屏（P1-1/2/4/5/6）**：`COST_BASIS_LABEL` 改为「实际成本/标准成本/无可用成本」；供应商取消/退款状态补中文映射表（`SUPPLIER_CANCEL_LABEL`/`SUPPLIER_REFUND_LABEL`）；`boundaryNotice` 从后端文案层去掉 W25 与内部词；空态不暴露 `mallOrderId`；审计区动作码降级为代码块样式小字、正文给中文说明。
3. **成本口径给真实数据（P1-3）**：接口契约（`MallConsumptionOrderView`）增加成本覆盖率聚合字段，前端删除 0/100/70 写死分支；在数据到位前，`CostCoverageNotice` 应改为按消费条目逐条展示口径（已有 `consumptionEntries[].currentCostAssessment` 可用），而不是百分比占位。
4. **合并重复状态展示（P2-4/5）**：删掉 `DocumentHeader.statuses` 或 `StatusTrackSummary` 之一；关键记录 tab 用卡片网格承载详情、时间线只保留时序，避免同数据两遍。
5. **补齐回退与反馈（P2-1/2/3）**：刷新按钮绑定 `isFetching` 并给成功提示；错误态加「返回列表」；「返回列表」携带来源 URL（列表页已有 `returnTo` 用法可复用）。
6. **一致性收尾（P2-7/8/9/10/15）**：统一日期格式常量、敏感字段用 `DocumentSummary`、统一「卡券」术语、矩阵注脚改中文、`metadata.title` 动态拼接单号。

---

## 附：评审口径说明

- 本页为纯只读页面，无提交类误操作风险，故无 P0 类"阻断操作"问题；P0-1 因属于**显式权限契约被绕过（信息泄露）**而评定为 P0。
- 术语类问题（P1-1/2/4/5/6）全部有 `docs/ui-glossary.md` 明确条目背书，属"验收挂钩"级违约。
