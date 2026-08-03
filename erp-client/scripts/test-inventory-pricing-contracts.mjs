/**
 * Behavioral contract checks for the W05/W08/W10/W14/W21/W22 alignment pass.
 * Run: npm run test:inventory-pricing-contracts
 */
import { spawnSync } from "node:child_process"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const runnerPath = path.join(root, "scripts", ".run-inventory-pricing-contracts.mts")

const runner = String.raw`
import fs from "node:fs"
import path from "node:path"
import {
  multiplyFixed,
  splitGrossByFractionRate,
  splitGrossByPercentRate,
  sumFixed,
} from "../lib/fixed-decimal.ts"
import { fetchInventoryList } from "../features/inventory/api.ts"
import { fetchSupplierCatalogQueue } from "../features/supplier-catalog/api.ts"
import { getW14Center } from "../features/master-data/session.ts"
import { SUPPLIER_CATALOG_SEED } from "../mock/supplier-catalog.ts"
import {
  getW22PublicationOverride,
  submitW22PublishRevision,
  triggerW22SystemSafetyPause,
} from "../mock/product-publications-session.ts"

let failed = 0
function assert(condition: unknown, message: string) {
  if (condition) console.log("OK:", message)
  else {
    console.error("FAIL:", message)
    failed += 1
  }
}

assert(
  multiplyFixed("12.3456", "0.333333", {
    leftMaxScale: 4,
    rightMaxScale: 6,
    outputScale: 2,
  }) === "4.12",
  "decimal multiplication rounds half-up without binary float"
)
assert(
  JSON.stringify(splitGrossByFractionRate("113.00", "0.13")) ===
    JSON.stringify({ gross: "113.00", net: "100.00", tax: "13.00" }),
  "fractional tax split keeps gross = net + tax"
)
assert(
  JSON.stringify(splitGrossByPercentRate("113.00", "13")) ===
    JSON.stringify({ gross: "113.00", net: "100.00", tax: "13.00" }),
  "percentage tax split uses the same line rounding rule"
)
assert(
  sumFixed(["0.10", "0.20"], { maxScale: 2, outputScale: 2 }) === "0.30",
  "decimal sum has no 0.30000000000000004 drift"
)

const firstPage = await fetchInventoryList({
  view: "balance",
  availability: "all",
  pageSize: 2,
  sort: ["warehouseCode:asc", "skuCode:asc"],
})
assert(firstPage.balances.length === 2, "W10 mock API returns only the requested page")
assert(firstPage.movements.length === 0, "W10 mock API does not leak another view's full rows")
assert(Boolean(firstPage.nextCursor), "W10 response contains an opaque next cursor")
const secondPage = await fetchInventoryList({
  view: "balance",
  availability: "all",
  cursor: firstPage.nextCursor,
  pageSize: 2,
  sort: ["warehouseCode:asc", "skuCode:asc"],
})
assert(
  firstPage.balances.every(
    (left) => secondPage.balances.every((right) => left.balanceId !== right.balanceId)
  ),
  "W10 cursor pages do not duplicate balance identities"
)
const receiptMovements = await fetchInventoryList({
  view: "movement",
  movementType: ["PURCHASE_RECEIPT"],
  occurredFrom: "2026-07-01",
  occurredTo: "2026-08-02",
  pageSize: 20,
  sort: ["occurredAt:desc", "movementId:desc"],
})
assert(
  receiptMovements.movements.length > 0 &&
    receiptMovements.movements.every((row) => row.movementType === "PURCHASE_RECEIPT"),
  "W10 movement type and date filters execute in the mock API"
)

const product = getW14Center("products", "prd_1")
assert(Boolean(product?.productDetail?.baseUnitId), "W14 product stores a stable base-unit ID")
assert(Boolean(product?.productDetail?.categoryId), "W14 product stores a stable category ID")
assert(
  Boolean(product?.revisionTimeline.every((entry) => entry.productSnapshot?.skus.length)),
  "W14 every product history entry carries a complete immutable snapshot"
)
const forbiddenW14SupplyKeys = [
  "fulfillmentResponsibility",
  "inputTaxRate",
  "dropshipCostPrice",
  "dropshipFloorPrice",
  "dropshipExpress",
  "bulkCostPrice",
  "bulkFloorPrice",
  "bulkMoq",
  "supplier",
]
assert(
  Boolean(
    product?.productDetail &&
      !("supplierId" in product.productDetail) &&
      !("supplier" in product.productDetail) &&
      product.productDetail.skus.every((sku) =>
        forbiddenW14SupplyKeys.every((key) => !(key in sku))
      )
  ),
  "W14 product and SKU revisions do not embed supplier-offering fields"
)

const skuOfferingQueue = await fetchSupplierCatalogQueue({
  mode: "list",
  changeType: "all",
  skuId: "sku_ny_box_01",
})
assert(
  skuOfferingQueue.items.length > 0 &&
    skuOfferingQueue.items.every(
      (item) =>
        item.mapping?.skuId === "sku_ny_box_01" ||
        item.skuCandidates.some((candidate) => candidate.skuId === "sku_ny_box_01")
    ),
  "W21 mock API filters supplier offerings by stable skuId"
)

const offerings = SUPPLIER_CATALOG_SEED.flatMap((item) => {
  if (!item.offering) return []
  return [
    ...(item.offering.currentRevision ? [item.offering.currentRevision] : []),
    ...(item.offering.proposedDefaults ? [item.offering.proposedDefaults] : []),
  ]
})
assert(
  offerings.length > 0 &&
    offerings.every((offering) =>
      Boolean(offering.floorPriceGross && offering.supplyMode)
    ),
  "W21 current revisions and proposed drafts expose floor price and supply mode"
)

const gateResult = submitW22PublishRevision({
  publicationId: "pub_ny_box_01",
  expectedObjectVersion: "ov-ny-box-12",
  expectedPublishGateVersion: "stale-gate-version",
  requestId: "test-w22-stale-gate",
  content: {
    skuRevisionId: "sku_rev",
    supplierOfferingRevisionId: "sor_ny_box_r12",
    categoryId: "cat",
    name: "name",
    specification: "spec",
    salesDescription: "description",
    minimumPurchaseQuantity: "1",
    salesPriceGross: "128.00",
    salesTaxRate: "0.13",
    baseUnitCode: "BOX",
    salesRegion: ["华东"],
    saleStatus: "ON_SALE",
    productCapabilities: [],
    validFrom: "2026-08-02",
    media: [],
  },
})
assert(
  gateResult.status === "blocked" && gateResult.code === "GATE_VERSION_MISMATCH",
  "W22 rejects a stale publish-gate version before committing"
)

const safetyCommand = {
  cause: "ZERO_INVENTORY" as const,
  sourceObjectType: "SUPPLIER_OFFERING" as const,
  sourceObjectId: "off-test-zero",
  sourceVersion: "sv-test-1",
  subjectHash: "sha256-test-zero",
  affectedPublicationIds: ["pub_ny_box_01"],
  occurredAt: "2026-08-02T12:00:00+08:00",
  idempotencyKey: "idem-test-zero-sv1",
}
const safetyResult = triggerW22SystemSafetyPause(safetyCommand)
const safetyReplay = triggerW22SystemSafetyPause(safetyCommand)
assert(
  safetyResult.resultStatus === "COMMITTED" &&
    safetyResult.cause === "ZERO_INVENTORY" &&
    safetyResult.followUpBlocker.code === "NO_MANUAL_FOLLOW_UP_TASK_BY_CURRENT_POLICY",
  "W22 system event atomically records safety-pause evidence and blocker"
)
assert(
  JSON.stringify(safetyReplay) === JSON.stringify(safetyResult),
  "W22 safety pause is idempotent by the original event key"
)
assert(
  getW22PublicationOverride("pub_ny_box_01")?.row.publicationStatus === "SAFETY_PAUSED",
  "W22 affected publication is fail-closed after the domain event"
)

const salesApi = fs.readFileSync(
  path.join(process.cwd(), "features/sales-orders/api.ts"),
  "utf8"
)
const purchaseMock = fs.readFileSync(
  path.join(process.cwd(), "mock/purchase-orders.ts"),
  "utf8"
)
const w21Page = fs.readFileSync(
  path.join(process.cwd(), "features/supplier-catalog/supplier-catalog-page.tsx"),
  "utf8"
)
const w14Page = fs.readFileSync(
  path.join(process.cwd(), "features/master-data/product-detail-page.tsx"),
  "utf8"
)
assert(
  salesApi.includes("multiplyFixed") && !salesApi.includes("Math.round"),
  "W05 formal totals use the shared fixed-decimal implementation"
)
assert(
  purchaseMock.includes("multiplyFixed") && !purchaseMock.includes("Math.round"),
  "W08 formal totals use the shared fixed-decimal implementation"
)
assert(
  w21Page.includes("useAppForm") &&
    w21Page.includes('name="floorPriceGross"') &&
    w21Page.includes('aria-label="供给模式（可多选）"'),
  "W21 full supply draft is wired through TanStack Form"
)
assert(
  w14Page.includes("mode=list&skuId=") &&
    w14Page.includes("RegisterSupplyForSkuDialog") &&
    w14Page.includes("添加供应商并登记成本") &&
    !w14Page.includes("sku.dropshipCostPrice") &&
    !w14Page.includes("sku.bulkCostPrice") &&
    !w14Page.includes("sku.inputTaxRate"),
  "W14 opens the W21 supplier-cost editor by stable skuId without embedding supply fields in the SKU"
)

if (failed) process.exit(1)
console.log("All inventory/pricing contract checks passed")
`

fs.writeFileSync(runnerPath, runner)
const result = spawnSync(
  "npx",
  ["--yes", "tsx", path.relative(root, runnerPath)],
  { cwd: root, encoding: "utf8", env: process.env }
)
try {
  fs.unlinkSync(runnerPath)
} catch {
  // ignore cleanup errors; the test result remains authoritative
}
console.log(result.stdout || result.stderr)
process.exit(result.status ?? 1)
