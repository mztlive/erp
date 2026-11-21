"use client"

import * as React from "react"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    QuantityValue,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import type { ContractListRow } from "@/features/contracts/types"
import { contractOwnerLabel } from "@/features/contracts/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function useContractListColumns(canReadDetail: boolean) {
    return React.useMemo<ColumnDef<ContractListRow>[]>(() => {
        const columns: ColumnDef<ContractListRow>[] = [
            {
                id: "contractNo",
                accessorKey: "contractNo",
                header: "合同编号",
                meta: { label: "合同编号", width: "flex" },
                cell: ({ row: { original } }) => {
                    const { contractNo, customer } = original
                    const customerNo =
                        customer.customerNo === customer.customerId
                            ? null
                            : customer.customerNo
                    const customerName =
                        customer.displayName === customer.customerId
                            ? "未标注客户"
                            : customer.displayName
                    const secondaryTitle = customerNo
                        ? `${customerName} · ${customerNo}`
                        : customerName
                    return (
                        <TableIdentityCell
                            primary={<span className="num">{contractNo}</span>}
                            primaryTitle={contractNo}
                            secondary={
                                <>
                                    {customerName}
                                    {customerNo ? (
                                        <>
                                            {" "}
                                            ·{" "}
                                            <span className="num">
                                                {customerNo}
                                            </span>
                                        </>
                                    ) : null}
                                </>
                            }
                            secondaryTitle={secondaryTitle}
                        />
                    )
                },
            },
            {
                id: "settlement",
                accessorFn: (row) => row.settlementParty.displayName,
                header: "结算主体",
                meta: { label: "结算主体", width: "default" },
                cell: ({
                    row: {
                        original: { settlementParty },
                    },
                }) => (
                    <TableTextCell
                        value={
                            settlementParty.displayName ===
                            settlementParty.partyId
                                ? null
                                : settlementParty.displayName
                        }
                        clamp={2}
                    />
                ),
            },
            {
                id: "validity",
                accessorKey: "validTo",
                header: "有效期",
                meta: { label: "有效期", width: "amount", numeric: true },
                cell: ({ row: { original } }) => (
                    <div>
                        <TableTextCell
                            value={`${original.validFrom} ~ ${original.validTo}`}
                            numeric
                        />
                        {original.expiringWithin30Days ? (
                            <div className="text-xs text-warning-soft-foreground">
                                将到期
                            </div>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "status",
                header: "状态",
                meta: { label: "状态", width: "status" },
                enableSorting: false,
                cell: ({ row: { original } }) => (
                    <BusinessStatusBadge
                        context="list"
                        label={original.statusLabel}
                        tone={original.statusTone}
                        appearance={
                            original.status === "EFFECTIVE" &&
                            !original.expiringWithin30Days
                                ? "quiet"
                                : "badge"
                        }
                    />
                ),
            },
            {
                id: "owner",
                accessorKey: "ownerLabel",
                header: "负责人",
                meta: { label: "负责人", width: "default" },
                cell: ({ row }) => {
                    const owner = contractOwnerLabel(row.original.ownerLabel)
                    return (
                        <TableTextCell
                            value={owner === "未指定" ? null : owner}
                            emptyVariant="label"
                            emptyLabel="未指定"
                            clamp={1}
                        />
                    )
                },
            },
            {
                id: "sales",
                header: "销售单",
                meta: {
                    label: "关联销售单",
                    width: "quantity",
                    align: "end",
                    numeric: true,
                },
                enableSorting: false,
                cell: ({ row: { original } }) => (
                    <div>
                        <QuantityValue
                            value={String(original.salesOrderCount)}
                            unit="张"
                        />
                        {original.activeSalesOrderCount > 0 ? (
                            <div className="text-xs text-muted-foreground">
                                进行中{" "}
                                <span className="num">
                                    {original.activeSalesOrderCount}
                                </span>
                            </div>
                        ) : null}
                    </div>
                ),
            },
            {
                id: "revision",
                accessorKey: "revisionNo",
                header: "版本",
                meta: {
                    label: "版本",
                    width: "quantity",
                    align: "end",
                    numeric: true,
                },
                cell: ({ row }) => (
                    <TableTextCell
                        value={`v${row.original.revisionNo}`}
                        numeric
                    />
                ),
            },
        ]
        if (canReadDetail) {
            columns.push({
                id: "actions",
                header: "操作",
                meta: { label: "操作", width: "status", align: "end" },
                enableSorting: false,
                cell: ({ row: { original } }) => {
                    const prefix = `card-contracts-list-row-${toAutomationIdSegment(original.contractId)}`
                    return (
                        <TableRowActions
                            maxInline={1}
                            moreId={`${prefix}-more`}
                            moreLabel={`合同 ${original.contractNo} 更多操作`}
                            actions={[
                                {
                                    id: `${prefix}-open`,
                                    label: (
                                        <>
                                            <span>打开合同</span>
                                            <span className="sr-only">
                                                {" "}
                                                {original.contractNo}
                                            </span>
                                        </>
                                    ),
                                    href: `/sales/contracts/${original.contractId}`,
                                },
                            ]}
                        />
                    )
                },
            })
        }
        return columns
    }, [canReadDetail])
}
