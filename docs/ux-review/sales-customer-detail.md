# UX 评审报告：销售 › 客户中心 › 客户详情（/sales/customers/[customerId]）

- **评审日期**：2026-08-05
- **评审视角**：产品经理 / UX
- **主要代码**：
  - `erp-client/features/customers/customer-detail-page.tsx`（页面主体）
  - `erp-client/features/customers/customer-form.tsx`（创建/编辑共用表单）
  - `erp-client/features/customers/queries.ts`、`types.ts`、`session.ts`
  - `erp-client/components/business/document.tsx`、`values.tsx`、`feedback.tsx`、`page.tsx`
  - `erp-client/app/(workspace)/sales/customers/[customerId]/page.tsx`（路由壳）
  - 参考：`docs/ui-glossary.md`、`mock/customers.ts`

---

## 1. 页面概述

客户详情页是「客户中心」的对象中心壳（M4 模式），自上而下为：

1. 面包屑 + 「返回选择」按钮（PageHeader `object-chrome`）
2. 对象身份头（DocumentHeader）：法定名称 + 客户号 + 版本 + 状态徽章 + 负责/协作人 + 主操作（上传合同 PDF / 新建销售单）
3. 停用告警（仅停用客户）
4. 关系指标条（MetricStrip）：有效合同 / 进行中销售单 / 应收余额 / 逾期金额
5. 五个 Tab：概览 / 合同与销售 / 票款摘要 / 经营摘要 / 归属与审计
6. 概览 Tab 内：主体身份（DocumentSummary）→ 联系与地址 → 银行账户，其中「编辑资料」以内嵌表单（TanStack Form）原位切换

整体结构成熟：对象身份、关键指标、分区 Tab、分区级加载/失败态、敏感信息打码揭示、权限阻断提示（GuardedBusinessAction）都有完整设计，文案整体业务化（「打码」「系统汇总」「打开客户往来」符合术语表）。主要问题集中在**编辑流程的数据安全与反馈**、**Tab 切换状态丢失**、以及若干文案与一致性瑕疵。

---

## 2. 易用性

### 顺畅的路径
- 查看详情 → 编辑资料：概览 Tab 首位区块头部即有「编辑资料」按钮（customer-detail-page.tsx:380），原位切换表单，不丢上下文。✓
- 关联单据：合同/销售单在「合同与销售」Tab 内直接给出最近 3 条 + 状态徽章 + 「打开」按钮（:936-956），并有「查看全部合同/销售单」带 `customerId` 预过滤跳转（:623-641）；目标列表页确认消费 `customerId` 参数（contracts-list-page.tsx:109）。✓
- 往来/经营：票款摘要 Tab 用术语表映射的 `openWorkspaceLabel("W11")` → 「打开客户往来」（:685），经营摘要 Tab → 「打开经营质量」（:769），按钮语义为动作词。✓

### 不顺畅的路径
- **编辑中切换 Tab 会静默丢弃全部未保存输入**（详见问题 #2，P1）：无确认、无提示。
- **「负责销售」无法变更**：编辑表单 grouped 分支没有负责人字段，归属与审计 Tab 只读（问题 #10，P2）。
- 停用客户提示「上传合同和新建销售单已禁用」，但主操作按钮禁用原因通过 hover 提示呈现，移动端无 hover 时只能看到灰色按钮（GuardedBusinessAction 禁用时为不可聚焦按钮 + tooltip，见 feedback.tsx:124-138；移动端 tooltip 触发不可靠）。次要。

---

## 3. 信息密度

总体把握得当：首屏 = 身份 + 4 个关系指标 + 2 个主操作，都是销售决策高频信息（有效合同、在途销售单、应收、逾期）；「应收余额」带「客户往来汇总」tooltip、「进行中销售单」带「系统汇总 · 非列表求和」tooltip 防口径误解（customer-detail-page.tsx:324-337）。

待改进点：
- 票款摘要 Tab 与指标条重复展示「应收余额/逾期金额」两项（:313-343 vs :696-718）。摘要重复可接受，但 Tab 内未标注「与上方指标一致」，用户可能误以为是增量数据。P2。
- 概览 Tab 三个区块（主体/联系地址/银行账户）均为平铺展示，无折叠能力；联系人与地址多时页面很长（DocumentSection 未用 `collapsible`，customer-detail-page.tsx:376-605）。P2 可选项。
- 「统一社会信用代码」全码展示（:423）：对销售角色是否必要值得商榷，但属字段权限设计，不展开。

关键信息突出度：客户等级（规模标签/利润贡献）只在「经营摘要」Tab 内，首屏没有体现「大客户/高贡献」这类经营分层信号；「逾期金额」虽有指标，但无颜色/风险强调（均为同一样式）。建议后续在指标条或身份头叠加经营分层徽章。P2 建议。

---

## 4. 交互合理性

### 良好的交互
- 加载：骨架屏 + `aria-busy`（:144-155）；错误：分区级 BusinessFailureState，失败分区不抹掉已成功分区（identity/contacts/related 均带重试按钮）。✓
- 敏感信息：SensitiveValue 打码 + 主动揭示 + 15 秒自动隐藏 + 揭示失败提示（feedback.tsx:910-1023），联系方式/地址/账号按字段权限分级。✓
- 权限：GuardedBusinessAction 保留禁用位并给原因。✓
- 编辑表单：Zod 校验、修订原因必填、beforeunload 防刷新丢失、冲突解决对话框、未知结果「查询最终结果」幂等闭环（customer-form.tsx:257-341, 777-800）。✓
- Tab 切换用 `router.replace(..., { scroll: false })`，保持滚动位置，Tab 栏 sticky。✓

### 问题交互
- **编辑态下 Tab 可点击且会整体重挂载页面**：路由壳对 `[customerId]-[section]` 设 `key`（page.tsx:26），Tab 切换 = 全页 remount = `editing` 状态（customer-detail-page.tsx:128）重置为 false，未保存输入直接消失，**没有 DiscardConfirmDialog**（该对话框只挂载在取消按钮与 beforeunload 上）。这是本次评审最严重的交互缺陷之一（P1，见问题 #2）。
- **保存成功后无任何成功反馈**：`onSucceeded` 立即 `setEditing(false)` 卸载表单（customer-detail-page.tsx:372），FormalActionResult「客户资料已保存」块（customer-form.tsx:750-775）在页面模式下从未有机会展示；页面上也无 toast。用户只能靠数据刷新判断成败（P1，问题 #3）。
- **分区错误态不一致**：settlement（:689-693）与 audit（:836-840）两个分区失败时只有提示、没有重试按钮，其余四个分区均有（P2，问题 #7）。
- 敏感揭示失败后无「重试」或「刷新页面」引导（feedback.tsx:1017-1020）（P2，问题 #15）。

---

## 5. 问题清单

### P0（阻断操作 / 数据安全）

| # | 严重度 | 位置 | 问题 |
|---|---|---|---|
| 1 | P0 | `customer-form.tsx:130-133`（`editableValue`）+ `:162/:168/:174` | **脱敏值会被当真实值写回**：编辑表单用 `editableValue(token, masked)` 预填，无揭示令牌（或无权限/令牌过期）时把打码串（如 `138****6210`、`上海市浦东新区 ****`、`****6210`）直接放进可编辑文本框；保存时这些打码串作为「新值」提交（`saveW03CustomerDetails`），静默把客户手机/地址/银行账号改写成打码文本。当前 mock 数据都有令牌所以演示不触发，但 `types.ts` 明确支持 `phoneRevealToken?: undefined` 的 masked 状态，是真实可达路径。**建议**：无令牌的字段在编辑态保持打码只读并提示「编辑前先揭示」，或保存时对含 `*` 的值拒收/跳过。 |

### P1（明显阻碍效率 / 数据风险）

| # | 严重度 | 位置 | 问题 |
|---|---|---|---|
| 2 | P1 | `app/(workspace)/sales/customers/[customerId]/page.tsx:26`（key remount）+ `customer-detail-page.tsx:128,346-351` | **编辑中切换 Tab 静默丢失全部未保存输入**：`key` 含 section 导致切 Tab 即全页重挂载，`editing` 归零；DiscardConfirmDialog 只挂在取消/关页，Tab 切换无拦截。用户在「概览」编辑到一半点「票款摘要」→ 内容全丢且无任何提示。**建议**：① 切 Tab 前若 `editing && dirty` 弹放弃确认；② 或改为 `useState` 切 Tab 不 remount（去掉 key 中 section 依赖）。 |
| 3 | P1 | `customer-detail-page.tsx:372` + `customer-form.tsx:319-322,750-775` | **页面编辑保存成功后零反馈**：`onSucceeded` 立即退出编辑态，FormalActionResult 成功块在页面模式下不会展示，全站无 toast。用户点「保存修订」后只能自行确认数据是否更新。**建议**：保存成功后在读视图顶部展示短暂成功提示（toast 或 Alert），并保留「保存成功，新版本 vN」字样。 |
| 4 | P1 | `customer-detail-page.tsx:439` | **枚举原值泄漏**：字段标签「负责销售（OWNER）」直接把 `OWNER` 上屏，违反术语表规则 7（枚举原值禁止渲染，`docs/ui-glossary.md:114`）。同页 audit Tab（:859）用的是映射「负责销售/协作销售」，应统一为「负责销售」。 |
| 5 | P1 | `customer-form.tsx:171-176` | **编辑银行账户时「默认账户」标记被静默清空**：`buildDefaults` 对 `bankAccounts` 硬编码 `isDefault: false`（联系人与地址都保留了原值），保存后原默认账户标记丢失且无任何提示。**建议**：回填 `b.isDefault`（注意 session 覆写层也要保留该字段）。 |
| 6 | P1 | `customer-detail-page.tsx:179` | **未找到客户文案泄漏内部 ID**：`未找到客户 ${customerId}` 直接输出 `cust_128` 这类内部 ID，术语表规定内部 ID 不得展示（glossary §1.3-7）。**建议**：改为「客户编号有误」或展示用户可见的 `customerNo`（mock 中 URL 与客户号并不一致，URL 本就是内部 ID）。 |

### P2（体验瑕疵）

| # | 严重度 | 位置 | 问题 |
|---|---|---|---|
| 7 | P2 | `customer-detail-page.tsx:689-693, 836-840` | 票款摘要/归属审计分区失败态没有重试按钮，与 identity/contacts/related/quality 四分区不一致（用户只能整体刷新）。 |
| 8 | P2 | `customer-detail-page.tsx:812-819` | 经营摘要 `DataFreshness` 文案生硬：`updatedAt` 传入「数据」/「数据可能不是最新」，渲染为「经营质量 数据 2026-07-28… 数据已更新」，读感差；应直接展示「经营质量汇总于 {时间}」。 |
| 9 | P2 | `customer-detail-page.tsx:615` + `mock/customers.ts:229` | 「合同与销售」区块说明「列出有效合同」，但列表含「已终止」合同（HT-2024-0402），文案与数据口径矛盾；建议改为「列出最近合同」。 |
| 10 | P2 | `customer-form.tsx:370-403`（grouped 分支无 owner 字段） | 「负责销售」在编辑表单与归属审计 Tab 均不可变更，无任何入口；若负责人调整是真实业务流程，属功能缺口。 |
| 11 | P2 | `customer-form.tsx:725-748` | 页面级编辑表单内出现「演示结果」演示控件（冲突/未知结果模拟），紧挨「保存修订」；按 G6 演示环境可保留，但建议视觉上弱化为调试组件。 |
| 12 | P2 | `customer-detail-page.tsx:582` | 银行账户行展示内部编号 `BA-128-01`（session.ts:132 生成，含客户 ID 序号），属内部标识，建议仅显示户名+银行+末四位。 |
| 13 | P2 | `customer-detail-page.tsx:54-72` | `?section=contacts` 被静默映射回 overview（:67）且 SECTION_NAV 的 `hash` 字段（:57）从未使用；深层链接语义混乱，建议删除 hash 字段并在非法 section 时明确处理。 |
| 14 | P2 | `feedback.tsx:1017-1020` | 敏感揭示失败只提示「暂时无法显示敏感信息」，无重试/刷新引导；且页面直接调用 `revealCustomerSensitiveField`（customer-detail-page.tsx:496-501）而 queries.ts:172 已定义未使用的 `useRevealCustomerSensitiveMutation`，建议走 mutation 以便统一 loading/error。 |
| 15 | P2 | `customer-detail-page.tsx:238-250` | 页面头「返回选择」按钮与面包屑「客户中心」功能重复，双入口占首屏空间，可二选一。 |
| 16 | P2 | `customer-detail-page.tsx:491-507, 508` | 打码策略不一致：手机号打码、邮箱明文展示；地址打码但联系人姓名明文。如邮箱/姓名同属敏感字段，建议统一分级（当前仅手机/地址/账号有 `fieldVisibility`）。 |
| 17 | P2 | `customer-detail-page.tsx:313-343, 696-718` | 指标条与票款摘要 Tab 重复展示应收余额/逾期金额，未标注一致性，用户易误读为增量数据。 |
| 18 | P2 | `customer-detail-page.tsx:296-305` | 停用客户告警文案良好，但主操作禁用原因仅 hover tooltip 可见，移动端无法得知「为什么按钮灰了」；建议在告警内直接列出原因。 |

---

## 6. 改进建议（按优先级）

1. **（P0）修掉脱敏值写回**：编辑态无揭示权限的敏感字段改为只读打码 + 「编辑前请先揭示」提示；保存端对含 `*` 的敏感值拒收。这是数据完整性红线。
2. **（P1）Tab 切换保护**：去掉路由壳 `key` 中 section 依赖（改为组件内状态切 Tab），或切 Tab 时 `editing && dirty` 先弹 DiscardConfirmDialog。
3. **（P1）保存成功反馈**：编辑成功后回到读视图并展示「已保存 · 新版本 vN」的短暂提示（Alert + 自动消失或 toast）。
4. **（P1）文案合规**：删除「（OWNER）」（:439）、不展示内部 `customerId`（:179）；统一枚举中文映射。
5. **（P1）回填默认账户标记**：`buildDefaults` 中 bankAccounts 保留 `b.isDefault`。
6. **（P2）一致性**：补全 settlement/audit 分区重试；统一敏感字段分级（邮箱、姓名）；修正「有效合同」口径与 DataFreshness 文案；去掉重复导航入口。

---

## 7. 总结

页面骨架与信息架构成熟，权限、打码、分区容错、幂等提交等硬功能力到位，文案整体合规。核心短板集中在**编辑闭环**：一处静默数据损坏（P0）、一处静默丢输入、一处无成功反馈（P1），以及少量枚举/内部 ID 泄漏与一致性瑕疵。
