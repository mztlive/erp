"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    fetchSupplierExecutionSetup,
    fetchSupplierAddressCandidates,
    querySupplierExecutionResult,
    saveSupplierExecutionSetup,
    type SaveSupplierExecutionSetup,
    type SupplierAddressLevel,
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
                "DISCOVER_ORDERS",
                "CONFIRM_ORDER",
            ].includes(query.state.data.stage)
                ? 5000
                : false,
    })
}

export function useSupplierAddressCandidatesQuery(
    orderId: string,
    executionVersion: number,
    level: SupplierAddressLevel,
    parentTicket: string | undefined,
    enabled: boolean,
) {
    return useQuery({
        queryKey: [
            ...executionSetupKey(orderId),
            "address-candidates",
            executionVersion,
            level,
            parentTicket ?? null,
        ],
        queryFn: ({ signal }) =>
            fetchSupplierAddressCandidates(
                orderId,
                level,
                parentTicket,
                signal,
            ),
        enabled: enabled && Boolean(orderId),
        retry: false,
        refetchOnMount: "always",
    })
}

export function useQuerySupplierExecutionResultMutation(orderId: string) {
    const client = useQueryClient()
    return useMutation({
        mutationFn: (expectedVersion: number) =>
            querySupplierExecutionResult(orderId, expectedVersion),
        onSuccess: async () => {
            await client.invalidateQueries({ queryKey: ["supplier-orders"] })
        },
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
