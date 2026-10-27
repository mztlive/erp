# 图稿参考与修订提示

## 参考图与修订提示

P02、P03 初稿以 P01 为风格参考；P04、P06、P07 初稿以 P02-pricing-v1.png 为工作区风格参考；P05 初稿以 P04-review-v1.png 为弹窗背景参考。P07 初稿中的待核实商品要求为“花生油礼盒 1.8L×2”。下列修订各以 input 对应图片作为编辑目标，不使用截图或浏览器。

### P02-pricing-v2.png

输入：P02-pricing-v1.png

```text
Edit this ERP pricing mockup with ONLY these precise corrections; preserve the shell, layout, all amounts, all other fields and typography. (1) Replace subtitle beneath 集采价, currently 适用于批量采购，达到起订量后生效。, with exact text 达到公司起订量时使用此参考价。 (2) The 单位 row must be a static grey read-only text surface reading 按各 SKU 的基础单位, with NO dropdown chevron; this is not a unit-editing field. (3) Rename the active workspace tab 商品库 to 批量选品入库. Keep all calculations 100→115, 80→92, 60→69 and percentages 15 unchanged. Output one standalone full screen, same aspect ratio and dimensions.
```

### P03-mapping-v2.png

输入：P03-mapping-v1.png

```text
Edit this ERP mapping screen with ONLY these corrections; preserve all unmentioned content, layout, neutral style and controls. (1) Check the checkbox 保留单项人工调整; it MUST be visibly black with white checkmark. (2) Replace radio text 全部覆盖 with 覆盖本批未入库项, and preserve 仅填空值 as selected. (3) Change the SKU counts in mapping rows 大米 from 120 to 132, 食用油 from 80 to 92, 面粉杂粮 from 36 to 12. The total remains exactly 236. (4) Replace amber message with 1 个来源分类尚未映射，涉及 12 个 SKU，可在核对页继续处理。 (5) Rename workspace tab 商品库 to 批量选品入库. No other changes. Output one complete desktop screen. IMPORTANT: Only the TOP workspace tab title changes; the LEFT SIDEBAR selected menu MUST remain exactly 商品库, never rename it or add a menu.
```

### P04-review-v2.png

输入：P04-review-v1.png

```text
Correct this ERP review screen while preserving all financial values, total counts, layout, style and content not explicitly mentioned. CRITICAL: (1) All four visible row checkboxes and the header checkbox must be visibly CHECKED, black with white ticks. Replace bottom-left 已选择 0 项 with 已选择全部 216 个可入库 SKU. The primary action stays 确认入库 216 个 SKU. (2) Move the 列设置 button from below the table into a slim toolbar immediately ABOVE the table, far right, leaving pagination below; do not duplicate it. (3) Correct company bulk threshold units by row: 五常大米 10袋, 葵花籽油 10桶, 香米 10袋, 食用油礼盒 10盒, instead of generic 10件. (4) Rename workspace tab 商品库 to 批量选品入库. Other numbers 236/216/12/8 and 180/16/20 and prices must remain exact. The scene shows selection already confirmed for all ready SKUs, never zero. Output one complete screen. IMPORTANT: Only the TOP workspace tab title changes; the LEFT SIDEBAR selected menu MUST remain exactly 商品库, never rename it or add a menu.
```

### P05-item-edit-v2.png

输入：P05-item-edit-v1.png

```text
Edit ONLY this modal screenshot's specified details, keep everything else unchanged, preserve dialog size and existing neutral ERP layout. (1) Move 已单独调整 and 原批量结果 ¥115.00 to directly beneath the LEFT 一件代发价 field 118.00, visually belonging to the 118.00 field; remove these notes from the right 集采价 field 115.00 because bulk price was not adjusted. (2) Change the suffix in 公司集采起订量 from 件 to 袋, so it reads 10 袋 and matches the source/company unit 袋. (3) Rename section 其他资料（可选） to 建档资料. (4) Make 公司计量单位 a read-only light grey field showing 袋 without dropdown, because source unit is explicitly known. (5) Rename background workspace tab 商品库 to 批量选品入库. Preserve API quote read-only ¥100.00, company prices 118.00 and 115.00, market price 139.00 and all footer actions. No new controls. IMPORTANT: Only the TOP workspace tab title changes; the LEFT SIDEBAR selected menu MUST remain exactly 商品库, never rename it or add a menu.
```

### P02-pricing-v3.png

输入：P02-pricing-v2.png

```text
Precisely edit this Chinese ERP mockup. Change ONLY the LEFT SIDEBAR selected navigation item under 供应商 API from 批量选品入库 to 商品库. The TOP workspace TAB must remain 批量选品入库, and main title must remain 批量选品入库. Preserve every other pixel and all numbers, pricing controls, tables, labels, whitespace, footer, and colors. No added menus. Output full screen at same dimensions. Date 2026-10-27.
```

