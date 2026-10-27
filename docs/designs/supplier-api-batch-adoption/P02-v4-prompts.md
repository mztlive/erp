# P02 V4 图稿生成提示

方式：内置 imagegen。状态：两张 V4 状态图均已获用户确认（2026-10-27）。旧版保持原文件，不修改生产代码。

## 同价状态

输入图片：P02-pricing-v3.png；输出：P02-pricing-v4.png。

```text
Use case: ui-mockup. Redesign ONLY the pricing work surface in this attached Chinese ERP screenshot. The attachment is an existing design to simplify, NOT a layout to copy within the content area. Create ONE full desktop screen at 1440x1024 approximately, same aspect as reference. Keep the ERP topbar, light left sidebar with 商品库 selected, workspace tab 批量选品入库, graphite primary color, neutral palette and Chinese sans serif. Today 2026-10-27. No new navigation. User specifically complains too much text and inconvenient interaction: the redesigned content MUST contain dramatically fewer labels and sentences, fewer controls, more breathing room, clear click targets. Do not just make fonts smaller. Body 16px, inputs around 42px.
Header: heading 设置售价. Small one-line supplier and scope 华选供应链 · 236 个 SKU. Compact four step indicator 设置售价 / 补齐资料 / 核对入库 / 入库结果 with first active. Remove long batch identifier and extra duplicate large headings from work surface.
Main content open white surface, not nested cards. Width about 940 within workspace, aligned from x280. Single mode segmented control three options: 仅代发 | 代发与集采同价 | 分别设置. Middle selected with subtle grey fill and dark text. Generous spacing below.
One simple horizontal rule composer, label 批量售价 above it; composer reads 接口采购报价 + [ 15 ] % with a small method dropdown 按比例加价 at beginning. Only ONE input 15, no duplicate cost-basis disabled input, no paragraph under it. A concise secondary phrase after or below the composer 两种售价使用同一规则 is acceptable, but preferably avoid it as segmented state already conveys it.
Next line 公司集采起订量 [10] followed small inline 按商品单位 and small info icon. Do NOT provide an editable unit dropdown. All company selling prices tax-inclusive, convey once via preview heading 售价预览（含税）.
Separate sparse preview section below a thin divider. Small heading 售价预览（含税）. Only TWO rows in a clean light table, columns 商品 / 接口采购报价 / 公司代发价 / 公司集采价. Row 五常大米 5kg | ¥100.00 | ¥115.00 | ¥115.00. Row 葵花籽油 5L | ¥80.00 | ¥92.00 | ¥92.00. Values right aligned, clear typography. No product images necessary. No alerts in default view, no success metrics.
Below preview a single collapsed disclosure row chevron 更多设置, with concise low contrast summary 仅填空值 · 保留人工调整 · 已有 SKU 原价不变. No expanded radio controls or explanatory paragraphs. Source cost is read-only; only company selling price calculated. No large capability disclaimer in normal state; restrictions can appear contextual in subsequent review.
Sticky footer only low-key 已保存 at left, 返回商品库 outline secondary and 下一步 black primary at right. No big saved badge. Primary action visually anchors footer.
There should be clearly more whitespace than reference. All essential input and preview above fold; do not add illustration, stats, side panels, repeated headings, tutorial prose, or any filler. Preserve exact amounts and Chinese text. The 15 and 10 values are user-entered example values, not automatic defaults.

```

## 分别设置状态

输入图片：P02-pricing-v4.png；输出：P02-pricing-separate-v4.png。

```text
Use case: ui-mockup precise state variation. Attached image is the simplified pricing screen. Preserve its ERP shell, header, stepper, supplier 华选供应链, 236 SKU scope, typography, spacing and sparse neutral palette. Create ONE independent full screen at same dimensions for clicking 分别设置. Date 2026-10-27. Change ONLY main pricing state and corresponding preview amounts.
Select the third segment 分别设置; the middle 代发与集采同价 is unselected.
Replace the single 批量售价 composer with TWO compact aligned rows under one quiet section heading 售价规则. Row one label 代发价, method dropdown 按比例加价, composer 接口采购报价 + [15] %. Row two label 集采价, method dropdown 按比例加价, composer 接口采购报价 + [10] %. Keep numbers as clear editable input boxes. Labels short, no added instructions or duplicate cost basis fields.
Keep 公司集采起订量 [10] 按商品单位 and info icon under these two rows, compact enough to preserve generous whitespace. Preview section remains exactly two rows with headers 商品 / 接口采购报价 / 公司代发价 / 公司集采价. 五常大米 5kg: ¥100.00 / ¥115.00 / ¥110.00. 葵花籽油 5L: ¥80.00 / ¥92.00 / ¥88.00. Correct math essential.
Keep 更多设置 collapsed with summary 仅填空值 · 保留人工调整 · 已有 SKU 原价不变. Keep footer 已保存 and 返回商品库 / 下一步：补齐资料. Do not add hints, alerts, cards, prose, illustrations, or new controls. Do not shrink type to accommodate rows; make vertical gaps slightly tighter in rules area. The single comparison to original should demonstrate two direct editable rows when prices differ.
```

