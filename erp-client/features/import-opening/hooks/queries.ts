"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    completeImportConfirmation,
    executeImportCommand,
    fetchImportBatchDetail,
    fetchImportBatchList,
    fetchImportIssues,
    startImportConfirmationProcessing,
    type CompleteImportConfirmationInput,
    type ExecuteImportCommandInput,
} from "@/features/import-opening/api/legacy-import"
import type {
    ImportBatchDetailContext,
    ImportBatchListQuery,
    ImportIssueQuery,
} from "@/features/import-opening/types"

export const importOpeningKeys = {
    all: ["import-opening"] as const,
    list: (query: ImportBatchListQuery) =>
        [...importOpeningKeys.all, "list", query] as const,
    detail: (context: ImportBatchDetailContext) =>
        [...importOpeningKeys.all, "detail", context] as const,
    issues: (query: ImportIssueQuery) =>
        [...importOpeningKeys.all, "issues", query] as const,
}

export function useImportBatchListQuery(query: ImportBatchListQuery) {
    return useQuery({
        queryKey: importOpeningKeys.list(query),
        queryFn: () => fetchImportBatchList(query),
    })
}

export function useImportBatchDetailQuery(
    context: ImportBatchDetailContext | undefined,
) {
    return useQuery({
        queryKey: importOpeningKeys.detail(context ?? { batchId: "" }),
        queryFn: () => fetchImportBatchDetail(context!),
        enabled: Boolean(context?.batchId),
    })
}

/** W18 责任建立与强类型确认写命令。 */
export function useImportConfirmationOperations() {
    const queryClient = useQueryClient()
    const refresh = async () => {
        await Promise.all([
            queryClient.invalidateQueries({
                queryKey: [...importOpeningKeys.all, "detail"],
            }),
            queryClient.invalidateQueries({
                queryKey: [...importOpeningKeys.all, "list"],
            }),
        ])
    }
    const start = useMutation({
        mutationFn: startImportConfirmationProcessing,
        onSuccess: refresh,
    })
    const complete = useMutation({
        mutationFn: (input: CompleteImportConfirmationInput) =>
            completeImportConfirmation(input),
        onSuccess: refresh,
    })
    return {
        startProcessing: start.mutateAsync,
        completeConfirmation: complete.mutateAsync,
        isStarting: start.isPending,
        isCompleting: complete.isPending,
        error: start.error ?? complete.error,
        resetError: () => {
            start.reset()
            complete.reset()
        },
    }
}

/** W18 独立应用、取消与失败项重试强命令。 */
export function useImportExecutionOperations() {
    const queryClient = useQueryClient()
    const execute = useMutation({
        mutationFn: (input: ExecuteImportCommandInput) =>
            executeImportCommand(input),
        onSuccess: async () => {
            await Promise.all([
                queryClient.invalidateQueries({
                    queryKey: [...importOpeningKeys.all, "detail"],
                }),
                queryClient.invalidateQueries({
                    queryKey: [...importOpeningKeys.all, "list"],
                }),
                queryClient.invalidateQueries({
                    queryKey: [...importOpeningKeys.all, "issues"],
                }),
            ])
        },
    })
    return {
        execute: execute.mutateAsync,
        isExecuting: execute.isPending,
        error: execute.error,
        resetError: execute.reset,
    }
}

export function useImportIssuesQuery(query: ImportIssueQuery, enabled = true) {
    return useQuery({
        queryKey: importOpeningKeys.issues(query),
        queryFn: () => fetchImportIssues(query),
        enabled: enabled && Boolean(query.batchId),
    })
}
