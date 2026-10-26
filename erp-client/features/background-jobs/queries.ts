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
import { readTransferContext } from "@/lib/data-transfer"

export function useDataTransferContextQuery(
    jobId: string,
    portal = false,
    enabled = true,
) {
    return useQuery({
        queryKey: ["data-transfer-context", portal, jobId],
        queryFn: () => readTransferContext(jobId, portal),
        enabled: enabled && Boolean(jobId),
        staleTime: 0,
        gcTime: 0,
        retry: false,
        refetchOnWindowFocus: "always",
        refetchInterval: (query) =>
            query.state.data?.resume_state === "pending" ? 2000 : false,
    })
}

import { backgroundJobKeys } from "@/lib/background-jobs"
export { backgroundJobKeys } from "@/lib/background-jobs"

/** 菜单角标使用服务端授权范围内的总数，不对当前页任务求和。 */
export function useBackgroundJobActiveCountQuery(enabled: boolean) {
    return useQuery({
        queryKey: [...backgroundJobKeys.all, "active-count"],
        queryFn: async () => {
            const page = await fetchBackgroundJobs({
                page: 1,
                page_size: 1,
                active_only: true,
            })
            return page.total
        },
        enabled,
        refetchInterval: 2000,
        refetchOnWindowFocus: "always",
    })
}

export function useBackgroundJobsQuery(params: BackgroundJobListParams) {
    return useQuery({
        queryKey: backgroundJobKeys.list(params),
        queryFn: () => fetchBackgroundJobs(params),
        placeholderData: (previous) => previous,
        refetchInterval: (current) => {
            const active = current.state.data?.items.some((job) =>
                isJobActive(job.status, job.finished_at),
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
        refetchInterval: (current) => {
            const status = current.state.data?.status
            return status &&
                isJobActive(status, current.state.data?.finished_at)
                ? 2000
                : false
        },
    })
}

export function useBackgroundJobItemsQuery(
    id: string | null,
    page = 1,
    active = false,
) {
    return useQuery({
        queryKey: [...backgroundJobKeys.items(id ?? "", page), active],
        queryFn: () => fetchBackgroundJobItems(id ?? "", page),
        placeholderData: (previous, query) =>
            query?.queryKey[2] === id ? previous : undefined,
        refetchInterval: active ? 2000 : false,
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
