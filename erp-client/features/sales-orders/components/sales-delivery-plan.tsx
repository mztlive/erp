import type { SalesDeliveryAllocation } from "../types"

/** 纸质预览中的收货地址分配摘要。 */
export function SalesDeliveryPlan({
    rows,
}: {
    rows?: readonly SalesDeliveryAllocation[]
}) {
    if (!rows?.length) return null
    return (
        <p className="mt-1 text-xs text-muted-foreground">
            收货地址分配 {rows.length} 条
        </p>
    )
}
