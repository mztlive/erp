"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    cancelAllExportJobs,
    cancelExportJob,
    fetchExportJobDetail,
    fetchExportJobItems,
    fetchExportJobs,
    type ExportJobListParams,
} from "@/features/export-tasks/api"
import { isExportJobActive } from "@/features/export-tasks/labels"

export const exportTaskKeys = {
    all: ["export-tasks"] as const,
    list: (params: ExportJobListParams) =>
        [...exportTaskKeys.all, "list", params] as const,
    detail: (id: string) => [...exportTaskKeys.all, "detail", id] as const,
    items: (id: string) => [...exportTaskKeys.all, "items", id] as const,
}

export function useExportJobsQuery(params: ExportJobListParams) {
    return useQuery({
        queryKey: exportTaskKeys.list(params),
        queryFn: () => fetchExportJobs(params),
        refetchInterval: (current) => {
            const active = current.state.data?.items.some((job) =>
                isExportJobActive(job.status),
            )
            return active ? 2000 : false
        },
    })
}

export function useExportJobDetailQuery(id: string | null) {
    return useQuery({
        queryKey: exportTaskKeys.detail(id ?? ""),
        queryFn: () => fetchExportJobDetail(id ?? ""),
        enabled: Boolean(id),
        refetchInterval: (current) => {
            const status = current.state.data?.status
            return status && isExportJobActive(status) ? 2000 : false
        },
    })
}

export function useExportJobItemsQuery(id: string | null) {
    return useQuery({
        queryKey: exportTaskKeys.items(id ?? ""),
        queryFn: () => fetchExportJobItems(id ?? ""),
        enabled: Boolean(id),
    })
}

export function useCancelExportJobMutation() {
    const client = useQueryClient()
    return useMutation({
        mutationFn: cancelExportJob,
        retry: false,
        onSuccess: async () => {
            await client.invalidateQueries({ queryKey: exportTaskKeys.all })
        },
    })
}

export function useCancelAllExportJobsMutation() {
    const client = useQueryClient()
    return useMutation({
        mutationFn: cancelAllExportJobs,
        retry: false,
        onSuccess: async () => {
            await client.invalidateQueries({ queryKey: exportTaskKeys.all })
        },
    })
}
