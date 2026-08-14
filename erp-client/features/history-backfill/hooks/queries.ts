"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    fetchHistoryBackfillDetail,
    fetchHistoryBackfillList,
    submitHistoryBackfillCommand,
} from "@/features/history-backfill/api/history-backfill"
import type {
    HistoryBackfillCommandInput,
    HistoryBackfillDetailQuery,
    HistoryBackfillListQuery,
    HistoryBackfillProcessingStatus,
} from "@/features/history-backfill/types"

const historyBackfillKeys = {
    all: ["history-backfill"] as const,
    list: (query: HistoryBackfillListQuery) =>
        [...historyBackfillKeys.all, "list", query] as const,
    detail: (query: HistoryBackfillDetailQuery) =>
        [...historyBackfillKeys.all, "detail", query] as const,
}

export function useHistoryBackfillListQuery(query: HistoryBackfillListQuery) {
    return useQuery({
        queryKey: historyBackfillKeys.list(query),
        queryFn: () => fetchHistoryBackfillList(query),
    })
}

/** 运行中任务允许后台刷新，但不把轮询当成功判定 */
export function historyBackfillDetailRefetchInterval(
    status: HistoryBackfillProcessingStatus | undefined,
): number | false {
    if (status === "RUNNING" || status === "VALIDATING") return 8_000
    return false
}

export function useHistoryBackfillDetailQuery(
    query: HistoryBackfillDetailQuery,
    enabled = true,
) {
    return useQuery({
        queryKey: historyBackfillKeys.detail(query),
        queryFn: () => fetchHistoryBackfillDetail(query),
        enabled: enabled && Boolean(query.jobId),
        refetchInterval: (q) =>
            historyBackfillDetailRefetchInterval(
                q.state.data?.job.processingStatus,
            ),
    })
}

export function useHistoryBackfillCommandMutation() {
    const queryClient = useQueryClient()
    return useMutation({
        mutationFn: (input: HistoryBackfillCommandInput) =>
            submitHistoryBackfillCommand(input),
        onSuccess: async (result) => {
            if (result.status === "COMMITTED") {
                await queryClient.invalidateQueries({
                    queryKey: historyBackfillKeys.all,
                })
            }
        },
    })
}
