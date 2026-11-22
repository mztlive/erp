"use client"

import { ArrowUpRightIcon, EyeIcon } from "lucide-react"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    MoneyValue,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { SettlementListRow } from "@/features/supplier-settlements/types"

export function useSettlementListColumns(
    onPreview: (statementId: string) => void,
    onOpen: (statementId: string) => void,
) {
    return React.useMemo<ColumnDef<SettlementListRow>[]>(
        () => [
            {
                id: "statementNo",
                accessorFn: (row) => row.statementNo,
                header: "结算单 / 供应商",
                meta: { label: "结算单 / 供应商", width: "flex" },
                cell: ({ row: { original } }) => (
                    <TableIdentityCell
                        primary={original.supplierName}
                        primaryTitle={original.supplierName}
                        secondary={original.statementNo}
                        secondaryTitle={original.statementNo}
                        numericSecondary
                    />
                ),
            },
            {
                id: "supplier",
                accessorFn: (row) => row.supplierName,
                header: "供应商",
                meta: { label: "供应商" },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.supplierName}
                        clamp={1}
                    />
                ),
            },
            {
                id: "period",
                accessorFn: (row) => row.periodLabel,
                header: "期间",
                meta: { label: "期间", width: "default" },
                cell: ({ row: { original } }) => (
                    <div>
                        <TableTextCell value={original.periodStart} numeric />
                        <TableTextCell
                            value={`至 ${original.periodEnd}`}
                            numeric
                            className="block text-xs text-muted-foreground"
                        />
                    </div>
                ),
            },
            {
                id: "erpAmount",
                accessorFn: (row) => row.erpAmountGross,
                header: "ERP 金额（含税）",
                meta: {
                    label: "ERP 计算金额（含税）",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <MoneyValue
                        value={row.original.erpAmountGross}
                        display="exact"
                    />
                ),
            },
            {
                id: "supplierAmount",
                accessorFn: (row) => row.supplierAmountGross ?? "",
                header: "账单金额（含税）",
                meta: {
                    label: "供应商账单金额（含税）",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) =>
                    row.original.supplierAmountGross != null ? (
                        <MoneyValue
                            value={row.original.supplierAmountGross}
                            display="exact"
                        />
                    ) : (
                        <TableEmptyCell variant="label" label="账单未同步" />
                    ),
            },
            {
                id: "difference",
                accessorFn: (row) => row.differenceAmountGross ?? "",
                header: "差异（含税）",
                meta: {
                    label: "差异金额（含税）",
                    width: "amount",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <div className="text-right">
                        {row.original.differenceAmountGross != null ? (
                            <MoneyValue
                                value={row.original.differenceAmountGross}
                                display="exact"
                            />
                        ) : (
                            <TableEmptyCell />
                        )}
                        {row.original.differenceDirectionLabel ? (
                            <TableTextCell
                                value={row.original.differenceDirectionLabel}
                                className="block text-xs text-muted-foreground"
                            />
                        ) : null}
                        {row.original.unresolvedDifferenceCount > 0 ? (
                            <BusinessStatusBadge
                                context="list"
                                tone="warning"
                                label={`未决 ${row.original.unresolvedDifferenceCount}`}
                                className="mt-0.5"
                            />
                        ) : null}
                    </div>
                ),
            },
            {
                id: "status",
                accessorFn: (row) => row.statusLabel,
                header: "状态",
                meta: { label: "状态", width: "status" },
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={row.original.statusLabel}
                        tone={row.original.statusTone}
                        appearance={
                            row.original.status === "CONFIRMED"
                                ? "quiet"
                                : "badge"
                        }
                    />
                ),
            },
            {
                id: "actors",
                accessorFn: (row) =>
                    `${row.preparedByLabel}/${row.differenceHandlerLabel}/${row.reviewedByLabel}`,
                header: "对账/差异/复核",
                meta: { label: "对账/差异/复核" },
                cell: ({ row }) => (
                    <div className="text-xs text-muted-foreground">
                        <div>对账 {row.original.preparedByLabel}</div>
                        <div>差异 {row.original.differenceHandlerLabel}</div>
                        <div>复核 {row.original.reviewedByLabel}</div>
                    </div>
                ),
            },
            {
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "status", align: "end" },
                enableSorting: false,
                cell: ({ row }) => {
                    const segment = toAutomationIdSegment(
                        row.original.statementId,
                    )
                    return (
                        <TableRowActions
                            moreId={`supplier-settlements-list-row-${segment}-more`}
                            moreLabel={`${row.original.statementNo} 更多操作`}
                            actions={[
                                {
                                    id: `supplier-settlements-list-row-${segment}-preview`,
                                    label: "预览",
                                    icon: EyeIcon,
                                    onClick: () =>
                                        onPreview(row.original.statementId),
                                },
                                {
                                    id: `supplier-settlements-list-row-${segment}-open`,
                                    label: "打开",
                                    icon: ArrowUpRightIcon,
                                    onClick: () =>
                                        onOpen(row.original.statementId),
                                },
                            ]}
                        />
                    )
                },
            },
        ],
        [onOpen, onPreview],
    )
}
