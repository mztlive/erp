"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    TableEmptyCell,
    TableTextCell,
} from "@/components/business"
import type { StatusTone } from "@/components/ui/status-badge"
import {
    lifecycleColumn,
    nameColumn,
    revisionNoColumn,
} from "@/features/master-data/components/list/list-column-primitives"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    qualificationHealthLabel,
    selectedSupplierOptionLabels,
    SUPPLIER_CAPABILITY_OPTIONS,
    SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
} from "@/features/master-data/lib/list-filters"
import type {
    MasterDataListItem,
    SupplierQualificationHealth,
} from "@/features/master-data/types"

const QUALIFICATION_HEALTH_TONE: Record<
    SupplierQualificationHealth,
    StatusTone
> = {
    valid: "success",
    expiring_30: "warning",
    expired: "destructive",
    unverified: "warning",
    not_registered: "neutral",
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
                id: "maintainer",
                header: "维护人",
                meta: { label: "维护人" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.supplierList?.maintainerUserName}
                        clamp={1}
                    />
                ),
            },
            {
                id: "capability",
                header: masterDataCopy.colCapability,
                meta: { label: masterDataCopy.colCapability },
                cell: ({ row }) => (
                    <TableTextCell
                        value={selectedSupplierOptionLabels(
                            row.original.supplierList?.capabilityCodes ?? [],
                            SUPPLIER_CAPABILITY_OPTIONS,
                        ).join("、")}
                        clamp={2}
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
                        numericSecondary
                    />
                ),
            },
            {
                id: "businessCategory",
                header: masterDataCopy.colBusinessCategory,
                meta: { label: masterDataCopy.colBusinessCategory },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.supplierList?.businessCategory}
                        clamp={1}
                    />
                ),
            },
            {
                ...revisionNoColumn(),
                meta: {
                    label: masterDataCopy.colVersion,
                    align: "end",
                    numeric: true,
                    width: "quantity",
                },
            },
        ],
        [],
    )
}

function QualificationCell({ item }: { item: MasterDataListItem }) {
    const health = item.supplierList?.qualificationHealth
    const types = selectedSupplierOptionLabels(
        item.supplierList?.qualificationTypes ?? [],
        SUPPLIER_QUALIFICATION_TYPE_OPTIONS,
    ).join("、")
    if (!health && !types) return <TableEmptyCell />
    return (
        <div className="flex min-w-0 flex-col items-start gap-0.5">
            {health ? (
                <BusinessStatusBadge
                    context="list"
                    tone={QUALIFICATION_HEALTH_TONE[health]}
                    label={qualificationHealthLabel(health)}
                    appearance={health === "valid" ? "quiet" : "badge"}
                />
            ) : null}
            {types ? (
                <TableTextCell
                    value={types}
                    clamp={1}
                    className="w-full text-xs text-muted-foreground"
                />
            ) : null}
        </div>
    )
}

function TwoLineCell({
    primary,
    secondary,
    numericSecondary = false,
}: {
    primary?: string
    secondary?: string
    numericSecondary?: boolean
}) {
    const main = primary !== "—" ? primary : undefined
    const sub = secondary !== "—" ? secondary : undefined
    if (!main && !sub) return <TableEmptyCell />
    return (
        <div className="flex min-w-0 flex-col">
            <TableTextCell
                value={main ?? sub}
                clamp={1}
                numeric={!main && numericSecondary}
            />
            {main && sub ? (
                <TableTextCell
                    value={sub}
                    clamp={1}
                    numeric={numericSecondary}
                    className="text-xs text-muted-foreground"
                />
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
    if (!payment || signing === payment) {
        return <TableTextCell value={signing} clamp={1} />
    }
    return (
        <div className="flex min-w-0 flex-col">
            <TableTextCell value={signing} clamp={1} />
            <TableTextCell
                value={`付款 ${payment}`}
                title={payment}
                clamp={1}
                className="text-xs text-muted-foreground"
            />
        </div>
    )
}
