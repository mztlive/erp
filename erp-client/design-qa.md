# 商品编辑页设计 QA

## 对照范围

- 参考：`.codex/product-editor-reference/02-media-info.png`、`.codex/product-editor-reference/03-price-inventory.png`
- 实现：`.codex/product-editor-qa/04-implementation-media-filled.png`、`.codex/product-editor-qa/05-implementation-sku-filled.png`、`.codex/product-editor-qa/06-implementation-history-tab.png`、`.codex/product-editor-qa/07-implementation-history-tab-active.png`、`.codex/product-editor-qa/08-implementation-detail-images-no-preview.png`、`.codex/product-editor-qa/09-implementation-spec-name-field.png`、`.codex/product-editor-qa/10-implementation-sku-compact-main-image.png`
- 浏览器与视口：Dia，1224 × 768；参考与实现使用同一窗口尺寸。
- 状态：均使用已有商品的“图文信息”和“价格与库存”填写态；ERP 使用 `prd_1` 的轮播图、详情图、规格与两行 SKU 数据。

## 全局与重点区域证据

- 全局：页面保留 ERP 原导航、主题 token、组件圆角和字阶；主体改成左侧填写助手、顶部分区导航、分段卡片和底部固定操作栏。
- 图文重点：轮播图为有序卡片，首图有标识，支持上移、下移、删除、批量上传；详情图使用有序卡片，不再额外显示页面预览。
- 规格重点：规格项与规格值分层展示，支持新增、删除、上下移动；规格值变化自动重建 SKU 组合。
- SKU 重点：规格维度动态成列；支持批量售价/成本价、行内价格、条码、SKU 编号、主图与启停状态；库存保持 ERP 所有权，只读展示并跳转库存台账。

## 视觉检查

- 字体：沿用项目现有字体与字号，没有引入外部字体。
- 间距与密度：参考抖店的紧凑表单、分组间距和宽表密度；在 ERP 侧栏占用下保持横向滚动，不压缩字段。
- 颜色与边框：仅使用既有 `background/card/muted/border/primary` 等语义 token，没有新增业务颜色。
- 图片：没有复制或发布抖店素材；ERP 仍按现有文件名数据展示，未改变媒体字段结构。
- 文案：使用 ERP 对象语义；库存明确标记“独立台账”，避免误导为商品资料可写库存。

## 交互检查

- 顶部分区导航：基础信息、图文信息、价格与库存、生效信息、历史与引用依次排列并可定位；新建商品不显示仅适用于已有商品的历史页签。
- 历史与引用：实测点击页签后定位到独立分区，变更历史、引用与选用、操作记录三块均保留原数据和展示内容。
- 详情图：实测“页面预览”已移除，图片排序、删除和上传入口保持可用。
- 规格名称：增加独立字段标签、规格项编号徽标和更清晰的输入框表面，名称与规格值的层级可以直接辨认。
- SKU 主图：上传框使用表格紧凑密度，选择前后都维持与价格、编号字段接近的行高，不再纵向撑高整张表格。
- 规格组合：实测新增“颜色”，添加“红色/蓝色”后从 1 行自动生成 2 行 SKU；刷新后临时测试数据清除。
- 已有数据：实测从新建页进入 `prd_1`，切换图文与 SKU 分区后，轮播图、详情图、规格、价格和 SKU 编号保持不丢失。
- 批量价格、媒体排序/删除、SKU 状态开关均有实际状态更新；未触发保存。
- 浏览器：修正 Combobox 的 Base UI `nativeButton` 契约后，Dia 的 Next.js issues badge 消失；最新开发日志没有当前页面错误。

## 验证

- `npx eslint components/ui/combobox.tsx features/master-data/product-detail-page.tsx`
- `npx tsc --noEmit --pretty false`
- `npm run lint`：0 errors；50 个仓库既有 warnings，修改文件无 warning。
- `npm run verify:workspaces`：30 个 workspace，0 failures。
- `npm run build`：生产构建成功，33 个静态页面生成完成；仅有仓库多 lockfile 的既有 workspace-root warning。

## 历史页签布局修正

- 问题截图：`/var/folders/hp/2pcnc0sd05zdp3blph8r6yjr0000gn/T/codex-clipboard-e9d6d1d8-8af9-4890-a7da-0780ac5d66a1.png`，2048 × 1215 px。
- 修正截图：`.codex/product-editor-qa/11-implementation-history-layout-connected.png`，Dia 1224 × 768 CSS px，截图 1224 × 768 px。
- 组合对比：`.codex/product-editor-qa/11-history-before-after.png`；两侧等宽缩放并补白到 1024 × 768 后横向拼接，仅用于比较内容容器关系。
- 状态：已有商品 `prd_1`，点击“历史与引用”页签后的定位状态。
- 初始 P1：历史内容位于表单栅格之外，横向覆盖填写助手列，并被保存操作栏与生效信息断开。
- 修正：将历史内容移入右侧主内容列、放在生效信息之后和保存操作栏之前；三块内容合并到与其它表单分区一致的圆角、边框、背景和阴影容器中。
- 修正后证据：历史卡片左、右边界与生效信息一致，填写助手列保持独立，变更历史、引用与选用、操作记录由卡片内分隔线连续组织。
- 字体与文案：沿用原字体、层级和业务文案；颜色继续使用语义 token；没有新增或替换图片资产。
- 交互：Dia 实测页签定位正常，三块原有数据均保留；未触发保存或发布。
- 结论：初始 P1 已消除，没有剩余 P0/P1/P2 布局问题。

## SKU 卡片宽度与价格字段修正

- 问题截图：`/var/folders/hp/2pcnc0sd05zdp3blph8r6yjr0000gn/T/codex-clipboard-2096efb0-96bd-4c64-9291-780092675902.png`，2678 × 1006 px。
- 实现截图：`.codex/product-editor-qa/13-sku-card-contained.png`，Dia 1224 × 768 CSS px，截图 1224 × 768 px。
- 组合对比：`.codex/product-editor-qa/13-sku-width-before-after.png`；问题截图按等高缩放后与实现截图横向拼接，用于比较卡片边界和页面横向溢出。
- 状态：已有商品 `prd_1`，两行 SKU 的价格与库存填写态；未触发保存或发布。
- 初始 P1：`fieldset` 采用浏览器默认最小内容宽度，内部宽表将整个“价格与库存”卡片撑出右侧主列，导致页面级横向滚动。
- 修正：为卡片增加 `min-w-0 max-w-full overflow-hidden`，将横向滚动约束到 SKU 表格容器自身；销售价标题同步改为完整业务文案“销售价”。
- 修正后证据：SKU 卡片左右边界与“商品规格”“生效与原因”一致，页面不再被宽表撑开；表格仍可在卡片内部横向查看全部列。
- 字体与文案：沿用现有字体、字号、字重；标题与表头层级未改变。
- 间距与布局：卡片外边界和相邻分区对齐；表格单元格密度、圆角和间距保持原设计。
- 颜色与 token：仅使用既有 `card/border/surface-sunken/muted/primary` 语义 token，没有新增颜色。
- 图片质量：SKU 主图继续使用已有文件名和紧凑上传框，没有替换或生成图片资产。
- 价格数据待确认：当前 `ProductSkuFields` 只有 `salePrice` 与单个 `costPrice`；“一件代发成本”和“集采成本”需要两份独立成本数据。在不改变现有数据结构的约束下，无法将两者都做成可独立保存的输入项。

## final result

blocked
