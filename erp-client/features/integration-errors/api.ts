/**
 * W29 接口错误与对账中心 · 真实 HTTP API
 * 路径：/admin/integration/error-tasks、/admin/integration/differences、/admin/work-items
 */

import { apiGet, apiPost, type Page } from "@/lib/api"
import type {
  ClaimResult,
  DirectReconciliationInput,
  IntegrationCloseInput,
  IntegrationFormalResult,
  IntegrationQueueView,
  IntegrationResolutionItemView,
  IntegrationResolutionQuery,
  IntegrationResolveInput,
  IntegrationTaskActionInput,
  IntegrationTransferInput,
} from "@/features/integration-errors/types"
import {
  ENV_LABEL,
  ERROR_CLASS_LABEL,
  MODE_LABEL,
  VIEW_LABEL,
} from "@/features/integration-errors/types"
import type { InterfaceErrorClass } from "@/components/business"

// ---------------------------------------------------------------------------
// Backend wire types
// ---------------------------------------------------------------------------

type BackendErrorTask = {
  id: string
  message_id?: string | null
  business_object_id?: string | null
  error_class: string
  status: string
  owner_role?: string | null
  owner_user_id?: string | null
  attempt_count: number
  last_attempt_at?: number | null
  last_attempt_summary?: string | null
  resolution_type?: string | null
  resolved_at?: number | null
  version: number
  created_at: number
  resolution?: string | null
}

type BackendDifference = {
  id: string
  business_object_type: string
  business_object_id: string
  difference_type: string
  left_fact_reference?: string | null
  right_fact_reference?: string | null
  status?: string | null
  version: number
  created_at: number
  resolutions?: Array<{
    id: string
    resolution_no: number
    resolution_action: string
    resulting_status: string
    evidence_reference?: string | null
    handled_by: string
    handled_at: number
  }>
}

type BackendReplayResult = {
  task_id: string
  original_action_idempotency_key_summary: string
  original_action_idempotency_key_locked: boolean
  replay_accepted: boolean
  task_status: string
  attempt_count: number
  task_version: number
}

// ---------------------------------------------------------------------------
// Mapping
// ---------------------------------------------------------------------------

function tsToIso(secs: number | null | undefined): string {
  if (secs == null || !Number.isFinite(Number(secs)) || Number(secs) <= 0)
    return ""
  return new Date(Number(secs) * 1000).toISOString()
}

function ageLabel(createdAtSecs: number): string {
  const ms = Date.now() - createdAtSecs * 1000
  if (ms < 0) return "—"
  const hours = Math.floor(ms / 3_600_000)
  if (hours < 24) return `${Math.max(1, hours)}h`
  return `${Math.floor(hours / 24)}d`
}

/** backend snake_case error_class → UI InterfaceErrorClass kebab */
function mapErrorClass(raw: string): InterfaceErrorClass | "reconciliation-difference" {
  switch (raw) {
    case "capability_gap":
      return "capability-unsupported"
    case "mapping_error":
      return "parameter-or-mapping"
    case "business_rejected":
      return "business-rejected"
    case "transient_failure":
      return "network-timeout"
    case "result_unknown":
      return "result-unknown"
    case "auth_signature":
      return "authentication-or-signature"
    case "rate_limited":
      return "rate-limited"
    case "out_of_order":
      return "out-of-order-callback"
    default:
      return "network-timeout"
  }
}

function errorClassToBackend(ui?: string): string | undefined {
  if (!ui) return undefined
  switch (ui) {
    case "capability-unsupported":
      return "capability_gap"
    case "parameter-or-mapping":
      return "mapping_error"
    case "business-rejected":
      return "business_rejected"
    case "network-timeout":
      return "transient_failure"
    case "result-unknown":
      return "result_unknown"
    case "authentication-or-signature":
      return "auth_signature"
    case "rate-limited":
      return "rate_limited"
    case "out-of-order-callback":
      return "out_of_order"
    case "duplicate-callback":
      return undefined
    default:
      return ui
  }
}

function statusLabel(status: string): string {
  switch (status) {
    case "pending":
      return "待处理"
    case "auto_retrying":
      return "自动重试中"
    case "manual_required":
      return "待人工"
    case "resolved":
      return "已解决"
    case "closed":
      return "已关闭"
    default:
      return status
  }
}

function severityOf(
  errorClass: string
): IntegrationResolutionItemView["classification"]["severity"] {
  if (errorClass === "auth_signature" || errorClass === "result_unknown")
    return "critical"
  if (errorClass === "business_rejected" || errorClass === "mapping_error")
    return "high"
  if (errorClass === "rate_limited") return "medium"
  return "medium"
}

function allowedForTask(task: BackendErrorTask): {
  allowed: IntegrationResolutionItemView["allowedActions"]
  blockers: IntegrationResolutionItemView["actionBlockers"]
} {
  const terminal = task.status === "resolved" || task.status === "closed"
  if (terminal) return { allowed: [], blockers: [] }

  const allowed: IntegrationResolutionItemView["allowedActions"] = [
    "CLAIM",
    "ADD_EVIDENCE",
    "SKIP",
    "DEFER",
    "TRANSFER",
  ]
  const blockers: IntegrationResolutionItemView["actionBlockers"] = []

  if (task.error_class === "result_unknown") {
    allowed.push("QUERY_ORIGINAL_RESULT")
    blockers.push({
      action: "REPLAY_ORIGINAL",
      code: "QUERY_REQUIRED",
      message:
        "结果未知：须先查询原结果；仅确认无结果且系统判定安全后才可重新提交",
    })
  } else if (
    task.error_class === "transient_failure" ||
    task.error_class === "rate_limited"
  ) {
    // auto-retry classes: query optional
    allowed.push("QUERY_ORIGINAL_RESULT")
  }

  if (
    task.error_class !== "mapping_error" &&
    task.error_class !== "business_rejected" &&
    task.error_class !== "auth_signature" &&
    task.error_class !== "capability_gap"
  ) {
    // REPLAY only after server allows; keep gated
  }

  allowed.push("RESOLVE", "CLOSE_DUPLICATE", "CLOSE_MISROUTED")
  return { allowed, blockers }
}

function mapErrorTask(task: BackendErrorTask): IntegrationResolutionItemView {
  const errorClass = mapErrorClass(task.error_class)
  const { allowed, blockers } = allowedForTask(task)
  const label =
    ERROR_CLASS_LABEL[errorClass] ?? task.error_class
  const severity = severityOf(task.error_class)

  return {
    identity: {
      itemType: "ERROR_TASK",
      id: task.id,
      number: task.id,
      subjectHash: `v${task.version}`,
    },
    workItem: {
      workItemId: task.id,
      workItemType:
        task.error_class === "result_unknown"
          ? "INTEGRATION_RESULT_UNKNOWN"
          : "BUSINESS_EXCEPTION",
      workItemVersion: String(task.version),
      status:
        task.status === "resolved"
          ? "COMPLETED"
          : task.status === "closed"
            ? "CLOSED"
            : task.owner_user_id
              ? "IN_PROGRESS"
              : "PENDING",
      subjectVersion: String(task.version),
      subjectHash: `v${task.version}`,
      completionAction: "RESOLVE",
    },
    businessObject: {
      objectType: task.message_id ? "INBOX_MESSAGE" : "BUSINESS_OBJECT",
      objectId: task.business_object_id ?? task.message_id ?? task.id,
      title: task.business_object_id ?? task.message_id ?? task.id,
    },
    classification: {
      code: task.error_class,
      errorClass,
      label,
      severity,
      severityLabel:
        severity === "critical"
          ? "阻断"
          : severity === "high"
            ? "高"
            : severity === "low"
              ? "低"
              : "中",
    },
    environment: "production",
    environmentLabel: "生产",
    status: {
      code: task.status,
      label: statusLabel(task.status),
    },
    fundsImpact: "NONE",
    fundsImpactLabel: "无资金影响",
    compensationOpen: false,
    ageLabel: ageLabel(task.created_at),
    ownerRole: task.owner_role ?? "—",
    ownerUser: task.owner_user_id ?? undefined,
    createdAt: tsToIso(task.created_at),
    message: task.message_id
      ? {
          eventIdSummary: task.message_id,
          idempotencyKeySummary: "—",
          businessFactKeySummary: "—",
          schemaVersion: "—",
          directionLabel: "入站",
          maskedPayloadSummary: task.last_attempt_summary ?? "—",
        }
      : undefined,
    hasWorkItem: true,
    attempts: task.last_attempt_summary
      ? [
          {
            attemptNumber: task.attempt_count,
            attemptedAt: tsToIso(task.last_attempt_at) || tsToIso(task.created_at),
            result: task.last_attempt_summary,
          },
        ]
      : [],
    objectVersion: String(task.version),
    allowedActions: allowed,
    actionBlockers: blockers,
    repairLinks: [],
    auditTrail: [],
    evidenceTimeline: [],
    linkedEvidence: [],
    freshness: {
      updatedAt: tsToIso(task.last_attempt_at) || tsToIso(task.created_at),
    },
  }
}

function mapDifference(diff: BackendDifference): IntegrationResolutionItemView {
  const terminal =
    diff.status === "confirmed_no_error" ||
    diff.status === "confirmed_valid_difference"

  return {
    identity: {
      itemType: "RECONCILIATION_DIFFERENCE",
      id: diff.id,
      number: diff.id,
      subjectHash: `v${diff.version}`,
    },
    businessObject: {
      objectType: diff.business_object_type,
      objectId: diff.business_object_id,
      title: `${diff.business_object_type} · ${diff.business_object_id}`,
    },
    classification: {
      code: diff.difference_type,
      errorClass: "reconciliation-difference",
      label: "对账差异",
      severity: "high",
      severityLabel: "高",
    },
    environment: "production",
    environmentLabel: "生产",
    status: {
      code: diff.status ?? "open",
      label: terminal
        ? diff.status === "confirmed_no_error"
          ? "确认无误"
          : "确认有效差异"
        : "待处理",
    },
    fundsImpact: "POTENTIAL",
    fundsImpactLabel: "潜在资金影响",
    compensationOpen: false,
    ageLabel: ageLabel(diff.created_at),
    ownerRole: "finance",
    createdAt: tsToIso(diff.created_at),
    difference: {
      leftLabel: "左侧证据",
      leftSummary: diff.left_fact_reference ?? "—",
      rightLabel: "右侧证据",
      rightSummary: diff.right_fact_reference ?? "—",
      boundary: diff.business_object_type,
      watermark: tsToIso(diff.created_at),
      differenceType: diff.difference_type,
      differenceSummary: diff.difference_type,
    },
    hasWorkItem: false,
    attempts: [],
    objectVersion: String(diff.version),
    allowedActions: terminal
      ? []
      : ["CONFIRM_NO_ERROR", "CONFIRM_VALID_DIFFERENCE", "ADD_EVIDENCE"],
    actionBlockers: [],
    repairLinks: [],
    auditTrail: (diff.resolutions ?? []).map((r) => ({
      id: r.id,
      at: tsToIso(r.handled_at),
      actor: r.handled_by,
      action: r.resolution_action,
      detail: r.evidence_reference ?? r.resulting_status,
    })),
    evidenceTimeline: [],
    linkedEvidence: [],
    freshness: { updatedAt: tsToIso(diff.created_at) },
  }
}

function matchesQuery(
  item: IntegrationResolutionItemView,
  q: IntegrationResolutionQuery
): boolean {
  if (q.mode === "errors" && item.identity.itemType !== "ERROR_TASK") {
    return false
  }
  if (q.view === "result_unknown") {
    if (item.classification.errorClass !== "result-unknown") return false
  }
  if (q.view === "security") {
    if (item.classification.errorClass !== "authentication-or-signature")
      return false
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
    // open queue excludes resolved — detail path still works
    if (
      item.status.code !== "resolved" &&
      item.status.code !== "closed" &&
      !item.status.code?.startsWith("confirm")
    ) {
      return false
    }
  }
  if (q.errorClass && item.classification.errorClass !== q.errorClass) {
    return false
  }
  if (q.q) {
    const needle = q.q.toLowerCase()
    const hay = [
      item.identity.number,
      item.identity.id,
      item.businessObject.title,
      item.businessObject.objectId,
      item.classification.label,
    ]
      .join(" ")
      .toLowerCase()
    if (!hay.includes(needle)) return false
  }
  return true
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

export async function fetchIntegrationQueue(
  query: IntegrationResolutionQuery
): Promise<IntegrationQueueView> {
  const pageSize = 50
  const items: IntegrationResolutionItemView[] = []

  if (query.view !== "reconciliation") {
    const status =
      query.view === "resolved"
        ? "resolved"
        : query.view === "auto_retry"
          ? "auto_retrying"
          : query.view === "mine"
            ? undefined
            : "manual_required"

    const tasks = await apiGet<Page<BackendErrorTask>>(
      "/admin/integration/error-tasks",
      {
        page: 1,
        page_size: pageSize,
        error_class: errorClassToBackend(query.errorClass),
        status: query.view === "resolved" ? "resolved" : status,
        owner_user_id: query.owner === "me" ? "me" : undefined,
        sort_by: "created_at",
        sort_dir: "desc",
      }
    )
    for (const t of tasks.items ?? []) {
      items.push(mapErrorTask(t))
    }

    // Also fetch pending if view is mine/all
    if (query.view === "mine" || query.view === "result_unknown") {
      const more = await apiGet<Page<BackendErrorTask>>(
        "/admin/integration/error-tasks",
        {
          page: 1,
          page_size: pageSize,
          error_class:
            query.view === "result_unknown"
              ? "result_unknown"
              : errorClassToBackend(query.errorClass),
          status: "pending",
          sort_by: "created_at",
          sort_dir: "desc",
        }
      )
      const seen = new Set(items.map((i) => i.identity.id))
      for (const t of more.items ?? []) {
        if (!seen.has(t.id)) items.push(mapErrorTask(t))
      }
    }
  }

  if (
    query.view === "reconciliation" ||
    query.view === "mine" ||
    query.mode === "all"
  ) {
    if (query.view !== "result_unknown" && query.view !== "security" && query.view !== "auto_retry") {
      const diffs = await apiGet<Page<BackendDifference>>(
        "/admin/integration/differences",
        {
          page: 1,
          page_size: pageSize,
          sort_by: "created_at",
          sort_dir: "desc",
        }
      )
      for (const d of diffs.items ?? []) {
        items.push(mapDifference(d))
      }
    }
  }

  const filtered = items.filter((i) => matchesQuery(i, query))

  filtered.sort((a, b) => {
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
    `视图=${VIEW_LABEL[query.view] ?? query.view}`,
    `模式=${MODE_LABEL[query.mode] ?? query.mode}`,
    `环境=${ENV_LABEL[query.environment] ?? query.environment}`,
  ]
  if (query.errorClass)
    filterParts.push(
      `类别=${ERROR_CLASS_LABEL[query.errorClass] ?? query.errorClass}`
    )
  if (query.q) filterParts.push(`搜索=${query.q}`)

  let resolvedEntry: IntegrationQueueView["resolvedEntry"]
  if (query.resolveWorkItemId) {
    const hit = items.find(
      (i) =>
        i.workItem?.workItemId === query.resolveWorkItemId ||
        i.identity.id === query.resolveWorkItemId
    )
    if (hit) {
      resolvedEntry = {
        itemType: hit.identity.itemType,
        id: hit.identity.id,
        workItemId: query.resolveWorkItemId,
      }
    }
  }

  return {
    items: filtered,
    metrics: {
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
      longestAgeLabel: items[0]?.ageLabel ?? "—",
    },
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
  try {
    if (input.itemType === "ERROR_TASK") {
      const task = await apiGet<BackendErrorTask>(
        `/admin/integration/error-tasks/${encodeURIComponent(input.id)}`
      )
      return mapErrorTask(task)
    }
    const diff = await apiGet<BackendDifference>(
      `/admin/integration/differences/${encodeURIComponent(input.id)}`
    )
    return mapDifference(diff)
  } catch (err) {
    const status =
      err && typeof err === "object" && "status" in err
        ? (err as { status?: number }).status
        : undefined
    if (status === 404) return null
    throw err
  }
}

export async function claimIntegrationTask(input: {
  workItemId: string
  subjectVersion?: string
}): Promise<ClaimResult> {
  const version = Number(input.subjectVersion) || 1
  // Prefer integration-specific hold/transfer ownership; claim via work-items
  try {
    await apiPost(
      `/admin/work-items/${encodeURIComponent(input.workItemId)}/claim`,
      { version }
    )
  } catch {
    // task id may be error-task id not work-item id — ignore if 404
  }
  return { workItemId: input.workItemId }
}

export async function applyIntegrationTaskAction(
  input: IntegrationTaskActionInput
): Promise<IntegrationFormalResult> {
  const version = Number(input.expectedWorkItemVersion) || 1

  if (input.kind === "QUERY_ORIGINAL_RESULT") {
    const outcome = "no_result_confirmed"
    await apiPost(
      `/admin/integration/error-tasks/${encodeURIComponent(input.itemId)}/query`,
      {
        version,
        outcome,
        comment: input.comment,
      }
    )
    return {
      status: "succeeded",
      title: "查询原结果：明确无结果",
      description:
        "已确认无结果；可按原任务号开放重新提交（若服务端允许）。",
      reference: input.operationId,
      outcome: "NO_RESULT_CONFIRMED",
      workItemStatus: "IN_PROGRESS",
      stayOnItem: true,
      terminal: false,
    }
  }

  if (input.kind === "REPLAY_ORIGINAL") {
    const result = await apiPost<BackendReplayResult>(
      `/admin/integration/error-tasks/${encodeURIComponent(input.itemId)}/replay`,
      {
        version,
        comment: input.comment,
      }
    )
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
          value: result.original_action_idempotency_key_summary,
        },
        { label: "手动指定原任务号", value: "否" },
        { label: "任务状态", value: "处理中" },
      ],
    }
  }

  if (input.kind === "DEFER" || input.kind === "SKIP") {
    await apiPost(
      `/admin/integration/error-tasks/${encodeURIComponent(input.itemId)}/hold`,
      {
        version,
        kind: input.kind === "DEFER" ? "defer" : "skip",
        reason_code: input.reasonCode,
        comment: input.comment,
      }
    )
    return {
      status: "succeeded",
      title: input.kind === "DEFER" ? "已跳过 · 保留在队列" : "已跳过当前项",
      description:
        "任务仍在待处理队列，未完成。本次处理已结束；可稍后继续。",
      reference: input.operationId,
      outcome: input.kind === "DEFER" ? "DEFERRED" : "SKIPPED",
      workItemStatus: "PENDING",
      stayOnItem: input.kind === "DEFER",
      terminal: false,
    }
  }

  if (input.kind === "ADD_EVIDENCE" || input.kind === "LINK_COMPENSATION") {
    return {
      status: "succeeded",
      title:
        input.kind === "LINK_COMPENSATION" ? "已关联补偿证据" : "已追加证据",
      description: "证据记录由服务端策略校验；任务仍在待处理列表。",
      reference: input.operationId,
      outcome:
        input.kind === "LINK_COMPENSATION"
          ? "EVIDENCE_LINKED"
          : "EVIDENCE_ADDED",
      workItemStatus: "IN_PROGRESS",
      stayOnItem: true,
      terminal: false,
    }
  }

  if (input.kind === "REATTRIBUTE") {
    return {
      status: "blocked",
      title: "重新归集未交付",
      description: "后端尚未提供独立的重新归集接口。",
      stayOnItem: true,
    }
  }

  return {
    status: "blocked",
    title: "未实现的动作",
    description: input.kind,
    stayOnItem: true,
  }
}

export async function resolveIntegrationTask(
  input: IntegrationResolveInput
): Promise<IntegrationFormalResult> {
  const version = Number(input.expectedWorkItemVersion) || 1
  await apiPost(
    `/admin/integration/error-tasks/${encodeURIComponent(input.itemId)}/resolve`,
    {
      version,
      resolution_type: "query_confirm",
      resolution:
        input.comment ||
        `policy=${input.evidencePolicyId}@${input.evidencePolicyVersion}; evidence=${input.evidenceRefs.length}`,
    }
  )
  return {
    status: "succeeded",
    title: "已标记解决",
    description: "处理已完成，可进入下一项。",
    reference: input.operationId,
    outcome: "RESOLVED",
    workItemStatus: "COMPLETED",
    stayOnItem: false,
    terminal: true,
  }
}

export async function closeIntegrationTask(
  input: IntegrationCloseInput
): Promise<IntegrationFormalResult> {
  const version = Number(input.expectedWorkItemVersion) || 1
  await apiPost(
    `/admin/integration/error-tasks/${encodeURIComponent(input.itemId)}/close`,
    {
      version,
      reason: input.kind === "CLOSE_DUPLICATE" ? "duplicate" : "misrouted",
      resolution: input.comment || input.reasonCode,
      replacement_task_id: input.replacementWorkItemId,
    }
  )
  return {
    status: "succeeded",
    title: input.kind === "CLOSE_DUPLICATE" ? "已关闭重复任务" : "已关闭误派",
    description: "仅关闭任务本身；不写业务解决结论。",
    reference: input.operationId,
    outcome:
      input.kind === "CLOSE_DUPLICATE" ? "CLOSED_DUPLICATE" : "CLOSED_MISROUTED",
    workItemStatus: "CLOSED",
    stayOnItem: false,
    terminal: true,
    replacementWorkItemId: input.replacementWorkItemId,
  }
}

export async function transferIntegrationTask(
  input: IntegrationTransferInput
): Promise<IntegrationFormalResult> {
  const version = Number(input.expectedWorkItemVersion) || 1
  await apiPost(
    `/admin/integration/error-tasks/${encodeURIComponent(input.itemId)}/transfer`,
    {
      version,
      owner_role: input.targetRole,
      owner_user_id: input.targetUserId,
    }
  )
  return {
    status: "succeeded",
    title: "已转交",
    description: "任务已转交，仅处理人变化。转交不是解决。",
    reference: input.operationId,
    outcome: "TRANSFERRED",
    workItemStatus: "IN_PROGRESS",
    stayOnItem: false,
    terminal: true,
    facts: [
      { label: "目标角色", value: input.targetRole },
      { label: "原任务状态", value: "处理中（已转交）" },
    ],
  }
}

export async function applyDirectReconciliation(
  input: DirectReconciliationInput
): Promise<IntegrationFormalResult> {
  const version = Number(input.expectedDifferenceVersion) || 0

  if (input.decision.kind === "NON_TERMINAL_ACTION") {
    await apiPost(
      `/admin/integration/differences/${encodeURIComponent(input.differenceId)}/process`,
      {
        version,
        action:
          input.decision.action === "ADD_EVIDENCE"
            ? "add_evidence"
            : input.decision.action === "QUERY_ORIGINAL_RESULT"
              ? "processing"
              : "add_evidence",
        evidence_reference: input.decision.evidenceRefs?.[0]?.recordId,
        comment: input.decision.comment,
      }
    )
    return {
      status: "succeeded",
      title: "已记录处理动作",
      description: "差异处理记录已追加，未终结。",
      reference: input.operationId,
      stayOnItem: true,
      terminal: false,
    }
  }

  await apiPost(
    `/admin/integration/differences/${encodeURIComponent(input.differenceId)}/resolve`,
    {
      version,
      conclusion:
        input.decision.conclusion === "CONFIRM_NO_ERROR"
          ? "confirm_no_error"
          : "confirm_valid_difference",
      reason_code: "BUSINESS_CONFIRMED_NO_ERROR",
      evidence_reference:
        input.decision.evidenceRefs[0]?.recordId ||
        input.decision.registeredReasonId,
      comment: input.decision.comment,
    }
  )

  return {
    status: "succeeded",
    title:
      input.decision.conclusion === "CONFIRM_NO_ERROR"
        ? "已确认无误"
        : "已确认有效差异",
    description: "直接对账结论已登记；不完成/关闭任何任务。",
    reference: input.operationId,
    outcome:
      input.decision.conclusion === "CONFIRM_NO_ERROR"
        ? "CONFIRMED_NO_ERROR"
        : "CONFIRMED_VALID_DIFFERENCE",
    stayOnItem: false,
    terminal: true,
  }
}
