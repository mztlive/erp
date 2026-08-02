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
import { fetchExternalCatalogQueue } from "../features/external-product-supply/api.ts"
import { MASTER_DATA_CENTER_SEEDS } from "../features/master-data/data.ts"
import { EXTERNAL_PRODUCT_SUPPLY_SEED } from "../mock/external-product-supply.ts"

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
  const activeMappings = EXTERNAL_PRODUCT_SUPPLY_SEED.filter(
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

const teaView = await fetchExternalCatalogQueue({
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
