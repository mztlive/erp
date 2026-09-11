"use client"

import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query"

import {
    cancelAllBackgroundJobs,
    cancelBackgroundJob,
    fetchBackgroundJobDetail,
    fetchBackgroundJobItems,
    fetchBackgroundJobs,
    type BackgroundJobListParams,
} from "@/features/background-jobs/api"
import { isJobActive } from "@/features/background-jobs/labels"

export const backgroundJobKeys = {
    all: ["background-jobs"] as const,
    list: (params: BackgroundJobListParams) =>
        [...backgroundJobKeys.all, "list", params] as const,
    detail: (id: string) => [...backgroundJobKeys.all, "detail", id] as const,
    items: (id: string) => [...backgroundJobKeys.all, "items", id] as const,
}

export function useBackgroundJobsQuery(params: BackgroundJobListParams) {
    return useQuery({
        queryKey: backgroundJobKeys.list(params),
        queryFn: () => fetchBackgroundJobs(params),
        placeholderData: (previous) => previous,
        refetchInterval: (current) => {
            const active = current.state.data?.items.some((job) =>
                isJobActive(job.status),
            )
            return active ? 2000 : false
        },
    })
}

export function useBackgroundJobDetailQuery(id: string | null) {
    return useQuery({
        queryKey: backgroundJobKeys.detail(id ?? ""),
        queryFn: () => fetchBackgroundJobDetail(id ?? ""),
        enabled: Boolean(id),
        placeholderData: (previous) => previous,
        refetchInterval: (current) => {
            const status = current.state.data?.status
            return status && isJobActive(status) ? 2000 : false
        },
    })
}

export function useBackgroundJobItemsQuery(id: string | null) {
    return useQuery({
        queryKey: backgroundJobKeys.items(id ?? ""),
        queryFn: () => fetchBackgroundJobItems(id ?? ""),
        enabled: Boolean(id),
    })
}

export function useCancelBackgroundJobMutation() {
    const client = useQueryClient()
    return useMutation({
        mutationFn: cancelBackgroundJob,
        retry: false,
        onSuccess: async () => {
            await client.invalidateQueries({ queryKey: backgroundJobKeys.all })
        },
    })
}

export function useCancelAllBackgroundJobsMutation() {
    const client = useQueryClient()
    return useMutation({
        mutationFn: (jobType?: string) => cancelAllBackgroundJobs(jobType),
        retry: false,
        onSuccess: async () => {
            await client.invalidateQueries({ queryKey: backgroundJobKeys.all })
        },
    })
}
