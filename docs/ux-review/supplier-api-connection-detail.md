# UX 评审报告：供应商 API 连接详情页（/supplier-api/connections/[connectionId]）

> 评审角色：产品经理 / UX
> 评审对象：`erp-client/features/supplier-api-connections/supplier-api-connections-page.tsx`（与列表页共用组件，重点 connectionId 详情分支）及 `types.ts`、`api.ts`、`url-state.ts`、`queries.ts`、`mock/supplier-api-connections.ts`、`components/business/document.tsx`、`feedback.tsx`、`workflow.tsx` 相关组件
> 文案基线：`docs/ui-glossary.md`
> 结论：0 P0 / 4 P1 / 12 P2

---

## 1. 页面概述

页面以 `ConnectionCenter`（supplier-api-connections-page.tsx:899）承载连接详情，支持两种 URL 形态：
- 路径深链 `/supplier-api/connections/:connectionId`（动态路由 `app/(workspace)/supplier-api/connections/[connectionId]/page.tsx`，Suspense fallback 骨架屏）
- 列表页点击「打开」后转为 `/supplier-api/connections?connectionId=...` 查询参数形态（page.tsx:195-206）

详情页结构：DocumentHeader（标题/状态/环境/最近健康/单号/版本/负责人）+ 生产环境警告 + 业务告警栈 + 结果反馈区 + 7 个页签（概览/能力/安全配置引用/健康与尝试/目录同步/关联业务/审计）。

亮点（值得保持）：
- 密钥安全模型完整：永不返回正文、只选 KMS 不透明引用、按角色控制别名/版本可见性（api.ts:244-259、page.tsx:1801-1806），术语表合规。
- 角色权限模型清晰：`allowedActions` / `actionBlockers` 驱动按钮显隐（page.tsx:933-936），能力页、目录页有禁用原因回退文案（page.tsx:1749-1754、2041-2043）。
- 加载/错误/空态齐备：骨架屏（page.tsx:942-950）、失败重试（952-967）、未找到（969-983）。
- 页签与 URL 双向同步（`section` 参数，url-state.ts:47-53），深链可直达具体页签。
- 生产环境防护做得足：停用弹窗带影响预览（page.tsx:1308-1385）、轮换提示、结果未知告警（1176-1184）、鉴权失败告警（1165-1174）。

---

## 2. 易用性

### 2.1 进入详情与深链反馈
- 深链（路径或 `?connectionId=`）均有骨架屏反馈，无效 ID 有「未找到连接」空态 + 返回列表按钮（page.tsx:969-983），面包屑可回列表（page.tsx:993-1005）。**结论：深链体验良好。**
- 瑕疵：无效 ID 空态直接把内部 ID 打在文案里（page.tsx:979），且没有「重试/刷新」或跳转列表的次级动作，仅一个返回按钮。

### 2.2 配置、密钥、同步状态信息
- 密钥/地址配置：安全页签双卡片 + 概览「技术就绪」卡都有引用状态（未绑定/已绑定/需轮换），角色可见性分层清楚（page.tsx:1813-1842）。
- 同步状态：目录同步页签有状态、最近成功时间、当前任务号与后台进度（page.tsx:2009-2030）。
- **主要缺口**：地址配置引用（endpoint）只有只读展示，没有「绑定/轮换」入口（见 P1-3），配置闭环不完整。

### 2.3 关键信息是否突出
- 状态、环境、健康在 DocumentHeader 状态轨与告警区都突出；「下一步」贯穿列表/概览/审计，指引连贯。
- 概览能力徽章的 ✓ / ! / ·停 符号无图例（P2-2）。

---

## 3. 信息密度

- 整体密度适中：DocumentHeader 紧凑（compact 密度，page.tsx:1026），概览两卡一横条，无堆砌感。
- 过疏：安全页签「地址配置引用」卡片只有一行状态徽章（page.tsx:1809-1821），信息利用率低且缺操作（P1-3）。
- 过密风险：告警区可同时叠加 生产警告 + 业务 alerts + 鉴权失败 + 结果未知 最多 3-4 条（page.tsx:1137-1184），故障连接（如 CONN-SF-STG）首屏告警占屏过高。
- 健康记录表 7 列（时间/检查类型/结果/耗时/任务号/追踪号/摘要）偏宽，但均有用；**追踪号（traceId）对业务/运维价值低且泄漏内部 ID**（P2-8）。
- 审计与健康记录均无分页（page.tsx:1963、2110），随历史增长页面无限变长（P2-6）。

---

## 4. 交互合理性

### 4.1 加载 / 空 / 错误状态
- 均已覆盖且文案业务化（见 §1 亮点）。唯一缺口：密钥选择弹窗依赖独立的 `useConnectionListQuery`（page.tsx:924-928）提供下拉选项，若该查询失败则弹窗内选项为空且无错误提示（P2 见问题清单 12 补充说明）。

### 4.2 编辑配置 / 密钥显示与重置
- 密钥轮换流程正确：选择不透明引用 → 确认绑定 → 成功才关弹窗（page.tsx:1432-1445），结果回显别名/版本 facts。
- 但**「绑定/轮换引用」弹窗默认预选第一项**（page.tsx:1278-1280），用户不留意会直接轮换到第一个候选，属于低感知高风险默认值（P2-13 补充）。
- 能力配置弹窗（CapConfigDialog）用原生 checkbox 直接改草稿，无确认差异视图，直接提交（page.tsx:2177-2234）——管理员路径可接受，但「提交能力配置」按钮语义不如「确认变更 N 项」明确。

### 4.3 按钮语义与误操作防护
- 停用：影响预览弹窗 + 确认停用（双重防护，好）。
- 启用 / 健康检查：**无任何确认直接执行**，与页面自身承诺「启停、密钥轮换与全能力检查均需二次确认」（page.tsx:1142）矛盾（P1-1）。
- 操作被角色阻断时按钮直接不渲染，blocker 文案只挂在 `title` 上而按钮永不可见（page.tsx:1089、1109）——用户得不到「为什么没有这个按钮」的解释（P2-3）。

---

## 5. 问题清单（按严重度）

### P0（阻断操作）
无。

### P1（明显阻碍效率 / 违背页面承诺）

**P1-1 生产环境「启用连接」「健康检查」无二次确认，违背页面明示承诺**
- page.tsx:1137-1145 生产环境 Alert 承诺「启停、密钥轮换与全能力检查均需二次确认」，且停用有完整确认弹窗（1308-1385），但「启用连接」（1104-1122）与「健康检查」（1083-1102）点击即执行，无任何确认。
- 影响：生产环境高危操作防护不齐，用户会在无预期的情况下切换连接状态；与页面自身文案不一致，降低信任。
- 建议：启用连接复用停用同款影响预览弹窗（展示订单/发布影响），健康检查加轻量确认（或补全能力影响说明）。

**P1-2 后台任务进度条数字硬编码且自相矛盾**
- page.tsx:1211-1220：`total={启用能力数 || 4}`、`succeeded={成功 ? 4 : 0}`、`failed={失败 ? 1 : 0}`、`processing 时 completed={1}`。
- 影响：成功 5 个启用能力的连接，进度条显示「成功 4 / 失败 0」且 4+0≠总数；processing 态显示 1/N 与 mock 任务真实 completed 数（api.ts:992-996 为启用能力数）不符。进度/结果数据失真，误导用户判断任务是否完成。
- 建议：由任务状态真实计数（total/completed/succeeded/failed 从任务数据透传），删除硬编码 4/1/0。

**P1-3 地址配置引用（endpoint）无绑定/轮换入口，待配置连接无法闭环**
- 权限已下发 `BIND_ENDPOINT_REFERENCE`（api.ts:152 ops、198 admin），但安全页签地址卡片只有只读 RefLabel（page.tsx:1809-1821），全页无 endpoint 绑定控件；密钥有（1833-1836）。
- 影响：CONN-MT-PROD 这类 PENDING_CONFIG 连接「下一步」提示「绑定地址/密钥引用 → 配置能力 → 健康检查」（mock/supplier-api-connections.ts:364），但地址绑定步骤在页面上不存在，管理员无法完成配置闭环。
- 建议：地址卡片补充「绑定/轮换地址引用」入口与弹窗（复用密钥弹窗模式）。

**P1-4 内部 ID、命令名与枚举原值上屏，违反术语表**
- 连接内部 ID 作「单号」展示：`documentNumber={conn.connectionId}`（page.tsx:1029）→ 界面出现 `conn_jd_prod`；「未找到连接」文案同样泄漏（page.tsx:979）。
- 审计动作命令名原样渲染：`{e.action}`（page.tsx:2117）→ 界面出现 `BIND_CREDENTIAL_REFERENCE`、`CONFIRM_CAPABILITY_REQUIREMENT`、`HEALTH_AUTH_FAILED` 等。
- 能力枚举码上屏：能力矩阵副行 `{row.original.capabilityCode}`（page.tsx:1648）、能力配置弹窗 `{c.capabilityCode}`（page.tsx:2186）→ `CATALOG`、`PRICE`、`ORDER`。
- 结果事实面板输出字段名与内部操作 ID：`{ label: "operationId", value: input.operationId }`（api.ts:820）→ `op_*` 原值 + 字段名「operationId」直出。
- 违反 ui-glossary.md §1.3 规则 7（不把命令名/字段名当文案）与 §2「枚举原值上屏」。
- 建议：connectionId 用连接代码替代；action 走中文映射（如「绑定密钥引用」「确认能力需求」）；能力码仅保留在 `capabilityLabel`；operationId 改为业务文案或不展示。

### P2（体验瑕疵）

**P2-1 页签名「健康与尝试」文案异常**
- types.ts:351 `SECTION_LABEL.health = "健康与尝试"`。疑似「健康与检查」笔误，或与健康检查语义不符。
- 建议：改为「健康检查」。

**P2-2 概览能力徽章符号无图例**
- page.tsx:1598-1603：`·停`（停用）、`✓`（验证成功）、`!`（验证失败）无任何图例说明，用户需猜符号含义。
- 建议：加小字图例或改用文字徽章。

**P2-3 角色无权限时按钮直接消失且无原因解释**
- 健康检查/启用/停用按钮按 `can()` 隐藏（page.tsx:1083-1132），blocker 文案仅作 `title` 挂在永不渲染的按钮上（1089、1109），页面上无替代说明。
- 对比：能力页（1749-1754）、目录页（2041-2043）有回退文案——页头操作区缺同一机制。
- 建议：页头操作区无权限时展示只读说明（参考能力页回退文案）。

**P2-4 停用弹窗「替代方案可链到…」并无实际链接**
- page.tsx:1355「替代方案可链到供应商商品库 / 供应商订单 / 接口错误中心。」——文中「可链到」但弹窗内没有任何链接，属误导性表述；「不暗示删除」为防御性内部语气。
- 建议：改成真实链接（复用 RelatedSection 的跳转模式）或删掉这句话。

**P2-5 停用影响预览数字单位混用**
- page.tsx:1323-1329：`estimated` = 发布+订单+同步任务之和（如 318），`processable=1`（连接数）→ 卡片呈现「预计处理 318 / 可处理 1 / 将跳过 0」，单位不一致，数字对比费解。
- 建议：改为「将受影响发布/订单/任务 318 项」的单列表达，去掉语义不清的 可处理/跳过。

**P2-6 健康记录与审计列表无分页，长历史不可控**
- page.tsx:1963 `showPagination={false}`；审计直接 `map`（2110-2136）。
- 建议：健康表加分页，审计按时间倒序截断 + 「查看更多」。

**P2-7 新建连接弹窗「正在创建生产环境连接身份」用 destructive 红色**
- page.tsx:872-876：中性提示使用红色文字（`text-destructive`），易被误读为错误。
- 建议：改 muted 色，仅生产警示类才用红色。

**P2-8 健康记录「追踪号」列泄漏内部 traceId**
- page.tsx:1925 渲染 `tr_*` 内部追踪 ID；对业务用户无价值。
- 建议：删除该列或对非运维角色隐藏（healthRecords 已对采购隐藏 latency/errorClass，可同法处理 api.ts:301-308）。

**P2-9 结果反馈「结果编号」泄漏内部引用 ID**
- page.tsx:117 `reference: outcome.reference ?? outcome.auditEventId`，绑定密钥后显示 `kms_ref_*`（api.ts:703）、能力确认显示 `ccr_op_*`（api.ts:790）、停用显示 `conn_*`。
- 建议：改用审计号（AUD-W20-xxxx）或任务号作为结果编号。

**P2-10 能力配置弹窗 checkbox aria-label 语义反向**
- page.tsx:2198：`aria-label={`启用 ${c.capabilityLabel}``，但取消勾选时是停用——读屏描述与实际动作相反。
- 建议：按 checked 状态生成「启用/停用」描述。

**P2-11 权限声明了「编辑业务资料」但无对应 UI**
- api.ts:119 采购角色 allowed 含 `EDIT_BUSINESS_PROFILE`，全页无业务资料编辑入口（业务负责人只读，page.tsx:1515）。
- 建议：补编辑入口，或从 allowedActions 摘除避免「有权限没功能」。

**P2-12 密钥引用弹窗默认预选第一项**
- page.tsx:1278-1280 `listQuery.data?.credentialOpaqueOptions[0]?.referenceId ?? ""`，弹窗打开即预选第一个候选，用户低感知情况下可能直接轮换到错误引用。
- 建议：默认空选择，强制用户显式选择；选项加载失败时给错误提示（当前依赖列表查询，失败无反馈）。

---

## 6. 改进建议（按优先级）

1. **（P1-1）补齐承诺的二次确认**：为「启用连接」「健康检查」增加与停用一致的确认交互；若技术判断不需要，删除生产 Alert 中「均需二次确认」表述，保持言行一致。
2. **（P1-2）进度条数据真实化**：`BackgroundJobProgress` 的 total/completed/succeeded/failed 全部改由任务状态数据透传，删除硬编码 4/1/0；无法拿到时宁可隐藏进度条也不显示错误数字。
3. **（P1-3）补地址引用操作入口**：安全页签地址卡片增加「绑定/轮换地址引用」弹窗，与密钥弹窗同构，使 PENDING_CONFIG 连接配置闭环。
4. **（P1-4）内部标识清零**：按 ui-glossary.md 第 5 轮口径统一处理 connectionId、audit action、capabilityCode、operationId/traceId 等上屏问题；建议把扫描脚本固化为回归门禁。
5. **（P2 批量）**：页签文案「健康与尝试」→「健康检查」；能力徽章加图例；停用弹窗链接真实化；密钥弹窗取消默认预选；健康/审计分页。

---

## 7. 附：评审范围与声明

- 本评审基于代码静态阅读（mock 数据源），未进行真实浏览器走查。
- 角色演示条（RoleDemoBar）、面包屑、DataFreshness 等为共享组件行为，仅在影响本页体验处提及。
- 术语合规核对以 `docs/ui-glossary.md`（基线 v1.2 / 第 5 轮）为基准。
