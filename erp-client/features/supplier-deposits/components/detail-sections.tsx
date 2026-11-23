"use client"

import { useMemo, useState } from "react"
import Link from "next/link"
import { format } from "date-fns"
import { ArrowUpRightIcon, DownloadIcon, SearchIcon, XIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessStatusBadge,
    DetailWorkspaceSection,
    DetailWorkspaceSubTabs,
    MoneyValue,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    TableTextCell,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Progress } from "@/components/ui/progress"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import type { StatusTone } from "@/components/ui/status-badge"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { downloadBlob } from "@/lib/download-blob"
import {
    compareDecimal,
    parseDecimal,
    subtractFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

import { reservationRemainder } from "../lib/resolution"
import type {
    AccountDetail,
    DepositAuthorization,
    DepositBatch,
    DepositLedgerEntry,
    DepositReconciliation,
    DepositReservation,
    RechargeRequest,
} from "../types"
import type { DepositAction } from "./command-dialog"
import {
    DepositBankReceiptAction,
    DepositResolutionEvidence,
} from "./resolution-evidence"
import type { DepositResolutionTarget } from "./resolution-dialog"

export type DepositDetailDialogTarget =
    | {
          action: DepositAction
          request?: RechargeRequest
          authorization?: DepositAuthorization
      }
    | { action: "pay"; request: RechargeRequest }
    | DepositResolutionTarget

const MONEY_OPTIONS = { maxScale: 2, outputScale: 2 } as const
const DEFAULT_PAGE_SIZE = 20

export function displayDepositTime(value: number | undefined | null) {
    return value == null
        ? "未标注"
        : format(new Date(value * 1000), "yyyy-MM-dd HH:mm")
}

function escapeCsvCell(value: string): string {
    if (/[",\r\n]/.test(value)) {
        return `"${value.replaceAll('"', '""')}"`
    }
    return value
}

function exportCsvFile(
    filename: string,
    headers: readonly string[],
    rows: readonly (readonly string[])[],
) {
    const lines = [
        headers.map(escapeCsvCell).join(","),
        ...rows.map((row) => row.map(escapeCsvCell).join(",")),
    ]
    const blob = new Blob(["\uFEFF" + lines.join("\r\n")], {
        type: "text/csv;charset=utf-8",
    })
    downloadBlob(blob, filename)
}

function matchesKeyword(
    keyword: string,
    fields: readonly (string | null | undefined)[],
): boolean {
    const needle = keyword.trim().toLowerCase()
    if (!needle) return true
    return fields.some((field) => field?.toLowerCase().includes(needle))
}

const RECHARGE_STATUS: Record<
    string,
    { label: string; tone: StatusTone; quiet?: boolean }
> = {
    draft: { label: "待批准", tone: "warning" },
    requested: { label: "待批准", tone: "warning" },
    pending: { label: "待批准", tone: "warning" },
    approved: { label: "待银行付款", tone: "info" },
    paid: { label: "待核对到账", tone: "info" },
    bank_paid: { label: "待核对到账", tone: "info" },
    received: { label: "已到账", tone: "success", quiet: true },
    reversed: { label: "已冲正", tone: "void" },
}

const EXPOSURE_STATUS: Record<
    DepositReservation["exposure"],
    { label: string; tone: StatusTone; quiet?: boolean }
> = {
    not_sent: { label: "尚未发送订单", tone: "neutral", quiet: true },
    possibly_sent: { label: "扣款结果待核实", tone: "destructive" },
    cleared: { label: "已核实无扣款风险", tone: "success", quiet: true },
}

const LEDGER_KIND: Record<
    DepositLedgerEntry["kind"],
    { label: string; tone: StatusTone; inflow: boolean }
> = {
    recharge: { label: "充值到账", tone: "success", inflow: true },
    return: { label: "账户返款", tone: "info", inflow: true },
    debit: { label: "订单扣款", tone: "neutral", inflow: false },
    recharge_reversal: { label: "充值冲正", tone: "warning", inflow: false },
}

function utilizationPercent(committed: string, limit: string): number {
    try {
        const limitNum = parseDecimal(limit, { maxScale: 2 }).unscaled
        if (limitNum <= BigInt(0)) return 0
        const committedNum = parseDecimal(committed, { maxScale: 2 }).unscaled
        if (committedNum <= BigInt(0)) return 0
        const pct = Number((committedNum * BigInt(100)) / limitNum)
        return Math.max(0, Math.min(100, pct))
    } catch {
        return 0
    }
}

function LocalTableToolbar({
    idPrefix,
    keyword,
    onKeywordChange,
    searchPlaceholder,
    filterValue,
    onFilterChange,
    filterOptions,
    filterAriaLabel,
    totalCount,
    filteredCount,
    truncatedHint,
    onExport,
    exportDisabled,
}: {
    idPrefix: string
    keyword: string
    onKeywordChange: (next: string) => void
    searchPlaceholder: string
    filterValue?: string
    onFilterChange?: (next: string) => void
    filterOptions?: readonly { value: string; label: string }[]
    filterAriaLabel?: string
    totalCount: number
    filteredCount: number
    truncatedHint?: boolean
    onExport?: () => void
    exportDisabled?: boolean
}) {
    const hasFilter =
        Boolean(keyword.trim()) ||
        (filterValue != null && filterValue !== "all")

    return (
        <div className="flex flex-wrap items-center justify-between gap-2.5">
            <div className="flex min-w-0 flex-1 flex-wrap items-center gap-2">
                <div className="relative w-full max-w-xs min-w-48">
                    <SearchIcon
                        aria-hidden="true"
                        className="pointer-events-none absolute top-1/2 left-2.5 size-3.5 -translate-y-1/2 text-muted-foreground"
                    />
                    <Input
                        id={`${idPrefix}-search`}
                        value={keyword}
                        onChange={(event) =>
                            onKeywordChange(event.target.value)
                        }
                        placeholder={searchPlaceholder}
                        aria-label={searchPlaceholder}
                        className="h-control-sm pr-7 pl-8"
                    />
                    {keyword ? (
                        <Button
                            id={`${idPrefix}-search-clear`}
                            type="button"
                            variant="ghost"
                            size="icon-xs"
                            aria-label="清空搜索"
                            className="absolute top-1/2 right-1 -translate-y-1/2 text-muted-foreground hover:text-foreground"
                            onClick={() => onKeywordChange("")}
                        >
                            <XIcon className="size-3.5" aria-hidden="true" />
                        </Button>
                    ) : null}
                </div>
                {filterOptions && onFilterChange && filterValue != null ? (
                    <Select
                        value={filterValue}
                        onValueChange={(value) => {
                            if (value) onFilterChange(value)
                        }}
                    >
                        <SelectTrigger
                            id={`${idPrefix}-filter`}
                            size="sm"
                            aria-label={filterAriaLabel ?? "筛选状态"}
                            className="min-w-36"
                        >
                            <SelectValue />
                        </SelectTrigger>
                        <SelectContent>
                            {filterOptions.map((option) => (
                                <SelectItem
                                    key={option.value}
                                    value={option.value}
                                    idPrefix={`${idPrefix}-filter`}
                                >
                                    {option.label}
                                </SelectItem>
                            ))}
                        </SelectContent>
                    </Select>
                ) : null}
                {hasFilter ? (
                    <Button
                        id={`${idPrefix}-reset-filter`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        className={detailWorkspaceStyles.quietButton}
                        onClick={() => {
                            onKeywordChange("")
                            onFilterChange?.("all")
                        }}
                    >
                        重置筛选
                    </Button>
                ) : null}
            </div>
            <div className="flex shrink-0 flex-wrap items-center gap-3">
                <span className="num text-xs text-muted-foreground">
                    {hasFilter
                        ? `命中 ${filteredCount} / 已载入 ${totalCount} 条`
                        : `共 ${totalCount} 条`}
                    {truncatedHint ? "（最多返回最近 100 条）" : null}
                </span>
                {onExport ? (
                    <Button
                        id={`${idPrefix}-export`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        disabled={exportDisabled || filteredCount === 0}
                        onClick={onExport}
                    >
                        <DownloadIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        导出 CSV
                    </Button>
                ) : null}
            </div>
        </div>
    )
}

function LocalPaginationBar({
    idPrefix,
    page,
    pageCount,
    totalCount,
    pageSize,
    onPageChange,
}: {
    idPrefix: string
    page: number
    pageCount: number
    totalCount: number
    pageSize: number
    onPageChange: (next: number) => void
}) {
    if (totalCount <= pageSize) return null
    const start = (page - 1) * pageSize + 1
    const end = Math.min(page * pageSize, totalCount)
    return (
        <div className="flex flex-wrap items-center justify-between gap-2 pt-1 text-xs text-muted-foreground">
            <span className="num">
                显示第 {start}–{end} 条，共 {totalCount} 条（第 {page} /{" "}
                {pageCount} 页）
            </span>
            <div className="flex items-center gap-1.5">
                <Button
                    id={`${idPrefix}-page-prev`}
                    type="button"
                    variant="outline"
                    size="xs"
                    disabled={page <= 1}
                    onClick={() => onPageChange(page - 1)}
                >
                    上一页
                </Button>
                <Button
                    id={`${idPrefix}-page-next`}
                    type="button"
                    variant="outline"
                    size="xs"
                    disabled={page >= pageCount}
                    onClick={() => onPageChange(page + 1)}
                >
                    下一页
                </Button>
            </div>
        </div>
    )
}

function TableShell({
    children,
    empty,
    isFiltered = false,
    emptyTitle,
    emptyDescription,
    onResetFilter,
    resetFilterId,
}: {
    children: React.ReactNode
    empty: boolean
    isFiltered?: boolean
    emptyTitle: string
    emptyDescription?: string
    onResetFilter?: () => void
    resetFilterId?: string
}) {
    if (empty) {
        return (
            <BusinessEmptyState
                kind={isFiltered ? "filter" : "no-data"}
                title={isFiltered ? "当前筛选无匹配记录" : emptyTitle}
                description={
                    isFiltered
                        ? "请调整关键词或筛选条件后重试。"
                        : emptyDescription
                }
                action={
                    isFiltered && onResetFilter ? (
                        <Button
                            id={resetFilterId}
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={onResetFilter}
                        >
                            清除筛选
                        </Button>
                    ) : undefined
                }
            />
        )
    }
    return (
        <div
            data-slot="detail-inline-table"
            className={cn(
                detailWorkspaceStyles.inlineTableWrapper,
                "overflow-x-auto",
            )}
        >
            <Table>{children}</Table>
        </div>
    )
}

export function DepositRechargesSection({
    requests,
    batches = [],
    permitted,
    onOpenDialog,
    actions,
}: {
    requests: readonly RechargeRequest[]
    batches?: readonly DepositBatch[]
    permitted: (action: string) => boolean
    onOpenDialog: (dialog: DepositDetailDialogTarget) => void
    actions?: React.ReactNode
}) {
    const [keyword, setKeyword] = useState("")
    const [statusFilter, setStatusFilter] = useState("all")
    const [page, setPage] = useState(1)

    const receiptByBankRef = useMemo(() => {
        const map = new Map<string, string>()
        for (const batch of batches) {
            if (
                batch.evidence.bank_reference &&
                batch.evidence.bank_receipt_asset_id
            ) {
                map.set(
                    batch.evidence.bank_reference,
                    batch.evidence.bank_receipt_asset_id,
                )
            }
        }
        return map
    }, [batches])

    const filtered = useMemo(() => {
        return requests.filter((request) => {
            const status = request.status.toLowerCase()
            if (statusFilter !== "all") {
                if (
                    statusFilter === "pending" &&
                    !["draft", "requested", "pending"].includes(status)
                ) {
                    return false
                }
                if (statusFilter === "approved" && status !== "approved") {
                    return false
                }
                if (
                    statusFilter === "paid" &&
                    !["paid", "bank_paid"].includes(status)
                ) {
                    return false
                }
                if (statusFilter === "received" && status !== "received") {
                    return false
                }
                if (statusFilter === "reversed" && status !== "reversed") {
                    return false
                }
            }
            return matchesKeyword(keyword, [
                request.request_no,
                request.reason,
                request.approval_reference,
                request.bank_reference,
                request.amount,
            ])
        })
    }, [keyword, requests, statusFilter])

    const pageCount = Math.max(
        1,
        Math.ceil(filtered.length / DEFAULT_PAGE_SIZE),
    )
    const safePage = Math.min(page, pageCount)
    const paged = filtered.slice(
        (safePage - 1) * DEFAULT_PAGE_SIZE,
        safePage * DEFAULT_PAGE_SIZE,
    )
    const isFiltered = Boolean(keyword.trim()) || statusFilter !== "all"

    return (
        <DetailWorkspaceSection
            title="充值办理"
            subtitle={`共 ${requests.length} 条`}
            width="full"
            actions={actions}
        >
            {requests.length > 0 ? (
                <LocalTableToolbar
                    idPrefix="supplier-deposits-recharges"
                    keyword={keyword}
                    onKeywordChange={(next) => {
                        setKeyword(next)
                        setPage(1)
                    }}
                    searchPlaceholder="搜索申请单号、用途、批准依据或银行流水"
                    filterValue={statusFilter}
                    onFilterChange={(next) => {
                        setStatusFilter(next)
                        setPage(1)
                    }}
                    filterAriaLabel="筛选充值办理进度"
                    filterOptions={[
                        { value: "all", label: "全部进度" },
                        { value: "pending", label: "待批准" },
                        { value: "approved", label: "待银行付款" },
                        { value: "paid", label: "待核对到账" },
                        { value: "received", label: "已到账" },
                        { value: "reversed", label: "已冲正" },
                    ]}
                    totalCount={requests.length}
                    filteredCount={filtered.length}
                    truncatedHint={requests.length >= 100}
                    onExport={() =>
                        exportCsvFile(
                            "预存款充值记录.csv",
                            [
                                "申请单号",
                                "充值用途",
                                "申请金额",
                                "办理进度",
                                "批准依据",
                                "银行流水号",
                            ],
                            filtered.map((item) => [
                                item.request_no || "充值申请",
                                item.reason,
                                item.amount,
                                RECHARGE_STATUS[item.status.toLowerCase()]
                                    ?.label ?? "需核对申请进度",
                                item.approval_reference || "",
                                item.bank_reference || "",
                            ]),
                        )
                    }
                />
            ) : null}

            <TableShell
                empty={paged.length === 0}
                isFiltered={isFiltered}
                emptyTitle="暂无充值申请"
                emptyDescription="点击右上角「申请充值」发起预存款充值。"
                resetFilterId="supplier-deposits-recharges-empty-reset"
                onResetFilter={() => {
                    setKeyword("")
                    setStatusFilter("all")
                    setPage(1)
                }}
            >
                <TableHeader>
                    <TableRow>
                        <TableHead>申请单号 / 用途</TableHead>
                        <TableHead data-align="end">申请金额</TableHead>
                        <TableHead>办理进度</TableHead>
                        <TableHead>批准及付款依据</TableHead>
                        <TableHead data-align="end">操作</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {paged.map((request) => {
                        const status = request.status.toLowerCase()
                        const statusMeta = RECHARGE_STATUS[status] ?? {
                            label: "需核对申请进度",
                            tone: "warning" as const,
                        }
                        const segment = toAutomationIdSegment(request.id)
                        const receiptAssetId =
                            request.bank_receipt_asset_id ??
                            (request.bank_reference
                                ? receiptByBankRef.get(request.bank_reference)
                                : undefined)
                        const nextAction =
                            ["draft", "pending", "requested"].includes(
                                status,
                            ) && permitted("approve_recharge")
                                ? {
                                      id: `supplier-deposits-approve-${segment}`,
                                      label: "核对并批准",
                                      onClick: () =>
                                          onOpenDialog({
                                              action: "approve",
                                              request,
                                          }),
                                  }
                                : status === "approved" &&
                                    permitted("pay_recharge")
                                  ? {
                                        id: `supplier-deposits-pay-${segment}`,
                                        label: "登记银行付款",
                                        onClick: () =>
                                            onOpenDialog({
                                                action: "pay",
                                                request,
                                            }),
                                    }
                                  : ["paid", "bank_paid"].includes(status) &&
                                      permitted("receive_recharge")
                                    ? {
                                          id: `supplier-deposits-receive-${segment}`,
                                          label: "核对供应商到账",
                                          onClick: () =>
                                              onOpenDialog({
                                                  action: "receive",
                                                  request,
                                              }),
                                      }
                                    : null
                        return (
                            <TableRow key={request.id}>
                                <TableCell>
                                    <TableIdentityCell
                                        primary={
                                            <span className="num">
                                                {request.request_no ||
                                                    "充值申请"}
                                            </span>
                                        }
                                        primaryTitle={
                                            request.request_no || "充值申请"
                                        }
                                        secondary={request.reason}
                                        secondaryTitle={request.reason}
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={request.amount}
                                        display="exact"
                                        className="font-medium"
                                    />
                                </TableCell>
                                <TableCell>
                                    <BusinessStatusBadge
                                        context="list"
                                        label={statusMeta.label}
                                        tone={statusMeta.tone}
                                        appearance={
                                            statusMeta.quiet ? "quiet" : "badge"
                                        }
                                    />
                                </TableCell>
                                <TableCell>
                                    <div className="flex min-w-0 flex-col gap-0.5">
                                        <TableTextCell
                                            value={
                                                request.approval_reference ||
                                                "尚未批准"
                                            }
                                            className={
                                                request.approval_reference
                                                    ? undefined
                                                    : "text-muted-foreground"
                                            }
                                        />
                                        <span className="num text-xs text-muted-foreground">
                                            {request.bank_reference
                                                ? `银行流水：${request.bank_reference}`
                                                : request.supplier_payment_id
                                                  ? "银行付款已登记"
                                                  : "尚未登记银行付款"}
                                        </span>
                                        {receiptAssetId ? (
                                            <DepositBankReceiptAction
                                                recordId={request.id}
                                                assetId={receiptAssetId}
                                                idPrefix="supplier-deposits-recharge-receipt"
                                            />
                                        ) : null}
                                    </div>
                                </TableCell>
                                <TableCell data-align="end">
                                    {nextAction ? (
                                        <TableRowActions
                                            moreId={`supplier-deposits-recharge-${segment}-more`}
                                            moreLabel="充值记录更多操作"
                                            actions={[nextAction]}
                                        />
                                    ) : (
                                        <TableEmptyCell />
                                    )}
                                </TableCell>
                            </TableRow>
                        )
                    })}
                </TableBody>
            </TableShell>

            <LocalPaginationBar
                idPrefix="supplier-deposits-recharges"
                page={safePage}
                pageCount={pageCount}
                totalCount={filtered.length}
                pageSize={DEFAULT_PAGE_SIZE}
                onPageChange={setPage}
            />
        </DetailWorkspaceSection>
    )
}

export function DepositReservationsAndUsagesSection({
    data,
    subTab,
    onSubTabChange,
    permitted,
    onOpenDialog,
}: {
    data: AccountDetail
    subTab: "reservations" | "usages"
    onSubTabChange: (next: "reservations" | "usages") => void
    permitted: (action: string) => boolean
    onOpenDialog: (dialog: DepositDetailDialogTarget) => void
}) {
    const [resKeyword, setResKeyword] = useState("")
    const [resFilter, setResFilter] = useState("all")
    const [resPage, setResPage] = useState(1)

    const [usageKeyword, setUsageKeyword] = useState("")
    const [usageFilter, setUsageFilter] = useState("all")
    const [usagePage, setUsagePage] = useState(1)

    const ledgerMap = useMemo(
        () => new Map(data.ledger.map((entry) => [entry.id, entry])),
        [data.ledger],
    )
    const reservationMap = useMemo(
        () => new Map(data.reservations.map((item) => [item.id, item])),
        [data.reservations],
    )

    const filteredReservations = useMemo(() => {
        return data.reservations.filter((reservation) => {
            const remainder = reservationRemainder(reservation)
            const hasRemainder = compareDecimal(remainder, "0", 2) > 0
            if (resFilter === "open" && !hasRemainder) return false
            if (
                resFilter !== "all" &&
                resFilter !== "open" &&
                reservation.exposure !== resFilter
            ) {
                return false
            }
            return matchesKeyword(resKeyword, [
                reservation.purchase_order_no,
                reservation.payment_request_no,
                reservation.amount,
            ])
        })
    }, [data.reservations, resFilter, resKeyword])

    const resPageCount = Math.max(
        1,
        Math.ceil(filteredReservations.length / DEFAULT_PAGE_SIZE),
    )
    const safeResPage = Math.min(resPage, resPageCount)
    const pagedReservations = filteredReservations.slice(
        (safeResPage - 1) * DEFAULT_PAGE_SIZE,
        safeResPage * DEFAULT_PAGE_SIZE,
    )

    const enrichedUsages = useMemo(() => {
        return data.usages.map((usage) => {
            const ledgerEntry = ledgerMap.get(usage.ledger_entry_id)
            const evidence = ledgerEntry?.evidence
            const reservation = reservationMap.get(usage.reservation_id)
            const occurredAt = evidence?.occurred_at ?? usage.occurred_at
            const externalOrderId =
                evidence?.external_order_id || usage.external_order_id || null
            const purchaseOrderNo =
                usage.purchase_order_no ||
                reservation?.purchase_order_no ||
                null
            const purchaseOrderId = reservation?.basis.purchase_order_id
            const paymentRequestNo = reservation?.payment_request_no || null
            const unsettled =
                compareDecimal(usage.amount, usage.settled_amount, 2) > 0
            const hasReturnOrReverse =
                compareDecimal(usage.returned_amount, "0", 2) > 0 ||
                compareDecimal(usage.reversed_settlement_amount, "0", 2) > 0
            return {
                usage,
                occurredAt,
                externalOrderId,
                purchaseOrderNo,
                purchaseOrderId,
                paymentRequestNo,
                unsettled,
                hasReturnOrReverse,
            }
        })
    }, [data.usages, ledgerMap, reservationMap])

    const filteredUsages = useMemo(() => {
        return enrichedUsages.filter((item) => {
            if (usageFilter === "unsettled" && !item.unsettled) return false
            if (usageFilter === "settled" && item.unsettled) return false
            if (usageFilter === "returned" && !item.hasReturnOrReverse) {
                return false
            }
            return matchesKeyword(usageKeyword, [
                item.externalOrderId,
                item.purchaseOrderNo,
                item.paymentRequestNo,
                item.usage.amount,
            ])
        })
    }, [enrichedUsages, usageFilter, usageKeyword])

    const usagePageCount = Math.max(
        1,
        Math.ceil(filteredUsages.length / DEFAULT_PAGE_SIZE),
    )
    const safeUsagePage = Math.min(usagePage, usagePageCount)
    const pagedUsages = filteredUsages.slice(
        (safeUsagePage - 1) * DEFAULT_PAGE_SIZE,
        safeUsagePage * DEFAULT_PAGE_SIZE,
    )

    return (
        <DetailWorkspaceSection
            title="订单占用与扣款"
            subtitle={
                subTab === "reservations"
                    ? `共 ${data.reservations.length} 条 · 最近 100 条及全部仍有剩余占用的记录`
                    : `共 ${data.usages.length} 条 · 最近 100 条订单实际扣款与核销记录`
            }
            width="full"
            actions={
                <DetailWorkspaceSubTabs
                    className="pt-0"
                    value={subTab}
                    onValueChange={(next) =>
                        onSubTabChange(
                            next === "usages" ? "usages" : "reservations",
                        )
                    }
                    idPrefix="supplier-deposits-usage-subtab"
                    items={[
                        {
                            id: "reservations",
                            label: `订单占用 (${data.reservations.length})`,
                        },
                        {
                            id: "usages",
                            label: `扣款及核销 (${data.usages.length})`,
                        },
                    ]}
                />
            }
        >
            {subTab === "reservations" ? (
                <>
                    {data.reservations.length > 0 ? (
                        <LocalTableToolbar
                            idPrefix="supplier-deposits-reservations"
                            keyword={resKeyword}
                            onKeywordChange={(next) => {
                                setResKeyword(next)
                                setResPage(1)
                            }}
                            searchPlaceholder="搜索来源采购单号或付款申请单号"
                            filterValue={resFilter}
                            onFilterChange={(next) => {
                                setResFilter(next)
                                setResPage(1)
                            }}
                            filterAriaLabel="筛选订单占用状态"
                            filterOptions={[
                                { value: "all", label: "全部占用状态" },
                                { value: "open", label: "仍有剩余占用" },
                                {
                                    value: "possibly_sent",
                                    label: "扣款结果待核实",
                                },
                                { value: "not_sent", label: "尚未发送订单" },
                                {
                                    value: "cleared",
                                    label: "已核实无扣款风险",
                                },
                            ]}
                            totalCount={data.reservations.length}
                            filteredCount={filteredReservations.length}
                            truncatedHint={data.reservations.length >= 100}
                            onExport={() =>
                                exportCsvFile(
                                    "预存款订单占用记录.csv",
                                    [
                                        "来源采购单",
                                        "付款申请",
                                        "占用预算",
                                        "已扣款消耗",
                                        "已释放",
                                        "未使用占用",
                                        "扣款核实进度",
                                    ],
                                    filteredReservations.map((r) => [
                                        r.purchase_order_no || "",
                                        r.payment_request_no || "",
                                        r.amount,
                                        r.consumed_amount,
                                        r.released_amount,
                                        reservationRemainder(r),
                                        EXPOSURE_STATUS[r.exposure]?.label ??
                                            "待核对",
                                    ]),
                                )
                            }
                        />
                    ) : null}

                    <TableShell
                        empty={pagedReservations.length === 0}
                        isFiltered={
                            Boolean(resKeyword.trim()) || resFilter !== "all"
                        }
                        emptyTitle="暂无订单占用"
                        emptyDescription="采购付款申请批准使用预存款后，将在此追踪预算占用、扣款消耗与剩余释放。"
                        resetFilterId="supplier-deposits-reservations-empty-reset"
                        onResetFilter={() => {
                            setResKeyword("")
                            setResFilter("all")
                            setResPage(1)
                        }}
                    >
                        <TableHeader>
                            <TableRow>
                                <TableHead>来源采购单 / 付款申请</TableHead>
                                <TableHead data-align="end">占用预算</TableHead>
                                <TableHead data-align="end">
                                    已扣款消耗
                                </TableHead>
                                <TableHead data-align="end">已释放</TableHead>
                                <TableHead data-align="end">
                                    未使用占用
                                </TableHead>
                                <TableHead>扣款核实进度</TableHead>
                                <TableHead data-align="end">操作</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {pagedReservations.map((reservation) => {
                                const remainder =
                                    reservationRemainder(reservation)
                                const hasRemainder =
                                    compareDecimal(remainder, "0", 2) > 0
                                const segment = toAutomationIdSegment(
                                    reservation.id,
                                )
                                const exposure = EXPOSURE_STATUS[
                                    reservation.exposure
                                ] ?? {
                                    label: "待核对",
                                    tone: "warning" as const,
                                }
                                const primaryLabel =
                                    reservation.purchase_order_no ||
                                    "查看采购及付款依据"
                                const hasFreight =
                                    compareDecimal(
                                        reservation.basis.approved_freight,
                                        "0",
                                        2,
                                    ) > 0

                                return (
                                    <TableRow key={reservation.id}>
                                        <TableCell>
                                            <div className="flex min-w-0 flex-col gap-0.5">
                                                <Link
                                                    id={`supplier-deposits-purchase-${segment}`}
                                                    href={`/procurement/orders/${encodeURIComponent(reservation.basis.purchase_order_id)}`}
                                                    className="num inline-flex items-center gap-1 font-medium text-primary underline-offset-4 hover:underline"
                                                >
                                                    <span>{primaryLabel}</span>
                                                    <ArrowUpRightIcon
                                                        className="size-3.5 shrink-0"
                                                        aria-hidden="true"
                                                    />
                                                </Link>
                                                <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                                                    {reservation.payment_request_no ? (
                                                        <>
                                                            <span className="num">
                                                                付款申请{" "}
                                                                {
                                                                    reservation.payment_request_no
                                                                }
                                                            </span>
                                                            <span aria-hidden="true">
                                                                ·
                                                            </span>
                                                        </>
                                                    ) : null}
                                                    <span className="inline-flex items-baseline gap-1">
                                                        <span>货款</span>
                                                        <MoneyValue
                                                            value={
                                                                reservation
                                                                    .basis
                                                                    .approved_goods_amount
                                                            }
                                                            display="exact"
                                                            className="font-normal text-muted-foreground"
                                                        />
                                                    </span>
                                                    {hasFreight ? (
                                                        <>
                                                            <span aria-hidden="true">
                                                                ·
                                                            </span>
                                                            <span className="inline-flex items-baseline gap-1">
                                                                <span>
                                                                    运费
                                                                </span>
                                                                <MoneyValue
                                                                    value={
                                                                        reservation
                                                                            .basis
                                                                            .approved_freight
                                                                    }
                                                                    display="exact"
                                                                    className="font-normal text-muted-foreground"
                                                                />
                                                            </span>
                                                        </>
                                                    ) : null}
                                                </div>
                                            </div>
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={reservation.amount}
                                                display="exact"
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={
                                                    reservation.consumed_amount
                                                }
                                                display="exact"
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={
                                                    reservation.released_amount
                                                }
                                                display="exact"
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={remainder}
                                                display="exact"
                                                className={
                                                    hasRemainder
                                                        ? "font-semibold"
                                                        : "text-muted-foreground"
                                                }
                                            />
                                        </TableCell>
                                        <TableCell>
                                            <BusinessStatusBadge
                                                context="list"
                                                label={exposure.label}
                                                tone={exposure.tone}
                                                appearance={
                                                    exposure.quiet
                                                        ? "quiet"
                                                        : "badge"
                                                }
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            {hasRemainder &&
                                            permitted("release") ? (
                                                <TableRowActions
                                                    moreId={`supplier-deposits-release-${segment}-more`}
                                                    moreLabel="占用记录更多操作"
                                                    actions={[
                                                        {
                                                            id: `supplier-deposits-release-${segment}`,
                                                            label: "核实剩余占用",
                                                            onClick: () =>
                                                                onOpenDialog({
                                                                    action: "release",
                                                                    reservation,
                                                                }),
                                                        },
                                                    ]}
                                                />
                                            ) : hasRemainder ? (
                                                <TableEmptyCell />
                                            ) : (
                                                <TableEmptyCell
                                                    variant="label"
                                                    label="无剩余占用"
                                                />
                                            )}
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                        </TableBody>
                    </TableShell>

                    <LocalPaginationBar
                        idPrefix="supplier-deposits-reservations"
                        page={safeResPage}
                        pageCount={resPageCount}
                        totalCount={filteredReservations.length}
                        pageSize={DEFAULT_PAGE_SIZE}
                        onPageChange={setResPage}
                    />
                </>
            ) : (
                <>
                    {data.usages.length > 0 ? (
                        <LocalTableToolbar
                            idPrefix="supplier-deposits-usages"
                            keyword={usageKeyword}
                            onKeywordChange={(next) => {
                                setUsageKeyword(next)
                                setUsagePage(1)
                            }}
                            searchPlaceholder="搜索供应商订单号、采购单号或付款申请"
                            filterValue={usageFilter}
                            onFilterChange={(next) => {
                                setUsageFilter(next)
                                setUsagePage(1)
                            }}
                            filterAriaLabel="筛选扣款核销状态"
                            filterOptions={[
                                { value: "all", label: "全部扣款记录" },
                                { value: "unsettled", label: "尚有待核销金额" },
                                { value: "settled", label: "已全额核销" },
                                {
                                    value: "returned",
                                    label: "含返款或冲回核销",
                                },
                            ]}
                            totalCount={data.usages.length}
                            filteredCount={filteredUsages.length}
                            truncatedHint={data.usages.length >= 100}
                            onExport={() =>
                                exportCsvFile(
                                    "预存款订单扣款核销记录.csv",
                                    [
                                        "扣款时间",
                                        "供应商订单",
                                        "来源采购单",
                                        "实际扣款",
                                        "已核销",
                                        "已到账返款",
                                        "已冲回核销",
                                    ],
                                    filteredUsages.map((item) => [
                                        item.occurredAt != null
                                            ? displayDepositTime(
                                                  item.occurredAt,
                                              )
                                            : "超出最近 100 条流水窗口",
                                        item.externalOrderId || "",
                                        item.purchaseOrderNo || "",
                                        item.usage.amount,
                                        item.usage.settled_amount,
                                        item.usage.returned_amount,
                                        item.usage.reversed_settlement_amount,
                                    ]),
                                )
                            }
                        />
                    ) : null}

                    <TableShell
                        empty={pagedUsages.length === 0}
                        isFiltered={
                            Boolean(usageKeyword.trim()) ||
                            usageFilter !== "all"
                        }
                        emptyTitle="暂无订单扣款及核销记录"
                        resetFilterId="supplier-deposits-usages-empty-reset"
                        onResetFilter={() => {
                            setUsageKeyword("")
                            setUsageFilter("all")
                            setUsagePage(1)
                        }}
                    >
                        <TableHeader>
                            <TableRow>
                                <TableHead>扣款时间 / 关联订单</TableHead>
                                <TableHead data-align="end">实际扣款</TableHead>
                                <TableHead data-align="end">已核销</TableHead>
                                <TableHead data-align="end">
                                    已到账返款
                                </TableHead>
                                <TableHead data-align="end">
                                    已冲回核销
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {pagedUsages.map((item) => {
                                const segment = toAutomationIdSegment(
                                    item.usage.id,
                                )
                                const primaryNode =
                                    item.occurredAt != null ? (
                                        <span className="num">
                                            {displayDepositTime(
                                                item.occurredAt,
                                            )}
                                        </span>
                                    ) : item.purchaseOrderNo &&
                                      item.purchaseOrderId ? (
                                        <Link
                                            id={`supplier-deposits-usage-purchase-${segment}`}
                                            href={`/procurement/orders/${encodeURIComponent(item.purchaseOrderId)}`}
                                            className="num inline-flex items-center gap-1 font-medium text-primary underline-offset-4 hover:underline"
                                        >
                                            <span>
                                                采购单 {item.purchaseOrderNo}
                                            </span>
                                            <ArrowUpRightIcon
                                                className="size-3.5 shrink-0"
                                                aria-hidden="true"
                                            />
                                        </Link>
                                    ) : (
                                        <span className="text-muted-foreground">
                                            历史扣款记录
                                        </span>
                                    )

                                const secondaryParts = [
                                    item.externalOrderId
                                        ? `供应商单号 ${item.externalOrderId}`
                                        : null,
                                    item.occurredAt != null &&
                                    item.purchaseOrderNo
                                        ? `采购单 ${item.purchaseOrderNo}`
                                        : null,
                                    item.paymentRequestNo
                                        ? `付款申请 ${item.paymentRequestNo}`
                                        : null,
                                ].filter(Boolean)

                                const secondaryText =
                                    secondaryParts.length > 0
                                        ? secondaryParts.join(" · ")
                                        : "对应流水已超出最近 100 条窗口"

                                return (
                                    <TableRow key={item.usage.id}>
                                        <TableCell>
                                            <TableIdentityCell
                                                primary={primaryNode}
                                                secondary={secondaryText}
                                                numericSecondary
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={item.usage.amount}
                                                display="exact"
                                                className="font-medium"
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={
                                                    item.usage.settled_amount
                                                }
                                                display="exact"
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={
                                                    item.usage.returned_amount
                                                }
                                                display="exact"
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <MoneyValue
                                                value={
                                                    item.usage
                                                        .reversed_settlement_amount
                                                }
                                                display="exact"
                                            />
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                        </TableBody>
                    </TableShell>

                    <LocalPaginationBar
                        idPrefix="supplier-deposits-usages"
                        page={safeUsagePage}
                        pageCount={usagePageCount}
                        totalCount={filteredUsages.length}
                        pageSize={DEFAULT_PAGE_SIZE}
                        onPageChange={setUsagePage}
                    />
                </>
            )}
        </DetailWorkspaceSection>
    )
}

export function DepositLedgerAndBatchesSection({
    data,
    subTab,
    onSubTabChange,
}: {
    data: AccountDetail
    subTab: "ledger" | "batches"
    onSubTabChange: (next: "ledger" | "batches") => void
}) {
    const [ledgerKeyword, setLedgerKeyword] = useState("")
    const [ledgerKindFilter, setLedgerKindFilter] = useState("all")
    const [ledgerPage, setLedgerPage] = useState(1)

    const [batchKeyword, setBatchKeyword] = useState("")
    const [batchPage, setBatchPage] = useState(1)

    const filteredLedger = useMemo(() => {
        return data.ledger.filter((entry) => {
            if (ledgerKindFilter !== "all" && entry.kind !== ledgerKindFilter) {
                return false
            }
            return matchesKeyword(ledgerKeyword, [
                entry.evidence.external_transaction_id,
                entry.evidence.external_order_id,
                entry.evidence.evidence_reference,
                entry.amount,
            ])
        })
    }, [data.ledger, ledgerKeyword, ledgerKindFilter])

    const ledgerPageCount = Math.max(
        1,
        Math.ceil(filteredLedger.length / DEFAULT_PAGE_SIZE),
    )
    const safeLedgerPage = Math.min(ledgerPage, ledgerPageCount)
    const pagedLedger = filteredLedger.slice(
        (safeLedgerPage - 1) * DEFAULT_PAGE_SIZE,
        safeLedgerPage * DEFAULT_PAGE_SIZE,
    )

    const filteredBatches = useMemo(() => {
        return data.batches.filter((batch) =>
            matchesKeyword(batchKeyword, [
                batch.evidence.bank_reference,
                batch.evidence.external_transaction_id,
                batch.evidence.external_evidence_reference,
                batch.amount,
            ]),
        )
    }, [batchKeyword, data.batches])

    const batchPageCount = Math.max(
        1,
        Math.ceil(filteredBatches.length / DEFAULT_PAGE_SIZE),
    )
    const safeBatchPage = Math.min(batchPage, batchPageCount)
    const pagedBatches = filteredBatches.slice(
        (safeBatchPage - 1) * DEFAULT_PAGE_SIZE,
        safeBatchPage * DEFAULT_PAGE_SIZE,
    )

    return (
        <DetailWorkspaceSection
            title="资金流水与批次"
            subtitle={
                subTab === "ledger"
                    ? `最近 ${data.ledger.length} 条（最多显示 100 条）`
                    : `最近 ${data.batches.length} 条`
            }
            width="full"
            actions={
                <DetailWorkspaceSubTabs
                    className="pt-0"
                    value={subTab}
                    onValueChange={(next) =>
                        onSubTabChange(
                            next === "batches" ? "batches" : "ledger",
                        )
                    }
                    idPrefix="supplier-deposits-ledger-subtab"
                    items={[
                        {
                            id: "ledger",
                            label: `资金流水 (${data.ledger.length})`,
                        },
                        {
                            id: "batches",
                            label: `充值来源批次 (${data.batches.length})`,
                        },
                    ]}
                />
            }
        >
            {subTab === "ledger" ? (
                <>
                    {data.ledger.length > 0 ? (
                        <LocalTableToolbar
                            idPrefix="supplier-deposits-ledger"
                            keyword={ledgerKeyword}
                            onKeywordChange={(next) => {
                                setLedgerKeyword(next)
                                setLedgerPage(1)
                            }}
                            searchPlaceholder="搜索供应商交易号、订单号或来源依据"
                            filterValue={ledgerKindFilter}
                            onFilterChange={(next) => {
                                setLedgerKindFilter(next)
                                setLedgerPage(1)
                            }}
                            filterAriaLabel="筛选资金流水类型"
                            filterOptions={[
                                { value: "all", label: "全部业务类型" },
                                { value: "recharge", label: "充值到账" },
                                { value: "return", label: "账户返款" },
                                { value: "debit", label: "订单扣款" },
                                {
                                    value: "recharge_reversal",
                                    label: "充值冲正",
                                },
                            ]}
                            totalCount={data.ledger.length}
                            filteredCount={filteredLedger.length}
                            truncatedHint={data.ledger.length >= 100}
                            onExport={() =>
                                exportCsvFile(
                                    "预存款资金流水.csv",
                                    [
                                        "发生时间",
                                        "业务类型",
                                        "收支方向",
                                        "变动金额",
                                        "供应商交易号",
                                        "供应商订单号",
                                        "来源依据",
                                    ],
                                    filteredLedger.map((entry) => {
                                        const kind = LEDGER_KIND[
                                            entry.kind
                                        ] ?? {
                                            label: "需核对资金类型",
                                            inflow: false,
                                        }
                                        return [
                                            displayDepositTime(
                                                entry.evidence.occurred_at,
                                            ),
                                            kind.label,
                                            kind.inflow ? "入账" : "出账",
                                            entry.amount,
                                            entry.evidence
                                                .external_transaction_id,
                                            entry.evidence.external_order_id,
                                            entry.evidence.evidence_reference,
                                        ]
                                    }),
                                )
                            }
                        />
                    ) : null}

                    <TableShell
                        empty={pagedLedger.length === 0}
                        isFiltered={
                            Boolean(ledgerKeyword.trim()) ||
                            ledgerKindFilter !== "all"
                        }
                        emptyTitle="暂无资金流水"
                        resetFilterId="supplier-deposits-ledger-empty-reset"
                        onResetFilter={() => {
                            setLedgerKeyword("")
                            setLedgerKindFilter("all")
                            setLedgerPage(1)
                        }}
                    >
                        <TableHeader>
                            <TableRow>
                                <TableHead>发生时间</TableHead>
                                <TableHead>业务类型</TableHead>
                                <TableHead data-align="end">变动金额</TableHead>
                                <TableHead>供应商交易号</TableHead>
                                <TableHead>来源依据</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {pagedLedger.map((entry) => {
                                const kind = LEDGER_KIND[entry.kind] ?? {
                                    label: "需核对资金类型",
                                    tone: "warning" as const,
                                    inflow: false,
                                }
                                return (
                                    <TableRow key={entry.id}>
                                        <TableCell className="num">
                                            {displayDepositTime(
                                                entry.evidence.occurred_at,
                                            )}
                                        </TableCell>
                                        <TableCell>
                                            <BusinessStatusBadge
                                                context="list"
                                                label={kind.label}
                                                tone={kind.tone}
                                                appearance="quiet"
                                            />
                                        </TableCell>
                                        <TableCell data-align="end">
                                            <span
                                                className={cn(
                                                    "num inline-flex items-baseline gap-0.5 font-medium",
                                                    kind.inflow
                                                        ? "text-success-soft-foreground"
                                                        : "text-foreground",
                                                )}
                                            >
                                                <span>
                                                    {kind.inflow ? "+" : "−"}
                                                </span>
                                                <MoneyValue
                                                    value={entry.amount}
                                                    display="exact"
                                                />
                                            </span>
                                        </TableCell>
                                        <TableCell className="num">
                                            <TableTextCell
                                                value={
                                                    entry.evidence
                                                        .external_transaction_id
                                                }
                                                numeric
                                            />
                                        </TableCell>
                                        <TableCell className="max-w-72 whitespace-normal">
                                            <TableTextCell
                                                value={
                                                    entry.evidence
                                                        .evidence_reference
                                                }
                                                clamp={2}
                                            />
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                        </TableBody>
                    </TableShell>

                    <LocalPaginationBar
                        idPrefix="supplier-deposits-ledger"
                        page={safeLedgerPage}
                        pageCount={ledgerPageCount}
                        totalCount={filteredLedger.length}
                        pageSize={DEFAULT_PAGE_SIZE}
                        onPageChange={setLedgerPage}
                    />
                </>
            ) : (
                <>
                    {data.batches.length > 0 ? (
                        <LocalTableToolbar
                            idPrefix="supplier-deposits-batches"
                            keyword={batchKeyword}
                            onKeywordChange={(next) => {
                                setBatchKeyword(next)
                                setBatchPage(1)
                            }}
                            searchPlaceholder="搜索银行流水号、供应商交易号或到账依据"
                            totalCount={data.batches.length}
                            filteredCount={filteredBatches.length}
                            truncatedHint={data.batches.length >= 100}
                            onExport={() =>
                                exportCsvFile(
                                    "预存款充值来源批次.csv",
                                    [
                                        "到账时间",
                                        "银行流水",
                                        "到账金额",
                                        "已占用",
                                        "累计使用",
                                        "已到账返款",
                                        "冲正",
                                    ],
                                    filteredBatches.map((batch) => [
                                        displayDepositTime(
                                            batch.evidence.received_at,
                                        ),
                                        batch.evidence.bank_reference,
                                        batch.amount,
                                        batch.reserved_amount,
                                        batch.consumed_amount,
                                        batch.returned_amount,
                                        batch.reversed_amount,
                                    ]),
                                )
                            }
                        />
                    ) : null}

                    <TableShell
                        empty={pagedBatches.length === 0}
                        isFiltered={Boolean(batchKeyword.trim())}
                        emptyTitle="暂无充值来源批次"
                        resetFilterId="supplier-deposits-batches-empty-reset"
                        onResetFilter={() => {
                            setBatchKeyword("")
                            setBatchPage(1)
                        }}
                    >
                        <TableHeader>
                            <TableRow>
                                <TableHead>到账时间 / 银行流水</TableHead>
                                <TableHead data-align="end">到账金额</TableHead>
                                <TableHead data-align="end">已占用</TableHead>
                                <TableHead data-align="end">累计使用</TableHead>
                                <TableHead data-align="end">
                                    已到账返款
                                </TableHead>
                                <TableHead data-align="end">冲正</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {pagedBatches.map((batch) => (
                                <TableRow key={batch.id}>
                                    <TableCell>
                                        <div className="flex flex-col gap-0.5">
                                            <TableIdentityCell
                                                primary={
                                                    <span className="num">
                                                        {displayDepositTime(
                                                            batch.evidence
                                                                .received_at,
                                                        )}
                                                    </span>
                                                }
                                                secondary={
                                                    batch.evidence
                                                        .bank_reference
                                                }
                                                numericSecondary
                                            />
                                            {batch.evidence
                                                .bank_receipt_asset_id ? (
                                                <DepositBankReceiptAction
                                                    recordId={batch.id}
                                                    assetId={
                                                        batch.evidence
                                                            .bank_receipt_asset_id
                                                    }
                                                    idPrefix="supplier-deposits-batch-receipt"
                                                />
                                            ) : null}
                                        </div>
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={batch.amount}
                                            display="exact"
                                            className="font-medium"
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={batch.reserved_amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={batch.consumed_amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={batch.returned_amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                    <TableCell data-align="end">
                                        <MoneyValue
                                            value={batch.reversed_amount}
                                            display="exact"
                                        />
                                    </TableCell>
                                </TableRow>
                            ))}
                        </TableBody>
                    </TableShell>

                    <LocalPaginationBar
                        idPrefix="supplier-deposits-batches"
                        page={safeBatchPage}
                        pageCount={batchPageCount}
                        totalCount={filteredBatches.length}
                        pageSize={DEFAULT_PAGE_SIZE}
                        onPageChange={setBatchPage}
                    />
                </>
            )}
        </DetailWorkspaceSection>
    )
}

export function DepositReconciliationsSection({
    data,
    permitted,
    onOpenDialog,
    actions,
}: {
    data: AccountDetail
    permitted: (action: string) => boolean
    onOpenDialog: (dialog: DepositDetailDialogTarget) => void
    actions?: React.ReactNode
}) {
    const [keyword, setKeyword] = useState("")
    const [statusFilter, setStatusFilter] = useState("all")
    const [page, setPage] = useState(1)

    const filtered = useMemo(() => {
        return data.reconciliations.filter((record) => {
            const diff = subtractFixed(
                record.remote_balance,
                record.local_balance_at_cutoff,
                MONEY_OPTIONS,
            )
            const balanced =
                compareDecimal(diff, "0", 2) === 0 &&
                record.unresolved_transaction_ids.length === 0
            if (statusFilter === "resolved" && !record.resolved_by) return false
            if (
                statusFilter === "balanced" &&
                (Boolean(record.resolved_by) || !balanced)
            ) {
                return false
            }
            if (
                statusFilter === "diff" &&
                (Boolean(record.resolved_by) || balanced)
            ) {
                return false
            }
            return matchesKeyword(keyword, [
                record.evidence_reference,
                record.resolution_reference,
                ...record.unresolved_transaction_ids,
            ])
        })
    }, [data.reconciliations, keyword, statusFilter])

    const pageCount = Math.max(
        1,
        Math.ceil(filtered.length / DEFAULT_PAGE_SIZE),
    )
    const safePage = Math.min(page, pageCount)
    const paged = filtered.slice(
        (safePage - 1) * DEFAULT_PAGE_SIZE,
        safePage * DEFAULT_PAGE_SIZE,
    )
    const isFiltered = Boolean(keyword.trim()) || statusFilter !== "all"

    return (
        <DetailWorkspaceSection
            title="资金对账"
            subtitle={`共 ${data.reconciliations.length} 条 · 最近 100 条及全部未结案差异，历史待处理记录继续保留`}
            width="full"
            actions={actions}
        >
            {data.reconciliations.length > 0 ? (
                <LocalTableToolbar
                    idPrefix="supplier-deposits-reconciliations"
                    keyword={keyword}
                    onKeywordChange={(next) => {
                        setKeyword(next)
                        setPage(1)
                    }}
                    searchPlaceholder="搜索对账依据或未核实交易号"
                    filterValue={statusFilter}
                    onFilterChange={(next) => {
                        setStatusFilter(next)
                        setPage(1)
                    }}
                    filterAriaLabel="筛选对账状态"
                    filterOptions={[
                        { value: "all", label: "全部对账状态" },
                        { value: "diff", label: "差异待核实" },
                        { value: "balanced", label: "双方余额一致" },
                        { value: "resolved", label: "已核实结案" },
                    ]}
                    totalCount={data.reconciliations.length}
                    filteredCount={filtered.length}
                    truncatedHint={data.reconciliations.length >= 100}
                    onExport={() =>
                        exportCsvFile(
                            "预存款资金对账记录.csv",
                            [
                                "起始时间",
                                "截止时间",
                                "供应商余额",
                                "本地余额",
                                "余额差额",
                                "未核实交易",
                                "状态",
                                "对账依据",
                            ],
                            filtered.map((record) => {
                                const diff = subtractFixed(
                                    record.remote_balance,
                                    record.local_balance_at_cutoff,
                                    MONEY_OPTIONS,
                                )
                                const balanced =
                                    compareDecimal(diff, "0", 2) === 0 &&
                                    record.unresolved_transaction_ids.length ===
                                        0
                                const statusLabel = record.resolved_by
                                    ? "已核实结案"
                                    : balanced
                                      ? "双方余额一致"
                                      : "差异待核实"
                                return [
                                    displayDepositTime(record.covered_from),
                                    displayDepositTime(record.covered_until),
                                    record.remote_balance,
                                    record.local_balance_at_cutoff,
                                    diff,
                                    record.unresolved_transaction_ids.join(
                                        "、",
                                    ),
                                    statusLabel,
                                    record.evidence_reference,
                                ]
                            }),
                        )
                    }
                />
            ) : null}

            <TableShell
                empty={paged.length === 0}
                isFiltered={isFiltered}
                emptyTitle="暂无资金对账记录"
                emptyDescription="定期按同一截止时点核对本地余额与供应商平台余额，发现差异后可在此结案。"
                resetFilterId="supplier-deposits-reconciliations-empty-reset"
                onResetFilter={() => {
                    setKeyword("")
                    setStatusFilter("all")
                    setPage(1)
                }}
            >
                <TableHeader>
                    <TableRow>
                        <TableHead>核对期间</TableHead>
                        <TableHead data-align="end">供应商余额</TableHead>
                        <TableHead data-align="end">本地余额</TableHead>
                        <TableHead data-align="end">余额差额</TableHead>
                        <TableHead>未核实交易</TableHead>
                        <TableHead>状态与核实依据</TableHead>
                        <TableHead data-align="end">操作</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {paged.map((record: DepositReconciliation) => {
                        const diff = subtractFixed(
                            record.remote_balance,
                            record.local_balance_at_cutoff,
                            MONEY_OPTIONS,
                        )
                        const diffSign = compareDecimal(diff, "0", 2)
                        const balanced =
                            diffSign === 0 &&
                            record.unresolved_transaction_ids.length === 0
                        const segment = toAutomationIdSegment(record.id)
                        const statusBadge = record.resolved_by
                            ? {
                                  label: "已核实结案",
                                  tone: "success" as const,
                                  quiet: true,
                              }
                            : balanced
                              ? {
                                    label: "双方余额一致",
                                    tone: "success" as const,
                                    quiet: true,
                                }
                              : {
                                    label: "差异待核实",
                                    tone: "destructive" as const,
                                    quiet: false,
                                }
                        const canResolve =
                            !record.resolved_by &&
                            permitted("resolve") &&
                            (data.account.reconciliation_required || !balanced)
                        const resolveLabel =
                            data.account.reconciliation_required && balanced
                                ? "核实后恢复使用"
                                : "核实对账差异"

                        return (
                            <TableRow key={record.id}>
                                <TableCell className="num">
                                    <div>
                                        {displayDepositTime(
                                            record.covered_from,
                                        )}
                                    </div>
                                    <div className="text-xs text-muted-foreground">
                                        至{" "}
                                        {displayDepositTime(
                                            record.covered_until,
                                        )}
                                    </div>
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={record.remote_balance}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={record.local_balance_at_cutoff}
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={diff}
                                        display="exact"
                                        className={
                                            diffSign !== 0
                                                ? "font-semibold text-destructive"
                                                : "text-muted-foreground"
                                        }
                                    />
                                </TableCell>
                                <TableCell className="max-w-52 whitespace-normal">
                                    {record.unresolved_transaction_ids.length >
                                    0 ? (
                                        <span className="num text-xs text-destructive">
                                            {record.unresolved_transaction_ids.join(
                                                "、",
                                            )}
                                        </span>
                                    ) : (
                                        <TableEmptyCell
                                            variant="label"
                                            label="无"
                                        />
                                    )}
                                </TableCell>
                                <TableCell className="max-w-64 whitespace-normal">
                                    <div className="flex flex-col items-start gap-1">
                                        <BusinessStatusBadge
                                            context="list"
                                            label={statusBadge.label}
                                            tone={statusBadge.tone}
                                            appearance={
                                                statusBadge.quiet
                                                    ? "quiet"
                                                    : "badge"
                                            }
                                        />
                                        <span className="text-xs text-muted-foreground">
                                            {record.evidence_reference}
                                        </span>
                                        {record.resolution
                                            ?.evidence_asset_id ? (
                                            <DepositResolutionEvidence
                                                reconciliationId={record.id}
                                                assetId={
                                                    record.resolution
                                                        .evidence_asset_id
                                                }
                                            />
                                        ) : null}
                                    </div>
                                </TableCell>
                                <TableCell data-align="end">
                                    {record.resolved_by ? (
                                        <TableEmptyCell
                                            variant="label"
                                            label="已结案"
                                        />
                                    ) : canResolve ? (
                                        <TableRowActions
                                            moreId={`supplier-deposits-resolve-${segment}-more`}
                                            moreLabel="对账记录更多操作"
                                            actions={[
                                                {
                                                    id: `supplier-deposits-resolve-${segment}`,
                                                    label: resolveLabel,
                                                    onClick: () =>
                                                        onOpenDialog({
                                                            action: "resolve",
                                                            reconciliation:
                                                                record,
                                                        }),
                                                },
                                            ]}
                                        />
                                    ) : (
                                        <TableEmptyCell />
                                    )}
                                </TableCell>
                            </TableRow>
                        )
                    })}
                </TableBody>
            </TableShell>

            <LocalPaginationBar
                idPrefix="supplier-deposits-reconciliations"
                page={safePage}
                pageCount={pageCount}
                totalCount={filtered.length}
                pageSize={DEFAULT_PAGE_SIZE}
                onPageChange={setPage}
            />
        </DetailWorkspaceSection>
    )
}

export function DepositAuthorizationsSection({
    authorizations,
    permitted,
    onOpenDialog,
    actions,
}: {
    authorizations: readonly DepositAuthorization[]
    permitted: (action: string) => boolean
    onOpenDialog: (dialog: DepositDetailDialogTarget) => void
    actions?: React.ReactNode
}) {
    const now = Date.now()
    return (
        <DetailWorkspaceSection
            title="余额执行授权"
            subtitle={`共 ${authorizations.length} 条 · 自动执行须同时满足有效期限、单次限额与累计限额`}
            width="full"
            actions={actions}
        >
            <TableShell
                empty={authorizations.length === 0}
                emptyTitle="暂无余额执行授权"
                emptyDescription="如需系统在采购付款审批通过后自动使用余额，请先登记财务授权。"
            >
                <TableHeader>
                    <TableRow>
                        <TableHead>授权方式 / 依据</TableHead>
                        <TableHead>有效期</TableHead>
                        <TableHead data-align="end">单次上限</TableHead>
                        <TableHead data-align="end">累计上限与已用</TableHead>
                        <TableHead>状态</TableHead>
                        <TableHead data-align="end">操作</TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {authorizations.map((authorization) => {
                        const segment = toAutomationIdSegment(authorization.id)
                        const isExpired =
                            authorization.valid_until * 1000 <= now
                        const statusMeta = authorization.revoked
                            ? {
                                  label: "已撤销",
                                  tone: "void" as const,
                                  quiet: false,
                              }
                            : isExpired
                              ? {
                                    label: "已到期",
                                    tone: "neutral" as const,
                                    quiet: true,
                                }
                              : authorization.valid_from * 1000 > now
                                ? {
                                      label: "尚未生效",
                                      tone: "info" as const,
                                      quiet: false,
                                  }
                                : {
                                      label: "有效",
                                      tone: "success" as const,
                                      quiet: true,
                                  }
                        const percent = utilizationPercent(
                            authorization.committed_amount,
                            authorization.cumulative_limit,
                        )
                        const canRevoke =
                            permitted("revoke") &&
                            !authorization.revoked &&
                            !isExpired

                        return (
                            <TableRow key={authorization.id}>
                                <TableCell>
                                    <TableIdentityCell
                                        primary={
                                            authorization.mode?.type ===
                                            "manual"
                                                ? "本次人工执行"
                                                : "自动执行"
                                        }
                                        secondary={
                                            authorization.approval_reference
                                        }
                                        secondaryTitle={
                                            authorization.approval_reference
                                        }
                                    />
                                </TableCell>
                                <TableCell className="num">
                                    <div>
                                        {displayDepositTime(
                                            authorization.valid_from,
                                        )}
                                    </div>
                                    <div className="text-xs text-muted-foreground">
                                        至{" "}
                                        {displayDepositTime(
                                            authorization.valid_until,
                                        )}
                                    </div>
                                </TableCell>
                                <TableCell data-align="end">
                                    <MoneyValue
                                        value={
                                            authorization.per_execution_limit
                                        }
                                        display="exact"
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    <div className="ml-auto flex w-44 flex-col items-end gap-1">
                                        <MoneyValue
                                            value={
                                                authorization.cumulative_limit
                                            }
                                            display="exact"
                                            className="font-medium"
                                        />
                                        <Progress
                                            value={percent}
                                            className="h-1.5 w-full"
                                        />
                                        <span className="num text-xs text-muted-foreground">
                                            已用{" "}
                                            <MoneyValue
                                                value={
                                                    authorization.committed_amount
                                                }
                                                display="exact"
                                            />{" "}
                                            ({percent}%)
                                        </span>
                                    </div>
                                </TableCell>
                                <TableCell>
                                    <BusinessStatusBadge
                                        context="list"
                                        label={statusMeta.label}
                                        tone={statusMeta.tone}
                                        appearance={
                                            statusMeta.quiet ? "quiet" : "badge"
                                        }
                                    />
                                </TableCell>
                                <TableCell data-align="end">
                                    {canRevoke ? (
                                        <TableRowActions
                                            moreId={`supplier-deposits-revoke-${segment}-more`}
                                            moreLabel="授权记录更多操作"
                                            actions={[
                                                {
                                                    id: `supplier-deposits-revoke-${segment}`,
                                                    label: "撤销授权",
                                                    destructive: true,
                                                    onClick: () =>
                                                        onOpenDialog({
                                                            action: "revoke",
                                                            authorization,
                                                        }),
                                                },
                                            ]}
                                        />
                                    ) : (
                                        <TableEmptyCell />
                                    )}
                                </TableCell>
                            </TableRow>
                        )
                    })}
                </TableBody>
            </TableShell>
        </DetailWorkspaceSection>
    )
}
