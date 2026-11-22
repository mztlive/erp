"use client"

import { DetailWorkspaceTabs } from "@/components/business"
import { SUPPLIER_SECTIONS } from "@/features/master-data/components/supplier/supplier-editor-fields"

export function SupplierSummaryStrip({
    rows,
}: {
    rows: ReadonlyArray<Readonly<{ label: string; value: string }>>
}) {
    return (
        <dl className="grid grid-cols-2 gap-x-6 gap-y-3 py-3.5 sm:grid-cols-3 xl:grid-cols-6">
            {rows.map(({ label, value }) => (
                <div key={label} className="min-w-0">
                    <dt className="text-xs text-muted-foreground">{label}</dt>
                    <dd
                        className="mt-0.5 wrap-anywhere text-body-compact font-medium"
                        title={value}
                    >
                        {value}
                    </dd>
                </div>
            ))}
        </dl>
    )
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
        <DetailWorkspaceTabs
            value={value}
            onValueChange={onValueChange}
            ariaLabel="供应商编辑分区"
            tabIdPrefix="master-data-supplier-editor-tab"
            items={SUPPLIER_SECTIONS.filter(
                (section) =>
                    (!isCreate ||
                        ![
                            "history",
                            "delivery",
                            "portal-accounts",
                            "portal-catalog",
                            "portal-applications",
                        ].includes(section.id)) &&
                    (section.id !== "delivery" || canReadDeliveryTemplates) &&
                    (section.id !== "portal-accounts" ||
                        canReadPortalAccounts) &&
                    (section.id !== "portal-catalog" || canReadPortalCatalog) &&
                    (section.id !== "portal-applications" ||
                        canReadPortalApplications),
            )}
        />
    )
}
