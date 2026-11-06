import type { BackgroundJobView } from "@/lib/background-jobs"
import { apiGet, apiPost } from "@/lib/api"

export type BrandImportRow = {
    row_number: number
    name: string
    brand_code: string
    error?: string
}

export type BrandImportResult = {
    row_number: number
    status: "created" | "skipped" | "failed"
    message: string
    brand_code: string | null
}

export function importBrands(
    requestId: string,
    fileName: string,
    rows: BrandImportRow[],
) {
    return apiPost<BackgroundJobView>(
        "/admin/product-brands/import/jobs",
        {
            request_id: requestId,
            file_name: fileName,
            rows,
        },
        { timeoutMs: 120_000 },
    )
}

export function fetchBrandImportFailures(jobId: string) {
    return apiGet<{ rows: BrandImportRow[]; results: BrandImportResult[] }>(
        `/admin/product-brands/import/jobs/${encodeURIComponent(jobId)}/failures`,
    )
}
