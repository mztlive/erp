import type {
    MasterDataListItem,
    ProductListingStatus,
} from "@/features/master-data/types"

/** 单次前端受控并发批量上架的上限，避免瞬时请求过多。 */
export const MAX_BATCH_PRODUCT_LISTING_COUNT = 100

export type ProductBatchListingSkipKind =
    | "already_listed"
    | "disabled"
    | "no_skus"
    | "missing_responsibility"

export type ProductBatchListingClassification =
    | Readonly<{
          eligible: true
          kind: "unlisted" | "partial"
          pendingSkuCount: number
      }>
    | Readonly<{
          eligible: false
          kind: ProductBatchListingSkipKind
          reason: string
      }>

export type BatchProductListingSkippedItem = Readonly<{
    product: MasterDataListItem
    kind: ProductBatchListingSkipKind
    reason: string
}>

export type ProductBatchListingPartition = Readonly<{
    actionable: readonly MasterDataListItem[]
    skipped: readonly BatchProductListingSkippedItem[]
    unlistedCount: number
    partialCount: number
    pendingSkuCount: number
    alreadyListedCount: number
    disabledCount: number
    noSkusCount: number
    missingResponsibilityCount: number
}>

export type BatchProductListingProgress = Readonly<{
    completed: number
    total: number
}>

export type BatchProductListingItemResult = Readonly<{
    productId: string
    productNo: string
    productName: string
    status: "succeeded" | "skipped" | "failed"
    reason?: string
    listedSkuCount?: number
    skuCount?: number
}>

export type BatchProductListingResult = Readonly<{
    succeeded: readonly BatchProductListingItemResult[]
    skipped: readonly BatchProductListingItemResult[]
    failed: readonly BatchProductListingItemResult[]
}>

export function formatProductListingStatusLabel(
    status: ProductListingStatus | undefined,
): string {
    if (status === "LISTED") return "已上架"
    if (status === "PARTIALLY_LISTED") return "部分上架"
    return "已下架"
}

/**
 * 按服务端整组上架前置条件对单个商品分类：
 * 停用、无启用 SKU、已全部上架、缺少维护责任的商品将在前端预检阶段归入跳过项。
 */
export function classifyProductForBatchListing(
    item: MasterDataListItem,
): ProductBatchListingClassification {
    if (item.lifecycleStatus !== "ENABLED") {
        return {
            eligible: false,
            kind: "disabled",
            reason: "已停用，停用商品不能上架",
        }
    }
    const totalSkus = item.skuCount ?? 0
    if (totalSkus === 0) {
        return {
            eligible: false,
            kind: "no_skus",
            reason: "没有启用中的 SKU",
        }
    }
    if (item.listingStatus === "LISTED") {
        return {
            eligible: false,
            kind: "already_listed",
            reason: "已全部上架，无需重复操作",
        }
    }
    if (!item.ownerUserId?.trim() || !item.businessOrgUnitId?.trim()) {
        return {
            eligible: false,
            kind: "missing_responsibility",
            reason: "缺少维护人或主属组织，请先交接",
        }
    }
    const listedSkus = item.listedSkuCount ?? 0
    return {
        eligible: true,
        kind:
            item.listingStatus === "PARTIALLY_LISTED" ? "partial" : "unlisted",
        pendingSkuCount: Math.max(0, totalSkus - listedSkus),
    }
}

export function partitionProductsForBatchListing(
    items: readonly MasterDataListItem[],
): ProductBatchListingPartition {
    const actionable: MasterDataListItem[] = []
    const skipped: BatchProductListingSkippedItem[] = []
    let unlistedCount = 0
    let partialCount = 0
    let pendingSkuCount = 0
    let alreadyListedCount = 0
    let disabledCount = 0
    let noSkusCount = 0
    let missingResponsibilityCount = 0

    for (const item of items) {
        const result = classifyProductForBatchListing(item)
        if (result.eligible) {
            actionable.push(item)
            pendingSkuCount += result.pendingSkuCount
            if (result.kind === "partial") partialCount += 1
            else unlistedCount += 1
            continue
        }
        skipped.push({
            product: item,
            kind: result.kind,
            reason: result.reason,
        })
        if (result.kind === "already_listed") alreadyListedCount += 1
        else if (result.kind === "disabled") disabledCount += 1
        else if (result.kind === "no_skus") noSkusCount += 1
        else missingResponsibilityCount += 1
    }

    return {
        actionable,
        skipped,
        unlistedCount,
        partialCount,
        pendingSkuCount,
        alreadyListedCount,
        disabledCount,
        noSkusCount,
        missingResponsibilityCount,
    }
}

export function formatBatchListingSkippedSummary(
    partition: ProductBatchListingPartition,
): string | undefined {
    const parts: string[] = []
    if (partition.alreadyListedCount > 0) {
        parts.push(`已上架 ${partition.alreadyListedCount} 个`)
    }
    if (partition.disabledCount > 0) {
        parts.push(`已停用 ${partition.disabledCount} 个`)
    }
    if (partition.noSkusCount > 0) {
        parts.push(`无启用 SKU ${partition.noSkusCount} 个`)
    }
    if (partition.missingResponsibilityCount > 0) {
        parts.push(`缺维护责任 ${partition.missingResponsibilityCount} 个`)
    }
    return parts.length > 0 ? parts.join(" · ") : undefined
}
