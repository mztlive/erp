"use client"

import { DetailWorkspaceTabs } from "@/components/business"
import {
    SUPPLIER_SECTIONS,
    type SupplierPrimarySectionId,
} from "@/features/master-data/components/supplier/supplier-editor-fields"

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
    canReadPortal,
    onValueChange,
}: {
    value: SupplierPrimarySectionId
    isCreate: boolean
    canReadDeliveryTemplates: boolean
    canReadPortal: boolean
    onValueChange: (value: SupplierPrimarySectionId) => void
}) {
    return (
        <DetailWorkspaceTabs
            value={value}
            onValueChange={(next) =>
                onValueChange(next as SupplierPrimarySectionId)
            }
            ariaLabel="供应商编辑分区"
            tabIdPrefix="master-data-supplier-editor-tab"
            items={SUPPLIER_SECTIONS.filter((section) => {
                if (isCreate && section.id !== "profile") return false
                if (section.id === "delivery" && !canReadDeliveryTemplates)
                    return false
                if (section.id === "portal" && !canReadPortal) return false
                return true
            })}
        />
    )
}
