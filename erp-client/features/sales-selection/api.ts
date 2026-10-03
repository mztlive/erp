import { apiGet, apiPost, getApiBaseUrl } from "@/lib/api"
import type { ProposalView, PublicPageView } from "./types"
const proposals = "/admin/sales-selection-proposals"
const publicBase = "/public/selection"

/** 公开接口旧响应不含提示列表，页面统一消费完整视图。 */
type PublicPageWire = Omit<PublicPageView, "notices"> & {
    notices?: string[] | null
}

function publicPageView(page: PublicPageWire): PublicPageView {
    return { ...page, notices: page.notices ?? [] }
}

export async function fetchProposal(id: string): Promise<ProposalView> {
    return apiGet(`${proposals}/${id}`)
}

export function publicImageUrl(
    token: string,
    coverPath?: string | null,
): string | undefined {
    if (!coverPath) return undefined
    if (coverPath.startsWith("http")) return coverPath
    return `${getApiBaseUrl()}${publicBase}/${encodeURIComponent(token)}/images?ref=${encodeURIComponent(coverPath)}`
}

export async function fetchPublicPage(token: string): Promise<PublicPageView> {
    return publicPageView(
        await apiGet<PublicPageWire>(
            `${publicBase}/${encodeURIComponent(token)}`,
        ),
    )
}

export async function savePublicSession(
    token: string,
    input: {
        idempotencyKey: string
        expectedSessionVersion: number
        choices: Array<{ item_id: string; quantity?: number }>
    },
): Promise<PublicPageView> {
    return publicPageView(
        await apiPost<PublicPageWire>(
            `${publicBase}/${encodeURIComponent(token)}/session`,
            {
                idempotency_key: input.idempotencyKey,
                expected_session_version: input.expectedSessionVersion,
                choices: input.choices,
            },
        ),
    )
}

export async function submitPublicSession(
    token: string,
    input: { idempotencyKey: string; expectedSessionVersion: number },
): Promise<PublicPageView> {
    return publicPageView(
        await apiPost<PublicPageWire>(
            `${publicBase}/${encodeURIComponent(token)}/submit`,
            {
                idempotency_key: input.idempotencyKey,
                expected_session_version: input.expectedSessionVersion,
            },
        ),
    )
}
