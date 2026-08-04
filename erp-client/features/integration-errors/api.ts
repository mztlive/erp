/**
 * W29 session-mock API：queryFn / mutationFn 纯函数。
 * - claimToken 仅出现在领取响应，不进列表/详情 query 视图
 * - REPLAY 永不接受 originalActionIdempotencyKey；服务端锁定原键
 * - QUERY 后才可能开放 REPLAY（NO_RESULT_CONFIRMED + 安全）
 * - 暂挂/跳过保留在队列；RESOLVE/CLOSE/TRANSFER 才终结任务
 * - 直接对账不伪造 CLOSED
 */

import { mockDelay } from "@/lib/mock-delay"
import type {
  ActionOutcome,
  ClaimResult,
  ControlledTerminalEvidenceRef,
  DirectReconciliationInput,
  IntegrationActionKind,
  IntegrationCloseInput,
  IntegrationFormalResult,
  IntegrationQueueView,
  IntegrationResolutionItemView,
  IntegrationResolutionQuery,
  IntegrationResolveInput,
  IntegrationTaskActionInput,
  IntegrationTransferInput,
  QueryOutcome,
} from "@/features/integration-errors/types"
import {
  INTEGRATION_ERROR_SEEDS,
  WORK_ITEM_TO_ITEM,
} from "@/mock/integration-errors"
import {
  applyWorkItemActionSession,
  claimWorkItemSession,
  clearSessionLease,
  closeWorkItemSession,
  completeWorkItemSession,
  getCompletedQueueTaskIds,
  getHeldQueueTaskIds,
  getSessionLeaseState,
  getWorkItemActionHistory,
  getWorkItemTerminal,
  markQueueTaskCompleted,
  markQueueTaskHeld,
  queryIdempotencyResult,
  setIdempotencySucceeded,
  transferWorkItemSession,
  WorkItemMockError,
} from "@/mock/session-state"
import { resultText } from "@/lib/ui-text"

const WS = "W29"

type ItemOverlay = {
  queryStage?: QueryOutcome | null
  linkedEvidence: ControlledTerminalEvidenceRef[]
  statusLabel?: string
  statusCode?: string
  terminalDifference?: "CONFIRMED_NO_ERROR" | "CONFIRMED_VALID_DIFFERENCE"
  auditExtra: IntegrationResolutionItemView["auditTrail"]
  evidenceExtra: IntegrationResolutionItemView["evidenceTimeline"]
  /** After safe query: server allows replay */
  replaySafe?: boolean
  compensated?: boolean
  objectVersion?: string
  subjectHash?: string
  workItemVersion?: string
}

const overlays = new Map<string, ItemOverlay>()

function getOverlay(id: string): ItemOverlay {
  let o = overlays.get(id)
  if (!o) {
    o = {
      linkedEvidence: [],
      auditExtra: [],
      evidenceExtra: [],
    }
    overlays.set(id, o)
  }
  return o
}

function appendAudit(
  id: string,
  entry: IntegrationResolutionItemView["auditTrail"][number]
) {
  const o = getOverlay(id)
  o.auditExtra = [...o.auditExtra, entry]
}

function appendEvidence(
  id: string,
  entry: IntegrationResolutionItemView["evidenceTimeline"][number]
) {
  const o = getOverlay(id)
  o.evidenceExtra = [...o.evidenceExtra, entry]
}

function newOpRef(prefix: string) {
  return `${prefix}-${Date.now().toString(36).toUpperCase()}`
}

function baseAllowed(
  seed: IntegrationResolutionItemView,
  overlay: ItemOverlay
): {
  allowed: IntegrationActionKind[]
  blockers: IntegrationResolutionItemView["actionBlockers"]
} {
  const terminal = seed.workItem
    ? getWorkItemTerminal(seed.workItem.workItemId)
    : null
  if (terminal || overlay.terminalDifference) {
    return { allowed: [], blockers: [] }
  }

  const allowed = new Set<IntegrationActionKind>(seed.allowedActions)
  const blockers = [...seed.actionBlockers]

  // RESULT_UNKNOWN gate: REPLAY only after NO_RESULT_CONFIRMED + safe
  if (seed.classification.errorClass === "result-unknown") {
    allowed.delete("REPLAY_ORIGINAL")
    if (overlay.queryStage === "NO_RESULT_CONFIRMED" && overlay.replaySafe) {
      allowed.add("REPLAY_ORIGINAL")
      // remove QUERY_REQUIRED blocker if present
      const idx = blockers.findIndex((b) => b.action === "REPLAY_ORIGINAL")
      if (idx >= 0) blockers.splice(idx, 1)
    } else if (overlay.queryStage === "TERMINAL_EVIDENCE_FOUND") {
      // still no replay — resolve path
    } else {
      if (!blockers.some((b) => b.action === "REPLAY_ORIGINAL")) {
        blockers.push({
          action: "REPLAY_ORIGINAL",
          code: "QUERY_REQUIRED",
          message:
            "结果未知：须先查询原结果；仅确认无结果且系统判定安全后才可重新提交",
        })
      }
    }
  }

  // Fail-closed auto-retry classes: never expose meaningless REPLAY
  if (
    seed.classification.errorClass === "parameter-or-mapping" ||
    seed.classification.errorClass === "business-rejected" ||
    seed.classification.errorClass === "authentication-or-signature" ||
    seed.classification.errorClass === "capability-unsupported"
  ) {
    allowed.delete("REPLAY_ORIGINAL")
  }

  // Generic close blocked when unknown / funds open / compensation incomplete
  const closeBlocked =
    seed.classification.errorClass === "result-unknown" ||
    (seed.fundsImpact !== "NONE" &&
      seed.classification.errorClass !== "duplicate-callback") ||
    (seed.compensationOpen && !overlay.compensated)

  if (closeBlocked) {
    allowed.delete("CLOSE_DUPLICATE")
    allowed.delete("CLOSE_MISROUTED")
  }

  // RESOLVE requires policy + complete evidence kinds
  const policy = seed.resolutionEvidencePolicy
  if (!policy) {
    allowed.delete("RESOLVE")
    if (
      seed.hasWorkItem &&
      !blockers.some((b) => b.action === "RESOLVE")
    ) {
      blockers.push({
        action: "RESOLVE",
        code: "EVIDENCE_POLICY_MISSING",
        message:
          "解决证据规则尚未配置，只能补证、先跳过或转交",
      })
    }
  } else {
    const linked = [
      ...seed.linkedEvidence,
      ...overlay.linkedEvidence,
    ]
    const kinds = new Set(linked.map((e) => e.kind))
    const missing = policy.requiredEvidenceKinds.filter((k) => !kinds.has(k))
    if (missing.length > 0 || (seed.compensationOpen && !overlay.compensated)) {
      allowed.delete("RESOLVE")
    } else if (seed.hasWorkItem) {
      allowed.add("RESOLVE")
      const bi = blockers.findIndex((b) => b.action === "RESOLVE")
      if (bi >= 0) blockers.splice(bi, 1)
    }
  }

  // Direct recon terminal requires registry
  if (!seed.hasWorkItem) {
    allowed.delete("RESOLVE")
    allowed.delete("CLOSE_DUPLICATE")
    allowed.delete("CLOSE_MISROUTED")
    const reg = seed.reconciliationReasonRegistry
    if (!reg || reg.registeredReasons.length === 0) {
      allowed.delete("CONFIRM_NO_ERROR")
      allowed.delete("CONFIRM_VALID_DIFFERENCE")
      blockers.push({
        action: "CONFIRM_NO_ERROR",
        code: "REASON_REGISTRY_MISSING",
        message: "对账原因注册表未配置，终结动作已阻断",
      })
    }
  }

  return { allowed: [...allowed], blockers }
}

function projectItem(
  seed: IntegrationResolutionItemView
): IntegrationResolutionItemView | null {
  const id = seed.identity.id
  const completed = getCompletedQueueTaskIds(WS)
  const held = getHeldQueueTaskIds(WS)
  const overlay = getOverlay(id)

  if (overlay.terminalDifference) {
    // Direct recon terminal — hide from open queues (still available in resolved)
    return null
  }

  if (seed.workItem) {
    const wi = seed.workItem.workItemId
    if (completed.has(wi) || completed.has(id)) {
      return null
    }
    const terminal = getWorkItemTerminal(wi)
    if (terminal) return null
  }

  const { allowed, blockers } = baseAllowed(seed, overlay)
  const lease = seed.workItem
    ? getSessionLeaseState(seed.workItem.workItemId)
    : null
  const actionHistory = seed.workItem
    ? getWorkItemActionHistory(seed.workItem.workItemId)
    : []

  const historyEntries = actionHistory.map((h) => ({
    id: h.actionRecordId,
    at: h.recordedAt,
    actor: "当前用户",
    action: h.actionKind,
    detail:
      h.evidenceNote ??
      `任务状态 ${
        h.workItemStatus === "IN_PROGRESS" ? "处理中" : "待处理"
      }`,
  }))

  const isHeld =
    (seed.workItem && held.has(seed.workItem.workItemId)) || held.has(id)

  let workItem = seed.workItem
  if (workItem) {
    workItem = {
      ...workItem,
      workItemVersion: overlay.workItemVersion ?? workItem.workItemVersion,
      subjectHash: overlay.subjectHash ?? workItem.subjectHash,
      status: isHeld ? "PENDING" : lease ? "IN_PROGRESS" : workItem.status,
      lease: lease
        ? {
            leaseVersion: lease.leaseVersion,
            leaseExpiresAt: lease.leaseExpiresAt,
            ownerDisplayName: "王敏（演示）",
          }
        : undefined,
    }
  }

  return {
    ...seed,
    objectVersion: overlay.objectVersion ?? seed.objectVersion,
    identity: {
      ...seed.identity,
      subjectHash: overlay.subjectHash ?? seed.identity.subjectHash,
    },
    workItem,
    status: {
      code: overlay.statusCode ?? (isHeld ? "HELD" : seed.status.code),
      label: overlay.statusLabel ?? (isHeld ? "已跳过" : seed.status.label),
    },
    queryStage: overlay.queryStage ?? seed.queryStage,
    linkedEvidence: [...seed.linkedEvidence, ...overlay.linkedEvidence],
    allowedActions: allowed,
    actionBlockers: blockers,
    auditTrail: [...seed.auditTrail, ...overlay.auditExtra, ...historyEntries],
    evidenceTimeline: [...seed.evidenceTimeline, ...overlay.evidenceExtra],
    compensationOpen: seed.compensationOpen && !overlay.compensated,
  }
}

function matchesQuery(
  item: IntegrationResolutionItemView,
  q: IntegrationResolutionQuery
): boolean {
  if (q.mode === "errors" && item.identity.itemType !== "ERROR_TASK") {
    return false
  }
  if (
    q.mode === "reconciliation" &&
    item.identity.itemType !== "RECONCILIATION_DIFFERENCE"
  ) {
    return false
  }
  if (q.environment !== "all" && item.environment !== q.environment) {
    return false
  }
  if (q.errorClass && item.classification.errorClass !== q.errorClass) {
    return false
  }
  if (q.view === "result_unknown") {
    if (item.classification.errorClass !== "result-unknown") return false
  }
  if (q.view === "security") {
    if (item.classification.errorClass !== "authentication-or-signature") {
      return false
    }
  }
  if (q.view === "reconciliation") {
    if (item.identity.itemType !== "RECONCILIATION_DIFFERENCE") return false
  }
  if (q.view === "auto_retry") {
    if (
      item.classification.errorClass !== "network-timeout" &&
      item.classification.errorClass !== "rate-limited"
    ) {
      return false
    }
  }
  if (q.view === "resolved") {
    return false // open queue projection excludes resolved
  }
  if (q.view === "mine") {
    // demo: show all open as "mine-capable"
  }
  if (q.q) {
    const needle = q.q.toLowerCase()
    const hay = [
      item.identity.number,
      item.identity.id,
      item.businessObject.title,
      item.businessObject.objectId,
      item.workItem?.workItemId ?? "",
      item.classification.label,
    ]
      .join(" ")
      .toLowerCase()
    if (!hay.includes(needle)) return false
  }
  return true
}

function metricsFrom(items: IntegrationResolutionItemView[]) {
  return {
    resultUnknown: items.filter(
      (i) => i.classification.errorClass === "result-unknown"
    ).length,
    manualRequired: items.filter((i) =>
      i.status.label.includes("人工")
    ).length,
    securityFaults: items.filter(
      (i) => i.classification.errorClass === "authentication-or-signature"
    ).length,
    openDifferences: items.filter(
      (i) => i.identity.itemType === "RECONCILIATION_DIFFERENCE"
    ).length,
    longestAgeLabel:
      items.find((i) => i.ageLabel.includes("2d"))?.ageLabel ??
      items[0]?.ageLabel ??
      "—",
  }
}

export async function fetchIntegrationQueue(
  query: IntegrationResolutionQuery
): Promise<IntegrationQueueView> {
  await mockDelay()

  let resolvedEntry: IntegrationQueueView["resolvedEntry"]
  if (query.resolveWorkItemId) {
    const mapped = WORK_ITEM_TO_ITEM[query.resolveWorkItemId]
    if (mapped) {
      resolvedEntry = {
        itemType: mapped.itemType,
        id: mapped.id,
        workItemId: query.resolveWorkItemId,
      }
    }
  }

  const projected = INTEGRATION_ERROR_SEEDS.map(projectItem).filter(
    (x): x is IntegrationResolutionItemView => x != null
  )
  const items = projected.filter((i) => matchesQuery(i, query))

  // Stable sort: security + result-unknown first, then age
  items.sort((a, b) => {
    const rank = (i: IntegrationResolutionItemView) => {
      if (i.classification.errorClass === "authentication-or-signature") return 0
      if (i.classification.errorClass === "result-unknown") return 1
      if (i.classification.severity === "critical") return 2
      if (i.classification.severity === "high") return 3
      return 4
    }
    return rank(a) - rank(b)
  })

  const filterParts = [
    `视图=${query.view}`,
    `模式=${query.mode}`,
    `环境=${query.environment}`,
  ]
  if (query.errorClass) filterParts.push(`类别=${query.errorClass}`)
  if (query.q) filterParts.push(`搜索=${query.q}`)

  return {
    items,
    metrics: metricsFrom(projected),
    context: {
      queueContextId: query.queueContextId ?? `queue:W29:${query.view}`,
      filterSummary: filterParts.join(" · "),
      updatedAt: new Date().toISOString(),
    },
    resolvedEntry,
  }
}

export async function fetchIntegrationItem(input: {
  itemType: "ERROR_TASK" | "RECONCILIATION_DIFFERENCE"
  id: string
}): Promise<IntegrationResolutionItemView | null> {
  await mockDelay()
  const seed = INTEGRATION_ERROR_SEEDS.find(
    (s) => s.identity.itemType === input.itemType && s.identity.id === input.id
  )
  if (!seed) return null
  // Detail can still show terminal overlay items
  const completed = getCompletedQueueTaskIds(WS)
  const overlay = getOverlay(seed.identity.id)
  if (
    (seed.workItem && completed.has(seed.workItem.workItemId)) ||
    overlay.terminalDifference
  ) {
    // Return projected terminal snapshot for read-only
    const base = projectItem(seed)
    if (base) return base
    return {
      ...seed,
      allowedActions: [],
      actionBlockers: [],
      status: {
        code: overlay.terminalDifference ?? "COMPLETED",
        label: overlay.terminalDifference
          ? overlay.terminalDifference === "CONFIRMED_NO_ERROR"
            ? "确认无误"
            : "确认有效差异"
          : "已解决",
      },
      linkedEvidence: [...seed.linkedEvidence, ...overlay.linkedEvidence],
      auditTrail: [...seed.auditTrail, ...overlay.auditExtra],
      evidenceTimeline: [...seed.evidenceTimeline, ...overlay.evidenceExtra],
    }
  }
  return projectItem(seed)
}

export async function claimIntegrationTask(input: {
  workItemId: string
  subjectVersion: string
  subjectHash: string
  leaseVersion?: number
}): Promise<ClaimResult> {
  await mockDelay(40)
  const lease = claimWorkItemSession({
    workItemId: input.workItemId,
    subjectVersion: input.subjectVersion,
    subjectHash: input.subjectHash,
    leaseVersion: input.leaseVersion ?? 1,
  })
  return {
    workItemId: lease.workItemId,
    claimToken: lease.claimToken,
    leaseVersion: lease.leaseVersion,
    expiresAt: lease.leaseExpiresAt,
  }
}

function requireItem(itemId: string): IntegrationResolutionItemView {
  const seed = INTEGRATION_ERROR_SEEDS.find((s) => s.identity.id === itemId)
  if (!seed) {
    throw new WorkItemMockError("NOT_FOUND", "未找到错误任务或差异")
  }
  return seed
}

export async function applyIntegrationTaskAction(
  input: IntegrationTaskActionInput
): Promise<IntegrationFormalResult> {
  await mockDelay(60)
  const seed = requireItem(input.itemId)
  const projected = projectItem(seed)
  if (!projected) {
    return {
      status: "blocked",
      title: "任务已不在开放队列",
      description: "当前项已终结或不在可处理范围。",
      stayOnItem: true,
    }
  }

  if (!projected.allowedActions.includes(input.kind)) {
    const blocker = projected.actionBlockers.find((b) => b.action === input.kind)
    return {
      status: "blocked",
      title: "动作不可用",
      description: blocker?.message ?? "该动作不在当前可操作范围内",
      stayOnItem: true,
    }
  }

  // Never accept client originalActionIdempotencyKey — enforce by type absence
  const unsafe = input as IntegrationTaskActionInput & {
    originalActionIdempotencyKey?: string
  }
  if (unsafe.originalActionIdempotencyKey) {
    return {
      status: "rejected",
      title: "禁止自行指定原任务号",
      description:
        "重新提交必须由系统沿用已锁定的原任务号，不允许手动指定或替换。",
      stayOnItem: true,
    }
  }

  if (input.kind === "REPLAY_ORIGINAL") {
    const o = getOverlay(input.itemId)
    if (o.queryStage !== "NO_RESULT_CONFIRMED" || !o.replaySafe) {
      return {
        status: "blocked",
        title: "暂不可重新提交",
        description:
          "仅在查询确认无结果且系统判定安全后才可重新提交；本次未放行。",
        stayOnItem: true,
      }
    }
  }

  try {
    if (input.simulateTimeout) {
      applyWorkItemActionSession({
        workItemId: input.workItemId,
        claimToken: input.claimToken,
        leaseVersion: input.leaseVersion,
        expectedSubjectHash: input.expectedSubjectHash,
        idempotencyKey: input.idempotencyKey,
        action: { kind: input.kind, note: input.comment },
        simulateTimeout: true,
      })
    }

    if (input.kind === "QUERY_ORIGINAL_RESULT") {
      const o = getOverlay(input.itemId)
      let outcome: QueryOutcome = "NO_RESULT_CONFIRMED"
      if (input.forceUnknown) {
        outcome = "RESULT_UNKNOWN"
        o.queryStage = "RESULT_UNKNOWN"
        o.replaySafe = false
      } else {
        // Demo default: confirm no result so REPLAY can unlock
        o.queryStage = "NO_RESULT_CONFIRMED"
        o.replaySafe = true
        outcome = "NO_RESULT_CONFIRMED"
      }
      appendEvidence(input.itemId, {
        id: newOpRef("QEV"),
        at: new Date().toISOString(),
        actor: "当前用户",
        action: "查询原结果",
        detail:
          outcome === "RESULT_UNKNOWN"
            ? "仍未知 · 任务保持待处理"
            : "确认无结果 · 系统判定可安全重发",
      })
      appendAudit(input.itemId, {
        id: newOpRef("QAU"),
        at: new Date().toISOString(),
        actor: "当前用户",
        action: "QUERY_ORIGINAL_RESULT",
        detail: outcome,
      })

      applyWorkItemActionSession({
        workItemId: input.workItemId,
        claimToken: input.claimToken,
        leaseVersion: input.leaseVersion,
        expectedSubjectHash: input.expectedSubjectHash,
        idempotencyKey: input.idempotencyKey,
        action: {
          kind: "QUERY_ORIGINAL_RESULT",
          note: `outcome=${outcome}`,
        },
      })

      const next = projectItem(seed)
      return {
        status: outcome === "RESULT_UNKNOWN" ? "unknown" : "succeeded",
        title:
          outcome === "RESULT_UNKNOWN"
            ? "查询后仍结果未知"
            : "查询原结果：明确无结果",
        description:
          outcome === "RESULT_UNKNOWN"
            ? "不得按成功处理，不得自动下一项；可再次查询或转交。任务仍在处理中。"
            : "已确认无结果且系统判定安全；已按原任务号开放重新提交。任务仍在处理中。",
        reference: input.operationId,
        outcome,
        nextAllowedActions: next?.allowedActions,
        workItemStatus: "IN_PROGRESS",
        stayOnItem: true,
        terminal: false,
        facts: [
          {
            label: "查询结果",
            value:
              outcome === "RESULT_UNKNOWN" ? "仍未知" : "确认无结果",
          },
          {
            label: resultText.originalTaskNo,
            value:
              seed.originalAction?.originalActionIdempotencyKeySummary ?? "—",
          },
          { label: "按原任务号重发", value: "是（系统）" },
          { label: "任务状态", value: "处理中" },
        ],
      }
    }

    if (input.kind === "REPLAY_ORIGINAL") {
      applyWorkItemActionSession({
        workItemId: input.workItemId,
        claimToken: input.claimToken,
        leaseVersion: input.leaseVersion,
        expectedSubjectHash: input.expectedSubjectHash,
        idempotencyKey: input.idempotencyKey,
        action: {
          kind: "REPLAY_ORIGINAL",
          note: "server used locked originalActionIdempotencyKey",
        },
      })
      appendEvidence(input.itemId, {
        id: newOpRef("REV"),
        at: new Date().toISOString(),
        actor: "当前用户",
        action: "按原任务重新提交",
        detail: `系统沿用 ${seed.originalAction?.originalActionIdempotencyKeySummary ?? "锁定原任务号"} · 未手动指定`,
      })
      appendAudit(input.itemId, {
        id: newOpRef("RAU"),
        at: new Date().toISOString(),
        actor: "当前用户",
        action: "REPLAY_ORIGINAL",
        detail: "已受理重新提交 · 任务尚未完成",
      })
      return {
        status: "succeeded",
        title: "重新提交已受理",
        description:
          "系统已按原任务号重新提交。任务仍在处理中，需处理完成后才能关闭。",
        reference: input.operationId,
        outcome: "REPLAY_ACCEPTED",
        workItemStatus: "IN_PROGRESS",
        stayOnItem: true,
        terminal: false,
        facts: [
          {
            label: "原任务号",
            value:
              seed.originalAction?.originalActionIdempotencyKeySummary ?? "—",
          },
          { label: "手动指定原任务号", value: "否" },
          { label: "任务状态", value: "处理中" },
        ],
      }
    }

    if (input.kind === "ADD_EVIDENCE" || input.kind === "LINK_COMPENSATION") {
      const o = getOverlay(input.itemId)
      const refs =
        input.evidenceRefs ??
        ([
          {
            kind:
              input.kind === "LINK_COMPENSATION"
                ? "COMPENSATION_RESULT"
                : "BUSINESS_OBJECT_VERIFICATION",
            recordId: newOpRef("EVD"),
            label:
              input.kind === "LINK_COMPENSATION"
                ? "关联补偿结果"
                : "补充业务核验",
          },
        ] satisfies ControlledTerminalEvidenceRef[])
      o.linkedEvidence = [...o.linkedEvidence, ...refs]
      if (input.kind === "LINK_COMPENSATION") {
        o.compensated = true
      }
      appendEvidence(input.itemId, {
        id: newOpRef("AEV"),
        at: new Date().toISOString(),
        actor: "当前用户",
        action: input.kind === "LINK_COMPENSATION" ? "关联补偿" : "补充证据",
        detail: refs.map((r) => `${r.kind}:${r.recordId}`).join(", "),
      })
      applyWorkItemActionSession({
        workItemId: input.workItemId,
        claimToken: input.claimToken,
        leaseVersion: input.leaseVersion,
        expectedSubjectHash: input.expectedSubjectHash,
        idempotencyKey: input.idempotencyKey,
        action: {
          kind: input.kind,
          note: input.comment,
        },
      })
      const next = projectItem(seed)
      return {
        status: "succeeded",
        title:
          input.kind === "LINK_COMPENSATION" ? "已关联补偿证据" : "已追加证据",
        description:
          "证据为追加式，不可覆盖历史。任务仍在待处理列表。",
        reference: input.operationId,
        outcome:
          input.kind === "LINK_COMPENSATION"
            ? "EVIDENCE_LINKED"
            : "EVIDENCE_ADDED",
        workItemStatus: "IN_PROGRESS",
        stayOnItem: true,
        terminal: false,
        nextAllowedActions: next?.allowedActions,
        facts: refs.map((r) => ({
          label: r.kind,
          value: r.recordId,
        })),
      }
    }

    if (input.kind === "REATTRIBUTE") {
      applyWorkItemActionSession({
        workItemId: input.workItemId,
        claimToken: input.claimToken,
        leaseVersion: input.leaseVersion,
        expectedSubjectHash: input.expectedSubjectHash,
        idempotencyKey: input.idempotencyKey,
        action: { kind: "REATTRIBUTE", note: input.comment },
      })
      appendEvidence(input.itemId, {
        id: newOpRef("REAT"),
        at: new Date().toISOString(),
        actor: "当前用户",
        action: "重新归集",
        detail: "复用原业务记录键 · 未复制消费记录",
      })
      return {
        status: "succeeded",
        title: "已发起重新归集",
        description:
          "使用原业务记录键重新归集；任务仍在待处理列表。",
        reference: input.operationId,
        outcome: "REATTRIBUTED",
        workItemStatus: "IN_PROGRESS",
        stayOnItem: true,
        terminal: false,
      }
    }

    if (input.kind === "DEFER" || input.kind === "SKIP") {
      applyWorkItemActionSession({
        workItemId: input.workItemId,
        claimToken: input.claimToken,
        leaseVersion: input.leaseVersion,
        expectedSubjectHash: input.expectedSubjectHash,
        idempotencyKey: input.idempotencyKey,
        action: {
          kind: input.kind,
          note: input.comment ?? input.reasonCode,
        },
      })
      if (input.kind === "DEFER") {
        markQueueTaskHeld(WS, input.workItemId)
        markQueueTaskHeld(WS, input.itemId)
        clearSessionLease(input.workItemId)
      }
      // SKIP: stay in queue, may advance focus (caller decides)
      return {
        status: "succeeded",
        title: input.kind === "DEFER" ? "已跳过" : "已跳过当前项",
        description:
          input.kind === "DEFER"
            ? "任务仍在待处理队列，未完成。本次处理已结束；可稍后继续。"
            : "已记录跳过；任务仍在待处理列表，未终结。",
        reference: input.operationId,
        outcome: input.kind === "DEFER" ? "DEFERRED" : "SKIPPED",
        workItemStatus: "PENDING",
        stayOnItem: input.kind === "DEFER",
        terminal: false,
        facts: [
          { label: "仍在队列", value: "是" },
          { label: "任务状态", value: "待处理" },
        ],
      }
    }

    return {
      status: "blocked",
      title: "未实现的动作",
      description: input.kind,
      stayOnItem: true,
    }
  } catch (e) {
    if (e instanceof WorkItemMockError && e.code === "TIMEOUT") {
      return {
        status: "unknown",
        title: "动作结果未知",
        description: e.message,
        stayOnItem: true,
        pendingIdempotencyKey: input.idempotencyKey,
        terminal: false,
      }
    }
    throw e
  }
}

export async function resolveIntegrationTask(
  input: IntegrationResolveInput
): Promise<IntegrationFormalResult> {
  await mockDelay(60)
  const seed = requireItem(input.itemId)
  const projected = projectItem(seed)
  if (!projected?.allowedActions.includes("RESOLVE")) {
    const blocker = projected?.actionBlockers.find((b) => b.action === "RESOLVE")
    return {
      status: "blocked",
      title: "不能标记已解决",
      description:
        blocker?.message ??
        "证据策略未命中或证据不全；只能补证、先跳过或转交。",
      stayOnItem: true,
    }
  }
  const policy = seed.resolutionEvidencePolicy
  if (
    !policy ||
    policy.evidencePolicyId !== input.evidencePolicyId ||
    policy.evidencePolicyVersion !== input.evidencePolicyVersion
  ) {
    return {
      status: "blocked",
      title: "证据策略不匹配",
      description: "必须引用当前命中的证据策略及版本。",
      stayOnItem: true,
    }
  }
  if (!input.evidenceRefs.length) {
    return {
      status: "blocked",
      title: "证据不能为空",
      description: "处理完成要求提供非空的处理证据。",
      stayOnItem: true,
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
        kind: "RESOLVE",
        note: input.comment,
        summary: `policy=${input.evidencePolicyId}@${input.evidencePolicyVersion}`,
      },
      simulateTimeout: input.simulateTimeout,
    })
    markQueueTaskCompleted(WS, input.workItemId)
    markQueueTaskCompleted(WS, input.itemId)
    appendAudit(input.itemId, {
      id: newOpRef("RES"),
      at: new Date().toISOString(),
      actor: "当前用户",
      action: "RESOLVE",
      detail: result.completionRecordId,
    })
    return {
      status: "succeeded",
      title: "已标记解决",
      description:
        "处理已完成，可进入下一项。",
      reference: result.completionRecordId,
      outcome: "RESOLVED",
      workItemStatus: "COMPLETED",
      stayOnItem: false,
      terminal: true,
      facts: [
        { label: "证据策略", value: `版本 v${input.evidencePolicyVersion}` },
        { label: "证据数", value: String(input.evidenceRefs.length) },
        { label: "任务状态", value: "已完成" },
      ],
    }
  } catch (e) {
    if (e instanceof WorkItemMockError && e.code === "TIMEOUT") {
      return {
        status: "unknown",
        title: "解决结果未知",
        description: e.message,
        stayOnItem: true,
        pendingIdempotencyKey: input.idempotencyKey,
      }
    }
    throw e
  }
}

export async function closeIntegrationTask(
  input: IntegrationCloseInput
): Promise<IntegrationFormalResult> {
  await mockDelay(60)
  const seed = requireItem(input.itemId)
  const projected = projectItem(seed)
  if (!projected?.allowedActions.includes(input.kind)) {
    return {
      status: "blocked",
      title: "不能关闭",
      description:
        "结果未知、资金未闭环或补偿未完成时禁止通用关闭；关闭须替代任务/受控证据。",
      stayOnItem: true,
    }
  }
  if (input.kind === "CLOSE_DUPLICATE" && !input.replacementWorkItemId) {
    return {
      status: "blocked",
      title: "重复关闭须引用替代任务",
      description: "CLOSE_DUPLICATE 强制 replacementWorkItemId。",
      stayOnItem: true,
    }
  }
  if (!input.closureEvidenceReference.trim()) {
    return {
      status: "blocked",
      title: "关闭证据必填",
      description: "须提供受控关闭证据引用。",
      stayOnItem: true,
    }
  }

  const result = closeWorkItemSession({
    workItemId: input.workItemId,
    claimToken: input.claimToken,
    leaseVersion: input.leaseVersion,
    expectedSubjectHash: input.expectedSubjectHash,
    idempotencyKey: input.idempotencyKey,
    closeAllowed: true,
    closure: {
      kind: input.kind,
      reasonCode: input.reasonCode,
      replacementWorkItemId: input.replacementWorkItemId,
      closureEvidenceReference: input.closureEvidenceReference,
      comment: input.comment,
    },
  })
  markQueueTaskCompleted(WS, input.workItemId)
  markQueueTaskCompleted(WS, input.itemId)
  return {
    status: "succeeded",
    title: input.kind === "CLOSE_DUPLICATE" ? "已关闭重复任务" : "已关闭误派",
    description:
      "仅关闭任务本身；不写业务解决结论，不影响业务记录。",
    reference: result.closureRecordId,
    outcome:
      input.kind === "CLOSE_DUPLICATE" ? "CLOSED_DUPLICATE" : "CLOSED_MISROUTED",
    workItemStatus: "CLOSED",
    stayOnItem: false,
    terminal: true,
    replacementWorkItemId: input.replacementWorkItemId,
    facts: [
      {
        label: "原因",
        value: input.reasonCode === "DUPLICATE" ? "重复" : "错误路由",
      },
      { label: "关闭证据", value: input.closureEvidenceReference },
      {
        label: "替代任务",
        value: input.replacementWorkItemId ? "已指定" : "—",
      },
      { label: "任务状态", value: "已关闭" },
    ],
  }
}

export async function transferIntegrationTask(
  input: IntegrationTransferInput
): Promise<IntegrationFormalResult> {
  await mockDelay(60)
  const seed = requireItem(input.itemId)
  const projected = projectItem(seed)
  if (!projected?.allowedActions.includes("TRANSFER")) {
    return {
      status: "blocked",
      title: "不能转交",
      description: "当前项不允许 TRANSFER。",
      stayOnItem: true,
    }
  }
  const result = transferWorkItemSession({
    workItemId: input.workItemId,
    claimToken: input.claimToken,
    leaseVersion: input.leaseVersion,
    expectedSubjectHash: input.expectedSubjectHash,
    idempotencyKey: input.idempotencyKey,
    transfer: {
      toUserLabel: input.targetRole,
      reason: input.comment ?? input.reasonCode,
    },
  })
  markQueueTaskCompleted(WS, input.workItemId)
  markQueueTaskCompleted(WS, input.itemId)
  return {
    status: "succeeded",
    title: "已转交",
    description:
      "原任务已转交，已创建待处理的后继任务。转交不是解决。",
    reference: result.transferRecordId,
    outcome: "TRANSFERRED",
    workItemStatus: "TRANSFERRED",
    stayOnItem: false,
    terminal: true,
    successorWorkItemId: result.successorWorkItemId,
    facts: [
      { label: "目标角色", value: input.targetRole },
      { label: "后继任务", value: "已创建" },
      { label: "原任务状态", value: "已转交" },
    ],
  }
}

export async function applyDirectReconciliation(
  input: DirectReconciliationInput
): Promise<IntegrationFormalResult> {
  await mockDelay(60)
  const seed = requireItem(input.differenceId)
  if (seed.hasWorkItem || seed.workItem) {
    return {
      status: "rejected",
      title: "存在关联任务",
      description:
        "有任务的差异须走任务流程；直接对账不得完成、转交或关闭任务。",
      stayOnItem: true,
    }
  }
  const projected = projectItem(seed)
  if (!projected) {
    return {
      status: "blocked",
      title: "差异已终结",
      description: "当前差异不在可处理范围。",
      stayOnItem: true,
    }
  }

  if (input.decision.kind === "NON_TERMINAL_ACTION") {
    const o = getOverlay(input.differenceId)
    if (input.decision.evidenceRefs?.length) {
      o.linkedEvidence = [
        ...o.linkedEvidence,
        ...input.decision.evidenceRefs,
      ]
    }
    appendEvidence(input.differenceId, {
      id: newOpRef("DNE"),
      at: new Date().toISOString(),
      actor: "当前用户",
      action: input.decision.action,
      detail: input.decision.comment ?? "非终结补证",
    })
    return {
      status: "succeeded",
      title: "已追加差异处理记录",
      description:
        "差异处理尚未完成；未完成或关闭任何任务。",
      reference: input.operationId,
      outcome: "EVIDENCE_ADDED",
      stayOnItem: true,
      terminal: false,
    }
  }

  const terminalDecision = input.decision
  // Terminal conclusion
  const reg = seed.reconciliationReasonRegistry
  if (!reg) {
    return {
      status: "blocked",
      title: "原因注册表未配置",
      description: "确认无误/有效差异均已阻断，不得使用自由文本原因。",
      stayOnItem: true,
    }
  }
  if (
    reg.reasonRegistryId !== terminalDecision.reasonRegistryId ||
    reg.reasonRegistryVersion !== terminalDecision.reasonRegistryVersion
  ) {
    return {
      status: "blocked",
      title: "注册表版本不匹配",
      description: "须引用当前有效原因注册表版本。",
      stayOnItem: true,
    }
  }
  const reason = reg.registeredReasons.find(
    (r) =>
      r.registeredReasonId === terminalDecision.registeredReasonId &&
      r.conclusion === terminalDecision.conclusion
  )
  if (!reason) {
    return {
      status: "blocked",
      title: "原因未注册",
      description: "终结结论必须命中注册表中的强类型原因。",
      stayOnItem: true,
    }
  }
  if (!terminalDecision.evidenceRefs.length) {
    return {
      status: "blocked",
      title: "受控证据不能为空",
      description: "终结对账强制非空受控证据。",
      stayOnItem: true,
    }
  }
  const o = getOverlay(input.differenceId)
  o.linkedEvidence = [...o.linkedEvidence, ...terminalDecision.evidenceRefs]
  const terminalOutcome =
    terminalDecision.conclusion === "CONFIRM_NO_ERROR"
      ? ("CONFIRMED_NO_ERROR" as const)
      : ("CONFIRMED_VALID_DIFFERENCE" as const)
  o.terminalDifference = terminalOutcome
  o.statusCode = terminalOutcome
  o.statusLabel =
    terminalDecision.conclusion === "CONFIRM_NO_ERROR"
      ? "确认无误"
      : "确认有效差异"
  appendAudit(input.differenceId, {
    id: newOpRef("DRT"),
    at: new Date().toISOString(),
    actor: "当前用户",
    action: terminalDecision.conclusion,
    detail: "按注册原因确认 · 任务未关闭",
  })
  setIdempotencySucceeded(input.idempotencyKey, terminalDecision.conclusion, {
    differenceId: input.differenceId,
    isTerminal: true,
  })

  return {
    status: "succeeded",
    title:
      terminalDecision.conclusion === "CONFIRM_NO_ERROR"
        ? "已确认无误"
        : "已确认有效差异",
    description:
      "仅追加对账处理记录；不会伪造任务完成状态。",
    reference: input.operationId,
    outcome: terminalOutcome,
    stayOnItem: false,
    terminal: true,
    facts: [
      { label: "注册原因", value: reason.label },
      {
        label: "差异结果",
        value:
          terminalOutcome === "CONFIRMED_NO_ERROR"
            ? "确认无误"
            : "确认有效差异",
      },
      { label: "任务已关闭", value: "否（直接对账）" },
    ],
  }
}

export async function queryIntegrationIdempotency(
  key: string
): Promise<IntegrationFormalResult> {
  await mockDelay(30)
  try {
    const payload = queryIdempotencyResult(key)
    if (!payload) {
      return {
        status: "unknown",
        title: "仍无最终结果",
        description: "请稍后用原任务号再查，勿自动下一项。",
        stayOnItem: true,
        pendingIdempotencyKey: key,
      }
    }
    return {
      status: "succeeded",
      title: resultText.querySucceeded,
      description: "请根据结果继续处理；非终结动作不自动下一项。",
      reference: key,
      stayOnItem: true,
      facts: [{ label: resultText.originalTaskNo, value: key }],
    }
  } catch (e) {
    return {
      status: "unknown",
      title: "查询失败",
      description: e instanceof Error ? e.message : "未知错误",
      stayOnItem: true,
      pendingIdempotencyKey: key,
    }
  }
}

export { WorkItemMockError }
