"use client"

import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export function SellableGallerySelectionBar({
    resultCount,
    selectedCount,
    allSelected,
    someSelected,
    onSelectAll,
    onClear,
}: {
    resultCount: number
    selectedCount: number
    allSelected: boolean
    someSelected: boolean
    onSelectAll: () => void
    onClear: () => void
}) {
    return (
        <div
            className="flex flex-wrap items-center gap-x-3 gap-y-2 border-b border-border py-2.5 text-[13px]"
            data-slot="sellable-gallery-selection-bar"
        >
            <label
                htmlFor="master-data-sellable-items-gallery-select-all"
                className="inline-flex items-center gap-2"
            >
                <Checkbox
                    id="master-data-sellable-items-gallery-select-all"
                    checked={
                        allSelected ? true : someSelected ? "mixed" : false
                    }
                    onCheckedChange={(checked) => {
                        if (checked === true) onSelectAll()
                        else onClear()
                    }}
                    disabled={resultCount === 0}
                    aria-label={masterDataCopy.sellableSelectAll}
                />
                <span>
                    已选{" "}
                    <span className="num font-medium">{selectedCount}</span> /{" "}
                    <span className="num">{resultCount}</span> 件
                </span>
            </label>
            <Button
                id="master-data-sellable-items-gallery-select-all-action"
                type="button"
                variant="ghost"
                size="sm"
                disabled={resultCount === 0 || allSelected}
                onClick={onSelectAll}
            >
                {masterDataCopy.sellableSelectAll}
            </Button>
            <Button
                id="master-data-sellable-items-gallery-clear-selection"
                type="button"
                variant="ghost"
                size="sm"
                disabled={selectedCount === 0}
                onClick={onClear}
            >
                {masterDataCopy.sellableClearSelection}
            </Button>
        </div>
    )
}
