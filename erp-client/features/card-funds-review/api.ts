/**
 * W13 session-mock API：queryFn / mutationFn 纯函数。
 * claimToken 仅出现在领取/续租响应；正式完成使用 CompleteWorkItemEnvelope 语义。
 */

import { mockDelay } from "@/features/workspace-kit/delay"
import type {
  AllocationDraftLine,
  CardFundsReviewDecision,
  CardFundsReviewItemView,
  CardFundsReviewQueueQuery,
  CardFundsReviewQueueView,
  FormalActionResponse,
  FormalOutcome,
  RegisterFundsResult,
  WorkItemLease,
} from "@/features/card-funds-review/types"
import {
  APPROVE_CONCLUSION_LABEL,
  REJECT_FOLLOW_UP_COLLABORATION,
} from "@/features/card-funds-review/types"
import { CARD_FUNDS_REVIEW_SEED } from "@/mock/card-funds-review"
import {
  applyWorkItemActionSession,
  appendW13Review,
  bumpW13SubjectHash,
  claimWorkItemSession,
  clearSessionLease,
  completeWorkItemSession,
  finalizePendingComplete,
  getIdempotencyEntry,
  getSessionLease,
  getSessionLeaseState,
  getW13AppendedReviews,
  getW13BusinessOutcome,
  getW13FundsOverlay,
  isW13WorkItemHeld,
  isW13WorkItemTerminal,
  markQueueTaskCompleted,
  markQueueTaskHeld,
  queryIdempotencyResult,
  setIdempotencySucceeded,
  setW13BusinessOutcome,
  setW13FundsOverlay,
  WorkItemMockError,
} from "@/mock/session-state"

function moneyNum(v: string): number {
  return Number(v) || 0
}

function moneyStr(n: number): string {
  return n.toFixed(2)
}

function recomputeHash(parts: {
  accountId: string
  fundsFactVersion: string
  settled: string
  invoiced: string
  gross: string
  revisionId: string
}): string {
  // Mock-only normalized fingerprint; real hash is server-side.
  return `sha256:${parts.accountId}_${parts.revisionId}_${parts.fundsFactVersion}_s${parts.settled}_i${parts.invoiced}_g${parts.gross}`
}

function projectItem(
  seed: CardFundsReviewItemView
): CardFundsReviewItemView | null {
  if (isW13WorkItemTerminal(seed.workItem.workItemId)) return null

  const held = isW13WorkItemHeld(seed.workItem.workItemId)
  const publicLease = getSessionLeaseState(seed.workItem.workItemId)
  const overlay = getW13FundsOverlay(seed.workItem.workItemId)
  const appended = getW13AppendedReviews(seed.workItem.workItemId)

  const settledTotal = overlay?.settledTotal ?? seed.account.settledTotal
  const invoicedTotal = overlay?.invoicedTotal ?? seed.account.invoicedTotal
  const gross = seed.account.grossTotal
  const openTotal =
    overlay?.openTotal ??
    moneyStr(Math.max(0, moneyNum(gross) - moneyNum(settledTotal)))
  const openInvoiceableTotal =
    overlay?.openInvoiceableTotal ??
    moneyStr(Math.max(0, moneyNum(gross) - moneyNum(invoicedTotal)))
  const fundsFactVersion =
    overlay?.fundsFactVersion ?? seed.fundsFactVersion
  const subjectHash = overlay?.subjectHash ?? seed.workItem.subjectHash
  const receiptFacts = overlay?.receiptFacts ?? seed.receiptFacts
  const invoiceFacts = overlay?.invoiceFacts ?? seed.invoiceFacts

  const settled = moneyNum(settledTotal)
  const invoiced = moneyNum(invoicedTotal)
  const canConfirmZero =
    seed.reviewType === "OPENING" && settled === 0 && invoiced === 0

  const blockers = [...seed.workItem.actionBlockers]
  if (!canConfirmZero) {
    if (!blockers.some((b) => b.action === "CONFIRM_ZERO")) {
      blockers.push({
        action: "CONFIRM_ZERO",
        code:
          seed.reviewType !== "OPENING"
            ? "NOT_OPENING"
            : "SETTLED_OR_INVOICED_NOT_ZERO",
        message:
          seed.reviewType !== "OPENING"
            ? "「从 0 起」仅适用于 OPENING 期初任务"
            : "净已收或净已开不为 0，不能使用「从 0 起」结论",
      })
    }
  }

  const chainItems = [
    ...seed.reviewChain.items,
    ...appended.map((a) => ({
      ...a,
      conclusion: a.conclusion as CardFundsReviewItemView["reviewChain"]["items"][number]["conclusion"],
      readOnly: true as const,
    })),
  ]

  const fundsChanged = Boolean(overlay)
  const fingerprintStatus = overlay?.forceHashDriftOnComplete
    ? {
        label: "指纹已漂移",
        tone: "destructive" as const,
        detail: "外部事实变化：完成时须使用最新 subject_hash，旧期望将阻断",
      }
    : fundsChanged
      ? {
          label: "票款已更新",
          tone: "warning" as const,
          detail: "登记后已重算指纹与金额；请核对后再正式完成",
        }
      : seed.fingerprintStatus

  return {
    ...seed,
    workItem: {
      ...seed.workItem,
      subjectHash,
      workItemStatus: held ? "IN_PROGRESS" : seed.workItem.workItemStatus,
      held,
      leaseVersion: publicLease?.leaseVersion ?? seed.workItem.leaseVersion,
      leaseExpiresAt: publicLease?.leaseExpiresAt ?? seed.workItem.leaseExpiresAt,
      claimedBy: publicLease
        ? { userId: "user_finance", displayName: "当前用户 · 王敏" }
        : seed.workItem.claimedBy,
      actionBlockers: blockers,
      allowedActions: canConfirmZero
        ? seed.workItem.allowedActions.includes("CONFIRM_ZERO")
          ? seed.workItem.allowedActions
          : [...seed.workItem.allowedActions, "CONFIRM_ZERO"]
        : seed.workItem.allowedActions.filter((a) => a !== "CONFIRM_ZERO"),
    },
    account: {
      ...seed.account,
      settledTotal,
      invoicedTotal,
      openTotal,
      openInvoiceableTotal,
      fundsReliability: held
        ? seed.account.fundsReliability
        : seed.account.fundsReliability,
      reliabilityNote: seed.account.reliabilityNote,
    },
    fundsFactVersion,
    receiptFacts,
    invoiceFacts,
    reviewChain: {
      ...seed.reviewChain,
      items: chainItems,
      nextReviewNo: seed.reviewChain.nextReviewNo + appended.length,
      tailReviewId:
        chainItems.length > 0
          ? chainItems[chainItems.length - 1]!.reviewId
          : seed.reviewChain.tailReviewId,
    },
    fingerprintStatus,
    currentEvidence: {
      evidenceDocumentIds:
        overlay?.evidenceDocumentIds ?? seed.currentEvidence.evidenceDocumentIds,
      evidenceReferences:
        overlay?.evidenceReferences ?? seed.currentEvidence.evidenceReferences,
      comment: overlay?.comment ?? seed.currentEvidence.comment,
    },
  }
}

function filterSummary(q: CardFundsReviewQueueQuery): string {
  const parts = [
    q.scope === "mine" ? "仅我的" : "角色池",
    q.type === "opening"
      ? "期初 OPENING"
      : q.type === "delta"
        ? "差额 SYNC_DELTA"
        : "全部类型",
    q.status === "held" ? "已暂挂" : "待处理有效队列",
    q.due === "overdue" ? "已超期" : q.due === "today" ? "今日到期" : "全部时限",
  ]
  if (q.q) parts.push(`搜索 ${q.q}`)
  return parts.join(" · ")
}

export async function fetchCardFundsReviewQueue(
  query: CardFundsReviewQueueQuery
): Promise<CardFundsReviewQueueView> {
  await mockDelay()
  let tasks = CARD_FUNDS_REVIEW_SEED.map(projectItem).filter(
    (t): t is CardFundsReviewItemView => t != null
  )

  if (query.type === "opening") {
    tasks = tasks.filter((t) => t.reviewType === "OPENING")
  } else if (query.type === "delta") {
    tasks = tasks.filter((t) => t.reviewType === "SYNC_DELTA")
  }

  if (query.status === "held") {
    tasks = tasks.filter((t) => t.workItem.held)
  } else {
    // 正常有效队列含 PENDING/IN_PROGRESS（含已暂挂）；不混入已完成
    // 已暂挂仍出现在 pending 中，另可切 held 子集
  }

  if (query.q?.trim()) {
    const q = query.q.trim().toUpperCase()
    tasks = tasks.filter(
      (t) =>
        t.salesOrder.orderNo.toUpperCase().includes(q) ||
        t.account.customerName.toUpperCase().includes(q) ||
        t.account.counterpartyPartyName.toUpperCase().includes(q)
    )
  }

  if (query.due === "overdue") {
    const now = Date.now()
    tasks = tasks.filter(
      (t) => t.workItem.dueAt && new Date(t.workItem.dueAt).getTime() < now
    )
  } else if (query.due === "today") {
    const today = new Date().toISOString().slice(0, 10)
    tasks = tasks.filter((t) => t.workItem.dueAt?.startsWith(today))
  }

  tasks = [...tasks].sort((a, b) => b.workItem.priority - a.workItem.priority)

  const queueContextId =
    query.queueContextId ?? `queue:card-funds-review:${query.scope}`

  let position = 0
  let current = tasks[0]
  if (query.currentWorkItemId) {
    const idx = tasks.findIndex(
      (t) => t.workItem.workItemId === query.currentWorkItemId
    )
    if (idx >= 0) {
      position = idx
      current = tasks[idx]
    }
  }

  const emptyReason = CARD_FUNDS_REVIEW_SEED.every((t) =>
    isW13WorkItemTerminal(t.workItem.workItemId)
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
      currentWorkItemId: current?.workItem.workItemId,
      previousWorkItemId: tasks[position - 1]?.workItem.workItemId,
      nextWorkItemId: tasks[position + 1]?.workItem.workItemId,
      filterSummary: filterSummary(query),
      queueContextUpdatedAt: new Date().toISOString(),
    },
    tasks,
    current,
    emptyReason,
  }
}

export async function claimCardFundsReviewWorkItem(
  workItemId: string
): Promise<WorkItemLease> {
  await mockDelay(80)
  const seed = CARD_FUNDS_REVIEW_SEED.find(
    (t) => t.workItem.workItemId === workItemId
  )
  if (!seed) throw new Error("任务不存在")
  if (isW13WorkItemTerminal(workItemId)) {
    throw new Error("任务已完成，无法领取")
  }
  const projected = projectItem(seed)
  try {
    const lease = claimWorkItemSession({
      workItemId,
      subjectVersion: projected?.workItem.subjectVersion ?? seed.workItem.subjectVersion,
      subjectHash: projected?.workItem.subjectHash ?? seed.workItem.subjectHash,
      leaseVersion: seed.workItem.leaseVersion ?? 1,
      ownerUserId: "user_finance",
    })
    return {
      workItemId,
      claimedByLabel: "当前用户 · 王敏",
      expiresAt: lease.leaseExpiresAt,
      leaseVersion: lease.leaseVersion,
      claimToken: lease.claimToken,
    }
  } catch (error) {
    if (error instanceof WorkItemMockError) throw new Error(error.message)
    throw error
  }
}

export async function holdCardFundsReview(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  reasonCode: string
  note?: string
  idempotencyKey: string
  nextWorkItemId?: string
}): Promise<FormalActionResponse> {
  await mockDelay(100)
  const seed = CARD_FUNDS_REVIEW_SEED.find(
    (t) => t.workItem.workItemId === input.workItemId
  )
  if (!seed) {
    return { status: "failed", code: "NOT_FOUND", message: "任务不存在" }
  }
  const projected = projectItem(seed)
  try {
    applyWorkItemActionSession({
      workItemId: input.workItemId,
      claimToken: input.claimToken,
      leaseVersion: input.leaseVersion,
      expectedSubjectHash:
        projected?.workItem.subjectHash ?? seed.workItem.subjectHash,
      idempotencyKey: input.idempotencyKey,
      action: {
        kind: "HOLD",
        note: input.note ?? input.reasonCode,
      },
    })
    markQueueTaskHeld("W13", input.workItemId)
    clearSessionLease(input.workItemId)
    const outcome: FormalOutcome = {
      kind: "HELD",
      workItemId: input.workItemId,
      workItemStatus: "IN_PROGRESS",
      heldAt: new Date().toISOString(),
      resumeHint:
        "任务仍为 PENDING/IN_PROGRESS，已暂挂标记保留在有效队列；可在「已暂挂」范围查看并手动恢复。未形成复核事实。",
      reference: `W13-HOLD-${input.workItemId.toUpperCase()}`,
      nextWorkItemId: input.nextWorkItemId,
    }
    return { status: "succeeded", outcome }
  } catch (error) {
    if (error instanceof WorkItemMockError) {
      return { status: "failed", code: error.code, message: error.message }
    }
    throw error
  }
}

function validateDecisionAgainstItem(
  item: CardFundsReviewItemView,
  decision: CardFundsReviewDecision
): { ok: true } | { ok: false; code: string; message: string } {
  if (decision.expectedSubjectHash !== item.workItem.subjectHash) {
    return {
      ok: false,
      code: "SUBJECT_HASH_MISMATCH",
      message:
        "复核对象 subject_hash 已变化（任务/事实/提交三方不一致），已阻断静默通过。请刷新后重新核对。",
    }
  }
  if (decision.expectedFundsFactVersion !== item.fundsFactVersion) {
    return {
      ok: false,
      code: "FUNDS_VERSION_MISMATCH",
      message: "票款事实版本已变化，请刷新后重审",
    }
  }
  if (
    decision.expectedAccountDomainVersion !== item.account.domainVersion ||
    decision.receivableAccountId !== item.account.id
  ) {
    return {
      ok: false,
      code: "ACCOUNT_VERSION_MISMATCH",
      message: "应收账户版本或身份不匹配",
    }
  }
  if (decision.reviewType !== item.reviewType) {
    return {
      ok: false,
      code: "REVIEW_TYPE_MISMATCH",
      message: "复核类型与当前任务不一致，禁止覆盖期初/差额类型",
    }
  }
  if (
    decision.evidenceDocumentIds.length === 0 &&
    decision.evidenceReferences.length === 0
  ) {
    return {
      ok: false,
      code: "EVIDENCE_REQUIRED",
      message: "完成复核时证据不能为空",
    }
  }
  if (decision.reviewResult === "APPROVED") {
    if (decision.conclusion === "NO_HISTORY_FROM_ZERO") {
      if (item.reviewType !== "OPENING") {
        return {
          ok: false,
          code: "ZERO_ONLY_OPENING",
          message: "「从 0 起」仅允许 OPENING + APPROVED",
        }
      }
      if (
        moneyNum(item.account.settledTotal) !== 0 ||
        moneyNum(item.account.invoicedTotal) !== 0
      ) {
        return {
          ok: false,
          code: "ZERO_REQUIRES_ZERO_BALANCES",
          message: "净已收/净已开不为 0，不能从 0 起；且不会创建 0 元回款/发票",
        }
      }
    }
  }
  if (decision.reviewResult === "REJECTED" && !decision.comment?.trim()) {
    return {
      ok: false,
      code: "REJECT_COMMENT_REQUIRED",
      message: "驳回原因与说明必填",
    }
  }
  return { ok: true }
}

export async function completeCardFundsReview(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  expectedSubjectHash: string
  expectedSubjectVersion: string
  idempotencyKey: string
  forceUnknown?: boolean
  /** Demo: force server hash drift before validate */
  simulateHashDrift?: boolean
  decision: CardFundsReviewDecision
}): Promise<FormalActionResponse> {
  await mockDelay(150)

  const cachedBiz = getW13BusinessOutcome(input.workItemId)
  const idem = getIdempotencyEntry(input.idempotencyKey)
  if (idem?.state === "succeeded") {
    const payload = idem.payload as { formalOutcome?: FormalOutcome }
    if (payload?.formalOutcome) {
      return { status: "succeeded", outcome: payload.formalOutcome }
    }
    if (cachedBiz) {
      const outcome: FormalOutcome =
        cachedBiz.reviewResult === "APPROVED"
          ? {
              kind: "APPROVED",
              business: {
                ...cachedBiz,
                conclusion: cachedBiz.conclusion as
                  | "NO_HISTORY_FROM_ZERO"
                  | "RECORDED_FACTS_RECONCILED",
              },
            }
          : {
              kind: "REJECTED",
              business: {
                ...cachedBiz,
                conclusion: "REJECTED",
              },
            }
      return { status: "succeeded", outcome }
    }
  }

  if (input.simulateHashDrift) {
    bumpW13SubjectHash(
      input.workItemId,
      `sha256:drift_${input.workItemId}_${Date.now()}`
    )
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
          decision: {
            kind:
              input.decision.reviewResult === "APPROVED"
                ? "CARD_FUNDS_APPROVED"
                : "CARD_FUNDS_REJECTED",
          },
          simulateTimeout: true,
        })
      }
    } catch (error) {
      if (error instanceof WorkItemMockError && error.code === "TIMEOUT") {
        return {
          status: "unknown",
          message:
            "正式结果尚未确定。请勿假定已通过或已驳回，停留当前项并用同一幂等键查询。",
          idempotencyKey: input.idempotencyKey,
        }
      }
    }
    if (idem?.state === "pending") {
      return {
        status: "unknown",
        message:
          "正式结果尚未确定。请勿假定已通过或已驳回，停留当前项并用同一幂等键查询。",
        idempotencyKey: input.idempotencyKey,
      }
    }
  }

  const seed = CARD_FUNDS_REVIEW_SEED.find(
    (t) => t.workItem.workItemId === input.workItemId
  )
  if (!seed) {
    return { status: "failed", code: "NOT_FOUND", message: "任务不存在" }
  }
  const item = projectItem(seed)
  if (!item) {
    return { status: "failed", code: "ALREADY_DONE", message: "任务已完成" }
  }

  // 完成前重新取得当前事实并校验 decision 中的期望指纹
  const validation = validateDecisionAgainstItem(item, input.decision)
  if (!validation.ok) {
    return {
      status: "failed",
      code: validation.code,
      message: validation.message,
    }
  }

  // 信封层 subject_hash 与当前事实一致
  if (input.expectedSubjectHash !== item.workItem.subjectHash) {
    return {
      status: "failed",
      code: "SUBJECT_HASH_MISMATCH",
      message:
        "任务信封 subject_hash 与当前事实不一致，已阻断。请刷新后重审。",
    }
  }

  const reviewNo = item.reviewChain.nextReviewNo
  const reviewId = `rfr_${input.workItemId}_${reviewNo}`
  const completedAt = new Date().toISOString()
  const operationId = `op_w13_${input.idempotencyKey.slice(0, 12)}`
  const workflowActionId = `wa_w13_${input.workItemId}_${reviewNo}`

  if (input.decision.reviewResult === "APPROVED") {
    const business = {
      receivableFundsReviewId: reviewId,
      receivableAccountId: item.account.id,
      reviewNo,
      accountReviewStatus:
        item.reviewType === "OPENING" ? "OPENING_APPROVED" : "DELTA_APPROVED",
      workflowActionId,
      operationId,
      completedAt,
      reviewResult: "APPROVED" as const,
      conclusion: input.decision.conclusion,
      subjectHash: item.workItem.subjectHash,
      reference: `W13-OK-${String(reviewNo).padStart(4, "0")}`,
    }
    try {
      completeWorkItemSession({
        workItemId: input.workItemId,
        claimToken: input.claimToken,
        leaseVersion: input.leaseVersion,
        expectedSubjectHash: input.expectedSubjectHash,
        idempotencyKey: input.idempotencyKey,
        decision: {
          kind: "CARD_FUNDS_APPROVED",
          summary: `复核号 ${reviewNo} · ${APPROVE_CONCLUSION_LABEL[input.decision.conclusion]}`,
        },
      })
      appendW13Review(input.workItemId, {
        reviewId,
        reviewNo,
        reviewType: item.reviewType,
        reviewResult: "APPROVED",
        conclusion: input.decision.conclusion,
        reviewerLabel: "当前用户 · 王敏",
        completedAt,
        subjectHashAtReview: item.workItem.subjectHash,
        predecessorReviewId: item.reviewChain.tailReviewId,
      })
      setW13BusinessOutcome(input.workItemId, business)
      markQueueTaskCompleted("W13", input.workItemId)
      const outcome: FormalOutcome = { kind: "APPROVED", business }
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
        if (error.code === "VERSION_CONFLICT") {
          return {
            status: "failed",
            code: "SUBJECT_HASH_MISMATCH",
            message: error.message,
          }
        }
        return { status: "failed", code: error.code, message: error.message }
      }
      throw error
    }
  }

  // REJECTED — 只形成驳回复核事实并完成当前任务；固定 blocker，不建后继
  const business = {
    receivableFundsReviewId: reviewId,
    receivableAccountId: item.account.id,
    reviewNo,
    accountReviewStatus: "REJECTED",
    workflowActionId,
    operationId,
    completedAt,
    reviewResult: "REJECTED" as const,
    conclusion: "REJECTED" as const,
    subjectHash: item.workItem.subjectHash,
    reference: `W13-REJ-${String(reviewNo).padStart(4, "0")}`,
    followUpConfiguration: {
      status: "BLOCKED" as const,
      blockerCode: "REJECT_FOLLOW_UP_WORK_ITEM_NOT_REGISTERED" as const,
      collaborationMessage: REJECT_FOLLOW_UP_COLLABORATION,
      requiredRegistration: [
        "WORK_ITEM_TYPE" as const,
        "OWNER_POOL" as const,
        "HANDLER_KEY" as const,
      ],
    },
  }

  try {
    completeWorkItemSession({
      workItemId: input.workItemId,
      claimToken: input.claimToken,
      leaseVersion: input.leaseVersion,
      expectedSubjectHash: input.expectedSubjectHash,
      idempotencyKey: input.idempotencyKey,
      decision: {
        kind: "CARD_FUNDS_REJECTED",
        summary: `复核号 ${reviewNo} · 驳回 · 后继未配置`,
      },
    })
    appendW13Review(input.workItemId, {
      reviewId,
      reviewNo,
      reviewType: item.reviewType,
      reviewResult: "REJECTED",
      conclusion: "REJECTED",
      reviewerLabel: "当前用户 · 王敏",
      completedAt,
      subjectHashAtReview: item.workItem.subjectHash,
      predecessorReviewId: item.reviewChain.tailReviewId,
    })
    setW13BusinessOutcome(input.workItemId, business)
    markQueueTaskCompleted("W13", input.workItemId)
    const outcome: FormalOutcome = { kind: "REJECTED", business }
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

/**
 * 登记历史回款：形成正式回款事实 + 多对多分配，不写累计覆盖字段。
 * 不创建 0 元回款。返回后刷新金额与 subject_hash。
 */
export async function registerHistoricalReceipt(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  receiptNo: string
  receivedAt: string
  grossAmount: string
  allocations: readonly AllocationDraftLine[]
  evidenceReference: string
  idempotencyKey: string
}): Promise<RegisterFundsResult> {
  await mockDelay(120)
  const seed = CARD_FUNDS_REVIEW_SEED.find(
    (t) => t.workItem.workItemId === input.workItemId
  )
  if (!seed) throw new Error("任务不存在")
  if (moneyNum(input.grossAmount) <= 0) {
    throw new Error("禁止创建 0 元或负金额回款；无历史票款请使用「从 0 起」")
  }
  const item = projectItem(seed)
  if (!item) throw new Error("任务已完成")

  const allocated = input.allocations.reduce(
    (s, a) => s + moneyNum(a.amount),
    0
  )
  if (Math.abs(allocated - moneyNum(input.grossAmount)) > 0.001) {
    throw new Error("分配合计须等于回款含税金额（多对多核销，禁止覆盖汇总字段）")
  }

  try {
    applyWorkItemActionSession({
      workItemId: input.workItemId,
      claimToken: input.claimToken,
      leaseVersion: input.leaseVersion,
      expectedSubjectHash: item.workItem.subjectHash,
      idempotencyKey: input.idempotencyKey,
      action: { kind: "REGISTER_RECEIPT", note: input.receiptNo },
    })
  } catch (error) {
    if (error instanceof WorkItemMockError) throw new Error(error.message)
    throw error
  }

  const toAccount = input.allocations
    .filter((a) => a.targetAccountId === item.account.id)
    .reduce((s, a) => s + moneyNum(a.amount), 0)

  const newReceipt = {
    receiptId: `rcpt_${input.workItemId}_${Date.now().toString(36)}`,
    receiptNo: input.receiptNo,
    receivedAt: input.receivedAt,
    grossAmount: moneyStr(moneyNum(input.grossAmount)),
    allocatedToAccount: moneyStr(toAccount),
    otherAllocationSummary:
      allocated > toAccount
        ? `同主体其它分配 ${moneyStr(allocated - toAccount)}`
        : "同主体其它应收 0",
    reversed: false,
  }

  const receiptFacts = [...item.receiptFacts, newReceipt]
  const settledTotal = moneyStr(
    receiptFacts
      .filter((r) => !r.reversed)
      .reduce((s, r) => s + moneyNum(r.allocatedToAccount), 0)
  )
  const invoicedTotal = item.account.invoicedTotal
  const openTotal = moneyStr(
    Math.max(0, moneyNum(item.account.grossTotal) - moneyNum(settledTotal))
  )
  const openInvoiceableTotal = item.account.openInvoiceableTotal
  const fundsFactVersion = `ffv_${input.workItemId}_${Date.now().toString(36)}`
  const subjectHash = recomputeHash({
    accountId: item.account.id,
    fundsFactVersion,
    settled: settledTotal,
    invoiced: invoicedTotal,
    gross: item.account.grossTotal,
    revisionId: item.currentSalesOrderRevisionId,
  })

  setW13FundsOverlay(input.workItemId, {
    fundsFactVersion,
    subjectHash,
    settledTotal,
    invoicedTotal,
    openTotal,
    openInvoiceableTotal,
    receiptFacts: [...receiptFacts],
    invoiceFacts: [...item.invoiceFacts],
    evidenceDocumentIds: item.currentEvidence.evidenceDocumentIds,
    evidenceReferences: [
      ...item.currentEvidence.evidenceReferences,
      input.evidenceReference,
    ].filter(Boolean),
    comment: item.currentEvidence.comment,
  })

  // 同步会话 subject，使后续 complete 三方校验使用新指纹
  bumpW13SubjectHash(input.workItemId, subjectHash)
  // 清除 force drift 标记（登记引起的合法更新）
  const ov = getW13FundsOverlay(input.workItemId)
  if (ov) {
    setW13FundsOverlay(input.workItemId, {
      ...ov,
      forceHashDriftOnComplete: false,
      subjectHash,
    })
  }

  return {
    fundsFactVersion,
    subjectHash,
    settledTotal,
    invoicedTotal,
    openTotal,
    openInvoiceableTotal,
    receiptFacts,
    invoiceFacts: item.invoiceFacts,
  }
}

export async function registerHistoricalInvoice(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  invoiceNo: string
  issuedAt: string
  grossAmount: string
  netAmount: string
  taxAmount: string
  allocations: readonly AllocationDraftLine[]
  evidenceReference: string
  idempotencyKey: string
}): Promise<RegisterFundsResult> {
  await mockDelay(120)
  const seed = CARD_FUNDS_REVIEW_SEED.find(
    (t) => t.workItem.workItemId === input.workItemId
  )
  if (!seed) throw new Error("任务不存在")
  if (moneyNum(input.grossAmount) <= 0) {
    throw new Error("禁止创建 0 元或负金额发票；无历史票款请使用「从 0 起」")
  }
  const item = projectItem(seed)
  if (!item) throw new Error("任务已完成")

  const allocated = input.allocations.reduce(
    (s, a) => s + moneyNum(a.amount),
    0
  )
  if (Math.abs(allocated - moneyNum(input.grossAmount)) > 0.001) {
    throw new Error("分配合计须等于发票含税金额")
  }

  try {
    applyWorkItemActionSession({
      workItemId: input.workItemId,
      claimToken: input.claimToken,
      leaseVersion: input.leaseVersion,
      expectedSubjectHash: item.workItem.subjectHash,
      idempotencyKey: input.idempotencyKey,
      action: { kind: "REGISTER_INVOICE", note: input.invoiceNo },
    })
  } catch (error) {
    if (error instanceof WorkItemMockError) throw new Error(error.message)
    throw error
  }

  const toAccount = input.allocations
    .filter((a) => a.targetAccountId === item.account.id)
    .reduce((s, a) => s + moneyNum(a.amount), 0)

  const newInv = {
    invoiceId: `inv_${input.workItemId}_${Date.now().toString(36)}`,
    invoiceNo: input.invoiceNo,
    direction: "BLUE" as const,
    issuedAt: input.issuedAt,
    grossAmount: moneyStr(moneyNum(input.grossAmount)),
    netAmount: moneyStr(moneyNum(input.netAmount)),
    taxAmount: moneyStr(moneyNum(input.taxAmount)),
    allocatedToAccount: moneyStr(toAccount),
    reversed: false,
  }
  const invoiceFacts = [...item.invoiceFacts, newInv]
  const invoicedTotal = moneyStr(
    invoiceFacts
      .filter((r) => !r.reversed)
      .reduce((s, r) => s + moneyNum(r.allocatedToAccount), 0)
  )
  const settledTotal = item.account.settledTotal
  const openTotal = item.account.openTotal
  const openInvoiceableTotal = moneyStr(
    Math.max(0, moneyNum(item.account.grossTotal) - moneyNum(invoicedTotal))
  )
  const fundsFactVersion = `ffv_${input.workItemId}_${Date.now().toString(36)}`
  const subjectHash = recomputeHash({
    accountId: item.account.id,
    fundsFactVersion,
    settled: settledTotal,
    invoiced: invoicedTotal,
    gross: item.account.grossTotal,
    revisionId: item.currentSalesOrderRevisionId,
  })

  setW13FundsOverlay(input.workItemId, {
    fundsFactVersion,
    subjectHash,
    settledTotal,
    invoicedTotal,
    openTotal,
    openInvoiceableTotal,
    receiptFacts: [...item.receiptFacts],
    invoiceFacts: [...invoiceFacts],
    evidenceDocumentIds: item.currentEvidence.evidenceDocumentIds,
    evidenceReferences: [
      ...item.currentEvidence.evidenceReferences,
      input.evidenceReference,
    ].filter(Boolean),
    comment: item.currentEvidence.comment,
  })
  bumpW13SubjectHash(input.workItemId, subjectHash)
  const ov = getW13FundsOverlay(input.workItemId)
  if (ov) {
    setW13FundsOverlay(input.workItemId, {
      ...ov,
      forceHashDriftOnComplete: false,
      subjectHash,
    })
  }

  return {
    fundsFactVersion,
    subjectHash,
    settledTotal,
    invoicedTotal,
    openTotal,
    openInvoiceableTotal,
    receiptFacts: item.receiptFacts,
    invoiceFacts,
  }
}

export async function saveCardFundsEvidence(input: {
  workItemId: string
  claimToken: string
  leaseVersion: number
  evidenceDocumentIds: string[]
  evidenceReferences: string[]
  comment?: string
  idempotencyKey: string
}): Promise<{ ok: true }> {
  await mockDelay(60)
  const seed = CARD_FUNDS_REVIEW_SEED.find(
    (t) => t.workItem.workItemId === input.workItemId
  )
  if (!seed) throw new Error("任务不存在")
  const item = projectItem(seed)
  if (!item) throw new Error("任务已完成")
  try {
    applyWorkItemActionSession({
      workItemId: input.workItemId,
      claimToken: input.claimToken,
      leaseVersion: input.leaseVersion,
      expectedSubjectHash: item.workItem.subjectHash,
      idempotencyKey: input.idempotencyKey,
      action: { kind: "SAVE_EVIDENCE", note: "保存复核证据" },
    })
  } catch (error) {
    if (error instanceof WorkItemMockError) throw new Error(error.message)
    throw error
  }
  const existing = getW13FundsOverlay(input.workItemId)
  setW13FundsOverlay(input.workItemId, {
    fundsFactVersion: existing?.fundsFactVersion ?? item.fundsFactVersion,
    subjectHash: existing?.subjectHash ?? item.workItem.subjectHash,
    settledTotal: existing?.settledTotal ?? item.account.settledTotal,
    invoicedTotal: existing?.invoicedTotal ?? item.account.invoicedTotal,
    openTotal: existing?.openTotal ?? item.account.openTotal,
    openInvoiceableTotal:
      existing?.openInvoiceableTotal ?? item.account.openInvoiceableTotal,
    receiptFacts: existing?.receiptFacts
      ? [...existing.receiptFacts]
      : [...item.receiptFacts],
    invoiceFacts: existing?.invoiceFacts
      ? [...existing.invoiceFacts]
      : [...item.invoiceFacts],
    evidenceDocumentIds: input.evidenceDocumentIds,
    evidenceReferences: input.evidenceReferences,
    comment: input.comment,
  })
  return { ok: true }
}

export async function resolveUnknownCardFundsResult(input: {
  idempotencyKey: string
  settle?: boolean
  settlePayload?: {
    workItemId: string
    claimToken: string
    leaseVersion: number
    expectedSubjectHash: string
    expectedSubjectVersion: string
    idempotencyKey: string
    decision: CardFundsReviewDecision
  }
}): Promise<FormalActionResponse> {
  await mockDelay(80)
  const entry = queryIdempotencyResult(input.idempotencyKey)
  if (entry?.state === "succeeded") {
    const payload = entry.payload as { formalOutcome?: FormalOutcome }
    if (payload?.formalOutcome) {
      return { status: "succeeded", outcome: payload.formalOutcome }
    }
  }
  if (entry?.state === "pending" && input.settle && input.settlePayload) {
    try {
      finalizePendingComplete({
        idempotencyKey: input.idempotencyKey,
        workItemId: input.settlePayload.workItemId,
        decision: {
          kind:
            input.settlePayload.decision.reviewResult === "APPROVED"
              ? "CARD_FUNDS_APPROVED"
              : "CARD_FUNDS_REJECTED",
        },
        expectedSubjectHash: input.settlePayload.expectedSubjectHash,
      })
      // Re-run complete path to attach business result
      return completeCardFundsReview({
        ...input.settlePayload,
        forceUnknown: false,
      })
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
      message: "结果仍不确定，请稍后查询或联系支持。",
      idempotencyKey: input.idempotencyKey,
    }
  }
  return {
    status: "failed",
    code: "NOT_FOUND",
    message: "未找到该幂等操作",
  }
}

export async function demoDriftCardFundsHash(
  workItemId: string
): Promise<{ subjectHash: string }> {
  await mockDelay(40)
  const next = `sha256:external_drift_${workItemId}_${Date.now()}`
  bumpW13SubjectHash(workItemId, next)
  return { subjectHash: next }
}

/** W11 列表：卡券相关应收是否标记不可靠 */
export function getW11FundsReliabilityFlags(): ReadonlyArray<{
  accountId: string
  customerName: string
  fundsReliability: string
  note: string
  reviewed: boolean
}> {
  return CARD_FUNDS_REVIEW_SEED.map((seed) => {
    const terminal = isW13WorkItemTerminal(seed.workItem.workItemId)
    const outcome = getW13BusinessOutcome(seed.workItem.workItemId)
    if (terminal && outcome?.reviewResult === "APPROVED") {
      return {
        accountId: seed.account.id,
        customerName: seed.account.customerName,
        fundsReliability: "VERIFIED",
        note: "卡券票款复核已通过，指标可作为经营结果",
        reviewed: true,
      }
    }
    return {
      accountId: seed.account.id,
      customerName: seed.account.customerName,
      fundsReliability: seed.account.fundsReliability,
      note: seed.account.reliabilityNote,
      reviewed: false,
    }
  })
}
