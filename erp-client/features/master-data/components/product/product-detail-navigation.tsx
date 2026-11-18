"use client"

import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { MasterDataSummaryStrip } from "@/features/master-data/components/shared/summary-strip"
import {
    PRODUCT_EDITOR_SECTIONS,
    type ProductEditorSectionId,
} from "@/features/master-data/lib/product-editor-model"
import { toAutomationIdSegment } from "@/lib/automation-id"

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
    return (
        <Tabs
            value={value}
            onValueChange={(nextValue) => {
                if (nextValue) {
                    onValueChange(nextValue as ProductEditorSectionId)
                }
            }}
            className="gap-0"
        >
            <TabsList
                variant="line"
                aria-label="商品编辑分区"
                className="sticky top-0 z-10 h-auto w-full flex-nowrap justify-start gap-6 overflow-x-auto rounded-none border-b border-border bg-card px-0 py-0"
            >
                {PRODUCT_EDITOR_SECTIONS.filter(
                    (section) =>
                        section.id !== "effective" &&
                        section.id !== "sku" &&
                        (section.id !== "supply" || canReadSupply) &&
                        (!isCreate ||
                            (section.id !== "history" &&
                                section.id !== "supply")),
                ).map((section) => (
                    <TabsTrigger
                        key={section.id}
                        id={`master-data-product-editor-tab-${toAutomationIdSegment(section.id)}`}
                        value={section.id}
                        className="h-object-tab flex-none"
                    >
                        {section.label}
                    </TabsTrigger>
                ))}
            </TabsList>
        </Tabs>
    )
}

export { ProductSectionTabs, ProductSummaryStrip }
