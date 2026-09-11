import { apiPostForm } from "@/lib/api"

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
