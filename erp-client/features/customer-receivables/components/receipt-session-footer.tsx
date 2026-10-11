"use client"

import type { ReactNode } from "react"

import { MoneyValue } from "@/components/business"
import {
    compareAmounts,
    subtractAmounts,
} from "@/features/customer-receivables/lib/allocation-math"

/** 回款核对侧栏；空间不足时顺排在应收表下，保留全部金额与操作。 */
export function ReceiptSessionFooter({
    amount,
    allocated,
    unallocated,
    allocationCount,
    existing,
    submitted,
    submitHint,
    actions,
}: {
    amount: string
    allocated: string
    unallocated: string
    allocationCount: number
    existing: boolean
    submitted: boolean
    submitHint?: string
    actions: ReactNode
}) {
    const overAllocated = compareAmounts(allocated, amount) > 0
    return (
        <aside
            aria-labelledby="customer-receivables-session-summary-title"
            className="flex min-w-0 flex-col gap-6 border-t border-grid bg-muted/20 p-6 @5xl:sticky @5xl:top-6 @5xl:min-h-[calc(100svh-3rem)] @5xl:self-start @5xl:border-t-0 @5xl:border-l"
        >
            <div className="space-y-5">
                <h2
                    id="customer-receivables-session-summary-title"
                    className="text-lg font-semibold"
                >
                    本次回款
                </h2>
                <div className="space-y-2">
                    <p className="text-sm text-muted-foreground">
                        {existing ? "可核销余额" : "到账金额"}
                    </p>
                    <div className="min-w-0 break-all">
                        <MoneyValue value={amount || "0"} size="hero" />
                    </div>
                </div>
            </div>
            <dl className="space-y-5 border-y border-grid py-6 text-sm">
                <div className="flex flex-wrap items-baseline justify-between gap-2">
                    <dt className="text-muted-foreground">已关联应收</dt>
                    <dd className="num font-medium">{allocationCount} 笔</dd>
                </div>
                <div className="flex flex-wrap items-baseline justify-between gap-2">
                    <dt className="text-muted-foreground">本次核销金额</dt>
                    <dd>
                        <MoneyValue value={allocated} />
                    </dd>
                </div>
                <div className="flex flex-wrap items-baseline justify-between gap-2">
                    <dt
                        className={
                            overAllocated
                                ? "text-destructive"
                                : "text-muted-foreground"
                        }
                    >
                        {overAllocated ? "超出回款金额" : "未核销金额"}
                    </dt>
                    <dd
                        className={
                            overAllocated ? "text-destructive" : undefined
                        }
                    >
                        <MoneyValue
                            value={
                                overAllocated
                                    ? subtractAmounts(allocated, amount)
                                    : unallocated
                            }
                        />
                    </dd>
                </div>
            </dl>
            <p className="text-xs leading-relaxed text-muted-foreground">
                {submitted
                    ? "已提交；核销进度以审批结果为准。"
                    : "审批通过后计入已收并完成核销。"}
            </p>
            <div className="mt-auto space-y-3 pt-6">
                {submitHint ? (
                    <p
                        role="status"
                        className="text-xs leading-relaxed text-muted-foreground"
                    >
                        {submitHint}
                    </p>
                ) : null}
                <div className="flex flex-col gap-3 [&>button]:w-full">
                    {actions}
                </div>
            </div>
        </aside>
    )
}
