"use client"

import { useState, type ReactNode } from "react"
import { FileTextIcon } from "lucide-react"

import {
    APPROVAL_TABLE_DEFAULT_PAGE_SIZE,
    ApprovalLineFilterBar,
    ApprovalLinePagination,
    MoneyValue,
    WorkspaceFactGrid,
    WorkspaceFactItem,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    displayDateString,
    displayUnixDate,
} from "@/features/approval-workflow/display"
import {
    canonicalInvoiceDecimal,
    INVOICE_QUANTITY_SCALE,
    INVOICE_UNIT_PRICE_SCALE,
    invoiceLineAmount,
    isPositiveInvoiceDecimal,
} from "@/features/invoice-requests/line-amounts"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayName } from "@/lib/display-name"
import { formatFixedDisplay } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { InvoiceApprovalReview } from "../api/invoice-review"
import {
    invoiceApprovalQuota,
    invoiceSourceNumber,
    type InvoiceSource,
    type invoiceApprovalSubmission,
} from "./workspace-invoice-approval-model"

type EvidenceColumn = { label: string; numeric?: boolean }
type EvidenceRow = {
    key: string
    cells: ReactNode[]
    search: string
    current?: boolean
}

const amountColumn = { label: "本笔净分配", numeric: true }

function EvidenceTable({
    id,
    title,
    columns,
    rows,
    totalCount = rows.length,
    total,
    totalLabel = "全部记录合计",
    emptyMessage = "暂无已登记记录",
}: {
    id: string
    title: string
    columns: readonly EvidenceColumn[]
    rows: readonly EvidenceRow[]
    totalCount?: number
    total?: ReactNode
    totalLabel?: string
    emptyMessage?: string
}) {
    const [search, setSearch] = useState("")
    const [page, setPage] = useState(1)
    const [pageSize, setPageSize] = useState(APPROVAL_TABLE_DEFAULT_PAGE_SIZE)
    const query = search.trim().toLocaleLowerCase()
    const filtered = query
        ? rows.filter((row) => row.search.toLocaleLowerCase().includes(query))
        : rows
    const currentPage = Math.min(
        page,
        Math.max(1, Math.ceil(filtered.length / pageSize)),
    )
    const visible = filtered.slice(
        (currentPage - 1) * pageSize,
        currentPage * pageSize,
    )
    const truncated = totalCount > rows.length

    const updateSearch = (value: string) => {
        setSearch(value)
        setPage(1)
    }

    return (
        <section aria-label={title} className="min-w-0 space-y-2">
            <div className="flex flex-wrap items-baseline justify-between gap-2">
                <h3 className="text-body-compact font-semibold">{title}</h3>
                {truncated ? (
                    <p className="num text-xs text-muted-foreground">
                        展示 {rows.length} / {totalCount} 项
                        {query ? "，仅筛选已展示记录" : ""}
                    </p>
                ) : null}
            </div>
            <ApprovalLineFilterBar
                idPrefix={id}
                totalCount={rows.length}
                filteredCount={filtered.length}
                riskCount={0}
                hasHighRisk={false}
                riskOnly={false}
                onRiskOnlyChange={() => undefined}
                search={search}
                onSearchChange={updateSearch}
                onClearFilters={() => updateSearch("")}
                searchPlaceholder="搜索明细"
                ariaLabel={`${title}筛选`}
            />
            {rows.length ? (
                <Table
                    aria-label={title}
                    className="table-fixed border-separate border-spacing-0 [&_:is(th,td)]:px-2 [&_:is(th,td)]:whitespace-normal [&_:is(th,td)]:wrap-anywhere"
                >
                    <TableHeader className="bg-surface-sunken [&_th]:bg-surface-sunken [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md [&_tr]:border-b-0">
                        <TableRow>
                            {columns.map(({ label, numeric }) => (
                                <TableHead
                                    key={label}
                                    scope="col"
                                    data-align={numeric ? "end" : undefined}
                                >
                                    {label}
                                </TableHead>
                            ))}
                        </TableRow>
                    </TableHeader>
                    <TableBody className="[&_tr:not(:last-child)>td]:border-b [&_tr:not(:last-child)>td]:border-border/60">
                        {visible.map((row) => (
                            <TableRow
                                key={row.key}
                                className={
                                    row.current ? "bg-info-soft/30" : undefined
                                }
                            >
                                {row.cells.map((value, index) => (
                                    <TableCell
                                        key={columns[index].label}
                                        data-align={
                                            columns[index].numeric
                                                ? "end"
                                                : undefined
                                        }
                                        className={
                                            index === 0
                                                ? "font-medium"
                                                : undefined
                                        }
                                    >
                                        {value}
                                    </TableCell>
                                ))}
                            </TableRow>
                        ))}
                        {!visible.length ? (
                            <TableRow>
                                <TableCell
                                    colSpan={columns.length}
                                    className="text-xs text-muted-foreground"
                                >
                                    未找到匹配记录
                                </TableCell>
                            </TableRow>
                        ) : null}
                    </TableBody>
                    {total != null ? (
                        <TableFooter className="border-t-0 bg-surface-sunken [&_td:first-child]:rounded-l-md [&_td:last-child]:rounded-r-md">
                            <TableRow className="border-0 hover:bg-transparent">
                                <TableCell
                                    colSpan={columns.length - 1}
                                    className="text-xs"
                                >
                                    {totalLabel}
                                    {query ||
                                    visible.length < rows.length ||
                                    truncated ? (
                                        <span className="num ml-1.5 text-tiny font-normal text-muted-foreground">
                                            （当前显示 {visible.length}{" "}
                                            项，合计按整单）
                                        </span>
                                    ) : null}
                                </TableCell>
                                <TableCell data-align="end">{total}</TableCell>
                            </TableRow>
                        </TableFooter>
                    ) : null}
                </Table>
            ) : (
                <p className="text-xs text-muted-foreground">{emptyMessage}</p>
            )}
            <ApprovalLinePagination
                idPrefix={id}
                ariaLabel={`${title}分页`}
                filteredCount={filtered.length}
                page={currentPage}
                pageSize={pageSize}
                onPageChange={setPage}
                onPageSizeChange={(size) => {
                    setPageSize(size)
                    setPage(1)
                }}
            />
        </section>
    )
}

function EvidenceAmount({ value }: { value?: string | null }) {
    return (
        <MoneyValue
            value={value}
            display="exact"
            className="[&_[data-slot=money-value-number]]:whitespace-normal [&_[data-slot=money-value-number]]:wrap-anywhere"
        />
    )
}

function detailDecimal(value: string, maxScale: number) {
    return isPositiveInvoiceDecimal(value, maxScale)
        ? formatFixedDisplay(canonicalInvoiceDecimal(value), { maxScale })
        : "—"
}

export function InvoiceRequestEvidence({
    prefix,
    submission,
}: {
    prefix: string
    submission: ReturnType<typeof invoiceApprovalSubmission>
}) {
    return (
        <EvidenceTable
            id={`${prefix}-request-lines`}
            title="本次申请明细"
            columns={[
                { label: "开票内容" },
                { label: "数量", numeric: true },
                { label: "含税单价", numeric: true },
                { label: "行金额", numeric: true },
            ]}
            rows={submission.lines.map((line, index) => ({
                key: `${index}:${line.content}`,
                search: [line.content, line.quantity, line.unit_price].join(
                    " ",
                ),
                cells: [
                    <span key="content" className="whitespace-pre-wrap">
                        {line.content.trim() || "—"}
                    </span>,
                    <span key="quantity" className="num">
                        {detailDecimal(line.quantity, INVOICE_QUANTITY_SCALE)}
                    </span>,
                    isPositiveInvoiceDecimal(
                        line.unit_price,
                        INVOICE_UNIT_PRICE_SCALE,
                    ) ? (
                        <EvidenceAmount
                            key="unit-price"
                            value={canonicalInvoiceDecimal(line.unit_price)}
                        />
                    ) : (
                        <span key="unit-price" className="num">
                            —
                        </span>
                    ),
                    <EvidenceAmount
                        key="amount"
                        value={invoiceLineAmount(line)}
                    />,
                ],
            }))}
            total={
                submission.lineTotal != null ? (
                    <EvidenceAmount value={submission.lineTotal} />
                ) : undefined
            }
            totalLabel="申请明细合计"
            emptyMessage="未记录开票明细"
        />
    )
}

export function InvoiceSourceEvidence({
    prefix,
    source,
    frozen,
    onOpen,
}: {
    prefix: string
    source: InvoiceSource
    frozen: boolean
    onOpen: () => void
}) {
    const id = `${prefix}-${frozen ? "submitted" : "current"}-sales-${toAutomationIdSegment(source.revision_id)}`
    const lines = source.source.lines

    return (
        <section
            className="min-w-0 space-y-2.5"
            aria-label={frozen ? "提交时销售依据" : "当前销售依据"}
        >
            <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
                <Button
                    id={`${id}-preview`}
                    type="button"
                    variant="ghost"
                    size="xs"
                    className={cn(
                        detailWorkspaceStyles.inlineLinkButton,
                        "h-auto max-w-full text-left whitespace-normal wrap-anywhere",
                    )}
                    onClick={onOpen}
                >
                    <FileTextIcon aria-hidden="true" />
                    <span>
                        预览销售单{" "}
                        <span className="num">
                            {invoiceSourceNumber(source)} · V
                            {source.revision_no}
                        </span>
                    </span>
                </Button>
                <StatusBadge
                    tone={frozen ? "neutral" : "info"}
                    label={frozen ? "提交时版本" : "当前生效版本"}
                />
            </div>
            <WorkspaceFactGrid
                columns={3}
                ariaLabel={frozen ? "提交时销售单信息" : "当前销售单信息"}
            >
                {displayName(source.source.customer) ? (
                    <WorkspaceFactItem
                        label="销售客户"
                        value={displayName(source.source.customer)}
                    />
                ) : null}
                {source.source.extra_sections
                    .filter((field) => field.value.trim() !== "")
                    .map((field, index) => {
                        const isLong =
                            field.value.length > 30 ||
                            field.value.includes("\n") ||
                            /(?:说明|原因|备注|事由|地址)/.test(field.label)
                        return (
                            <WorkspaceFactItem
                                key={`${field.label}:${index}`}
                                label={field.label}
                                value={field.value}
                                numeric={field.numeric}
                                selectable={field.numeric}
                                span={isLong ? "full" : 1}
                            />
                        )
                    })}
            </WorkspaceFactGrid>
            <EvidenceTable
                id={`${id}-lines`}
                title="销售单明细"
                columns={[
                    { label: "商品或服务" },
                    { label: "数量", numeric: true },
                    { label: "金额与交期", numeric: true },
                ]}
                rows={lines.map((line, index) => ({
                    key: `${index}:${line.title}`,
                    search: [line.title, line.quantity, line.due_label].join(
                        " ",
                    ),
                    cells: [
                        line.title,
                        <span key="quantity" className="num">
                            {line.quantity ?? "—"}
                        </span>,
                        <span key="due" className="num">
                            {line.due_label ?? "—"}
                        </span>,
                    ],
                }))}
                totalCount={lines.length + source.source.more_count}
                total={
                    source.source.amount_label ? (
                        <span className="num font-semibold">
                            {source.source.amount_label}
                        </span>
                    ) : undefined
                }
                totalLabel="销售整单含税"
                emptyMessage="未记录销售明细"
            />
        </section>
    )
}

export function InvoiceReceiptEvidence({
    prefix,
    review,
}: {
    prefix: string
    review: InvoiceApprovalReview
}) {
    return (
        <>
            <WorkspaceFactGrid columns={3} ariaLabel="销售与回款概览">
                <WorkspaceFactItem label="本笔未回款" numeric>
                    <EvidenceAmount value={review.open_total} />
                </WorkspaceFactItem>
                {review.sales_status ? (
                    <WorkspaceFactItem
                        label="销售状态"
                        value={review.sales_status}
                    />
                ) : null}
                {review.fulfillment_status ? (
                    <WorkspaceFactItem
                        label="履约进度"
                        value={review.fulfillment_status}
                    />
                ) : null}
                {displayName(review.sales_owner) ? (
                    <WorkspaceFactItem
                        label="负责销售"
                        value={displayName(review.sales_owner)}
                    />
                ) : null}
            </WorkspaceFactGrid>
            <p className="text-xs text-muted-foreground">
                回款按本笔应收核销金额统计。开票时点请核对合同约定。
            </p>
            <EvidenceTable
                id={`${prefix}-receipts-table`}
                title="相关回款记录"
                columns={[
                    { label: "回款单号" },
                    { label: "状态" },
                    { label: "日期" },
                    amountColumn,
                ]}
                rows={recordRows(review.records.receipts)}
                totalCount={review.records.receipt_count}
                total={<EvidenceAmount value={review.settled_total} />}
                totalLabel="本笔已核销回款"
            />
        </>
    )
}

function recordRows(
    records: InvoiceApprovalReview["records"]["receipts"],
): EvidenceRow[] {
    return records.map((row, index) => ({
        key: `${row.number}:${row.date}:${index}`,
        search: [row.number, row.status, row.date, row.amount].join(" "),
        cells: [
            <span key="number" className="num">
                {row.number}
            </span>,
            row.status,
            <span key="date" className="num">
                {displayDateString(row.date)}
            </span>,
            <EvidenceAmount key="amount" value={row.amount} />,
        ],
    }))
}

export function InvoiceHistoryEvidence({
    prefix,
    review,
    quotaEvidence,
}: {
    prefix: string
    review: InvoiceApprovalReview
    quotaEvidence: ReactNode
}) {
    return (
        <>
            {quotaEvidence}
            <EvidenceTable
                id={`${prefix}-reservations-table`}
                title="占用额度的开票申请"
                columns={[
                    { label: "申请单号" },
                    { label: "状态" },
                    { label: "日期" },
                    { label: "申请额", numeric: true },
                    { label: "已开额", numeric: true },
                ]}
                rows={review.requests.map((row) => {
                    const date = displayUnixDate(row.created_at)
                    return {
                        key: row.request_no,
                        current: row.is_current,
                        search: [
                            row.request_no,
                            row.status,
                            date,
                            row.amount,
                            row.invoiced_amount,
                            row.is_current ? "本次申请" : "",
                        ].join(" "),
                        cells: [
                            <span
                                key="number"
                                className="flex flex-wrap items-center gap-1"
                            >
                                <span className="num">{row.request_no}</span>
                                {row.is_current ? (
                                    <StatusBadge tone="info" label="本次申请" />
                                ) : null}
                            </span>,
                            row.status,
                            <span key="date" className="num">
                                {date}
                            </span>,
                            <EvidenceAmount key="amount" value={row.amount} />,
                            <EvidenceAmount
                                key="invoiced"
                                value={row.invoiced_amount}
                            />,
                        ],
                    }
                })}
                totalCount={review.request_count}
                total={
                    <EvidenceAmount
                        value={invoiceApprovalQuota(review).reserved}
                    />
                }
                totalLabel="有效占用合计（尚未开票部分）"
                emptyMessage="暂无占用额度的开票申请"
            />
            <EvidenceTable
                id={`${prefix}-invoices-table`}
                title="已开发票记录"
                columns={[
                    { label: "发票单号 / 编号" },
                    { label: "状态" },
                    { label: "日期" },
                    amountColumn,
                ]}
                rows={recordRows(review.records.invoices)}
                totalCount={review.records.invoice_count}
                total={<EvidenceAmount value={review.invoiced_total} />}
                totalLabel="本笔已开票净额"
            />
        </>
    )
}
