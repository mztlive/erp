"use client"
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import { queryKeyRoots } from "@/lib/query-key-roots"
import {
    fetchPaymentArrangement,
    schedulePaymentFollowUp,
    type SchedulePaymentFollowUp,
} from "../api/payment-arrangement"

export function usePaymentArrangement(id: string) {
    return useQuery({
        queryKey: ["purchase-orders", "payment-arrangement", id],
        queryFn: () => fetchPaymentArrangement(id),
        enabled: Boolean(id),
    })
}
export function useSchedulePaymentFollowUp(id: string) {
    const client = useQueryClient()
    return useMutation({
        mutationFn: (data: SchedulePaymentFollowUp) =>
            schedulePaymentFollowUp(id, data),
        onSuccess: async () => {
            await Promise.all([
                client.invalidateQueries({
                    queryKey: queryKeyRoots.purchaseOrders,
                }),
                client.invalidateQueries({
                    queryKey: queryKeyRoots.workItems,
                }),
                client.invalidateQueries({
                    queryKey: queryKeyRoots.workspaceHome,
                }),
            ])
        },
    })
}
