"use client"

import { SparklesIcon } from "lucide-react"
import { sourcingRecommendationReason } from "../lib/sourcing/recommendation-reason"
import type {
    SourcingProductLine,
    SourcingSalesOrder,
} from "../lib/sourcing/types"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"

/** 推荐依据紧随供应商选择器，切换到其他方案后不再标记为系统推荐。 */
export function SourcingRecommendationHint({
    form,
    order,
    product,
    index,
}: {
    form: PurchaseOrderCreateFormApi
    order: SourcingSalesOrder
    product: SourcingProductLine
    index: number
}) {
    return (
        <form.Subscribe selector={(state) => state.values.lines}>
            {(lines) => {
                const line = lines[index]
                const reason = line
                    ? sourcingRecommendationReason(order, product, line, lines)
                    : undefined
                if (!reason) return null
                return (
                    <div className="flex items-start gap-2 rounded-md bg-muted/40 px-3 py-2.5 text-xs">
                        <SparklesIcon
                            aria-hidden="true"
                            className="mt-0.5 size-3.5 shrink-0 text-muted-foreground"
                        />
                        <div className="min-w-0 space-y-1">
                            <p className="font-medium">
                                系统推荐 · {reason.title}
                            </p>
                            <p className="leading-relaxed text-muted-foreground">
                                {reason.detail}
                            </p>
                        </div>
                    </div>
                )
            }}
        </form.Subscribe>
    )
}
