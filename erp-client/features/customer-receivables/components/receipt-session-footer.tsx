"use client"

import type { ReactNode } from "react"
import { MoneyValue } from "@/components/business"
import { compareAmounts, subtractAmounts } from "../lib/allocation-math"

/** 左右分栏工作区底部紧凑金额核对栏：单行展示总额、本次核销、余额与操作。 */
export function ReceiptSessionFooter({
    amount,
    allocated,
    unallocated,
    allocationCount,
    submitted,
    submitHint,
    actions,
    ariaLabel = "本次回款",
    amountLabel = "到账金额",
    allocatedLabel = "本次核销",
    unallocatedLabel = "未核销",
    overAllocatedLabel = "超出金额",
    defaultHint,
}: {
    amount: string
    allocated: string
    unallocated: string
    allocationCount?: number
    submitted: boolean
    submitHint?: string
    actions: ReactNode
    ariaLabel?: string
    amountLabel?: string
    allocatedLabel?: string
    unallocatedLabel?: string
    overAllocatedLabel?: string
    defaultHint?: string
}) {
    const overAllocated = compareAmounts(allocated, amount) > 0
    const hintText =
        submitHint ??
        (overAllocated
            ? `已超出${amountLabel}，请调整核销金额`
            : submitted
              ? "已提交，核销进度以审批结果为准"
              : (defaultHint ??
                (allocationCount !== undefined
                    ? `已选 ${allocationCount} 笔应收`
                    : undefined)))

    return (
        <aside
            aria-label={ariaLabel}
            className="sticky bottom-0 z-20 mt-auto border-t border-border/70 bg-card/95 px-(--scaffold-px) py-3 backdrop-blur-xs"
        >
            <div className="flex flex-wrap items-center justify-between gap-x-6 gap-y-2">
                <div className="flex flex-wrap items-center gap-x-6 gap-y-1.5">
                    <div className="flex items-baseline gap-2">
                        <span className="text-xs text-muted-foreground">
                            {amountLabel}
                        </span>
                        <MoneyValue
                            value={amount || "0"}
                            className="text-value-compact font-semibold tracking-tight"
                        />
                    </div>
                    <div className="flex items-baseline gap-2 border-l border-border/70 pl-6">
                        <span className="text-xs text-muted-foreground">
                            {allocatedLabel}
                        </span>
                        <MoneyValue
                            value={allocated}
                            className="text-value-compact font-semibold tracking-tight"
                        />
                    </div>
                    <div className="flex items-baseline gap-2 border-l border-border/70 pl-6">
                        <span className="text-xs text-muted-foreground">
                            {overAllocated
                                ? overAllocatedLabel
                                : unallocatedLabel}
                        </span>
                        <MoneyValue
                            className={
                                overAllocated
                                    ? "text-value-compact font-semibold text-destructive tracking-tight"
                                    : "text-value-compact font-semibold tracking-tight"
                            }
                            value={
                                overAllocated
                                    ? subtractAmounts(allocated, amount)
                                    : unallocated
                            }
                        />
                    </div>
                    {hintText ? (
                        <span
                            role={
                                submitHint || overAllocated
                                    ? "status"
                                    : undefined
                            }
                            className={
                                submitHint || overAllocated
                                    ? "border-l border-border/70 pl-6 text-xs text-destructive"
                                    : "border-l border-border/70 pl-6 text-xs text-muted-foreground"
                            }
                        >
                            {hintText}
                        </span>
                    ) : null}
                </div>
                <div className="flex flex-wrap items-center justify-end gap-2.5">
                    {actions}
                </div>
            </div>
        </aside>
    )
}
