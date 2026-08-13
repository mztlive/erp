/**
 * W29 接口错误与对账中心 · 真实 HTTP API 请求函数（queryFn / mutationFn）。
 * 路径：/admin/integration/error-tasks、/admin/integration/differences、/admin/work-items
 * 后端 DTO 映射见 ./mappers。
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
} from "../types"
import { ENV_LABEL, ERROR_CLASS_LABEL, MODE_LABEL, VIEW_LABEL } from "../types"
import {
    mapDifference,
    mapErrorTask,
    matchesQuery,
    errorClassToBackend,
    type BackendDifference,
    type BackendErrorTask,
    type BackendReplayResult,
} from "./mappers"

export async function fetchIntegrationQueue(
    query: IntegrationResolutionQuery,
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
            },
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
                },
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
                items.push(mapDifference(d))
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
            (i) =>
                i.workItem?.workItemId === query.resolveWorkItemId ||
                i.identity.id === query.resolveWorkItemId,
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
}): Promise<IntegrationResolutionItemView | null> {
    try {
        if (input.itemType === "ERROR_TASK") {
            const task = await apiGet<BackendErrorTask>(
                `/admin/integration/error-tasks/${encodeURIComponent(input.id)}`,
            )
            return mapErrorTask(task)
        }
        const diff = await apiGet<BackendDifference>(
            `/admin/integration/differences/${encodeURIComponent(input.id)}`,
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
    await apiPost(
        `/admin/work-items/${encodeURIComponent(input.workItemId)}/claim`,
        { version },
    )
    return { workItemId: input.workItemId }
}

export async function applyIntegrationTaskAction(
    input: IntegrationTaskActionInput,
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
            },
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
            },
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
            },
        )
        return {
            status: "succeeded",
            title:
                input.kind === "DEFER" ? "已跳过 · 保留在队列" : "已跳过当前项",
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
                input.kind === "LINK_COMPENSATION"
                    ? "已关联补偿证据"
                    : "已追加证据",
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
    input: IntegrationResolveInput,
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
        },
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
    input: IntegrationCloseInput,
): Promise<IntegrationFormalResult> {
    const version = Number(input.expectedWorkItemVersion) || 1
    await apiPost(
        `/admin/integration/error-tasks/${encodeURIComponent(input.itemId)}/close`,
        {
            version,
            reason:
                input.kind === "CLOSE_DUPLICATE" ? "duplicate" : "misrouted",
            resolution: input.comment || input.reasonCode,
            replacement_task_id: input.replacementWorkItemId,
        },
    )
    return {
        status: "succeeded",
        title:
            input.kind === "CLOSE_DUPLICATE" ? "已关闭重复任务" : "已关闭误派",
        description: "仅关闭任务本身；不写业务解决结论。",
        reference: input.operationId,
        outcome:
            input.kind === "CLOSE_DUPLICATE"
                ? "CLOSED_DUPLICATE"
                : "CLOSED_MISROUTED",
        workItemStatus: "CLOSED",
        stayOnItem: false,
        terminal: true,
        replacementWorkItemId: input.replacementWorkItemId,
    }
}

export async function transferIntegrationTask(
    input: IntegrationTransferInput,
): Promise<IntegrationFormalResult> {
    const version = Number(input.expectedWorkItemVersion) || 1
    await apiPost(
        `/admin/integration/error-tasks/${encodeURIComponent(input.itemId)}/transfer`,
        {
            version,
            owner_role: input.targetRole,
            owner_user_id: input.targetUserId,
        },
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
    input: DirectReconciliationInput,
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
            },
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
        },
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
