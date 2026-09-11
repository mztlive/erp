"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import { Badge } from "@/components/ui/badge"
import {
    lifecycleColumn,
    nameColumn,
    revisionNoColumn,
} from "@/features/master-data/components/list/list-column-primitives"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    qualificationHealthLabel,
    SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
} from "@/features/master-data/lib/list-filters"
import { capabilityLabel } from "@/features/master-data/api/presentation"
import type {
    MasterDataListItem,
    SupplierQualificationHealth,
} from "@/features/master-data/types"

const HEALTH_BADGE_VARIANT: Record<
    SupplierQualificationHealth,
    React.ComponentProps<typeof Badge>["variant"]
> = {
    valid: "success",
    expiring_30: "warning",
    expired: "destructive",
    unverified: "orange",
    not_registered: "outline",
}

export function useSupplierListColumns() {
    return React.useMemo<ColumnDef<MasterDataListItem>[]>(
        () => [
            {
                ...nameColumn({ showNumber: true }),
                header: masterDataCopy.colSupplier,
                meta: { label: masterDataCopy.colSupplier, width: "flex" },
            },
            lifecycleColumn(),
            {
                id: "capability",
                header: masterDataCopy.colCapability,
                meta: { label: masterDataCopy.colCapability },
                cell: ({ row }) => (
                    <CapabilityBadges
                        codes={row.original.supplierList?.capabilityCodes ?? []}
                    />
                ),
            },
            {
                id: "qualification",
                header: masterDataCopy.colQualification,
                meta: { label: masterDataCopy.colQualification },
                cell: ({ row }) => <QualificationCell item={row.original} />,
            },
            {
                id: "settlement",
                header: masterDataCopy.colSettlement,
                meta: { label: masterDataCopy.colSettlement },
                cell: ({ row }) => (
                    <TwoLineCell
                        primary={row.original.supplierList?.settlementLabel}
                        secondary={row.original.supplierList?.paymentTermLabel}
                    />
                ),
            },
            {
                id: "entities",
                header: masterDataCopy.colEntities,
                meta: { label: masterDataCopy.colEntities },
                cell: ({ row }) => (
                    <EntityNames
                        signing={row.original.supplierList?.signingEntityName}
                        payment={row.original.supplierList?.paymentEntityName}
                    />
                ),
            },
            {
                id: "invoice",
                header: masterDataCopy.colInvoice,
                meta: { label: masterDataCopy.colInvoice },
                cell: ({ row }) => (
                    <TwoLineCell
                        primary={row.original.supplierList?.invoiceTypeLabel}
                        secondary={
                            row.original.supplierList?.invoiceTaxRatesLabel
                        }
                    />
                ),
            },
            {
                id: "businessCategory",
                header: masterDataCopy.colBusinessCategory,
                meta: { label: masterDataCopy.colBusinessCategory },
                cell: ({ row }) => (
                    <span className="truncate text-sm">
                        {row.original.supplierList?.businessCategory || (
                            <span className="text-muted-foreground">—</span>
                        )}
                    </span>
                ),
            },
            revisionNoColumn(),
        ],
        [],
    )
}

function CapabilityBadges({ codes }: { codes: readonly string[] }) {
    if (codes.length === 0) {
        return <span className="text-sm text-muted-foreground">—</span>
    }
    return (
        <div className="flex min-w-0 flex-wrap gap-1">
            {codes.map((code) => (
                <Badge key={code} variant="secondary">
                    {capabilityLabel(code) || code}
                </Badge>
            ))}
        </div>
    )
}

function QualificationCell({ item }: { item: MasterDataListItem }) {
    const health = item.supplierList?.qualificationHealth
    const types = item.supplierList?.qualificationTypes ?? []
    const typeLabel = types
        .map(
            (code) =>
                SUPPLIER_QUALIFICATION_TYPE_OPTIONS.find(
                    (option) => option.value === code,
                )?.label ?? code,
        )
        .join("、")
    if (!health) {
        return <span className="text-sm text-muted-foreground">—</span>
    }
    return (
        <div className="min-w-0 space-y-1">
            <Badge variant={HEALTH_BADGE_VARIANT[health]}>
                {qualificationHealthLabel(health)}
            </Badge>
            {typeLabel ? (
                <div
                    className="truncate text-xs text-muted-foreground"
                    title={typeLabel}
                >
                    {typeLabel}
                </div>
            ) : null}
        </div>
    )
}

function TwoLineCell({
    primary,
    secondary,
}: {
    primary?: string
    secondary?: string
}) {
    const main = primary && primary !== "—" ? primary : undefined
    const sub = secondary && secondary !== "—" ? secondary : undefined
    if (!main && !sub) {
        return <span className="text-sm text-muted-foreground">—</span>
    }
    return (
        <div className="min-w-0">
            <div className="truncate text-sm">{main ?? sub}</div>
            {main && sub ? (
                <div className="truncate text-xs text-muted-foreground">
                    {sub}
                </div>
            ) : null}
        </div>
    )
}

function EntityNames({
    signing,
    payment,
}: {
    signing?: string
    payment?: string
}) {
    if (!signing && !payment) {
        return <span className="text-sm text-muted-foreground">—</span>
    }
    if (signing && payment && signing === payment) {
        return (
            <span className="truncate text-sm" title={signing}>
                {signing}
            </span>
        )
    }
    return (
        <div className="min-w-0 space-y-0.5">
            <div className="truncate text-sm" title={signing}>
                {signing || "—"}
            </div>
            {payment ? (
                <div
                    className="truncate text-xs text-muted-foreground"
                    title={payment}
                >
                    付款 {payment}
                </div>
            ) : null}
        </div>
    )
}
