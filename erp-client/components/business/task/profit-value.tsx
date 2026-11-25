"use client"

import type * as React from "react"

import { MoneyValue } from "@/components/business/values"
import { cn } from "@/lib/utils"

const zeroDecimalPattern = /^-?0+(?:\.0+)?$/

export interface ProfitValueProps {
    /** 利润或价差十进制金额字符串（如 `"120.00"`、`"-50.00"`、`"0.00"`）。 */
    amount: string | null | undefined
    className?: string
    /** 金额展示模式，默认精确展示 `"exact"`。 */
    display?: React.ComponentProps<typeof MoneyValue>["display"]
    /** 金额字号档位，默认继承父级字号。 */
    size?: React.ComponentProps<typeof MoneyValue>["size"]
    /** 正利润中文前缀，默认 `"赚 "`。 */
    profitPrefix?: string
    /** 负利润/倒挂中文前缀，默认 `"亏 "`。 */
    lossPrefix?: string
}

/**
 * 任务页盈亏双编码原语（docs/task-page-design-system.md §3.3.1）：
 * - 正利润：`赚 ¥X.XX`（`text-success-soft-foreground`）
 * - 负利润/倒挂：`亏 ¥X.XX`（`text-destructive-soft-foreground`）
 * - 持平零值：`¥0.00`（`text-muted-foreground`）
 * - 空值：`—`（由 `MoneyValue` 统一下沉展示）
 */
export function ProfitValue({
    amount,
    className,
    display = "exact",
    size,
    profitPrefix = "赚 ",
    lossPrefix = "亏 ",
}: ProfitValueProps) {
    const trimmed = amount?.trim() ?? null
    const isEmpty = trimmed == null || trimmed.length === 0
    const isZero = !isEmpty && zeroDecimalPattern.test(trimmed)
    const loss = !isEmpty && !isZero && trimmed.startsWith("-")
    const profit = !isEmpty && !isZero && !loss
    const unsignedAmount =
        !isEmpty && trimmed.startsWith("-") ? trimmed.slice(1) : trimmed

    return (
        <span
            data-slot="profit-value"
            data-trend={
                loss ? "loss" : profit ? "profit" : isZero ? "flat" : "empty"
            }
            className={cn(
                "num font-medium whitespace-nowrap",
                loss && "text-destructive-soft-foreground",
                profit && "text-success-soft-foreground",
                isZero && "text-muted-foreground",
                className,
            )}
        >
            {loss ? lossPrefix : profit ? profitPrefix : null}
            <MoneyValue
                display={display}
                size={size}
                value={unsignedAmount}
                inheritColor
            />
        </span>
    )
}

export { ProfitValue as PurchaseProfitValue }
