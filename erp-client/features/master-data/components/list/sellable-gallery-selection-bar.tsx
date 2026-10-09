"use client"

import type { ReactNode } from "react"

import { cn } from "@/lib/utils"
import type { SellableListLayout } from "@/features/master-data/lib/sellable-list-layout"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export function SellableGallerySelectionBar({
    idPrefix = "master-data-sellable-items-gallery",
    layout = "gallery",
    actions,
    resultCount,
    selectedCount,
    allSelected,
    someSelected,
    onSelectAll,
    onClear,
}: {
    idPrefix?: string
    layout?: SellableListLayout
    actions?: ReactNode
    resultCount: number
    selectedCount: number
    allSelected: boolean
    someSelected: boolean
    onSelectAll: () => void
    onClear: () => void
}) {
    return (
        <div
            className="flex flex-wrap items-center gap-x-3 gap-y-2 text-body-compact"
            data-slot="sellable-gallery-selection-bar"
        >
            <label
                htmlFor={`${idPrefix}-select-all`}
                className={cn(
                    "inline-flex items-center",
                    layout === "gallery" && "gap-2",
                )}
            >
                <span
                    className={cn(
                        "inline-flex shrink-0 items-center justify-center",
                        layout === "table" && "w-table-column-selection",
                    )}
                >
                    <Checkbox
                        id={`${idPrefix}-select-all`}
                        checked={allSelected}
                        indeterminate={someSelected}
                        onCheckedChange={(checked) => {
                            if (checked === true) onSelectAll()
                            else onClear()
                        }}
                        disabled={resultCount === 0}
                        aria-label={masterDataCopy.sellableSelectAll}
                    />
                </span>
                <span>
                    已选{" "}
                    <span className="num font-medium">{selectedCount}</span> /{" "}
                    <span className="num">{resultCount}</span> 件
                </span>
            </label>
            <Button
                id={`${idPrefix}-select-all-action`}
                type="button"
                variant="ghost"
                size="sm"
                disabled={resultCount === 0 || allSelected}
                onClick={onSelectAll}
            >
                {masterDataCopy.sellableSelectAll}
            </Button>
            <Button
                id={`${idPrefix}-clear-selection`}
                type="button"
                variant="ghost"
                size="sm"
                disabled={selectedCount === 0}
                onClick={onClear}
            >
                {masterDataCopy.sellableClearSelection}
            </Button>
            {actions}
        </div>
    )
}
