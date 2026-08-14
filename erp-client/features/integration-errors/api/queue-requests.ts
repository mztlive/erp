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
import { fetchW29WorkItems, workItemObjectKey } from "./work-item-lookup"

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
