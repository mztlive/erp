/** 公司商品池按配送地区筛选：范围解析与条件文案。 */

import {
    deliveryExtentLabel,
    type DeliveryExtent,
} from "@/lib/delivery-coverage"

export type SellableDeliveryExtent = DeliveryExtent

export function parseSellableDeliveryExtent(
    value: string | null | undefined,
): SellableDeliveryExtent {
    return value === "PARTIAL" ? "PARTIAL" : "FULL"
}

/** 例：「广东省深圳市 · 全部可送 · 含需另付运费」。 */
export function sellableDeliveryFilterText(
    regionName: string,
    extent: SellableDeliveryExtent,
    includeFreightExtra: boolean,
): string {
    return [
        regionName,
        deliveryExtentLabel(extent),
        includeFreightExtra ? "含需另付运费" : "",
    ]
        .filter(Boolean)
        .join(" · ")
}
