"use client"

import * as React from "react"
import { XIcon } from "lucide-react"

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
            variant="neutral"
            data-slot="filter-chip"
            className={cn(
                "h-control-xs gap-1 rounded-md border border-border/65 bg-muted/65 px-2 text-xs font-normal text-foreground/85",
                className,
            )}
            id={baseId}
            {...props}
        >
            {label}
            <button
                type="button"
                id={baseId ? `${baseId}-clear` : undefined}
                onClick={onClear}
                aria-label={clearLabel ?? `清除${String(label)}筛选`}
                className="-mr-0.5 inline-flex size-4 items-center justify-center rounded-sm text-muted-foreground/75 transition-colors hover:bg-foreground/10 hover:text-foreground focus-visible:outline-none focus-visible:ring-3 focus-visible:ring-ring"
            >
                <XIcon className="size-3" aria-hidden="true" />
            </button>
        </Badge>
    )
}

export { FilterChip, type FilterChipProps }
