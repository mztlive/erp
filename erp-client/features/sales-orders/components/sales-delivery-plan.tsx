import type { SalesDeliveryAllocation } from "../types"

/** 保存版本中的交付安排，不表示已发货。 */
export function SalesDeliveryPlan({
    rows,
    unit,
}: {
    rows?: readonly SalesDeliveryAllocation[]
    unit?: string | null
}) {
    if (!rows?.length) return null
    return (
        <div className="mt-2 space-y-1 text-xs text-muted-foreground">
            {rows.map((row, index) => (
                <p key={index}>
                    {row.quantity} {unit} → {row.recipient} · {row.phone}
                    <br />
                    {row.address}
                </p>
            ))}
        </div>
    )
}
