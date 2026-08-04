/**
 * 商品中心 SKU 与商品供给 mock 的身份完整性测试。
 * Run: node scripts/test-product-supply-links.mjs
 */
import { spawnSync } from "node:child_process"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const scratch = process.env.SCRATCH

const runner = `
import {
  createSupplierCatalogItem,
  fetchCompanySkuOptions,
  fetchSupplierCatalogCenter,
  fetchSupplierCatalogQueue,
  promoteSupplierProductToPool,
} from "../features/supplier-catalog/api.ts"
import { createMasterDataObject } from "../features/master-data/api.ts"
import { MASTER_DATA_CENTER_SEEDS } from "../features/master-data/data.ts"
import { SUPPLIER_CATALOG_SEED } from "../mock/supplier-catalog.ts"

let failed = 0
function assert(cond: boolean, msg: string) {
  if (!cond) {
    console.error("FAIL:", msg)
    failed += 1
  } else {
    console.log("OK:", msg)
  }
}

const enabledSkus = Object.values(MASTER_DATA_CENTER_SEEDS)
  .filter((center) => center.resource === "products")
  .flatMap((center) =>
    (center.productDetail?.skus ?? [])
      .filter((sku) => sku.lifecycleStatus === "ENABLED" && sku.skuId)
      .map((sku) => ({
        productName: center.name,
        productRevisionId: center.currentRevision.revisionId,
        skuId: sku.skuId!,
        skuCode: sku.skuNo,
        specification: sku.specLabel,
      }))
  )

for (const sku of enabledSkus) {
  const activeMappings = SUPPLIER_CATALOG_SEED.filter(
    (item) =>
      item.mapping?.mappingStatus === "ACTIVE" &&
      item.mapping.skuId === sku.skuId
  )
  assert(activeMappings.length > 0, \`\${sku.skuCode} has a supply relationship\`)
  assert(
    activeMappings.every(
      (item) =>
        item.mapping?.skuCode === sku.skuCode &&
        item.mapping.specification === sku.specification &&
        item.mapping.skuRevisionId === \`\${sku.productRevisionId}:\${sku.skuId}\`
    ),
    \`\${sku.skuCode} relationship uses the W14 SKU identity\`
  )
}

const teaView = await fetchSupplierCatalogQueue({
  mode: "list",
  skuId: "sku_tea_04",
  changeType: "all",
})
assert(
  teaView.skuContext?.productName === "礼盒红茶" &&
    teaView.skuContext.skuCode === "SKU-TEA-250-TIN",
  "list read model carries SKU context from W14"
)
assert(teaView.items.length === 2, "tea 250g tin shows two supplier relationships")
assert(
  teaView.items.some(
    (item) => item.offering?.currentRevision?.status === "ACTIVE"
  ) &&
    teaView.items.some(
      (item) => item.offering?.currentRevision?.status === "STOPPED"
    ),
  "one SKU demonstrates active and stopped supplier relationships"
)

assert(
  SUPPLIER_CATALOG_SEED.every(
    (item) =>
      item.supplierProduct.source.type === "API" &&
      Boolean(item.supplierProduct.source.connection)
  ),
  "API seeds expose connection only as source metadata"
)

const excelResult = await createSupplierCatalogItem({
  sourceType: "EXCEL",
  supplierId: "supplier_excel_test",
  supplierName: "Excel 测试供应商",
  supplierSpuCode: "SPU-EXCEL-01",
  supplierSkuCode: "SKU-EXCEL-01",
  name: "Excel 导入测试商品",
  description: "Excel 来源商品描述",
  specification: "250g",
  category: "茶叶",
  sourceBaseUnit: "盒",
  barcode: "6900000000001",
  attributes: [{ name: "净含量", value: "250g" }],
  media: [
    {
      usage: "SKU_MAIN",
      fileName: "excel-tea-main.webp",
      sortOrder: 0,
      fileAssetId: "asset_excel_tea_main",
      archiveStatus: "ARCHIVED",
    },
  ],
  sourceQuotedPriceGross: "42.00",
  inputTaxRate: "0.13",
  supplyRegion: ["全国"],
  sourceReference: "supplier-catalog-test.xlsx",
  minimumOrderQuantity: "1",
  supplyMode: ["BULK"],
  validFrom: "2026-08-02",
  idempotencyKey: "test-excel-intake-1",
})
const excelCenterBefore = await fetchSupplierCatalogCenter({
  supplierProductId: excelResult.supplierProductId,
})
assert(
  excelCenterBefore?.item.supplierProduct.source.type === "EXCEL" &&
    !excelCenterBefore.item.supplierProduct.source.connection &&
    !excelCenterBefore.item.poolEntry &&
    excelCenterBefore.item.supplierProduct.currentRevision.media?.[0]?.usage === "SKU_MAIN",
  "Excel intake creates a supplier SKU without a fake API connection or automatic pool entry"
)

const existingTeaPool = SUPPLIER_CATALOG_SEED.find(
  (item) => item.mapping?.skuId === "sku_tea_04" && item.poolEntry
)?.poolEntry
const promoteResult = await promoteSupplierProductToPool({
  supplierProductId: excelResult.supplierProductId,
  targetSkuId: "sku_tea_04",
  targetSkuCode: "SKU-TEA-250-TIN",
  targetSkuName: "礼盒红茶",
  specification: "250g 罐装",
  baseUnit: "盒",
  confirmedCostGross: "40.00",
  inputTaxRate: "0.13",
  minimumOrderQuantity: "2",
  supplyMode: ["BULK"],
  supplyRegion: ["全国"],
  validFrom: "2026-08-02",
  poolPriceAction: "KEEP_EXISTING",
  expectedSourceRevisionNo: 1,
  expectedPoolEntryRevisionId: existingTeaPool?.poolEntryRevisionId,
  idempotencyKey: "test-excel-promote-1",
})
const excelCenterAfter = await fetchSupplierCatalogCenter({
  supplierProductId: excelResult.supplierProductId,
})
assert(
    excelCenterAfter?.item.mapping?.skuId === "sku_tea_04" &&
    excelCenterAfter.item.offering?.currentRevision?.supplyPriceGross === "40.00" &&
    excelCenterAfter.item.poolEntry?.salesVisiblePriceGross === existingTeaPool?.salesVisiblePriceGross &&
    excelCenterAfter.item.poolEntry?.poolEntryRevisionId === existingTeaPool?.poolEntryRevisionId &&
    promoteResult.poolEntryChange === "UNCHANGED" &&
    (promoteResult.activeSupplierCount ?? 0) >= 2,
  "second supplier adds mapping and cost while keeping the singleton pool revision unchanged"
)

const operationsView = await fetchSupplierCatalogCenter({
  supplierProductId: excelResult.supplierProductId,
  demoRole: "operations",
})
assert(
  operationsView?.costFieldVisibility === "masked" &&
    operationsView.item.offering?.currentRevision?.supplyPriceGross === "***" &&
    operationsView.item.poolEntry?.salesVisiblePriceGross === existingTeaPool?.salesVisiblePriceGross,
  "operations sees the pool price but not procurement cost"
)

const newCompanyProduct = await createMasterDataObject({
  resource: "products",
  name: "首次建品测试礼盒",
  effectiveFrom: "2026-08-03",
  changeReason: "从供应商来源资料创建",
  fields: {
    description: "公司审核后的商品描述",
    baseUnitId: "uom_box",
    baseUnitCode: "BOX",
    baseUnit: "盒",
    categoryId: "md_cat_snack",
    category: "零食",
    brandId: "md_brand_corp",
    brand: "企业优选",
    carouselImages: ["source-carousel.webp"],
    detailImages: ["source-detail.webp"],
    specs: [],
    skus: [{
      skuNo: "SKU-FIRST-SOURCE-01",
      attributeValues: [],
      specLabel: "默认规格",
      mainImage: "source-main.webp",
      lifecycleStatus: "ENABLED",
    }],
  },
  idempotencyKey: "test-create-company-from-source-1",
})
assert(newCompanyProduct.outcome === "succeeded", "source-prefilled company product can be saved after required content is complete")
const newCompanySku = newCompanyProduct.outcome === "succeeded"
  ? (await fetchCompanySkuOptions()).find((sku) => sku.productId === newCompanyProduct.stableId)
  : undefined
assert(Boolean(newCompanySku && !newCompanySku.poolEntry), "new company SKU is immediately available to W21 but is not yet in the company pool")

const firstSupplierResult = await createSupplierCatalogItem({
  sourceType: "MANUAL",
  supplierId: "supplier_first_source",
  supplierName: "首次建品供应商",
  supplierSkuCode: "SUP-FIRST-01",
  name: "首次建品测试礼盒",
  specification: "默认规格",
  category: "零食",
  sourceBaseUnit: "盒",
  attributes: [],
  media: [],
  sourceQuotedPriceGross: "60.00",
  inputTaxRate: "0.13",
  supplyRegion: ["全国"],
  minimumOrderQuantity: "1",
  supplyMode: ["BULK"],
  validFrom: "2026-08-03",
  idempotencyKey: "test-first-source-intake-1",
})
const firstPoolResult = newCompanySku
  ? await promoteSupplierProductToPool({
      supplierProductId: firstSupplierResult.supplierProductId,
      targetSkuId: newCompanySku.skuId,
      targetSkuCode: newCompanySku.skuCode,
      targetSkuName: newCompanySku.skuName,
      specification: newCompanySku.specification,
      baseUnit: newCompanySku.baseUnit,
      confirmedCostGross: "58.00",
      inputTaxRate: "0.13",
      minimumOrderQuantity: "1",
      supplyMode: ["BULK"],
      supplyRegion: ["全国"],
      validFrom: "2026-08-03",
      salesVisiblePriceGross: "88.00",
      poolPriceAction: "SET_PRICE",
      expectedSourceRevisionNo: 1,
      idempotencyKey: "test-first-source-promote-1",
    })
  : undefined
assert(
  firstPoolResult?.poolEntryChange === "CREATED",
  "first supplier promotion creates the singleton company pool entry"
)

if (failed) process.exit(1)
console.log("All product supply relationship checks passed")
`

const tmp = path.join(root, "scripts", ".run-product-supply-links.mts")
fs.writeFileSync(tmp, runner)

const result = spawnSync(
  "npx",
  ["--yes", "tsx", path.join("scripts", ".run-product-supply-links.mts")],
  { cwd: root, encoding: "utf8", env: process.env }
)

if (scratch) {
  fs.mkdirSync(scratch, { recursive: true })
  fs.writeFileSync(
    path.join(scratch, "product-supply-links-test.log"),
    [
      "exit=" + result.status,
      "stdout:",
      result.stdout || "",
      "stderr:",
      result.stderr || "",
    ].join("\n")
  )
}

try {
  fs.unlinkSync(tmp)
} catch {
  // ignore
}

console.log(result.stdout || result.stderr)
process.exit(result.status ?? 1)
