"use client"

import { useQuery } from "@tanstack/react-query"
import { queryKeyRoots } from "@/lib/query-key-roots"
import type { FulfillmentOperationType } from "@/features/fulfillment-operations/types"
import { fetchFulfillmentGate } from "../api/fulfillment-gate"

export function useFulfillmentGate(
    workItemId: string,
    operationId: string,
    operationType: FulfillmentOperationType,
) {
    return useQuery({
        queryKey: [
            ...queryKeyRoots.fulfillmentOperations,
            "work-item-gate",
            workItemId,
            operationId,
            operationType,
        ],
        queryFn: () =>
            fetchFulfillmentGate(workItemId, operationId, operationType),
        staleTime: 10_000,
        refetchInterval: 60_000,
    })
}
