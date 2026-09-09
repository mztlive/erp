"use client"

import { LayoutGridIcon, TableIcon } from "lucide-react"

import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { SellableListLayout } from "@/features/master-data/lib/sellable-list-layout"

export function SellableLayoutToggle({
    layout,
    onLayoutChange,
}: {
    layout: SellableListLayout
    onLayoutChange: (layout: SellableListLayout) => void
}) {
    return (
        <ToggleGroup
            value={[layout]}
            onValueChange={(values) => {
                const next = values[0]
                if (next === "table" || next === "gallery") onLayoutChange(next)
            }}
            variant="outline"
            spacing={0}
            size="sm"
            className="shrink-0"
            aria-label={masterDataCopy.sellableLayoutAria}
        >
            <ToggleGroupItem
                id="master-data-sellable-items-layout-table"
                value="table"
                aria-label={masterDataCopy.sellableLayoutTable}
            >
                <TableIcon data-icon="inline-start" aria-hidden="true" />
                {masterDataCopy.sellableLayoutTable}
            </ToggleGroupItem>
            <ToggleGroupItem
                id="master-data-sellable-items-layout-gallery"
                value="gallery"
                aria-label={masterDataCopy.sellableLayoutGallery}
            >
                <LayoutGridIcon data-icon="inline-start" aria-hidden="true" />
                {masterDataCopy.sellableLayoutGallery}
            </ToggleGroupItem>
        </ToggleGroup>
    )
}
