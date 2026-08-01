/**
 * Exercises the shipped filterSalesOrders pure function via tsx.
 * Run: node scripts/test-filter-orders.mjs
 */
import { spawnSync } from "node:child_process"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const scratch = process.env.SCRATCH || root

const runner = `
import { filterSalesOrders, matchesSalesOrderSearch, salesOrderSummaryLabels } from "../features/sales-orders/filter-orders.ts"
import { MOCK_SALES_ORDERS } from "../mock/sales-orders.ts"

let failed = 0
function assert(cond: boolean, msg: string) {
  if (!cond) {
    console.error("FAIL:", msg)
    failed += 1
  } else {
    console.log("OK:", msg)
  }
}

assert(MOCK_SALES_ORDERS.length >= 2, "mock has orders")
assert(
  matchesSalesOrderSearch(
    MOCK_SALES_ORDERS[0],
    MOCK_SALES_ORDERS[0].documentNumber
  ),
  "search by document number"
)
assert(
  !matchesSalesOrderSearch(MOCK_SALES_ORDERS[0], "___no_match___"),
  "search miss"
)

const pending = filterSalesOrders(MOCK_SALES_ORDERS, {
  summaryFilter: "pending",
})
assert(
  pending.every((o) =>
    ["待二次确认", "待销售处理", "待销售领导审批", "待运营审批", "草稿"].includes(
      o.primaryStatus.label
    )
  ),
  "pending filter"
)
assert(pending.length > 0, "pending non-empty")

const card = filterSalesOrders(MOCK_SALES_ORDERS, {
  natureFilter: "card_voucher",
})
assert(
  card.every((o) => o.nature === "card_voucher"),
  "cardVoucher filter"
)

const searchHit = filterSalesOrders(MOCK_SALES_ORDERS, {
  search: MOCK_SALES_ORDERS[0].customerName.slice(0, 2),
})
assert(searchHit.length >= 1, "customer partial search")

assert(
  salesOrderSummaryLabels("pending") === "待处理",
  "summary label"
)
assert(salesOrderSummaryLabels("all") === "全部指标", "default summary label")

if (failed) process.exit(1)
console.log("All filterSalesOrders checks passed against shipped module")
`

const tmp = path.join(root, "scripts", ".run-filter-test.mts")
const typecheckConfig = path.join(root, "scripts", ".run-filter-test.tsconfig.json")
const typecheckBuildInfo = path.join(
  root,
  "scripts",
  ".run-filter-test.tsconfig.tsbuildinfo"
)
fs.writeFileSync(tmp, runner)
fs.writeFileSync(
  typecheckConfig,
  JSON.stringify({
    extends: "../tsconfig.json",
    compilerOptions: { allowImportingTsExtensions: true, incremental: false },
    include: [".run-filter-test.mts"],
    exclude: [],
  })
)

const typecheck = spawnSync(
  "npx",
  ["tsc", "-p", typecheckConfig, "--pretty", "false"],
  { cwd: root, encoding: "utf8", env: process.env }
)
const tryTsx =
  typecheck.status === 0
    ? spawnSync(
        "npx",
        ["--yes", "tsx", path.join("scripts", ".run-filter-test.mts")],
        { cwd: root, encoding: "utf8", env: process.env }
      )
    : typecheck

fs.mkdirSync(scratch, { recursive: true })
const logPath = path.join(scratch, "filter-orders-test.log")
fs.writeFileSync(
  logPath,
  [
    "exit=" + tryTsx.status,
    "stdout:",
    tryTsx.stdout || "",
    "stderr:",
    tryTsx.stderr || "",
  ].join("\n")
)

try {
  fs.unlinkSync(tmp)
  fs.unlinkSync(typecheckConfig)
  if (fs.existsSync(typecheckBuildInfo)) fs.unlinkSync(typecheckBuildInfo)
} catch {
  // ignore
}

console.log(tryTsx.stdout || tryTsx.stderr)
process.exit(tryTsx.status ?? 1)
