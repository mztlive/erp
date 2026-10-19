import { apiGet } from "@/lib/api"
import { decodeFulfillmentQueuePage } from "@/features/fulfillment-operations/api/work-queue"
import type { FulfillmentOperationType } from "@/features/fulfillment-operations/types"

/** 只读当前任务的付款放行条件，列表不加载履约及来源单据全文。 */
export async function fetchFulfillmentGate(
    workItemId: string,
    operationId: string,
    operationType: FulfillmentOperationType,
) {
    const page = decodeFulfillmentQueuePage(
        await apiGet<unknown>("/admin/work-items/fulfillment-queue", {
            operation_id: operationId,
            operation_types: operationType,
            page: 1,
            page_size: 1,
            timezone: "Asia/Shanghai",
        }),
    )
    return (
        page.items.find(
            (row) =>
                row.work_item_id === workItemId &&
                row.operation_id === operationId &&
                row.operation_type === operationType,
        )?.gate_state ?? null
    )
}
