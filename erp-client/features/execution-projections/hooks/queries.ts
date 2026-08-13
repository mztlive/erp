"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    fetchExecutionProjectionDetail,
    fetchExecutionProjectionList,
    fetchSalesOrderCollaboration,
    submitBulkProjectionCommand,
    submitProjectionDeliveryCommand,
    type BulkCommandInput,
    type DeliveryCommandInput,
} from "@/features/execution-projections/api/projections"
import type { ExecutionProjectionListQuery } from "@/features/execution-projections/types"

const executionProjectionKeys = {
    all: ["execution-projections"] as const,
    list: (query: ExecutionProjectionListQuery) =>
        [...executionProjectionKeys.all, "list", query] as const,
    detail: (projectionId: string, revisionId?: string) =>
        [
            ...executionProjectionKeys.all,
            "detail",
            projectionId,
            revisionId ?? "current",
        ] as const,
    collaboration: (salesOrderId: string) =>
        [
            ...executionProjectionKeys.all,
            "collaboration",
            salesOrderId,
        ] as const,
    bulkJob: (jobId: string) =>
        [...executionProjectionKeys.all, "bulk-job", jobId] as const,
}

export function useExecutionProjectionListQuery(
    query: ExecutionProjectionListQuery,
) {
    return useQuery({
        queryKey: executionProjectionKeys.list(query),
        queryFn: () => fetchExecutionProjectionList(query),
    })
}

export function useExecutionProjectionDetailQuery(
    projectionId: string | undefined,
    revisionId?: string,
) {
    return useQuery({
        queryKey: executionProjectionKeys.detail(
            projectionId ?? "",
            revisionId,
        ),
        queryFn: () =>
            fetchExecutionProjectionDetail({
                projectionId: projectionId!,
                revisionId,
            }),
        enabled: Boolean(projectionId),
    })
}

export function useSalesOrderCollaborationQuery(salesOrderId: string) {
    return useQuery({
        queryKey: executionProjectionKeys.collaboration(salesOrderId),
        queryFn: () => fetchSalesOrderCollaboration(salesOrderId),
        enabled: Boolean(salesOrderId),
    })
}

export function useProjectionDeliveryCommandMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: DeliveryCommandInput) =>
            submitProjectionDeliveryCommand(input),
        onSuccess: async (result) => {
            // 结果未知期间仍失效以便刷新状态轨，但调用方不得标成功
            await queryClient.invalidateQueries({
                queryKey: executionProjectionKeys.all,
            })
            void result
        },
    })
}

export function useBulkProjectionCommandMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: BulkCommandInput) =>
            submitBulkProjectionCommand(input),
        onSuccess: async () => {
            await queryClient.invalidateQueries({
                queryKey: executionProjectionKeys.all,
            })
        },
    })
}
