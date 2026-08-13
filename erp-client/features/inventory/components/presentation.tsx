"use client"

import { FilterChip } from "@/components/business/filter-chip"

/** 深链/隐形筛选参数的可移除标记：URL 参数与界面控件一一对应。
 *  复用共享 FilterChip（components/business/filter-chip.tsx），保持跨页形态一致。 */
function ChipFilter({
    label,
    onClear,
}: {
    label: string
    onClear: () => void
}) {
    return <FilterChip label={label} onClear={onClear} />
}

function formatQty(value: string, unit: string) {
    return (
        <span className="num text-sm">
            {value}
            <span className="ml-1 text-xs font-normal text-muted-foreground">
                {unit}
            </span>
        </span>
    )
}

export { ChipFilter, formatQty }
