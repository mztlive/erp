/**
 * W29 队列请求函数。
 * 从 requests.ts 拆出；requests.ts 统一再导出 fetchIntegrationQueue。
 */

import { apiGet, type Page } from "@/lib/api"
import type {
    IntegrationQueueView,
    IntegrationResolutionItemView,
    IntegrationResolutionQuery,
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

type ScopedPage<T> = Page<T> & {
    empty_reason?: string | null
    scope_version?: string
    scope_summary?: string
    ownership_basis?: string
}

function handlerIds(query: IntegrationResolutionQuery): string | undefined {
    if (query.handlerUserIds?.trim()) return query.handlerUserIds.trim()
    if (query.view === "mine" && query.currentUserId?.trim()) {
        return query.currentUserId.trim()
    }
    return undefined
}

function errorStatus(query: IntegrationResolutionQuery): string | undefined {
    if (query.view === "resolved") return "resolved"
    if (query.view === "auto_retry") return "auto_retrying"
    if (query.view === "mine") return undefined
    return "manual_required"
}

export async function fetchIntegrationQueue(
    query: IntegrationResolutionQuery,
): Promise<IntegrationQueueView> {
    const pageSize = 50
    const items: IntegrationResolutionItemView[] = []
    const handlers = handlerIds(query)
    const operators = query.operatorUserIds?.trim() || undefined
    let emptyReason: "no_scope" | null = null
    let scopeVersion = query.scopeVersion
    let scopeSummary: string | undefined
    let ownershipBasis: string | undefined

    async function takeScope<T>(page: ScopedPage<T>): Promise<void> {
        if (page.empty_reason === "no_scope") emptyReason = "no_scope"
        if (page.scope_version) scopeVersion = page.scope_version
        scopeSummary ??= page.scope_summary
        ownershipBasis ??= page.ownership_basis
    }

    if (query.view !== "reconciliation") {
        const tasks = await apiGet<ScopedPage<BackendErrorTask>>(
            "/admin/integration/error-tasks",
            {
                page: 1,
                q: query.q?.trim() || undefined,
                page_size: pageSize,
                error_class:
                    query.view === "result_unknown"
                        ? "result_unknown"
                        : errorClassToBackend(query.errorClass),
                status:
                    query.view === "resolved" ? "resolved" : errorStatus(query),
                handler_user_ids: handlers,
                operator_user_ids: operators,
                scope_version: query.scopeVersion,
                sort_by: "created_at",
                sort_dir: "desc",
            },
        )
        await takeScope(tasks)
        for (const t of tasks.items) items.push(mapErrorTask(t))
    }

    if (
        query.mode !== "errors" &&
        (query.view === "reconciliation" ||
            query.view === "mine" ||
            query.mode === "all")
    ) {
        if (
            query.view !== "result_unknown" &&
            query.view !== "security" &&
            query.view !== "auto_retry"
        ) {
            const diffs = await apiGet<ScopedPage<BackendDifference>>(
                "/admin/integration/differences",
                {
                    page: 1,
                    q: query.q?.trim() || undefined,
                    page_size: pageSize,
                    handler_user_ids: handlers,
                    operator_user_ids: operators,
                    scope_version: query.scopeVersion,
                    sort_by: "created_at",
                    sort_dir: "desc",
                },
            )
            await takeScope(diffs)
            for (const d of diffs.items) items.push(mapDifference(d))
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
        return (
            rank(a) - rank(b) ||
            b.createdAt.localeCompare(a.createdAt) ||
            a.identity.itemType.localeCompare(b.identity.itemType) ||
            a.identity.id.localeCompare(b.identity.id)
        )
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
    if (handlers) filterParts.push(`当前处理人=${handlers}`)
    if (operators) filterParts.push(`历史处理人=${operators}`)

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
        emptyReason,
        scopeVersion,
        scopeSummary,
        ownershipBasis,
        metrics: {
            resultUnknown: filtered.filter(
                (i) => i.classification.errorClass === "result-unknown",
            ).length,
            manualRequired: filtered.filter((i) =>
                i.status.label.includes("人工"),
            ).length,
            securityFaults: filtered.filter(
                (i) =>
                    i.classification.errorClass ===
                    "authentication-or-signature",
            ).length,
            openDifferences: filtered.filter(
                (i) => i.identity.itemType === "RECONCILIATION_DIFFERENCE",
            ).length,
            longestAgeLabel:
                [...filtered].sort((a, b) =>
                    a.createdAt.localeCompare(b.createdAt),
                )[0]?.ageLabel ?? "—",
        },
        context: {
            queueContextId: query.queueContextId ?? `queue:W29:${query.view}`,
            filterSummary: filterParts.join(" · "),
            updatedAt: new Date().toISOString(),
        },
        resolvedEntry,
    }
}
