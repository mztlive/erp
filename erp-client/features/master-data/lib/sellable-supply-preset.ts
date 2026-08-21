import type {
    MasterDataListItem,
    SellableSupplyPreset,
} from "@/features/master-data/types"

export const SELLABLE_SUPPLY_PRESET_LABELS: Record<
    SellableSupplyPreset,
    string
> = {
    "single-supplier": "单一供应商",
    nationwide: "全国可供",
}

/**
 * 公司商品池快捷视图只对已通过销售资格校验的结果做派生过滤，
 * 不得把它当作新的服务端资格规则或独立商品池状态。
 */
export function matchesSellableSupplyPreset(
    row: MasterDataListItem,
    preset?: SellableSupplyPreset,
): boolean {
    if (!preset) return true
    const sellable = row.sellableItem
    if (!sellable) return false
    if (preset === "single-supplier") return sellable.supplierCount === 1
    return sellable.supplyRegions.includes("全国")
}

export function filterBySellableSupplyPreset(
    rows: readonly MasterDataListItem[],
    preset?: SellableSupplyPreset,
): MasterDataListItem[] {
    return rows.filter((row) => matchesSellableSupplyPreset(row, preset))
}
