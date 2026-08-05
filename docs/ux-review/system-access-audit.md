# UX 评审：/system/access-audit（访问审计 / 权限与审计）

> 评审对象：`erp-client/features/access-audit/access-audit-page.tsx`（2149 行）及其依赖
> `types.ts` / `queries.ts` / `session.ts`、`components/business/{data-table,list,feedback,page,option-combobox,workflow}.tsx`
> 评审日期：2026-08-05 ｜ 口径：`docs/ui-glossary.md`（术语表 v1.2）

---

## 1. 页面概述

W19「权限与审计」，URL `/system/access-audit`，路由标题「权限与审计」（`lib/workspace-registry.ts:279`）。
单页 5 视图（Tab）：角色权限 / 用户授权 / 数据范围 / 字段策略 / 审计查询（`types.ts:341-347`）。

- 数据层：TanStack Query + session mock（`queries.ts`），列表/详情/解释/预览/提交均为客户端查询。
- 核心能力：① 审计事件查询（按操作者/动作/结果/对象/追踪号）；② 有效权限解释 Sheet；
  ③ 授权变更影响预览 + 确认提交（紧急撤权/停用/扩权/字段策略调整）；④ 导出（禁用态 stub）。
- 亮点：空态按 5 种原因分型（`EmptyByReason`）、键盘 `/` 聚焦搜索、关闭 Sheet 后行焦点恢复、
  敏感字段只展示「字段名 + 已变更」不打码原则、追加式审计只读说明清楚。

总体评价：结构与状态管理是成熟工程水准，但**审计查询路径、导出动作与文案合规是主要短板**。

---

## 2. 易用性

### 2.1 查询审计记录（按用户/时间/动作）

- **按动作**：动作筛选下拉齐全（`access-audit-page.tsx:1410-1429`），但只覆盖 5 类动作；seed 中
  `OPEN_SUPPLIER`、`EXPORT_RECEIVABLE`、`CREATE_ADJUSTMENT`、`VIEW_CUSTOMER_SENSITIVE`、
  `PERMISSION_VERSION_BUMP` 等事件无法按动作筛选（`session.ts:555-676`）。
- **按用户**：主搜索可命中操作者姓名（`session.ts:807`），另有「操作者 ID」输入框
  （`access-audit-page.tsx:1391-1402`）——该输入要求用户知道 `user_chenlei` 这类内部 ID，业务用户无从下手。
- **按时间（核心缺口）**：查询契约支持 `from`/`to`（`types.ts:26-27`，消费于 `access-audit-page.tsx:403-404`），
  但**页面上没有任何时间范围控件**（无日期选择器、无快捷区间），该参数只能手改 URL 产生，且无法主动清除——
  是「被 queryFn 消费、却没有控件」的隐形状态（违反 AGENTS.md 界面契约）。审计查询的第一维度「按时间」实际不可用。
- **理解记录的路径**：行内「详情」打开右侧 Sheet，结构清晰（谁/何时/做了什么/结果/变更字段/追踪号）；
  但与列表联动时会把列表整体按 `eventId` 重新过滤（见 4.3），背后列表会重排闪烁。

### 2.2 授权操作路径

- 角色行：有效权限 / 调整权限 / 扩权（将阻断）/ 停用 —— 动作按钮与风险标记对应关系直观，「扩权（将阻断）」
  预先告知结局是好设计。但「调整权限」硬编码变更项 `W22.publish` REMOVE（`access-audit-page.tsx:759-765`），
  演示痕迹重，用户无法选择要改什么。
- 用户行：紧急撤权 → 影响预览 → 原因/说明 → 确认，链路完整且带幂等防重。
- 提交按钮与动作一致（撤权走 destructive 样式，`access-audit-page.tsx:2109-2114`）。

---

## 3. 信息密度

### 3.1 审计列表（9 列）

时间 / 操作者 / 责任角色 / 动作 / 对象 / 结果 / 变更字段 / 请求追踪号 / 查看
（`access-audit-page.tsx:1072-1166`）。

- **过密**：9 列中「责任角色」「请求追踪号」为低频列，且每行还带两个 mono 小字 ID（操作者 ID、对象类型/ID）。
  三列以上视觉噪音（mono 小字 + 中文混排）叠加，核心信息「谁在何时做了什么」反而被稀释。
- 「变更字段」列逐字段拼接「字段名 · 已变更」（`access-audit-page.tsx:1131-1136`）——字段名是英文原值
  （`modulePermissions`、`scopeTargets`…），用户读不懂，属无效信息，且 `changedFieldDisplay`
  （`types.ts:171`）已由服务端拼好中文意图，页面却重写一遍拼接。
- 列固定策略正确（时间/操作者左钉、查看右钉，`access-audit-page.tsx:1677-1680`），窄屏可用性好。

### 3.2 角色/用户列表

「有效期间」单元格内嵌两行长说明（`access-audit-page.tsx:861-863`），单格信息过载；
「版本」列展示 `pv-w19-12` 原始版本串（`access-audit-page.tsx:706-708`），对业务用户是噪声。
风险徽章、状态徽章密度控制良好。

### 3.3 有效权限解释 Sheet

分节（模块权限/数据范围/字段策略/历史参与者/拒绝阻塞）结构清晰，但每条还带 mono code（如 `OBJECT_STATE`、
`EMPTY_DATA_SCOPE`、`ROLE_REVOKED`，`access-audit-page.tsx:1816-1818`）——审计员需要可读的「结果 + 原因」，
原值 code 是给开发看的。

---

## 4. 交互合理性

### 4.1 加载 / 空 / 错误状态

- 加载：整页骨架屏，得体。
- 空态：5 种原因分型 + 差异化文案（`access-audit-page.tsx:208-265`），「无模块权限/无数据范围/范围内无记录/
  字段打码/筛选无结果」区分清晰，且「筛选无结果」带「清除筛选」动作——本页最佳实践。
  （小瑕疵：「范围内无记录」文案在审计视图下仍提「角色/范围」，`access-audit-page.tsx:237`。）
- 错误：`BusinessFailureState` + 重试按钮，标准。
- **刷新反馈缺失**：变更提交后 `invalidateQueries`（`queries.ts:143-147`）触发 refetch，页面静默替换旧数据，
  `DataTable` 的 `loading` 指示（`data-table.tsx:655-663`）未接线，用户感知不到刷新中状态。

### 4.2 筛选器与 URL 参数

- 参数双向同步（`patch-search-params.ts`）设计正确；Tab 切换保留 `q`、清理审计专属筛选（`access-audit-page.tsx:1265-1282`）合理。
- **问题 A（P1）**：`from`/`to` 无控件、无清除入口（除「清除筛选」外），隐形状态。
- **问题 B（P1）**：`status/risk/action/result` 下拉变更只 patchUrl、**不重置分页**（如 `access-audit-page.tsx:1350-1355`；
  对比 `q` 有重置，`:386-387`）。用户在第二页后改筛选 → `rows` 变少 → 当前页切片为空 →
  「共 10 条」与表内「当前筛选没有结果」（`data-table.tsx:858-868`）并存，互相矛盾。
- **问题 C（P2）**：「操作者 ID」「高级筛选」三个输入逐键 patchUrl 发请求（无防抖），且要求内部 ID 输入。

### 4.3 分页与详情联动

- 打开「详情」时 `eventId` 进入 listQuery（`access-audit-page.tsx:410`），列表被过滤成单行重建；
  关闭后再全量 refetch。Sheet 背后列表闪变，且浪费一次列表请求（`access-audit-page.tsx:481-495`）。
- 表头排序控件存在但只对「当前页切片」排序（`data-table.tsx:719-736` + 页面未设 `enableSorting:false`），
  用户点击「时间」表头以为全局排序，实际只在 20 条页内排——误导性交互。

### 4.4 导出按钮（P1）

`access-audit-page.tsx:1229-1250`：策略已配置、按钮启用时，点击永远落入
「导出暂不可用 / 当前账号尚未配置导出权限，无法生成导出文件」——启用态与失败文案自相矛盾，
用户无法判断是自己没权限还是功能没做。要么接真导出，要么保持禁用并解释条件。

### 4.5 表单校验被绕过（P1）

影响预览 Dialog 的 form（`access-audit-page.tsx:2039-2045`）`onSubmit` 同时 `void form.handleSubmit()` 与
`void confirmChange()`——校验异步、提交同步，说明超 200 字等校验失败时 `confirmChange` 依然执行。
（当前 reasonCode 有默认值所以平时不暴露，但属于错误提交路径。）

### 4.6 演示控件

「演示状态」下拉与 3 个策略按钮直接挂在正式工具栏 actions（`access-audit-page.tsx:1504-1567`）。
术语表 G6 决议保留演示标记，但该控件在工具栏与「导出」并列，任何访客可见可点；建议收敛到演示条或隐藏。

---

## 5. 文案（对照 `docs/ui-glossary.md`）

术语表 v1.2 明确「字段名/枚举原值上屏必须清零」「内部 ID 不得进界面」「Q1–Q5 代号不出现」，
本页是重灾区，集中在四个面上：

1. **变更字段列/详情**：`modulePermissions`、`activeRoles`、`accessCapabilities`、`scopeTargets`、
   `onHandQuantity`、`permissionVersion` 等字段名原值（`session.ts:537-658`）直出到列表列与详情
   （`access-audit-page.tsx:1131-1136`、`1932-1936`）。术语表规则 7「不把字段名当文案」违反。
2. **对象列/详情**：`objectType/objectId` 原值直出（`CUSTOMER/cust_10086`、`ROLE/role_sales`，
   `access-audit-page.tsx:1111-1113`、`1913-1915`）；详情动作旁附 `actionType` 原值（`:1895-1897`）。
3. **风险/策略码**：影响预览 `riskFlags.join(", ")` 直出（`IMMEDIATE, SESSION_INVALIDATION`，`:2009-2011`）；
   治理策略 Banner 直出 `blockerCode`（`USER_ROLE_TIME_POLICY_MISSING` 等，`:153`、`:170`、`:186`）与
   `REVIEW_POLICY_UNCONFIGURED`（`:201`）；有效权限 Sheet 直出 `e.code`（`:1818`）。
4. **内部引用/版本/水位**：差异面板字段直出 `W22.publish`、`sensitive.field.expand`（`:759-765`、`:786-790`，
   渲染于 `session.ts:1408-1410`）；「可提交 policyTargetId」徽章（`:1031`）；期望权限版本 `pv-w19-12`（`:2092-2095`）；
   表格说明「更新于 w19-audit-12」水位原值（`:1603`）。

其他措辞问题：

- 「当前动作 blocker」（`:1830`）——英文术语标题。
- 「本命令不携带任务标识」（`:2096`）——机制语言。
- 「最大在线 168 小时」（`:193`）——应为「最长可查窗口」；「治理策略门闩」（`:144`）——「门闩」生造词。
- 「首屏固定身份与操作列」（`:1604`）——把固定列的实现写进说明文字。

做得好的：动作枚举全部映射中文（`actionLabel`、`resultLabel`、`riskLabel` 映射表 `:102-112`）、
「掩码」已统一为「打码」（`:1581-1587`）、空态与反馈文案业务化、敏感值不展示。

---

## 6. 问题清单（按严重度）

### P0（阻断操作）— 0 个

无完全不可操作的硬阻断（导出一事见 P1-2）。

### P1（明显阻碍效率 / 核心功能缺失）— 7 个

| # | 问题 | 位置 |
| --- | --- | --- |
| P1-1 | 审计「按时间查询」不可用：`from`/`to` 被查询消费但页面无任何控件，只能手改 URL，且无独立清除入口（隐形 URL 状态） | `access-audit-page.tsx:403-404`、`types.ts:26-27` |
| P1-2 | 导出按钮启用即必败：策略已配置、按钮可用时点击恒得「导出暂不可用 / 尚未配置导出权限」，启用态与文案自相矛盾 | `access-audit-page.tsx:1236-1249` |
| P1-3 | 筛选控件（状态/风险/动作/结果）变更不重置分页；第二页起筛选变窄后出现「共 N 条」与「当前筛选没有结果」并存 | `access-audit-page.tsx:1350-1355`、`1370-1387`、`1407-1455`（对比 `q` 有重置 `:386-387`）、`data-table.tsx:858-868` |
| P1-4 | 内部字段名原值上屏：变更字段列/详情直出 `modulePermissions`、`scopeTargets` 等（术语表规则 7 违反） | `access-audit-page.tsx:1131-1136`、`1932-1936`；数据 `session.ts:537-658` |
| P1-5 | 枚举原值 + 内部 ID 上屏：对象列 `objectType/objectId`（`CUSTOMER/cust_10086`）、详情 `actionType`、Sheet 主体 ID | `access-audit-page.tsx:1111-1113`、`1895-1897`、`1913-1915`、`1734-1736` |
| P1-6 | 风险/策略码直出：影响预览 `riskFlags` 原值、Banner `blockerCode`/`REVIEW_POLICY_UNCONFIGURED`、差异面板 `W22.publish`/`sensitive.field.expand`、「可提交 policyTargetId」 | `access-audit-page.tsx:2009-2011`、`153`、`170`、`186`、`201`、`759-765`、`786-790`、`1031` |
| P1-7 | 表单校验被绕过：onSubmit 并行触发 `form.handleSubmit()` 与 `confirmChange()`，校验失败仍可提交 | `access-audit-page.tsx:2039-2045` |

### P2（体验瑕疵）— 12 个

| # | 问题 | 位置 |
| --- | --- | --- |
| P2-1 | 「更新于 w19-audit-12」水位原值进说明文字；应为「数据更新于 时间」 | `access-audit-page.tsx:1603` |
| P2-2 | 「操作者 ID」/「高级筛选」输入逐键发请求（无防抖），且要求输入内部 ID | `access-audit-page.tsx:1391-1402`、`1461-1496` |
| P2-3 | 动作筛选选项不全：seed 中 5 类动作无法按动作筛选 | `access-audit-page.tsx:1410-1429` vs `session.ts:555-676` |
| P2-4 | 打开详情/有效权限时列表被 `eventId`/`subjectId` 过滤重建，背后列表闪变 + 多余 refetch | `access-audit-page.tsx:383-411`、`481-495` |
| P2-5 | 表头排序控件只排「当前页切片」，点击后静默无全局效果，误导用户 | `data-table.tsx:719-736`（页面列未设 `enableSorting:false`） |
| P2-6 | 审计列表 9 列密度高；「变更字段」列服务端已备 `changedFieldDisplay` 却未用，前端重拼英文名 | `access-audit-page.tsx:1127-1139`、`types.ts:170-171` |
| P2-7 | 列表 refetch 无刷新指示（DataTable `loading` 未接线），旧数据被静默替换 | `access-audit-page.tsx:1668-1681`、`queries.ts:143-147` |
| P2-8 | 「当前动作 blocker」英文术语标题；「本命令不携带任务标识」机制语言 | `access-audit-page.tsx:1830`、`2092-2096` |
| P2-9 | 「最大在线 168 小时」措辞含混；「治理策略门闩」生造词 | `access-audit-page.tsx:193`、`144` |
| P2-10 | 「演示状态」演示控件挂在正式工具栏，任意访客可见可点（G6 保留但建议收敛） | `access-audit-page.tsx:1504-1567` |
| P2-11 | 空态「范围内无记录」在审计视图下仍提「角色/范围」，语境错位 | `access-audit-page.tsx:237` |
| P2-12 | 表格说明「共 N 条 · 首屏固定身份与操作列」把固定列实现写进用户文案 | `access-audit-page.tsx:1604` |

---

## 7. 改进建议（按优先级）

1. **补时间筛选控件（P1-1，最高优先）**：审计视图工具栏加「起始/截止」日期区间（含快捷项：今天/近 7 天/近 30 天），
   与 `from`/`to` 双向绑定，并纳入「清除筛选」；清除后回退到策略窗口并提示「当前可查范围」。
2. **修导出按钮语义（P1-2）**：策略配置前保持禁用 + tooltip 解释；配置后接真实导出；在 mock 阶段至少把
   点击结果改为「导出功能待接入」，不与「无权限」混淆。
3. **筛选变更重置分页（P1-3）**：所有 patchUrl 类筛选统一先 `setPagination({pageIndex:0,...})`（可仿照 `q` 的写法）。
4. **文案合规整改（P1-4/5/6、P2-1/8/9）**：建 `FIELD_NAME_LABEL`/`RISK_LABEL`/`CODE_LABEL` 中文映射表，
   覆盖：变更字段名→中文（模块权限/数据范围/访问能力/库存数量…）、对象类型→中文（客户/角色/供应商…）、
   对象 ID 尽量隐藏或换业务名（审计场景可保留「审计事件号 ACC-…」这类正式编号）、`riskFlags` 走
   `riskLabel` 同款映射、Banner 的策略码换成业务句；`watermark` 换成 `formatDateTime(calculatedAt)`。
5. **固定页内排序误导（P2-5）**：对只展示切片数据的表格关闭排序（列上 `enableSorting:false`），或接全量服务端排序。
6. **详情/列表解耦（P2-4）**：`eventId`/`subjectId` 只驱动详情请求，不进入 listQuery；或列表请求剔除这两个参数。
7. **校验与提交串行化（P1-7）**：`onSubmit` 内 `await form.handleSubmit()` 成功后再 `confirmChange()`。
8. **低优先**：动作筛选补全 seed 动作；「操作者」改为人员下拉/姓名联想；审计列瘦身（责任角色/追踪号收进详情）；
   演示控件收敛；refetch 时透传 `loading` 到 DataTable。
