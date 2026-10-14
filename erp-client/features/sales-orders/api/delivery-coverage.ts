import { apiPost } from "@/lib/api"

/** 销售录入收货地址时的配送提示。 */
export type DeliveryCoverageHint =
    | "NONE"
    | "NO_SUPPLY"
    | "BULK_FREIGHT_EXTRA_ONLY"

export type DeliveryCoverageLine = Readonly<{
    skuId: string
    regionCodes: readonly string[]
}>

type DeliveryCoverageResponseWire = {
    items: { sku_id: string; region_code: string; hint: DeliveryCoverageHint }[]
}

/** 查询商品收货地区的配送提示；以 `sku|地区代码` 为键。 */
export async function fetchDeliveryCoverage(
    lines: readonly DeliveryCoverageLine[],
): Promise<ReadonlyMap<string, DeliveryCoverageHint>> {
    const response = await apiPost<DeliveryCoverageResponseWire>(
        "/admin/sellable-skus/delivery-coverage",
        {
            lines: lines.map((line) => ({
                sku_id: line.skuId,
                region_codes: line.regionCodes,
            })),
        },
    )
    return new Map(
        response.items.map((item) => [
            deliveryCoverageKey(item.sku_id, item.region_code),
            item.hint,
        ]),
    )
}

export function deliveryCoverageKey(skuId: string, regionCode: string) {
    return `${skuId}|${regionCode}`
}
