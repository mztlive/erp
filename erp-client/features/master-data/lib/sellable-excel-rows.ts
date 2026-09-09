import type { MasterDataListItem } from "@/features/master-data/types"

export const SELLABLE_EXCEL_HEADERS = [
    "主图",
    "商品名称",
    "规格",
    "SKU 编号",
    "SPU 编号",
    "销售价（含税）",
    "市场参考价",
    "可供区域",
    "供应保障",
    "商品类型",
    "基础单位",
    "条码",
] as const

export type SellableExcelImageExtension = "jpeg" | "png" | "gif"

export type SellableExcelRow = Readonly<{
    name: string
    specification: string
    skuNo: string
    productNo: string
    salesPrice: string
    marketPrice: string
    supplyRegions: string
    supplierLabel: string
    productKind: string
    baseUnit: string
    barcode: string
    imageAssetId?: string
}>

export function sellableSupplierLabel(supplierCount: number): string {
    return supplierCount <= 1 ? "单一供应商" : `${supplierCount} 家可供`
}

export function toSellableExcelRow(row: MasterDataListItem): SellableExcelRow {
    const item = row.sellableItem
    const specification =
        item && item.specificationLabel !== "无规格"
            ? item.specificationLabel
            : "—"
    return {
        name: row.name,
        specification,
        skuNo: row.stableNo,
        productNo: item?.productNo ?? "—",
        salesPrice: item?.salesVisiblePriceGross ?? "—",
        marketPrice: item?.marketPrice ?? "—",
        supplyRegions:
            item && item.supplyRegions.length > 0
                ? item.supplyRegions.join("、")
                : "未标注",
        supplierLabel: sellableSupplierLabel(item?.supplierCount ?? 0),
        productKind: item?.productKindLabel ?? "—",
        baseUnit: item?.baseUnit ?? "—",
        barcode: item?.barcode ?? "—",
        imageAssetId: item?.mainImageAssetId,
    }
}

export function selectSellableRows(
    rows: readonly MasterDataListItem[],
    selectedIds: ReadonlySet<string>,
): MasterDataListItem[] {
    if (selectedIds.size === 0) return []
    return rows.filter((row) => selectedIds.has(row.stableId))
}

export function excelImageExtension(
    contentType: string,
    fileName?: string,
): SellableExcelImageExtension | undefined {
    const mime = contentType.split(";")[0]?.trim().toLowerCase() ?? ""
    if (mime === "image/jpeg" || mime === "image/jpg") return "jpeg"
    if (mime === "image/png") return "png"
    if (mime === "image/gif") return "gif"
    const lowerName = fileName?.trim().toLowerCase() ?? ""
    if (lowerName.endsWith(".jpg") || lowerName.endsWith(".jpeg")) return "jpeg"
    if (lowerName.endsWith(".png")) return "png"
    if (lowerName.endsWith(".gif")) return "gif"
    return undefined
}

export async function mapWithConcurrency<T, R>(
    items: readonly T[],
    limit: number,
    mapper: (item: T, index: number) => Promise<R>,
): Promise<R[]> {
    if (items.length === 0) return []
    const results: R[] = Array.from({ length: items.length })
    let next = 0
    const workerCount = Math.max(1, Math.min(limit, items.length))
    await Promise.all(
        Array.from({ length: workerCount }, async () => {
            while (next < items.length) {
                const index = next
                next += 1
                results[index] = await mapper(items[index]!, index)
            }
        }),
    )
    return results
}

export function downloadBlob(blob: Blob, fileName: string) {
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = fileName
    anchor.click()
    URL.revokeObjectURL(url)
}
