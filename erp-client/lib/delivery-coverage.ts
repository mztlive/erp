/** 按配送地区筛选的范围：所选地区全部可送，或只有部分可送。 */
export type DeliveryExtent = "FULL" | "PARTIAL"

export const DELIVERY_EXTENT_OPTIONS: ReadonlyArray<{
    value: DeliveryExtent
    label: string
}> = [
    { value: "FULL", label: "全部可送" },
    { value: "PARTIAL", label: "部分可送" },
]

export function deliveryExtentLabel(
    extent: DeliveryExtent | undefined,
): string {
    return extent === "PARTIAL" ? "部分可送" : "全部可送"
}
