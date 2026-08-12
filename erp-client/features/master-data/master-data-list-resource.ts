import type { MasterDataResource } from "@/features/master-data/types"

/** 按资源类型派生列表页行为开关（预览/列/写门禁差异）。 */
export function getMasterDataResourceFlags(resource: MasterDataResource) {
    const isProductResource = resource === "products"
    const isSupplierResource = resource === "suppliers"
    const isBrandResource = resource === "brands"
    const isVoucherCategoryResource = resource === "voucher-categories"
    const isUnitOfMeasureResource = resource === "unit-of-measures"
    const isSellableResource = resource === "sellable-items"
    const isWarehouse = resource === "warehouses"

    return {
        isProductResource,
        isSupplierResource,
        isBrandResource,
        isVoucherCategoryResource,
        isUnitOfMeasureResource,
        isSellableResource,
        isWarehouse,
        /** 商品 / 供应商 / 品牌 / 卡券类目 / 计量单位不走侧边预览 sheet。 */
        skipPreviewSheet:
            isProductResource ||
            isSupplierResource ||
            isBrandResource ||
            isVoucherCategoryResource ||
            isUnitOfMeasureResource,
        /** 即时字典（品牌 / 计量单位等）与供应商不展示生效期间列。 */
        showEffectiveColumn:
            resource !== "brands" &&
            resource !== "unit-of-measures" &&
            !isSupplierResource,
    } as const
}

export type MasterDataResourceFlags = ReturnType<
    typeof getMasterDataResourceFlags
>
