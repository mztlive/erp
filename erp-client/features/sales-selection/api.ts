import { apiGet, apiPost, getApiBaseUrl } from "@/lib/api"
import type {
    BookletListItem,
    BookletView,
    CreateTierInput,
    PoolFilterSnapshot,
    PoolSourceKind,
    ProposalView,
    PublicPageView,
    SelectionForm,
    SubmitMode,
} from "./types"

const admin = "/admin/sales-selection-books"
const proposals = "/admin/sales-selection-proposals"
const publicBase = "/public/selection"

export async function fetchBooklets(params: {
    customerId?: string
    form?: SelectionForm
    status?: string
    submitMode?: SubmitMode
    page?: number
}): Promise<{
    items: BookletListItem[]
    total: number
    page: number
    page_size: number
}> {
    const query = new URLSearchParams()
    if (params.customerId) query.set("customer_id", params.customerId)
    if (params.form) query.set("form", params.form)
    if (params.status) query.set("status", params.status)
    if (params.submitMode) query.set("submit_mode", params.submitMode)
    query.set("page", String(params.page ?? 1))
    return apiGet(`${admin}?${query.toString()}`)
}

export async function fetchBooklet(id: string): Promise<BookletView> {
    return apiGet(`${admin}/${id}`)
}

export async function createBooklet(input: {
    idempotencyKey: string
    customerId: string
    form: SelectionForm
    submitMode: SubmitMode
    poolSourceKind: PoolSourceKind
    poolFilter?: PoolFilterSnapshot
    skuIds?: string[]
    tiers: CreateTierInput[]
}): Promise<BookletView> {
    return apiPost(admin, {
        idempotency_key: input.idempotencyKey,
        customer_id: input.customerId,
        form: input.form,
        submit_mode: input.submitMode,
        pool_source_kind: input.poolSourceKind,
        pool_filter: input.poolFilter,
        sku_ids: input.skuIds,
        tiers: input.tiers,
    })
}

export async function prepareBooklet(
    id: string,
    input: {
        idempotencyKey: string
        expectedVersion: number
        kind:
            | "FIRST_PREPARE"
            | "REGENERATED_TIERS"
            | "REGENERATED_ALL"
            | "RE_PREPARE"
        tierIds?: string[]
    },
): Promise<BookletView> {
    return apiPost(`${admin}/${id}/prepare`, {
        idempotency_key: input.idempotencyKey,
        expected_version: input.expectedVersion,
        kind: input.kind,
        tier_ids: input.tierIds ?? [],
    })
}

export async function deleteDisplayItem(
    bookletId: string,
    itemId: string,
    expectedVersion: number,
): Promise<BookletView> {
    return apiPost(`${admin}/${bookletId}/display-items/${itemId}/delete`, {
        expected_version: expectedVersion,
    })
}

export async function publishBooklet(
    id: string,
    input: {
        idempotencyKey: string
        expectedVersion: number
        batchId?: string
    },
): Promise<BookletView> {
    return apiPost(`${admin}/${id}/publish`, {
        idempotency_key: input.idempotencyKey,
        expected_version: input.expectedVersion,
        batch_id: input.batchId ?? undefined,
    })
}

export async function copyBookletLink(id: string): Promise<BookletView> {
    return apiPost(`${admin}/${id}/copy-link`)
}

export async function rotateBookletLink(
    id: string,
    input: { idempotencyKey: string; expectedVersion: number },
): Promise<BookletView> {
    return apiPost(`${admin}/${id}/rotate-link`, {
        idempotency_key: input.idempotencyKey,
        expected_version: input.expectedVersion,
    })
}

export async function closeBooklet(
    id: string,
    input: { idempotencyKey: string; expectedVersion: number },
): Promise<BookletView> {
    return apiPost(`${admin}/${id}/close`, {
        idempotency_key: input.idempotencyKey,
        expected_version: input.expectedVersion,
    })
}

export async function revokeBooklet(
    id: string,
    input: { idempotencyKey: string; expectedVersion: number },
): Promise<BookletView> {
    return apiPost(`${admin}/${id}/revoke`, {
        idempotency_key: input.idempotencyKey,
        expected_version: input.expectedVersion,
    })
}

export async function voidBooklet(
    id: string,
    input: { idempotencyKey: string; expectedVersion: number },
): Promise<BookletView> {
    return apiPost(`${admin}/${id}/void`, {
        idempotency_key: input.idempotencyKey,
        expected_version: input.expectedVersion,
    })
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
