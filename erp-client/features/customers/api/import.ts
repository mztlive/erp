import { apiGet, apiPost } from "@/lib/api"
import type { BackgroundJobView } from "@/lib/background-jobs"

export type CustomerImportRow = {
    row_number: number
    legal_name: string
    short_name?: string
    unified_credit_code?: string
    error?: string
}

export type CustomerImportSubmission = {
    request_id: string
    file_name: string
    rows: CustomerImportRow[]
}

export type CustomerImportResultRow = CustomerImportRow & { message: string }

export function submitCustomerImport(input: CustomerImportSubmission) {
    return apiPost<BackgroundJobView>("/admin/customers/import/jobs", input, {
        timeoutMs: 120_000,
    })
}

export function fetchCustomerImportResultRows(
    jobId: string,
    status: "success" | "skipped" | "failed" | "unprocessed",
) {
    return apiGet<CustomerImportResultRow[]>(
        `/admin/customers/import/jobs/${encodeURIComponent(jobId)}/result-rows?status=${status}`,
    )
}
