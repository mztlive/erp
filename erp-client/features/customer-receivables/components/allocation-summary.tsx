import { MoneyValue } from "@/components/business"

/** 回款与发票列表共用的分配摘要：未分配在上，净已分配在下。 */
export function allocationSummaryCell(row: {
    unallocatedAmount: string
    allocatedTotal: string
}) {
    return (
        <div className="flex flex-col items-end gap-1">
            <div>
                <span className="mr-2 text-xs text-muted-foreground">
                    未分配
                </span>
                <MoneyValue display="exact" value={row.unallocatedAmount} />
            </div>
            <div>
                <span className="mr-2 text-xs text-muted-foreground">
                    净已分配
                </span>
                <MoneyValue display="exact" value={row.allocatedTotal} />
            </div>
        </div>
    )
}
