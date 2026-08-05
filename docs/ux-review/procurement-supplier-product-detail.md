# UX 评审：供应商商品详情 / 供给关系（/procurement/supplier-catalog/[supplierProductId]）

> 评审视角：产品经理 / UX
> 评审范围：
> - `erp-client/features/supplier-catalog/supplier-product-center-page.tsx`（详情即编辑）
> - `erp-client/features/supplier-catalog/supply-relationship-list-view.tsx`（供给关系列表）
> - `erp-client/features/supplier-catalog/catalog-write-dialogs.tsx`（录入 / 登记供给 / 入池对话框）
> - 关联：`supplier-catalog-page.tsx`（列表/队列容器）、`supplier-product-form-model.ts`（校验模型）、`types.ts` / `api.ts`（数据契约）、`docs/ui-glossary.md`（术语基线 v1.2）
> 结论：P0 = 0，P1 = 6，P2 = 16

---

## 1. 页面概述

本页承载三条核心链路：

1. **查看/编辑供给品来源资料**（中心页）：从列表「详情」进入，页面为"详情即编辑"模式（`supplier-product-center-page.tsx:4-8`），分区：基础信息 / 图文信息 / SKU·规格与供给 / 映射与入池，支持「填写检查」「保存来源版本」（形成新修订）、「加入公司商品池」。
2. **供给关系列表**（同路由族的 `/procurement/supplier-catalog` list 模式）：从公司 SKU（W14）进入时展示"当前 ERP 商品 SKU ← 多条供应商供给关系"，含来源、采购条件、确认成本、供货范围、状态列；从商品库进入时为全部供应商商品。
3. **登记/入池对话框**：固定 SKU 的「添加供应商并登记成本」最小表单、手工/Excel 录入对话框、「加入公司商品池」对话框。

整体文案质量高：业务词（一件代发底价、集采底价、入池、来源修订、可供状态）使用正确，术语表合规度好；入池完整度侧栏、匹配信号提示、成本掩码等设计亮点明显。

---

## 2. 易用性

**顺畅路径（亮点）**
- 列表 → 详情：行内「详情」直达（`supply-relationship-list-view.tsx:367-378`），且「加入公司商品池」在行内与详情页均可发起，路径短。
- 从固定 SKU 进入时：「添加供应商并登记成本」对话框只补录供应商差异字段（SKU 编码、双底价、起订量），公司侧资料自动复用（`catalog-write-dialogs.tsx:532-538`），效率设计正确。
- 入池完整度侧栏的每条未完成项可点击定位到对应分区（`supplier-product-center-page.tsx:760-785`），"问题 → 位置"闭环好。
- 入池对话框的公司 SKU 下拉自带匹配信号（条码一致/规格一致…）并在下方提示"不能仅按名称合并"（`catalog-write-dialogs.tsx:1013-1021`），防误合并到位。

**不顺畅处**
- **详情页看不到供给关系核心数据**：中心页完全没有渲染 `item.offering`（供给价/确认成本、供给起订量、可供区域、生效期 validFrom/validTo、供给状态）。这些数据列表里有（`supply-relationship-list-view.tsx:253-333`），点进"详情"反而消失。详情页只有 SPU 内容 + SKU 底价 + 映射状态，对"查看供给关系"诉求是反向的。
- 必填的「变更原因」文本框被放在 SKU 分区最底部（`supplier-product-center-page.tsx:1474-1490`），而校验错误只在顶部 Alert 提示（`supplier-product-center-page.tsx:873-879`）。用户改完基础信息直接保存，报错后要滚到页面最底才能找到必填项。
- 无任何未保存离开防护：头部「返回」「加入公司商品池」、映射卡片「打开公司商品」「在商品与 SKU 中查找」都是裸链接，编辑内容未保存时直接跳走即静默丢失（`supplier-product-center-page.tsx:671-675, 1542-1568`）。

---

## 3. 信息密度

**关键信息突出情况**
- 顶部徽章（新供应商商品/关键变化/停止供应/异常数据）与「待采购复核入池」Alert 位置正确（`supplier-product-center-page.tsx:612-626, 695-705`）。
- 成本字段掩码后输入框显示 `***` 且禁用（`supplier-product-center-page.tsx:1387-1421`），并有"价格/税率/费用字段已按权限隐藏"徽标（707-709），符合掩码语义。

**过疏 / 过密**
- **过疏（核心缺口）**：详情页缺「供给状态（正常供货/已暂停/已停供/待确认）」「采购确认成本」「生效期」「可供区域」——供给品详情最重要的"供给"维度为空白。列表页有（`supply-relationship-list-view.tsx:274-333`），详情页无，存在信息断裂。
- 过密：头部副标题同时堆了 供应商、来源 r{n}、SPU/SKU 编码、角色、队列上下文 5 类信息（`supplier-product-center-page.tsx:628-651`），其中"队列 {queueContextId}"是调试信息（见问题 P1-4）。
- SKU 表为 9 列、min-width 72rem 横滚（`supplier-product-center-page.tsx:1268-1269`），可接受但价格列无单位/币种标注（表头已注明含税，可接受）。
- 「SKU 行数」徽标 `{n} / {n} 行`（`supplier-product-center-page.tsx:1264-1266`）分子分母恒相等，无信息量，疑似应为"已填行/总行"。

---

## 4. 交互合理性

**合理处**
- 加载骨架屏、错误空态（含返回入口）、列表页错误重试（`supplier-catalog-page.tsx:808-833`）齐全。
- 提交幂等键 + 成功后 `router.replace` 到详情页（`supplier-product-center-page.tsx:473-489`），防重复提交意识正确。
- 入池对话框在"已有池价"时默认「沿用现有价格（推荐）」并展示当前销售可见价与有效供应商数（`catalog-write-dialogs.tsx:1022-1030`），误改价格防护到位。
- 保存成功文案明确"不会自动改写公司商品或商品池价格"（`supplier-product-center-page.tsx:720-727`），降低对作用域误判。

**不合理处**
- 生效日期默认值硬编码 `"2026-08-02"` 三处（`supplier-product-center-page.tsx:481`、`catalog-write-dialogs.tsx:844, 917`），而录入/登记对话框用 `todayIso()`（`catalog-write-dialogs.tsx:66-68, 283, 641`）。评审日 2026-08-05，默认生效日已过期 3 天，用户不手动改就会保存出一个已过期的生效期。
- 详情页错误态无「重试」按钮，只有"返回列表"（`supplier-product-center-page.tsx:546-559`），加载失败一次就退出，不符合"错误说下一步"。
- 「填写检查」按钮（`supplier-product-center-page.tsx:676-685`）与提交共用同一套本地校验（`supplier-product-form-model.ts:479-522`），且通过后的提示是"必填项完整，保存时仍以系统校验结果为准"（`supplier-product-center-page.tsx:881-889`）——检查结果自称不可靠，按钮语义存疑。
- 提交中仅禁用按钮，无"正在保存…"状态文案；按 Enter 仍可触发 submit（`supplier-product-center-page.tsx:686-689, 449-461`）。
- 删除规格项（`supplier-product-center-page.tsx:1164-1176`）会重建整张 SKU 表，无任何二次确认；用户已填的行级编码/价格若与新组合无法匹配会按自动编码重建（`supplier-product-form-model.ts:186-214`），无法撤销。
- 修改后与当前修订完全一致时仍可保存形成空修订（`supplier-product-center-page.tsx:449-511` 无差异检测）。
- Excel「导入 Excel」对话框（`catalog-write-dialogs.tsx:325-339`）实际上只记录文件名，SPU/SKU/报价/规格全部仍需手工逐项填写——按钮文案与行为不符（术语表 §1.3 契约）。
- 对话框成功后按钮禁用、无"查看详情"出口，只能"关闭"后回列表再找（`catalog-write-dialogs.tsx:489-499`）。

---

## 5. 问题清单（按严重度）

### P0（阻断操作）：无

### P1（明显阻碍效率 / 数据或权限正确性）

| # | 问题 | 位置 |
|---|------|------|
| P1-1 | **详情页不展示供给关系核心数据**（供给状态、采购确认成本/双供给价、供给起订量、可供区域、生效期 validFrom/validTo）——列表有、详情无，"详情"反而信息更少，查看/确认供给关系的核心诉求落空 | `supplier-product-center-page.tsx`（全文无 `offering` 渲染）对照 `supply-relationship-list-view.tsx:253-333` |
| P1-2 | **未保存离开无防护**：返回/加入公司商品池/打开公司商品等链接直接跳走，已编辑内容静默丢失，且无 beforeunload/路由守卫 | `supplier-product-center-page.tsx:671-675, 1542-1568` |
| P1-3 | **权限上下文在跳转中丢失**：列表页 `maskCost=1`/`demoRole` 后点「详情」，详情链接（及队列模式的 centerHref）均不带这两个参数，被掩码的成本在详情页以默认 procurement 角色重新可见 | `supply-relationship-list-view.tsx:370-374`、`supplier-catalog-page.tsx:798-800` 对照 `supplier-product-center-page.tsx:248-254` |
| P1-4 | **内部 ID / 调试信息上屏**：①错误态直接展示 `supplierProductId`；②页头展示 `队列 {queueContextId}`（形如 `queue:W21:procurement:all:all`，含 W21 工作面编号，违反术语表 §1/§3.6/§4）；③SKU 上下文缺失时回退显示原始 `skuId` | ①`supplier-product-center-page.tsx:552`；②`supplier-product-center-page.tsx:644`；③`supply-relationship-list-view.tsx:392-394, 461`、`supplier-catalog-page.tsx:945` |
| P1-5 | **生效日期默认值硬编码过期**：三处 `"2026-08-02"`，与录入/登记对话框的 `todayIso()` 不一致；按默认保存会产生已过期的生效期（评审日 2026-08-05） | `supplier-product-center-page.tsx:481`、`catalog-write-dialogs.tsx:844, 917` |
| P1-6 | **「导入 Excel」按钮语义与行为不符**：对话框只记录文件名，全部字段仍需手工填写，无解析/预览/校验反馈，用户以为导入即完成实则被要求手录整表 | `catalog-write-dialogs.tsx:325-339, 356-450` |

### P2（体验瑕疵）

| # | 问题 | 位置 |
|---|------|------|
| P2-1 | 必填「变更原因」位于 SKU 分区底部，校验错误仅顶部提示，无字段级定位 | `supplier-product-center-page.tsx:1474-1490, 873-879` |
| P2-2 | 「填写检查」与提交共用同一本地校验，结果提示自称"以系统校验结果为准"，权威性存疑、按钮冗余 | `supplier-product-center-page.tsx:676-689, 881-889` |
| P2-3 | 同一枚举 STALE 三种文案：详情"过期" / 列表"供货信息待更新" / 队列"信息待更新" | `supplier-product-center-page.tsx:1460`、`supply-relationship-list-view.tsx:88-89`、`supplier-catalog-page.tsx:188-194` |
| P2-4 | 「采购条件」列金额裸数字无 ¥/单位/含税标注（表头在别处）；指标「供应商 SKU」实为供给关系条数，口径与名称不符 | `supply-relationship-list-view.tsx:265-268, 486` |
| P2-5 | 提交中无"正在保存…"状态，仅按钮禁用；Enter 可绕过提交按钮的 pending 禁制 | `supplier-product-center-page.tsx:449-461, 686-689` |
| P2-6 | 新建保存成功后立即 `router.replace`，成功 Alert 在跳转中丢失，用户无法确认"已保存成功"，存在重复点击风险 | `supplier-product-center-page.tsx:473-489, 484-488` |
| P2-7 | 详情页查询错误态无「重试」，只有返回列表 | `supplier-product-center-page.tsx:546-559` |
| P2-8 | 映射历史直接渲染 `history.status` 原始枚举（当前 mock 恒空，属隐患） | `supplier-product-center-page.tsx:1528-1537`（类型 `types.ts:114-120`） |
| P2-9 | 「已在同一事务中保存」——"事务"为禁用实现词，应改"同时生效/同一次提交" | `catalog-write-dialogs.tsx:671` |
| P2-10 | 进项税率说明自相矛盾："无可靠来源时留空" vs "缺失时无法保存，需先补充来源"（3 处同文案） | `catalog-write-dialogs.tsx:430, 754, 1108` |
| P2-11 | 删除规格项无二次确认，SKU 行重建导致行级已填数据不可恢复（编码/价格自动重建） | `supplier-product-center-page.tsx:1164-1176`、`supplier-product-form-model.ts:186-214` |
| P2-12 | 同一字段两种标签：「登记供给」叫"供给起订量"，SKU 表叫"集采起订量" | `catalog-write-dialogs.tsx:747-749` 对照 `supplier-product-center-page.tsx:1295` |
| P2-13 | 对话框成功后无"查看详情"出口，提交按钮禁用后只能关闭，创建成果不可直达 | `catalog-write-dialogs.tsx:489-499, 770-780, 1122-1132` |
| P2-14 | 无内容差异检测，未修改即可保存形成空修订 | `supplier-product-center-page.tsx:449-511` |
| P2-15 | 页头"来源 r{n}"版本记号（`r1`）偏内部实现风格，建议"来源版本 1" | `supplier-product-center-page.tsx:636-638` |
| P2-16 | 「SKU 行数」徽标 `{n} / {n} 行` 恒等无信息量 | `supplier-product-center-page.tsx:1264-1266` |

---

## 6. 改进建议（按优先级）

1. **补全详情页供给维度（P1-1）**：在「映射与商品池」卡片之上（或并入其内）新增"供给条件"区块，渲染 `item.offering.currentRevision`：供给状态徽章（复用列表 `relationshipStatus` 的文案映射）、采购确认成本（掩码感知）、双供给价、供给起订量、可供区域、生效期。中心接口已返回该数据（`types.ts:263-268`），纯展示层工作。
2. **未保存离开防护（P1-2）**：维护表单 dirty 标记（对比 hydrate 基线），对返回/入池/公司商品跳转统一走确认或自动保存草稿；参考 `supplier-catalog-page.tsx` 已有会话草稿机制（`api.ts:637-660`）。
3. **权限上下文透传（P1-3）**：列表/队列 → 详情的链接补齐 `demoRole`、`maskCost` 参数（与 `centerHref` 同构处理），或在中心页消费队列缓存的 `costFieldVisibility`。
4. **ID 与调试信息清零（P1-4）**：错误态改为"未找到该供应商商品，可能已被移出目录范围"；删除页头队列上下文；SKU 上下文缺失时展示"商品规格待补充"而非原始 id。上线前按术语表 §5.2 扫描。
5. **生效日期动态化（P1-5）**：统一走 `todayIso()`，删除硬编码日期；对话框重新打开时重算。
6. **Excel 导入语义（P1-6）**：要么接入真实解析+预览（推荐），要么把按钮改为"手工录入（Excel 模板来源）"并在对话框说明当前为模板登记。
7. **表单体验（P2-1/2/5/6/14）**：变更原因上移或校验报错时滚动定位；提交按钮加"正在保存…"态并禁用表单；创建成功改为详情页顶部成功横幅（携带 `?created=1`）；保存前做内容差异检测。
8. **文案对齐（P2-3/4/9/10/12/15/16）**：STALE 统一"供货信息待更新"；金额列统一 `¥` 前缀与单位；"同一事务"改"同时生效"；进项税率文案改为"留空时提交会要求补充，建议先向供应商确认"；起订量统一标签；删除恒等徽标。

---

*评审基准：`docs/ui-glossary.md` v1.2；页面代码截至 2026-08-05。*
