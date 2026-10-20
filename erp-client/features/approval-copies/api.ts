import { apiGet, apiGetBlob, apiPost } from "@/lib/api"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import type { CopyTrigger } from "@/features/approval-processes/types"
export type CopyReceipt = {
    instance_status?: string
    id: string
    instance_id: string
    document_type: string
    document_no: string
    submitter_name: string | null
    trigger: CopyTrigger
    node_name: string | null
    round_no: number
    copied_at: number
    read_at: number | null
}
export type CopyFilters = {
    page?: number
    read?: boolean
    document_type?: string
    query?: string
}
export type CopyPage = {
    items: CopyReceipt[]
    total: number
    unread_count: number
    document_type_counts: Record<string, number>
}
export type CopyDetail = {
    receipt: CopyReceipt
    instance_status: string
    materials: ApprovalMaterials
    history: {
        items: {
            execution_id: string
            node_name: string
            round_no: number
            result: string
            assignee_name: string | null
            decision_reason: string | null
        }[]
        has_more: boolean
        next_cursor: string | null
    }
}
export const fetchCopies = (query: CopyFilters) =>
    apiGet<CopyPage>("/account/approval-copies", query)
export const fetchCopy = (id: string, after?: string) =>
    apiGet<CopyDetail>(`/account/approval-copies/${encodeURIComponent(id)}`, {
        after,
    })
export const markCopiesRead = (ids: string[]) =>
    apiPost<number>("/account/approval-copies/read", { ids })
export const fetchCopyFile = (id: string, fileId: string, preview: boolean) =>
    apiGetBlob(
        `/account/approval-copies/${encodeURIComponent(id)}/materials/${encodeURIComponent(fileId)}/${preview ? "preview" : "download"}`,
    )
