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
import type { HistoryQualityRow } from "../../dual-types"
import type { PatchDual } from "../../lib/dual-filter-state"

export function HistoryRowsTable({
    items,
    dimension,
    patchDual,
}: {
    items: readonly HistoryQualityRow[]
    dimension: string
    patchDual: PatchDual
}) {
    const byUser = dimension === "attribution_user"
    const data = React.useMemo(() => [...items], [items])
    const columns = React.useMemo<ColumnDef<HistoryQualityRow>[]>(
        () => [
            {
                id: "identity",
                header: byUser ? "历史归属销售" : "历史归属组织",
                meta: { width: "flex" },
                cell: ({ row: { original: row } }) => {
                    const name =
                        (byUser
                            ? row.attributionUserName
                            : row.attributionOrgUnitName) ||
                        (!row.groupId ? row.label : undefined)
                    return (
                        <TableIdentityCell
                            primary={name || <TableEmptyCell />}
                            primaryTitle={name}
                            secondary={row.orderNo}
                            secondaryTitle={row.orderNo}
                            numericSecondary
                        />
                    )
                },
            },
            {
                id: "customer",
                header: "归属客户",
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.customerName}
                        clamp={1}
                    />
                ),
            },
            {
                id: "orderCount",
                header: "订单数",
                meta: { align: "end", numeric: true, width: "quantity" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.orderCount?.toString()}
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
            {
                id: "actions",
                header: "查看明细",
                meta: { align: "end", width: "status" },
                cell: ({ row: { original: row } }) => {
                    if (row.groupId == null) return <TableEmptyCell />
                    const prefix = byUser
                        ? "attribution_user"
                        : "attribution_org"
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
                                            attributionGroup: `${prefix}:${row.groupId}`,
                                            scopeVersion: null,
                                            dualPage: null,
                                        }),
                                },
                            ]}
                        />
                    )
                },
            },
        ],
        [byUser, patchDual],
    )

    return (
        <ListWorkSurface
            ariaLabel="历史贡献明细"
            className="flex-none"
            tableClassName="max-lg:[&_[data-column-pinned]]:static"
            table={
                <ListDataTable
                    id="customers-quality-dual-history-table"
                    caption="历史贡献明细"
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
