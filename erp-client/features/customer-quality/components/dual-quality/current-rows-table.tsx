"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    MoneyValue,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import {
    ListDataTable,
    ListWorkSurface,
} from "@/components/business/list-workspace"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { CurrentQualityRow } from "../../dual-types"
import type { PatchDual } from "../../lib/dual-filter-state"

const currentCustomerCountColumn: ColumnDef<CurrentQualityRow> = {
    id: "customerCount",
    header: "客户数",
    meta: { align: "end", numeric: true, width: "quantity" },
    cell: ({ row }) => (
        <TableTextCell value={row.original.customerCount?.toString()} numeric />
    ),
}

export function CurrentRowsTable({
    items,
    dimension,
    patchDual,
}: {
    items: readonly CurrentQualityRow[]
    dimension: string
    patchDual: PatchDual
}) {
    const grouped = dimension !== "customer"
    const data = React.useMemo(() => [...items], [items])
    const columns = React.useMemo<ColumnDef<CurrentQualityRow>[]>(() => {
        const result: ColumnDef<CurrentQualityRow>[] = [
            {
                id: "identity",
                header: grouped ? "分组" : "客户",
                meta: { width: "flex" },
                cell: ({ row: { original: row } }) => {
                    const name = grouped
                        ? (dimension === "owner_user"
                              ? row.ownerUserName
                              : row.ownerOrgUnitName) ||
                          (!row.groupId ? row.label : undefined)
                        : row.customerName
                    const secondary = grouped ? undefined : row.customerNo
                    return (
                        <TableIdentityCell
                            primary={name || <TableEmptyCell />}
                            primaryTitle={name}
                            secondary={secondary}
                            secondaryTitle={secondary}
                            numericSecondary
                        />
                    )
                },
            },
            {
                id: "ownerUser",
                header: "现任负责人",
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.ownerUserName}
                        clamp={1}
                    />
                ),
            },
            {
                id: "ownerOrg",
                header: "现任组织",
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.ownerOrgUnitName}
                        clamp={1}
                    />
                ),
            },
            ...(grouped ? [currentCustomerCountColumn] : []),
            {
                id: "orderCount",
                header: "订单数",
                meta: { align: "end", numeric: true, width: "quantity" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.orderCount.toString()}
                        numeric
                    />
                ),
            },
            {
                id: "grossTotal",
                header: "含税总额",
                meta: { align: "end", numeric: true, width: "amount" },
                cell: ({ row }) => (
                    <MoneyValue
                        value={row.original.grossTotal}
                        taxBasis="gross"
                        display="compact"
                    />
                ),
            },
            {
                id: "unpricedCount",
                header: "待核价单数",
                meta: { align: "end", numeric: true, width: "quantity" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.unpricedCount.toString()}
                        numeric
                    />
                ),
            },
        ]
        if (grouped) {
            result.push({
                id: "actions",
                header: "查看明细",
                meta: { align: "end", width: "status" },
                cell: ({ row: { original: row } }) => {
                    if (row.groupId == null) return <TableEmptyCell />
                    const prefix =
                        dimension === "owner_user"
                            ? "user"
                            : dimension === "owner_org"
                              ? "org"
                              : null
                    if (!prefix) return <TableEmptyCell />

                    const id = `customers-quality-dual-drill-${toAutomationIdSegment(row.rowId)}`
                    return (
                        <TableRowActions
                            moreId={`${id}-more`}
                            moreLabel="更多操作"
                            actions={[
                                {
                                    id,
                                    label: "查看明细",
                                    onClick: () =>
                                        patchDual({
                                            ownerGroup: `${prefix}:${row.groupId}`,
                                            scopeVersion: null,
                                            dualPage: null,
                                        }),
                                },
                            ]}
                        />
                    )
                },
            })
        }
        return result
    }, [dimension, grouped, patchDual])

    return (
        <ListWorkSurface
            ariaLabel="当前负责明细"
            className="flex-none"
            tableClassName="max-lg:[&_[data-column-pinned]]:static"
            table={
                <ListDataTable
                    id="customers-quality-dual-current-table"
                    caption="当前负责明细"
                    data={data}
                    columns={columns}
                    getRowId={(row) => row.rowId}
                    rowCount={data.length}
                    defaultColumnPinning={{
                        left: ["identity"],
                        right: ["actions"],
                    }}
                    manualPagination
                    showPagination={false}
                />
            }
        />
    )
}
