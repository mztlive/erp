"use client"

import { ListWorkspaceViews } from "@/components/business/list-workspace"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    SELLABLE_LIST_LAYOUTS,
    type SellableListLayout,
} from "@/features/master-data/lib/sellable-list-layout"

export function SellableLayoutToggle({
    layout,
    onLayoutChange,
}: {
    layout: SellableListLayout
    onLayoutChange: (layout: SellableListLayout) => void
}) {
    return (
        <ListWorkspaceViews
            variant="inline"
            ariaLabel={masterDataCopy.sellableLayoutAria}
            items={SELLABLE_LIST_LAYOUTS.map((mode) => ({
                id: `master-data-sellable-items-layout-${mode}`,
                label:
                    mode === "gallery"
                        ? masterDataCopy.sellableLayoutGallery
                        : masterDataCopy.sellableLayoutTable,
                active: mode === layout,
                onClick: () => onLayoutChange(mode),
            }))}
        />
    )
}
