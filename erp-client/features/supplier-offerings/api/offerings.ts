import { apiGet, apiPost } from "@/lib/api/client"
import {
    mapWorkItemDto,
    type WorkItemAllowedAction,
    type WorkItemDto,
} from "@/features/work-items/types"

import type {
    CreateSupplierOfferingInput,
    ReviseSupplierOfferingInput,
    SupplierOfferingListQuery,
    SupplierOfferingPage,
    SupplierSupplyExceptionWorkItem,
    UpdateOfferingAvailabilityInput,
} from "@/features/supplier-offerings/types"

const SUPPLY_EXCEPTION_ALLOWED_ACTIONS: ReadonlySet<WorkItemAllowedAction> =
    new Set([
        "VIEW",
        "PROCESS",
        "START_PROCESSING",
        "RELEASE_TO_TEAM",
        "REASSIGN",
    ])

/**
 * 读取 W21 供应停止任务，对未注册路由、对象或终态动作失败关闭。
 */
export async function fetchSupplierSupplyExceptionWorkItem(
    workItemId: string,
): Promise<SupplierSupplyExceptionWorkItem> {
    const normalizedId = workItemId.trim()
    if (!normalizedId) throw new Error("任务标识不能为空")

    const dto = await apiGet<WorkItemDto>(
        `/admin/work-items/${encodeURIComponent(normalizedId)}`,
    )
    const task = mapWorkItemDto(dto)
    const unsupportedActions = task.allowedActions.filter(
        (action) => !SUPPLY_EXCEPTION_ALLOWED_ACTIONS.has(action),
    )

    if (
        task.workItemId !== normalizedId ||
        task.workItemType !== "BUSINESS_EXCEPTION" ||
        task.handlerKey !== "supplier_supply_exception" ||
        task.destinationWorkspaceId !== "W21" ||
        task.status !== "OPEN" ||
        task.businessObjectType !== "SUPPLIER_OFFERING" ||
        task.reasonCode !== "SUPPLIER_STOPPED" ||
        !task.businessObjectId.trim() ||
        !task.subjectVersion.trim() ||
        !task.taskVersion.trim() ||
        unsupportedActions.length > 0
    ) {
        throw new Error("当前任务不符合供应停止核对合同，已阻止处理。")
    }

    return task as SupplierSupplyExceptionWorkItem
}

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
