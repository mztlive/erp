"use client"

import { useQuery } from "@tanstack/react-query"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { fetchFulfillmentPayment } from "../api/purchase-payment"

export function usePurchasePayment(workItemId: string) {
    return useQuery({
        queryKey: [
            ...queryKeyRoots.workspaceHome,
            "purchase-payment",
            workItemId,
        ],
        queryFn: () => fetchFulfillmentPayment(workItemId),
        staleTime: 10_000,
        refetchInterval: 60_000,
    })
}
