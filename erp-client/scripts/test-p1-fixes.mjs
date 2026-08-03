/**
 * Behavioral checks for the six verified fixes.
 * Run: node scripts/test-p1-fixes.mjs
 */
import { spawnSync } from "node:child_process"
import fs from "node:fs"
import path from "node:path"
import { fileURLToPath } from "node:url"

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..")
const scratch = process.env.SCRATCH || root

const runner = `
import { isNavItemActive } from "../lib/nav-active.ts"
import { WORKSPACE_NAV_GROUPS } from "../lib/workspace-registry.ts"
import {
  FULFILLMENT_NEUTRAL_HEADER,
  laneHeader,
  resolveLane,
} from "../features/fulfillment-operations/lanes.ts"
import {
  buildQueueSearchParams,
  scopeLabelToSlug,
  scopeSlugToLabel,
} from "../features/workspace-kit/queue-scope.ts"
import {
  completeWorkspaceQueueTask,
  fetchWorkspaceQueueTasks,
} from "../features/workspace-kit/queries.ts"
import {
  fetchSalesOrderDetail,
  submitSalesOrderAcceptance,
} from "../features/sales-orders/acceptance.ts"
import { exportListRowsToCsv } from "../features/workspace-kit/export-list-csv.ts"
import { getWorkspacePageDef } from "../mock/workspace-pages.ts"
import { WORK_ITEM_FIXTURES } from "../mock/work-items.ts"
import { buildTodayWorkspaceView } from "../mock/workspace.ts"
import { fetchProcurementQueue } from "../features/procurement-confirmation/api.ts"

let failed = 0
function assert(cond: boolean, msg: string) {
  if (!cond) {
    console.error("FAIL:", msg)
    failed += 1
  } else {
    console.log("OK:", msg)
  }
}

// --- Nav longest-match ---
const hrefs = ["/workspace", "/workspace/tasks", "/sales/orders"]
assert(
  isNavItemActive("/workspace/tasks", "/workspace", hrefs) === false,
  "/workspace not active on /workspace/tasks"
)
assert(
  isNavItemActive("/workspace/tasks", "/workspace/tasks", hrefs) === true,
  "/workspace/tasks active on itself"
)
assert(
  isNavItemActive("/workspace", "/workspace", hrefs) === true,
  "/workspace active on exact"
)

// --- Nav query constraints (W09 dual lane) ---
const fulfillmentHrefs = [
  "/fulfillment?lane=warehouse",
  "/fulfillment?lane=procurement",
  "/inventory",
]
assert(
  isNavItemActive(
    "/fulfillment",
    "/fulfillment?lane=warehouse",
    fulfillmentHrefs,
    "lane=warehouse&scope=mine"
  ) === true,
  "warehouse lane active"
)
assert(
  isNavItemActive(
    "/fulfillment",
    "/fulfillment?lane=procurement",
    fulfillmentHrefs,
    "lane=warehouse&scope=mine"
  ) === false,
  "procurement lane inactive on warehouse"
)
assert(
  isNavItemActive(
    "/fulfillment",
    "/fulfillment?lane=procurement",
    fulfillmentHrefs,
    "lane=procurement&scope=mine"
  ) === true,
  "procurement lane active"
)
// 无 lane 的深链不该点亮任一岗位入口
assert(
  isNavItemActive("/fulfillment", "/fulfillment?lane=warehouse", fulfillmentHrefs, "scope=mine") ===
    false &&
    isNavItemActive(
      "/fulfillment",
      "/fulfillment?lane=procurement",
      fulfillmentHrefs,
      "scope=mine"
    ) === false,
  "lane-less deep link highlights neither lane"
)

// --- W09 lane header（防「对着电子交付写收货与发货」回归）---
assert(resolveLane("warehouse", null) === "warehouse", "explicit lane wins")
assert(resolveLane(null, "procurement") === "procurement", "lane derived from role")
// 只读角色与未声明岗位的深链必须落中性页头，不能回落 warehouse
assert(resolveLane(null, "sales") === null, "read-only sales has no lane")
assert(resolveLane(null, "finance") === null, "read-only finance has no lane")
assert(resolveLane(null, null) === null, "lane-less deep link has no lane")
assert(
  laneHeader(null).label === FULFILLMENT_NEUTRAL_HEADER.label &&
    laneHeader(null).group === undefined,
  "neutral header has no owning group"
)
assert(
  laneHeader("warehouse").label === "收货与发货" &&
    laneHeader("procurement").label === "交付与代发",
  "lane headers are role-facing"
)

// --- 侧栏两个岗位入口都要有待处理数徽章 ---
const w09NavItems = WORKSPACE_NAV_GROUPS.flatMap((g) =>
  g.items.filter((i) => i.href.startsWith("/fulfillment"))
)
assert(w09NavItems.length === 2, "two W09 lane entries in sidebar")
assert(
  w09NavItems.every((i) => Boolean(i.badge)),
  "both lane entries carry a pending-count badge"
)

// --- Queue URL helpers ---
const scopes = ["我的待办", "待领取", "团队"]
assert(scopeLabelToSlug("待领取", scopes) === "role_pool", "scope slug")
assert(scopeSlugToLabel("role_pool", scopes) === "待领取", "scope label")
const qs = buildQueueSearchParams({
  scopeLabel: "我的待办",
  scopeLabels: scopes,
  currentWorkItemId: "wi_pc_01",
  queueContextId: "queue:W02:mine",
})
assert(qs.includes("scope=mine"), "url has scope")
assert(qs.includes("currentWorkItemId=wi_pc_01"), "url has work item")
assert(qs.includes("queueContextId="), "url has queue context")

// --- Queue complete mutates session + fetch ---
const before = await fetchWorkspaceQueueTasks("W02")
const target = before.find((t) => t.id === "wi_map_03") ?? before[before.length - 1]
assert(Boolean(target), "has queue task")
assert(
  Boolean(before.find((t) => t.handlerHref)),
  "W02 tasks expose specialized handlerHref"
)
const holdTarget =
  before.find((t) => t.id !== target!.id) ?? before[0]
const held = await completeWorkspaceQueueTask({
  workspaceId: "W02",
  taskId: holdTarget!.id,
  outcome: "blocked",
})
assert(held.reference.includes("HOLD-W02"), "hold reference")
const afterHold = await fetchWorkspaceQueueTasks("W02")
const heldRow = afterHold.find((t) => t.id === holdTarget!.id)
assert(Boolean(heldRow), "held task remains in active queue")
assert(heldRow?.status.label === "已暂挂", "held task status is 已暂挂")

const done = await completeWorkspaceQueueTask({
  workspaceId: "W02",
  taskId: target!.id,
  outcome: "succeeded",
})
assert(done.reference.includes("OK-W02"), "complete reference")
const after = await fetchWorkspaceQueueTasks("W02")
assert(
  after.every((t) => t.id !== target!.id),
  "completed task removed from queue fetch"
)

// --- Acceptance posts + reloads ---
const orderId = "so_1002"
await submitSalesOrderAcceptance({
  salesOrderId: orderId,
  documentNumber: "XS-TEST",
  acceptedQuantity: "10",
  note: "客户现场确认",
})
const detail = await fetchSalesOrderDetail(orderId)
assert(Boolean(detail?.acceptance), "acceptance persisted on detail fetch")
assert(
  detail!.acceptance!.acceptedQuantity === "10",
  "acceptance quantity round-trip"
)

// --- Export / primary action surface ---
const w04 = getWorkspacePageDef("W04")
if (w04.shell.kind !== "list") throw new Error("W04")
assert(typeof exportListRowsToCsv === "function", "export helper exported")
assert(
  w04.shell.payload.primaryActionLabel != null,
  "list has primary action label"
)

// handler on specialized task
const w02 = getWorkspacePageDef("W02")
if (w02.shell.kind !== "queue") throw new Error("W02")
const specialized = w02.shell.payload.tasks.find((t) => t.handlerHref)
assert(
  specialized?.handlerHref?.includes("/procurement/confirm") === true,
  "采购二次确认 opens specialized confirm queue"
)
const rolePoolHandler = WORK_ITEM_FIXTURES.find((item) => item.id === "wi_pc_03")
assert(
  rolePoolHandler?.handlerHref?.includes("scope=role_pool") === true,
  "W02 role-pool task preserves scope in W07 handler"
)

// --- W01 server-side responsibility scope ---
const mineDashboard = buildTodayWorkspaceView({
  scope: "mine",
  timezone: "Asia/Shanghai",
})
const mineDashboardItems = mineDashboard.groups.flatMap((group) => group.items)
assert(
  mineDashboardItems.length > 0 &&
    mineDashboardItems.every((item) => item.ownerUserLabel === "王敏"),
  "W01 mine includes only current viewer tasks"
)
const rolePoolDashboard = buildTodayWorkspaceView({
  scope: "role_pool",
  timezone: "Asia/Shanghai",
})
const rolePoolDashboardItems = rolePoolDashboard.groups.flatMap(
  (group) => group.items
)
assert(
  rolePoolDashboardItems.length > 0 &&
    rolePoolDashboardItems.every((item) => item.ownerUserLabel == null),
  "W01 role pool includes only unassigned tasks"
)

// --- W07 responsibility and due filters ---
const mineProcurement = await fetchProcurementQueue({
  scope: "mine",
  due: "active",
})
assert(
  mineProcurement.tasks.length === 2 &&
    mineProcurement.tasks.every((task) => task.responsibilityScope === "mine"),
  "W07 mine filters responsibility scope"
)
const poolProcurement = await fetchProcurementQueue({
  scope: "role_pool",
  due: "active",
})
assert(
  poolProcurement.tasks.length === 1 &&
    poolProcurement.tasks.every(
      (task) => task.responsibilityScope === "role_pool"
    ),
  "W07 role pool filters responsibility scope"
)
const todayProcurement = await fetchProcurementQueue({
  scope: "mine",
  due: "today",
})
assert(
  todayProcurement.tasks.length === 1 &&
    todayProcurement.tasks.every((task) => task.dueAt.startsWith("2026-08-01")),
  "W07 today returns only current-date tasks"
)
const overdueProcurement = await fetchProcurementQueue({
  scope: "mine",
  due: "overdue",
})
assert(
  overdueProcurement.tasks.length === 1 &&
    overdueProcurement.tasks[0]?.workItemId === "wi_pc_02",
  "W07 overdue returns only overdue tasks"
)

if (failed) process.exit(1)
console.log("All P1/P2 fix checks passed")
`

const tmp = path.join(root, "scripts", ".run-p1-fixes.mts")
fs.writeFileSync(tmp, runner)
const result = spawnSync(
  "npx",
  ["--yes", "tsx", path.join("scripts", ".run-p1-fixes.mts")],
  { cwd: root, encoding: "utf8", env: process.env }
)
fs.mkdirSync(scratch, { recursive: true })
fs.writeFileSync(
  path.join(scratch, "p1-fixes-test.log"),
  ["exit=" + result.status, result.stdout || "", result.stderr || ""].join("\n")
)
try {
  fs.unlinkSync(tmp)
} catch {
  // ignore
}
console.log(result.stdout || result.stderr)
process.exit(result.status ?? 1)
