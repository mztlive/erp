"use client"
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query"
import { fetchSupplierCredits, useSupplierCredit } from "../credit-api"
import { paymentRequestKeys } from "./queries"
import { workItemKeys } from "@/features/work-items/queries"
import { queryKeyRoots } from "@/lib/query-key-roots"
export function useSupplierCreditsQuery(requestId: string) {
    return useQuery({
        queryKey: [...paymentRequestKeys.all, "credits", requestId],
        queryFn: () => fetchSupplierCredits(requestId),
    })
}
export function useSupplierCreditMutation() {
    const client = useQueryClient()
    return useMutation({
        mutationFn: useSupplierCredit,
        onSuccess: () => {
            void client.invalidateQueries({ queryKey: paymentRequestKeys.all })
            void client.invalidateQueries({ queryKey: workItemKeys.all })
            void client.invalidateQueries({
                queryKey: queryKeyRoots.purchaseOrders,
            })
            void client.invalidateQueries({
                queryKey: queryKeyRoots.supplierPayables,
            })
            void client.invalidateQueries({
                queryKey: queryKeyRoots.workspaceHome,
            })
        },
    })
}
