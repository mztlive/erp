"use client"

import type { ReactNode } from "react"

import {
    MoneyValue,
    StickyTotalBar,
    WorkspaceTaskFooter,
    useWorkspaceTaskPane,
} from "@/components/business"
import {
    compareAmounts,
    subtractAmounts,
} from "@/features/customer-receivables/lib/allocation-math"

/** 回款核对金额与审批动作在页面或工作台底栏持续可见。 */
export function ReceiptSessionFooter({
    amount,
    allocated,
    unallocated,
    existing,
    submitted,
    actions,
}: {
    amount: string
    allocated: string
    unallocated: string
    existing: boolean
    submitted: boolean
    actions: ReactNode
}) {
    const embedded = useWorkspaceTaskPane()
    const overAllocated = compareAmounts(allocated, amount) > 0
    const content = (
        <StickyTotalBar
            className={`${embedded ? "static w-full border-0 py-0" : "mt-auto w-full"} [&>div]:lg:flex-wrap [&>div>div:first-child]:lg:basis-144 [&>div>div:last-child]:lg:ml-auto`}
            items={[
                {
                    id: "receipt",
                    label: existing ? "可核销余额" : "回款金额",
                    value: <MoneyValue value={amount || "0"} size="section" />,
                },
                {
                    id: "allocated",
                    label: "拟核销金额",
                    value: <MoneyValue value={allocated} size="section" />,
                },
                {
                    id: "remaining",
                    label: overAllocated ? "超出回款金额" : "剩余待核销",
                    value: (
                        <span
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
                                size="section"
                            />
                        </span>
                    ),
                },
            ]}
            note={
                submitted
                    ? "已提交；核销进度以审批结果为准。"
                    : "审批通过后计入已收并完成核销。"
            }
            actions={actions}
        />
    )
    return (
        <WorkspaceTaskFooter fallback={content}>{content}</WorkspaceTaskFooter>
    )
}
