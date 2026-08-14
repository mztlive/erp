/**
 * W29 接口错误与对账中心 · 真实 HTTP API 请求函数（queryFn / mutationFn）。
 * 路径：/admin/integration/error-tasks、/admin/integration/differences、/admin/work-items
 * 后端 DTO 映射见 ./mappers。
 */

import { apiGet, apiPost, type Page } from "@/lib/api"
import {
    listWorkItems,
    mapWorkItemDto,
    type WorkItemProjection,
} from "@/features/work-items"
import type {
    DirectReconciliationInput,
    IntegrationFormalResult,
    IntegrationQueueView,
    IntegrationResolutionItemView,
    IntegrationResolutionQuery,
    IntegrationResolveInput,
    IntegrationTaskActionInput,
} from "../types"
import { ENV_LABEL, ERROR_CLASS_LABEL, MODE_LABEL, VIEW_LABEL } from "../types"
import {
    mapDifference,
    mapErrorTask,
    matchesQuery,
    errorClassToBackend,
    type BackendDifference,
    type BackendErrorTask,
} from "./mappers"
import {
    mapAllowedIntegrationActions,
    toDirectReconciliationWire,
    toTaskActionWire,
    toTaskCompletionWire,
} from "./wire"

function workItemObjectKey(type: string, id: string): string {
    return `${type.trim().toUpperCase()}:${id}`
}

async function fetchW29WorkItems(
    owner: IntegrationResolutionQuery["owner"],
    history = false,
): Promise<Map<string, WorkItemProjection>> {
    const scope = history
        ? "history"
        : owner === "team"
          ? "team"
          : owner === "assigned"
            ? "managed"
            : "mine"
    const page = await listWorkItems({
        scope,
        timezone:
            Intl.DateTimeFormat().resolvedOptions().timeZone || "Asia/Shanghai",
        page: 1,
        pageSize: 100,
    })
    const byObject = new Map<string, WorkItemProjection>()
    for (const dto of page.items) {
        const item = mapWorkItemDto(dto)
        const objectType = item.businessObjectType.trim().toUpperCase()
        if (
            item.destinationWorkspaceId !== "W29" ||
            (item.workItemType !== "INTEGRATION_RESULT_UNKNOWN" &&
                item.workItemType !== "BUSINESS_EXCEPTION") ||
            (objectType !== "INTEGRATION_ERROR_TASK" &&
                objectType !== "RECONCILIATION_DIFFERENCE")
        ) {
            continue
        }
        byObject.set(workItemObjectKey(objectType, item.businessObjectId), item)
    }
    return byObject
}

export async function fetchIntegrationQueue(
    query: IntegrationResolutionQuery,
): Promise<IntegrationQueueView> {
    const pageSize = 50
    const items: IntegrationResolutionItemView[] = []

    const workItems = await fetchW29WorkItems(
        query.owner,
        query.view === "resolved",
    )

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
            },
        )
        for (const t of tasks.items ?? []) {
            items.push(
                mapErrorTask(
                    t,
                    workItems.get(
                        workItemObjectKey("INTEGRATION_ERROR_TASK", t.id),
                    ),
                ),
            )
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
                },
            )
            const seen = new Set(items.map((i) => i.identity.id))
            for (const t of more.items ?? []) {
                if (!seen.has(t.id)) {
                    items.push(
                        mapErrorTask(
                            t,
                            workItems.get(
                                workItemObjectKey(
                                    "INTEGRATION_ERROR_TASK",
                                    t.id,
                                ),
                            ),
                        ),
                    )
                }
            }
        }
    }

    if (
        query.view === "reconciliation" ||
        query.view === "mine" ||
        query.mode === "all"
    ) {
        if (
            query.view !== "result_unknown" &&
            query.view !== "security" &&
            query.view !== "auto_retry"
        ) {
            const diffs = await apiGet<Page<BackendDifference>>(
                "/admin/integration/differences",
                {
                    page: 1,
                    page_size: pageSize,
                    sort_by: "created_at",
                    sort_dir: "desc",
                },
            )
            for (const d of diffs.items ?? []) {
                items.push(
                    mapDifference(
                        d,
                        workItems.get(
                            workItemObjectKey(
                                "RECONCILIATION_DIFFERENCE",
                                d.id,
                            ),
                        ),
                    ),
                )
            }
        }
    }

    const filtered = items.filter((i) => matchesQuery(i, query))

    filtered.sort((a, b) => {
        const rank = (i: IntegrationResolutionItemView) => {
            if (i.classification.errorClass === "authentication-or-signature")
                return 0
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
            `类别=${ERROR_CLASS_LABEL[query.errorClass] ?? query.errorClass}`,
        )
    if (query.q) filterParts.push(`搜索=${query.q}`)

    let resolvedEntry: IntegrationQueueView["resolvedEntry"]
    if (query.resolveWorkItemId) {
        const hit = items.find(
            (i) => i.workItem?.workItemId === query.resolveWorkItemId,
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
                (i) => i.classification.errorClass === "result-unknown",
            ).length,
            manualRequired: items.filter((i) => i.status.label.includes("人工"))
                .length,
            securityFaults: items.filter(
                (i) =>
                    i.classification.errorClass ===
                    "authentication-or-signature",
            ).length,
            openDifferences: items.filter(
                (i) => i.identity.itemType === "RECONCILIATION_DIFFERENCE",
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
}): Promise<IntegrationResolutionItemView> {
    const [mine, team, history] = await Promise.all([
        fetchW29WorkItems("me"),
        fetchW29WorkItems("team"),
        fetchW29WorkItems("me", true),
    ])
    const workItems = new Map([...mine, ...team, ...history])
    if (input.itemType === "ERROR_TASK") {
        const task = await apiGet<BackendErrorTask>(
            `/admin/integration/error-tasks/${encodeURIComponent(input.id)}`,
        )
        return mapErrorTask(
            task,
            workItems.get(
                workItemObjectKey("INTEGRATION_ERROR_TASK", input.id),
            ),
        )
    }
    const diff = await apiGet<BackendDifference>(
        `/admin/integration/differences/${encodeURIComponent(input.id)}`,
    )
    return mapDifference(
        diff,
        workItems.get(workItemObjectKey("RECONCILIATION_DIFFERENCE", input.id)),
    )
}

export async function applyIntegrationTaskAction(
    input: IntegrationTaskActionInput,
): Promise<IntegrationFormalResult> {
    const result = await apiPost<{
        work_item_id: string
        work_item_status: "OPEN"
        evidence: {
            operation_id: string
            outcome:
                | "TERMINAL_EVIDENCE_FOUND"
                | "NO_RESULT_CONFIRMED"
                | "RESULT_UNKNOWN"
                | "REPLAY_ACCEPTED"
                | "REATTRIBUTED"
                | "EVIDENCE_LINKED"
                | "EVIDENCE_ADDED"
            business_result_reference?: string | null
            evidence_reference?: string | null
        }
        next_allowed_actions: string[]
    }>("/admin/integration/task-actions", toTaskActionWire(input))
    const titleByOutcome: Record<typeof result.evidence.outcome, string> = {
        TERMINAL_EVIDENCE_FOUND: "已取得可验证结果",
        NO_RESULT_CONFIRMED: "已确认原操作无结果",
        RESULT_UNKNOWN: "结果仍需核实",
        REPLAY_ACCEPTED: "重新提交已受理",
        REATTRIBUTED: "重新归集已记录",
        EVIDENCE_LINKED: "补偿证据已关联",
        EVIDENCE_ADDED: "证据已补充",
    }
    return {
        status:
            result.evidence.outcome === "RESULT_UNKNOWN"
                ? "unknown"
                : "succeeded",
        title: titleByOutcome[result.evidence.outcome],
        description:
            "本次处理记录已追加；当前任务仍为待处理，取得完成凭证后需单独确认解决。",
        reference: result.evidence.operation_id,
        outcome: result.evidence.outcome,
        nextAllowedActions: mapAllowedIntegrationActions(
            result.next_allowed_actions,
            {
                hasWorkItem: true,
                hasResolutionPolicy: true,
                directConclusions: [],
            },
        ),
        workItemStatus: result.work_item_status,
        stayOnItem: true,
        terminal: false,
        facts: [
            ...(result.evidence.business_result_reference
                ? [
                      {
                          label: "业务结果",
                          value: result.evidence.business_result_reference,
                      },
                  ]
                : []),
            ...(result.evidence.evidence_reference
                ? [
                      {
                          label: "证据记录",
                          value: result.evidence.evidence_reference,
                      },
                  ]
                : []),
        ],
    }
}

export async function resolveIntegrationTask(
    input: IntegrationResolveInput,
): Promise<IntegrationFormalResult> {
    const result = await apiPost<{
        work_item_id: string
        work_item_status: "COMPLETED"
        operation_id: string
        resolution_record_id: string
        terminal_evidence_reference: string
    }>("/admin/integration/task-completions", toTaskCompletionWire(input))
    return {
        status: "succeeded",
        title: "已标记解决",
        description: "处理已完成，可进入下一项。",
        reference: result.resolution_record_id,
        outcome: "RESOLVED",
        workItemStatus: result.work_item_status,
        stayOnItem: false,
        terminal: true,
        facts: [
            {
                label: "完成凭证",
                value: result.terminal_evidence_reference,
            },
        ],
    }
}

export async function applyDirectReconciliation(
    input: DirectReconciliationInput,
): Promise<IntegrationFormalResult> {
    const result = await apiPost<{
        difference_id: string
        operation_id: string
        resolution_record_id: string
        resulting_status:
            | "OPEN"
            | "EVIDENCE_PENDING"
            | "CONFIRMED_NO_ERROR"
            | "CONFIRMED_VALID_DIFFERENCE"
        is_terminal: boolean
        outcome:
            | "TERMINAL_EVIDENCE_FOUND"
            | "NO_RESULT_CONFIRMED"
            | "RESULT_UNKNOWN"
            | "REPLAY_ACCEPTED"
            | "REATTRIBUTED"
            | "EVIDENCE_LINKED"
            | "EVIDENCE_ADDED"
            | "CONFIRMED_NO_ERROR"
            | "CONFIRMED_VALID_DIFFERENCE"
        business_result_reference?: string | null
    }>(
        `/admin/integration/differences/${encodeURIComponent(input.differenceId)}/decisions`,
        toDirectReconciliationWire(input),
    )

    return {
        status: result.outcome === "RESULT_UNKNOWN" ? "unknown" : "succeeded",
        title: result.is_terminal ? "对账结论已登记" : "对账证据已追加",
        description: result.is_terminal
            ? "直接对账结论已登记；未完成或关闭任何处理任务。"
            : "差异处理记录已追加，当前差异仍待处理。",
        reference: result.resolution_record_id,
        outcome: result.outcome,
        stayOnItem: !result.is_terminal,
        terminal: result.is_terminal,
        facts: result.business_result_reference
            ? [
                  {
                      label: "业务结果",
                      value: result.business_result_reference,
                  },
              ]
            : undefined,
    }
}
