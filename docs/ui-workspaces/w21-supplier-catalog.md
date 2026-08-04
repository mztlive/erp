# W21 · 供应商商品库与供给管理

> 状态：已确认业务方向，接口与持久化待实现
> 页面模式：M2 供应商商品库 + M3 变化/异常队列 + M4 供应商商品中心
> 主要路由：`/procurement/supplier-catalog`、`/procurement/supplier-catalog/:supplierProductId`
> 主要角色：采购；运营只看发布准备信息，销售只消费符合资格的公司 SKU（销售查询称公司商品池）
> 最后更新：2026-08-04（确认 W21 一期启用、双价格、SKU 级映射，以及公司商品池不是独立实体）

## 1. 结论

W21 不再是“API 供应商的供应商商品异常页”，而是所有供应商商品进入 ERP 的统一入口。

**阶段边界已经确认**：第一期完整建设并启用供应商商品库、Excel/手工录入、供应商
SPU/SKU、供应商 SKU 到公司 SKU 的映射、多供应商供给修订，以及新建/关联公司商品与供给；第一期不
对接 API 供应商。第二期只增加 W20 API 连接、自动同步及 API 来源变化/异常处理，继续复用
第一期的供应商商品、映射、供给和公司 Product/SKU，不建立第二套目录或供给模型。前端可以完整
实现两期界面骨架，但一期运行开关不得启用 API 连接、API 同步或 `source_type = API` 的正式
写入路径。

- Excel、API、手工录入只是三种来源渠道，进入后都形成相同的供应商 SPU/SKU。
- 供应商商品库必须保存足以支撑首次建品的来源内容快照：名称、描述、可选来源商品类型、来源品牌/类目、结构化规格、条码、单位以及主图/轮播/详情媒体；允许导入来源缺字段，但缺失项必须在公司商品表单补齐。
- 采购先拥有完整的供应商商品库，再从供应商 SPU 页面逐个选择供应商 SKU：有同款时关联
  已有公司 SKU；没有同款时在“新建/关联公司商品与供给”Dialog 中使用来源同字段预填并创建公司商品/SKU。SPU
  只作为页面容器和批量选择范围。
- 一个公司 SKU 可以关联多个供应商 SKU；每个关联分别维护一件代发供给价、集采供给价、集采起订量、税率、费用、区域和有效期。系统不设置供给方式字段，每条供给默认同时支持一件代发与集采。
- 第二家供应商的同款供应商 SKU 继续保留独立身份和来源版本，但可映射到同一公司 SKU；不得重复创建公司商品。
- `sales_visible_price_gross` 与 `market_price` 都属于公司 `sku_revision`；前者供销售选品/报价使用，二者都不是供应商成本，也不能从最低成本自动推导。
- 销售查询、导出和下单只使用符合资格的公司 SKU（业务称为公司商品池），不读取供应商商品库和采购成本。

原先“商品主档与供给关系分离”的方向保留；错误在于把供给关系的创建入口绑定到了 API，同样也没有给手工商品提供逐供应商成本维护入口。

### 1.1 已确认的双向 SKU 创建边界

- 公司商品/SKU 正向创建：W14 `/master-data/products/new` 使用独立空白表单创建公司 SPU
  及一个或多个公司 SKU，随后可关联已有供应商 SKU，或由“添加供应商并登记成本”自动创建
  供应商商品/SKU。
- 供应商商品/SKU 独立创建：W21 `/procurement/supplier-catalog/new` 使用完整手工表单创建
  供应商 SPU 及一个或多个供应商 SKU，保存后仍处于待映射状态。
- 反向创建：先有供应商 SKU；W21“新建/关联公司商品与供给”在没有同款公司 SKU 时显示“新建公司
  商品/SKU”分支。名称、描述、来源商品类型、分类、品牌、单位、规格、条码、已归档主图/图文等语义相同
  字段自动预填，采购可二次修改；独立 `product_kind`、销售可见价与市场价均必填，并写入新建 `sku_revision`。提交原子创建公司商品/SKU、
  精确映射和双价供给修订，不创建独立商品池条目或修订。
- **当前实现状态（与上述已确认设计的代码缺口）**：前端只实现已有公司 SKU 选择器与
  公司 SKU→供应商 SKU 路径；供应商 SKU→公司 SKU 的反向创建 Dialog 属一期待实现，
  不能再以前往 W14 空白页代替。若当前前端仍将 `product_pool_entry`、`sellable-items` 或其 mock 作为独立稳定对象、ID 或写入路径，也属于缺口：应改为公司 Product/SKU 与有效 offering 的查询投影。

## 2. 对象所有权

| 对象 | 主责 | 说明 |
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

## 4. 新建/关联公司商品与供给

采购在供应商商品库选择“新建/关联公司商品与供给”后必须完成：

1. 选择目标分支：**关联已有公司 SKU**，或在无同款时**新建公司商品/SKU并关联供给**。新建
   分支固定当前 `supplier_catalog_sku_id`，自动预填同字段，采购可二次修改；独立
   `product_kind`、销售可见价和市场价必须显式确认，商品类型不能由分类推导，价格不能从
   来源底价或采购成本推导。
2. 确认当前供应商的一件代发供给价（含税运）、集采供给价（含税）、集采起订量、进项税率、费用、区域和有效期；不选择供给方式。
3. 新建分支把销售可见价与市场价写入新建公司的 `sku_revision`；关联已有公司 SKU 的分支不修改其价格。
4. 已有分支原子写入供应商 SKU 映射和供给修订；新建分支还须在同一数据库事务创建公司商品/SKU及其修订。任一步失败全部回滚，返回各稳定 ID/修订 ID。

同一公司 SKU 增加第二家供应商时，只新增映射和供给修订；不得借此修改目标 `sku_revision` 的价格。销售可见价或市场价需要变更时，采购必须在 W14 公司商品/SKU 编辑页按该 SKU 的修订规则单独提交。

“同款”不能只按名称自动判断：系统按 GTIN/条码、厂家货号、品牌型号、结构化规格和包装单位给出匹配证据，采购确认最终映射。完全相同的可销售单位映射同一公司 SKU；颜色/尺寸等规格不同映射同一公司 SPU 下的不同 SKU；单品与箱装通常是不同公司 SKU，并记录包装换算。

供应商来源图文只作目录与匹配参考。映射已有公司 SKU 时不自动覆盖公司图文；采购若选择采用第二供应商更好的图片，必须在 W14 创建公司商品新草稿修订并审核。供应商后续图片变化只产生来源差异，不直接改写公司商品。

来源报价与采购确认后的两项供给价必须分开：

- `dropship_floor_price_gross` / `bulk_floor_price_gross` / `bulk_minimum_order_quantity` 是供应商目录 SKU 上的代发底价（含税运）、集采底价（含税）与集采起订量。
- `dropship_supply_price_gross` / `bulk_supply_price_gross` 是关联公司 SKU 时采购确认后生效的两项供给价（可参考对应目录底价，不自动覆盖，也不得合并成单一确认成本）。
- `sales_visible_price_gross` 与 `market_price` 都是公司 `sku_revision` 字段；前者供销售选品/报价使用。

上述价格事实不能互相覆盖，也不能自动保持相等。

## 5. 页面

### 5.1 供应商商品库

默认路由：`/procurement/supplier-catalog?mode=list`

页面提供：

- 来源筛选：全部、Excel、API、手工录入。
- 供应商、供应商 SPU/SKU、名称/规格、映射状态、供给状态、是否已关联公司 SKU。
- 来源描述、品牌/类目、结构化属性、条码、单位及主图/轮播/详情媒体完整度。
- “导入 Excel”“手工录入”（跳转全页新建）“新建/关联公司商品与供给”“打开商品中心”。
- “新建/关联公司商品与供给”Dialog 提供“关联已有公司 SKU / 新建公司商品并关联供给”两个分支；已有分支展示匹配
  证据、当前有效供应商数量、目标 SKU 状态与销售可见价，但不修改目标 SKU 的价格。
- 新建分支固定当前供应商目录 SKU，把同名语义字段自动预填为公司草稿并允许采购修改；
  独立 `product_kind`、销售可见价与市场价均必填。公司对象仍遵守 W14 商品类型、字典、
  规格、主图和版本校验。
- 从 W14 携带 `skuId` 进入时，只显示映射到该 SKU 的供给；即使尚无供给，也必须保留 W14 的 SKU 上下文和「添加供应商并登记成本」对话框入口。

### 5.2 变化与异常队列

路由：`/procurement/supplier-catalog?mode=queue`

- API 同步和后续 Excel 重导产生的新增、变化、停供、错误可进入队列。
- 停供、不可供、库存为零或新鲜度超时仍按既有安全规则暂停相关发布。
- 正常目录浏览不依赖任务；只有需要人工领取/终结的异常才使用 W02 的租约和幂等信封。
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
| 映射与公司商品 | 公司商品/SKU 新建草稿、销售可见价、市场价 | 映射状态、映射历史、当前 SKU 价格摘要 | 主动作：**新建/关联公司商品与供给**；可关联已有公司 SKU，或自动预填并新建公司商品/SKU；不展示来源版本差异、供给版本时间线、发布影响 |

页头主动作（采购）：填写检查、保存来源版本、新建/关联公司商品与供给、返回。

成本区仅采购及明确授权的财务角色可见；销售、运营、管理员和技术角色返回掩码。非采购角色只读。

**新建公司商品/SKU 分支**：使用精确 `supplier_catalog_sku_id` 与来源修订作为预填上下文；
`supplierProductId` 只可辅助恢复 SPU 页面，不能替代 SKU 级身份。所有相同字段自动预填，
采购可二次修改；公司 `product_kind` 是独立必填稳定属性，来源类型只能预填、不能由分类
推导；分类必须与采购最终确认的商品类型兼容。分类、品牌、单位必须解析为 W14 稳定字典
身份，未归档媒体不得直接成为公司长期媒体。销售可见价与市场价必填。

**明确不在中心页展示**（仍可在队列/其他工作面出现）：来源版本差异、供给版本修订时间线、发布影响与恢复入口。

### 5.4 W14 商品编辑页

每个公司 SKU 行展示：

- 销售可见价（公司 `sku_revision` 字段）；
- 供给列只显示**供应商数量**，鼠标悬停弹出面板：供应商列表（暂无时给出空态）、「添加供应商」、「查看全部供给」；
- 库存列只有「查看库存」链接，不展示独立台账徽标。

「添加供应商并登记成本」是**最小对话框**（区别于手工录入全页表单）：固定当前 `sku_id`，名称/商品类型/规格/分类/品牌/单位/条码/图文从公司商品资料正向复用，形成供应商商品来源快照；要求填写供应商、供应商 SKU 编码、一件代发供给价（含税运）、集采供给价（含税）、集采起订量、进项税率、供给区域和生效日期，不收集供给方式。税率、区域和生效日期只有可靠版本化来源时自动预填并显示来源，采购可修改；无可靠来源时空白必填，禁止静默使用 `0.13`、“全国”或浏览器当天。两项供给价分别写入同一供给修订，不再自动择一生成单一采购确认成本。同一业务事务内创建或关联供应商商品及供应商目录 SKU、精确的 `supplier_catalog_sku_id → sku_id` 映射和供给修订；该正向路径不创建独立商品池条目或修订，也不修改当前 `sku_revision` 的销售可见价或市场价。W21 携带 `skuContext` 进入列表页时，页头「添加供应商并登记成本」按钮使用同一对话框。

公司 SKU 可由 W14 独立创建，也可由 W21 反向“新建/关联公司商品与供给”复合命令创建；两种路径都必须由服务端
分配稳定 `sku_id`。正向添加供应商与反向创建最终都只能引用该稳定 ID，不得由前端临时生成。

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

### 6.2 新建/关联公司商品与供给

```text
CreateOrLinkCompanySkuFromSupplierCatalog {
  supplier_catalog_sku_id
  expected_supplier_catalog_sku_revision_id
  target:
    | { kind: EXISTING_COMPANY_SKU, company_sku_id, expected_company_sku_revision_id }
    | {
        kind: CREATE_COMPANY_PRODUCT_AND_SKU
        company_product {
          name
          description?
          product_kind: PHYSICAL | VIRTUAL | OFFLINE_SERVICE | VOUCHER
          category_id
          brand_id
          carousel_file_asset_ids[]?
          detail_file_asset_ids[]?
        }
        company_sku {
          sku_no?                 // 仅业务编码；系统生成，可由采购覆盖，不参与身份恢复
          base_unit_id
          attribute_values[]      // 服务端派生 specification_signature
          barcode?
          main_image_file_asset_id
          sales_visible_price_gross // 新建分支必填，写入 sku_revision
          market_price_gross        // 新建分支必填，写入 sku_revision
        }
        change_reason
      }
  dropship_supply_price_gross
  bulk_supply_price_gross
  bulk_minimum_order_quantity
  input_tax_rate
  fees?
  supply_region
  valid_from / valid_to?
  prefill_source_refs? {
    input_tax_rate?: {
      kind: SUPPLIER_COMMERCIAL_PROFILE_REVISION | TAX_POLICY_REVISION
      revision_id
    }
    supply_region?: {
      kind: SUPPLIER_CAPABILITY_REVISION | SUPPLY_REGION_POLICY_REVISION
      revision_id
    }
    valid_from?: {
      kind: SERVER_BUSINESS_DATE
      business_date
      timezone
      calendar_version?
    }
  }
  idempotency_key
}
```

`prefill_source_refs` 按字段记录自动预填依据：只有字段确实由可靠来源预填时，对应引用才
允许出现且必须提交并进入审计；采购手工填写而未发生预填时不得伪造来源引用。税率可以引用
供应商商业资料修订或税务策略修订，区域可以引用供应商能力修订或供给区域策略修订，生效
日期只能引用服务端业务日期快照。写入供给修订的是采购最终确认值，来源引用不替代最终值。

新建分支中，名称、描述、来源商品类型、分类、品牌、单位、规格、条码、主图、轮播/详情图等两侧语义相同
字段全部由当前供应商商品/SKU 修订自动预填，采购可在提交前二次修改。`product_kind` 必须
由采购最终确认并写入公司商品稳定身份；来源没有可靠类型时保持空白必填，不得从分类推导。
分类必须允许最终确认的类型。字典项无法精确匹配、
媒体未归档或启用 SKU 缺主图时必须补齐；不得伪造字典 ID 或长期媒体地址。新建公司的销售可见价和
市场价均必填，写入新建 `sku_revision`，且不得由来源底价、正式供给价或彼此自动推导。关联已有公司 SKU 时不传也不修改这两项价格。

新建分支不得接收客户端提供的 `company_sku_id`，也不得按来源 SKU 编码、拟定的公司
`sku_no`、表格位置或“只有一个规格”猜测并复用既有公司 SKU 身份；服务端按规范化属性
代码/值代码派生新 `specification_signature` 并分配新 `company_sku_id`。已有分支只引用已选定的
`company_sku_id + expected_company_sku_revision_id`，且不能借关联命令改变其规格签名。

`CREATE_COMPANY_PRODUCT_AND_SKU` 是单一数据库事务：创建公司 product/SKU 及其包含销售可见价和市场价的 `sku_revision`、精确
SKU 映射、双价 offering 修订、审计、幂等结果与 outbox 必须一起
提交；外部媒体归档须在事务前完成或在提交后由 outbox 处理，不能用 Saga 留下业务半状态。

单项新建/关联公司商品与供给的正式粒度始终是 `supplier_catalog_sku_id`。供应商 SPU 页面可以多选 SKU，
但批量提交必须拆成 SKU 级 `items[]`；每项都显式携带来源 SKU 与目标公司 SKU，独立执行
原子校验和幂等控制，不得生成一条 SPU 级映射或隐式处理未选择的兄弟 SKU。

```text
CreateOrLinkCompanySkusFromSupplierCatalogBatch {
  items[]: CreateOrLinkCompanySkuFromSupplierCatalog
}

CreateOrLinkCompanySkusFromSupplierCatalogBatchResult {
  items[] {
    supplier_catalog_sku_id
    idempotency_key
    status: SUCCEEDED | FAILED
    result?: SupplierCatalogWriteResult
    error?: { code, message, retryable }
  }
}
```

批量仅是传输层分组，不是 SPU 级业务命令，也不承诺跨 SKU 的整体原子性。每个 `items[]`
元素按自己的幂等键独立成功或失败；重试失败项不得重复执行已成功项。

成功结果至少返回 `supplier_product_id`、`supplier_catalog_sku_id`、`company_product_id`、
`company_sku_id`、`company_sku_revision_id`、`supplier_product_mapping_id`、`supplier_offering_revision_id`、
当前有效供应商数量、审计引用和记录时间。单项执行中出现并发冲突、来源修订已变化或价格无效时该项整体失败，不允许只完成映射、供给或新建公司 SKU 中的一部分。

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
3. 采购可把供应商 SKU 关联已有公司 SKU，也可在无同款时选择“新建公司商品/SKU并关联供给”；
   新建分支同字段自动预填、允许二次修改，独立 `product_kind`、销售可见价与市场价均必填；
   `product_kind` 不得由分类派生，分类必须与最终确认类型兼容。
4. 一个公司 SKU 能维护至少两家供应商，分别拥有不同成本、MOQ、区域和有效期。
5. W14 SKU 供给列显示供应商数量，悬停面板可新增一条带成本的手工供给或查看全部供给。
6. 新建/关联公司商品与供给的已有分支同时形成映射和供给修订，且不修改目标 `sku_revision`；新建分支还原子
   形成公司商品/SKU及修订。失败时没有半完成状态。
7. 采购成本与销售可见价明确分栏，销售和运营请求中成本值为掩码或根本不返回。
8. 销售查询和导出只依赖符合资格的公司 SKU（公司商品池查询视图）；未关联公司 SKU 的供应商商品不可被下单。
9. 二期启用后，API 停供/不可供触发安全暂停，且不改写历史订单快照；一期不开启该入口。
10. 所有写命令有幂等键、期望修订、审计记录和明确的冲突/未知结果处理。
11. 来源无图时仍可进入供应商商品库；公司商品主图规则仅在 W14 建品/修订时校验。
12. 第二供应商关联已有公司 SKU 时，不修改该 SKU 修订；销售只看到一份公司商品和一个销售可见价，采购看到逐供应商成本与条件。
13. 供应商商品中心与 W14 商品详情分区同构（基础 / 图文 / SKU·规格 + 供应商独有来源供给）；手工新建使用全页表单而非简陋对话框。
14. 分类、品牌、单位使用与公司商品相同的字典控件；规格使用与公司商品相同的规格维度编辑器。
15. 多规格生成多 SKU 行；每行维护 1:1 主图与价格字段：一件代发底价（含税运）、集采底价（含税）、集采起订量，以及可供数量/状态。
16. 系统不存在供给方式字段；每条正式供给默认同时支持一件代发与集采，并分别保存两项供给价。供应商商品目录**不包含**：统一含税报价、进项税率、运费、其他费用、可供区域、预计发货、售后说明、商品能力（关联供给确认另有字段，不进目录主档）。
17. 中心页不展示来源版本差异、供给版本时间线、发布影响。
18. 供应商商品中心保存只形成来源修订（`ReviseSupplierCatalogProduct`，含 `skus[]`），带期望修订号与幂等键。
19. 同一供应商 SPU 下的多个供应商 SKU 可以分别映射不同公司 SKU；只选择其中部分 SKU
    关联公司 SKU 时，未选择的兄弟 SKU 保持未映射，且任何一项都不能覆盖另一项的映射。
20. 反向新建分支固定精确供应商目录 SKU 与来源修订；同字段自动预填但不自动提交，采购
    可修改。独立 `product_kind`、销售可见价、市场价、两项供给价及集采起订量缺任一项均
    fail-closed；商品类型不能由分类派生，分类必须与最终确认类型兼容。
21. 进项税率、供给区域和生效日期均在 Dialog 可见且可修改；有可靠来源时展示预填值及
    来源版本，无来源时空白必填。系统不存在 `0.13`、“全国”或浏览器当天的静默正式默认。
