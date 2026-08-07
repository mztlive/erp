# W21 · 供应商商品库与供给管理

> 状态：业务规则已定（入池双分支与池内匹配已落地实现）
> 页面模式：M2 供应商商品库 + M3 变化/异常队列 + M4 供应商商品中心
> 主要路由：`/procurement/supplier-catalog`、`/procurement/supplier-catalog/:supplierProductId`
> 主要角色：采购；运营只看发布准备信息，销售只消费符合资格的公司 SKU（销售查询称公司商品池）
> 最后更新：2026-08-07

## 1. 结论

W21 是所有供应商商品进入 ERP 的统一入口；禁止仅作为“API 供应商的供应商商品异常页”。

**一期范围（必须建设并启用）**：供应商商品库、Excel/手工录入、供应商 SPU/SKU、供应商 SKU
到公司 SKU 的映射、多供应商供给修订，以及新建/关联公司商品与供给。一期**禁止**对接 API
供应商；一期运行开关**不得**启用 API 连接、API 同步或 `source_type = API` 的正式写入路径。
**二期范围**：仅增加 W20 API 连接、自动同步及 API 来源变化/异常处理；必须继续复用一期的
供应商商品、映射、供给和公司 Product/SKU，**禁止**建立第二套目录或供给模型。前端可实现
两期界面骨架，但一期运行时仍受上述写入与开关约束。

- Excel、API、手工录入只是三种来源渠道，进入后都必须形成相同的供应商 SPU/SKU。
- 供应商商品库必须保存足以支撑首次建品的来源内容快照：名称、描述、可选来源商品类型、来源品牌/类目、结构化规格、条码、单位以及主图/轮播/详情媒体；允许导入来源缺字段，但缺失项必须在公司商品表单补齐。
- 采购必须先拥有完整的供应商商品库，再以**供应商 SPU（商品中心）**为作业上下文入池：
  系统先给出各供应商 SKU 的**池内状态与匹配候选**；有同款时走**关联入池**，无同款时走**反向入池**。
  SPU 是页面容器；正式映射/供给粒度始终是 `supplier_catalog_sku_id → company_sku_id`。
- 一个公司 SKU 可以关联多个供应商 SKU；每个关联必须分别维护一件代发供给价、集采供给价、
  集采起订量、税率、区域等。系统不设置供给方式字段，每条供给默认同时支持一件代发与集采。
- 第二家供应商的同款必须**关联已有公司 SKU**，不得再次「反向入池」新建重复公司商品。
- `sales_visible_price_gross` 与 `market_price` 都属于公司 `sku_revision`；前者供销售选品/报价使用，二者都不是供应商成本，也不得从最低成本自动推导。
- 销售查询、导出和下单只使用符合资格的公司 SKU（业务称为公司商品池），不得读取供应商商品库和采购成本。

商品主档与供给关系必须分离。供给关系的创建入口必须覆盖 Excel、API、手工三种来源，禁止仅绑定 API。手工与 Excel 来源必须提供逐供应商成本维护入口。

### 1.1 双向 SKU 创建边界

- 公司商品/SKU 正向创建：W14 `/master-data/products/new` 使用独立空白表单创建公司 SPU
  及一个或多个公司 SKU，随后可关联已有供应商 SKU，或由“添加供应商并登记成本”自动创建
  供应商商品/SKU。
- 供应商商品/SKU 独立创建：W21 `/procurement/supplier-catalog/new` 使用完整手工表单创建
  供应商 SPU 及一个或多个供应商 SKU，保存后仍处于待映射状态。
- **入池 Dialog（页头「入池」）**含两个分支，禁止用跳转 W14 空白页代替：
  - **关联已有**：挂已有公司 SKU，只写映射 `Active` + 双价供给；不改公司销售可见价/市场价。
  - **反向新建（反向入池）**：公司无同款时，同构新建公司 SPU + 勾选 SKU 行，并原子写映射与供给；
    名称等预填、采购可改；`product_kind`、销售可见价、市场价必填。
- 禁止将 `product_pool_entry`、`sellable-items` 或其 mock 作为独立稳定对象、ID 或写入路径；
  公司商品池必须仅为公司 Product/SKU 与有效 offering 的查询投影。

### 1.2 池内状态与匹配候选（采购如何知道「有没有同款」）

系统在入池前按**供应商 SKU 行**给出状态（证据排序，**不自动合并**；采购最终确认）：

| 状态 | 含义 | 默认动作建议 |
| --- | --- | --- |
| `MAPPED` | 已有生效映射 | 改供给 / 查看；禁止重复关联或反向 |
| `HAS_CANDIDATES` | 未映射但有公司 SKU 候选 | **关联已有** |
| `UNMATCHED` | 未映射且无可靠候选 | **反向新建** |

匹配证据（由强到弱，可多条并存）：

1. **条码 / GTIN 精确一致**（最高分）
2. **名称相近**（包含关系，弱）
3. **规格相近 / 规格线索**（弱）
4. **包装单位一致**（加分）

「同款」粒度是 **可售单位（SKU）**：颜色/尺寸不同映射同一公司 SPU 下不同 SKU；
单品与箱装通常是不同公司 SKU。不得仅按商品名称自动合并。

HTTP：`GET /admin/supplier-catalog/products/{id}/pool-match`（权限同商品详情 `supplier_catalog_product:detail`）。

## 2. 对象所有权

| 对象 | 负责工作面 | 说明 |
| --- | --- | --- |
| 公司商品 SPU/SKU | W14 基础资料 | 公司统一商品身份、规格、图文和生命周期 |
| 供应商商品 SPU/SKU | W21 供应商商品库 | 保存供应商自己的编码、名称、描述、可选来源商品类型、品牌/类目、规格、条码、单位、来源图文及来源修订；它与公司商品主档字段高度重合，但所有权和版本独立 |
| 供应商 SKU 映射 | W21 | 一条供应商 SKU 映射到一条公司 SKU；多条映射可指向同一公司 SKU |
| 供应商供给及修订 | W21 | `公司 SKU × 供应商 SKU × 供给修订`，采购成本的唯一事实来源 |
| 公司商品/SKU（销售查询称公司商品池） | W14 | 公司稳定身份与 `sku_revision`，其中包含销售可见价与市场价；W21 只通过映射和 offering 影响其销售资格，不创建独立池条目 |
| 商品发布 | W22 | 商城渠道售价、图文和上下架版本，不反写公司 SKU 价格或供给成本 |
| 销售单商品快照 | W05 | 固定 `sku_revision_id`、成交价及必要展示快照 |

## 3. 三种录入流程

### 3.1 Excel

1. 采购选择供应商并上传供应商商品表。
2. 系统校验列映射、供应商 SPU/SKU 唯一性、金额与规格，生成批次预览。
3. 采购确认后，合法行逐条形成供应商商品及不可变来源修订；错误行保留在批次错误清单。
4. 图片 URL 或文件必须归档到受控文件资产；短期签名 URL 不能作为长期商品媒体。来源未提供图片时仍可入供应商商品库，但标记内容待补齐。
5. 导入成功不等于关联公司 SKU，也不自动创建公司 SKU。
6. 采购从供应商 SPU 页面选择一个或多个供应商 SKU，逐项选择“关联已有公司 SKU”或
   “新建公司商品/SKU并关联供给”。新建分支自动预填同字段、允许采购修改，并要求独立
   `product_kind`、销售可见价与市场价必填；未选择的兄弟 SKU 不得被隐式创建或映射。

### 3.2 API

1. W20 管理连接、凭证引用和同步任务。
2. 同步任务把远端商品规范化为与 Excel 相同的供应商商品及来源修订。
3. 新增、关键变化、停供和错误进入 W21 变化队列；正常目录仍可在商品库浏览。
4. 新建/关联公司商品、映射和供给成本的后续流程与 Excel 完全相同；既有公司 SKU 的价格仍由 W14 维护。

API 连接是可选来源信息。Excel 和手工商品不得伪造 `connection_id`。

### 3.3 手工录入

有两个入口：

- W21「手工录入」：进入全页同构表单 `/procurement/supplier-catalog/new`（与公司商品分区一致），保存后进入供应商商品中心，再选择是否新建/关联公司商品与供给。
- W14 商品 SKU 行「添加供应商并登记成本」：固定当前公司 SKU，使用**最小对话框**一次性填写该供应商 SKU、两项供给价和供给条件（仍写 W21 实体）。名称、商品类型、规格、分类、品牌、单位、条码与图文从公司 SKU **正向复用**为供应商商品来源快照；对话框补录供应商、供应商 SKU 编码、**一件代发供给价（含税运）/ 集采供给价（含税）**、集采起订量、进项税率、供给区域和生效日期。两项供给价分别写入供给修订，不折叠成单一确认成本。税率、区域、生效日期有可靠版本化来源时自动预填并显示来源，采购可修改；无可靠来源时空白必填。禁止静默使用 `0.13`、“全国”或浏览器当天。系统不设置供给方式字段，默认两种方式都可用。**该正向入口只新增供应商 SKU 映射和供给，不展示或修改公司 SKU 的销售可见价/市场价。**不在该入口展示供应商商品媒体上传、来源描述、规格属性等字段——需维护时进入供应商商品中心全页编辑。

同一 SKU 需要多个供应商时，重复添加供给行。界面可以表现为多选/多行编辑，但提交必须拆成逐供应商供给记录；禁止只保存 `supplier_ids[]` 而没有对应成本和有效期。

Excel 导入仍用批次对话框；API 同步由 W20 触发。三种来源入库后的**展示与编辑形态统一**为供应商商品中心全页。

## 4. 入池（关联已有 / 反向新建）

采购在供应商商品中心或列表点击 **「入池」** 打开统一 Dialog。打开时请求 `pool-match`，
展示各供应商 SKU 的池内状态与匹配证据；默认分支：存在 `HAS_CANDIDATES` 时优先「关联已有」，
全部 `UNMATCHED` 时优先「反向新建」。

### 4.1 两分支共性

1. 作业上下文是**供应商 SPU**；勾选处理的是该 SPU 下**供应商 SKU 行**（未选兄弟 SKU 保持未映射）。
2. 必须确认本供应商的**一件代发供给价、集采供给价、进项税率、可供区域**；不选供给方式。
3. **集采起订量**取自供应商目录 SKU 修订，Dialog **不重复填写**；目录缺起订量则该行不可提交并引导回中心页补齐。
4. **确认即生效**：Dialog **不展示、不采集** 供给 `valid_from` / `valid_to`；服务端用业务日写入修订内部字段即可。
5. **进项税率 UI** 使用整数百分比（如 `13` + `%` 后缀），提交时换算为小数税率串（`0.13`）再调后端。
6. 正式双价可参考目录底价预填，可改；不得与销售可见价互相覆盖。

### 4.2 关联已有（第二家供应商 / 池内已有同款）

1. 仅对未映射且有候选（或采购选定目标）的 SKU 勾选。
2. 每行选择目标 **公司 SKU**（候选列表展示 `sku_no`、名称、规格、匹配证据、有效供应商数）。
3. 原子写入：映射直接 `Active` + 首条双价 `supplier_offering_revision`；**不得**修改目标公司 `sku_revision` 的销售可见价/市场价。
4. HTTP：`POST /admin/supplier-catalog/link-promote`（权限 `supplier_product_mapping:approve`）。

### 4.3 反向新建 / 反向入池（公司尚无同款）

1. 同构新建 **一个公司 Product** + 勾选的多行 **公司 SKU**（主档字段从来源预填，字典 ID 由采购确认）。
2. SPU 级必填：`product_kind`（不可由分类推导）、分类/品牌/基础单位。
3. 每行 SKU 必填：销售可见价、市场价；正式双价（缺省回退目录底价）。
4. 若勾选行仍带 `HAS_CANDIDATES`，提交前须二次确认，避免误建重复主档。
5. 单事务：公司 product/revision + 各 sku/revision + 映射 Active + offering 首修订 + 审计；任一步失败整单回滚。
6. 并发保护使用 **SPU 来源修订号** `expected_source_revision_no`（不得误传 SKU 修订号）。
7. HTTP：`POST /admin/supplier-catalog/reverse-promote`（权限 `supplier_product_mapping:approve`）。

同一公司 SKU 增加第二家供应商时，只走关联入池；销售可见价或市场价变更必须在 W14 单独修订。

供应商来源图文只作目录与匹配参考。映射已有公司 SKU 时不自动覆盖公司图文；采用第二供应商图片须在 W14 走公司商品修订。

来源报价与采购确认后的两项供给价必须分开：

- `dropship_floor_price_gross` / `bulk_floor_price_gross` / `bulk_minimum_order_quantity` 是供应商目录 SKU 上的代发底价（含税运）、集采底价（含税）与集采起订量。
- `dropship_supply_price_gross` / `bulk_supply_price_gross` 是入池时采购确认后生效的两项供给价（可参考对应目录底价，不自动覆盖，也不得合并成单一确认成本）。
- `sales_visible_price_gross` 与 `market_price` 都是公司 `sku_revision` 字段；前者供销售选品/报价使用。

上述价格事实不能互相覆盖，也不能自动保持相等。

## 5. 页面

### 5.1 供应商商品库

默认路由：`/procurement/supplier-catalog?mode=list`

页面提供：

- 来源筛选：全部、Excel、API、手工录入。
- 供应商、供应商 SPU/SKU、名称/规格、映射状态、供给状态、是否已关联公司 SKU。
- 来源描述、品牌/类目、结构化属性、条码、单位及主图/轮播/详情媒体完整度。
- “按 Excel 模板录入”“手工录入”（跳转全页新建）“**入池**”“打开商品中心”。
- “入池”Dialog：先展示 `pool-match` 状态表，再选 **关联已有 / 反向新建**；
  关联分支展示候选与匹配证据、目标公司 SKU 销售可见价摘要，**不修改**目标 SKU 价格；
  反向分支同构预填公司草稿，独立 `product_kind`、销售可见价与市场价必填。
- 从 W14 携带 `skuId` 进入时，只显示映射到该 SKU 的供给；即使尚无供给，也必须保留 W14 的 SKU 上下文和「添加供应商并登记成本」对话框入口。
- 搜索 300ms 防抖 + Enter + `/` 聚焦；「清除筛选」清搜索与来源筛选（list 模式同时清 `skuId` 锁定）。从 W14 带入的 `skuId` 锁定在 list 模式以共享 `FilterChip` 显性展示并可单独移除（queue 模式保留原「清除 SKU 筛选」）。list 模式不消费队列上下文参数（`changeType/status/autoNext`），进入 list 时清理残留。

### 5.2 变化与异常队列

路由：`/procurement/supplier-catalog?mode=queue`

- API 同步和后续 Excel 重导产生的新增、变化、停供、错误可进入队列。
- 停供、不可供、库存为零或新鲜度超时仍按既有安全规则暂停相关发布。
- 正常目录浏览不依赖任务；只有需要人工领取/终结的异常才使用 W02 的领取和动作契约。
- 来源是 Excel/手工时不显示“API 连接”链接。

### 5.3 供应商商品中心（与 W14 同构）

路由：

- 新建：`/procurement/supplier-catalog/new`
- 查看/编辑：`/procurement/supplier-catalog/:supplierProductId`

**页面模式与 W14 商品详情一致：详情即编辑**，保存形成新的**来源修订**（不写公司 `product`/`sku`）。

布局分区（吸顶作业条 + 左侧关联完整度助手 + 右侧分区导航）：

| 分区 | 与公司商品同构 | 供应商独有 | 说明 |
| --- | --- | --- | --- |
| 基础信息 | 名称、描述、商品类型、分类、品牌、单位 | 供应商、来源类型、供应商 SPU 编码 | 手工录入的来源商品类型必填；Excel/API 可缺失并在反向建品时补齐。分类 / 品牌 / 单位与 W14 使用同类控件；SPU 级不含条码与供给 |
| 图文信息 | SPU 轮播图、详情图 | 归档状态 | 主图不在此区；主图随 SKU |
| SKU / 规格与供给 | 规格维度编辑（名称 + 多取值） | **可编辑 SKU 表**：规格组合生成多行；每行可编供应商 SKU 编码、条码、**1:1 主图**、**一件代发底价（含税运）**、**集采底价（含税）**、**集采起订量**、可供数量/状态 | 与 W14 商品 SKU 表同构；主图为 1:1 小方块上传/预览；SKU 价格字段仅为上述三项，**不含**统一含税报价、进项税率、运费、区域、售后、商品能力等 |
| 映射与公司商品 | 公司商品/SKU 草稿、销售可见价、市场价 | 池内状态、匹配候选、映射历史 | 主动作：**入池**（关联已有 / 反向新建）；不展示来源版本差异、供给版本时间线、发布影响 |

页头主动作（采购）：填写检查、保存供应商商品资料、**入池**、返回。保存仅更新供应商商品资料，不自动改写公司商品或商品池价格。

成本区仅采购及明确授权的财务角色可见；销售、运营、管理员和技术角色返回掩码。非采购角色只读。

**反向新建分支**：以供应商 SPU 为上下文，勾选供应商 SKU 行；预填同名字段，采购确认
`product_kind`、字典身份、销售可见价与市场价。未归档媒体不得直接成为公司长期媒体。

**明确不在中心页展示**（仍可在队列/其他工作面出现）：来源版本差异、供给版本修订时间线、发布影响与恢复入口。

### 5.4 W14 商品编辑页

每个公司 SKU 行展示：

- 销售可见价（公司 `sku_revision` 字段）；
- 供给列只显示**供应商数量**，鼠标悬停弹出面板：供应商列表（暂无时给出空态）、「添加供应商」、「查看全部供给」；
- 库存列只有「查看库存」链接，不展示独立台账徽标。

「添加供应商并登记成本」是**最小对话框**（区别于手工录入全页表单）：固定当前 `sku_id`，名称/商品类型/规格/分类/品牌/单位/条码/图文从公司商品资料正向复用，形成供应商商品来源快照；要求填写供应商、供应商 SKU 编码、一件代发供给价（含税运）、集采供给价（含税）、集采起订量、进项税率、供给区域；不收集供给方式。税率、区域有可靠版本化来源时预填；无可靠来源时空白必填，禁止静默使用 `0.13`、“全国”。入池类 Dialog 与关联/反向路径**不向用户采集供给生效日**（确认即生效）。两项供给价分别写入同一供给修订。同一业务事务内创建或关联供应商商品及供应商目录 SKU、精确的 `supplier_catalog_sku_id → sku_id` 映射和供给修订；该正向路径不修改当前 `sku_revision` 的销售可见价或市场价。W21 携带 `skuContext` 进入列表页时，页头「添加供应商并登记成本」按钮使用同一对话框。

公司 SKU 可由 W14 独立创建，也可由 W21 **反向入池**创建；两种路径都必须由服务端分配稳定 `sku_id`。正向添加供应商与反向/关联入池最终都只能引用该稳定 ID，不得由前端临时生成。

## 6. 写命令与事务边界

### 6.1 导入/手工录入供应商商品

```text
CreateSupplierCatalogItem {
  source_type: EXCEL | API | MANUAL
  supplier_id
  source_reference?
  supplier_spu_code?
  supplier_sku_code
  name / description / specification / source_product_kind? / source_category / source_brand
  source_base_unit / barcode / structured_attributes
  source_media[] { usage, file_asset_id?, source_url?, archive_status, sort_order }
  dropship_floor_price_gross   // 一件代发底价（含税运）
  bulk_floor_price_gross       // 集采底价（含税）
  bulk_minimum_order_quantity  // 集采起订量
  available_quantity? / availability_status?
  idempotency_key
}
```

只创建供应商商品时不要求公司 SKU。W14 固定 SKU 入口可以在同一业务事务中继续创建映射和供给修订，不创建独立商品池条目或修订。

### 6.1b 供应商商品中心保存来源内容

```text
ReviseSupplierCatalogProduct {
  supplier_product_id
  expected_source_revision_no
  supplier_spu_code? / supplier_sku_code
  // 与 Create 相同的内容 + SKU 级来源报价观察字段
  name / description / specification / ...
  source_media[]
  dropship_floor_price_gross
  bulk_floor_price_gross
  bulk_minimum_order_quantity
  change_reason
  idempotency_key
}
```

仅形成新的 `supplier_catalog_*_revision`；**不得**修改公司 SKU 的 `sku_revision` 或已确认供给成本。并发时 `expected_source_revision_no` 冲突整体失败。

### 6.2 池内匹配

```text
GET /admin/supplier-catalog/products/{supplier_product_id}/pool-match

SupplierProductPoolMatchView {
  supplier_product_id
  source_revision_no          // SPU 当前来源修订号（入池并发键）
  items[] {
    supplier_catalog_sku_id
    supplier_sku_code
    specification?
    barcode?
    pool_status: MAPPED | HAS_CANDIDATES | UNMATCHED
    mapped_company_sku_id?
    mapped_company_sku_no?
    candidates[] {
      sku_id / sku_no / product_id / product_no
      name / specification? / barcode?
      base_unit_id
      sales_visible_price_gross?
      active_supplier_count
      match_signals[]           // 如「条码一致」「名称相近」「规格线索」「单位一致」
      score
    }
  }
}
```

### 6.3 关联入池

```text
POST /admin/supplier-catalog/link-promote

LinkPromoteToCompanyPool {
  supplier_product_id
  expected_source_revision_no   // 必须为 SPU 修订号，禁止传 SKU 修订号
  input_tax_rate                  // 小数串，如 "0.13"（前端由整数百分比换算）
  supply_region[]
  items[] {
    supplier_catalog_sku_id
    company_sku_id
    dropship_supply_price_gross?  // 空则回退目录代发底价
    bulk_supply_price_gross?      // 空则回退目录集采底价
    // bulk_minimum_order_quantity 不传：服务端读目录 SKU 修订
  }
  idempotency_key
}
// 无 valid_from：确认即生效；服务端可用业务日写入修订内部字段
```

单事务：各行映射 `Active` + offering 首修订 + 审计。不创建/不修改公司 product/sku 修订价格。

### 6.4 反向入池

```text
POST /admin/supplier-catalog/reverse-promote

ReversePromoteToCompanyPool {
  supplier_product_id
  expected_source_revision_no
  product_kind
  product_no?
  category_id / brand_id / base_unit_id
  input_tax_rate
  supply_region[]
  items[] {
    supplier_catalog_sku_id
    sku_no?
    dropship_supply_price_gross?
    bulk_supply_price_gross?
    sales_visible_price_gross   // 必填，写新建 sku_revision
    market_price                // 必填，写新建 sku_revision
  }
  idempotency_key
}
```

单事务：1 个公司 Product + N 个公司 SKU/修订 + N 条映射 Active + N 条 offering 首修订 + 审计。
反向分支不得接收客户端伪造的既有 `company_sku_id` 以“假装新建”；规格签名由服务端按来源属性
或供应商 SKU 编码派生以保证同行唯一。未选兄弟 SKU 不得隐式入池。

成功结果至少返回 `supplier_product_id`、`company_product_id`、各行 `company_sku_id` /
`mapping_id` / `offering_id`、审计引用与记录时间。来源修订冲突、已映射、缺起订量/底价时
整单失败，不允许半完成状态。

## 7. 权限与保密

| 角色 | 供应商目录 | 采购成本 | 销售可见价 | 写权限 |
| --- | --- | --- | --- | --- |
| 采购 | 可见授权范围 | 可见 | 可见 | 录入、映射、确认供给；新建公司 SKU 时确认初始价格，既有 SKU 调价从 W14 发起 |
| 销售 | 不直接消费 | 不返回字段 | 可见 | 选品、导出、下销售单 |
| 运营 | 可见必要摘要 | 掩码 | 可见 | 查看发布准备；不确认采购成本 |
| 财务 | 按职责只读 | 经字段授权可见 | 可见 | 核对，不改供给 |
| 管理员/技术 | 技术元数据 | 掩码 | 非必要不返回 | 修复连接/同步，不做采购决策 |

成本字段不能通过列表摘要、搜索索引、CSV 导出、错误文案、审计详情或前端隐藏字段泄露。权限判断必须在服务端投影层完成。

## 8. 销售消费边界

- 销售选品查询只返回 SKU 启用、当前 `sku_revision` 已配置销售可见价且至少一条 offering 在业务日期有效的公司 SKU；该查询结果称为公司商品池，但不产生独立池条目或池修订。
- 销售导出包含公司商品/SKU、规格、销售可见价、可售区域和必要图文，不包含供应商、成本、进项税、MOQ 或连接信息。
- 销售单行固定 `sku_revision_id` 和成交价；后续成本或 SKU 价格变化不改历史订单。
- 采购执行时再按已确认策略选择供给修订；不能让销售端直接选择供应商成本。

## 9. 验收标准

1. 一期 Excel、手工两种来源都能形成统一供应商 SPU/SKU，且没有虚假 API 连接；二期 API 来源接入后继续写入同一模型。
2. 一期 Excel/手工入库、二期 API 入库后的商品都默认只在供应商商品库，未关联公司 SKU 前销售不可见。
3. 入池 Dialog 提供 **关联已有 / 反向新建**；打开时展示 `pool-match` 状态与候选证据；
   有候选默认关联，无候选默认反向；第二家同款必须关联，禁止反向重复建档。
4. 反向新建：同字段预填可改；独立 `product_kind`、销售可见价、市场价必填；
   `product_kind` 不得由分类派生；起订量取自目录 SKU。
5. 一个公司 SKU 能维护至少两家供应商，分别拥有不同成本、MOQ、区域。
6. W14 SKU 供给列显示供应商数量，悬停面板可新增一条带成本的手工供给或查看全部供给。
7. 关联入池只形成映射与供给修订且不修改目标 `sku_revision`；反向入池还原子形成公司商品/SKU 及修订。失败无半完成状态。
8. 采购成本与销售可见价明确分栏，销售和运营请求中成本值为掩码或根本不返回。
9. 销售查询和导出只依赖符合资格的公司 SKU（公司商品池查询视图）；未关联公司 SKU 的供应商商品不可被下单。
10. 二期启用后，API 停供/不可供触发安全暂停，且不改写历史订单快照；一期不开启该入口。
11. 写命令有幂等键、`expected_source_revision_no`（**SPU** 修订号）、审计与冲突文案（含期望/当前版本）。
12. 来源无图时仍可进入供应商商品库；公司商品主图规则仅在 W14 建品/修订时校验。
13. 第二供应商关联已有公司 SKU 时，不修改该 SKU 修订；销售只看到一份公司商品和一个销售可见价。
14. 供应商商品中心与 W14 商品详情分区同构；手工新建使用全页表单。
15. 分类、品牌、单位与公司商品相同字典控件；规格使用相同规格维度编辑器。
16. 多规格多 SKU 行；目录价仅代发底价/集采底价/集采起订量；不含进项税率、区域等（入池时再确认）。
17. 系统不存在供给方式字段；每条正式供给默认同时支持一件代发与集采。
18. 中心页不展示来源版本差异、供给版本时间线、发布影响。
19. 中心页保存只形成来源修订（含 `skus[]`），带期望修订号与幂等键。
20. 同一 SPU 下多 SKU 可分别映射不同公司 SKU；未选兄弟 SKU 保持未映射。
21. 入池 Dialog：**不向用户采集供给生效日**（确认即生效）；进项税率为整数百分比 UI，提交换算小数；
    税率/区域无可靠来源时空白必填，禁止静默默认 `0.13` /「全国」。
