"use client"

import { LayoutGridIcon, TableIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { SellableListLayout } from "@/features/master-data/lib/sellable-list-layout"
import { cn } from "@/lib/utils"

const statusActionClassName = "shadow-none"

export function SellableLayoutToggle({
    layout,
    onLayoutChange,
}: {
    layout: SellableListLayout
    onLayoutChange: (layout: SellableListLayout) => void
}) {
    return (
        <div
            role="group"
            aria-label={masterDataCopy.sellableLayoutAria}
            className="flex items-center"
        >
            <Button
                id="master-data-sellable-items-layout-table"
                type="button"
                variant="ghost"
                size="sm"
                aria-pressed={layout === "table"}
                className={cn(
                    statusActionClassName,
                    layout === "table"
                        ? "font-medium text-foreground"
                        : "font-normal text-muted-foreground",
                )}
                onClick={() => onLayoutChange("table")}
            >
                <TableIcon data-icon="inline-start" aria-hidden="true" />
                {masterDataCopy.sellableLayoutTable}
            </Button>
            <span className="text-border" aria-hidden="true">
                /
            </span>
            <Button
                id="master-data-sellable-items-layout-gallery"
                type="button"
                variant="ghost"
                size="sm"
                aria-pressed={layout === "gallery"}
                className={cn(
                    statusActionClassName,
                    layout === "gallery"
                        ? "font-medium text-foreground"
                        : "font-normal text-muted-foreground",
                )}
                onClick={() => onLayoutChange("gallery")}
            >
                <LayoutGridIcon data-icon="inline-start" aria-hidden="true" />
                {masterDataCopy.sellableLayoutGallery}
            </Button>
        </div>
    )
}
