# UX 评审：/supplier-api/connections（供应商 API 连接）

> 评审日期：2026-08-05
> 评审对象：`erp-client/features/supplier-api-connections/supplier-api-connections-page.tsx`（及同域 types.ts / url-state.ts / api.ts / mock，共享组件 data-table.tsx / page.tsx / feedback.tsx / document.tsx / workflow.tsx 等）
> 角色：产品体验评审员（产品经理 / UX 视角），纯分析，不改代码。

---

## 一、页面概述

供应商 API 连接管理页，核心对象是「连接身份」（连接代码 + 供应商 + 环境）。页面分两个视图：

- **列表视图**（`/supplier-api/connections`）：5 个可点击指标（已启用/故障/待配置/健康异常/目录陈旧）+ 搜索 + 环境/状态筛选 + 9 列表格（身份/环境/状态/能力摘要/健康/目录同步时间/下一步/负责人/操作），支持列固定（左身份、右操作）、列设置、行级 Enter 打开；空态区分无权限/无数据范围/无数据/筛选无结果。
- **对象中心视图**（`/supplier-api/connections/:connectionId`）：文档头（状态轨 + 健康检查/启用/停用主操作）+ 生产环境警示 + 告警区 + 7 个 Tab（概览/能力/安全配置引用/健康与尝试/目录同步/关联业务/审计），停用有影响预览弹窗，密钥采用不透明引用 + 角色分层可见。

整体架构成熟：角色演示条（采购/研发运维/系统管理员 × 权限 flag）、URL 状态恢复（role/section/connectionId）、结果反馈统一走 `FormalActionResult` + 任务号、防误操作设计（生产环境红色警示、停用二次确认、结果未知不乐观处理）都做得很到位，属于本仓库里完成度较高的工作面。问题集中在**移动端入口缺失、内部标识符上屏、分页交互失效**三处。

---

## 二、易用性

**顺畅的地方：**

- 查看列表 → 打开详情 → 返回列表路径清晰，路径式深链 `/supplier-api/connections/:id` 与查询参数双轨（supplier-api-connections-page.tsx:156-166），可从详情 URL 直接分享/恢复。
- 新建连接 → 成功自动跳转详情继续配置（supplier-api-connections-page.tsx:459-463），形成「新建 → 待配置 → 绑定引用 → 配置能力 → 健康检查」的引导链（nextStep 文案同步驱动）。
- 指标条即筛选（点「故障」即筛故障连接，再点取消），状态筛选与指标状态一致（supplier-api-connections-page.tsx:575-640）。
- 行聚焦后 Enter 打开详情（data-table.tsx:808-818），键盘可达。

**不顺畅的地方：**

1. **移动端（<640px）完全没有进入详情的入口**（P0）：操作列在窄屏被隐藏（data-table.tsx:698、836 `max-sm:hidden`），行点击只触发 `onRowPreview` 而本页未传（data-table.tsx:795-798），`onRowOpen` 仅绑定在键盘 Enter 上（data-table.tsx:808-818）。页面其他部分对移动端有专门处理（如 PageActions `mobileVisibility`），但详情入口对触屏用户完全消失。
2. 桌面端行点击无反应，只能点列尾「打开」按钮，连接代码也不是链接（supplier-api-connections-page.tsx:292-304、418-427），与表格行交互习惯不符。
3. 「每页条数」选择器是**假控件**（P1）：查询固定 `pageSize: 20`（supplier-api-connections-page.tsx:267），URL 不携带 pageSize（url-state.ts:10-24），但分页组件照常渲染 20/50/100 选择（data-table.tsx:1076-1091）。选择 50/100 后本地 `pagination.pageSize` 变化 → 页码按 50 计算而数据仍是每页 20 条 → 「第 x/y 页」错误，且 `getCanNextPage()` 按新 pageSize 判定后翻页按钮可能直接失效，用户无法再翻页。
4. 新建成功反馈在跳转详情时丢失（P2）：先 `setResult` 再 `onOpen`（supplier-api-connections-page.tsx:457-463），ConnectionList 随即卸载，「连接身份已创建」结果条从未展示。
5. 列表无手动刷新入口，DataFreshness 硬编码「刚刚」（supplier-api-connections-page.tsx:510-516），长时间停留页面会显示过期数据而不自知（P2）。
6. 默认环境筛选为「生产」（url-state.ts:35），首次进入只看到生产连接，页面无任何「默认只看生产」的提示（P2）。

---

## 三、信息密度

**突出得当：**

- 状态、健康、目录新鲜度均为徽章 + 语义色，列表扫读性强；「连接级 ≠ 商品级」的口径在列表（supplier-api-connections-page.tsx:356-358）与详情（supplier-api-connections-page.tsx:1571-1609、1735-1741）反复澄清，防误解做得好。
- 「下一步」列把引导文案放进列表，业务价值高。

**偏密 / 偏弱：**

1. 9 列同时展示，健康列徽章+时间双行、能力摘要列两行（supplier-api-connections-page.tsx:346-399），默认密度偏高；好在列设置/固定支持自助裁切（data-table.tsx:887-1015）。
2. **供应商名称是页面第一业务维度，却被压成连接代码下的灰色小字**（supplier-api-connections-page.tsx:299-301），连代码本身不是链接、不可排序，识别效率低（P2）。
3. 「业务/技术负责人」列以小字挤占两列宽度（supplier-api-connections-page.tsx:401-412），与「下一步」相比价值低，建议默认收起（P2）。

---

## 四、交互合理性

**做得好的：**

- 加载（骨架）、失败（BusinessFailureState + 重试，supplier-api-connections-page.tsx:477-493）、空态（4 类 EmptyReason 分别处理）齐全。
- 停用：生产环境标题强化（「停用生产环境连接」）+ 影响预览（生效发布/待处理订单/同步任务）+ 明确「不删除、保留历史」文案（supplier-api-connections-page.tsx:1307-1385），误操作防护到位。
- 密钥只做不透明引用选择，页面/结果不返回正文，安全文案一致（supplier-api-connections-page.tsx:1387-1452）。
- 结果未知（UNKNOWN）时全页面「不乐观处理」的告警与任务号引导（supplier-api-connections-page.tsx:1176-1184）符合业务风险偏好。

**问题：**

1. **排序是虚假交互**（P2）：DataTable 默认 `manualSorting: true`（data-table.tsx:270）且未接 `onSortingChange`，本页查询也没有排序参数，点击列头只切箭头图标，数据不动（supplier-api-connections-page.tsx:285-431）。
2. **健康检查的进度条永不推进**（P2）：结果块 `BackgroundJobProgress` 的 total/completed/succeeded/failed 是硬编码（supplier-api-connections-page.tsx:1201-1223），`succeeded` 恒为 0（健康检查 outcome 恒为 processing），进度永远停在「1/4 执行中」；mock 中任务实际立即成功（api.ts:986-998），但没有任何任务轮询去修正。会误导用户以为任务卡死。
3. **停用弹窗的「可处理 1 / 将跳过 0」语义错位**（P2）：`BatchImpactPreview` 是批量组件，这里「预计处理」= 发布+订单+同步任务之和，「可处理 1」指连接本身（supplier-api-connections-page.tsx:1318-1333），两个数字并列不可比，用户难以解读。
4. **「新建连接」禁用无原因**（P2）：非 admin 角色下按钮直接 disabled（supplier-api-connections-page.tsx:525），未用共享的 `GuardedBusinessAction`（feedback.tsx:88-148）展示原因，用户只能靠角色条 hint 猜。
5. **capability / supplierId 是隐形 URL 状态**（P1）：url-state.ts:14-15 定义了 `capability`、`supplierId` 且被查询消费（supplier-api-connections-page.tsx:263-265），但列表工具栏没有对应控件，「清除筛选」（supplier-api-connections-page.tsx:710-727）也不重置它们。一旦这两个参数被带进 URL，用户既看不到也清不掉，违反 AGENTS.md「URL 参数与界面控件一一对应」契约。
6. **空态文案是开发向口吻**（P2）：「不展示导航内快捷创建」「不显示 0 连接」（supplier-api-connections-page.tsx:565、571）——像给测试看的行为说明，不是用户文案。

---

## 五、问题清单（按严重度）

### P0（阻断操作）· 1 个

| # | 问题 | 位置 |
| --- | --- | --- |
| P0-1 | 移动端（<640px）无任何进入连接详情的入口：操作列窄屏隐藏、行点击无 `onRowPreview`、`onRowOpen` 仅绑定键盘 Enter，触屏用户完全无法打开详情 | supplier-api-connections-page.tsx:419-427；data-table.tsx:698/795-819/836 |

### P1（明显阻碍效率 / 契约违规）· 5 个

| # | 问题 | 位置 |
| --- | --- | --- |
| P1-1 | 「每页条数」选择器与固定 `pageSize: 20` 不一致：选 50/100 后页码按新 pageSize 计算而数据仍每页 20 条，「第 x/y 页」错误且可能无法翻页 | supplier-api-connections-page.tsx:267/273-276；url-state.ts（无 pageSize）；data-table.tsx:1076-1091 |
| P1-2 | 审计 Tab 直接渲染原始动作码：`CREATE_CONNECTION` / `BIND_CREDENTIAL_REFERENCE` / `CONFIRM_CAPABILITY_REQUIREMENT` / `UPDATE_CAPABILITIES` / `DISABLE` / `HEALTH_AUTH_FAILED` 上屏，违反术语表规则 7（不把命令名/字段名当文案） | supplier-api-connections-page.tsx:2117；api.ts:598/692/797/890/1199；mock/supplier-api-connections.ts:212/220/305/359/566 |
| P1-3 | 内部 ID 直接上屏：文档头单号显示 `conn_jd_prod`（documentNumber）、「未找到连接」文案拼接 `conn_*`、「结果编号」显示 `conn_new_*` / `kms_ref_*` / `op_*` / `ccr_*`，违反「禁止把内部 ID 展示给用户」 | supplier-api-connections-page.tsx:979/1029；api.ts:566/610/703/790/901；document.tsx:130 |
| P1-4 | 事实表泄露字段名：`{ label: "operationId", value: input.operationId }` 作为用户可见 label | api.ts:820 |
| P1-5 | `capability` / `supplierId` 为隐形 URL 状态：有查询消费、无界面控件、清除筛选不重置 | url-state.ts:14-15/41-42；supplier-api-connections-page.tsx:263-265/710-727 |

### P2（体验瑕疵）· 12 个

| # | 问题 | 位置 |
| --- | --- | --- |
| P2-1 | 桌面端行点击无行为，连接代码非链接，唯一入口是「打开」按钮 | supplier-api-connections-page.tsx:292-304/418-427 |
| P2-2 | 新建成功反馈被跳转吞掉，结果条从未展示 | supplier-api-connections-page.tsx:457-463 |
| P2-3 | 无手动刷新；DataFreshness 硬编码「刚刚」 | supplier-api-connections-page.tsx:510-516 |
| P2-4 | 默认环境筛选为「生产」且无提示，首访只见生产连接 | url-state.ts:35；supplier-api-connections-page.tsx:445 |
| P2-5 | 9 列默认密度偏高，负责人列价值低；供应商名称被压成灰色小字 | supplier-api-connections-page.tsx:299-301/401-412 |
| P2-6 | 列头排序为虚假交互（有箭头、无排序） | data-table.tsx:270；supplier-api-connections-page.tsx:285-431 |
| P2-7 | 健康检查进度条硬编码 1/4 永不推进，`succeeded` 恒 0，无任务轮询 | supplier-api-connections-page.tsx:1201-1223 |
| P2-8 | 停用弹窗「预计处理」与「可处理 1」口径不一致，数字不可比 | supplier-api-connections-page.tsx:1318-1333；workflow.tsx:618-651 |
| P2-9 | 「新建连接」禁用无原因说明（未用 GuardedBusinessAction） | supplier-api-connections-page.tsx:525 |
| P2-10 | 空态文案开发向：「不展示导航内快捷创建」「不显示 0 连接」 | supplier-api-connections-page.tsx:565/571 |
| P2-11 | 实现词残留：「系统管理员独立命令」「唯一键组成部分」「结果以任务号固定」 | supplier-api-connections-page.tsx:809/1968/2174；api.ts:554 |
| P2-12 | 采购角色仍可见 `tr_*` 追踪号（traceId 未按角色剥离） | api.ts:301-308；supplier-api-connections-page.tsx:1920-1928 |

---

## 六、改进建议

**P0-1（移动端入口）**

1. 列表行在移动端默认整行可点：为 `DataTable` 传 `onRowPreview` 或在窄屏保留操作列（改为图标按钮）；同时给「连接代码」单元格加链接语义，桌面端行点击与按钮并存。

**P1-1（分页）**

2. 二选一：a) URL 增加 `pageSize` 参数并与查询联动（推荐，与 AGENTS.md URL 契约一致）；b) 隐藏每页条数选择器（`pageSizeOptions` 传固定值），避免假控件。

**P1-2 / P1-3 / P1-4（标识符与命令名上屏）**

3. 审计动作建中文映射（如 `AUDIT_ACTION_LABEL`），参考 types.ts 现有 `STATUS_LABEL` 模式。
4. `documentNumber` 改用连接代码（`CONN-JD-PROD`）或删除单号位；「结果编号」对 create/disable 等成功路径返回业务号（如审计号 `AUD-W20-*`）而非 `conn_*`/`op_*`/`ccr_*`；「未找到连接」不拼接内部 ID。
5. 事实表删除 `operationId` 行，改用「审计号」等业务标识。

**P1-5（隐形状态）**

6. 补能力筛选控件（可挂到工具栏），或从 url-state/查询中摘除 `capability`、`supplierId`。

**P2 快速项**

7. 健康检查进度：结果块改为「已创建后台任务 xx · 可随时在健康 Tab 查看固定结果」，去掉硬编码进度数字，或加任务轮询。
8. 停用弹窗去掉 `BatchImpactPreview` 的「可处理/将跳过」两栏，只保留三项影响计数。
9. 排序要么接 URL 参数实现服务端排序，要么 `enableSorting: false`。
10. 空态与弹窗措辞过一遍术语表：删除「唯一键」「独立命令」等实现词。
11. 新建成功提示：跳转详情前可先关闭弹窗并展示结果条，或把成功结果透传给详情页展示一次。

---

## 附：评审范围

- 主文件：`features/supplier-api-connections/supplier-api-connections-page.tsx`（2303 行，全文通读）
- 同域：`types.ts`、`url-state.ts`、`queries.ts`、`api.ts`、`mock/supplier-api-connections.ts`
- 共享组件：`components/business/data-table.tsx`、`page.tsx`、`feedback.tsx`、`list.tsx`、`document.tsx`、`workflow.tsx`（BatchImpactPreview）、`role-demo-bar.tsx`、`values.tsx`、`entity-comboboxes.tsx`
- 依据：`docs/ui-glossary.md`（术语表 v1.2）、`erp-client/AGENTS.md`（URL 契约、文案守则）
