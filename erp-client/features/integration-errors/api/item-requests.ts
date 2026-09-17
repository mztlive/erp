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

export async function fetchIntegrationItem(input: {
    itemType: "ERROR_TASK" | "RECONCILIATION_DIFFERENCE"
    id: string
}): Promise<IntegrationResolutionItemView> {
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
}
