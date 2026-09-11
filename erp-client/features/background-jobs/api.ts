"use client"

import { apiGet, apiPost } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

export type JobStatus =
    | "pending"
    | "running"
    | "partially_succeeded"
    | "succeeded"
    | "failed"
    | "cancelled"

export type BackgroundJobView = {
    id: string
    job_no: string
    job_type: string
    domain_job_type: string | null
    domain_job_id: string | null
    selection_snapshot_id: string | null
    requested_by: string
    status: JobStatus
    total_count: number
    processed_count: number
    success_count: number
    skipped_count: number
    failed_count: number
    started_at: number | null
    finished_at: number | null
    last_progress_at: number | null
    result_expires_at: number | null
    result_file_asset_id: string | null
    error_summary: string | null
    version: number
    created_at: number
}

export type BackgroundJobItemView = {
    id: string
    background_job_id: string
    item_no: number
    object_type: string | null
    object_id: string | null
    worksheet_name: string | null
    source_row_no: number | null
    status: string | null
    result_code: string | null
    result_summary: string | null
    result_object_type: string | null
    result_object_id: string | null
}

export type BackgroundJobListParams = {
    page: number
    page_size: number
    job_no?: string
    status?: JobStatus | "active"
    /** 业务类型（`PRODUCT_IMPORT`/`SALES_ORDER_EXPORT` 等）。 */
    domain_job_type?: string
    /** 任务类型（`import`/`export`…）；省略表示全部类型。 */
    job_type?: string
    requested_by?: string
}

export async function fetchBackgroundJobs(
    params: BackgroundJobListParams,
): Promise<Page<BackgroundJobView>> {
    const { status, ...rest } = params
    return apiGet<Page<BackgroundJobView>>("/admin/background-jobs", {
        ...rest,
        status: status === "active" ? undefined : status,
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
