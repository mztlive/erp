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
    onValueChange,
}: {
    value: ProductEditorSectionId
    isCreate: boolean
    canReadSupply: boolean
    onValueChange: (value: ProductEditorSectionId) => void
}) {
    const items = PRODUCT_EDITOR_SECTIONS.filter(
        (section) =>
            section.id !== "effective" &&
            section.id !== "sku" &&
            (section.id !== "supply" || canReadSupply) &&
            (!isCreate ||
                (section.id !== "history" && section.id !== "supply")),
    ).map((section) => ({
        id: section.id,
        label: section.label,
    }))

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
