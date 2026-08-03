/**
 * W21 session-mock API：queryFn / mutationFn 纯函数。
 * claimToken 仅出现在领取响应；任务内动作 / 终结复用 W02 会话信封语义。
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  CostFieldVisibility,
  CreateSupplierCatalogItemInput,
  DemoRole,
  DiffChange,
  SupplierCatalogCenterView,
  SupplierCatalogDecision,
  SupplierCatalogItemView,
  SupplierCatalogQueueQuery,
  SupplierCatalogQueueView,
  SupplierCatalogWorkItemAction,
  SupplierProductRevisionView,
  FormalActionResponse,
  FormalOutcome,
  SessionCatalogDraft,
  SupplierCatalogWriteResult,
  SupplierOfferingRevisionView,
  PromoteSupplierProductInput,
  ReviseSupplierCatalogProductInput,
  SupplierCatalogSkuView,
  SupplierCatalogSkuWriteFields,
  WorkItemLease,
} from "@/features/supplier-catalog/types"
import {
  COST_MASK,
  DEMO_ROLE_LABEL,
  REGISTRATION_BLOCKER_MESSAGE,
} from "@/features/supplier-catalog/types"
import {
  buildW14ListResult,
  getW14Center,
} from "@/features/master-data/session"
import { SUPPLIER_CATALOG_SEED } from "@/mock/supplier-catalog"
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
const catalogOverlays = new Map<string, SupplierCatalogItemView>()
const createdCatalogIds: string[] = []
const writeResults = new Map<string, SupplierCatalogWriteResult>()
type ProductPoolEntryView = NonNullable<SupplierCatalogItemView["poolEntry"]>
const poolEntryOverlays = new Map<string, ProductPoolEntryView>()

function listCatalogItems(): SupplierCatalogItemView[] {
  const seeded = SUPPLIER_CATALOG_SEED.map(
    (item) => catalogOverlays.get(item.supplierProduct.id) ?? item
  )
  const created = createdCatalogIds
    .map((id) => catalogOverlays.get(id))
    .filter((item): item is SupplierCatalogItemView => Boolean(item))
  return [...created, ...seeded]
}

function companyProductCenters() {
  return buildW14ListResult("products").rows.flatMap((row) => {
    const center = getW14Center("products", row.stableId)
    return center?.productDetail ? [center] : []
  })
}

function currentPoolEntryForSku(skuId: string): ProductPoolEntryView | undefined {
  const overlay = poolEntryOverlays.get(skuId)
  if (overlay) return overlay
  const entries = listCatalogItems().flatMap((item) =>
    item.mapping?.mappingStatus === "ACTIVE" &&
    item.mapping.skuId === skuId &&
    item.poolEntry
      ? [item.poolEntry]
      : []
  )
  return entries.find((entry) => entry.status === "ACTIVE") ?? entries[0]
}

function activeSupplierCountForSku(skuId: string): number {
  return new Set(
    listCatalogItems().flatMap((item) => {
      const offering = item.offering?.currentRevision
      return item.mapping?.mappingStatus === "ACTIVE" &&
        item.mapping.skuId === skuId &&
        offering?.status === "ACTIVE" &&
        offering.availabilityStatus === "AVAILABLE"
        ? [item.supplierProduct.supplier.id]
        : []
    })
  ).size
}

function resolveSkuContext(skuId?: string) {
  if (!skuId) return undefined

  for (const product of companyProductCenters()) {
    const detail = product.productDetail
    if (!detail) continue
    const sku = detail.skus.find((entry) => entry.skuId === skuId)
    if (!sku) continue

    return {
      productId: product.stableId,
      productName: product.name,
      skuId,
      skuCode: sku.skuNo,
      specification: sku.specLabel,
      baseUnit: sku.baseUnit ?? detail.baseUnit,
      poolEntry: currentPoolEntryForSku(skuId),
    }
  }

  return undefined
}

function maskCostValue(v: string | null | undefined): string | null {
  if (v == null) return null
  return COST_MASK
}

function maskRevision(
  rev: SupplierProductRevisionView,
  mask: boolean
): SupplierProductRevisionView {
  if (!mask) return rev
  return {
    ...rev,
    dropshipFloorPriceGross: maskCostValue(rev.dropshipFloorPriceGross),
    bulkFloorPriceGross: maskCostValue(rev.bulkFloorPriceGross),
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
    floorPriceGross: maskCostValue(o.floorPriceGross),
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
  if (role === "operations" || role === "ops_tech" || role === "admin") {
    return "masked"
  }
  return "visible"
}

function roleBlockers(
  item: SupplierCatalogItemView,
  role: DemoRole
): SupplierCatalogItemView["actionBlockers"] {
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
          message: "运营可查看发布准备情况并前往商品发布，但不能确认商品关联或供货成本",
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
          message: "系统管理员和技术人员只能处理数据异常，不能确认商品关联或供货条件",
        })
      }
    }
  }
  return blockers
}

function projectItem(
  seed: SupplierCatalogItemView,
  role: DemoRole,
  mask: boolean
): SupplierCatalogItemView | null {
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

  const ep = seed.supplierProduct
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
        reason: "页面草稿（尚未提交）",
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

  const base: SupplierCatalogItemView = {
    ...seed,
    supplierProduct: {
      ...ep,
      currentRevision: maskRevision(ep.currentRevision, mask),
      incomingRevision: ep.incomingRevision
        ? maskRevision(ep.incomingRevision, mask)
        : undefined,
    },
    mapping,
    poolEntry: mapping?.skuId
      ? currentPoolEntryForSku(mapping.skuId) ?? seed.poolEntry
      : seed.poolEntry,
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
                  floorPriceGross: COST_MASK,
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

function filterSummary(q: SupplierCatalogQueueQuery): string {
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
    q.status === "held" ? "稍后处理" : "待处理",
    DEMO_ROLE_LABEL[resolveRole(q.demoRole)],
  ]
  if (q.q) parts.push(`搜索 ${q.q}`)
  if (q.skuId) parts.push(`SKU ${q.skuId}`)
  if (q.sourceType && q.sourceType !== "all") {
    parts.push(
      q.sourceType === "API"
        ? "API 来源"
        : q.sourceType === "EXCEL"
          ? "Excel 来源"
          : "手工录入"
    )
  }
  if (q.maskCost) parts.push("成本掩码")
  return parts.join(" · ")
}

function sortItems(items: SupplierCatalogItemView[]): SupplierCatalogItemView[] {
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

function sortRelationshipItems(
  items: SupplierCatalogItemView[]
): SupplierCatalogItemView[] {
  const rank = (item: SupplierCatalogItemView) => {
    const offering = item.offering?.currentRevision
    if (
      item.mapping?.mappingStatus === "ACTIVE" &&
      offering?.status === "ACTIVE" &&
      offering.availabilityStatus === "AVAILABLE"
    ) {
      return 0
    }
    if (item.mapping?.mappingStatus !== "ACTIVE") return 1
    return 2
  }

  return [...items].sort((a, b) => rank(a) - rank(b))
}

export async function fetchSupplierCatalogQueue(
  query: SupplierCatalogQueueQuery
): Promise<SupplierCatalogQueueView> {
  await mockDelay()
  const role = resolveRole(query.demoRole)
  const mask = costVisibility(role, query.maskCost) === "masked"

  const catalogItems = listCatalogItems()
  let items = catalogItems.map((s) =>
    projectItem(s, role, mask)
  ).filter((t): t is SupplierCatalogItemView => t != null)

  if (query.sourceType && query.sourceType !== "all") {
    items = items.filter(
      (item) => item.supplierProduct.source.type === query.sourceType
    )
  }

  // 默认 actionable：排除 UNCHANGED（种子中无）
  if (!query.changeType || query.changeType === "actionable") {
    items = items.filter((i) => i.changeType !== "UNCHANGED")
  } else if (query.changeType !== "all") {
    items = items.filter((i) => i.changeType === query.changeType)
  }

  if (query.skuId) {
    items = items.filter(
      (i) =>
        i.mapping?.skuId === query.skuId ||
        i.skuCandidates.some((candidate) => candidate.skuId === query.skuId)
    )
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
      const ep = i.supplierProduct
      return (
        ep.supplierSpuCode?.toUpperCase().includes(q) ||
        ep.supplierSkuCode.toUpperCase().includes(q) ||
        ep.currentRevision.name.toUpperCase().includes(q) ||
        i.mapping?.skuCode?.toUpperCase().includes(q) ||
        ep.supplier.name.includes(query.q!.trim())
      )
    })
  }

  items =
    query.mode === "list" ? sortRelationshipItems(items) : sortItems(items)

  const queueContextId =
    query.queueContextId ??
    `queue:W21:${role}:${query.changeType ?? "actionable"}:${query.skuId ?? "all"}`

  let position = 0
  let current = items[0]

  // 优先 workItemId（已注册异常），其次 supplierProductId
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
  } else if (query.currentSupplierProductId) {
    const idx = items.findIndex(
      (i) => i.supplierProduct.id === query.currentSupplierProductId
    )
    if (idx >= 0) {
      position = idx
      current = items[idx]
    }
  }

  const emptyReason =
    catalogItems.length === 0
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
    skuContext: resolveSkuContext(query.skuId),
    context: {
      queueContextId,
      position: items.length === 0 ? 0 : position + 1,
      total: items.length,
      currentSupplierProductId: current?.supplierProduct.id,
      currentWorkItemId,
      previousSupplierProductId: items[position - 1]?.supplierProduct.id,
      nextSupplierProductId: items[position + 1]?.supplierProduct.id,
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

export async function fetchCompanySkuOptions() {
  await mockDelay(60)
  return companyProductCenters()
    .flatMap((center) =>
      (center.productDetail?.skus ?? [])
        .filter((sku) => sku.lifecycleStatus === "ENABLED" && sku.skuId)
        .map((sku) => ({
          productId: center.stableId,
          skuId: sku.skuId!,
          skuCode: sku.skuNo,
          skuName: center.name,
          specification: sku.specLabel,
          baseUnit: sku.baseUnit ?? center.productDetail!.baseUnit,
          barcode: sku.barcode,
          brand: center.productDetail!.brand,
          category: center.productDetail!.category,
          revisionNo: center.currentRevision.revisionNo,
          similarityLabel: "公司商品候选",
          activeSupplierCount: activeSupplierCountForSku(sku.skuId!),
          poolEntry: currentPoolEntryForSku(sku.skuId!),
        }))
    )
}

export async function fetchSupplierCatalogCenter(input: {
  supplierProductId: string
  section?: string
  demoRole?: DemoRole
  maskCost?: boolean
}): Promise<SupplierCatalogCenterView | null> {
  await mockDelay()
  const role = resolveRole(input.demoRole)
  const mask = costVisibility(role, input.maskCost) === "masked"
  const seed = listCatalogItems().find(
    (s) =>
      s.supplierProduct.id === input.supplierProductId ||
      s.supplierProduct.supplierSpuCode === input.supplierProductId
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
                href: `/governance/integration-errors?from=W21&supplierProductId=${item.supplierProduct.id}`,
              },
            ]
          : [],
    },
  }
}

export function getSessionDraft(
  supplierProductId: string
): SessionCatalogDraft | null {
  return drafts.get(supplierProductId) ?? null
}

export async function saveSessionDraft(input: {
  supplierProductId: string
  selectedSkuId?: string
  offeringDraft?: SessionCatalogDraft["offeringDraft"]
  substituteCandidateSkuIds?: string[]
  note?: string
}): Promise<SessionCatalogDraft> {
  await mockDelay(60)
  const next: SessionCatalogDraft = {
    supplierProductId: input.supplierProductId,
    selectedSkuId: input.selectedSkuId,
    offeringDraft: input.offeringDraft,
    substituteCandidateSkuIds: input.substituteCandidateSkuIds,
    note: input.note,
    updatedAt: new Date().toISOString(),
  }
  drafts.set(input.supplierProductId, next)
  return next
}

export async function claimSupplierCatalogWorkItem(
  workItemId: string
): Promise<WorkItemLease> {
  await mockDelay(80)
  const seed = SUPPLIER_CATALOG_SEED.find(
    (s) =>
      (s.changeType === "ERROR" || s.changeType === "STOPPED") &&
      s.workItem.workItemId === workItemId
  )
  if (!seed || (seed.changeType !== "ERROR" && seed.changeType !== "STOPPED")) {
    throw new Error("当前事项不能领取处理")
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

export async function applySupplierCatalogWorkItemAction(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectHash: string
  action: SupplierCatalogWorkItemAction
  idempotencyKey: string
  simulateTimeout?: boolean
}): Promise<FormalActionResponse> {
  await mockDelay(100)
  const seed = SUPPLIER_CATALOG_SEED.find(
    (s) =>
      (s.changeType === "ERROR" || s.changeType === "STOPPED") &&
      s.workItem.workItemId === input.workItemId
  )
  if (!seed || (seed.changeType !== "ERROR" && seed.changeType !== "STOPPED")) {
    return {
      status: "failed",
      code: "NOT_REGISTERED",
      message: "当前事项暂不支持此操作",
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
          ? "已标记为稍后处理，当前事项仍在待处理列表中。"
          : input.action.kind === "RETURN_FOR_DATA_FIX"
            ? "已退回供应商数据修正，修正完成后可以继续处理。"
            : "处理已记录，当前事项仍在待处理列表中。",
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

export async function completeSupplierCatalogWorkItem(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectHash: string
  decision: SupplierCatalogDecision
  idempotencyKey: string
  simulateTimeout?: boolean
}): Promise<FormalActionResponse> {
  await mockDelay(120)
  const seed = SUPPLIER_CATALOG_SEED.find(
    (s) =>
      (s.changeType === "ERROR" || s.changeType === "STOPPED") &&
      s.workItem.workItemId === input.workItemId
  )
  if (!seed || (seed.changeType !== "ERROR" && seed.changeType !== "STOPPED")) {
    return {
      status: "failed",
      code: "NOT_REGISTERED",
      message: "当前商品关联或供货条件暂不能提交确认",
    }
  }

  if (
    input.decision.kind === "CONFIRM_ERROR_RESOLVED" &&
    seed.changeType !== "ERROR"
  ) {
    return {
      status: "failed",
      code: "DECISION_MISMATCH",
      message: "当前事项不是供应商数据异常，不能使用此操作",
    }
  }
  if (
    input.decision.kind === "CONFIRM_STOP_SUPPLY" &&
    seed.changeType !== "STOPPED"
  ) {
    return {
      status: "failed",
      code: "DECISION_MISMATCH",
      message: "当前事项不是供应商停供，不能使用此操作",
    }
  }

  const expectedRev = String(
    seed.supplierProduct.incomingRevision?.revisionNo ??
      seed.supplierProduct.currentRevision.revisionNo
  )
  if (input.decision.expectedSourceRevision !== expectedRev) {
    return {
      status: "failed",
      code: "REVISION_MISMATCH",
      message: "供应商商品数据已经更新，请刷新并重新核对后提交",
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
      supplierProductId: seed.supplierProduct.id,
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

export async function resolveUnknownSupplierCatalogResult(input: {
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
            "已查到原处理结果，当前事项仍在待处理列表中。",
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
            supplierProductId: "",
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

const EMPTY_PUBLICATION_IMPACT = {
  activePublicationCount: 0,
  pausedPublicationCount: 0,
  historicalPaidOrderCount: 0,
  safetyPauseTriggered: false,
  safetyPauseReasons: [] as string[],
  pauseSubResults: [] as Array<{
    id: string
    publicationId: string
    reason: string
    outboxId: string
    status: string
  }>,
  mallSalePriceAutoUpdate: false as const,
  moqCopiedToMallMinPurchase: false as const,
  note: "尚未绑定商城发布。公司商品池价格与供应商采购成本分别维护。",
}

function createConfirmedOffering(input: {
  offeringId: string
  costGross: string
  inputTaxRate: string
  minimumOrderQuantity: string
  supplyMode: "DROPSHIP" | "BULK"
  supplyRegion: string[]
  validFrom: string
}): SupplierOfferingRevisionView {
  return {
    offeringId: input.offeringId,
    offeringRevisionId: `${input.offeringId}_r1`,
    revisionNo: 1,
    status: "ACTIVE",
    supplyPriceGross: input.costGross,
    supplyPriceNet: input.costGross,
    floorPriceGross: input.costGross,
    supplyMode: input.supplyMode,
    inputTaxRate: input.inputTaxRate,
    freightAmount: "0.00",
    serviceFeeAmount: "0.00",
    minimumOrderQuantity: input.minimumOrderQuantity,
    supplyRegion: input.supplyRegion,
    availabilityStatus: "AVAILABLE",
    availableQuantity: "—",
    productCapabilities: [],
    validFrom: input.validFrom,
    createdAt: new Date().toISOString(),
    immutable: true,
  }
}

function writeProductPoolEntry(input: {
  skuId: string
  action: "KEEP_EXISTING" | "SET_PRICE"
  salesVisiblePrice?: string
  validFrom: string
  expectedPoolEntryRevisionId?: string
}): {
  poolEntry: ProductPoolEntryView
  change: "CREATED" | "REVISED" | "UNCHANGED"
} {
  const existing = currentPoolEntryForSku(input.skuId)
  if (existing && input.action === "KEEP_EXISTING") {
    return { poolEntry: existing, change: "UNCHANGED" }
  }
  if (!existing && input.action === "KEEP_EXISTING") {
    throw new Error("该公司 SKU 尚未加入商品池，必须先设置销售可见价")
  }
  if (!input.salesVisiblePrice?.trim()) {
    throw new Error("新建或修改公司商品池价格时必须填写销售可见价")
  }
  if (
    existing &&
    input.expectedPoolEntryRevisionId &&
    input.expectedPoolEntryRevisionId !== existing.poolEntryRevisionId
  ) {
    throw new Error("公司商品池价格已经更新，请刷新后重新确认")
  }

  const revisionSuffix = existing ? Date.now().toString(36) : "r1"
  const poolEntry: ProductPoolEntryView = {
    poolEntryId: existing?.poolEntryId ?? `pool_${input.skuId}`,
    poolEntryRevisionId: existing
      ? `${existing.poolEntryId}_${revisionSuffix}`
      : `pool_${input.skuId}_${revisionSuffix}`,
    status: "ACTIVE",
    salesVisiblePrice: input.salesVisiblePrice.trim(),
    validFrom: input.validFrom,
  }
  poolEntryOverlays.set(input.skuId, poolEntry)
  return { poolEntry, change: existing ? "REVISED" : "CREATED" }
}

function normalizeSkuWrites(
  input: CreateSupplierCatalogItemInput | ReviseSupplierCatalogProductInput,
): SupplierCatalogSkuWriteFields[] {
  if ("skus" in input && input.skus && input.skus.length > 0) {
    return [...input.skus]
  }
  const flat = input as CreateSupplierCatalogItemInput
  if (!flat.supplierSkuCode?.trim()) {
    throw new Error("请至少填写一个供应商 SKU 编码")
  }
  return [
    {
      supplierSkuCode: flat.supplierSkuCode.trim(),
      barcode: flat.barcode,
      specification: flat.specification,
      attributes: flat.attributes,
      media: flat.media.filter((m) => m.usage === "SKU_MAIN"),
      dropshipFloorPriceGross: flat.dropshipFloorPriceGross ?? "0",
      bulkFloorPriceGross: flat.bulkFloorPriceGross ?? "0",
      bulkMinimumOrderQuantity: flat.bulkMinimumOrderQuantity ?? "1",
      availableQuantity: flat.availableQuantity,
      availabilityStatus: flat.availabilityStatus,
    },
  ]
}

function buildCatalogSkus(input: {
  productId: string
  revisionNo: number
  spuName: string
  spuDescription?: string
  spuCategory: string
  spuBrand?: string
  spuBaseUnit?: string
  spuAttributes: readonly { name: string; value: string }[]
  spuMedia: readonly Omit<
    import("@/features/supplier-catalog/types").SupplierCatalogMediaView,
    "id"
  >[]
  skus: readonly SupplierCatalogSkuWriteFields[]
  now: string
}): {
  catalogSkus: SupplierCatalogSkuView[]
  primary: SupplierCatalogSkuView
  currentRevision: SupplierProductRevisionView
} {
  const catalogSkus: SupplierCatalogSkuView[] = input.skus.map((sku, index) => {
    const skuMedia = [
      ...input.spuMedia.filter((m) => m.usage !== "SKU_MAIN"),
      ...(sku.media ?? []).filter((m) => m.usage === "SKU_MAIN"),
    ]
    const revision: SupplierProductRevisionView = {
      revisionNo: input.revisionNo,
      sourceUpdatedAt: input.now,
      syncedAt: input.now,
      name: input.spuName,
      description: input.spuDescription,
      specification: sku.specification ?? input.spuName,
      category: input.spuCategory,
      brand: input.spuBrand,
      baseUnit: input.spuBaseUnit,
      barcode: sku.barcode,
      attributes: sku.attributes ?? input.spuAttributes,
      media: skuMedia.map((media, mediaIndex) => ({
        ...media,
        id: `${input.productId}_sku${index + 1}_media_${mediaIndex + 1}`,
      })),
      dropshipFloorPriceGross: sku.dropshipFloorPriceGross,
      bulkFloorPriceGross: sku.bulkFloorPriceGross,
      bulkMinimumOrderQuantity: sku.bulkMinimumOrderQuantity,
      availableQuantity: sku.availableQuantity?.trim() || "—",
      availabilityStatus: sku.availabilityStatus ?? "AVAILABLE",
      contentFingerprintShort: `r${input.revisionNo}-s${index + 1}`,
    }
    return {
      id: sku.id ?? `${input.productId}_sku_${index + 1}`,
      supplierSkuCode: sku.supplierSkuCode,
      currentRevision: revision,
    }
  })
  const primary = catalogSkus[0]!
  return {
    catalogSkus,
    primary,
    currentRevision: primary.currentRevision,
  }
}

/**
 * 日常供应商商品录入：Excel、API 和手工共用同一命令形状。
 * API 连接只是来源元数据，不是供应商商品或供给的创建前置条件。
 */
export async function createSupplierCatalogItem(
  input: CreateSupplierCatalogItemInput
): Promise<SupplierCatalogWriteResult> {
  await mockDelay(120)
  const cached = writeResults.get(input.idempotencyKey)
  if (cached) return cached
  if (input.targetSkuId && !input.confirmedCostGross) {
    throw new Error("登记供应商供给时必须确认采购成本")
  }

  const skuWrites = normalizeSkuWrites(input)
  const primaryWrite = skuWrites[0]!
  const seq = createdCatalogIds.length + 1
  const id = `supplier_product_manual_${seq}`
  const now = new Date().toISOString()
  const offeringId = `supplier_offering_${seq}`
  const offering = input.targetSkuId
    ? createConfirmedOffering({
        offeringId,
        costGross: input.confirmedCostGross!,
        inputTaxRate: input.inputTaxRate ?? "0.13",
        minimumOrderQuantity:
          input.minimumOrderQuantity ||
          primaryWrite.bulkMinimumOrderQuantity ||
          "1",
        supplyMode: input.supplyMode,
        supplyRegion: input.supplyRegion ?? ["全国"],
        validFrom: input.validFrom,
      })
    : undefined
  const poolWrite = input.targetSkuId
    ? writeProductPoolEntry({
        skuId: input.targetSkuId,
        action:
          input.poolPriceAction ??
          (currentPoolEntryForSku(input.targetSkuId)
            ? "KEEP_EXISTING"
            : "SET_PRICE"),
        salesVisiblePrice: input.salesVisiblePrice,
        validFrom: input.validFrom,
      })
    : undefined

  const { catalogSkus, primary, currentRevision } = buildCatalogSkus({
    productId: id,
    revisionNo: 1,
    spuName: input.name,
    spuDescription: input.description,
    spuCategory: input.category,
    spuBrand: input.brand,
    spuBaseUnit: input.sourceBaseUnit,
    spuAttributes: input.attributes,
    spuMedia: input.media,
    skus: skuWrites,
    now,
  })

  const base = {
    supplierProduct: {
      id,
      supplier: { id: input.supplierId, name: input.supplierName },
      source: {
        type: input.sourceType,
        label:
          input.sourceType === "EXCEL"
            ? "Excel 导入"
            : input.sourceType === "API"
              ? "API 同步"
              : "手工录入",
        fileName:
          input.sourceType === "EXCEL" ? input.sourceReference : undefined,
        batchNo: `SC-${input.sourceType}-${String(seq).padStart(4, "0")}`,
        recordedBy: "采购 · 当前用户",
      },
      supplierSpuCode: input.supplierSpuCode,
      supplierSkuCode: primary.supplierSkuCode,
      status: "ACTIVE",
      currentRevision,
      catalogSkus,
    },
    skuCandidates: input.targetSkuId
      ? []
      : [],
    offering: offering
      ? {
          stableId: offeringId,
          currentRevision: offering,
          revisionHistory: [offering],
        }
      : {
          stableId: offeringId,
          revisionHistory: [],
        },
    poolEntry: poolWrite?.poolEntry,
    publicationImpact: EMPTY_PUBLICATION_IMPACT,
    sourceContext: {
      intakeId: `intake_${input.sourceType.toLowerCase()}_${seq}`,
      sourceReference:
        input.sourceReference ?? `${input.sourceType.toLowerCase()}:${seq}`,
      receivedAt: now,
    },
    sourceDiff: [],
    costFieldVisibility: "visible" as const,
  }

  const item: SupplierCatalogItemView = input.targetSkuId
    ? {
        ...base,
        changeType: "UNCHANGED",
        mapping: {
          mappingStatus: "ACTIVE",
          skuId: input.targetSkuId,
          skuCode: input.targetSkuCode,
          skuName: input.targetSkuName,
          skuRevisionId: `${input.targetSkuId}:current`,
          specification: input.targetSpecification ?? input.specification,
          baseUnit: input.baseUnit,
          approvedBy: "采购 · 当前用户",
          approvedAt: now,
          reason: "手工录入并加入公司商品池",
          mappingVersion: `mapping_${seq}_v1`,
          history: [],
        },
        allowedActions: ["BROWSE", "REVISE_OFFERING"],
        actionBlockers: [],
      }
    : {
        ...base,
        changeType: "NEW",
        registrationBlocker: {
          code: "WORK_ITEM_TYPE_UNREGISTERED",
          message: REGISTRATION_BLOCKER_MESSAGE,
          businessProcess: "MAPPING",
        },
        allowedActions: ["PROMOTE_TO_PRODUCT_POOL", "BROWSE"],
        actionBlockers: [],
      }

  catalogOverlays.set(id, item)
  createdCatalogIds.unshift(id)
  const result: SupplierCatalogWriteResult = {
    supplierProductId: id,
    supplierOfferingRevisionId: offering?.offeringRevisionId,
    poolEntryRevisionId: item.poolEntry?.poolEntryRevisionId,
    poolEntryChange: poolWrite?.change ?? "NONE",
    activeSupplierCount: input.targetSkuId
      ? activeSupplierCountForSku(input.targetSkuId)
      : undefined,
    reference: `SC-${input.sourceType}-${String(seq).padStart(4, "0")}`,
    recordedAt: now,
  }
  writeResults.set(input.idempotencyKey, result)
  return result
}

function buildSourceDiff(
  previous: SupplierProductRevisionView,
  next: SupplierProductRevisionView,
): DiffChange[] {
  const pairs: Array<[string, string, string | undefined, string | undefined]> =
    [
      ["name", "名称", previous.name, next.name],
      ["description", "描述", previous.description, next.description],
      ["specification", "规格", previous.specification, next.specification],
      ["category", "来源分类", previous.category, next.category],
      ["brand", "来源品牌", previous.brand, next.brand],
      ["baseUnit", "来源单位", previous.baseUnit, next.baseUnit],
      ["barcode", "条码", previous.barcode, next.barcode],
      [
        "dropshipFloorPriceGross",
        "一件代发底价（含税运）",
        previous.dropshipFloorPriceGross ?? undefined,
        next.dropshipFloorPriceGross ?? undefined,
      ],
      [
        "bulkFloorPriceGross",
        "集采底价（含税）",
        previous.bulkFloorPriceGross ?? undefined,
        next.bulkFloorPriceGross ?? undefined,
      ],
      [
        "bulkMinimumOrderQuantity",
        "集采起订量",
        previous.bulkMinimumOrderQuantity ?? undefined,
        next.bulkMinimumOrderQuantity ?? undefined,
      ],
      [
        "media",
        "图文数量",
        String(previous.media?.length ?? 0),
        String(next.media?.length ?? 0),
      ],
    ]
  return pairs
    .filter(([, , before, after]) => (before ?? "") !== (after ?? ""))
    .map(([id, field, before, after], index) => ({
      id: `diff_${id}_${index}`,
      field,
      before: before || "—",
      after: after || "—",
      costSensitive:
        id === "dropshipFloorPriceGross" || id === "bulkFloorPriceGross",
    }))
}

/** 供应商商品中心保存：形成新的来源内容修订，不写公司主档。 */
export async function reviseSupplierCatalogProduct(
  input: ReviseSupplierCatalogProductInput,
): Promise<SupplierCatalogWriteResult> {
  await mockDelay(120)
  const cached = writeResults.get(input.idempotencyKey)
  if (cached) return cached
  const current = listCatalogItems().find(
    (item) => item.supplierProduct.id === input.supplierProductId,
  )
  if (!current) throw new Error("供应商商品不存在或无权访问")
  const previous =
    current.supplierProduct.incomingRevision ??
    current.supplierProduct.currentRevision
  if (previous.revisionNo !== input.expectedSourceRevisionNo) {
    throw new Error("供应商商品来源版本已经变化，请刷新后重新保存")
  }
  const now = new Date().toISOString()
  const nextRevisionNo = previous.revisionNo + 1
  const { catalogSkus, primary, currentRevision: nextRevision } =
    buildCatalogSkus({
      productId: input.supplierProductId,
      revisionNo: nextRevisionNo,
      spuName: input.name,
      spuDescription: input.description,
      spuCategory: input.category,
      spuBrand: input.brand,
      spuBaseUnit: input.sourceBaseUnit,
      spuAttributes: input.attributes,
      spuMedia: input.media,
      skus: input.skus,
      now,
    })
  const next: SupplierCatalogItemView = {
    ...current,
    supplierProduct: {
      ...current.supplierProduct,
      supplierSpuCode: input.supplierSpuCode,
      supplierSkuCode: primary.supplierSkuCode,
      currentRevision: nextRevision,
      catalogSkus,
      incomingRevision: undefined,
    },
    sourceDiff: buildSourceDiff(previous, nextRevision),
    allowedActions: current.allowedActions.includes("PROMOTE_TO_PRODUCT_POOL")
      ? current.allowedActions
      : [...current.allowedActions, "PROMOTE_TO_PRODUCT_POOL"].filter(
          (action, index, arr) => arr.indexOf(action) === index,
        ),
  }
  catalogOverlays.set(input.supplierProductId, next)
  const result: SupplierCatalogWriteResult = {
    supplierProductId: input.supplierProductId,
    poolEntryChange: "NONE",
    reference: `SC-REV-${input.supplierProductId}-${nextRevisionNo}`,
    recordedAt: now,
  }
  writeResults.set(input.idempotencyKey, result)
  return result
}

/** 采购把已有供应商 SKU 关联到公司 SKU，并同时确认成本与销售可见价。 */
export async function promoteSupplierProductToPool(
  input: PromoteSupplierProductInput
): Promise<SupplierCatalogWriteResult> {
  await mockDelay(140)
  const cached = writeResults.get(input.idempotencyKey)
  if (cached) return cached
  const current = listCatalogItems().find(
    (item) => item.supplierProduct.id === input.supplierProductId
  )
  if (!current) throw new Error("供应商商品不存在或无权访问")
  if (current.changeType === "ERROR") {
    throw new Error("异常来源数据必须先修复，不能直接加入公司商品池")
  }
  const sourceRevision =
    current.supplierProduct.incomingRevision ??
    current.supplierProduct.currentRevision
  if (sourceRevision.revisionNo !== input.expectedSourceRevisionNo) {
    throw new Error("供应商商品来源版本已经变化，请刷新后重新确认")
  }
  if (
    current.mapping?.mappingStatus === "ACTIVE" &&
    current.mapping.skuId &&
    current.mapping.skuId !== input.targetSkuId
  ) {
    throw new Error("该供应商 SKU 已关联其他公司 SKU，必须先走映射变更流程")
  }

  const now = new Date().toISOString()
  const offeringId =
    current.offering?.stableId ?? `offering_${input.supplierProductId}`
  const offering = createConfirmedOffering({
    offeringId,
    costGross: input.confirmedCostGross,
    inputTaxRate: input.inputTaxRate,
    minimumOrderQuantity: input.minimumOrderQuantity,
    supplyMode: input.supplyMode,
    supplyRegion: input.supplyRegion,
    validFrom: input.validFrom,
  })
  const poolWrite = writeProductPoolEntry({
    skuId: input.targetSkuId,
    action: input.poolPriceAction,
    salesVisiblePrice: input.salesVisiblePrice,
    validFrom: input.validFrom,
    expectedPoolEntryRevisionId: input.expectedPoolEntryRevisionId,
  })
  const { workItem: _workItem, registrationBlocker: _registrationBlocker, ...rest } =
    current as SupplierCatalogItemView & {
      workItem?: unknown
      registrationBlocker?: unknown
    }
  void _workItem
  void _registrationBlocker
  const next: SupplierCatalogItemView = {
    ...rest,
    changeType: "UNCHANGED",
    mapping: {
      mappingStatus: "ACTIVE",
      skuId: input.targetSkuId,
      skuCode: input.targetSkuCode,
      skuName: input.targetSkuName,
      skuRevisionId: `${input.targetSkuId}:current`,
      specification: input.specification,
      baseUnit: input.baseUnit,
      approvedBy: "采购 · 当前用户",
      approvedAt: now,
      reason: "采购确认加入公司商品池",
      mappingVersion: `map_${input.supplierProductId}_${Date.now()}`,
      history: current.mapping?.history ?? [],
    },
    offering: {
      stableId: offeringId,
      currentRevision: offering,
      revisionHistory: [
        ...(current.offering?.revisionHistory ?? []),
        offering,
      ],
    },
    poolEntry: poolWrite.poolEntry,
    allowedActions: ["BROWSE", "REVISE_OFFERING"],
    actionBlockers: [],
  }
  catalogOverlays.set(input.supplierProductId, next)
  const result = {
    supplierProductId: input.supplierProductId,
    supplierOfferingRevisionId: offering.offeringRevisionId,
    poolEntryRevisionId: next.poolEntry?.poolEntryRevisionId,
    poolEntryChange: poolWrite.change,
    activeSupplierCount: activeSupplierCountForSku(input.targetSkuId),
    reference: `POOL-${input.targetSkuCode}-${Date.now().toString(36)}`,
    recordedAt: now,
  }
  writeResults.set(input.idempotencyKey, result)
  return result
}

/** 映射确认/供给确认端点不存在（类型未登记） */
export async function attemptUnregisteredFormalWrite(): Promise<FormalActionResponse> {
  await mockDelay(40)
  return {
    status: "failed",
    code: "WORK_ITEM_TYPE_UNREGISTERED",
    message:
      "商品关联和供货条件确认功能暂未开放。当前可以保存草稿或前往商品资料。",
  }
}
