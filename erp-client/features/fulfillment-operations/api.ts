/**
 * W09 session-mock API：queryFn / mutationFn 纯函数。
 * claimToken 仅出现在领取/续租响应；查询 View 不回显令牌。
 * 正式过账后才更新队列完成集合与业务结果；结果未知时不改库存/预占/队列。
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  DeferReasonCode,
  FormalActionResponse,
  FulfillmentDraft,
  FulfillmentFormalOutcome,
  FulfillmentOperationType,
  FulfillmentQueueView,
  FulfillmentTask,
  WorkItemLease,
} from "@/features/fulfillment-operations/types"
import {
  OPERATION_TYPE_LABEL,
  OPERATION_TYPE_SHORT,
} from "@/features/fulfillment-operations/types"
import { FULFILLMENT_OPERATIONS_SEED } from "@/mock/fulfillment-operations"
import {
  applyWorkItemActionSession,
  claimWorkItemSession,
  clearSessionLease,
  completeWorkItemSession,
  finalizePendingComplete,
  getFulfillmentBusinessOutcome,
  getFulfillmentDraft,
  getFulfillmentPaymentGateOverride,
  getIdempotencyEntry,
  getSessionLease,
  getSessionLeaseState,
  isFulfillmentWorkItemHeld,
  isFulfillmentWorkItemTerminal,
  markQueueTaskCompleted,
  markQueueTaskHeld,
  queryIdempotencyResult,
  saveFulfillmentDraft,
  setFulfillmentBusinessOutcome,
  setIdempotencySucceeded,
  WorkItemMockError,
} from "@/mock/session-state"

export type FulfillmentQueueFilters = {
  scope: "mine" | "role_pool"
  /** 空 = 全部类型 */
  operationTypes?: FulfillmentOperationType[]
  warehouseId?: string
  q?: string
  due?: "active" | "today" | "overdue"
  gate?: "blocked" | "satisfied"
  salesOrderId?: string
  purchaseOrderId?: string
  currentWorkItemId?: string
  queueContextId?: string
}

function projectTask(seed: FulfillmentTask): FulfillmentTask | null {
  if (isFulfillmentWorkItemTerminal(seed.workItemId)) return null
  const held = isFulfillmentWorkItemHeld(seed.workItemId)
  const draftStore = getFulfillmentDraft(seed.workItemId)
  const publicLease = getSessionLeaseState(seed.workItemId)
  const draft = (draftStore?.draft as FulfillmentDraft | undefined) ?? seed.draft
  const editVersion = draftStore?.editVersion ?? seed.editVersion

  const poId = seed.source.purchaseOrderId
  const gateOverride = poId ? getFulfillmentPaymentGateOverride(poId) : null
  const gate = gateOverride
    ? {
        ...seed.gate,
        state: gateOverride.state,
        message: gateOverride.message,
        effectivePaidAmount: gateOverride.effectivePaidAmount,
        requiredAmount: gateOverride.requiredAmount,
      }
    : seed.gate

  const allowedActions =
    gate.state === "BLOCKED"
      ? seed.allowedActions.filter((a) => a !== "POST")
      : seed.allowedActions.includes("POST")
        ? seed.allowedActions
        : gate.state === "SATISFIED" && seed.gate.state === "BLOCKED"
          ? ([...seed.allowedActions, "POST"] as typeof seed.allowedActions)
          : seed.allowedActions

  const actionBlockers =
    gate.state === "BLOCKED"
      ? [
          ...seed.actionBlockers.filter((b) => b.code !== "PREPAYMENT_BLOCKED"),
          {
            action: "POST",
            code: "PREPAYMENT_BLOCKED",
            message: gate.message,
          },
        ]
      : seed.actionBlockers.filter((b) => b.code !== "PREPAYMENT_BLOCKED")

  return {
    ...seed,
    held,
    statusLabel: held ? "已暂挂" : seed.statusLabel,
    statusTone: held ? "warning" : seed.statusTone,
    editVersion,
    draft,
    gate,
    allowedActions,
    actionBlockers,
    lease: publicLease
      ? {
          claimedByLabel: "当前用户 · 履约经办",
          expiresAt: publicLease.leaseExpiresAt,
          leaseVersion: publicLease.leaseVersion,
        }
      : seed.lease,
  }
}

function filterSummary(filters: FulfillmentQueueFilters): string {
  const parts = [
    filters.scope === "mine" ? "仅我的" : "团队",
    filters.operationTypes && filters.operationTypes.length > 0
      ? filters.operationTypes.map((t) => OPERATION_TYPE_SHORT[t]).join("/")
      : "全部类型",
  ]
  if (filters.due === "overdue") parts.push("已超期")
  else if (filters.due === "today") parts.push("今日到期")
  if (filters.gate === "blocked") parts.push("门禁阻塞")
  if (filters.gate === "satisfied") parts.push("门禁满足")
  if (filters.warehouseId) parts.push(`仓 ${filters.warehouseId}`)
  if (filters.q) parts.push(`单号 ${filters.q}`)
  if (filters.salesOrderId) parts.push(`销售 ${filters.salesOrderId}`)
  if (filters.purchaseOrderId) parts.push(`采购 ${filters.purchaseOrderId}`)
  return parts.join(" · ")
}

function matchTask(
  task: FulfillmentTask,
  filters: FulfillmentQueueFilters
): boolean {
  if (
    filters.operationTypes &&
    filters.operationTypes.length > 0 &&
    !filters.operationTypes.includes(task.operationType)
  ) {
    return false
  }
  if (filters.warehouseId) {
    const wh = task.source.warehouseId
    // 仓筛选仅对入库/仓发生效；其它类型在筛选时忽略仓库条件
    if (
      (task.operationType === "RECEIPT" ||
        task.operationType === "WAREHOUSE_SHIP") &&
      wh !== filters.warehouseId
    ) {
      return false
    }
  }
  if (filters.salesOrderId && task.source.salesOrderId !== filters.salesOrderId) {
    return false
  }
  if (
    filters.purchaseOrderId &&
    task.source.purchaseOrderId !== filters.purchaseOrderId
  ) {
    return false
  }
  if (filters.q) {
    const q = filters.q.trim().toUpperCase()
    const hay = [
      task.source.salesOrderNo,
      task.source.purchaseNo ?? "",
      task.source.customerLabel,
      task.source.supplierLabel ?? "",
    ]
      .join(" ")
      .toUpperCase()
    if (!hay.includes(q) && !hay.startsWith(q)) {
      // 允许前缀匹配业务号
      const nos = [
        task.source.salesOrderNo.toUpperCase(),
        (task.source.purchaseNo ?? "").toUpperCase(),
      ]
      if (!nos.some((n) => n.startsWith(q) || n.includes(q))) return false
    }
  }
  if (filters.due === "overdue" && !task.overdue) return false
  if (filters.gate === "blocked" && task.gate.state !== "BLOCKED") return false
  if (filters.gate === "satisfied" && task.gate.state !== "SATISFIED") return false
  return true
}

function computeMetrics(
  all: readonly FulfillmentTask[]
): FulfillmentQueueView["metrics"] {
  const types: FulfillmentOperationType[] = [
    "RECEIPT",
    "WAREHOUSE_SHIP",
    "SUPPLIER_DIRECT",
    "ELECTRONIC",
    "SERVICE",
  ]
  return types.map((operationType) => ({
    operationType,
    label: `待${OPERATION_TYPE_SHORT[operationType]}`,
    count: all.filter((t) => t.operationType === operationType).length,
    visible: true,
  }))
}

export async function fetchFulfillmentQueue(
  filters: FulfillmentQueueFilters
): Promise<FulfillmentQueueView> {
  await mockDelay()
  const projected = FULFILLMENT_OPERATIONS_SEED.map(projectTask).filter(
    (t): t is FulfillmentTask => t != null
  )

  // 指标按权限范围全量（演示 = 全部投影），不由当前筛选队列求和
  const metrics = computeMetrics(projected)

  let tasks = projected.filter((t) => matchTask(t, filters))
  tasks = [...tasks].sort((a, b) => {
    if (a.overdue !== b.overdue) return a.overdue ? -1 : 1
    if (a.priority !== b.priority) return b.priority - a.priority
    return a.dueAt.localeCompare(b.dueAt)
  })

  const queueContextId =
    filters.queueContextId ??
    `queue:fulfillment:demo:${filters.scope}`

  let position = 0
  let current = tasks[0]
  if (filters.currentWorkItemId) {
    const idx = tasks.findIndex((t) => t.workItemId === filters.currentWorkItemId)
    if (idx >= 0) {
      position = idx
      current = tasks[idx]
    }
  }

  const emptyReason =
    FULFILLMENT_OPERATIONS_SEED.every((t) =>
      isFulfillmentWorkItemTerminal(t.workItemId)
    )
      ? "NO_TASKS"
      : tasks.length === 0
        ? "FILTER_NO_RESULT"
        : undefined

  return {
    preferences: { autoNextDefault: true },
    context: {
      queueContextId,
      position: tasks.length === 0 ? 0 : position + 1,
      total: tasks.length,
      currentWorkItemId: current?.workItemId,
      previousWorkItemId: tasks[position - 1]?.workItemId,
      nextWorkItemId: tasks[position + 1]?.workItemId,
      filterSummary: filterSummary(filters),
      snapshotUpdatedAt: new Date().toISOString(),
    },
    metrics,
    tasks,
    current,
    emptyReason,
  }
}

export async function claimFulfillmentWorkItem(
  workItemId: string
): Promise<WorkItemLease> {
  await mockDelay(80)
  const seed = FULFILLMENT_OPERATIONS_SEED.find((t) => t.workItemId === workItemId)
  if (!seed) throw new Error("任务不存在")
  if (isFulfillmentWorkItemTerminal(workItemId)) {
    throw new Error("任务已完成，无法领取")
  }
  try {
    const lease = claimWorkItemSession({
      workItemId,
      subjectVersion: seed.sourceVersion,
      subjectHash: seed.subjectHash,
      leaseVersion: seed.lease?.leaseVersion ?? 1,
      ownerUserId: "user_fulfillment",
    })
    return {
      workItemId,
      claimedByLabel: "当前用户 · 履约经办",
      expiresAt: lease.leaseExpiresAt,
      leaseVersion: lease.leaseVersion,
      claimToken: lease.claimToken,
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) throw new Error(error.message)
    throw error
  }
}

export async function saveFulfillmentOperation(input: {
  workItemId: string
  expectedEditVersion: number
  claimToken: string
  leaseVersion: number
  draft: FulfillmentDraft
  idempotencyKey: string
}): Promise<{ editVersion: number }> {
  await mockDelay(100)
  const seed = FULFILLMENT_OPERATIONS_SEED.find(
    (t) => t.workItemId === input.workItemId
  )
  if (!seed) throw new Error("任务不存在")
  try {
    applyWorkItemActionSession({
      workItemId: input.workItemId,
      claimToken: input.claimToken,
      leaseVersion: input.leaseVersion,
      expectedSubjectHash: seed.subjectHash,
      idempotencyKey: input.idempotencyKey,
      action: { kind: "SAVE_EVIDENCE", note: "保存履约作业草稿" },
    })
    return saveFulfillmentDraft(
      input.workItemId,
      input.draft,
      input.expectedEditVersion
    )
  } catch (error) {
    if (error instanceof WorkItemMockError) throw new Error(error.message)
    throw error
  }
}

function validateDraft(
  task: FulfillmentTask,
  draft: FulfillmentDraft
): string | null {
  if (draft.type !== task.operationType) {
    return "草稿类型与当前作业类型不一致，禁止跨类型生成记录"
  }
  if (task.gate.state === "BLOCKED" && draft.type !== "WAREHOUSE_SHIP") {
    return task.gate.message
  }

  if (draft.type === "RECEIPT") {
    if (!draft.warehouseId) return "请选择入库仓"
    for (const line of draft.lines) {
      const recv = Number(line.receivedQuantity)
      const qual = Number(line.qualifiedQuantity)
      const rej = Number(line.rejectedQuantity)
      if (!(recv > 0)) return "到货数量必须大于 0"
      if (qual < 0 || rej < 0) return "合格/不合格数量不能为负"
      if (qual + rej > recv + 1e-9) {
        return "合格 + 不合格不得超过到货数量"
      }
      const src = task.lines.find(
        (l) => l.purchaseRevisionLineId === line.purchaseRevisionLineId
      )
      if (src && qual + rej > Number(src.remainingQuantity) + 1e-9) {
        return `累计收货不得超过剩余可收 ${src.remainingQuantity}`
      }
    }
  }

  if (draft.type === "WAREHOUSE_SHIP") {
    if (!draft.warehouseId) return "请选择发货仓"
    if (!draft.carrier.trim()) return "请填写承运方"
    if (!draft.trackingNo.trim()) return "请填写物流单号"
    for (const line of draft.lines) {
      const qty = Number(line.quantity)
      if (!(qty > 0)) return "发货数量必须大于 0"
      if (!line.stockReservationId) return "仓发必须引用有效销售预占"
      const src = task.lines.find((l) => l.salesOrderLineId === line.salesOrderLineId)
      if (!src) return "销售明细不存在"
      if (src.stockReservationId !== line.stockReservationId) {
        return "预占必须归属本销售明细"
      }
      const cap = Number(src.reservedQuantity ?? src.remainingQuantity)
      if (qty > cap + 1e-9) return `发货量不得超过有效预占 ${cap}`
    }
  }

  if (draft.type === "SUPPLIER_DIRECT") {
    if (!draft.carrier.trim()) return "请填写承运方"
    if (!draft.trackingNo.trim()) return "请填写物流单号"
    for (const line of draft.lines) {
      const qty = Number(line.quantity)
      if (!(qty > 0)) return "发货数量必须大于 0"
      if (!line.purchaseLineSalesAllocationId) {
        return "直发必须引用有效采购销售分配"
      }
      const src = task.lines.find((l) => l.salesOrderLineId === line.salesOrderLineId)
      if (!src) return "销售明细不存在"
      if (qty > Number(src.remainingQuantity) + 1e-9) {
        return `发货量不得超过剩余可发 ${src.remainingQuantity}`
      }
    }
  }

  if (draft.type === "ELECTRONIC") {
    if (!draft.recipientMasked.trim()) return "交付对象不能为空"
    for (const line of draft.lines) {
      const qty = Number(line.quantity)
      if (!(qty > 0)) return "交付数量必须大于 0"
      if (!line.purchaseLineSalesAllocationId) {
        return "电子交付必须引用有效采购销售分配"
      }
      const src = task.lines.find((l) => l.salesOrderLineId === line.salesOrderLineId)
      if (src && qty > Number(src.remainingQuantity) + 1e-9) {
        return `交付量不得超过剩余可交付 ${src.remainingQuantity}`
      }
    }
  }

  if (draft.type === "SERVICE") {
    if (!draft.serviceLocation.trim()) return "请填写服务地点"
    if (!draft.startedAt || !draft.endedAt) return "请填写服务起止时间"
    if (draft.endedAt < draft.startedAt) return "结束时间不得早于开始时间"
    if (!draft.completionNote.trim() || draft.completionNote.trim().length < 4) {
      return "请填写至少 4 个字的完成说明"
    }
    for (const line of draft.lines) {
      const qty = Number(line.quantity)
      if (!(qty > 0)) return "服务数量必须大于 0"
      if (!line.purchaseLineSalesAllocationId) {
        return "服务履约必须引用有效采购销售分配"
      }
    }
  }

  return null
}

function buildFormalOutcome(
  task: FulfillmentTask,
  draft: FulfillmentDraft,
  nextWorkItemId?: string
): FulfillmentFormalOutcome {
  const ts = new Date().toISOString()
  const short = task.workItemId.replace("wi_ff_", "").toUpperCase()

  if (draft.type === "RECEIPT") {
    const inventoryDelta = draft.lines.flatMap((line) => {
      const src = task.lines.find(
        (l) => l.purchaseRevisionLineId === line.purchaseRevisionLineId
      )
      const qual = line.qualifiedQuantity
      if (Number(qual) <= 0 || !src) return []
      return [
        {
          warehouseId: draft.warehouseId,
          warehouseLabel: draft.warehouseLabel,
          skuId: src.skuCode,
          skuLabel: src.itemName,
          quantity: qual,
          direction: "INCREASE" as const,
        },
      ]
    })
    const reservationDelta = draft.lines.flatMap((line) => {
      const src = task.lines.find(
        (l) => l.purchaseRevisionLineId === line.purchaseRevisionLineId
      )
      const qual = line.qualifiedQuantity
      if (Number(qual) <= 0 || !src) return []
      return [
        {
          reservationId: `rsv_new_${src.salesOrderLineId}`,
          quantity: qual,
          action: "CREATE" as const,
          salesOrderLineId: src.salesOrderLineId,
        },
      ]
    })
    const remainingByLine = task.lines.map((src) => {
      const line = draft.lines.find(
        (l) => l.purchaseRevisionLineId === src.purchaseRevisionLineId
      )
      const used =
        Number(line?.qualifiedQuantity ?? 0) + Number(line?.rejectedQuantity ?? 0)
      const rem = Math.max(0, Number(src.remainingQuantity) - used)
      return {
        salesOrderLineId: src.salesOrderLineId,
        itemName: src.itemName,
        quantity: String(rem),
      }
    })
    const qualTotal = draft.lines.reduce(
      (s, l) => s + Number(l.qualifiedQuantity || 0),
      0
    )
    const rejTotal = draft.lines.reduce(
      (s, l) => s + Number(l.rejectedQuantity || 0),
      0
    )
    return {
      kind: "POSTED",
      workItemId: task.workItemId,
      factType: "PURCHASE_RECEIPT",
      factId: `prcpt_${short}`,
      factNo: `RK${new Date().toISOString().slice(0, 10).replace(/-/g, "")}${short.slice(-4)}`,
      formalStatus: "POSTED",
      occurredAt: draft.occurredAt || ts,
      operationType: "RECEIPT",
      inventoryDelta,
      reservationDelta,
      remainingByLine,
      acceptanceRequired: false,
      acceptanceNextStep:
        "入库不触发客户验收。合格量已形成库存与销售预占；后续仓发后再由销售在 W06 验收。",
      inventoryImpactSummary: `合格 ${qualTotal} 入库存并建立预占；不合格 ${rejTotal} 不入库、不预占。`,
      reference: `FF-RK-${short}`,
      nextWorkItemId,
      salesOrderId: task.source.salesOrderId,
      salesOrderNo: task.source.salesOrderNo,
    }
  }

  if (draft.type === "WAREHOUSE_SHIP") {
    const inventoryDelta = draft.lines.flatMap((line) => {
      const src = task.lines.find((l) => l.salesOrderLineId === line.salesOrderLineId)
      if (!src) return []
      return [
        {
          warehouseId: draft.warehouseId,
          warehouseLabel: draft.warehouseLabel,
          skuId: src.skuCode,
          skuLabel: src.itemName,
          quantity: line.quantity,
          direction: "DECREASE" as const,
        },
      ]
    })
    const reservationDelta = draft.lines.map((line) => ({
      reservationId: line.stockReservationId,
      quantity: line.quantity,
      action: "CONSUME" as const,
      salesOrderLineId: line.salesOrderLineId,
    }))
    const remainingByLine = task.lines.map((src) => {
      const line = draft.lines.find((l) => l.salesOrderLineId === src.salesOrderLineId)
      const used = Number(line?.quantity ?? 0)
      const rem = Math.max(0, Number(src.remainingQuantity) - used)
      return {
        salesOrderLineId: src.salesOrderLineId,
        itemName: src.itemName,
        quantity: String(rem),
      }
    })
    return {
      kind: "POSTED",
      workItemId: task.workItemId,
      factType: "DELIVERY",
      factId: `dlv_wh_${short}`,
      factNo: `FH${new Date().toISOString().slice(0, 10).replace(/-/g, "")}${short.slice(-4)}`,
      formalStatus: "SHIPPED",
      occurredAt: draft.shippedAt || ts,
      operationType: "WAREHOUSE_SHIP",
      inventoryDelta,
      reservationDelta,
      remainingByLine,
      acceptanceRequired: true,
      acceptanceNextStep:
        "仓发记录已确认。物流签收不等于客户验收；请销售在 W06 登记客户验收。",
      inventoryImpactSummary:
        "已消耗本销售明细有效预占，并减少自有库存（不写采购付款流水）。",
      reference: `FF-FH-${short}`,
      nextWorkItemId,
      salesOrderId: task.source.salesOrderId,
      salesOrderNo: task.source.salesOrderNo,
    }
  }

  if (draft.type === "SUPPLIER_DIRECT") {
    const remainingByLine = task.lines.map((src) => {
      const line = draft.lines.find((l) => l.salesOrderLineId === src.salesOrderLineId)
      const used = Number(line?.quantity ?? 0)
      const rem = Math.max(0, Number(src.remainingQuantity) - used)
      return {
        salesOrderLineId: src.salesOrderLineId,
        itemName: src.itemName,
        quantity: String(rem),
      }
    })
    return {
      kind: "POSTED",
      workItemId: task.workItemId,
      factType: "DELIVERY",
      factId: `dlv_df_${short}`,
      factNo: `DF${new Date().toISOString().slice(0, 10).replace(/-/g, "")}${short.slice(-4)}`,
      formalStatus: "SHIPPED",
      occurredAt: draft.shippedAt || ts,
      operationType: "SUPPLIER_DIRECT",
      inventoryDelta: [],
      reservationDelta: [],
      remainingByLine,
      acceptanceRequired: true,
      acceptanceNextStep:
        "供应商直发记录已确认，不影响自有库存。请销售在 W06 登记客户验收（物流签收≠验收）。",
      inventoryImpactSummary: "不影响自有库存与销售预占流水。",
      reference: `FF-DF-${short}`,
      nextWorkItemId,
      salesOrderId: task.source.salesOrderId,
      salesOrderNo: task.source.salesOrderNo,
    }
  }

  if (draft.type === "ELECTRONIC") {
    const remainingByLine = task.lines.map((src) => {
      const line = draft.lines.find((l) => l.salesOrderLineId === src.salesOrderLineId)
      const used = Number(line?.quantity ?? 0)
      const rem = Math.max(0, Number(src.remainingQuantity) - used)
      return {
        salesOrderLineId: src.salesOrderLineId,
        itemName: src.itemName,
        quantity: String(rem),
      }
    })
    return {
      kind: "POSTED",
      workItemId: task.workItemId,
      factType: "ELECTRONIC_DELIVERY",
      factId: `ed_${short}`,
      factNo: `ED${new Date().toISOString().slice(0, 10).replace(/-/g, "")}${short.slice(-4)}`,
      formalStatus: draft.result === "FAILED" ? "FAILED" : "CONFIRMED",
      occurredAt: draft.occurredAt || ts,
      operationType: "ELECTRONIC",
      inventoryDelta: [],
      reservationDelta: [],
      remainingByLine,
      acceptanceRequired: draft.result !== "FAILED",
      acceptanceNextStep:
        draft.result === "FAILED"
          ? "电子交付失败已留痕，不可覆盖；重做须新建记录。不进入客户验收。"
          : "电子交付已确认，不影响自有库存。请销售在 W06 登记客户验收。",
      inventoryImpactSummary: "不影响自有库存。",
      reference: `FF-ED-${short}`,
      nextWorkItemId,
      salesOrderId: task.source.salesOrderId,
      salesOrderNo: task.source.salesOrderNo,
    }
  }

  // SERVICE
  const remainingByLine = task.lines.map((src) => {
    const line = draft.lines.find((l) => l.salesOrderLineId === src.salesOrderLineId)
    const used = Number(line?.quantity ?? 0)
    const rem = Math.max(0, Number(src.remainingQuantity) - used)
    return {
      salesOrderLineId: src.salesOrderLineId,
      itemName: src.itemName,
      quantity: String(rem),
    }
  })
  return {
    kind: "POSTED",
    workItemId: task.workItemId,
    factType: "SERVICE_FULFILLMENT",
    factId: `svc_${short}`,
    factNo: `FW${new Date().toISOString().slice(0, 10).replace(/-/g, "")}${short.slice(-4)}`,
    formalStatus: draft.result === "FAILED" ? "FAILED" : "CONFIRMED",
    occurredAt: draft.endedAt || ts,
    operationType: "SERVICE",
    inventoryDelta: [],
    reservationDelta: [],
    remainingByLine,
    acceptanceRequired: draft.result !== "FAILED",
    acceptanceNextStep:
      draft.result === "FAILED"
        ? "服务失败已留痕，不可覆盖；重做须新建记录。"
        : "服务履约已确认。请销售在 W06 登记客户验收。",
    inventoryImpactSummary: "不影响自有库存。",
    reference: `FF-FW-${short}`,
    nextWorkItemId,
    salesOrderId: task.source.salesOrderId,
    salesOrderNo: task.source.salesOrderNo,
  }
}

export async function postFulfillmentOperation(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectHash: string
  expectedSourceVersion: string
  expectedEditVersion: number
  draft: FulfillmentDraft
  idempotencyKey: string
  nextWorkItemId?: string
  forceUnknown?: boolean
}): Promise<FormalActionResponse> {
  await mockDelay(150)

  const cachedBiz = getFulfillmentBusinessOutcome(input.workItemId)
  const idem = getIdempotencyEntry(input.idempotencyKey)
  if (idem?.state === "succeeded") {
    const payload = idem.payload as { formalOutcome?: FulfillmentFormalOutcome }
    if (payload?.formalOutcome) {
      return { status: "succeeded", outcome: payload.formalOutcome }
    }
    if (cachedBiz) {
      return {
        status: "succeeded",
        outcome: cachedBiz as FulfillmentFormalOutcome,
      }
    }
  }
  if (idem?.state === "pending" || input.forceUnknown) {
    try {
      if (input.forceUnknown) {
        completeWorkItemSession({
          workItemId: input.workItemId,
          claimToken: input.claimToken,
          leaseVersion: input.leaseVersion,
          expectedSubjectHash: input.expectedSubjectHash,
          idempotencyKey: input.idempotencyKey,
          decision: { kind: "FULFILLMENT_POSTED" },
          simulateTimeout: true,
        })
      }
    } catch (error) {
      if (error instanceof WorkItemMockError && error.code === "TIMEOUT") {
        return {
          status: "unknown",
          message:
            "处理结果尚未确定。请勿假定库存/预占/履约已变更，停留当前项并按原任务号查询。",
          idempotencyKey: input.idempotencyKey,
        }
      }
    }
    if (idem?.state === "pending") {
      return {
        status: "unknown",
        message:
          "处理结果尚未确定。请勿假定库存/预占/履约已变更，停留当前项并按原任务号查询。",
        idempotencyKey: input.idempotencyKey,
      }
    }
  }

  const seed = FULFILLMENT_OPERATIONS_SEED.find(
    (t) => t.workItemId === input.workItemId
  )
  if (!seed) {
    return { status: "failed", code: "NOT_FOUND", message: "任务不存在" }
  }
  if (seed.subjectHash !== input.expectedSubjectHash) {
    return {
      status: "failed",
      code: "SUBJECT_HASH_MISMATCH",
      message: "来源版本数据已变化，请刷新后处理最新上下文",
    }
  }

  const projected = projectTask(seed)
  if (!projected) {
    return { status: "failed", code: "ALREADY_DONE", message: "任务已完成" }
  }

  // 合并会话草稿版本校验：提交时使用请求中的 draft
  const validationError = validateDraft(projected, input.draft)
  if (validationError) {
    return {
      status: "failed",
      code: "VALIDATION_BLOCKED",
      message: validationError,
    }
  }

  const outcome = buildFormalOutcome(
    projected,
    input.draft,
    input.nextWorkItemId
  )

  try {
    completeWorkItemSession({
      workItemId: input.workItemId,
      claimToken: input.claimToken,
      leaseVersion: input.leaseVersion,
      expectedSubjectHash: input.expectedSubjectHash,
      idempotencyKey: input.idempotencyKey,
      decision: {
        kind: `FULFILLMENT_${input.draft.type}`,
        summary: `${OPERATION_TYPE_LABEL[input.draft.type]}过账 ${outcome.factNo}`,
      },
    })
    setFulfillmentBusinessOutcome(input.workItemId, outcome)
    markQueueTaskCompleted("W09", input.workItemId)
    const entry = getIdempotencyEntry(input.idempotencyKey)
    if (entry?.state === "succeeded") {
      setIdempotencySucceeded(input.idempotencyKey, "COMPLETE", {
        ...(entry.payload as object),
        formalOutcome: outcome,
      })
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

export async function deferFulfillmentOperation(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  queueContextId: string
  reasonCode: DeferReasonCode
  reasonNote?: string
  nextWorkItemId?: string
  idempotencyKey: string
}): Promise<FormalActionResponse> {
  await mockDelay(100)
  const seed = FULFILLMENT_OPERATIONS_SEED.find(
    (t) => t.workItemId === input.workItemId
  )
  if (!seed) {
    return { status: "failed", code: "NOT_FOUND", message: "任务不存在" }
  }
  if (!input.reasonCode) {
    return { status: "failed", code: "REASON_REQUIRED", message: "暂挂须选择原因" }
  }
  try {
    applyWorkItemActionSession({
      workItemId: input.workItemId,
      claimToken: input.claimToken,
      leaseVersion: input.leaseVersion,
      expectedSubjectHash: seed.subjectHash,
      idempotencyKey: input.idempotencyKey,
      action: {
        kind: "DEFER",
        note: `${input.reasonCode}${input.reasonNote ? `: ${input.reasonNote}` : ""}`,
      },
    })
    markQueueTaskHeld("W09", input.workItemId)
    return {
      status: "succeeded",
      outcome: {
        kind: "DEFERRED",
        workItemId: input.workItemId,
        workItemStatus: "PENDING",
        leaseDisposition: "RELEASED",
        reasonCode: input.reasonCode,
        reasonNote: input.reasonNote,
        nextWorkItemId: input.nextWorkItemId,
        reference: `FF-HOLD-${input.workItemId.toUpperCase()}`,
      },
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      return { status: "failed", code: error.code, message: error.message }
    }
    throw error
  }
}

export async function resolveUnknownFulfillmentResult(input: {
  idempotencyKey: string
  settle?: boolean
  settlePayload?: Parameters<typeof postFulfillmentOperation>[0]
}): Promise<FormalActionResponse> {
  await mockDelay(80)
  const entry = queryIdempotencyResult(input.idempotencyKey)
  if (entry?.state === "succeeded") {
    const payload = entry.payload as { formalOutcome?: FulfillmentFormalOutcome }
    if (payload?.formalOutcome) {
      return { status: "succeeded", outcome: payload.formalOutcome }
    }
    const biz = input.settlePayload
      ? getFulfillmentBusinessOutcome(input.settlePayload.workItemId)
      : null
    if (biz) {
      return { status: "succeeded", outcome: biz as FulfillmentFormalOutcome }
    }
  }
  if (entry?.state === "pending" && input.settle && input.settlePayload) {
    try {
      finalizePendingComplete({
        idempotencyKey: input.idempotencyKey,
        workItemId: input.settlePayload.workItemId,
        expectedSubjectHash: input.settlePayload.expectedSubjectHash,
        decision: {
          kind: `FULFILLMENT_${input.settlePayload.draft.type}`,
        },
      })
      const seed = FULFILLMENT_OPERATIONS_SEED.find(
        (t) => t.workItemId === input.settlePayload!.workItemId
      )
      if (!seed) {
        return { status: "failed", code: "NOT_FOUND", message: "任务不存在" }
      }
      const projected = projectTask(seed) ?? seed
      const outcome = buildFormalOutcome(
        projected,
        input.settlePayload.draft,
        input.settlePayload.nextWorkItemId
      )
      setFulfillmentBusinessOutcome(input.settlePayload.workItemId, outcome)
      markQueueTaskCompleted("W09", input.settlePayload.workItemId)
      setIdempotencySucceeded(input.idempotencyKey, "COMPLETE", {
        formalOutcome: outcome,
      })
      return { status: "succeeded", outcome }
    } catch (error) {
      if (error instanceof WorkItemMockError) {
        return { status: "failed", code: error.code, message: error.message }
      }
      throw error
    }
  }
  if (entry?.state === "pending") {
    return {
      status: "unknown",
      message: "仍在处理中，处理结果待确认。停留当前项，不移动队列、不改库存。",
      idempotencyKey: input.idempotencyKey,
    }
  }
  return {
    status: "failed",
    code: "NO_PENDING",
    message: "未找到该任务号对应的处理中请求",
  }
}

export async function renewFulfillmentLease(input: {
  workItemId: string
  claimToken: string
}): Promise<WorkItemLease> {
  await mockDelay(60)
  const existing = getSessionLease(input.workItemId)
  if (!existing || existing.claimToken !== input.claimToken) {
    throw new Error("操作已失效，请重新领取")
  }
  clearSessionLease(input.workItemId)
  return claimFulfillmentWorkItem(input.workItemId)
}
