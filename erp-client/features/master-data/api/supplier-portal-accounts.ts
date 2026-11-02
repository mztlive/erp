import { apiGet, apiPost, apiPut } from "@/lib/api"
import type {
    PortalPage,
    PortalProfile,
} from "@/features/supplier-portal/types"

export type PortalAccount = PortalProfile & {
    active: boolean
    account_version: number
    binding_version: number
}

const base = "/admin/supplier-portal/accounts"

export const listPortalAccounts = async (
    query: Record<string, unknown>,
): Promise<PortalPage<PortalAccount>> => {
    const result = await apiGet<PortalPage<PortalAccount> | PortalAccount[]>(
        base,
        query,
    )
    return Array.isArray(result)
        ? { items: result, total: result.length, page: 1, page_size: 100 }
        : result
}

export const createPortalAccount = (input: Record<string, unknown>) =>
    apiPost<PortalAccount>(base, input)

export const updatePortalAccount = (
    id: string,
    input: Record<string, unknown>,
) => apiPut<PortalAccount>(`${base}/${encodeURIComponent(id)}`, input)
