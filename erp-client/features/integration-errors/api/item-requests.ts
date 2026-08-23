/**
 * W29 单项详情请求函数。
 * 从 requests.ts 拆出；requests.ts 统一再导出 fetchIntegrationItem。
 */

import { apiGet } from "@/lib/api"
import type { IntegrationResolutionItemView } from "../types"
import {
    mapDifference,
    mapErrorTask,
    type BackendDifference,
    type BackendErrorTask,
} from "./mappers"
import { fetchW29WorkItems, workItemObjectKey } from "./work-item-lookup"

export async function fetchIntegrationItem(input: {
    itemType: "ERROR_TASK" | "RECONCILIATION_DIFFERENCE"
    id: string
}): Promise<IntegrationResolutionItemView> {
    const [mine, managed, history] = await Promise.all([
        fetchW29WorkItems("me"),
        fetchW29WorkItems("assigned"),
        fetchW29WorkItems("me", true),
    ])
    const workItems = new Map([...managed, ...mine, ...history])
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
