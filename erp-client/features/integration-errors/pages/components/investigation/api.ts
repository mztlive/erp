import { getWorkItem, mapWorkItemDto } from "@/features/work-items"
import { apiGet } from "@/lib/api"

import {
    mapDifference,
    mapErrorTask,
    type BackendDifference,
    type BackendErrorTask,
} from "../../../api/mappers"
import type { IntegrationItemType } from "../../../types"

async function fetchRelatedTask(
    workItemId: string,
    objectType: "integration_error_task" | "reconciliation_difference",
    objectId: string,
) {
    const workItem = mapWorkItemDto(await getWorkItem(workItemId))
    if (
        workItem.businessObjectType !== objectType ||
        workItem.businessObjectId !== objectId
    ) {
        throw new Error("关联任务已变化，请刷新后重新打开。")
    }
    return workItem
}

/** 装配工作台待办版本与异常详情，保留尝试计数和差异证据策略。 */
export async function fetchInvestigationDetail(
    itemType: IntegrationItemType,
    id: string,
    workItemId: string,
) {
    if (itemType === "ERROR_TASK") {
        const error = await apiGet<BackendErrorTask>(
            `/admin/integration/error-tasks/${encodeURIComponent(id)}`,
        )
        if (error.work_item_id && error.work_item_id !== workItemId) {
            throw new Error("关联任务已变化，请刷新后重新打开。")
        }
        const workItem = error.work_item_id
            ? await fetchRelatedTask(workItemId, "integration_error_task", id)
            : undefined
        return {
            item: mapErrorTask(error, workItem),
            attemptCount: error.attempt_count,
        }
    }

    const [difference, workItem] = await Promise.all([
        apiGet<BackendDifference>(
            `/admin/integration/differences/${encodeURIComponent(id)}`,
        ),
        fetchRelatedTask(workItemId, "reconciliation_difference", id),
    ])

    // 直接对账原因仅在没有正式待办时下发。以服务端分支为准。
    return {
        item: mapDifference(
            difference,
            difference.reconciliation_reason_registry ? undefined : workItem,
        ),
        attemptCount: undefined,
    }
}
