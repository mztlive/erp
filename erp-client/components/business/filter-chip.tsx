"use client"

import * as React from "react"

import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"

interface FilterChipProps extends React.ComponentProps<"span"> {
    /** 芯片展示文案，例如「客户：张三」或「销售单 SO-2024-001」。 */
    readonly label: React.ReactNode
    /** 点击 × 移除该筛选。 */
    readonly onClear: () => void
    /** 清除按钮的无障碍说明；缺省为「清除{label}筛选」。 */
    readonly clearLabel?: string
    readonly idPrefix?: string
}

/**
 * 来源/深链筛选的可移除徽标。
 *
 * 把被查询消费、但由其它页面带入的隐形参数（customerId/skuId/orderNo…）
 * 显性化为可单独移除的 chip；「清除筛选」会一并清除它们。
 */
function FilterChip({
    label,
    onClear,
    clearLabel,
    className,
    id,
    idPrefix,
    ...props
}: FilterChipProps) {
    const baseId = idPrefix ?? id
    return (
        <Badge
            variant="secondary"
            data-slot="filter-chip"
            className={cn("gap-1 font-normal", className)}
            id={baseId}
            {...props}
        >
            {label}
            <button
                type="button"
                id={baseId ? `${baseId}-clear` : undefined}
                onClick={onClear}
                aria-label={clearLabel ?? `清除${String(label)}筛选`}
                className="rounded-sm opacity-70 transition-opacity hover:opacity-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
                <svg
                    className="size-3"
                    aria-hidden="true"
                    viewBox="0 0 24 24"
                    fill="none"
                    stroke="currentColor"
                    strokeWidth="2"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                >
                    <path d="M18 6 6 18" />
                    <path d="m6 6 12 12" />
                </svg>
            </button>
        </Badge>
    )
}

export { FilterChip, type FilterChipProps }
