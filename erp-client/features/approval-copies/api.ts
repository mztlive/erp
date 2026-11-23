import { apiGet, apiGetBlob, apiPost } from "@/lib/api"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import type { CopyTrigger } from "@/features/approval-processes/types"

/** 抄送队列与待办队列统一每页 50 条；后端接口固定 20 条/页且启用 deny_unknown_fields，前端聚合映射。 */
export const COPY_PAGE_SIZE = 50
const BACKEND_COPY_PAGE_SIZE = 20

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
export type CopyHistoryItemDto = {
    execution_id: string
    round_no: number
    execution_no?: number | null
    node_key?: string | null
    node_name: string
    result: string
    assignee_name: string | null
    decided_by?: string | null
    decision_reason: string | null
    decided_at?: number | null
}
export type CopyDetail = {
    receipt: CopyReceipt
    instance_status: string
    materials: ApprovalMaterials
    history: {
        items: CopyHistoryItemDto[]
        has_more: boolean
        next_cursor: string | null
    }
}

export async function fetchCopies(query: CopyFilters): Promise<CopyPage> {
    const logicalPage = Math.max(1, query.page ?? 1)
    const startIndex = (logicalPage - 1) * COPY_PAGE_SIZE
    const endIndex = startIndex + COPY_PAGE_SIZE
    const firstBackendPage = Math.floor(startIndex / BACKEND_COPY_PAGE_SIZE) + 1
    const lastBackendPage =
        Math.floor((endIndex - 1) / BACKEND_COPY_PAGE_SIZE) + 1

    const first = await apiGet<CopyPage>("/account/approval-copies", {
        ...query,
        page: firstBackendPage,
    })

    const sliceStart =
        startIndex - (firstBackendPage - 1) * BACKEND_COPY_PAGE_SIZE
    const neededEnd = Math.min(endIndex, first.total)
    if (neededEnd <= firstBackendPage * BACKEND_COPY_PAGE_SIZE) {
        return {
            ...first,
            items: first.items.slice(sliceStart, sliceStart + COPY_PAGE_SIZE),
        }
    }

    const extraPages: number[] = []
    for (
        let backendPage = firstBackendPage + 1;
        backendPage <= lastBackendPage;
        backendPage += 1
    ) {
        if ((backendPage - 1) * BACKEND_COPY_PAGE_SIZE < neededEnd) {
            extraPages.push(backendPage)
        }
    }

    const extras = await Promise.all(
        extraPages.map((backendPage) =>
            apiGet<CopyPage>("/account/approval-copies", {
                ...query,
                page: backendPage,
            }),
        ),
    )

    const seen = new Set<string>()
    const combined: CopyReceipt[] = []
    for (const chunk of [first, ...extras]) {
        for (const item of chunk.items) {
            if (seen.has(item.id)) continue
            seen.add(item.id)
            combined.push(item)
        }
    }

    return {
        ...first,
        items: combined.slice(sliceStart, sliceStart + COPY_PAGE_SIZE),
    }
}

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
