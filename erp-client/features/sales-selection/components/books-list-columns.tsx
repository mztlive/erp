"use client"

import Link from "next/link"
import type { ColumnDef } from "@tanstack/react-table"

import {
    BusinessStatusBadge,
    QuantityValue,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
} from "@/components/business"
import {
    bookIdentity,
    bookletStatusTone,
    formatBookInstant,
} from "@/features/sales-selection/lib/presentation"
import type { SelectionBook } from "@/features/sales-selection/types"
import {
    BOOKLET_STATUS_LABEL,
    SELECTION_FORM_LABEL,
} from "@/features/sales-selection/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

const identityLinkClass =
    "block truncate rounded-sm underline-offset-4 hover:text-primary hover:underline focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary"
const rowActionPrefix = (book: SelectionBook) =>
    `sales-selection-row-${toAutomationIdSegment(bookIdentity(book))}`

export function buildBooksListColumns({
    canOpen,
    canOpenProposal,
    onCopy,
}: {
    canOpen: boolean
    canOpenProposal: boolean
    onCopy?: (book: SelectionBook) => void
}): ColumnDef<SelectionBook>[] {
    const columns: ColumnDef<SelectionBook>[] = [
        {
            id: "customer",
            accessorKey: "customer_name",
            header: "客户",
            meta: { label: "客户", width: "flex" },
            cell: ({ row: { original: book } }) => {
                const createdAt = formatBookInstant(book.created_at)
                const secondary =
                    createdAt !== "—" ? `创建于 ${createdAt}` : undefined
                return (
                    <TableIdentityCell
                        primary={
                            canOpen ? (
                                <Link
                                    id={`${rowActionPrefix(book)}-open`}
                                    href={`/sales/selection/${bookIdentity(book)}`}
                                    className={identityLinkClass}
                                    aria-label={`查看 ${book.customer_name} 的选品册`}
                                    onClick={(event) => event.stopPropagation()}
                                >
                                    {book.customer_name}
                                </Link>
                            ) : (
                                book.customer_name
                            )
                        }
                        primaryTitle={book.customer_name}
                        secondary={secondary}
                        secondaryTitle={secondary}
                        numericSecondary
                    />
                )
            },
        },
        {
            id: "owner",
            header: "负责销售",
            meta: { label: "负责销售", width: "default" },
            cell: ({ row }) => (
                <TableTextCell
                    value={row.original.sales_owner_name}
                    clamp={1}
                />
            ),
        },
        {
            id: "form",
            header: "形态",
            meta: { label: "形态", width: "status" },
            cell: ({ row: { original: book } }) => (
                <TableTextCell
                    value={
                        SELECTION_FORM_LABEL[book.selection_form ?? book.form]
                    }
                />
            ),
        },
        {
            id: "status",
            header: "状态",
            meta: { label: "状态", width: "status" },
            cell: ({
                row: {
                    original: { status },
                },
            }) => (
                <BusinessStatusBadge
                    context="list"
                    label={BOOKLET_STATUS_LABEL[status]}
                    tone={bookletStatusTone(status)}
                    appearance={
                        status === "PUBLISHED" || status === "SUBMITTED"
                            ? "quiet"
                            : "badge"
                    }
                />
            ),
        },
        {
            id: "display",
            header: "陈列",
            meta: {
                label: "陈列",
                width: "quantity",
                align: "end",
                numeric: true,
            },
            cell: ({
                row: {
                    original: { display_count },
                },
            }) =>
                display_count == null ? (
                    <TableEmptyCell />
                ) : (
                    <QuantityValue value={String(display_count)} unit="项" />
                ),
        },
        {
            id: "proposal",
            header: "方案",
            meta: { label: "方案", width: "default" },
            cell: ({ row: { original: book } }) => {
                if (!book.proposal_id || !book.proposal_no)
                    return <TableEmptyCell />
                if (!canOpenProposal)
                    return (
                        <TableTextCell
                            value={book.proposal_no}
                            numeric
                            clamp={1}
                        />
                    )
                return (
                    <Link
                        id={`${rowActionPrefix(book)}-proposal`}
                        href={`/sales/selection/proposals/${book.proposal_id}`}
                        className={identityLinkClass}
                        onClick={(event) => event.stopPropagation()}
                    >
                        <TableTextCell
                            value={book.proposal_no}
                            numeric
                            clamp={1}
                        />
                    </Link>
                )
            },
        },
        {
            id: "createdAt",
            header: "创建时间",
            meta: {
                label: "创建时间",
                width: "amount",
                align: "end",
                numeric: true,
            },
            cell: ({ row }) => {
                const value = formatBookInstant(row.original.created_at)
                return value === "—" ? (
                    <TableEmptyCell />
                ) : (
                    <TableTextCell value={value} numeric />
                )
            },
        },
    ]

    if (onCopy)
        columns.push({
            id: "actions",
            header: "操作",
            enableHiding: false,
            meta: { label: "操作", width: "status", align: "end" },
            cell: ({ row: { original: book } }) => (
                <TableRowActions
                    moreId={`${rowActionPrefix(book)}-more`}
                    moreLabel={`${book.customer_name}的选品册更多操作`}
                    actions={[
                        {
                            id: `${rowActionPrefix(book)}-copy`,
                            label: "复制",
                            disabled: book.status === "PREPARING",
                            disabledReason:
                                book.status === "PREPARING"
                                    ? "请准备完成后再复制"
                                    : undefined,
                            onClick: () => onCopy(book),
                        },
                    ]}
                />
            ),
        })
    return columns
}
