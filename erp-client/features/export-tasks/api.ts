"use client"

import { apiGet, apiPost } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

export type ExportJobStatus =
    | "pending"
    | "running"
    | "partially_succeeded"
    | "succeeded"
    | "failed"
    | "cancelled"

export type ExportJob = {
    id: string
    job_no: string
    job_type: string
    domain_job_type: string | null
    domain_job_id: string | null
    selection_snapshot_id: string | null
    requested_by: string
    status: ExportJobStatus
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

export type ExportJobItem = {
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

export type ExportJobListParams = {
    page: number
    page_size: number
    job_no?: string
    status?: ExportJobStatus | "active"
    domain_job_type?: string
}

export async function fetchExportJobs(
    params: ExportJobListParams,
): Promise<Page<ExportJob>> {
    const { status, ...rest } = params
    return apiGet<Page<ExportJob>>("/admin/background-jobs", {
        ...rest,
        job_type: "export",
        status: status === "active" ? undefined : status,
    })
}

export async function fetchExportJobDetail(id: string): Promise<ExportJob> {
    return apiGet<ExportJob>(`/admin/background-jobs/${id}`)
}

export async function fetchExportJobItems(
    id: string,
    page = 1,
): Promise<Page<ExportJobItem>> {
    return apiGet<Page<ExportJobItem>>(`/admin/background-jobs/${id}/items`, {
        page,
        page_size: 100,
    })
}

export async function cancelExportJob(input: {
    id: string
    version: number
}): Promise<ExportJob> {
    return apiPost<ExportJob>(`/admin/background-jobs/${input.id}/cancel`, {
        version: input.version,
    })
}
