# Phase 3：供应商商品库、供给与公司商品池

## 1. 分支与隔离

| 项目 | 约定 |
| --- | --- |
| 分支名 | `codex/backend-p1-03-catalog-product-pool` |
| 基线 | 与 Phase 1～10 相同的冻结 `BACKEND_PHASE_BASE_SHA` |
| 实现语言 | 读取冻结基线中的统一后端语言/版本决定；缺失时停止，不自行选栈 |
| 独占目录 | `backend/modules/catalog-product-pool/**` |
| 编译要求 | 不要求根工程编译；要求业务模型、命令、端口、测试向量可审查 |
| 禁止修改 | 共享入口、全局 OpenAPI、物理迁移、其他 phase 目录和前端 |

## 2. 目标与唯一写者

本 phase 统一实现第一期的：

- 分类、品牌、单位、规格字典；
- 公司 `product` / `sku` 稳定身份和不可变修订；
- 供应商商品 SPU/SKU、来源修订、映射和导入批次语义；
- `supplier_offering` / `supplier_offering_revision` 多供应商供给；
- `product_pool_entry` / revision 和销售可见价；
- W14 商品/SKU/商品池与 W21 供应商商品库所需查询和命令。

对象所有权必须严格分开：

- 公司 SKU 只保存稳定商品资料，不保存默认供应商、采购成本、履约责任、进项税率、
  代发/起批等供给字段。
- 供应商商品库保存来源身份和来源报价，不直接成为公司 SKU。
- 逐供应商的一件代发供给价、集采供给价、集采起订量、税率和有效期只属于供给修订；不设置供给方式字段，每条供给默认同时支持一件代发与集采。
- 公司商品池保存销售可见价；销售查询/搜索/导出只读商品池，绝不返回供应商成本。

依据：`erp-phase-1.md` §4.4、§5.1、§5.3；`erp-data-model.md` §6.3、§6.14、§10；
W14、W21。

## 3. 独立目录结构

```text
backend/modules/catalog-product-pool/
  domain/{dictionary,company_product,supplier_catalog,sku_mapping,offering,product_pool}/
  application/{commands,queries}/
  ports/
  contracts/
  persistence-spec/
  fixtures/
  tests/
  DECISIONS.md
```

Phase 2 的供应商和 Phase 5 的采购都只通过稳定引用端口交互，不导入它们的实现。

## 4. 核心命令和查询

### 4.1 公司商品与商品池

- `CreateCompanyProduct`、`AppendCompanyProductRevision`；
- `CreateCompanySku`、`AppendCompanySkuRevision`；
- `CreateProductPoolEntry`、`AppendProductPoolRevision`、`SetProductPoolStatus`。

启用商品池前必须确认 SKU 有效、销售可见价有效，并存在至少一条业务日有效供给；
Phase 10 负责把该跨对象 guard 绑定到真实事务。

### 4.2 供应商商品与供给

- `ApplySupplierCatalogImportIntent`：第一期仅 `MANUAL` / `EXCEL`；
- `AppendSupplierCatalogProductRevision`、`AppendSupplierCatalogSkuRevision`；
- `MapSupplierSkuToCompanySku`；
- `CreateSupplierOffering`、`AppendSupplierOfferingRevision`、`EndSupplierOffering`；
- `PromoteMappedSupplierSkuToPool`：按供应商 SKU 粒度，不接受仅 SPU 身份。

前端当前存在三种创建能力；本次确认新增供应商 SKU 反向建公司 SKU 的入池分支。本 phase
按以下边界实现：

- W14 `/master-data/products/new` 继续独立创建公司商品及一个或多个公司 SKU；
- W21 手工录入独立创建供应商商品及一个或多个供应商 SKU，不建立公司映射；
- W14 可固定公司 `sku_id` 正向发起 W21 创建供应商商品/SKU，并在同一业务动作中建立精确
  映射、供给与必要的商品池修订；
- W21 入池可选择已有公司 SKU；没有同款时调用
  `CreateCompanySkuAndPromoteSupplierCatalogSku`。命令固定一个 `supplierCatalogSkuId`，以
  来源同字段预填公司草稿并允许采购修改；独立 `productKind`、销售可见价和市场价必填。公司商品/SKU、映射、
  双价供给修订、商品池修订、审计、幂等结果和 outbox 必须在单一数据库事务提交；
- W14 正向创建与 W21 反向创建都必须显式提交独立必填 `productKind`。它写入
  `product.product_kind` 后永久不可变，决定商品业务作用；`categoryId` 只参与兼容性校验，
  不得用于派生或覆盖 `productKind`。反向创建可由可靠 `sourceProductKind` 预填，但采购必须
  最终确认；来源缺失时空白必填；
- 页面导航中的 `supplierProductId`、SPU 或其他上下文不得替代正式
  `supplier_catalog_sku_id → sku_id` 映射，也不得因来源变化自动覆盖另一侧修订。

### 4.3 查询与权限投影

- W14 公司商品、SKU、商品池及供给摘要；
- W21 供应商商品、映射、逐供应商供给和修订时间线；
- 销售端 `SalesCatalogQuery`：只返回启用、业务日/区域/履约方式匹配的商品池修订；
- 采购端 `ProcurementOfferingQuery`：返回授权的供应商供给和成本事实；
- 销售导出使用专门 DTO，类型上不包含 supplier、cost、input tax、MOQ 字段。

## 5. 领域不变量

- Excel、手工和未来 API 共用一套供应商 SPU/SKU 身份；API 连接不是非 API 来源的父对象。
- W18 只编排批次、manifest 和责任确认；供应商目录行的领域校验、修订和应用仍由本
  phase 唯一负责，Phase 8 只能提交强类型导入意图。
- `supplier_catalog_intake_batch/item` 是本域的目录摄取事实；Phase 8 的
  `legacy_import_batch/row` 是开账治理记录。两者通过稳定批次引用关联，不能各自重复写
  同一供应商目录/供给事实。
- 同一供应商 SKU 同一时点只映射一个公司 SKU；一个公司 SKU 可以有多家供应商供给。
- 供应商商品修订、映射、供给修订和商品池修订是不同事实，不能互相覆盖。
- 来源代发/集采报价、采购确认后的两项供给价、销售可见价和未来商城发布价是不同价格事实。
- 供给有效期不得重叠；采购提交按业务日期重验供给修订、能力、资质和供应商状态。
- 商品池启用不能凭 UI 摘要决定，必须在提交事务内重验有效供给。
- 规格身份和基础单位被正式单据引用后不能就地改变；以新 SKU 或纠正修订处理。
- 销售查询、缓存、索引、导出和错误信息都不得包含采购成本。

## 6. 必须补齐的契约孔洞

以下问题属于当前文档冲突，不能由本 phase 静默选择：

1. `erp-phase-1.md` 明确把供应商商品库、多供应商供给列入一期；
   `erp-data-model.md` §5.4 却把相关表列为二期扩展。第一期最小闭环按业务文档实现
   `MANUAL/EXCEL → supplier SKU → mapping → offering → pool`，但 Phase 10 必须先修正
   阶段启用目录，禁止出现两套物理表。
2. 一期只登记 API 能力，不建设 API 连接/同步；W21 文档混有 API 来源入口。
   本 phase 保留 `source_type=API` 的未来兼容语义但运行时拒绝 API intake，W20 属第二期。
3. W21 修订命令出现 `input_tax_rate`，而统一模型不允许供应商目录保存进项税率；该字段必须
   移到供给修订，不得进入目录修订。
4. **[已确认，2026-08-04] 提升入池的唯一正式粒度是 supplier catalog SKU**：单项命令
   必须显式携带 `supplierCatalogSkuId + targetCompanySkuId`；批量操作只接受由这些单项组成的
   `items[]`。supplier product/SPU 只作为页面容器和批量选择范围，不得形成 SPU 级映射，
   也不得隐式映射未选择的兄弟 SKU。
5. `procurement_confirmation_line` 缺少精确 `supplier_offering_revision_id`；本 phase 先提供
   `OfferingRevisionRef`，Phase 10 补真实 FK、同供应商/SKU和业务日有效性约束。
6. 供给不设置 `supply_mode`；入池命令必须同时提交一件代发供给价、集采供给价和集采
   起订量，缺任一项均 fail-closed，不能把两项价格折叠成单一 `confirmed_cost_gross`。
7. **[已确认，2026-08-04] 供给条件预填**：进项税率、供给区域和生效日期必须显式展示并
   允许采购修改；仅当存在供应商开票资料/税务策略、供应商能力/供给策略或服务端业务日期
   时自动预填，并携带策略修订或业务日期快照。无可靠来源时空白必填；禁止 `0.13`、“全国”、
   浏览器当天或单一确认成本成为静默默认。

## 7. 测试要求

1. 同一来源批次/行幂等，A→B→A 修订仍保留历史。
2. 供应商 SKU 并发映射、一个有效映射限制和多供应商供给。
3. 供给生效区间重叠、未来生效、失效和业务日重验。
4. 商品池启用时无有效供给、无价格、SKU 停用均拒绝。
5. 销售 DTO、搜索索引和导出结构无成本/供应商敏感字段。
6. 采购端在权限不足时只返回可销售事实，不通过错误文案泄露成本。
7. 同一 SPU 的多个供应商 SKU 可分别映射不同公司 SKU；部分选择不得影响未选 SKU，
   任一单项缺供给价/集采起订量或引用错误 offering revision 时，该单项 fail-closed。
8. API 来源在一期能力 gate 下拒绝，MANUAL/EXCEL 正常进入同一模型。
9. 税率、区域、生效日期无可靠来源时缺失全部 fail-closed；有来源时断言策略版本/服务端
   业务日期快照被审计，采购修改后的最终值而非建议值进入供给修订。
10. W14 正向创建和 W21 反向创建缺少 `productKind` 都必须拒绝；可靠
    `sourceProductKind` 只能预填，采购最终确认值写入稳定 `product.product_kind`；分类与
    类型不兼容时拒绝，创建后任何修改 `productKind` 的命令都拒绝。

## 8. 完成标准

- W14 与 W21 的对象归属、价格事实和权限边界由类型与测试共同证明。
- 仅写独占目录，不创建全局路由、物理 DDL 或跨 phase 外键。
- API 供应商连接、商城商品发布等二期能力没有被一期入口误启用。
- 向 Phase 10 交付逻辑 schema、端口、DTO、错误码、跨对象 guard 和六项文档冲突清单。
