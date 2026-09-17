/**
 * W29 正式处理责任的批量查询：按业务对象键归集 work item 投影。
 * 从 requests.ts 拆出，供队列与详情请求函数共用。
 */

import {
    listWorkItems,
    mapWorkItemDto,
    type WorkItemProjection,
} from "@/features/work-items"

export function workItemObjectKey(type: string, id: string): string {
    return `${type.trim().toUpperCase()}:${id}`
}

export async function fetchW29WorkItems(input?: {
    handlerUserIds?: string
    history?: boolean
}): Promise<Map<string, WorkItemProjection>> {
    const result = await listWorkItems({
        scope: input?.history ? "history" : "managed",
        timezone:
            Intl.DateTimeFormat().resolvedOptions().timeZone ||
            "Asia/Shanghai",
        page: 1,
        pageSize: 100,
        handlerUserIds: input?.handlerUserIds,
    })
    const byObject = new Map<string, WorkItemProjection>()
    for (const dto of result.items) {
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
