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
  fetchSupplierCatalogCenter,
  fetchSupplierCatalogQueue,
  promoteSupplierProductToPool,
} from "../features/supplier-catalog/api.ts"
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
  specification: "250g",
  category: "茶叶",
  sourceQuotedPriceGross: "42.00",
  inputTaxRate: "0.13",
  supplyRegion: ["全国"],
  sourceReference: "supplier-catalog-test.xlsx",
  minimumOrderQuantity: "1",
  supplyMode: "BULK",
  validFrom: "2026-08-02",
  idempotencyKey: "test-excel-intake-1",
})
const excelCenterBefore = await fetchSupplierCatalogCenter({
  supplierProductId: excelResult.supplierProductId,
})
assert(
  excelCenterBefore?.item.supplierProduct.source.type === "EXCEL" &&
    !excelCenterBefore.item.supplierProduct.source.connection &&
    !excelCenterBefore.item.poolEntry,
  "Excel intake creates a supplier SKU without a fake API connection or automatic pool entry"
)

await promoteSupplierProductToPool({
  supplierProductId: excelResult.supplierProductId,
  targetSkuId: "sku_tea_04",
  targetSkuCode: "SKU-TEA-250-TIN",
  targetSkuName: "礼盒红茶",
  specification: "250g 罐装",
  baseUnit: "盒",
  confirmedCostGross: "40.00",
  inputTaxRate: "0.13",
  minimumOrderQuantity: "2",
  supplyMode: "BULK",
  supplyRegion: ["全国"],
  validFrom: "2026-08-02",
  salesVisiblePrice: "68.00",
  idempotencyKey: "test-excel-promote-1",
})
const excelCenterAfter = await fetchSupplierCatalogCenter({
  supplierProductId: excelResult.supplierProductId,
})
assert(
  excelCenterAfter?.item.mapping?.skuId === "sku_tea_04" &&
    excelCenterAfter.item.offering?.currentRevision?.supplyPriceGross === "40.00" &&
    excelCenterAfter.item.poolEntry?.salesVisiblePrice === "68.00",
  "promotion atomically records company SKU mapping, confirmed supplier cost, and sales-visible pool price"
)

const operationsView = await fetchSupplierCatalogCenter({
  supplierProductId: excelResult.supplierProductId,
  demoRole: "operations",
})
assert(
  operationsView?.costFieldVisibility === "masked" &&
    operationsView.item.offering?.currentRevision?.supplyPriceGross === "***" &&
    operationsView.item.poolEntry?.salesVisiblePrice === "68.00",
  "operations sees the pool price but not procurement cost"
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
