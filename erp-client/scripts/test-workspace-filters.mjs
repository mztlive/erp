/**
 * Honest behavioral tests against shipped filter helpers + W14 fixtures.
 * Run: node scripts/test-workspace-filters.mjs
 */
import { spawnSync } from "node:child_process"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const scratch = process.env.SCRATCH || root

const runner = `
import { filterListRows } from "../features/workspace-kit/filter-list-rows.ts"
import { filterQueueTasks } from "../features/workspace-kit/filter-queue-tasks.ts"
import { filterObjectItems } from "../features/workspace-kit/filter-object-items.ts"
import {
  getMasterDataPageDef,
  getWorkspacePageDef,
  MASTER_DATA_FIXTURES,
  MASTER_DATA_RESOURCES,
} from "../mock/workspace-pages.ts"

let failed = 0
function assert(cond: boolean, msg: string) {
  if (!cond) {
    console.error("FAIL:", msg)
    failed += 1
  } else {
    console.log("OK:", msg)
  }
}

// --- List: W04 contracts metric + filter ---
const w04 = getWorkspacePageDef("W04")
if (w04.shell.kind !== "list") throw new Error("W04 not list")
const w04p = w04.shell.payload
const allContracts = filterListRows(w04p.rows, {
  metrics: w04p.metrics,
  filterLabels: w04p.filterLabels,
  metricKey: "all",
  filterLabel: "全部",
})
assert(allContracts.length === w04p.rows.length, "W04 default shows all rows")

const activeOnly = filterListRows(w04p.rows, {
  metrics: w04p.metrics,
  filterLabels: w04p.filterLabels,
  metricKey: "active",
  filterLabel: "全部",
})
assert(
  activeOnly.length > 0 && activeOnly.every((r) => r.status?.label === "有效"),
  "W04 metric active filters to 有效"
)
assert(activeOnly.length < allContracts.length, "W04 metric active narrows list")

const endingFilter = filterListRows(w04p.rows, {
  metrics: w04p.metrics,
  filterLabels: w04p.filterLabels,
  metricKey: "all",
  filterLabel: "将到期",
})
assert(
  endingFilter.length > 0 &&
    endingFilter.every((r) => r.status?.label === "将到期"),
  "W04 filterLabel 将到期 changes table"
)

// --- List: W10 inventory filterTags ---
const w10 = getWorkspacePageDef("W10")
if (w10.shell.kind !== "list") throw new Error("W10 not list")
const w10p = w10.shell.payload
const zero = filterListRows(w10p.rows, {
  metrics: w10p.metrics,
  filterLabels: w10p.filterLabels,
  metricKey: "zero",
  filterLabel: "全部",
})
assert(
  zero.length > 0 && zero.every((r) => r.status?.label === "零可用"),
  "W10 metric zero filters 零可用"
)
const inTransit = filterListRows(w10p.rows, {
  metrics: w10p.metrics,
  filterLabels: w10p.filterLabels,
  metricKey: "combos",
  filterLabel: "有在途",
})
assert(
  inTransit.length > 0 &&
    inTransit.every((r) => (r.filterTags ?? []).includes("有在途")),
  "W10 filter 有在途 uses filterTags"
)

// --- Queue: W02 scope ---
const w02 = getWorkspacePageDef("W02")
if (w02.shell.kind !== "queue") throw new Error("W02 not queue")
const w02p = w02.shell.payload
const mine = filterQueueTasks(w02p.tasks, {
  scope: "我的待办",
  scopeLabels: w02p.scopeLabels,
})
assert(mine.length === w02p.tasks.length, "W02 default scope shows all tasks")
const claimable = filterQueueTasks(w02p.tasks, {
  scope: "待领取",
  scopeLabels: w02p.scopeLabels,
})
assert(
  claimable.length > 0 &&
    claimable.every(
      (t) =>
        t.status.label === "待领取" || (t.scopeTags ?? []).includes("待领取")
    ),
  "W02 待领取 scope filters tasks"
)
assert(claimable.length < mine.length, "W02 待领取 narrows queue")
const team = filterQueueTasks(w02p.tasks, {
  scope: "团队",
  scopeLabels: w02p.scopeLabels,
})
assert(
  team.length > 0 && team.every((t) => (t.scopeTags ?? []).includes("团队")),
  "W02 团队 scope uses scopeTags"
)

// --- Object: W03 scope ---
const w03 = getWorkspacePageDef("W03")
if (w03.shell.kind !== "object") throw new Error("W03 not object")
const w03p = w03.shell.payload
const myCustomers = filterObjectItems(w03p.items, {
  scope: "我的客户",
  scopeLabels: w03p.scopeLabels,
})
assert(
  myCustomers.length === w03p.items.length,
  "W03 default scope shows all customers"
)
const collab = filterObjectItems(w03p.items, {
  scope: "协作客户",
  scopeLabels: w03p.scopeLabels,
})
assert(
  collab.length > 0 &&
    collab.every((i) => (i.scopeTags ?? []).includes("协作客户")),
  "W03 协作客户 scope filters list"
)
assert(collab.length < myCustomers.length, "W03 协作客户 narrows list")
const teamCust = filterObjectItems(w03p.items, {
  scope: "团队客户",
  scopeLabels: w03p.scopeLabels,
  search: "北辰",
})
assert(
  teamCust.length === 1 && teamCust[0].code === "KH-000311",
  "W03 team scope + search hits 北辰"
)

// --- W14 per-resource fixtures ---
for (const resource of MASTER_DATA_RESOURCES) {
  const page = getMasterDataPageDef(resource.key)
  if (page.shell.kind !== "list") throw new Error("W14 not list")
  const rows = page.shell.payload.rows
  assert(rows.length > 0, \`W14 \${resource.key} has rows\`)
  assert(
    page.title.includes(resource.label),
    \`W14 title includes \${resource.label}\`
  )
}

const sellable = getMasterDataPageDef("sellable-items")
const warehouses = getMasterDataPageDef("warehouses")
if (sellable.shell.kind !== "list" || warehouses.shell.kind !== "list") {
  throw new Error("bad shells")
}
const sellCodes = sellable.shell.payload.rows.map((r) => r.cells.code ?? r.cells.name)
const whCodes = warehouses.shell.payload.rows.map((r) => r.cells.code ?? r.cells.name)
assert(
  sellCodes.some((c) => String(c).startsWith("SI-")),
  "sellable-items uses SI-* codes"
)
assert(
  whCodes.some((c) => String(c).startsWith("WH-")),
  "warehouses uses WH-* codes"
)
assert(
  !whCodes.some((c) => String(c).startsWith("SI-")),
  "warehouses does not show sellable-item codes"
)
assert(
  !sellCodes.some((c) => String(c).startsWith("WH-")),
  "sellable-items does not show warehouse codes"
)

const products = getMasterDataPageDef("products")
if (products.shell.kind !== "list") throw new Error("products")
assert(
  products.shell.payload.rows.some((r) =>
    String(r.cells.code ?? "").startsWith("SPU-")
  ),
  "products list uses SPU codes"
)
assert(
  MASTER_DATA_FIXTURES.suppliers.rows.some((r) =>
    String(r.cells.code ?? "").startsWith("SUP-")
  ),
  "suppliers fixture identity"
)

if (failed) process.exit(1)
console.log("All workspace filter + W14 resource checks passed")
`

const tmp = path.join(root, "scripts", ".run-workspace-filters.mts")
fs.writeFileSync(tmp, runner)

const result = spawnSync(
  "npx",
  ["--yes", "tsx", path.join("scripts", ".run-workspace-filters.mts")],
  { cwd: root, encoding: "utf8", env: process.env }
)

fs.mkdirSync(scratch, { recursive: true })
fs.writeFileSync(
  path.join(scratch, "workspace-filters-test.log"),
  ["exit=" + result.status, "stdout:", result.stdout || "", "stderr:", result.stderr || ""].join(
    "\n"
  )
)

try {
  fs.unlinkSync(tmp)
} catch {
  // ignore
}

console.log(result.stdout || result.stderr)
process.exit(result.status ?? 1)
