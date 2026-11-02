/**
 * W29 单项详情请求函数。
 * 从 requests.ts 拆出；requests.ts 统一再导出 fetchIntegrationItem。
 */

import { apiGet } from "@/lib/api"
import { getWorkItem, mapWorkItemDto } from "@/features/work-items"
import type { IntegrationResolutionItemView } from "../types"
import {
    mapDifference,
    mapErrorTask,
    type BackendDifference,
    type BackendErrorTask,
} from "./mappers"

export async function fetchIntegrationItem(input: {
    itemType: "ERROR_TASK" | "RECONCILIATION_DIFFERENCE"
    id: string
}): Promise<IntegrationResolutionItemView> {
    if (input.itemType === "ERROR_TASK") {
        const task = await apiGet<BackendErrorTask>(
            `/admin/integration/error-tasks/${encodeURIComponent(input.id)}`,
        )
        const workItem = task.work_item_id
            ? mapWorkItemDto(await getWorkItem(task.work_item_id))
            : undefined
        if (
            workItem &&
            (workItem.businessObjectType !== "integration_error_task" ||
                workItem.businessObjectId !== task.id)
        ) {
            throw new Error("关联任务已变化，请刷新后重新打开异常。")
        }
        return mapErrorTask(task, workItem)
    }
    const diff = await apiGet<BackendDifference>(
        `/admin/integration/differences/${encodeURIComponent(input.id)}`,
    )
    return mapDifference(diff)
}
