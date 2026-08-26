"use client"

import { surfaceInsetClassName } from "@/components/business"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import {
    PRODUCT_EDITOR_SECTIONS,
    type ProductEditorSectionId,
} from "@/features/master-data/lib/product-editor-model"
import { cn } from "@/lib/utils"

function ProductSummaryStrip({
    rows,
}: {
    rows: ReadonlyArray<Readonly<{ label: string; value: string }>>
}) {
    return (
        <dl
            className={cn(
                surfaceInsetClassName,
                "grid grid-cols-2 gap-x-6 gap-y-3 px-4 py-3 sm:grid-cols-4",
            )}
        >
            {rows.map((row) => (
                <div key={row.label} className="min-w-0">
                    <dt className="text-tiny text-muted-foreground">
                        {row.label}
                    </dt>
                    <dd
                        className="mt-0.5 truncate text-sm font-medium text-foreground"
                        title={row.value}
                    >
                        {row.value}
                    </dd>
                </div>
            ))}
        </dl>
    )
}

function ProductSectionTabs({
    value,
    isCreate,
    onValueChange,
}: {
    value: ProductEditorSectionId
    isCreate: boolean
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
                className="sticky top-0 z-10 h-auto w-full flex-nowrap justify-start gap-0 overflow-x-auto rounded-none border-b border-grid bg-card/95 px-4 py-0 backdrop-blur supports-backdrop-filter:bg-card/85"
            >
                {PRODUCT_EDITOR_SECTIONS.filter(
                    (section) => !isCreate || section.id !== "history",
                ).map((section) => (
                    <TabsTrigger
                        key={section.id}
                        value={section.id}
                        className="h-11 flex-none rounded-none px-4 text-sm after:inset-x-3 after:bottom-0 after:h-0.5 after:rounded-full after:bg-primary data-active:font-semibold"
                    >
                        {section.label}
                    </TabsTrigger>
                ))}
            </TabsList>
        </Tabs>
    )
}

export { ProductSectionTabs, ProductSummaryStrip }
