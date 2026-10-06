import { apiGet, apiGetBlob, apiPost, apiPut } from "@/lib/api"
import { mapPortalApplication } from "@/features/supplier-portal/api"
import type {
    PortalDictionary,
    PortalCategoryMappingSuggestion,
    PortalPage,
    PortalProfile,
} from "@/features/supplier-portal/types"
export type PortalAccount = PortalProfile & {
    active: boolean
    account_version: number
    binding_version: number
}
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
export type PortalDuplicate = {
    product_id: string
    version: number
    revision_id: string
    name: string
    product_kind: string
    skus: {
        sku_id: string
        version: number
        revision_id: string
        sku_no?: string
        name?: string
        specification?: string
        unit_name?: string
    }[]
}
const base = "/admin/supplier-portal"
const encoded = (id: string) => encodeURIComponent(id)
function asPage<T>(result: PortalPage<T> | T[]): PortalPage<T> {
    return Array.isArray(result)
        ? { items: result, total: result.length, page: 1, page_size: 100 }
        : result
}
export const listPortalAccounts = async (query: Record<string, unknown>) =>
    asPage(
        await apiGet<PortalPage<PortalAccount> | PortalAccount[]>(
            `${base}/accounts`,
            query,
        ),
    )
export const createPortalAccount = (input: Record<string, unknown>) =>
    apiPost<PortalAccount>(`${base}/accounts`, input)
export const updatePortalAccount = (
    id: string,
    input: Record<string, unknown>,
) => apiPut<PortalAccount>(`${base}/accounts/${encoded(id)}`, input)
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
export const getPortalApplication = async (id: string) =>
    mapPortalApplication(
        await apiGet<Record<string, unknown>>(
            `${base}/applications/${encoded(id)}`,
        ),
    )
export const reviewPortalApplication = (
    id: string,
    input: Record<string, unknown>,
) => apiPost<unknown>(`${base}/applications/${encoded(id)}/review`, input)
export const portalReviewDictionaries = (kind: string) =>
    apiGet<PortalDictionary[]>(`${base}/dictionaries/${kind}`)
export const portalDuplicates = (applicationId: string, q: string) =>
    apiGet<{
        duplicates: PortalDuplicate[]
        category_mapping_suggestion?: PortalCategoryMappingSuggestion | null
        existing_offerings: {
            row_id: string
            offering_id: string
            expected_offering_version: number
            expected_revision_no: number
            sku_id: string
            supplier_sku_code: string
            name: string
        }[]
    }>(`${base}/applications/${encoded(applicationId)}/duplicates`, { q })

export const portalReviewFile = (applicationId: string, fileId: string) =>
    apiGetBlob(
        `${base}/applications/${encoded(applicationId)}/files/${encoded(fileId)}/download`,
    )

export type PortalOfferingImpactTask = {
    id: string
    object_type: string
    object_id: string
    status: string
    version: number
    owner_user_id: string | null
}
export type PortalPurchaseImpact = {
    purchase_order_id: string
    purchase_no: string
    status: string
    purchase_order_version: number
    owner_name: string | null
    owner_user_id: string
    lines: {
        line_id: string
        offering_revision_id: string
        selected_offering_version: number
        selected_revision_version: number
        selected_availability_version: number
    }[]
    tasks: PortalOfferingImpactTask[]
}
export type PortalOfferingImpact = {
    offering_id: string
    warning: {
        code: string
        message: string
        offering_id: string
        status: string | null
        availability_version: number | null
    } | null
    items: PortalPurchaseImpact[]
    total: number
    page: number
    page_size: number
    association_notice: string
}

/** 读取实际选源的采购影响；调用方须先证明当前页面的内部读取资格。 */
export function getPortalOfferingImpacts(
    offeringId: string,
    query: { page: number; page_size: number },
): Promise<PortalOfferingImpact> {
    return apiGet<PortalOfferingImpact>(
        `/admin/supplier-portal/offerings/${encodeURIComponent(offeringId)}/impacts`,
        query,
    )
}
