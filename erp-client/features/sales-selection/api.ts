import { apiGet, apiPost, getApiBaseUrl } from "@/lib/api"
import type { ProposalView, PublicPageView } from "./types"
const proposals = "/admin/sales-selection-proposals"
const publicBase = "/public/selection"

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
    return apiGet(`${publicBase}/${encodeURIComponent(token)}`)
}

export async function savePublicSession(
    token: string,
    input: {
        idempotencyKey: string
        expectedSessionVersion: number
        choices: Array<{ item_id: string; quantity?: number }>
    },
): Promise<PublicPageView> {
    return apiPost(`${publicBase}/${encodeURIComponent(token)}/session`, {
        idempotency_key: input.idempotencyKey,
        expected_session_version: input.expectedSessionVersion,
        choices: input.choices,
    })
}

export async function submitPublicSession(
    token: string,
    input: { idempotencyKey: string; expectedSessionVersion: number },
): Promise<PublicPageView> {
    return apiPost(`${publicBase}/${encodeURIComponent(token)}/submit`, {
        idempotency_key: input.idempotencyKey,
        expected_session_version: input.expectedSessionVersion,
    })
}
