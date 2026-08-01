/**
 * W24 session-mock API：queryFn / mutationFn 纯函数。
 * 会话覆盖保存在本模块，避免与其它 wave 的 session-state 冲突。
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  BatchStatus,
  BlockerCode,
  ConfirmationState,
  ConsumptionCutoverView,
  MaintenanceFreezeView,
  MigrationFormalCommand,
  MigrationFormalResult,
  OwnershipMigrationBatchRow,
  OwnershipMigrationBatchView,
  OwnershipMigrationListQuery,
  OwnershipMigrationListView,
  ViewerRoleDemo,
} from "@/features/ownership-migration/types"
import {
  BATCH_SEEDS,
  CUTOVER_READY_PATCH,
  CUTOVER_SEED,
  MALL,
  MAINTENANCE_FREEZE_SEED,
} from "@/mock/ownership-migration"

type BatchOverlay = Partial<
  Pick<
    OwnershipMigrationBatchView,
    | "status"
    | "stage"
    | "scopeHash"
    | "freeze"
    | "counts"
    | "confirmations"
    | "lastSyncWatermark"
    | "fullReconcileDone"
    | "items"
    | "backgroundOperation"
    | "formalResult"
    | "allowedActions"
    | "actionBlockers"
    | "objectVersion"
    | "checks"
  >
>

const batchOverlays = new Map<string, BatchOverlay>()
let freezeOverlay: Partial<MaintenanceFreezeView> | null = null
let cutoverOverlay: Partial<ConsumptionCutoverView> | null = null
let cutoverDemoReady = false
const formalOps = new Map<string, MigrationFormalResult>()

function nowIso() {
  return new Date().toISOString()
}

function deepMergeConfirmations(
  base: OwnershipMigrationBatchView["confirmations"],
  patch?: OwnershipMigrationBatchView["confirmations"]
) {
  if (!patch) return base
  return {
    sales: { ...base.sales, ...patch.sales },
    finance: { ...base.finance, ...patch.finance },
    baseline: { ...base.baseline, ...patch.baseline },
  }
}

function projectBatch(
  seed: OwnershipMigrationBatchView,
  role: ViewerRoleDemo
): OwnershipMigrationBatchView {
  const overlay = batchOverlays.get(seed.identity.batchId)
  const confirmations = deepMergeConfirmations(
    seed.confirmations,
    overlay?.confirmations
  )

  const financeMasked =
    role === "SALES_CONFIRMER" ||
    role === "BUSINESS_USER" ||
    role === "NO_MODULE"

  let allowedActions = [
    ...(overlay?.allowedActions ?? seed.allowedActions),
  ]
  const actionBlockers = [
    ...(overlay?.actionBlockers ?? seed.actionBlockers),
  ]

  // 管理员不可代签销售/财务/基线
  if (role === "SYSTEM_ADMIN") {
    for (const action of [
      "CONFIRM_SALES",
      "CONFIRM_FINANCE",
      "CONFIRM_BASELINE",
    ] as const) {
      allowedActions = allowedActions.filter((a) => a !== action)
      if (
        !actionBlockers.some(
          (b) => b.action === action && b.code === "ADMIN_CANNOT_CONFIRM"
        )
      ) {
        actionBlockers.push({
          action,
          code: "ADMIN_CANNOT_CONFIRM",
          message:
            "系统管理员不能代签业务确认；销售/财务/基线须由各自责任角色独立完成。",
        })
      }
    }
  }

  if (role === "SALES_CONFIRMER") {
    allowedActions = allowedActions.filter(
      (a) => a === "CONFIRM_SALES" || a === "RECHECK_SCOPE"
    )
    if (!allowedActions.includes("CONFIRM_SALES") && confirmations.sales.state !== "VALID") {
      allowedActions.push("CONFIRM_SALES")
    }
    actionBlockers.push({
      action: "CONFIRM_FINANCE",
      code: "ROLE_MISMATCH",
      message: "销售确认人不能替代财务确认。",
    })
    actionBlockers.push({
      action: "CONFIRM_BASELINE",
      code: "ROLE_MISMATCH",
      message: "销售确认人不能替代最终基线确认。",
    })
    actionBlockers.push({
      action: "EXECUTE_BATCH",
      code: "ROLE_MISMATCH",
      message: "仅系统管理员可执行客户批次。",
    })
  }

  if (role === "FINANCE_CONFIRMER") {
    allowedActions = allowedActions.filter((a) => a === "CONFIRM_FINANCE")
    if (
      !allowedActions.includes("CONFIRM_FINANCE") &&
      confirmations.finance.state !== "VALID"
    ) {
      allowedActions.push("CONFIRM_FINANCE")
    }
    actionBlockers.push({
      action: "CONFIRM_SALES",
      code: "ROLE_MISMATCH",
      message: "财务确认人不能替代销售确认。",
    })
    actionBlockers.push({
      action: "CONFIRM_BASELINE",
      code: "ROLE_MISMATCH",
      message: "财务确认人不能替代最终基线确认。",
    })
  }

  if (role === "CUTOVER_OWNER") {
    // 上线负责人可确认基线，不可代签销售/财务/执行
    allowedActions = allowedActions.filter(
      (a) =>
        a === "CONFIRM_BASELINE" ||
        a === "START_FREEZE" ||
        a === "RUN_FINAL_SYNC"
    )
    const freezeActive = overlay?.freeze?.active ?? seed.freeze.active
    const fullReconcile =
      overlay?.fullReconcileDone ?? seed.fullReconcileDone
    const lastSync =
      overlay?.lastSyncWatermark ?? seed.lastSyncWatermark
    if (
      freezeActive &&
      fullReconcile &&
      lastSync &&
      confirmations.baseline.state !== "VALID"
    ) {
      if (!allowedActions.includes("CONFIRM_BASELINE")) {
        allowedActions.push("CONFIRM_BASELINE")
      }
    } else if (!freezeActive || !fullReconcile || !lastSync) {
      allowedActions = allowedActions.filter((a) => a !== "CONFIRM_BASELINE")
      if (
        !actionBlockers.some(
          (b) => b.action === "CONFIRM_BASELINE" && b.code === "BASELINE_GATES"
        )
      ) {
        actionBlockers.push({
          action: "CONFIRM_BASELINE",
          code: "BASELINE_GATES",
          message:
            "最终基线仅在维护冻结生效、最后一期同步完成且全量核对通过后可提交。",
        })
      }
    }
    for (const action of ["CONFIRM_SALES", "CONFIRM_FINANCE", "EXECUTE_BATCH", "RESUME_BATCH"]) {
      if (
        !actionBlockers.some(
          (b) => b.action === action && b.code === "ROLE_MISMATCH"
        )
      ) {
        actionBlockers.push({
          action,
          code: "ROLE_MISMATCH",
          message: "上线负责人不能代签该动作。",
        })
      }
    }
  }

  if (role === "BUSINESS_USER" || role === "NO_MODULE") {
    allowedActions = []
    actionBlockers.push({
      action: "*",
      code: role === "NO_MODULE" ? "NO_MODULE_ACCESS" : "READ_ONLY",
      message:
        role === "NO_MODULE"
          ? "无 W24 管理权限；仅可通过维护 Banner 查看授权摘要。"
          : "业务用户只读查看本人客户是否在迁移范围。",
    })
  }

  const items = (overlay?.items ?? seed.items).map((item) => {
    // 执行中/失败不得把项显示为已迁移
    const status = overlay?.status ?? seed.status
    if (
      (status === "EXECUTING" || status === "FAILED") &&
      item.itemStatus === "MIGRATED"
    ) {
      return { ...item, itemStatus: "NOT_MIGRATED" as const }
    }
    return item
  })

  const counts = overlay?.counts ?? seed.counts
  // migratedCount 仅完成态
  const safeCounts = {
    ...counts,
    migrated:
      (overlay?.status ?? seed.status) === "COMPLETED" ? counts.migrated : 0,
  }

  return {
    ...seed,
    status: overlay?.status ?? seed.status,
    stage: overlay?.stage ?? seed.stage,
    scopeHash: overlay?.scopeHash ?? seed.scopeHash,
    freeze: overlay?.freeze ?? seed.freeze,
    counts: safeCounts,
    confirmations,
    lastSyncWatermark: overlay?.lastSyncWatermark ?? seed.lastSyncWatermark,
    fullReconcileDone: overlay?.fullReconcileDone ?? seed.fullReconcileDone,
    items,
    checks: overlay?.checks ?? seed.checks,
    backgroundOperation:
      overlay?.backgroundOperation ?? seed.backgroundOperation,
    formalResult: overlay?.formalResult ?? seed.formalResult,
    allowedActions,
    actionBlockers,
    objectVersion: overlay?.objectVersion ?? seed.objectVersion,
    financeSummaryMasked: financeMasked,
    financeSummary: financeMasked ? "（票款字段已按权限掩码）" : seed.financeSummary,
    queriedAt: nowIso(),
    viewerRole: role,
  }
}

function primaryBlocker(
  batch: OwnershipMigrationBatchView
): { code?: BlockerCode; label?: string } {
  if (batch.confirmations.sales.state === "INVALIDATED") {
    return { code: "SCOPE_DRIFT", label: "确认失效·范围变化" }
  }
  const blocked = batch.checks.find((c) => c.status === "BLOCKED")
  if (blocked?.code.includes("MAP")) {
    return { code: "MAPPING", label: "映射未完成" }
  }
  if (blocked?.code.includes("LINE")) {
    return { code: "SINGLE_LINE", label: "非唯一卡券明细" }
  }
  if (batch.status === "FAILED") {
    return { code: "SYNC_WATERMARK", label: batch.formalResult?.title ?? "执行失败" }
  }
  if (batch.counts.blocked > 0) {
    return { code: "MAPPING", label: `${batch.counts.blocked} 项阻塞` }
  }
  return {}
}

function toListRow(batch: OwnershipMigrationBatchView): OwnershipMigrationBatchRow {
  const blocker = primaryBlocker(batch)
  return {
    batchId: batch.identity.batchId,
    batchNo: batch.identity.batchNo,
    sourceMallId: batch.identity.sourceMallId,
    sourceMallName: batch.identity.sourceMallName,
    customerId: batch.identity.customerId,
    customerName: batch.identity.customerName,
    singleCustomer: true,
    scopeHash: batch.scopeHash,
    status: batch.status,
    freezeActive: batch.freeze.active,
    eligibleCount: batch.counts.eligible,
    blockedCount: batch.counts.blocked,
    migratedCount: batch.counts.migrated,
    salesConfirmation: batch.confirmations.sales,
    financeConfirmation: batch.confirmations.finance,
    baselineConfirmation: batch.confirmations.baseline,
    lastSyncWatermark: batch.lastSyncWatermark,
    errorSummary: batch.formalResult?.title ?? batch.items.find((i) => i.errorSummary)?.errorSummary,
    primaryBlocker: blocker.code,
    primaryBlockerLabel: blocker.label,
    allowedActions: [...batch.allowedActions],
    actionBlockers: batch.actionBlockers,
    updatedAt: batch.queriedAt,
  }
}

function allProjected(role: ViewerRoleDemo) {
  return BATCH_SEEDS.map((s) => projectBatch(s, role))
}

function confirmationMatches(
  row: OwnershipMigrationBatchRow,
  filter?: OwnershipMigrationListQuery["confirmation"]
) {
  if (!filter) return true
  if (filter === "pending_sales") return row.salesConfirmation.state === "MISSING"
  if (filter === "pending_finance")
    return row.financeConfirmation.state === "MISSING"
  if (filter === "pending_baseline")
    return row.baselineConfirmation.state === "MISSING"
  if (filter === "invalidated") {
    return (
      row.salesConfirmation.state === "INVALIDATED" ||
      row.financeConfirmation.state === "INVALIDATED" ||
      row.baselineConfirmation.state === "INVALIDATED"
    )
  }
  return true
}

export async function fetchMaintenanceFreeze(): Promise<MaintenanceFreezeView> {
  await mockDelay()
  const base = { ...MAINTENANCE_FREEZE_SEED, ...(freezeOverlay ?? {}) }
  // 任一投影批次冻结则全局 banner 生效
  const anyFrozen = allProjected("SYSTEM_ADMIN").some((b) => b.freeze.active)
  return {
    ...base,
    active: freezeOverlay?.active ?? anyFrozen ?? base.active,
    queriedAt: nowIso(),
  }
}

export async function fetchOwnershipMigrationList(
  query: OwnershipMigrationListQuery
): Promise<OwnershipMigrationListView> {
  await mockDelay()
  const role = query.role ?? "SYSTEM_ADMIN"

  if (role === "NO_MODULE") {
    return {
      hasModuleAccess: false,
      hasCustomerScope: false,
      mallId: query.mallId || MALL.id,
      mallName: MALL.name,
      metrics: {
        pendingPrepare: 0,
        pendingSales: 0,
        pendingFinance: 0,
        pendingBaseline: 0,
        executable: 0,
        failedFrozen: 0,
        completed: 0,
      },
      statusSummary: {
        phase1WatermarkLabel: "—",
        freezeActive: (await fetchMaintenanceFreeze()).active,
        freezeScopeLabel: MAINTENANCE_FREEZE_SEED.scopeLabel,
        migratedCustomers: 0,
        totalCustomers: 0,
        migratedOrders: 0,
        totalOrders: 0,
        tStatus: "NOT_REGISTERED",
      },
      rows: [],
      totalCount: 0,
      queriedAt: nowIso(),
      financeFieldsMasked: true,
    }
  }

  let batches = allProjected(role)

  if (role === "BUSINESS_USER" || query.view === "my_customers") {
    // 演示：业务用户仅见星河客户
    batches = batches.filter((b) => b.identity.customerId === "cust_xinghe")
  }

  if (role === "BUSINESS_USER" && batches.length === 0) {
    return {
      hasModuleAccess: true,
      hasCustomerScope: false,
      mallId: MALL.id,
      mallName: MALL.name,
      metrics: {
        pendingPrepare: 0,
        pendingSales: 0,
        pendingFinance: 0,
        pendingBaseline: 0,
        executable: 0,
        failedFrozen: 0,
        completed: 0,
      },
      statusSummary: {
        phase1WatermarkLabel: "wm_20260801_phase1",
        freezeActive: true,
        freezeStartedAt: MAINTENANCE_FREEZE_SEED.startedAt,
        freezeScopeLabel: MAINTENANCE_FREEZE_SEED.scopeLabel,
        migratedCustomers: 0,
        totalCustomers: 0,
        migratedOrders: 0,
        totalOrders: 0,
        tStatus: "NOT_REGISTERED",
      },
      rows: [],
      totalCount: 0,
      queriedAt: nowIso(),
      financeFieldsMasked: true,
    }
  }

  const mallId = query.mallId || MALL.id
  batches = batches.filter((b) => b.identity.sourceMallId === mallId)

  if (query.customerId) {
    batches = batches.filter((b) => b.identity.customerId === query.customerId)
  }
  if (query.q) {
    const q = query.q.trim().toLowerCase()
    batches = batches.filter(
      (b) =>
        b.identity.batchNo.toLowerCase().includes(q) ||
        b.identity.customerName.toLowerCase().includes(q) ||
        b.identity.customerId.toLowerCase().includes(q)
    )
  }
  if (query.status && query.status !== "open") {
    batches = batches.filter((b) => b.status === query.status)
  } else if (!query.status || query.status === "open") {
    // 默认未完成与失败
    batches = batches.filter((b) => b.status !== "COMPLETED")
  }

  let rows = batches.map(toListRow)
  rows = rows.filter((r) => confirmationMatches(r, query.confirmation))
  if (query.blocker) {
    rows = rows.filter((r) => r.primaryBlocker === query.blocker)
  }

  // 风险优先排序
  const riskRank = (s: BatchStatus) => {
    if (s === "FAILED") return 0
    if (s === "BASELINE_CONFIRMED") return 1
    if (s === "FROZEN") return 2
    if (s === "AWAITING_CONFIRMATION") return 3
    if (s === "PREPARING") return 4
    if (s === "EXECUTING") return 5
    return 6
  }
  rows = [...rows].sort((a, b) => {
    const inv =
      Number(
        b.salesConfirmation.state === "INVALIDATED" ||
          b.financeConfirmation.state === "INVALIDATED"
      ) -
      Number(
        a.salesConfirmation.state === "INVALIDATED" ||
          a.financeConfirmation.state === "INVALIDATED"
      )
    if (inv !== 0) return inv
    return riskRank(a.status) - riskRank(b.status)
  })

  const allForMetrics = allProjected(role).filter(
    (b) => b.identity.sourceMallId === mallId
  )
  const metrics = {
    pendingPrepare: allForMetrics.filter((b) => b.status === "PREPARING")
      .length,
    pendingSales: allForMetrics.filter(
      (b) => b.confirmations.sales.state !== "VALID"
    ).length,
    pendingFinance: allForMetrics.filter(
      (b) => b.confirmations.finance.state !== "VALID"
    ).length,
    pendingBaseline: allForMetrics.filter(
      (b) => b.confirmations.baseline.state !== "VALID"
    ).length,
    executable: allForMetrics.filter(
      (b) =>
        b.status === "BASELINE_CONFIRMED" ||
        (b.confirmations.sales.state === "VALID" &&
          b.confirmations.finance.state === "VALID" &&
          b.confirmations.baseline.state === "VALID" &&
          b.status !== "COMPLETED" &&
          b.status !== "FAILED")
    ).length,
    failedFrozen: allForMetrics.filter((b) => b.status === "FAILED").length,
    completed: allForMetrics.filter((b) => b.status === "COMPLETED").length,
  }

  const freeze = await fetchMaintenanceFreeze()
  const cutover = await projectCutover(role)

  const page = Math.max(1, query.page)
  const pageSize = Math.max(1, query.pageSize)
  const start = (page - 1) * pageSize
  const pageRows = rows.slice(start, start + pageSize)

  return {
    hasModuleAccess: true,
    hasCustomerScope: true,
    mallId,
    mallName: MALL.name,
    metrics,
    statusSummary: {
      phase1WatermarkLabel: "wm_20260801_phase1 · 一期增量已封存",
      freezeActive: freeze.active,
      freezeStartedAt: freeze.startedAt,
      freezeScopeLabel: freeze.scopeLabel,
      migratedCustomers: metrics.completed,
      totalCustomers: allForMetrics.length,
      migratedOrders: allForMetrics.reduce(
        (sum, b) => sum + b.counts.migrated,
        0
      ),
      totalOrders: allForMetrics.reduce((sum, b) => sum + b.counts.eligible, 0),
      tStatus: cutover.status === "ENABLED" ? "ENABLED" : "NOT_REGISTERED",
      tEnabledAt: cutover.enabledAt,
    },
    rows: pageRows,
    totalCount: rows.length,
    queriedAt: nowIso(),
    financeFieldsMasked:
      role === "SALES_CONFIRMER" || role === "BUSINESS_USER",
  }
}

export async function fetchOwnershipMigrationBatch(params: {
  batchId: string
  role?: ViewerRoleDemo
}): Promise<OwnershipMigrationBatchView | null> {
  await mockDelay()
  const role = params.role ?? "SYSTEM_ADMIN"
  if (role === "NO_MODULE") return null
  const seed = BATCH_SEEDS.find((b) => b.identity.batchId === params.batchId)
  if (!seed) return null
  if (
    role === "BUSINESS_USER" &&
    seed.identity.customerId !== "cust_xinghe"
  ) {
    return null
  }
  return projectBatch(seed, role)
}

async function projectCutover(
  role: ViewerRoleDemo
): Promise<ConsumptionCutoverView> {
  const base: ConsumptionCutoverView = {
    ...CUTOVER_SEED,
    ...(cutoverDemoReady ? CUTOVER_READY_PATCH : {}),
    ...(cutoverOverlay ?? {}),
    checks: cutoverOverlay?.checks
      ?? (cutoverDemoReady ? CUTOVER_READY_PATCH.checks! : CUTOVER_SEED.checks),
    prerequisites:
      cutoverOverlay?.prerequisites ??
      (cutoverDemoReady
        ? CUTOVER_READY_PATCH.prerequisites!
        : CUTOVER_SEED.prerequisites),
    allowedActions: [
      ...(cutoverOverlay?.allowedActions ??
        (cutoverDemoReady
          ? CUTOVER_READY_PATCH.allowedActions!
          : CUTOVER_SEED.allowedActions)),
    ],
    actionBlockers: [
      ...(cutoverOverlay?.actionBlockers ??
        (cutoverDemoReady
          ? CUTOVER_READY_PATCH.actionBlockers!
          : CUTOVER_SEED.actionBlockers)),
    ],
    queriedAt: nowIso(),
  }

  // 仅上线负责人可登记 T
  if (role !== "CUTOVER_OWNER") {
    base.allowedActions = base.allowedActions.filter(
      (a) => a !== "ENABLE_CUTOVER"
    )
    if (
      !base.actionBlockers.some(
        (b) => b.action === "ENABLE_CUTOVER" && b.code === "ROLE_MISMATCH"
      )
    ) {
      base.actionBlockers.push({
        action: "ENABLE_CUTOVER",
        code: "ROLE_MISMATCH",
        message: "仅上线负责人可登记唯一消费回流启用时间 T；管理员与运维不能代签。",
      })
    }
  }

  // T 已登记后不可再改
  if (base.status === "ENABLED") {
    base.allowedActions = base.allowedActions.filter(
      (a) => a !== "ENABLE_CUTOVER"
    )
    if (
      !base.actionBlockers.some((b) => b.code === "T_IMMUTABLE")
    ) {
      base.actionBlockers.push({
        action: "ENABLE_CUTOVER",
        code: "T_IMMUTABLE",
        message: "T 一经登记不可修改或删除。",
      })
    }
  }

  return base
}

export async function fetchConsumptionCutover(params: {
  mallId?: string
  role?: ViewerRoleDemo
}): Promise<ConsumptionCutoverView> {
  await mockDelay()
  return projectCutover(params.role ?? "SYSTEM_ADMIN")
}

export async function submitMigrationFormal(
  command: MigrationFormalCommand
): Promise<MigrationFormalResult> {
  await mockDelay(120)
  const role = command.role ?? "SYSTEM_ADMIN"
  const requestId = command.requestId

  // 幂等：同一 requestId 返回既有结果
  const existing = formalOps.get(requestId)
  if (existing) return existing

  if (command.action === "QUERY_FORMAL_RESULT") {
    const found = [...formalOps.values()].find(
      (op) =>
        (command.batchId && op.batchId === command.batchId) ||
        (command.cutoverId && op.cutoverId === command.cutoverId)
    )
    if (found) return found
    return {
      operationId: `op_query_${Date.now()}`,
      batchId: command.batchId,
      cutoverId: command.cutoverId,
      status: "NOT_COMMITTED",
      nextAction: "无已登记正式结果",
      message: "未找到对应正式操作记录。",
    }
  }

  if (command.action === "START_FREEZE") {
    freezeOverlay = {
      active: true,
      startedAt: nowIso(),
      stageLabel: "维护冻结 · 已生效",
    }
    // 标记进行中批次冻结
    for (const seed of BATCH_SEEDS) {
      if (seed.status !== "COMPLETED") {
        const prev = batchOverlays.get(seed.identity.batchId) ?? {}
        batchOverlays.set(seed.identity.batchId, {
          ...prev,
          freeze: {
            active: true,
            startedAt: nowIso(),
            scopeLabel: MAINTENANCE_FREEZE_SEED.scopeLabel,
          },
          status: prev.status ?? (seed.status === "PREPARING" ? "FROZEN" : seed.status),
          objectVersion: `ov-freeze-${Date.now()}`,
        })
      }
    }
    const result: MigrationFormalResult = {
      operationId: `op_freeze_${Date.now()}`,
      status: "COMMITTED",
      committedAt: nowIso(),
      nextAction: "全局维护 Banner 已生效；执行最后同步与基线确认",
      message: "维护冻结已写入服务端事实，不可由浏览器忽略。",
    }
    formalOps.set(requestId, result)
    return result
  }

  if (command.action === "DEMO_INVALIDATE_SCOPE" && command.batchId) {
    const seed = BATCH_SEEDS.find((b) => b.identity.batchId === command.batchId)
    if (!seed) {
      return {
        operationId: `op_miss_${Date.now()}`,
        status: "NOT_COMMITTED",
        nextAction: "检查批次身份",
        message: "批次不存在",
      }
    }
    const projected = projectBatch(seed, role)
    const newHash = `scp_inv_${Date.now().toString(36)}`
    const sales = projected.confirmations.sales
    const finance = projected.confirmations.finance
    batchOverlays.set(command.batchId, {
      ...(batchOverlays.get(command.batchId) ?? {}),
      scopeHash: newHash,
      confirmations: {
        sales: {
          ...sales,
          state: sales.state === "MISSING" ? "MISSING" : "INVALIDATED",
          invalidatedReason: `scopeHash 变化（${projected.scopeHash} → ${newHash}），旧确认失效`,
          priorAudit:
            sales.confirmedBy && sales.confirmedAt && sales.subjectHash
              ? {
                  confirmedBy: sales.confirmedBy,
                  confirmedAt: sales.confirmedAt,
                  subjectHash: sales.subjectHash,
                }
              : sales.priorAudit,
        },
        finance: {
          ...finance,
          state: finance.state === "MISSING" ? "MISSING" : "INVALIDATED",
          invalidatedReason: "范围摘要变化使财务分面确认失效",
          priorAudit:
            finance.confirmedBy && finance.confirmedAt && finance.subjectHash
              ? {
                  confirmedBy: finance.confirmedBy,
                  confirmedAt: finance.confirmedAt,
                  subjectHash: finance.subjectHash,
                }
              : finance.priorAudit,
        },
        baseline: {
          ...projected.confirmations.baseline,
          state:
            projected.confirmations.baseline.state === "VALID"
              ? "INVALIDATED"
              : projected.confirmations.baseline.state,
          invalidatedReason:
            projected.confirmations.baseline.state === "VALID"
              ? "范围变化使基线确认失效"
              : undefined,
        },
      },
      objectVersion: `ov-inv-${Date.now()}`,
    })
    const result: MigrationFormalResult = {
      operationId: `op_inv_${Date.now()}`,
      batchId: command.batchId,
      status: "COMMITTED",
      nextAction: "对应角色重新确认",
      message: "范围摘要已变化，旧确认保留审计并标记失效。",
    }
    formalOps.set(requestId, result)
    return result
  }

  if (
    (command.action === "CONFIRM_SALES" ||
      command.action === "CONFIRM_FINANCE" ||
      command.action === "CONFIRM_BASELINE") &&
    command.batchId
  ) {
    // 职责分离
    if (
      command.action === "CONFIRM_SALES" &&
      role !== "SALES_CONFIRMER"
    ) {
      return deny(
        requestId,
        command.batchId,
        "ADMIN_CANNOT_CONFIRM",
        "当前角色不能代签销售清单确认。"
      )
    }
    if (
      command.action === "CONFIRM_FINANCE" &&
      role !== "FINANCE_CONFIRMER"
    ) {
      return deny(
        requestId,
        command.batchId,
        "ADMIN_CANNOT_CONFIRM",
        "当前角色不能代签财务清单确认。"
      )
    }
    if (
      command.action === "CONFIRM_BASELINE" &&
      role !== "CUTOVER_OWNER"
    ) {
      return deny(
        requestId,
        command.batchId,
        "ADMIN_CANNOT_CONFIRM",
        "仅上线负责人可确认最终权威基线。"
      )
    }

    const seed = BATCH_SEEDS.find((b) => b.identity.batchId === command.batchId)
    if (!seed) {
      return deny(requestId, command.batchId, "NOT_FOUND", "批次不存在")
    }
    const projected = projectBatch(seed, role)
    if (
      command.expectedScopeHash &&
      command.expectedScopeHash !== projected.scopeHash
    ) {
      return deny(
        requestId,
        command.batchId,
        "SCOPE_MISMATCH",
        "提交时 scopeHash 与服务端不一致，确认未写入。"
      )
    }

    if (command.action === "CONFIRM_BASELINE") {
      if (
        !projected.freeze.active ||
        !projected.lastSyncWatermark ||
        !projected.fullReconcileDone
      ) {
        return deny(
          requestId,
          command.batchId,
          "BASELINE_GATES",
          "最终基线仅在冻结 + 最后同步 + 全量核对后可提交。"
        )
      }
    }

    const prev = batchOverlays.get(command.batchId) ?? {}
    const conf = {
      ...projected.confirmations,
      ...(prev.confirmations ?? {}),
    }
    const who =
      role === "SALES_CONFIRMER"
        ? "销售 · 演示确认人"
        : role === "FINANCE_CONFIRMER"
          ? "财务 · 演示确认人"
          : "上线负责人 · 演示确认人"
    const hash = `${command.action.toLowerCase()}_${Date.now().toString(36)}`
    if (command.action === "CONFIRM_SALES") {
      conf.sales = {
        state: "VALID",
        confirmedBy: who,
        confirmedAt: nowIso(),
        subjectHash: hash,
      }
    } else if (command.action === "CONFIRM_FINANCE") {
      conf.finance = {
        state: "VALID",
        confirmedBy: who,
        confirmedAt: nowIso(),
        subjectHash: hash,
      }
    } else {
      conf.baseline = {
        state: "VALID",
        confirmedBy: who,
        confirmedAt: nowIso(),
        subjectHash: hash,
        lastSyncWatermark: projected.lastSyncWatermark,
      }
    }

    batchOverlays.set(command.batchId, {
      ...prev,
      confirmations: conf,
      status:
        command.action === "CONFIRM_BASELINE"
          ? "BASELINE_CONFIRMED"
          : prev.status ?? projected.status,
      stage:
        command.action === "CONFIRM_BASELINE"
          ? "EXECUTION"
          : prev.stage ?? projected.stage,
      objectVersion: `ov-conf-${Date.now()}`,
      allowedActions:
        command.action === "CONFIRM_BASELINE"
          ? ["EXECUTE_BATCH"]
          : projected.allowedActions,
    })

    const result: MigrationFormalResult = {
      operationId: `op_conf_${Date.now()}`,
      batchId: command.batchId,
      status: "COMMITTED",
      committedAt: nowIso(),
      batchStatus:
        command.action === "CONFIRM_BASELINE"
          ? "BASELINE_CONFIRMED"
          : projected.status,
      nextAction:
        command.action === "CONFIRM_BASELINE"
          ? "可由系统管理员执行迁移批次"
          : "继续其它确认或推进冻结",
      message: "确认已与任务完成在同一事务落地（演示）。",
    }
    formalOps.set(requestId, result)
    return result
  }

  if (
    (command.action === "EXECUTE_BATCH" || command.action === "RESUME_BATCH") &&
    command.batchId
  ) {
    if (role !== "SYSTEM_ADMIN") {
      return deny(
        requestId,
        command.batchId,
        "ROLE_MISMATCH",
        "仅系统管理员可执行或续跑客户批次。"
      )
    }
    const seed = BATCH_SEEDS.find((b) => b.identity.batchId === command.batchId)
    if (!seed) {
      return deny(requestId, command.batchId, "NOT_FOUND", "批次不存在")
    }
    const projected = projectBatch(seed, role)
    const salesOk = projected.confirmations.sales.state === "VALID"
    const finOk = projected.confirmations.finance.state === "VALID"
    const baseOk = projected.confirmations.baseline.state === "VALID"
    if (!salesOk || !finOk || !baseOk) {
      return deny(
        requestId,
        command.batchId,
        "CONFIRMATIONS_REQUIRED",
        "三类确认须全部有效且对象摘要一致。"
      )
    }
    if (!projected.freeze.active) {
      return deny(
        requestId,
        command.batchId,
        "FREEZE_REQUIRED",
        "维护冻结未生效，不能执行。"
      )
    }

    // 演示：蓝海失败批次续跑成功；其它可执行批次成功
    const willSucceed =
      command.action === "RESUME_BATCH" ||
      projected.status === "BASELINE_CONFIRMED" ||
      projected.identity.batchId === "omb_lh_003"

    if (projected.identity.batchId === "omb_lh_003" && command.action === "EXECUTE_BATCH") {
      // 保持失败语义（若尚未续跑）
      if (projected.status === "FAILED") {
        return deny(
          requestId,
          command.batchId,
          "USE_RESUME",
          "失败批次请使用原批次续跑。"
        )
      }
    }

    if (!willSucceed && projected.status === "FAILED") {
      const result: MigrationFormalResult = {
        operationId: `op_exec_${Date.now()}`,
        batchId: command.batchId,
        status: "NOT_COMMITTED",
        batchStatus: "FAILED",
        nextAction: "修复后原批次续跑",
        message: "本批未提交，维护冻结仍有效。无部分成功语义。",
      }
      formalOps.set(requestId, result)
      return result
    }

    const eligible = projected.items.filter(
      (i) =>
        i.itemStatus !== "EXCLUDED_DRAFT" &&
        i.itemStatus !== "EXCLUDED_VOIDED"
    )
    batchOverlays.set(command.batchId, {
      ...(batchOverlays.get(command.batchId) ?? {}),
      status: "COMPLETED",
      stage: "COMPLETE",
      counts: {
        ...projected.counts,
        migrated: eligible.length,
        blocked: 0,
      },
      items: projected.items.map((i) =>
        i.itemStatus === "EXCLUDED_DRAFT" ||
        i.itemStatus === "EXCLUDED_VOIDED"
          ? i
          : {
              ...i,
              itemStatus: "MIGRATED" as const,
              errorSummary: undefined,
            }
      ),
      formalResult: {
        status: "COMMITTED",
        title: "本批已全部提交",
        description: projected.successSemanticsNote,
        operationId: `op_exec_${Date.now()}`,
        committedAt: nowIso(),
      },
      backgroundOperation: {
        operationId: `op_exec_${Date.now()}`,
        status: "succeeded",
        progressLabel: "后台执行结束 · 正式结果：全批已提交（非项目级部分成功）",
        progressPercent: 100,
        startedAt: nowIso(),
        lastProgressAt: nowIso(),
      },
      freeze: {
        ...projected.freeze,
        active: false,
        scopeLabel: "本批完成；客户级冻结解除",
      },
      objectVersion: `ov-done-${Date.now()}`,
      allowedActions: [],
      actionBlockers: [
        {
          action: "EXECUTE_BATCH",
          code: "ALREADY_COMPLETED",
          message: "批次已完成；不提供恢复商城主责。",
        },
      ],
    })

    const result: MigrationFormalResult = {
      operationId: `op_exec_${Date.now()}`,
      batchId: command.batchId,
      status: "COMMITTED",
      batchStatus: "COMPLETED",
      committedAt: nowIso(),
      nextAction: "查看商城级切换总览",
      message: projected.successSemanticsNote,
    }
    formalOps.set(requestId, result)
    return result
  }

  if (command.action === "RUN_FINAL_SYNC" && command.batchId) {
    if (role !== "SYSTEM_ADMIN" && role !== "CUTOVER_OWNER") {
      return deny(
        requestId,
        command.batchId,
        "ROLE_MISMATCH",
        "无权执行最后一期同步。"
      )
    }
    const seed = BATCH_SEEDS.find((b) => b.identity.batchId === command.batchId)
    if (!seed) {
      return deny(requestId, command.batchId, "NOT_FOUND", "批次不存在")
    }
    const projected = projectBatch(seed, role)
    if (!projected.freeze.active) {
      return deny(
        requestId,
        command.batchId,
        "FREEZE_REQUIRED",
        "须先启动维护冻结。"
      )
    }
    batchOverlays.set(command.batchId, {
      ...(batchOverlays.get(command.batchId) ?? {}),
      lastSyncWatermark: `wm_final_${Date.now().toString(36)}`,
      fullReconcileDone: true,
      stage: "BASELINE",
      objectVersion: `ov-sync-${Date.now()}`,
    })
    const result: MigrationFormalResult = {
      operationId: `op_sync_${Date.now()}`,
      batchId: command.batchId,
      status: "COMMITTED",
      nextAction: "上线负责人确认最终权威基线",
      message: "最后一期同步与全量核对完成；基线确认不产生新销售版本。",
    }
    formalOps.set(requestId, result)
    return result
  }

  if (command.action === "RECHECK_SCOPE" && command.batchId) {
    const seed = BATCH_SEEDS.find((b) => b.identity.batchId === command.batchId)
    if (!seed) {
      return deny(requestId, command.batchId, "NOT_FOUND", "批次不存在")
    }
    const projected = projectBatch(seed, role)
    const newHash = `scp_re_${Date.now().toString(36)}`
    batchOverlays.set(command.batchId, {
      ...(batchOverlays.get(command.batchId) ?? {}),
      scopeHash: newHash,
      objectVersion: `ov-re-${Date.now()}`,
    })
    const result: MigrationFormalResult = {
      operationId: `op_re_${Date.now()}`,
      batchId: command.batchId,
      status: "COMMITTED",
      nextAction:
        newHash !== projected.scopeHash
          ? "如确认对象变化，对应确认将失效"
          : "范围未变",
      message: `预检完成，当前 scopeHash=${newHash}`,
    }
    formalOps.set(requestId, result)
    return result
  }

  if (command.action === "ENABLE_CUTOVER") {
    if (role !== "CUTOVER_OWNER") {
      return deny(
        requestId,
        undefined,
        "ROLE_MISMATCH",
        "仅上线负责人可登记 T。"
      )
    }
    const cutover = await projectCutover(role)
    if (cutover.status === "ENABLED") {
      return {
        operationId: cutover.lastFormalResult?.operationId ?? `op_t_${Date.now()}`,
        cutoverId: cutover.cutoverId,
        status: "COMMITTED",
        enabledAt: cutover.enabledAt,
        nextAction: "只读查看；不可修改 T",
        message: "T 已登记，返回既有记录（幂等）。",
      }
    }
    const allPass = cutover.prerequisites.every((p) => p.passed)
    const tailsPass = cutover.checks
      .filter((c) => c.isCurrentTail)
      .every((c) => c.checkStatus === "PASSED")
    if (!allPass || !tailsPass || !cutover.allBatchesCompleted) {
      return deny(
        requestId,
        undefined,
        "PREREQUISITES_NOT_MET",
        "前提检查链尾未全部通过，无法登记 T。"
      )
    }
    const enabledAt = nowIso()
    cutoverOverlay = {
      status: "ENABLED",
      enabledAt,
      enabledBy: "上线负责人 · 演示确认人",
      confirmationDigest: `cd_${Date.now().toString(36)}`,
      objectVersion: `cv-enabled-${Date.now()}`,
      allowedActions: [],
      actionBlockers: [
        {
          action: "ENABLE_CUTOVER",
          code: "T_IMMUTABLE",
          message: "T 一经登记不可修改或删除。",
        },
      ],
      lastFormalResult: {
        status: "COMMITTED",
        operationId: `op_t_${Date.now()}`,
        message: "唯一消费回流启用时间 T 已原子登记",
      },
    }
    const result: MigrationFormalResult = {
      operationId: `op_t_${Date.now()}`,
      cutoverId: cutover.cutoverId,
      status: "COMMITTED",
      enabledAt,
      committedAt: enabledAt,
      nextAction: "T 后支付进入自动供应商履约；历史回填见 W30",
      message: "唯一 T 已登记，不可修改或删除。",
    }
    formalOps.set(requestId, result)
    return result
  }

  if (command.action === "CREATE_BATCH") {
    return {
      operationId: `op_create_${Date.now()}`,
      status: "NOT_COMMITTED",
      nextAction: "使用现有演示批次",
      message:
        "演示环境使用固定种子批次；创建须明确唯一客户且正式范围已预检。",
    }
  }

  return {
    operationId: `op_unknown_${Date.now()}`,
    status: "NOT_COMMITTED",
    nextAction: "检查动作",
    message: `未处理动作 ${command.action}`,
  }
}

function deny(
  requestId: string,
  batchId: string | undefined,
  code: string,
  message: string
): MigrationFormalResult {
  const result: MigrationFormalResult = {
    operationId: `op_deny_${code}_${Date.now()}`,
    batchId,
    status: "NOT_COMMITTED",
    nextAction: "根据阻断原因处理",
    message: `${message}（${code}）`,
  }
  formalOps.set(requestId, result)
  return result
}

/** 演示：切换 cutover 为「可登记 T」前提全过 */
export async function enableCutoverDemoReady(): Promise<void> {
  await mockDelay(40)
  cutoverDemoReady = true
}

export function confirmationStateLabel(state: ConfirmationState) {
  if (state === "VALID") return "已确认"
  if (state === "INVALIDATED") return "已失效"
  return "待确认"
}
