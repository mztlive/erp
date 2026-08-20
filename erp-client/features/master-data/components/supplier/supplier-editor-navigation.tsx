"use client"

import { surfaceInsetClassName } from "@/components/business"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { SUPPLIER_SECTIONS } from "@/features/master-data/components/supplier/supplier-editor-fields"
import { cn } from "@/lib/utils"

export function SupplierSummaryStrip({
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

export function SupplierSectionTabs({
    value,
    isCreate,
    onValueChange,
}: {
    value: string
    isCreate: boolean
    onValueChange: (value: string) => void
}) {
    return (
        <Tabs
            value={value}
            onValueChange={(nextValue) => {
                if (nextValue) onValueChange(nextValue)
            }}
            className="gap-0"
        >
            <TabsList
                variant="line"
                aria-label="供应商编辑分区"
                className="sticky top-0 z-10 h-auto w-full flex-nowrap justify-start gap-0 overflow-x-auto rounded-none border-b border-grid bg-card/95 px-4 py-0 backdrop-blur supports-backdrop-filter:bg-card/85"
            >
                {SUPPLIER_SECTIONS.filter(
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
