import { apiGet, apiPost } from "@/lib/api"

export type FulfillmentHandoverCandidate = {
    user_id: string
    display_name: string
    account: string
}

export type FulfillmentHandoverResult = {
    order_id: string
    follow_up_user_id: string
    business_org_unit_id: string
    version: number
    transferred_work_item_ids: string[]
}

export async function fetchFulfillmentHandoverCandidates(
    orderId: string,
): Promise<FulfillmentHandoverCandidate[]> {
    return apiGet<FulfillmentHandoverCandidate[]>(
        `/admin/supplier-fulfillment-orders/${orderId}/handover-candidates`,
    )
}

export async function handoverFulfillmentOrder(
    orderId: string,
    input: {
        targetUserId: string
        targetOrgUnitId?: string
        reason: string
        expectedVersion: number
        idempotencyKey: string
        transferOpenExceptionTasks?: boolean
    },
): Promise<FulfillmentHandoverResult> {
    return apiPost<FulfillmentHandoverResult>(
        `/admin/supplier-fulfillment-orders/${orderId}/handover`,
        {
            target_user_id: input.targetUserId,
            target_org_unit_id: input.targetOrgUnitId,
            reason: input.reason,
            expected_version: input.expectedVersion,
            idempotency_key: input.idempotencyKey,
            transfer_open_exception_tasks:
                input.transferOpenExceptionTasks ?? false,
        },
    )
}
