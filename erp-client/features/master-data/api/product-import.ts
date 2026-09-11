import { apiGet, apiPostForm } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

export type ProductImportJob = {
    id: string
    job_no: string
    status: string
    file_name: string | null
    total_count: number
    processed_count: number
    success_count: number
    skipped_count: number
    failed_count: number
    started_at: number | null
    finished_at: number | null
    error_summary: string | null
    version: number
    created_at: number
}

export type ProductImportItem = {
    item_no: number
    source_row_no: number | null
    name: string | null
    status: string | null
    result_summary: string | null
    product_id: string | null
}

export async function submitProductImport(input: {
    file: File
    requestId: string
}): Promise<ProductImportJob> {
    const body = new FormData()
    body.set("file", input.file)
    body.set("request_id", input.requestId)
    return apiPostForm<ProductImportJob>("/admin/products/import", body, {
        timeoutMs: 15 * 60 * 1000,
    })
}

export async function fetchProductImportJobs(page = 1): Promise<Page<ProductImportJob>> {
    return apiGet<Page<ProductImportJob>>("/admin/products/import-jobs", {
        page,
        page_size: 20,
    })
}

export async function fetchProductImportJob(id: string): Promise<ProductImportJob> {
    return apiGet<ProductImportJob>(`/admin/products/import-jobs/${id}`)
}

export async function fetchProductImportItems(
    id: string,
    page = 1,
): Promise<Page<ProductImportItem>> {
    return apiGet<Page<ProductImportItem>>(
        `/admin/products/import-jobs/${id}/items`,
        { page, page_size: 100 },
    )
}
