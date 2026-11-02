import { apiGet, apiPost } from "@/lib/api"
import { mapPortalApplication } from "./api"
import type { PortalPage } from "./types"
export type PortalGrant = {
    id: string
    supplier_id: string
    sku_id: string
    supplier_name?: string
    sku_no?: string
    name?: string
    specification?: string
    active: boolean
    version: number
}
const base = "/admin/supplier-portal"
function asPage<T>(result: PortalPage<T> | T[]): PortalPage<T> {
    return Array.isArray(result)
        ? { items: result, total: result.length, page: 1, page_size: 100 }
        : result
}
export const listPortalGrants = async (query: Record<string, unknown>) =>
    asPage(
        await apiGet<PortalPage<PortalGrant> | PortalGrant[]>(
            `${base}/catalog-grants`,
            query,
        ),
    )
export const savePortalGrant = (input: Record<string, unknown>) =>
    apiPost<PortalGrant>(`${base}/catalog-grants`, input)
export const listPortalApplications = async (
    query: Record<string, unknown>,
) => {
    const page = asPage(
        await apiGet<
            PortalPage<Record<string, unknown>> | Record<string, unknown>[]
        >(`${base}/applications`, query),
    )
    return { ...page, items: page.items.map(mapPortalApplication) }
}
