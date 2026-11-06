"use client"

import type { ReactNode } from "react"
import { MoneyValue } from "@/components/business"
import { compareAmounts, subtractAmounts } from "../lib/allocation-math"

/** 左右分栏工作区底部金额核对栏：集中展示到账金额、本次核销与未核销余额，保留全部操作。 */
export function ReceiptSessionFooter({
    amount,
    allocated,
    unallocated,
    allocationCount,
    submitted,
    submitHint,
    actions,
}: {
    amount: string
    allocated: string
    unallocated: string
    allocationCount?: number
    submitted: boolean
    submitHint?: string
    actions: ReactNode
}) {
    const overAllocated = compareAmounts(allocated, amount) > 0
    return (
        <aside
            aria-label="本次回款"
            className="sticky bottom-0 z-20 mt-auto border-t border-border bg-card/95 py-4 backdrop-blur-xs"
        >
            <div className="flex flex-wrap items-center justify-between gap-x-8 gap-y-4">
                <div className="flex flex-wrap items-center gap-x-8 gap-y-3">
                    <div className="space-y-1">
                        <p className="text-xs text-muted-foreground">
                            到账金额
                        </p>
                        <MoneyValue
                            value={amount || "0"}
                            className="text-2xl font-semibold"
                        />
                    </div>
                    <div className="space-y-1 border-l pl-8">
                        <p className="text-xs text-muted-foreground">
                            本次核销
                        </p>
                        <MoneyValue
                            value={allocated}
                            className="text-2xl font-semibold"
                        />
                    </div>
                    <div className="space-y-1 border-l pl-8">
                        <p className="text-xs text-muted-foreground">
                            {overAllocated ? "超出回款金额" : "未核销金额"}
                        </p>
                        <MoneyValue
                            className={
                                overAllocated
                                    ? "text-lg font-semibold text-destructive"
                                    : "text-lg font-medium"
                            }
                            value={
                                overAllocated
                                    ? subtractAmounts(allocated, amount)
                                    : unallocated
                            }
                        />
                    </div>
                </div>
                <div className="flex flex-wrap items-center justify-end gap-3">
                    {actions}
                </div>
            </div>
            <p
                role={submitHint ? "status" : undefined}
                className={
                    submitHint || overAllocated
                        ? "mt-3 text-xs text-destructive"
                        : "mt-3 text-xs text-muted-foreground"
                }
            >
                {submitHint ??
                    (submitted
                        ? "已提交，核销进度以审批结果为准。"
                        : allocationCount !== undefined
                          ? `本次已选 ${allocationCount} 笔应收 · 审批通过后计入已收并完成核销。`
                          : "审批通过后计入已收并完成核销。")}
            </p>
        </aside>
    )
}
