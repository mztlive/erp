"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    applyDirectReconciliation,
    applyIntegrationTaskAction,
    fetchIntegrationItem,
    fetchIntegrationQueue,
    resolveIntegrationTask,
} from "@/features/integration-errors/api/requests"
import type {
    DirectReconciliationInput,
    IntegrationResolutionQuery,
    IntegrationResolveInput,
    IntegrationTaskActionInput,
} from "@/features/integration-errors/types"

const integrationErrorKeys = {
    all: ["integration-errors"] as const,
    queue: (query: IntegrationResolutionQuery) =>
        [...integrationErrorKeys.all, "queue", query] as const,
    item: (itemType: string, id: string) =>
        [...integrationErrorKeys.all, "item", itemType, id] as const,
}

export function useIntegrationQueueQuery(query: IntegrationResolutionQuery) {
    return useQuery({
        queryKey: integrationErrorKeys.queue(query),
        queryFn: () => fetchIntegrationQueue(query),
    })
}

export function useIntegrationItemQuery(input: {
    itemType: "ERROR_TASK" | "RECONCILIATION_DIFFERENCE"
    id: string
    enabled?: boolean
}) {
    return useQuery({
        queryKey: integrationErrorKeys.item(input.itemType, input.id),
        queryFn: () =>
            fetchIntegrationItem({ itemType: input.itemType, id: input.id }),
        enabled: input.enabled !== false && Boolean(input.id),
    })
}

export function useIntegrationActionMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: IntegrationTaskActionInput) =>
            applyIntegrationTaskAction(input),
        onSuccess: async (result) => {
            if (result.status === "succeeded" || result.status === "unknown") {
                await Promise.all([
                    queryClient.invalidateQueries({
                        queryKey: integrationErrorKeys.all,
                    }),
                    queryClient.invalidateQueries({
                        queryKey: ["work-items"],
                    }),
                ])
            }
        },
    })
}

export function useResolveIntegrationMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: IntegrationResolveInput) =>
            resolveIntegrationTask(input),
        onSuccess: async (result) => {
            if (result.status === "succeeded" || result.status === "unknown") {
                await Promise.all([
                    queryClient.invalidateQueries({
                        queryKey: integrationErrorKeys.all,
                    }),
                    queryClient.invalidateQueries({
                        queryKey: ["work-items"],
                    }),
                ])
            }
        },
    })
}

export function useDirectReconciliationMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: DirectReconciliationInput) =>
            applyDirectReconciliation(input),
        onSuccess: async (result) => {
            if (result.status === "succeeded") {
                await queryClient.invalidateQueries({
                    queryKey: integrationErrorKeys.all,
                })
            }
        },
    })
}
