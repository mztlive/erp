import Link from "next/link"
import type { SalesSelectionSource } from "@/features/sales-orders/api/selection-basis"

export function SalesOrderSelectionSource({
    source,
}: {
    source: SalesSelectionSource
}) {
    return (
        <div className="flex flex-wrap items-baseline gap-x-3 gap-y-1 rounded-lg border border-info-border bg-info-soft px-4 py-3 text-sm text-info-soft-foreground">
            <span>来源销售方案</span>
            <Link
                id="sales-order-selection-source"
                className="font-medium underline underline-offset-4"
                href={`/sales/selection/proposals/${encodeURIComponent(source.proposal_id)}`}
            >
                {source.proposal_no}
            </Link>
        </div>
    )
}
