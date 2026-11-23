"use client"

import { SparklesIcon } from "lucide-react"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { sourcingRecommendationReason } from "../lib/sourcing/recommendation-reason"
import type {
    SourcingProductLine,
    SourcingSalesOrder,
} from "../lib/sourcing/types"
import type { PurchaseOrderCreateFormApi } from "../lib/purchase-order-create-form-types"

/** 推荐标记紧随方案摘要，依据放在提示里；切换到其他方案后不再标记为系统推荐。 */
export function SourcingRecommendationHint({
    id,
    form,
    order,
    product,
    index,
}: {
    id: string
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
                const text = `${reason.title}。${reason.detail}`
                return (
                    <Tooltip>
                        <TooltipTrigger
                            render={
                                <button
                                    id={id}
                                    type="button"
                                    aria-label={`系统推荐：${text}`}
                                    className="inline-flex h-5 cursor-pointer items-center gap-1 rounded-sm bg-info-soft px-1.5 text-xs font-medium text-info-soft-foreground transition-colors outline-none hover:bg-info-soft/80 focus-visible:ring-2 focus-visible:ring-ring"
                                />
                            }
                        >
                            <SparklesIcon
                                aria-hidden="true"
                                className="size-3 shrink-0"
                            />
                            系统推荐
                        </TooltipTrigger>
                        <TooltipContent className="block">
                            {text}
                        </TooltipContent>
                    </Tooltip>
                )
            }}
        </form.Subscribe>
    )
}
