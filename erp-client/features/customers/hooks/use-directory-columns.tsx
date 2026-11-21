import * as React from "react"
import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    TableIdentityCell,
    TableTextCell,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { CustomerDirectoryItem } from "@/features/customers/types"

export function useCustomerDirectoryColumns(
    canOpenDetail: boolean,
): ColumnDef<CustomerDirectoryItem>[] {
    return React.useMemo<ColumnDef<CustomerDirectoryItem>[]>(
        () => [
            {
                id: "customer",
                accessorFn: (row) => row.shortName || row.legalName,
                header: "客户",
                meta: { label: "客户", width: "flex" },
                enableSorting: false,
                cell: ({ row }) => {
                    const {
                        id,
                        shortName,
                        legalName,
                        customerNo,
                        attentionTags,
                    } = row.original
                    const name = shortName || legalName
                    return (
                        <TableIdentityCell
                            primary={
                                canOpenDetail ? (
                                    <Link
                                        id={`customers-directory-row-${toAutomationIdSegment(id)}-open`}
                                        href={`/sales/customers/${id}`}
                                        className="underline-offset-4 hover:underline"
                                    >
                                        {name}
                                    </Link>
                                ) : (
                                    name
                                )
                            }
                            primaryTitle={name}
                            secondary={
                                attentionTags?.length ? (
                                    <span className="flex min-w-0 items-center gap-1.5">
                                        <span className="truncate">
                                            {customerNo}
                                        </span>
                                        {attentionTags.map((tag) => (
                                            <Badge key={tag} variant="outline">
                                                {tag}
                                            </Badge>
                                        ))}
                                    </span>
                                ) : (
                                    customerNo
                                )
                            }
                            secondaryTitle={[
                                customerNo,
                                ...(attentionTags ?? []),
                            ].join(" · ")}
                            numericSecondary
                        />
                    )
                },
            },
            {
                id: "owner",
                accessorKey: "ownerName",
                header: "负责销售",
                meta: { label: "负责销售", width: "default" },
                enableSorting: false,
                cell: ({ row }) => (
                    <TableTextCell value={row.original.ownerName} clamp={1} />
                ),
            },
            {
                id: "status",
                accessorFn: (row) => row.statusLabel.label,
                header: "状态",
                meta: { label: "状态", width: "status" },
                enableSorting: false,
                cell: ({ row }) => (
                    <BusinessStatusBadge
                        context="list"
                        appearance={
                            row.original.status === "active" ? "quiet" : "badge"
                        }
                        {...row.original.statusLabel}
                    />
                ),
            },
            {
                id: "business",
                accessorFn: (row) => row.updatedAt,
                header: "资料更新",
                sortDescFirst: true,
                meta: { label: "资料更新", width: "default", numeric: true },
                cell: ({ row }) => (
                    <TableTextCell
                        value={row.original.updatedAt.slice(0, 10)}
                    />
                ),
            },
        ],
        [canOpenDetail],
    )
}
