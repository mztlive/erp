"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"
import {
    fetchSalesChangeDraft,
    saveSalesChangeDraft,
    salesChangeSubmissionMatches,
    type SaveSalesChangeDraft,
} from "../api/sales-change-draft"

const draftKey = (id: string) => ["sales-orders", "change-draft", id] as const

export function useSalesChangeDraftQuery(id: string, enabled: boolean) {
    return useQuery({
        queryKey: draftKey(id),
        queryFn: () => fetchSalesChangeDraft(id),
        enabled,
        staleTime: 0,
        refetchOnWindowFocus: false,
        refetchOnReconnect: false,
    })
}

/** 保存与恢复统一走查询缓存；结果未知时保留原请求。 */
export function useSalesChangeDraftCommands(id: string) {
    const client = useQueryClient()
    const save = useMutation({
        mutationFn: (request: SaveSalesChangeDraft) =>
            saveSalesChangeDraft(id, request),
    })
    return {
        save: save.mutateAsync,
        read: () =>
            client.fetchQuery({
                queryKey: draftKey(id),
                queryFn: () => fetchSalesChangeDraft(id),
                staleTime: 0,
            }),
        matches: (version: number, contentHash: string) =>
            client.fetchQuery({
                queryKey: [...draftKey(id), "submission", version, contentHash],
                queryFn: () =>
                    salesChangeSubmissionMatches(id, version, contentHash),
                staleTime: 0,
            }),
    }
}
