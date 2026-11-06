"use client"

import type { ReactNode } from "react"
import { MoneyValue, StickyTotalBar } from "@/components/business"
import { compareAmounts, subtractAmounts } from "../lib/allocation-math"

/** 回款登记第二步的金额核对栏，窄窗口保留全部操作。 */
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
        <StickyTotalBar
            items={[
                {
                    id: "receipt",
                    label: existing ? "原单到账金额" : "到账金额",
                    value: <MoneyValue value={amount || "0"} />,
                },
                {
                    id: "allocated",
                    label: "本次核销",
                    value: <MoneyValue value={allocated} />,
                },
                {
                    id: "unallocated",
                    label: overAllocated ? "超出回款金额" : "剩余未核销",
                    value: (
                        <MoneyValue
                            className={
                                overAllocated ? "text-destructive" : undefined
                            }
                            value={
                                overAllocated
                                    ? subtractAmounts(allocated, amount)
                                    : unallocated
                            }
                        />
                    ),
                },
            ]}
            note={
                submitHint ??
                `本次 ${allocationCount} 笔应收 · ${submitted ? "已提交，核销进度以审批结果为准" : "审批通过后计入已收并完成核销"}`
            }
            actions={actions}
        />
    )
}
