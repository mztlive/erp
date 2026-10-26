"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    fetchSupplierExecutionSetup,
    saveSupplierExecutionSetup,
    type SaveSupplierExecutionSetup,
} from "../api/execution-setup"

const executionSetupKey = (orderId: string) =>
    ["supplier-orders", "execution-setup", orderId] as const

export function useSupplierExecutionSetupQuery(orderId: string) {
    return useQuery({
        queryKey: executionSetupKey(orderId),
        queryFn: () => fetchSupplierExecutionSetup(orderId),
        enabled: Boolean(orderId),
        refetchInterval: (query) =>
            query.state.data &&
            [
                "QUEUED",
                "PREPARE_ADDRESS",
                "PLAN_DELIVERY",
                "CREATE",
                "CONFIRM_PAYMENT",
            ].includes(query.state.data.stage)
                ? 5000
                : false,
    })
}

export function useSaveSupplierExecutionSetupMutation(orderId: string) {
    const client = useQueryClient()
    return useMutation({
        mutationFn: (request: SaveSupplierExecutionSetup) =>
            saveSupplierExecutionSetup(orderId, request),
        onSuccess: async () => {
            await client.invalidateQueries({ queryKey: ["supplier-orders"] })
        },
    })
}
