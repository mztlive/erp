import { apiPost } from "@/lib/api"
import type { DirectReturnRequest } from "../direct-return-types"
export async function confirmDirectReturn(input: {
    purchaseOrderId: string
    request: DirectReturnRequest
}) {
    return apiPost<string>(
        `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/direct-returns`,
        input.request,
    )
}
