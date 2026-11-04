import Link from "next/link"
import { ArrowUpRightIcon } from "lucide-react"
import type { SalesSelectionSource } from "@/features/sales-orders/api/selection-basis"

export function SalesOrderSelectionSource({
    source,
    compact = false,
}: {
    source: SalesSelectionSource
    compact?: boolean
}) {
    return (
        <div
            className={
                compact
                    ? "text-sm text-muted-foreground"
                    : "flex flex-wrap items-baseline gap-x-3 gap-y-1 rounded-lg border border-info-border bg-info-soft px-4 py-3 text-sm text-info-soft-foreground"
            }
        >
            {!compact ? <span>来源销售方案</span> : null}
            <Link
                id="sales-order-selection-source"
                className="inline-flex items-center gap-1 font-medium underline underline-offset-4"
                href={`/sales/selection/proposals/${encodeURIComponent(source.proposal_id)}`}
            >
                {compact ? "来源销售方案" : source.proposal_no}
                {compact ? (
                    <ArrowUpRightIcon className="size-3.5" aria-hidden="true" />
                ) : null}
            </Link>
        </div>
    )
}
