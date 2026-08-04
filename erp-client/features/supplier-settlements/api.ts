/**
 * W27 session-mock API
 * - 金额/差异方向/是否可确认均由 mock 投影给出
 * - 岗位分离：采购只证据；经办结论；另一名复核人确认
 * - 策略缺失 fail-closed；UNKNOWN 可按幂等键查询
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  AppendEvidenceInput,
  CreateDraftInput,
  DemoRole,
  FormalOutcome,
  RefreshDraftInput,
  ResolveDifferenceInput,
  ReviewDecisionInput,
  SettlementDetailView,
  SettlementListRow,
  SettlementListView,
  SettlementStatus,
  SettlementView,
  SubmitReviewInput,
  DifferenceType,
} from "@/features/supplier-settlements/types"
import {
  ACTORS,
  DEMO_ROLE_LABEL,
  DIFF_TYPE_LABEL,
  RESOLUTION_LABEL,
  STATUS_LABEL,
  STATUS_TONE,
  roleToActor,
  roleToUserId,
} from "@/features/supplier-settlements/types"
import {
  DEFAULT_PERIOD_POLICY,
  DEFAULT_REFRESH_CUTOFF,
  SEED_STATEMENTS,
  SUPPLIERS,
  projectDifference,
  statusMeta,
  withDirection,
  type SeedStatement,
} from "@/mock/supplier-settlements"
import {
  getIdempotencyEntry,
  setIdempotencyPending,
  setIdempotencySucceeded,
} from "@/mock/session-state"

const WORKSPACE = "W27"

type Overlay = Partial<
  Pick<
    SeedStatement,
    | "status"
    | "lockVersion"
    | "subjectHash"
    | "sourceAsOf"
    | "sourceSnapshotAt"
    | "sourceSnapshotHash"
    | "differences"
    | "reviewRecords"
    | "auditEvents"
    | "payable"
    | "workItem"
    | "pendingCostDeltaGross"
    | "confirmedCostDeltaGross"
    | "erpAmountGross"
    | "supplierAmountGross"
    | "differenceAmountGross"
    | "items"
    | "preparedBy"
    | "reviewedBy"
    | "externalBillNo"
    | "externalBillVersion"
  >
>

const overlays = new Map<string, Overlay>()
const created: SeedStatement[] = []
let forceRefreshCutoffMissing = false

export type ListQueryInput = {
  view: SettlementView
  supplierId?: string
  periodFrom?: string
  periodTo?: string
  status?: string
  differenceType?: DifferenceType
  q?: string
  page: number
  pageSize?: number
  role: DemoRole
  demoFlag?: "no-permission" | "no-scope" | "policy-missing"
}

function mergeSeed(seed: SeedStatement): SeedStatement {
  const o = overlays.get(seed.statementId)
  if (!o) return seed
  return { ...seed, ...o }
}

function allSeeds(): SeedStatement[] {
  return [...created, ...SEED_STATEMENTS].map(mergeSeed)
}

function findSeed(id: string): SeedStatement | undefined {
  return allSeeds().find((s) => s.statementId === id)
}

function ensureOverlay(id: string, seed: SeedStatement): Overlay {
  let o = overlays.get(id)
  if (!o) {
    o = { lockVersion: seed.lockVersion }
    overlays.set(id, o)
  }
  return o
}

function directionLabel(diff?: string): string | undefined {
  if (diff == null) return undefined
  const n = Number(diff)
  if (!Number.isFinite(n) || n === 0) return "无差异"
  if (n > 0) return "供应商账单高于 ERP"
  return "ERP 高于供应商账单"
}

function periodPolicy(demoFlag?: ListQueryInput["demoFlag"]) {
  if (demoFlag === "policy-missing") {
    return {
      state: "UNCONFIGURED" as const,
      blocker: {
        action: "CREATE_DRAFT",
        code: "PERIOD_POLICY_UNCONFIGURED",
        message:
          "供应商结算期间策略未配置或已过期；列表可查历史，但不得新建草稿",
      },
    }
  }
  return { ...DEFAULT_PERIOD_POLICY }
}

function refreshCutoffPolicy() {
  if (forceRefreshCutoffMissing) {
    return {
      state: "UNCONFIGURED" as const,
      blocker: {
        action: "SUBMIT_REVIEW",
        code: "REFRESH_CUTOFF_POLICY_UNCONFIGURED",
        message:
          "刷新截止策略未配置，不得提交复核或创建任务；请先完成策略配置",
      },
    }
  }
  return { ...DEFAULT_REFRESH_CUTOFF }
}

function unresolvedCount(seed: SeedStatement): number {
  return seed.differences.filter(
    (d) => d.status !== "RESOLVED" && d.blocking
  ).length
}

function roleActions(
  role: DemoRole,
  seed: SeedStatement
): { allowed: string[]; blockers: SettlementDetailView["actionBlockers"] } {
  const allowed = new Set<string>(["OPEN_CENTER", "VIEW", "OPEN_PREVIEW"])
  const blockers: SettlementDetailView["actionBlockers"] = []
  const isConfirmed = seed.status === "CONFIRMED" || seed.status === "VOIDED"
  const prepId = seed.preparedBy?.userId
  const userId = roleToUserId(role)
  const openBlocking = seed.differences.some(
    (d) => d.blocking && d.status !== "RESOLVED"
  )
  const cutoff = refreshCutoffPolicy()

  if (role === "manager") {
    blockers.push(
      {
        action: "CREATE_DRAFT",
        code: "ROLE_READONLY",
        message: "管理层只读，不可新建或变更结算",
      },
      {
        action: "RESOLVE_DIFFERENCE",
        code: "ROLE_READONLY",
        message: "管理层不可登记差异结论",
      },
      {
        action: "CONFIRM",
        code: "ROLE_READONLY",
        message: "管理层不可确认结算",
      }
    )
    if (seed.payable) allowed.add("OPEN_W12")
    return { allowed: [...allowed], blockers }
  }

  if (role === "procurement") {
    allowed.add("APPEND_EVIDENCE")
    if (!isConfirmed) {
      // ok
    } else {
      blockers.push({
        action: "APPEND_EVIDENCE",
        code: "CONFIRMED_READONLY",
        message: "已确认结算永久只读，不可再追加证据",
      })
    }
    blockers.push(
      {
        action: "RESOLVE_DIFFERENCE",
        code: "ROLE_PROCUREMENT",
        message: "采购只能追加证据与业务意见，不能选择差异结论",
      },
      {
        action: "SUBMIT_REVIEW",
        code: "ROLE_PROCUREMENT",
        message: "采购不可提交复核",
      },
      {
        action: "CONFIRM",
        code: "ROLE_PROCUREMENT",
        message: "采购不可确认结算",
      },
      {
        action: "CREATE_DRAFT",
        code: "ROLE_PROCUREMENT",
        message: "仅财务经办可新建结算草稿",
      }
    )
    return { allowed: [...allowed], blockers }
  }

  if (role === "finance_prep") {
    allowed.add("CREATE_DRAFT")
    if (
      seed.status === "DRAFT" ||
      seed.status === "PENDING_RECONCILE" ||
      seed.status === "HAS_DIFFERENCE"
    ) {
      allowed.add("REFRESH_TRIAL")
      allowed.add("RESOLVE_DIFFERENCE")
      allowed.add("SUBMIT_REVIEW")
      allowed.add("VOID_DRAFT")
    }
    if (openBlocking) {
      blockers.push({
        action: "SUBMIT_REVIEW",
        code: "BLOCKING_DIFFERENCES",
        message: "存在未处理的阻断差异，须先完成受控结论后方可提交复核",
      })
      allowed.delete("SUBMIT_REVIEW")
    }
    if (cutoff.state === "UNCONFIGURED") {
      blockers.push(cutoff.blocker)
      allowed.delete("SUBMIT_REVIEW")
    }
    blockers.push(
      {
        action: "CONFIRM",
        code: "ROLE_PREP_NOT_REVIEW",
        message: "经办人不能复核并确认自己准备的结算单（岗位分离）",
      },
      {
        action: "APPEND_EVIDENCE",
        code: "ROLE_PREP",
        message: "采购协同证据由被指派采购追加；经办登记结论",
      }
    )
    if (isConfirmed) {
      allowed.delete("REFRESH_TRIAL")
      allowed.delete("RESOLVE_DIFFERENCE")
      allowed.delete("SUBMIT_REVIEW")
      allowed.delete("VOID_DRAFT")
      blockers.push({
        action: "RESOLVE_DIFFERENCE",
        code: "CONFIRMED_READONLY",
        message: "已确认结算不可再处理差异或改写金额",
      })
    }
    if (seed.payable) allowed.add("OPEN_W12")
    return { allowed: [...allowed], blockers }
  }

  // finance_review
  if (seed.status === "PENDING_REVIEW" && seed.workItem) {
    if (prepId && prepId === userId) {
      blockers.push({
        action: "CONFIRM",
        code: "SOD_VIOLATION",
        message: "经办与复核不能为同一人；当前用户是本单经办人",
      })
      blockers.push({
        action: "REJECT",
        code: "SOD_VIOLATION",
        message: "经办与复核不能为同一人",
      })
    } else {
      allowed.add("CONFIRM")
      allowed.add("REJECT")
      allowed.add("CLAIM_REVIEW")
    }
  } else {
    blockers.push({
      action: "CONFIRM",
      code: "NOT_IN_REVIEW",
      message: "仅待复核且已领取任务的结算单可由复核人确认",
    })
  }
  if (openBlocking) {
    blockers.push({
      action: "CONFIRM",
      code: "BLOCKING_DIFFERENCES",
      message: "存在未解决阻断差异，不能确认结算",
    })
    allowed.delete("CONFIRM")
  }
  blockers.push(
    {
      action: "RESOLVE_DIFFERENCE",
      code: "ROLE_REVIEW",
      message: "复核人不可改写差异结论；驳回后由经办处理",
    },
    {
      action: "CREATE_DRAFT",
      code: "ROLE_REVIEW",
      message: "复核人不可新建草稿",
    }
  )
  if (seed.payable) allowed.add("OPEN_W12")
  if (isConfirmed && seed.payable) {
    // read only confirmed
  }
  return { allowed: [...allowed], blockers }
}

function toListRow(seed: SeedStatement, role: DemoRole): SettlementListRow {
  const { allowed, blockers } = roleActions(role, seed)
  const meta = statusMeta(seed.status)
  const dir = withDirection(seed)
  return {
    statementId: seed.statementId,
    statementNo: seed.statementNo,
    supplierId: seed.supplierId,
    supplierName: seed.supplierName,
    periodStart: seed.periodStart,
    periodEnd: seed.periodEnd,
    periodLabel: seed.periodLabel,
    status: seed.status,
    statusLabel: meta.statusLabel,
    statusTone: meta.statusTone,
    erpAmountGross: seed.erpAmountGross,
    supplierAmountGross: seed.supplierAmountGross,
    differenceAmountGross: seed.differenceAmountGross,
    differenceDirectionLabel: dir.differenceDirectionLabel,
    unresolvedDifferenceCount: unresolvedCount(seed),
    preparedBy: seed.preparedBy,
    reviewedBy: seed.reviewedBy,
    preparedByLabel: seed.preparedBy?.displayName ?? "—",
    reviewedByLabel: seed.reviewedBy?.displayName ?? "待复核人",
    updatedAt: seed.sourceSnapshotAt,
    allowedActions: allowed,
    actionBlockers: blockers,
  }
}

function toDetail(seed: SeedStatement, role: DemoRole): SettlementDetailView {
  const { allowed, blockers } = roleActions(role, seed)
  const meta = statusMeta(seed.status)
  const dir = withDirection(seed)
  const diffs = seed.differences.map(projectDifference)
  const open = diffs.filter((d) => d.status !== "RESOLVED").length
  const blocking = diffs.filter(
    (d) => d.blocking && d.status !== "RESOLVED"
  ).length
  const resolved = diffs.filter((d) => d.status === "RESOLVED").length

  return {
    statement: {
      id: seed.statementId,
      statementNo: seed.statementNo,
      supplierId: seed.supplierId,
      supplierName: seed.supplierName,
      periodStart: seed.periodStart,
      periodEnd: seed.periodEnd,
      periodLabel: seed.periodLabel,
      externalBillNo: seed.externalBillNo,
      externalBillVersion: seed.externalBillVersion,
      erpAmountGross: seed.erpAmountGross,
      supplierAmountGross: seed.supplierAmountGross,
      differenceAmountGross: seed.differenceAmountGross,
      differenceDirectionLabel: dir.differenceDirectionLabel,
      status: seed.status,
      statusLabel: meta.statusLabel,
      statusTone: meta.statusTone,
      preparedBy: seed.preparedBy,
      reviewedBy: seed.reviewedBy,
      lockVersion: seed.lockVersion,
      subjectHash: seed.subjectHash,
      sourceAsOf: seed.sourceAsOf,
      sourceSnapshotAt: seed.sourceSnapshotAt,
      sourceSnapshotHash: seed.sourceSnapshotHash,
    },
    totals: {
      orderAmountGross: seed.orderAmountGross,
      freightGross: seed.freightGross,
      serviceFeeGross: seed.serviceFeeGross,
      refundGross: seed.refundGross,
      erpAmountGross: seed.erpAmountGross,
      supplierAmountGross: seed.supplierAmountGross,
      differenceAmountGross: seed.differenceAmountGross,
      differenceDirectionLabel: dir.differenceDirectionLabel,
      taxBasisLabel: "含税",
      pendingCostDeltaGross: seed.pendingCostDeltaGross,
      confirmedCostDeltaGross: seed.confirmedCostDeltaGross,
    },
    items: seed.items,
    differences: diffs,
    differenceSummary: {
      total: diffs.length,
      open,
      blocking,
      resolved,
    },
    reviewRecords: seed.reviewRecords,
    payable: seed.payable
      ? {
          ...seed.payable,
          w12Href: `/finance/supplier-accounts?view=payable&sourceType=SUPPLIER_SETTLEMENT&q=${encodeURIComponent(seed.payable.payableNo)}`,
        }
      : undefined,
    workItem: seed.workItem
      ? {
          workItemId: seed.workItem.workItemId,
          workItemType: "SUPPLIER_SETTLEMENT_REVIEW",
          subjectVersion: seed.workItem.subjectVersion,
          subjectHash: seed.workItem.subjectHash,
          claimedBy: seed.workItem.claimedBy,
          leaseVersion: seed.workItem.leaseVersion,
        }
      : undefined,
    refreshCutoffPolicy: refreshCutoffPolicy(),
    periodPolicy: periodPolicy(),
    auditEvents: seed.auditEvents,
    allowedActions: allowed,
    actionBlockers: blockers,
    freshness: {
      immutableFactsAsOf: seed.sourceAsOf,
      externalBillAsOf: seed.externalBillVersion
        ? seed.sourceAsOf
        : undefined,
      w26ProjectionUpdatedAt: "2026-08-01T07:55:00+08:00",
      queriedAt: new Date().toISOString(),
    },
    viewerRole: role,
    viewerRoleLabel: DEMO_ROLE_LABEL[role],
    viewerUserId: roleToUserId(role),
    canEditBillOrOrder: false,
  }
}

export async function fetchSettlementList(
  input: ListQueryInput
): Promise<SettlementListView> {
  await mockDelay()
  const queriedAt = new Date().toISOString()
  const sourceAsOf = "2026-08-01T10:00:00+08:00"
  const emptyBase = {
    view: input.view,
    rows: [] as SettlementListRow[],
    page: 1,
    pageSize: input.pageSize ?? 50,
    total: 0,
    totals: {
      pendingReconcile: 0,
      hasDifference: 0,
      pendingReview: 0,
      confirmedAmountThisPeriod: "0.00",
    },
    metrics: {
      pending: 0,
      hasDifference: 0,
      pendingReview: 0,
      confirmedAmount: "0.00",
    },
    periodPolicy: periodPolicy(input.demoFlag),
    suppliers: SUPPLIERS.map((s) => ({ ...s })),
    viewerRole: input.role,
    viewerRoleLabel: DEMO_ROLE_LABEL[input.role],
    viewerUserId: roleToUserId(input.role),
    permissionVersion: "pv_w27_1",
    sourceAsOf,
    queriedAt,
    filterSummary: "",
  }

  if (input.demoFlag === "no-permission") {
    return {
      ...emptyBase,
      emptyReason: "NO_PERMISSION",
      hasModulePermission: false,
      hasDataScope: false,
    }
  }
  if (input.demoFlag === "no-scope") {
    return {
      ...emptyBase,
      emptyReason: "NO_SCOPE",
      hasModulePermission: true,
      hasDataScope: false,
    }
  }

  let seeds = allSeeds()
  const metricsBase = seeds
  const metrics = {
    pending: metricsBase.filter(
      (s) =>
        s.status === "DRAFT" ||
        s.status === "PENDING_RECONCILE" ||
        s.status === "HAS_DIFFERENCE" ||
        s.status === "PENDING_REVIEW"
    ).length,
    hasDifference: metricsBase.filter((s) => s.status === "HAS_DIFFERENCE")
      .length,
    pendingReview: metricsBase.filter((s) => s.status === "PENDING_REVIEW")
      .length,
    confirmedAmount: metricsBase
      .filter((s) => s.status === "CONFIRMED")
      .reduce((acc, s) => acc + Number(s.erpAmountGross || 0), 0)
      .toFixed(2),
  }
  const totals = {
    pendingReconcile: metricsBase.filter(
      (s) => s.status === "PENDING_RECONCILE" || s.status === "DRAFT"
    ).length,
    hasDifference: metrics.hasDifference,
    pendingReview: metrics.pendingReview,
    confirmedAmountThisPeriod: metrics.confirmedAmount,
  }

  // view
  if (input.view === "pending") {
    seeds = seeds.filter(
      (s) =>
        s.status === "DRAFT" ||
        s.status === "PENDING_RECONCILE" ||
        s.status === "HAS_DIFFERENCE" ||
        s.status === "PENDING_REVIEW"
    )
  } else if (input.view === "confirmed") {
    seeds = seeds.filter((s) => s.status === "CONFIRMED")
  } else if (input.view === "prepared_by_me") {
    const uid = roleToUserId(input.role)
    seeds = seeds.filter((s) => s.preparedBy?.userId === uid)
  } else if (input.view === "review_by_me") {
    seeds = seeds.filter(
      (s) =>
        s.status === "PENDING_REVIEW" ||
        s.reviewedBy?.userId === roleToUserId(input.role)
    )
  }

  if (input.supplierId) {
    seeds = seeds.filter((s) => s.supplierId === input.supplierId)
  }
  if (input.periodFrom) {
    seeds = seeds.filter((s) => s.periodStart >= input.periodFrom!)
  }
  if (input.periodTo) {
    seeds = seeds.filter((s) => s.periodEnd <= input.periodTo!)
  }
  if (input.status) {
    const set = new Set(
      input.status.split(",").map((x) => x.trim().toUpperCase())
    )
    seeds = seeds.filter((s) => set.has(s.status))
  }
  if (input.differenceType) {
    seeds = seeds.filter((s) =>
      s.differences.some(
        (d) =>
          d.type === input.differenceType && d.status !== "RESOLVED"
      )
    )
  }
  if (input.q?.trim()) {
    const q = input.q.trim().toUpperCase()
    seeds = seeds.filter(
      (s) =>
        s.statementNo.toUpperCase().includes(q) ||
        s.supplierName.toUpperCase().includes(q) ||
        (s.externalBillNo?.toUpperCase().includes(q) ?? false)
    )
  }

  // sort: HAS_DIFFERENCE first, then period end asc, id
  seeds = [...seeds].sort((a, b) => {
    const rank = (s: SeedStatement) =>
      s.status === "HAS_DIFFERENCE"
        ? 0
        : s.status === "PENDING_REVIEW"
          ? 1
          : s.status === "PENDING_RECONCILE" || s.status === "DRAFT"
            ? 2
            : 3
    const dr = rank(a) - rank(b)
    if (dr !== 0) return dr
    const pe = a.periodEnd.localeCompare(b.periodEnd)
    if (pe !== 0) return pe
    return a.statementId.localeCompare(b.statementId)
  })

  const pageSize = input.pageSize ?? 50
  const page = Math.max(1, input.page)
  const total = seeds.length
  const start = (page - 1) * pageSize
  const pageSeeds = seeds.slice(start, start + pageSize)
  const rows = pageSeeds.map((s) => toListRow(s, input.role))

  let emptyReason: SettlementListView["emptyReason"]
  if (total === 0) {
    const any = allSeeds().length > 0
    emptyReason = any ? "FILTER_NO_RESULT" : "NO_STATEMENTS"
  }

  const filterParts = [
    input.view !== "pending" ? `视图=${input.view}` : null,
    input.supplierId
      ? `供应商=${SUPPLIERS.find((s) => s.supplierId === input.supplierId)?.supplierName ?? input.supplierId}`
      : null,
    input.status ? `状态=${input.status}` : null,
    input.differenceType
      ? `差异=${DIFF_TYPE_LABEL[input.differenceType]}`
      : null,
    input.q ? `搜索=${input.q}` : null,
  ].filter(Boolean)

  return {
    view: input.view,
    rows,
    page,
    pageSize,
    total,
    totals,
    metrics,
    periodPolicy: periodPolicy(input.demoFlag),
    suppliers: SUPPLIERS.map((s) => ({ ...s })),
    emptyReason,
    hasModulePermission: true,
    hasDataScope: true,
    viewerRole: input.role,
    viewerRoleLabel: DEMO_ROLE_LABEL[input.role],
    viewerUserId: roleToUserId(input.role),
    permissionVersion: "pv_w27_1",
    sourceAsOf,
    queriedAt,
    filterSummary: filterParts.length
      ? filterParts.join(" · ")
      : "默认待处理视图",
  }
}

export async function fetchSettlementDetail(input: {
  statementId: string
  role: DemoRole
}): Promise<SettlementDetailView | null> {
  await mockDelay()
  const seed = findSeed(input.statementId)
  if (!seed) return null
  return toDetail(seed, input.role)
}

export async function createSettlementDraft(
  input: CreateDraftInput
): Promise<FormalOutcome> {
  await mockDelay(120)
  if (input.role !== "finance_prep") {
    return {
      status: "blocked",
      code: "FORBIDDEN",
      title: "无权新建草稿",
      message: "仅财务经办可在期间策略已配置时新建结算草稿",
    }
  }
  const policy = periodPolicy()
  if (policy.state !== "CONFIGURED") {
    return {
      status: "blocked",
      code: "PERIOD_POLICY_UNCONFIGURED",
      title: "期间策略未配置",
      message: policy.blocker.message,
    }
  }
  if (
    input.periodPolicyId !== policy.policyId ||
    input.expectedPeriodPolicyVersion !== policy.policyVersion
  ) {
    return {
      status: "blocked",
      code: "PERIOD_POLICY_STALE",
      title: "期间策略版本过期",
      message:
        "当前使用的策略版本已更新，已拒绝创建草稿。请重新加载策略后选择完整周期。",
    }
  }
  const periodOk = policy.selectablePeriods.some(
    (p) =>
      p.periodStart === input.periodStart && p.periodEnd === input.periodEnd
  )
  if (!periodOk) {
    return {
      status: "blocked",
      code: "PERIOD_NOT_IN_POLICY",
      title: "期间不在策略内",
      message: "必须选择策略返回的完整周期，不接受任意自然日拼接",
    }
  }
  const supplier = SUPPLIERS.find((s) => s.supplierId === input.supplierId)
  if (!supplier) {
    return {
      status: "failed",
      code: "SUPPLIER_NOT_FOUND",
      title: "供应商不存在",
      message: "请选择授权范围内的供应商",
    }
  }
  const covered = allSeeds().some(
    (s) =>
      s.supplierId === input.supplierId &&
      s.periodStart === input.periodStart &&
      s.periodEnd === input.periodEnd &&
      s.status !== "VOIDED"
  )
  if (covered) {
    return {
      status: "blocked",
      code: "PERIOD_ALREADY_COVERED",
      title: "期间已被覆盖",
      message: "同一供应商同一结算范围已有有效结算单，不可重复创建",
    }
  }

  const now = new Date().toISOString()
  const statementId = `st_new_${Date.now().toString(36)}`
  const statementNo = `ST-${input.periodStart.slice(0, 7).replace("-", "")}-${supplier.supplierId.replace("sup_", "").toUpperCase()}`
  const snapshotHash = `ssh_new_${Date.now().toString(36)}`
  const seed: SeedStatement = {
    statementId,
    statementNo,
    supplierId: supplier.supplierId,
    supplierName: supplier.supplierName,
    periodStart: input.periodStart,
    periodEnd: input.periodEnd,
    periodLabel: input.periodStart.slice(0, 7),
    status: "DRAFT",
    orderAmountGross: "0.00",
    freightGross: "0.00",
    serviceFeeGross: "0.00",
    refundGross: "0.00",
    erpAmountGross: "0.00",
    preparedBy: roleToActor(input.role),
    lockVersion: 1,
    subjectHash: `sh_${statementId}`,
    sourceAsOf: now,
    sourceSnapshotAt: now,
    sourceSnapshotHash: snapshotHash,
    items: [],
    differences: [],
    reviewRecords: [],
    auditEvents: [
      {
        eventId: `ae_${statementId}`,
        at: now,
        actor: ACTORS.prep.displayName,
        action: "CREATE_DRAFT",
        summary: `按策略 ${policy.policyId}@${policy.policyVersion} 创建草稿 · ${snapshotHash}`,
        auditNo: `AUD-W27-${Date.now().toString().slice(-4)}`,
      },
    ],
  }
  created.unshift(seed)
  setIdempotencySucceeded(input.idempotencyKey, `${WORKSPACE}:CREATE`, {
    statementId,
  })

  return {
    status: "succeeded",
    title: "结算草稿已创建",
    message: "已按策略周期冻结来源数据并形成明细试算（演示为空明细可刷新）。",
    reference: statementNo,
    statementId,
    sourceSnapshotHash: snapshotHash,
    lockVersion: 1,
    facts: [
      { label: "结算单号", value: statementNo },
      { label: "供应商", value: supplier.supplierName },
      {
        label: "期间",
        value: `${input.periodStart} ~ ${input.periodEnd}`,
      },
      { label: "策略版本", value: `${policy.policyId}@${policy.policyVersion}` },
      { label: "sourceSnapshotHash", value: snapshotHash },
      { label: "requestId", value: input.requestId },
    ],
  }
}

export async function refreshSettlementTrial(
  input: RefreshDraftInput
): Promise<FormalOutcome> {
  await mockDelay(140)
  if (input.role !== "finance_prep") {
    return {
      status: "blocked",
      code: "FORBIDDEN",
      title: "无权刷新试算",
      message: "仅财务经办可刷新草稿试算",
    }
  }
  const seed = findSeed(input.statementId)
  if (!seed) {
    return {
      status: "failed",
      code: "NOT_FOUND",
      title: "结算单不存在",
      message: "未找到结算单",
    }
  }
  if (
    seed.status !== "DRAFT" &&
    seed.status !== "PENDING_RECONCILE" &&
    seed.status !== "HAS_DIFFERENCE"
  ) {
    return {
      status: "blocked",
      code: "STATUS_NOT_REFRESHABLE",
      title: "当前状态不可刷新",
      message: "已提交复核或已确认的结算不可刷新试算",
    }
  }
  if (seed.lockVersion !== input.expectedLockVersion) {
    return {
      status: "failed",
      code: "VERSION_CONFLICT",
      title: "数据已更新",
      message: "数据已更新，请刷新后重新加载试算",
    }
  }
  if (seed.sourceSnapshotHash !== input.expectedSourceSnapshotHash) {
    return {
      status: "failed",
      code: "SNAPSHOT_STALE",
      title: "来源数据已过期",
      message:
        "来源数据版本与当前不一致；旧提交已失效，请使用最新数据重试",
    }
  }

  const now = new Date().toISOString()
  const o = ensureOverlay(input.statementId, seed)
  o.lockVersion = seed.lockVersion + 1
  o.sourceAsOf = now
  o.sourceSnapshotAt = now
  o.sourceSnapshotHash = `ssh_refreshed_${Date.now().toString(36)}`
  o.subjectHash = `sh_refreshed_${seed.statementId}_${o.lockVersion}`
  // keep status; bump bill if draft missing bill
  if (!seed.supplierAmountGross) {
    o.supplierAmountGross = seed.erpAmountGross
    o.differenceAmountGross = "0.00"
    o.externalBillNo = o.externalBillNo ?? `BILL-MOCK-${seed.periodLabel}`
    o.externalBillVersion = "v1"
    if (seed.status === "DRAFT") o.status = "PENDING_RECONCILE"
  }
  o.auditEvents = [
    {
      eventId: `ae_ref_${Date.now()}`,
      at: now,
      actor: ACTORS.prep.displayName,
      action: "REFRESH_TRIAL",
      summary: `刷新试算 · 来源时间 ${now} · 数据版本 ${o.sourceSnapshotHash}`,
      auditNo: `AUD-W27-${Date.now().toString().slice(-4)}`,
    },
    ...seed.auditEvents,
  ]

  const result: FormalOutcome = {
    status: "succeeded",
    title: "明细试算已刷新",
    message:
      "已更新草稿试算版本，不修改原订单或供应商账单原值。",
    reference: seed.statementNo,
    statementId: seed.statementId,
    sourceSnapshotHash: o.sourceSnapshotHash,
    subjectHash: o.subjectHash,
    lockVersion: o.lockVersion,
    facts: [
      { label: "来源时间", value: now },
      { label: "数据版本", value: o.sourceSnapshotHash! },
      { label: "版本号", value: String(o.lockVersion) },
      { label: "请求编号", value: input.requestId },
    ],
  }
  setIdempotencySucceeded(input.idempotencyKey, `${WORKSPACE}:REFRESH`, result)
  return result
}

export async function appendDifferenceEvidence(
  input: AppendEvidenceInput
): Promise<FormalOutcome> {
  await mockDelay(100)
  if (input.role !== "procurement") {
    return {
      status: "blocked",
      code: "ROLE_NOT_PROCUREMENT",
      title: "仅采购可追加协同证据",
      message: "采购证据不改变差异结论、试算金额或成本基线",
    }
  }
  const seed = findSeed(input.statementId)
  if (!seed) {
    return {
      status: "failed",
      code: "NOT_FOUND",
      title: "结算单不存在",
      message: "未找到结算单",
    }
  }
  if (seed.status === "CONFIRMED") {
    return {
      status: "blocked",
      code: "CONFIRMED_READONLY",
      title: "已确认只读",
      message: "已确认结算不可追加证据",
    }
  }
  const diff = seed.differences.find((d) => d.differenceId === input.differenceId)
  if (!diff) {
    return {
      status: "failed",
      code: "DIFF_NOT_FOUND",
      title: "差异不存在",
      message: "未找到差异行",
    }
  }
  if (diff.version !== input.expectedDifferenceVersion) {
    return {
      status: "failed",
      code: "DIFF_VERSION_CONFLICT",
      title: "差异数据已更新",
      message: "请刷新后重试",
    }
  }

  const now = new Date().toISOString()
  const o = ensureOverlay(input.statementId, seed)
  o.differences = seed.differences.map((d) => {
    if (d.differenceId !== input.differenceId) return d
    return {
      ...d,
      version: d.version + 1,
      status: d.status === "OPEN" ? "EVIDENCE_PENDING" : d.status,
      evidence: [
        ...d.evidence,
        {
          evidenceId: `ev_${Date.now()}`,
          kind: "PROCUREMENT_OPINION" as const,
          label: input.opinionCode ?? "采购业务意见",
          comment: input.comment,
          by: roleToActor(input.role),
          at: now,
        },
      ],
    }
  })
  o.auditEvents = [
    {
      eventId: `ae_ev_${Date.now()}`,
      at: now,
      actor: ACTORS.procurement.displayName,
      action: "APPEND_EVIDENCE",
      summary: `追加采购证据 · ${DIFF_TYPE_LABEL[diff.type]}（结论未变）`,
      auditNo: `AUD-W27-${Date.now().toString().slice(-4)}`,
    },
    ...seed.auditEvents,
  ]

  const result: FormalOutcome = {
    status: "succeeded",
    title: "采购证据已追加",
    message: "仅追加证据与审计，差异结论、金额与成本基线未改变。",
    reference: input.requestId,
    statementId: seed.statementId,
    facts: [
      { label: "差异", value: DIFF_TYPE_LABEL[diff.type] },
      { label: "说明", value: input.comment ?? "—" },
    ],
  }
  setIdempotencySucceeded(
    input.idempotencyKey,
    `${WORKSPACE}:EVIDENCE`,
    result
  )
  return result
}

export async function resolveDifference(
  input: ResolveDifferenceInput
): Promise<FormalOutcome> {
  await mockDelay(120)
  if (input.role !== "finance_prep") {
    return {
      status: "blocked",
      code: "ROLE_NOT_PREP",
      title: "仅财务经办可登记结论",
      message: "采购证据不能自行改变差异状态或成本基线",
    }
  }
  const seed = findSeed(input.statementId)
  if (!seed) {
    return {
      status: "failed",
      code: "NOT_FOUND",
      title: "结算单不存在",
      message: "未找到结算单",
    }
  }
  if (seed.status === "CONFIRMED" || seed.status === "PENDING_REVIEW") {
    return {
      status: "blocked",
      code: "STATUS_LOCKED",
      title: "当前状态不可处理差异",
      message: "待复核或已确认不可改写差异结论",
    }
  }
  if (seed.lockVersion !== input.expectedLockVersion) {
    return {
      status: "failed",
      code: "VERSION_CONFLICT",
      title: "数据已更新",
      message: "请刷新后基于最新数据提交",
    }
  }
  const diff = seed.differences.find((d) => d.differenceId === input.differenceId)
  if (!diff) {
    return {
      status: "failed",
      code: "DIFF_NOT_FOUND",
      title: "差异不存在",
      message: "未找到差异行",
    }
  }
  if (diff.version !== input.expectedDifferenceVersion) {
    return {
      status: "failed",
      code: "DIFF_VERSION_CONFLICT",
      title: "差异数据已更新",
      message: "请刷新后重试",
    }
  }
  if (
    diff.requiresProcurementEvidence &&
    diff.evidence.length === 0 &&
    input.resolution !== "CLOSED_NO_ADJUSTMENT"
  ) {
    return {
      status: "blocked",
      code: "EVIDENCE_REQUIRED",
      title: "缺少采购证据",
      message: "该差异需采购协同证据齐备后方可登记结论",
    }
  }

  const now = new Date().toISOString()
  const costImpact =
    input.resolution === "SUPPLIER_ACCEPTED" ||
    input.resolution === "COMPENSATED"
      ? diff.amountGross ?? "0.00"
      : "0.00"

  const o = ensureOverlay(input.statementId, seed)
  o.lockVersion = seed.lockVersion + 1
  o.differences = seed.differences.map((d) => {
    if (d.differenceId !== input.differenceId) return d
    return {
      ...d,
      version: d.version + 1,
      status: "RESOLVED" as const,
      blocking: false,
      resolution: {
        resolutionId: `res_${input.operationId}`,
        resolution: input.resolution,
        resolutionLabel: RESOLUTION_LABEL[input.resolution],
        reasonCode: input.reasonCode,
        reasonLabel: input.reasonCode,
        by: roleToActor(input.role),
        at: now,
        costImpactPreview: costImpact,
      },
    }
  })
  const stillBlocking = o.differences!.some(
    (d) => d.blocking && d.status !== "RESOLVED"
  )
  if (!stillBlocking && seed.status === "HAS_DIFFERENCE") {
    o.status = "PENDING_RECONCILE"
  }
  const pending = o.differences!
    .filter((d) => d.resolution)
    .reduce((acc, d) => {
      const v = Number(d.resolution?.costImpactPreview ?? 0)
      return acc + (Number.isFinite(v) ? v : 0)
    }, 0)
  o.pendingCostDeltaGross = pending.toFixed(2)
  o.subjectHash = `sh_diff_${seed.statementId}_${o.lockVersion}`
  o.auditEvents = [
    {
      eventId: `ae_res_${Date.now()}`,
      at: now,
      actor: ACTORS.prep.displayName,
      action: "RESOLVE_DIFFERENCE",
      summary: `${DIFF_TYPE_LABEL[diff.type]} → ${RESOLUTION_LABEL[input.resolution]} · 成本预览 ${costImpact}`,
      auditNo: `AUD-W27-${Date.now().toString().slice(-4)}`,
    },
    ...seed.auditEvents,
  ]

  const result: FormalOutcome = {
    status: "succeeded",
    title: "差异处理结论已登记",
    message:
      "已追加财务处理记录并刷新待确认成本差额；未改写账单原值或历史订单成本。",
    reference: input.operationId,
    statementId: seed.statementId,
    costDeltaGross: costImpact,
    subjectHash: o.subjectHash,
    lockVersion: o.lockVersion,
    facts: [
      { label: "结论", value: RESOLUTION_LABEL[input.resolution] },
      { label: "成本影响预览（含税）", value: costImpact },
      { label: "差异版本", value: String(diff.version + 1) },
    ],
  }
  setIdempotencySucceeded(
    input.idempotencyKey,
    `${WORKSPACE}:RESOLVE`,
    result
  )
  return result
}

export async function submitSettlementReview(
  input: SubmitReviewInput
): Promise<FormalOutcome> {
  await mockDelay(130)
  if (input.role !== "finance_prep") {
    return {
      status: "blocked",
      code: "FORBIDDEN",
      title: "无权提交复核",
      message: "仅财务经办可提交复核",
    }
  }
  const seed = findSeed(input.statementId)
  if (!seed) {
    return {
      status: "failed",
      code: "NOT_FOUND",
      title: "结算单不存在",
      message: "未找到结算单",
    }
  }
  if (seed.lockVersion !== input.expectedLockVersion) {
    return {
      status: "failed",
      code: "VERSION_CONFLICT",
      title: "数据已更新",
      message: "版本变化会使旧提交失效",
    }
  }
  if (seed.subjectHash && seed.subjectHash !== input.subjectHash) {
    return {
      status: "failed",
      code: "SUBJECT_HASH_MISMATCH",
      title: "数据版本不一致",
      message: "数据版本不一致，请刷新后重试",
    }
  }
  const cutoff = refreshCutoffPolicy()
  if (cutoff.state !== "CONFIGURED") {
    return {
      status: "blocked",
      code: "REFRESH_CUTOFF_POLICY_UNCONFIGURED",
      title: "刷新截止策略未配置",
      message: cutoff.blocker.message,
    }
  }
  if (
    input.refreshCutoffPolicyId !== cutoff.policyId ||
    input.expectedRefreshCutoffPolicyVersion !== cutoff.policyVersion
  ) {
    return {
      status: "blocked",
      code: "REFRESH_CUTOFF_STALE",
      title: "截止策略版本过期",
      message: "请重取策略版本后再提交",
    }
  }
  if (
    seed.differences.some((d) => d.blocking && d.status !== "RESOLVED")
  ) {
    return {
      status: "blocked",
      code: "BLOCKING_DIFFERENCES",
      title: "存在阻断差异",
      message: "全部阻断差异须有允许进入复核的处理结论",
    }
  }

  const now = new Date().toISOString()
  const o = ensureOverlay(input.statementId, seed)
  o.lockVersion = seed.lockVersion + 1
  o.status = "PENDING_REVIEW"
  o.subjectHash = input.subjectHash || `sh_sub_${seed.statementId}`
  o.workItem = {
    workItemId: `wi_${seed.statementId}`,
    subjectVersion: String(o.lockVersion),
    subjectHash: o.subjectHash,
    claimedBy: undefined,
    leaseVersion: 0,
  }
  o.reviewRecords = [
    {
      recordId: `rr_sub_${Date.now()}`,
      action: "SUBMIT",
      actionLabel: "提交复核",
      by: roleToActor(input.role),
      at: now,
      comment: input.comment,
    },
    ...seed.reviewRecords,
  ]
  o.auditEvents = [
    {
      eventId: `ae_sub_${Date.now()}`,
      at: now,
      actor: ACTORS.prep.displayName,
      action: "SUBMIT_REVIEW",
      summary: `提交复核 · 截止策略 ${cutoff.policyId}@${cutoff.policyVersion}`,
      auditNo: `AUD-W27-${Date.now().toString().slice(-4)}`,
    },
    ...seed.auditEvents,
  ]

  const result: FormalOutcome = {
    status: "succeeded",
    title: "已提交复核",
    message: "已冻结提交版本并创建 SUPPLIER_SETTLEMENT_REVIEW 待办。",
    reference: o.workItem.workItemId,
    statementId: seed.statementId,
    subjectHash: o.subjectHash,
    lockVersion: o.lockVersion,
    facts: [
      { label: "workItemId", value: o.workItem.workItemId },
      { label: "subjectHash", value: o.subjectHash! },
      {
        label: "截止策略",
        value: `${cutoff.policyId}@${cutoff.policyVersion}`,
      },
    ],
  }
  setIdempotencySucceeded(input.idempotencyKey, `${WORKSPACE}:SUBMIT`, result)
  return result
}

export async function decideSettlementReview(
  input: ReviewDecisionInput
): Promise<FormalOutcome> {
  await mockDelay(150)
  if (input.role !== "finance_review") {
    return {
      status: "blocked",
      code: "ROLE_NOT_REVIEW",
      title: "仅财务复核可决策",
      message: "确认/驳回须使用完整任务流程，且操作人非经办人",
    }
  }
  const seed = findSeed(input.statementId)
  if (!seed) {
    return {
      status: "failed",
      code: "NOT_FOUND",
      title: "结算单不存在",
      message: "未找到结算单",
    }
  }
  if (seed.preparedBy?.userId === roleToUserId(input.role)) {
    return {
      status: "blocked",
      code: "SOD_VIOLATION",
      title: "岗位分离冲突",
      message: "经办与复核不能为同一人",
    }
  }
  if (!seed.workItem || seed.workItem.workItemId !== input.workItemId) {
    return {
      status: "blocked",
      code: "WORK_ITEM_MISMATCH",
      title: "任务不匹配",
      message: "请先领取任务后再提交结论",
    }
  }
  if (seed.lockVersion !== input.expectedLockVersion) {
    return {
      status: "failed",
      code: "VERSION_CONFLICT",
      title: "数据已更新",
      message: "请刷新任务与结算单后重试",
    }
  }
  if (
    seed.workItem.subjectHash !== input.expectedSubjectHash ||
    (seed.subjectHash && seed.subjectHash !== input.expectedSubjectHash)
  ) {
    return {
      status: "failed",
      code: "SUBJECT_HASH_MISMATCH",
      title: "数据已变更",
      message: "提交数据已变化，不能静默确认过期试算",
    }
  }
  if (
    seed.differences.some((d) => d.blocking && d.status !== "RESOLVED")
  ) {
    return {
      status: "blocked",
      code: "BLOCKING_DIFFERENCES",
      title: "存在阻断差异",
      message: "未解决阻断差异不能确认结算",
    }
  }

  if (input.forceUnknown) {
    setIdempotencyPending(input.idempotencyKey, `${WORKSPACE}:REVIEW`)
    return {
      status: "unknown",
      title: "确认结果未知",
      message:
        "不得乐观切换为已确认或重复生成应付。请用同一操作号查询最终结果。",
      operationId: input.operationId,
      idempotencyKey: input.idempotencyKey,
      statementId: seed.statementId,
    }
  }

  const now = new Date().toISOString()
  const o = ensureOverlay(input.statementId, seed)

  if (input.action === "REJECT") {
    if (!input.reasonCode) {
      return {
        status: "failed",
        code: "REASON_REQUIRED",
        title: "驳回原因必填",
        message: "请填写驳回原因",
      }
    }
    o.lockVersion = seed.lockVersion + 1
    o.status = "HAS_DIFFERENCE"
    o.workItem = undefined
    o.reviewRecords = [
      {
        recordId: `rr_rej_${Date.now()}`,
        action: "REJECT",
        actionLabel: "驳回复核",
        by: roleToActor(input.role),
        at: now,
        reasonCode: input.reasonCode,
        comment: input.comment,
      },
      ...seed.reviewRecords,
    ]
    o.auditEvents = [
      {
        eventId: `ae_rej_${Date.now()}`,
        at: now,
        actor: ACTORS.review.displayName,
        action: "REJECT",
        summary: `驳回 · ${
          input.reasonCode === "NEEDS_MORE_EVIDENCE"
            ? "证据不足"
            : input.reasonCode === "AMOUNT_MISMATCH"
              ? "金额仍不一致"
              : "其他"
        }`,
        auditNo: `AUD-W27-${Date.now().toString().slice(-4)}`,
      },
      ...seed.auditEvents,
    ]
    const result: FormalOutcome = {
      status: "rejected",
      title: "已驳回复核",
      message: "已退回经办并保留复核记录；可继续处理差异后重提。",
      reference: input.operationId,
      statementId: seed.statementId,
      facts: [
        {
          label: "原因",
          value:
            input.reasonCode === "NEEDS_MORE_EVIDENCE"
              ? "证据不足"
              : input.reasonCode === "AMOUNT_MISMATCH"
                ? "金额仍不一致"
                : "其他",
        },
        { label: "说明", value: input.comment ?? "—" },
      ],
    }
    setIdempotencySucceeded(input.idempotencyKey, `${WORKSPACE}:REVIEW`, result)
    return result
  }

  // CONFIRM
  const payableNo = `AP-${seed.supplierId.replace("sup_", "").toUpperCase()}-${seed.periodLabel.replace("-", "")}-01`
  const payableAccountId = `pa_${seed.statementId}`
  const costDelta = seed.pendingCostDeltaGross ?? "0.00"
  const payableGross =
    seed.supplierAmountGross ?? seed.erpAmountGross

  o.lockVersion = seed.lockVersion + 1
  o.status = "CONFIRMED"
  o.reviewedBy = roleToActor(input.role)
  o.confirmedCostDeltaGross = costDelta
  o.pendingCostDeltaGross = undefined
  o.workItem = undefined
  o.payable = {
    payableAccountId,
    payableNo,
    grossAmount: payableGross,
    dueDate: "2026-08-20",
    statusLabel: "未结",
  }
  o.reviewRecords = [
    {
      recordId: `rr_cfm_${Date.now()}`,
      action: "CONFIRM",
      actionLabel: "确认结算",
      by: roleToActor(input.role),
      at: now,
      comment: input.comment,
    },
    ...seed.reviewRecords,
  ]
  o.auditEvents = [
    {
      eventId: `ae_cfm_${Date.now()}`,
      at: now,
      actor: ACTORS.review.displayName,
      action: "CONFIRM",
      summary: `确认结算 · 应付 ${payableNo} · 成本差额 ${costDelta}`,
      auditNo: `AUD-W27-${Date.now().toString().slice(-4)}`,
    },
    ...seed.auditEvents,
  ]

  const result: FormalOutcome = {
    status: "succeeded",
    title: "结算已确认",
    message:
      "同一次提交已追加成本差额并形成唯一应付。付款、进项发票与核销请进入供应商往来，本页不复制财务流程。",
    reference: payableNo,
    statementId: seed.statementId,
    payableNo,
    payableAccountId,
    costDeltaGross: costDelta,
    lockVersion: o.lockVersion,
    facts: [
      { label: "应付编号", value: payableNo },
      { label: "应付含税金额", value: payableGross },
      { label: "成本差额（含税）", value: costDelta },
      { label: "确认时间", value: now },
      { label: "operationId", value: input.operationId },
    ],
  }
  setIdempotencySucceeded(input.idempotencyKey, `${WORKSPACE}:REVIEW`, result)
  return result
}

export async function queryFormalByIdempotency(
  key: string
): Promise<FormalOutcome | null> {
  await mockDelay(80)
  const entry = getIdempotencyEntry(key)
  if (!entry) return null
  if (entry.state === "pending") {
    return {
      status: "unknown",
      title: "结果仍未知",
      message: "请稍后按原任务号再次查询，勿换新键重提。",
      idempotencyKey: key,
      operationId: entry.kind,
    }
  }
  if (entry.state === "succeeded" && entry.payload) {
    return entry.payload as FormalOutcome
  }
  return null
}

/** Demo helpers */
export function setRefreshCutoffMissing(missing: boolean) {
  forceRefreshCutoffMissing = missing
}

export type { SettlementStatus }
