"use client"

import { apiGet, apiPost } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

import type {
    BackgroundJobListParams,
    BackgroundJobView,
    BackgroundJobItemView,
} from "@/lib/background-jobs"
export type {
    JobStatus,
    BackgroundJobView,
    BackgroundJobItemView,
    BackgroundJobListParams,
} from "@/lib/background-jobs"

export async function fetchBackgroundJobs(
    params: BackgroundJobListParams,
): Promise<Page<BackgroundJobView>> {
    const { status, ...rest } = params
    return apiGet<Page<BackgroundJobView>>("/admin/background-jobs", {
        ...rest,
        status: status === "active" ? undefined : status,
        active_only: status === "active" ? true : rest.active_only,
    })
}

export async function fetchBackgroundJobDetail(
    id: string,
): Promise<BackgroundJobView> {
    return apiGet<BackgroundJobView>(`/admin/background-jobs/${id}`)
}

export async function fetchBackgroundJobItems(
    id: string,
    page = 1,
): Promise<Page<BackgroundJobItemView>> {
    return apiGet<Page<BackgroundJobItemView>>(
        `/admin/background-jobs/${id}/items`,
        {
            page,
            page_size: 100,
        },
    )
}

export async function cancelBackgroundJob(input: {
    id: string
    version: number
}): Promise<BackgroundJobView> {
    return apiPost<BackgroundJobView>(
        `/admin/background-jobs/${input.id}/cancel`,
        {
            version: input.version,
        },
    )
}

export type CancelAllBackgroundJobsResult = {
    cancelled_count: number
    skipped_count: number
    failed_count: number
}

/**
 * 停止并取消全部未完成后台任务。
 * @param jobType 任务类型；省略表示全部类型
 */
export async function cancelAllBackgroundJobs(
    jobType?: string,
): Promise<CancelAllBackgroundJobsResult> {
    return apiPost<CancelAllBackgroundJobsResult>(
        "/admin/background-jobs/cancel-all",
        jobType ? { job_type: jobType } : {},
    )
}
