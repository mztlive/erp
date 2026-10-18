import { apiPost } from "@/lib/api"
import type {
    DirectReturnRequest,
    ReturnSettlementDecision,
} from "../direct-return-types"
export async function confirmDirectReturn(input: {
    purchaseOrderId: string
    request: DirectReturnRequest
}) {
    return apiPost<string>(
        `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/direct-returns`,
        input.request,
    )
}

/** 补齐已有退货的财务交接；不登记退款或红票。 */
export async function syncReturnFinance(input: {
    purchaseOrderId: string
    decision: ReturnSettlementDecision
}): Promise<void> {
    await apiPost(
        `/admin/purchase-orders/${encodeURIComponent(input.purchaseOrderId)}/return-finance/sync`,
        input.decision,
    )
}
