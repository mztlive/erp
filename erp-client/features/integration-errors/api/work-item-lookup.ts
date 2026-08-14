/**
 * W29 正式处理责任的批量查询：按业务对象键归集 work item 投影。
 * 从 requests.ts 拆出，供队列与详情请求函数共用。
 */

import { listWorkItems, mapWorkItemDto, type WorkItemProjection } from "@/features/work-items"
import type { IntegrationResolutionQuery } from "../types"

export function workItemObjectKey(type: string, id: string): string {
    return `${type.trim().toUpperCase()}:${id}`
}

export async function fetchW29WorkItems(
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
