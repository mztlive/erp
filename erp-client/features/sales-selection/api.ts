import { apiGet, apiPost, getApiBaseUrl } from "@/lib/api"
import type {
    ProposalView,
    PublicPageView,
    PublicUnlockView,
    SelectionRecipient,
} from "./types"
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
    accessToken?: string,
): string | undefined {
    if (!coverPath) return undefined
    return `${getApiBaseUrl()}${publicBase}/${encodeURIComponent(token)}/images?ref=${encodeURIComponent(coverPath)}${accessToken ? `&access_token=${encodeURIComponent(accessToken)}` : ""}`
}

export async function fetchPublicPage(
    token: string,
    accessToken?: string,
): Promise<PublicPageView> {
    return publicPageView(
        await apiGet<PublicPageWire>(
            `${publicBase}/${encodeURIComponent(token)}`,
            undefined,
            { session: "none", headers: selectionHeaders(accessToken) },
        ),
    )
}

export async function savePublicSession(
    token: string,
    accessToken: string,
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
            { session: "none", headers: selectionHeaders(accessToken) },
        ),
    )
}

export async function submitPublicSession(
    token: string,
    accessToken: string,
    input: {
        idempotencyKey: string
        expectedSessionVersion: number
        recipient?: SelectionRecipient
    },
): Promise<PublicPageView> {
    return publicPageView(
        await apiPost<PublicPageWire>(
            `${publicBase}/${encodeURIComponent(token)}/submit`,
            {
                idempotency_key: input.idempotencyKey,
                expected_session_version: input.expectedSessionVersion,
                recipient: input.recipient,
            },
            { session: "none", headers: selectionHeaders(accessToken) },
        ),
    )
}

const selectionHeaders = (accessToken?: string): Record<string, string> =>
    accessToken ? { "X-Selection-Access": accessToken } : {}

export async function unlockPublicSelection(
    token: string,
    input: { password: string; voucher_code?: string },
): Promise<PublicUnlockView> {
    const result = await apiPost<PublicUnlockView>(
        `${publicBase}/${encodeURIComponent(token)}/unlock`,
        input,
        { session: "none" },
    )
    return { ...result, page: publicPageView(result.page) }
}
