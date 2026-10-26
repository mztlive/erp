/** 后台任务跨业务共用的数据合同与缓存身份。 */
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
    active_only?: boolean
    /** 业务类型（`PRODUCT_IMPORT`/`SALES_ORDER_EXPORT` 等）。 */
    domain_job_type?: string
    /** 任务类型（`import`/`export`…）；省略表示全部类型。 */
    job_type?: string
    requested_by?: string
}

export const backgroundJobKeys = {
    all: ["background-jobs"] as const,
    list: (params: BackgroundJobListParams) =>
        [...backgroundJobKeys.all, "list", params] as const,
    detail: (id: string) => [...backgroundJobKeys.all, "detail", id] as const,
    items: (id: string, page = 1) =>
        [...backgroundJobKeys.all, "items", id, page] as const,
}
