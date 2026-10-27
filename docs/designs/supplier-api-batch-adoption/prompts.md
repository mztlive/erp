# 页面效果图生成提示

日期：2026-10-27。生成方式：内置 imagegen。每图完整提示由“共用约束”与对应“页面要求”拼接；无现有系统截图附件，不使用 CLI 或浏览器截图。P01 从文字规范生成；后续页面使用本组已生成图片作为风格参考。具体参考图、P07 待核实商品名称和最终版本修订提示以 [图稿参考与修订提示](revision-prompts.md) 为准。

## 共用约束

```text
Use case: ui-mockup. Create one realistic, production-quality Chinese ERP desktop UI design, a single screen only, at 1440 x 1024. This is a coherent seven-screen review set for API supplier batch product adoption into company catalog. Current date anchor 2026-10-27. These are design mockups, no live data. Reference source is existing repository erp-client/app/globals.css, docs/erp-ui-design.md and the UI components, inspected as text; there is no screenshot attachment.
Style MUST preserve established ERP: near-white #fafafa canvas, white work surface, graphite #18181b primary buttons and text, very pale grey sidebar #f9f9fb, subtle grey rules, 8px input/button radii. Small semantic green success, amber needs-attention, red failure only. No blue primary, purple gradients, decorative charts, mock browser frame, perspective or device bezel. Chinese PingFang-style readable sans-serif, 26px heading, 14px body, 12px metadata, tabular right-aligned money. Page padding 24-32px, 36px controls, 44px table header, 64px two-line product rows. Generous whitespace, purposeful grouping with separators; no cards nested in cards. Crisp legible Chinese labels, no gibberish, no English lorem ipsum. Long names can wrap. UI contains one primary action and restrained secondary controls.
Common shell: 48px top bar 福尚云 ERP; left 220-240px light navigation including 工作台, 销售管理, 采购管理, 商品资料, 供应商 API with 商品库 selected, 财务管理. Compact workspace tab below topbar. All content fits canvas; no clipped footer. Use simple line icons where useful. Supplier identity 华选供应链 is illustrative. Batch business number 入库批次 RK-20261027-001. Screens in the workflow show a horizontal stepper 设置售价 → 补齐资料 → 核对入库 → 入库结果, with current step emphasized in graphite.
Business invariants: API purchase costs are read-only and never edited or increased; batch formulas write only company sales reference prices (含税). New company SKUs remain 未上架. Existing company SKU prices retained. Source missing bulk procurement terms cannot be invented from company bulk sales price. Batch selection 240 source SKUs minus 4 excluded = 236 selected across 180 products. Review: 216 ready, 12 need info, 8 previously adopted. Ready composition: 180 new SKUs, 16 new variants, 20 linked existing SKUs. Final results: 210 success + 4 failures + 2 awaiting verification + 12 need info + 8 previously adopted = 236. Pending total 18.
Create only the screen requested below; other features belong off-screen.
```

## P01 供应商品库：跨页全选与排除

输出文件：P01-catalog-v1.png

```text
Primary screen catalog selection. Workspace tab 商品库. Page identity 华选供应链, secondary 供应商品库. Page top right secondary 同步商品; below identity API 设置 / 商品库 tabs with 商品库 active. Compact filter surface: 商品搜索 text field 名称、规格或订货编码; 供应商分类 multi-select 粮油 with 包含子分类 checked; 品牌 全部; 接口采购报价 ¥50 to ¥200; 可供状态 可供; 选品状态 未入库; 查询 and 重置 actions. Applied chips 粮油（含子分类）, 报价 50–200 元, 可供, 未入库.
Table toolbar left 共 240 个 SKU; selection banner states 已选择全部筛选结果 240 个 SKU · 已排除 4 个 · 最终选中 236 个, with 查看排除项 and 清空选择 secondary text actions. A secondary note 本页 20 个 SKU makes page selection scope obvious. Right of toolbar graphite primary 配置入库（236） and column settings icon. Table checkboxes, 商品 / 来源分类 / 品牌 / 接口采购报价 / 可供状态 / 入库状态. Show five legible rows including 五常大米 5kg ¥100.00, 葵花籽油 5L ¥80.00, 香米 5kg ¥60.00, 食用油礼盒 2L×2 ¥120.00, 东北大米 10kg ¥150.00. Small realistic generic product thumbnails optional, not logos. All in grain/oil classification; checked rows, one unchecked excluded row visually muted with 已排除. Costs read-only. Footer 180 个商品 · 236 个 SKU 已选, pagination page 1 / 12 with 20条/页. Do not introduce batch pricing fields yet.
```

## P02 批量入库：设置公司售价

输出文件：P02-pricing-v1.png

```text
Full batch workspace titled 批量选品入库 with supplier 华选供应链 and RK-20261027-001. Step 设置售价 active. Compact range line 粮油 · 180 个商品 / 236 个 SKU · 已排除 4 个. Main section heading 公司销售参考价（含税）, supporting sentence 采购报价来自接口，以下规则仅设置公司售价。 Use two balanced columns separated by whitespace and a thin vertical line. Left 一件代发价: 计算方法 dropdown 按比例加价, 计算基础 read-only 接口采购报价, 加价比例 numeric field 15 with % suffix. Right 集采价: method 按比例加价, same read-only basis, 加价比例 15 %, 公司集采起订量 10, 单位 按商品单位. Under both, an unobtrusive line 来源不支持集采时，本供给仍仅可代发。 Below separator 应用范围 radio row 仅填空值 selected / 覆盖本批未入库项 unselected, checked 保留单项人工调整. A 3-row illustrative mini table 计算示例 with headers 商品 / 接口采购报价 / 公司代发价 / 公司集采价, rows 五常大米 5kg 100.00 115.00 115.00, 葵花籽油 5L 80.00 92.00 92.00, 香米 5kg 60.00 69.00 69.00. Note 自动关联已有 SKU 时保留原售价。 Fixed footer 保存草稿 · 已保存 on left; 返回商品库 secondary; black 下一步：补齐资料 primary on right. Keep fields comfortably readable, not a dense technical configuration dashboard.
```

## P03 批量入库：分类映射与资料补齐

输出文件：P03-mapping-v1.png

```text
Same batch workspace, step 补齐资料 active. Main section 公司分类映射. Small line 只影响新建商品，已有商品保留原分类。 A clean 3-row table: 来源分类 / 涉及 SKU / 公司分类. Rows 大米 / 120 / 粮油副食 > 米面; 食用油 / 80 / 粮油副食 > 食用油; 面粉杂粮 / 36 / 请选择公司分类 (amber border, 待补齐). Numbers total 236. Above table restrained bulk control 将选中分类统一设为 [选择公司分类] [应用]. Add narrow help text 同一来源商品的规格沿用同一公司分类。 Below broad divider section 供给条件 with 2x2 field grid: 进项税率 13 %; 配送模板 华选常温配送; 生效日期 2026-10-27; 有效期 长期有效. Subtle line above grid 来自接口的采购价保持不变。 At bottom of form a low-key amber inline message 1 个来源分类尚未映射，可在核对页继续处理受影响商品。 Selection radio 仅填空值 selected and checkbox 保留单项人工调整 can sit in compact shared footer of form. Fixed bottom bar 已保存, 上一步 secondary and 下一步：核对入库 black primary. No charts, no warehouse stock entry, no supplier price editing.
```

## P04 批量入库：提交前核对

输出文件：P04-review-v1.png

```text
Same batch workspace, step 核对入库 active. Important crisp summary in a single metric strip not cards: 本批 236 个 SKU | 可入库 216 | 待补齐 12 | 已入库跳过 8. Secondary line 就绪项：新建 SKU 180 · 新增规格 16 · 关联已有 SKU 20. Tabs 全部 236 / 可入库 216 / 待补齐 12 / 已入库 8, 可入库 active. Small hint 关联已有 SKU 保留原售价；新建 SKU 入库后未上架。
Main wide table checkbox, 商品与规格, 处理方式, 接口采购报价, 公司代发价, 公司集采价 / 起订量, 操作. Only 4 readable rows: 五常大米 / 5kg / 新建 SKU / ¥100.00 / ¥118.00 with small amber 已单独调整 / ¥115.00 · 10件 / 调整; 葵花籽油 / 5L / 新建 SKU / ¥80.00 / ¥92.00 / ¥92.00 · 10件 / 调整; 香米 / 5kg / 新增规格 / ¥60.00 / ¥69.00 / ¥69.00 · 10件 / 调整; 食用油礼盒 / 2L×2 / 关联已有 SKU / ¥120.00 / ¥149.00 保留原价 / ¥139.00 · 10件 保留原价 / 查看. Show a small secondary capability note 本供给仅支持代发 under one relevant row, without hiding the company sales values.
Table has pagination and column settings far right. Below table a restrained amber line 12 个待补齐项不会提交，8 个已入库项将跳过。 Fixed bottom bar 将入库 216 个 SKU; 上一步, 保存草稿 as secondary and black 确认入库 216 个 SKU primary. All summary numbers consistent. Do not make it seem all 236 will be submitted.
```

## P05 单项调整：人工价格例外

输出文件：P05-item-edit-v1.png

```text
Show the review page lightly dimmed (20% black overlay, NO blur) behind a centered 760px wide edit Dialog. It is a form, not a narrow read-only drawer. Dialog title 调整入库商品 with close icon. Identity 五常大米 · 5kg and code HX-RICE-001; small status 新建 SKU. First a compact read-only provenance strip 接口采购报价 ¥100.00 and 来源规格 5kg, 来源单位 袋, all clearly static not editable.
Section 公司销售参考价（含税） has two columns: 一件代发价 input 118.00 and 集采价 input 115.00; second row 公司集采起订量 input 10 and 市场价 input 139.00. Next to one changed input show small 已单独调整 and subtle 原批量结果 ¥115.00. Display 本供给仅支持代发；公司集采参考价不改变供应商采购条件。 Below, a quiet line divider and optional fields 公司展示名称 五常大米, 公司计量单位 袋, 公司分类 粮油副食 > 米面 read-only or selected. A text button 恢复批量规则 is below price fields. Footer hint 批量重新计算时默认保留本项调整。 Bottom right 取消 outline and 保存调整 black primary. No writable API cost field, no target-SKU picker, no force-create button. Plenty of breathing room, all dialog content visible.
```

## P06 入库结果：部分完成与后续处理

输出文件：P06-result-v1.png

```text
Same batch workspace step 入库结果 active, title 本批入库部分完成 with small amber status badge 有待处理项, not an all-success banner. Show business batch number and small timestamp 2026-10-27 14:32. One metric strip with five mutually exclusive counts: 已成功 210 (green); 失败 4 (red); 结果待核实 2 (amber); 待补齐 12 (neutral amber); 已入库跳过 8 (grey). Compact explanatory line 本批共 236 个 SKU；已提交 216 个，待处理 18 个。 Another thin line 新建 SKU 保持未上架；关联已有 SKU 保留原状态。 
Primary content a result table, tabs 已成功 210 active / 待处理 18 / 已跳过 8. Four successful sample rows 五常大米 5kg, 葵花籽油 5L, 香米 5kg, 食用油礼盒 2L×2. Columns 商品与规格 / 处理结果 / 公司商品 / 上架状态 / 操作. New rows show 已新建 or 已新增规格, business SKU codes e.g. SP-00881-01, 未上架. Existing linked row shows 已增加供给 and 保留原状态. Row actions 打开公司商品 and 查看供给 are quiet text actions.
A small amber inline notice 2 个操作结果待核实，请先查询原结果，避免重复入库。 Footer 返回商品库 secondary and black 处理待办（18） primary. Do not show automatic publish or retry-all buttons. No pie charts or celebratory illustration. Table and count strip need to be clear and immediately useful.
```

## P07 待处理商品：修正与重新核对

输出文件：P07-recovery-v1.png

```text
Same batch shell, title 待处理商品 and scope 入库批次 RK-20261027-001 · 18 个 SKU. Compact tabs 全部 18 / 待补齐 12 / 失败 4 / 结果待核实 2; 全部 active. Brief note 已成功入库的商品不会再次提交。 The main table has checkbox / 商品与规格 / 问题 / 本次处理 / 操作. Show 5 clear rows: 杂粮礼盒 2kg, 缺少公司分类, selected dropdown 粮油副食 > 杂粮 now filled, small 已修正, checked; 全麦面粉 5kg, 配送模板未选择, dropdown 华选常温配送 now filled, 已修正 checked; 香米 10kg, 来源报价已变化, old ¥90.00 → new ¥95.00 with small 需重新计算售价, 已确认新报价 checked; 五常大米 10kg, 条码与规格不一致, text 请核对来源规格, 查看差异 action, unchecked disabled; 食用油礼盒 2L×2, 操作结果待核实, text 先查询原操作结果, 查询结果 action, disabled checkbox.
Use gentle red/amber state labels, explanatory text not technical exception dumps. A restrained banner results-waiting reads 结果待核实的 2 项不能直接重试。 Bottom sticky bar 已选择 3 项，重新核对后再确认入库; 返回入库结果 secondary, black 重新核对 3 项 primary. No button that immediately submits unreviewed changes; no hidden auto retry. Table columns readable and aligned, not crowded. Avoid a sidebar with arbitrary metrics.
```

