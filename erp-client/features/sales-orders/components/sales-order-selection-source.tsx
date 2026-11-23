import Link from "next/link"
import { ArrowUpRightIcon, SparklesIcon } from "lucide-react"

import type { SalesSelectionSource } from "@/features/sales-orders/api/selection-basis"

export function SalesOrderSelectionSource({
    source,
    compact = false,
}: {
    source: SalesSelectionSource
    compact?: boolean
}) {
    if (compact) {
        return (
            <div className="text-body-compact text-muted-foreground">
                <Link
                    id="sales-order-selection-source"
                    className="inline-flex items-center gap-1 font-medium text-primary underline-offset-4 hover:underline"
                    href={`/sales/selection/proposals/${encodeURIComponent(source.proposal_id)}`}
                >
                    来源销售方案
                    <ArrowUpRightIcon className="size-3.5" aria-hidden="true" />
                </Link>
            </div>
        )
    }

    return (
        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1 rounded-lg border border-info-border/75 bg-info-soft/70 px-3.5 py-2.5 text-info-soft-foreground">
            <SparklesIcon className="size-4 shrink-0" aria-hidden="true" />
            <span>来源销售方案</span>
            <Link
                id="sales-order-selection-source"
                className="num inline-flex items-center gap-0.5 font-medium underline underline-offset-4"
                href={`/sales/selection/proposals/${encodeURIComponent(source.proposal_id)}`}
            >
                {source.proposal_no}
                <ArrowUpRightIcon className="size-3.5" aria-hidden="true" />
            </Link>
        </div>
    )
}
