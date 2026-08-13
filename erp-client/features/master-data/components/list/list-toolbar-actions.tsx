"use client"

import { Button } from "@/components/ui/button"

export function ListToolbarCount({
    label,
    rowCount,
    hasActiveFilters,
    onClear,
}: {
    label?: string
    rowCount: number
    hasActiveFilters: boolean
    onClear: () => void
}) {
    return (
        <>
            <span className="text-xs text-muted-foreground" aria-live="polite">
                {label ? `${label} · ${rowCount} 条` : `共 ${rowCount} 条`}
            </span>
            {hasActiveFilters ? (
                <Button type="button" size="sm" variant="ghost" onClick={onClear}>
                    清除筛选
                </Button>
            ) : null}
        </>
    )
}
