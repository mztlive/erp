import { listWorkItems } from "@/features/work-items"

import { countOpenWorkItems, mapQueueWorkItemDto } from "./dto"
import type { UnifiedQueueFilters, UnifiedTaskQueueView } from "../types"

function currentTimezone(): string {
    return Intl.DateTimeFormat().resolvedOptions().timeZone || "Asia/Shanghai"
}

export async function fetchUnifiedTaskQueue(
    filters: UnifiedQueueFilters,
): Promise<UnifiedTaskQueueView> {
    const page = await listWorkItems({
        scope: filters.scope,
        family: filters.family,
        workItemType: filters.workItemType,
        status: filters.scope === "history" ? filters.historyStatus : undefined,
        due: filters.due,
        priorities: filters.priorities,
        query: filters.query,
        sort: filters.sort,
        queueContextId: filters.queueContextId,
        currentWorkItemId: filters.currentWorkItemId,
        timezone: currentTimezone(),
    })

    return {
        queueContextId:
            page.queue_context_id ??
            page.items[0]?.queue_context_id ??
            undefined,
        total: page.total,
        items: page.items.map((item) =>
            mapQueueWorkItemDto(item, filters.viewerUserId),
        ),
    }
}

export async function fetchUnifiedTaskQueueCounts() {
    const page = await listWorkItems({
        scope: "mine",
        timezone: currentTimezone(),
        pageSize: 100,
    })
    return countOpenWorkItems(page.items.map(mapQueueWorkItemDto))
}
