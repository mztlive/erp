"use client"

import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { MasterDataSummaryStrip } from "@/features/master-data/components/shared/summary-strip"
import { SUPPLIER_SECTIONS } from "@/features/master-data/components/supplier/supplier-editor-fields"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function SupplierSummaryStrip({
    rows,
}: {
    rows: ReadonlyArray<Readonly<{ label: string; value: string }>>
}) {
    return <MasterDataSummaryStrip rows={rows} />
}

export function SupplierSectionTabs({
    value,
    isCreate,
    canReadDeliveryTemplates,
    canReadPortalAccounts,
    canReadPortalCatalog,
    canReadPortalApplications,
    onValueChange,
}: {
    value: string
    isCreate: boolean
    canReadDeliveryTemplates: boolean
    canReadPortalAccounts: boolean
    canReadPortalCatalog: boolean
    canReadPortalApplications: boolean
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
                className="sticky top-0 z-10 h-auto w-full flex-nowrap justify-start gap-6 overflow-x-auto rounded-none border-b border-border bg-card px-0 py-0"
            >
                {SUPPLIER_SECTIONS.filter(
                    (section) =>
                        (!isCreate ||
                            ![
                                "history",
                                "delivery",
                                "portal-accounts",
                                "portal-catalog",
                                "portal-applications",
                            ].includes(section.id)) &&
                        (section.id !== "delivery" ||
                            canReadDeliveryTemplates) &&
                        (section.id !== "portal-accounts" ||
                            canReadPortalAccounts) &&
                        (section.id !== "portal-catalog" ||
                            canReadPortalCatalog) &&
                        (section.id !== "portal-applications" ||
                            canReadPortalApplications),
                ).map((section) => (
                    <TabsTrigger
                        key={section.id}
                        id={`master-data-supplier-editor-tab-${toAutomationIdSegment(section.id)}`}
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
