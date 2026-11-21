"use client"

import { DetailWorkspaceTabs } from "@/components/business/detail-workspace"
import { MasterDataSummaryStrip } from "@/features/master-data/components/shared/summary-strip"
import {
    PRODUCT_EDITOR_SECTIONS,
    type ProductEditorSectionId,
} from "@/features/master-data/lib/product-editor-model"

function ProductSummaryStrip({
    rows,
}: {
    rows: ReadonlyArray<Readonly<{ label: string; value: string }>>
}) {
    return <MasterDataSummaryStrip rows={rows} />
}

function ProductSectionTabs({
    value,
    isCreate,
    canReadSupply,
    supplyCount,
    missingSupplyCount,
    mediaCount,
    onValueChange,
}: {
    value: ProductEditorSectionId
    isCreate: boolean
    canReadSupply: boolean
    supplyCount?: number
    missingSupplyCount?: number
    mediaCount?: number
    onValueChange: (value: ProductEditorSectionId) => void
}) {
    const items = PRODUCT_EDITOR_SECTIONS.filter(
        (section) =>
            section.id !== "effective" &&
            section.id !== "sku" &&
            (section.id !== "supply" || canReadSupply) &&
            (!isCreate ||
                (section.id !== "history" && section.id !== "supply")),
    ).map((section) => {
        let badge: React.ReactNode
        if (section.id === "supply" && !isCreate) {
            if (missingSupplyCount != null && missingSupplyCount > 0) {
                badge = (
                    <span className="num rounded-sm bg-warning-soft px-1.5 py-0.5 text-tiny leading-3.5 font-medium text-warning-soft-foreground">
                        缺 {missingSupplyCount}
                    </span>
                )
            } else if (supplyCount != null && supplyCount > 0) {
                badge = (
                    <span className="num text-xs font-normal text-muted-foreground/75">
                        {supplyCount}
                    </span>
                )
            }
        } else if (
            section.id === "media" &&
            mediaCount != null &&
            mediaCount > 0
        ) {
            badge = (
                <span className="num text-xs font-normal text-muted-foreground/75">
                    {mediaCount}
                </span>
            )
        }
        return {
            id: section.id,
            label: section.label,
            badge,
        }
    })

    return (
        <DetailWorkspaceTabs
            value={value}
            onValueChange={(nextValue) =>
                onValueChange(nextValue as ProductEditorSectionId)
            }
            items={items}
            ariaLabel="商品编辑分区"
            tabIdPrefix="master-data-product-editor-tab"
        />
    )
}

export { ProductSectionTabs, ProductSummaryStrip }
