import { transferJson, transferFileInput } from "@/lib/data-transfer"
import { apiGet } from "@/lib/api"
import type { InquiryImportPreview } from "./components/inquiry-import-dialog"
import type {
    InquiryList,
    InquiryListQuery,
    ProcurementInquiry,
    SalesInquiry,
} from "./types"

export function inquiryPath(procurement = false, inquiryId?: string): string {
    const root = procurement
        ? "/admin/procurement-inquiries"
        : "/admin/sales-inquiries"
    return inquiryId ? `${root}/${encodeURIComponent(inquiryId)}` : root
}

export async function previewInquiryCreation(file: File, customerId: string) {
    return transferJson<InquiryImportPreview>("inquiry_creation_preview", {
        ...(await transferFileInput(file)),
        customer_id: customerId,
    })
}

export function fetchInquiries(query: InquiryListQuery, procurement = false) {
    return apiGet<InquiryList>(inquiryPath(procurement), {
        q: query.q,
        status: query.status,
        task_status: procurement ? query.task_status : undefined,
        page: query.page,
        scope_version: query.scope_version,
    })
}

export function fetchInquiryFeeSales(inquiryId: string) {
    return apiGet<
        Array<{
            id: string
            order_no: string
            status_label: string
            eligible_for_fee_allocation: boolean
        }>
    >(`${inquiryPath(false, inquiryId)}/fee-sales`)
}

export function fetchInquiry(
    inquiryId: string,
    procurement = false,
    taskId?: string,
) {
    return apiGet<SalesInquiry | ProcurementInquiry>(
        inquiryPath(procurement, inquiryId),
        taskId ? { task_id: taskId } : undefined,
    )
}
