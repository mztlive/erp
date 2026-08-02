/**
 * W21 session-mock API：queryFn / mutationFn 纯函数。
 * claimToken 仅出现在领取响应；任务内动作 / 终结复用 W02 会话信封语义。
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  CostFieldVisibility,
  DemoRole,
  DiffChange,
  ExternalCatalogCenterView,
  ExternalCatalogDecision,
  ExternalCatalogItemView,
  ExternalCatalogQueueQuery,
  ExternalCatalogQueueView,
  ExternalCatalogWorkItemAction,
  ExternalProductRevisionView,
  FormalActionResponse,
  FormalOutcome,
  SessionCatalogDraft,
  SupplierOfferingRevisionView,
  WorkItemLease,
} from "@/features/external-product-supply/types"
import {
  COST_MASK,
  DEMO_ROLE_LABEL,
} from "@/features/external-product-supply/types"
import { EXTERNAL_PRODUCT_SUPPLY_SEED } from "@/mock/external-product-supply"
import {
  applyWorkItemActionSession,
  claimWorkItemSession,
  clearSessionLease,
  completeWorkItemSession,
  getCompletedQueueTaskIds,
  getHeldQueueTaskIds,
  getSessionLeaseState,
  getWorkItemActionHistory,
  getWorkItemTerminal,
  isWorkItemHeld,
  markQueueTaskCompleted,
  markQueueTaskHeld,
  queryIdempotencyResult,
  setIdempotencySucceeded,
  WorkItemMockError,
} from "@/mock/session-state"

const drafts = new Map<string, SessionCatalogDraft>()

function maskCostValue(v: string | null | undefined): string | null {
  if (v == null) return null
  return COST_MASK
}

function maskRevision(
  rev: ExternalProductRevisionView,
  mask: boolean
): ExternalProductRevisionView {
  if (!mask) return rev
  return {
    ...rev,
    supplyPriceGross: maskCostValue(rev.supplyPriceGross),
    inputTaxRate: maskCostValue(rev.inputTaxRate),
    freightAmount: maskCostValue(rev.freightAmount),
    otherFeeAmount: maskCostValue(rev.otherFeeAmount),
  }
}

function maskOffering(
  o: SupplierOfferingRevisionView,
  mask: boolean
): SupplierOfferingRevisionView {
  if (!mask) return o
  return {
    ...o,
    supplyPriceGross: maskCostValue(o.supplyPriceGross),
    supplyPriceNet: maskCostValue(o.supplyPriceNet),
    inputTaxRate: maskCostValue(o.inputTaxRate),
    freightAmount: maskCostValue(o.freightAmount),
    serviceFeeAmount: maskCostValue(o.serviceFeeAmount),
  }
}

function maskDiff(changes: readonly DiffChange[], mask: boolean): DiffChange[] {
  if (!mask) return [...changes]
  return changes.map((c) =>
    c.costSensitive
      ? { ...c, before: COST_MASK, after: COST_MASK, note: "已变化（成本字段掩码）" }
      : c
  )
}

function resolveRole(q?: DemoRole): DemoRole {
  return q ?? "procurement"
}

function costVisibility(
  role: DemoRole,
  forceMask?: boolean
): CostFieldVisibility {
  if (forceMask) return "masked"
  if (role === "ops_tech" || role === "admin") return "masked"
  return "visible"
}

function roleBlockers(
  item: ExternalCatalogItemView,
  role: DemoRole
): ExternalCatalogItemView["actionBlockers"] {
  const blockers = [...item.actionBlockers]
  if (role === "operations") {
    for (const action of [
      "APPROVE_MAPPING",
      "CONFIRM_OFFERING_REVISION",
      "CONFIRM_STOP_SUPPLY",
      "CONFIRM_ERROR_RESOLVED",
    ]) {
      if (!blockers.some((b) => b.action === action)) {
        blockers.push({
          action,
          code: "ROLE_PROCUREMENT_ONLY",
          message: "运营可查看发布准备度并转商品发布，不能确认映射或供给成本",
        })
      }
    }
  }
  if (role === "admin" || role === "ops_tech") {
    for (const action of [
      "APPROVE_MAPPING",
      "CONFIRM_OFFERING_REVISION",
      "CONFIRM_STOP_SUPPLY",
    ]) {
      if (!blockers.some((b) => b.action === action)) {
        blockers.push({
          action,
          code: "ROLE_TECH_ONLY",
          message: "运维/管理员仅处理技术异常，不能确认业务映射或供给",
        })
      }
    }
  }
  return blockers
}

function projectItem(
  seed: ExternalCatalogItemView,
  role: DemoRole,
  mask: boolean
): ExternalCatalogItemView | null {
  // 仅已注册异常任务可终结移除
  if (seed.changeType === "ERROR" || seed.changeType === "STOPPED") {
    const terminal = getWorkItemTerminal(seed.workItem.workItemId)
    if (terminal || markCompletedHas(seed.workItem.workItemId)) {
      return null
    }
  }

  const held =
    seed.changeType === "ERROR" || seed.changeType === "STOPPED"
      ? isWorkItemHeld(seed.workItem.workItemId) ||
        markHeldHas(seed.workItem.workItemId)
      : false

  const publicLease =
    seed.changeType === "ERROR" || seed.changeType === "STOPPED"
      ? getSessionLeaseState(seed.workItem.workItemId)
      : null

  const ep = seed.externalProduct
  const draft = drafts.get(ep.id)

  let mapping = seed.mapping
  if (draft?.selectedSkuId && mapping) {
    const cand = seed.skuCandidates.find((c) => c.skuId === draft.selectedSkuId)
    if (cand) {
      mapping = {
        ...mapping,
        mappingStatus: "PENDING",
        skuId: cand.skuId,
        skuCode: cand.skuCode,
        skuName: cand.skuName,
        specification: cand.specification,
        baseUnit: cand.baseUnit,
        reason: "会话草稿（未经审核，未写 ERP）",
      }
    }
  }

  let offering = seed.offering
  if (draft?.offeringDraft && offering) {
    offering = {
      ...offering,
      proposedDefaults: draft.offeringDraft,
    }
  }

  const base: ExternalCatalogItemView = {
    ...seed,
    externalProduct: {
      ...ep,
      currentRevision: maskRevision(ep.currentRevision, mask),
      incomingRevision: ep.incomingRevision
        ? maskRevision(ep.incomingRevision, mask)
        : undefined,
    },
    mapping,
    offering: offering
      ? {
          ...offering,
          currentRevision: offering.currentRevision
            ? maskOffering(offering.currentRevision, mask)
            : undefined,
          revisionHistory: offering.revisionHistory.map((r) =>
            maskOffering(r, mask)
          ),
          proposedDefaults:
            offering.proposedDefaults && mask
              ? {
                  ...offering.proposedDefaults,
                  supplyPriceGross: COST_MASK,
                  inputTaxRate: COST_MASK,
                  freightAmount: COST_MASK,
                  serviceFeeAmount: COST_MASK,
                }
              : offering.proposedDefaults,
        }
      : undefined,
    sourceDiff: maskDiff(seed.sourceDiff, mask),
    actionBlockers: roleBlockers(seed, role),
    costFieldVisibility: mask ? "masked" : "visible",
  }

  if (base.changeType === "ERROR" || base.changeType === "STOPPED") {
    const actions = getWorkItemActionHistory(base.workItem.workItemId)
    return {
      ...base,
      workItem: {
        ...base.workItem,
        workItemStatus: held
          ? "IN_PROGRESS"
          : actions.length > 0
            ? "IN_PROGRESS"
            : base.workItem.workItemStatus,
        held,
        leaseVersion: publicLease?.leaseVersion ?? base.workItem.leaseVersion,
        leaseExpiresAt:
          publicLease?.leaseExpiresAt ?? base.workItem.leaseExpiresAt,
        claimedBy: publicLease
          ? { userId: "user_demo", displayName: `当前用户 · ${DEMO_ROLE_LABEL[role]}` }
          : base.workItem.claimedBy,
      },
    }
  }

  return base
}

function markCompletedHas(id: string): boolean {
  return getCompletedQueueTaskIds("W21").has(id)
}

function markHeldHas(id: string): boolean {
  return getHeldQueueTaskIds("W21").has(id)
}

function filterSummary(q: ExternalCatalogQueueQuery): string {
  const parts = [
    q.changeType === "all"
      ? "全部变化"
      : q.changeType === "NEW"
        ? "新商品"
        : q.changeType === "CHANGED"
          ? "关键变化"
          : q.changeType === "STOPPED"
            ? "停止供应"
            : q.changeType === "ERROR"
              ? "异常"
              : "需处理",
    q.status === "held" ? "已暂挂" : "待处理",
    DEMO_ROLE_LABEL[resolveRole(q.demoRole)],
  ]
  if (q.q) parts.push(`搜索 ${q.q}`)
  if (q.maskCost) parts.push("成本掩码")
  return parts.join(" · ")
}

function sortItems(items: ExternalCatalogItemView[]): ExternalCatalogItemView[] {
  const rank: Record<string, number> = {
    STOPPED: 0,
    ERROR: 1,
    CHANGED: 2,
    NEW: 3,
    UNCHANGED: 4,
  }
  return [...items].sort((a, b) => {
    const ra = rank[a.changeType] ?? 9
    const rb = rank[b.changeType] ?? 9
    if (ra !== rb) return ra - rb
    const pa =
      a.changeType === "ERROR" || a.changeType === "STOPPED"
        ? a.workItem.priority
        : 50
    const pb =
      b.changeType === "ERROR" || b.changeType === "STOPPED"
        ? b.workItem.priority
        : 50
    return pb - pa
  })
}

export async function fetchExternalCatalogQueue(
  query: ExternalCatalogQueueQuery
): Promise<ExternalCatalogQueueView> {
  await mockDelay()
  const role = resolveRole(query.demoRole)
  const mask = costVisibility(role, query.maskCost) === "masked"

  let items = EXTERNAL_PRODUCT_SUPPLY_SEED.map((s) =>
    projectItem(s, role, mask)
  ).filter((t): t is ExternalCatalogItemView => t != null)

  // 默认 actionable：排除 UNCHANGED（种子中无）
  if (!query.changeType || query.changeType === "actionable") {
    items = items.filter((i) => i.changeType !== "UNCHANGED")
  } else if (query.changeType !== "all") {
    items = items.filter((i) => i.changeType === query.changeType)
  }

  if (query.status === "held") {
    items = items.filter(
      (i) =>
        (i.changeType === "ERROR" || i.changeType === "STOPPED") &&
        i.workItem.held
    )
  }

  if (query.q?.trim()) {
    const q = query.q.trim().toUpperCase()
    items = items.filter((i) => {
      const ep = i.externalProduct
      return (
        ep.externalProductId.toUpperCase().includes(q) ||
        ep.externalSkuId?.toUpperCase().includes(q) ||
        ep.currentRevision.name.toUpperCase().includes(q) ||
        i.mapping?.skuCode?.toUpperCase().includes(q) ||
        ep.supplier.name.includes(query.q!.trim())
      )
    })
  }

  items = sortItems(items)

  const queueContextId =
    query.queueContextId ?? `queue:W21:${role}:${query.changeType ?? "actionable"}`

  let position = 0
  let current = items[0]

  // 优先 workItemId（已注册异常），其次 externalProductId
  if (query.currentWorkItemId) {
    const idx = items.findIndex(
      (i) =>
        (i.changeType === "ERROR" || i.changeType === "STOPPED") &&
        i.workItem.workItemId === query.currentWorkItemId
    )
    if (idx >= 0) {
      position = idx
      current = items[idx]
    }
  } else if (query.currentExternalProductId) {
    const idx = items.findIndex(
      (i) => i.externalProduct.id === query.currentExternalProductId
    )
    if (idx >= 0) {
      position = idx
      current = items[idx]
    }
  }

  const emptyReason =
    EXTERNAL_PRODUCT_SUPPLY_SEED.length === 0
      ? "NO_TASKS"
      : items.length === 0
        ? "FILTER_NO_RESULT"
        : undefined

  const currentWorkItemId =
    current &&
    (current.changeType === "ERROR" || current.changeType === "STOPPED")
      ? current.workItem.workItemId
      : undefined

  return {
    preferences: { autoNextDefault: true },
    context: {
      queueContextId,
      position: items.length === 0 ? 0 : position + 1,
      total: items.length,
      currentExternalProductId: current?.externalProduct.id,
      currentWorkItemId,
      previousExternalProductId: items[position - 1]?.externalProduct.id,
      nextExternalProductId: items[position + 1]?.externalProduct.id,
      filterSummary: filterSummary(query),
      queueContextUpdatedAt: new Date().toISOString(),
    },
    items,
    current,
    emptyReason,
    role,
    costFieldVisibility: mask ? "masked" : "visible",
  }
}

export async function fetchExternalCatalogCenter(input: {
  externalProductId: string
  section?: string
  demoRole?: DemoRole
  maskCost?: boolean
}): Promise<ExternalCatalogCenterView | null> {
  await mockDelay()
  const role = resolveRole(input.demoRole)
  const mask = costVisibility(role, input.maskCost) === "masked"
  const seed = EXTERNAL_PRODUCT_SUPPLY_SEED.find(
    (s) =>
      s.externalProduct.id === input.externalProductId ||
      s.externalProduct.externalProductId === input.externalProductId
  )
  if (!seed) return null
  const item = projectItem(seed, role, mask)
  if (!item) return null

  const impact = item.publicationImpact
  return {
    item,
    section: input.section ?? "overview",
    role,
    costFieldVisibility: mask ? "masked" : "visible",
    related: {
      publications: impact.pauseSubResults.map((p) => ({
        id: p.publicationId,
        label: p.publicationId,
        status: p.status,
        href: `/commerce/publications?q=${encodeURIComponent(p.publicationId)}`,
      })),
      historyOrders: [
        {
          id: "ho1",
          label: `历史已支付 ${impact.historicalPaidOrderCount} 笔`,
          note: "保留下单时商品、销售价、供应商与成本记录，不可改写",
        },
      ],
      techExceptions:
        item.changeType === "ERROR"
          ? [
              {
                id: "te1",
                label: "接口错误与对账",
                href: `/governance/integration-errors?from=W21&externalProductId=${item.externalProduct.id}`,
              },
            ]
          : [],
    },
  }
}

export function getSessionDraft(
  externalProductId: string
): SessionCatalogDraft | null {
  return drafts.get(externalProductId) ?? null
}

export async function saveSessionDraft(input: {
  externalProductId: string
  selectedSkuId?: string
  offeringDraft?: SessionCatalogDraft["offeringDraft"]
  substituteCandidateSkuIds?: string[]
  note?: string
}): Promise<SessionCatalogDraft> {
  await mockDelay(60)
  const next: SessionCatalogDraft = {
    externalProductId: input.externalProductId,
    selectedSkuId: input.selectedSkuId,
    offeringDraft: input.offeringDraft,
    substituteCandidateSkuIds: input.substituteCandidateSkuIds,
    note: input.note,
    updatedAt: new Date().toISOString(),
  }
  drafts.set(input.externalProductId, next)
  return next
}

export async function claimExternalCatalogWorkItem(
  workItemId: string
): Promise<WorkItemLease> {
  await mockDelay(80)
  const seed = EXTERNAL_PRODUCT_SUPPLY_SEED.find(
    (s) =>
      (s.changeType === "ERROR" || s.changeType === "STOPPED") &&
      s.workItem.workItemId === workItemId
  )
  if (!seed || (seed.changeType !== "ERROR" && seed.changeType !== "STOPPED")) {
    throw new Error("仅已注册 ERROR/STOPPED 异常任务可领取")
  }
  if (getWorkItemTerminal(workItemId) || markCompletedHas(workItemId)) {
    throw new Error("任务已完成，无法领取")
  }
  try {
    const lease = claimWorkItemSession({
      workItemId,
      subjectVersion: seed.workItem.subjectVersion,
      subjectHash: seed.workItem.subjectHash,
      leaseVersion: seed.workItem.leaseVersion ?? 1,
      ownerUserId: "user_demo",
    })
    return {
      workItemId,
      claimedByLabel: "当前用户",
      expiresAt: lease.leaseExpiresAt,
      leaseVersion: lease.leaseVersion,
      claimToken: lease.claimToken,
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) throw new Error(error.message)
    throw error
  }
}

export async function applyExternalCatalogWorkItemAction(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectHash: string
  action: ExternalCatalogWorkItemAction
  idempotencyKey: string
  simulateTimeout?: boolean
}): Promise<FormalActionResponse> {
  await mockDelay(100)
  const seed = EXTERNAL_PRODUCT_SUPPLY_SEED.find(
    (s) =>
      (s.changeType === "ERROR" || s.changeType === "STOPPED") &&
      s.workItem.workItemId === input.workItemId
  )
  if (!seed || (seed.changeType !== "ERROR" && seed.changeType !== "STOPPED")) {
    return {
      status: "failed",
      code: "NOT_REGISTERED",
      message: "非已注册异常任务，禁止任务内动作",
    }
  }

  try {
    const record = applyWorkItemActionSession({
      workItemId: input.workItemId,
      claimToken: input.claimToken,
      leaseVersion: input.leaseVersion,
      expectedSubjectHash: input.expectedSubjectHash,
      idempotencyKey: input.idempotencyKey,
      action: {
        kind: input.action.kind,
        note:
          "comment" in input.action
            ? input.action.comment
            : input.action.kind === "HOLD"
              ? input.action.reasonCode
              : undefined,
      },
      simulateTimeout: input.simulateTimeout,
    })

    if (input.action.kind === "HOLD") {
      markQueueTaskHeld("W21", input.workItemId)
    }

    const outcome: FormalOutcome = {
      kind: "ACTION",
      workItemId: input.workItemId,
      workItemStatus: record.workItemStatus,
      actionKind: input.action.kind,
      heldAt: input.action.kind === "HOLD" ? record.recordedAt : undefined,
      resumeHint:
        input.action.kind === "HOLD"
          ? "暂挂已写入证据，任务仍为 PENDING/IN_PROGRESS，保留在有效队列；不自动下一项。"
          : input.action.kind === "RETURN_FOR_DATA_FIX"
            ? "已追加退回数据修复请求；任务未终结，仍为 PENDING/IN_PROGRESS。"
            : "任务内动作成功，状态保持 PENDING/IN_PROGRESS；不自动完成、不自动下一项。",
      reference: `W21-${input.action.kind}-${input.workItemId.toUpperCase()}`,
    }
    return { status: "succeeded", outcome }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      if (error.code === "TIMEOUT") {
        return {
          status: "unknown",
          message: error.message,
          idempotencyKey: input.idempotencyKey,
        }
      }
      return { status: "failed", code: error.code, message: error.message }
    }
    throw error
  }
}

export async function completeExternalCatalogWorkItem(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectHash: string
  decision: ExternalCatalogDecision
  idempotencyKey: string
  simulateTimeout?: boolean
}): Promise<FormalActionResponse> {
  await mockDelay(120)
  const seed = EXTERNAL_PRODUCT_SUPPLY_SEED.find(
    (s) =>
      (s.changeType === "ERROR" || s.changeType === "STOPPED") &&
      s.workItem.workItemId === input.workItemId
  )
  if (!seed || (seed.changeType !== "ERROR" && seed.changeType !== "STOPPED")) {
    return {
      status: "failed",
      code: "NOT_REGISTERED",
      message: "仅已注册 ERROR/STOPPED 可终结；正常映射/供给无提交端点",
    }
  }

  if (
    input.decision.kind === "CONFIRM_ERROR_RESOLVED" &&
    seed.changeType !== "ERROR"
  ) {
    return {
      status: "failed",
      code: "DECISION_MISMATCH",
      message: "确认异常已解决仅适用于 ERROR 项",
    }
  }
  if (
    input.decision.kind === "CONFIRM_STOP_SUPPLY" &&
    seed.changeType !== "STOPPED"
  ) {
    return {
      status: "failed",
      code: "DECISION_MISMATCH",
      message: "确认停供记录仅适用于 STOPPED 项",
    }
  }

  const expectedRev = String(
    seed.externalProduct.incomingRevision?.revisionNo ??
      seed.externalProduct.currentRevision.revisionNo
  )
  if (input.decision.expectedExternalRevision !== expectedRev) {
    return {
      status: "failed",
      code: "REVISION_MISMATCH",
      message: "外部修订已变化，请重新核对后提交",
    }
  }

  try {
    const result = completeWorkItemSession({
      workItemId: input.workItemId,
      claimToken: input.claimToken,
      leaseVersion: input.leaseVersion,
      expectedSubjectHash: input.expectedSubjectHash,
      idempotencyKey: input.idempotencyKey,
      decision: {
        kind: input.decision.kind,
        note: input.decision.comment,
        summary: input.decision.kind,
      },
      simulateTimeout: input.simulateTimeout,
    })
    markQueueTaskCompleted("W21", input.workItemId)
    clearSessionLease(input.workItemId)

    const business = {
      decisionKind: input.decision.kind,
      externalProductId: seed.externalProduct.id,
      auditEventId: result.completionRecordId,
      publicationImpact: seed.publicationImpact,
      reference: `W21-DONE-${input.workItemId.toUpperCase()}`,
      completedAt: new Date().toISOString(),
      subjectHash: input.expectedSubjectHash,
    }

    const outcome: FormalOutcome = {
      kind: "COMPLETED",
      business,
    }
    setIdempotencySucceeded(input.idempotencyKey, "COMPLETE", {
      status: "succeeded",
      outcome,
    })
    return { status: "succeeded", outcome }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      if (error.code === "TIMEOUT") {
        return {
          status: "unknown",
          message: error.message,
          idempotencyKey: input.idempotencyKey,
        }
      }
      return { status: "failed", code: error.code, message: error.message }
    }
    throw error
  }
}

export async function resolveUnknownExternalCatalogResult(input: {
  idempotencyKey: string
  settle?: boolean
}): Promise<FormalActionResponse> {
  await mockDelay(80)
  const entry = queryIdempotencyResult(input.idempotencyKey)
  if (!entry) {
    return {
      status: "failed",
      code: "UNKNOWN_KEY",
      message: "找不到该任务号的结果",
    }
  }
  if (entry.state === "succeeded") {
    const payload = entry.payload as
      | FormalActionResponse
      | {
          actionRecordId?: string
          actionKind?: string
          workItemStatus?: "PENDING" | "IN_PROGRESS"
          recordedAt?: string
          subjectHash?: string
        }
      | {
          workItemStatus?: "COMPLETED"
          completionRecordId?: string
          businessResult?: { kind: string; reference: string; summary: string }
          subjectHash?: string
        }
    if (payload && typeof payload === "object" && "status" in payload) {
      return payload as FormalActionResponse
    }
    if (
      payload &&
      typeof payload === "object" &&
      "actionKind" in payload &&
      payload.actionKind
    ) {
      return {
        status: "succeeded",
        outcome: {
          kind: "ACTION",
          workItemId: "",
          workItemStatus: payload.workItemStatus ?? "IN_PROGRESS",
          actionKind: payload.actionKind,
          resumeHint:
            "查询到任务内动作已成功；状态保持 PENDING/IN_PROGRESS，不自动下一项。",
          reference: payload.actionRecordId ?? input.idempotencyKey,
        },
      }
    }
    if (
      payload &&
      typeof payload === "object" &&
      "completionRecordId" in payload &&
      payload.completionRecordId
    ) {
      return {
        status: "succeeded",
        outcome: {
          kind: "COMPLETED",
          business: {
            decisionKind:
              (payload.businessResult?.kind as
                | "CONFIRM_ERROR_RESOLVED"
                | "CONFIRM_STOP_SUPPLY") ?? "CONFIRM_ERROR_RESOLVED",
            externalProductId: "",
            auditEventId: payload.completionRecordId,
            publicationImpact: {
              activePublicationCount: 0,
              pausedPublicationCount: 0,
              historicalPaidOrderCount: 0,
              safetyPauseTriggered: false,
              safetyPauseReasons: [],
              pauseSubResults: [],
              mallSalePriceAutoUpdate: false,
              moqCopiedToMallMinPurchase: false,
              note: payload.businessResult?.summary ?? "终结已确认",
            },
            reference:
              payload.businessResult?.reference ?? payload.completionRecordId,
            completedAt: new Date().toISOString(),
            subjectHash: payload.subjectHash ?? "",
          },
        },
      }
    }
    return {
      status: "failed",
      code: "UNKNOWN_PAYLOAD",
      message: "处理结果格式无法识别",
    }
  }
  if (entry.state === "pending") {
    return {
      status: "unknown",
      message: "结果仍不确定，请稍后用原任务号再查",
      idempotencyKey: input.idempotencyKey,
    }
  }
  return {
    status: "failed",
    code: "FAILED",
    message: entry.error ?? "动作失败",
  }
}

/** 映射确认/供给确认端点不存在（类型未登记） */
export async function attemptUnregisteredFormalWrite(): Promise<FormalActionResponse> {
  await mockDelay(40)
  return {
    status: "failed",
    code: "WORK_ITEM_TYPE_UNREGISTERED",
    message:
      "正常映射/供给类型未登记：不存在可调用的提交入口。请仅使用会话草稿或进入基础资料。",
  }
}
