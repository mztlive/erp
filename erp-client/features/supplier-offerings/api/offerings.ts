import { apiGet, apiPost } from "@/lib/api/client"

import type {
    CreateSupplierOfferingInput,
    ReviseSupplierOfferingInput,
    SupplierOfferingListQuery,
    SupplierOfferingPage,
    UpdateOfferingAvailabilityInput,
} from "@/features/supplier-offerings/types"

export function fetchSupplierOfferings(
    query: SupplierOfferingListQuery,
): Promise<SupplierOfferingPage> {
    return apiGet<SupplierOfferingPage>("/admin/supplier-offerings", {
        q: query.q?.trim() || undefined,
        sku_id: query.skuId || undefined,
        sku_no: query.skuNo?.trim() || undefined,
        product_no: query.productNo?.trim() || undefined,
        supplier_id: query.supplierId || undefined,
        status: query.status || undefined,
        source_type: query.sourceType || undefined,
        availability_status: query.availabilityStatus || undefined,
        page: query.page ?? 1,
        page_size: query.pageSize ?? 50,
        sort_by: "created_at",
        sort_dir: "desc",
    })
}

/** 按多个稳定 SKU 读取全部供给关系，供商品列表判断与明细弹窗复用。 */
export async function fetchSupplierOfferingsForSkus(
    skuIds: readonly string[],
): Promise<SupplierOfferingPage["items"]> {
    const uniqueIds = [...new Set(skuIds.filter(Boolean))]
    const pages = await Promise.all(
        uniqueIds.map(async (skuId) => {
            const items: SupplierOfferingPage["items"][number][] = []
            let page = 1
            let total = Number.POSITIVE_INFINITY
            while (items.length < total) {
                const result = await fetchSupplierOfferings({
                    skuId,
                    page,
                    pageSize: 100,
                })
                items.push(...result.items)
                total = result.total
                if (result.items.length === 0) break
                page += 1
            }
            return items
        }),
    )
    return pages.flat()
}

export function createSupplierOffering(input: CreateSupplierOfferingInput) {
    return apiPost<{
        offering_id: string
        revision_id: string
        availability_id: string
        revision_no: number
        status: string
    }>("/admin/supplier-offerings", input)
}

export function reviseSupplierOffering(input: ReviseSupplierOfferingInput) {
    const { offeringId, ...body } = input
    return apiPost<{
        offering_id: string
        revision_id: string
        revision_no: number
        status: string
        version: number
    }>(
        `/admin/supplier-offerings/${encodeURIComponent(offeringId)}/revisions`,
        body,
    )
}

export function updateSupplierOfferingAvailability(
    input: UpdateOfferingAvailabilityInput,
) {
    const { offeringId, ...body } = input
    return apiPost<{
        offering_id: string
        availability_status: string
        availability_version: number
        source_updated_at: number
    }>(
        `/admin/supplier-offerings/${encodeURIComponent(offeringId)}/availability`,
        body,
    )
}
