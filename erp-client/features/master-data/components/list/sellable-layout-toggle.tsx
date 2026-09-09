"use client"

import { DownloadIcon, LayoutGridIcon, TableIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type { SellableListLayout } from "@/features/master-data/lib/sellable-list-layout"
import { cn } from "@/lib/utils"

const statusActionClassName =
    "h-auto rounded-none px-1 text-xs shadow-none hover:bg-transparent"

function StatusDivider() {
    return (
        <span
            className="mx-0.5 h-3 w-px shrink-0 bg-border"
            aria-hidden="true"
        />
    )
}

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
                size="xs"
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
                size="xs"
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

export function SellableListStatusActions({
    layout,
    onLayoutChange,
    exportPending,
    exportDisabled,
    exportLabel,
    onExport,
}: {
    layout: SellableListLayout
    onLayoutChange: (layout: SellableListLayout) => void
    exportPending: boolean
    exportDisabled: boolean
    exportLabel: string
    onExport: () => void
}) {
    return (
        <div className="flex items-center text-xs text-muted-foreground">
            <StatusDivider />
            <SellableLayoutToggle
                layout={layout}
                onLayoutChange={onLayoutChange}
            />
            <StatusDivider />
            <Button
                id="master-data-sellable-items-list-export"
                type="button"
                variant="ghost"
                size="xs"
                disabled={exportDisabled}
                className={cn(
                    statusActionClassName,
                    "font-normal",
                    exportDisabled
                        ? "text-muted-foreground"
                        : "text-foreground",
                )}
                onClick={onExport}
            >
                {exportPending ? (
                    <Spinner data-icon="inline-start" />
                ) : (
                    <DownloadIcon data-icon="inline-start" aria-hidden="true" />
                )}
                {exportLabel}
            </Button>
        </div>
    )
}
