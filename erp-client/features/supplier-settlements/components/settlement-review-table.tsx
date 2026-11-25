import {
    Fragment,
    useEffect,
    useImperativeHandle,
    useMemo,
    useRef,
    useState,
    type ReactNode,
    type Ref,
} from "react"
import Link from "next/link"
import {
    ChevronDownIcon,
    ChevronLeftIcon,
    ChevronRightIcon,
    SearchIcon,
    XIcon,
} from "lucide-react"

import { TableToolbar } from "@/components/business/table-toolbar"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import type {
    SettlementDetailView,
    SettlementDifferenceView,
} from "@/features/supplier-settlements/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

import {
    settlementDifference,
    settlementLineDelta,
    settlementMoney,
} from "./settlement-review-metrics"

const FILTER_THRESHOLD = 8
const PAGE_SIZE = 20
const PULSE_DURATION = 2200

type Item = SettlementDetailView["items"][number]
type ItemRow = {
    id: string
    kind: "item"
    item: Item
    delta?: string
    matched: boolean
}
type DifferenceRow = {
    id: string
    kind: "difference"
    difference: SettlementDifferenceView
}
type ReconciliationRow = ItemRow | DifferenceRow
type Filter = "all" | "open" | "matched" | "amount" | "selected"

export type SettlementReviewTableHandle = { locateOpenDifferences: () => void }

export function useSettlementReviewRows(detail: SettlementDetailView) {
    return useMemo(() => {
        const items: ItemRow[] = detail.items.map((item) => {
            const delta = settlementLineDelta(
                item.supplierBillLineGross,
                item.erpAmountGross,
            )
            return {
                id: `item-${item.itemId}`,
                kind: "item",
                item,
                delta,
                matched: delta != null && compareDecimal(delta, "0", 6) === 0,
            }
        })
        const differences: DifferenceRow[] = detail.differences.map(
            (difference) => ({
                id: `difference-${difference.differenceId}`,
                kind: "difference",
                difference,
            }),
        )
        const open = differences.filter(
            ({ difference }) => difference.status === "PENDING",
        )
        const disputed = open.filter(
            ({ difference }) => difference.blocking,
        ).length
        return {
            all: [...items, ...differences] satisfies ReconciliationRow[],
            open,
            matched: items.filter((row) => row.matched),
            amount: items.filter((row) => !row.matched),
            disputed,
        }
    }, [detail.items, detail.differences])
}

function rowContains(row: ReconciliationRow, needle: string) {
    const fields =
        row.kind === "item"
            ? [
                  row.item.productName,
                  row.item.supplierOrderNo,
                  row.item.purchaseNo,
                  row.item.externalOrderNo,
                  row.matched ? "已匹配" : "金额待核对",
              ]
            : [
                  row.difference.typeLabel,
                  row.difference.statusLabel,
                  row.difference.amountDirectionLabel,
              ]
    return fields.some((field) => field?.toLowerCase().includes(needle))
}

function SettlementItemDetails({
    item,
    idPrefix,
    statementId,
}: {
    item: Item
    idPrefix: string
    statementId: string
}) {
    const fields = [
        ["数量", item.quantity],
        ["订单含税", settlementMoney(item.orderAmountGross)],
        ["运费", settlementMoney(item.freightGross)],
        ["服务费", settlementMoney(item.serviceFeeGross)],
        ["退款", settlementMoney(item.refundGross)],
        ["外部订单", item.externalOrderNo],
    ] as const
    return (
        <div className="space-y-2">
            <dl className="flex flex-wrap gap-x-4 gap-y-1 text-xs">
                {fields.map(([label, value]) => (
                    <div
                        key={label}
                        className="flex min-w-0 items-baseline gap-1.5"
                    >
                        <dt className="shrink-0 text-muted-foreground">
                            {label}
                        </dt>
                        <dd className="num min-w-0 wrap-anywhere">{value}</dd>
                    </div>
                ))}
            </dl>
            <div className="flex flex-wrap gap-x-4 gap-y-1 text-xs">
                <Link
                    id={`${idPrefix}-supplier-order`}
                    href={`/supplier-api/orders/${encodeURIComponent(item.supplierOrderId)}`}
                    className="num text-primary underline-offset-2 hover:underline"
                >
                    供应商订单 {item.supplierOrderNo}
                </Link>
                {item.purchaseNo && (
                    <Link
                        id={`${idPrefix}-purchase-order`}
                        href={
                            item.purchaseOrderId
                                ? `/procurement/orders/${encodeURIComponent(item.purchaseOrderId)}?returnTo=${encodeURIComponent(`/supplier-api/settlements/${encodeURIComponent(statementId)}`)}`
                                : `/procurement/orders?q=${encodeURIComponent(item.purchaseNo)}`
                        }
                        className="num text-primary underline-offset-2 hover:underline"
                    >
                        采购单 {item.purchaseNo}
                    </Link>
                )}
            </div>
        </div>
    )
}

export function SettlementReviewTable({
    idPrefix,
    detail,
    rows,
    ref,
    utilityActions,
    onSelectDifference,
    renderDifference,
}: {
    idPrefix: string
    detail: SettlementDetailView
    rows: ReturnType<typeof useSettlementReviewRows>
    ref: Ref<SettlementReviewTableHandle>
    utilityActions: ReactNode
    onSelectDifference: (id: string) => void
    renderDifference: (difference: SettlementDifferenceView) => ReactNode
}) {
    const [filter, setFilter] = useState<Filter>("all")
    const [keyword, setKeyword] = useState("")
    const [page, setPage] = useState(1)
    const [checked, setChecked] = useState<ReadonlySet<string>>(new Set())
    const [isolated, setIsolated] = useState<ReadonlySet<string>>(new Set())
    const [expanded, setExpanded] = useState<string>()
    const [locateSequence, setLocateSequence] = useState(0)
    const [pulse, setPulse] = useState(false)
    const targetRef = useRef<HTMLButtonElement>(null)
    const searchRef = useRef<HTMLInputElement>(null)

    const effectiveFilter =
        filter === "selected" || rows[filter].length > 0 ? filter : "all"
    const source =
        effectiveFilter === "selected"
            ? rows.all.filter((row) => isolated.has(row.id))
            : rows[effectiveFilter]
    const needle = keyword.trim().toLowerCase()
    const shown = needle
        ? source.filter((row) => rowContains(row, needle))
        : source
    const pageCount = Math.max(1, Math.ceil(shown.length / PAGE_SIZE))
    const visiblePage = Math.min(page, pageCount)
    const pageRows = shown.slice(
        (visiblePage - 1) * PAGE_SIZE,
        visiblePage * PAGE_SIZE,
    )
    const hasFilters = effectiveFilter !== "all" || needle.length > 0
    const showFilters =
        rows.all.length > FILTER_THRESHOLD || rows.open.length > 0 || hasFilters
    const showSearch = rows.all.length > FILTER_THRESHOLD || needle.length > 0
    const selectedRows = rows.all.filter((row) => checked.has(row.id))
    const firstOpenId = rows.open[0]?.id
    const filterOptions = [
        { value: "all", label: "全部", count: rows.all.length },
        { value: "open", label: "待处理", count: rows.open.length },
        { value: "matched", label: "已匹配", count: rows.matched.length },
        { value: "amount", label: "金额待核对", count: rows.amount.length },
        { value: "selected", label: "已选", count: source.length },
    ] as const

    useImperativeHandle(
        ref,
        () => ({
            locateOpenDifferences() {
                if (!firstOpenId) return
                setFilter("open")
                setKeyword("")
                setPage(1)
                setChecked(new Set())
                setExpanded(undefined)
                setPulse(true)
                setLocateSequence((sequence) => sequence + 1)
            },
        }),
        [firstOpenId],
    )

    useEffect(() => {
        if (locateSequence === 0) return
        let frame2: number | null = null
        const frame1 = requestAnimationFrame(() => {
            frame2 = requestAnimationFrame(() => {
                const target = targetRef.current
                if (!target) return
                const reduceMotion = window.matchMedia(
                    "(prefers-reduced-motion: reduce)",
                ).matches
                target.focus({ preventScroll: true })
                target.scrollIntoView({
                    block: "center",
                    behavior: reduceMotion ? "instant" : "smooth",
                })
            })
        })
        const timer = window.setTimeout(() => setPulse(false), PULSE_DURATION)
        return () => {
            cancelAnimationFrame(frame1)
            if (frame2 !== null) cancelAnimationFrame(frame2)
            window.clearTimeout(timer)
        }
    }, [locateSequence])

    function changeFilter(next: Filter) {
        setFilter(next)
        setPage(1)
        setExpanded(undefined)
        setPulse(false)
    }

    const toolbar =
        selectedRows.length > 0 ? (
            <div
                className="flex min-w-0 flex-wrap items-center gap-1.5"
                role="group"
                aria-label="已选核对项"
            >
                <span
                    className="num px-1 text-xs font-medium"
                    aria-live="polite"
                >
                    已选 {selectedRows.length} 项
                </span>
                <Button
                    id={`${idPrefix}-select-page`}
                    size="xs"
                    variant="ghost"
                    onClick={() =>
                        setChecked(
                            new Set([
                                ...checked,
                                ...pageRows.map((row) => row.id),
                            ]),
                        )
                    }
                >
                    全选本页
                </Button>
                <Button
                    id={`${idPrefix}-isolate-selection`}
                    size="xs"
                    variant="secondary"
                    onClick={() => {
                        setIsolated(new Set(selectedRows.map((row) => row.id)))
                        setChecked(new Set())
                        setKeyword("")
                        changeFilter("selected")
                    }}
                >
                    仅看已选
                </Button>
                <Button
                    id={`${idPrefix}-clear-selection`}
                    size="xs"
                    variant="ghost"
                    onClick={() => setChecked(new Set())}
                >
                    清空
                </Button>
            </div>
        ) : (
            <div className="flex min-w-0 flex-wrap items-center gap-1.5">
                {showFilters &&
                    filterOptions.map(
                        ({ value, label, count }) =>
                            count > 0 &&
                            (value !== "selected" ||
                                effectiveFilter === "selected") && (
                                <Button
                                    key={value}
                                    id={`${idPrefix}-filter-${value}`}
                                    size="xs"
                                    variant={
                                        effectiveFilter === value
                                            ? "secondary"
                                            : "ghost"
                                    }
                                    className="rounded-full"
                                    aria-pressed={effectiveFilter === value}
                                    onClick={() => changeFilter(value)}
                                >
                                    {label} <span className="num">{count}</span>
                                </Button>
                            ),
                    )}
                {showSearch && (
                    <div className="relative flex w-36 max-w-full items-center">
                        <SearchIcon
                            className="pointer-events-none absolute left-2 size-3.5 text-muted-foreground"
                            aria-hidden="true"
                        />
                        <Input
                            ref={searchRef}
                            id={`${idPrefix}-search`}
                            size="sm"
                            value={keyword}
                            aria-label="搜索结算明细或差异"
                            placeholder="商品、单号或差异"
                            className="pr-7 pl-7"
                            onChange={(event) => {
                                setKeyword(event.target.value)
                                setPage(1)
                            }}
                        />
                        {keyword && (
                            <Button
                                id={`${idPrefix}-search-clear`}
                                variant="ghost"
                                size="icon-xs"
                                aria-label="清除搜索"
                                className="absolute right-0.5"
                                onClick={() => {
                                    setKeyword("")
                                    setPage(1)
                                    searchRef.current?.focus()
                                }}
                            >
                                <XIcon aria-hidden="true" />
                            </Button>
                        )}
                    </div>
                )}
                {hasFilters && (
                    <Button
                        id={`${idPrefix}-clear-filters`}
                        type="button"
                        size="xs"
                        variant="ghost"
                        onClick={() => {
                            setKeyword("")
                            setIsolated(new Set())
                            changeFilter("all")
                        }}
                    >
                        清除筛选
                    </Button>
                )}
            </div>
        )

    const { totals } = detail
    const totalDelta = settlementDifference(totals.differenceAmountGross)

    return (
        <section aria-label="结算明细核对" className="min-w-0 space-y-2">
            <TableToolbar
                className={cn(
                    "min-h-9 gap-x-2 rounded-lg border border-transparent py-0.5",
                    selectedRows.length > 0 &&
                        "border-info-border/70 bg-info-soft/50 px-2",
                )}
                actions={selectedRows.length === 0 ? utilityActions : undefined}
            >
                {toolbar}
            </TableToolbar>
            <Table
                id={`${idPrefix}-table`}
                aria-label="结算明细与差异"
                className="table-fixed [--table-cell-inline:--spacing(2)]"
            >
                <colgroup>
                    <col className="w-8" />
                    <col />
                    <col className="w-[21%]" />
                    <col className="w-[21%]" />
                    <col className="w-[20%]" />
                </colgroup>
                <TableHeader className="bg-surface-sunken [&_tr]:border-0">
                    <TableRow>
                        <TableHead className="rounded-l-lg bg-transparent px-0">
                            <span className="sr-only">选择核对项</span>
                        </TableHead>
                        <TableHead className="bg-transparent whitespace-normal">
                            明细 / 差异
                        </TableHead>
                        <TableHead
                            data-align="end"
                            className="bg-transparent whitespace-normal"
                        >
                            ERP 含税
                        </TableHead>
                        <TableHead
                            data-align="end"
                            className="bg-transparent whitespace-normal"
                        >
                            账单含税
                        </TableHead>
                        <TableHead
                            data-align="end"
                            className="rounded-r-lg bg-transparent whitespace-normal"
                        >
                            账单差额
                        </TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {pageRows.map((row) => {
                        const rowId = `${idPrefix}-${toAutomationIdSegment(row.id)}`
                        const isOpen = expanded === row.id
                        const item = row.kind === "item" ? row.item : undefined
                        const difference =
                            row.kind === "difference"
                                ? row.difference
                                : undefined
                        const discrepant =
                            row.kind === "difference" || !row.matched
                        const blocking =
                            difference?.status === "PENDING" &&
                            difference.blocking
                        const resolved =
                            difference && difference.status !== "PENDING"
                        const flash = pulse && firstOpenId === row.id
                        const delta = settlementDifference(
                            row.kind === "item"
                                ? row.delta
                                : difference?.amountGross,
                        )
                        const label = item?.productName ?? difference?.typeLabel
                        const marker = resolved
                            ? "border-l-border"
                            : blocking
                              ? "border-l-destructive"
                              : "border-l-warning"
                        const line = resolved
                            ? "bg-surface-sunken text-muted-foreground"
                            : blocking
                              ? "bg-destructive-soft text-destructive-soft-foreground"
                              : "bg-warning-soft text-warning-soft-foreground"

                        return (
                            <Fragment key={row.id}>
                                <TableRow
                                    id={rowId}
                                    data-state={
                                        checked.has(row.id)
                                            ? "selected"
                                            : undefined
                                    }
                                    className={cn(
                                        discrepant && "border-b-0",
                                        flash &&
                                            "bg-info-soft/50 ring-1 ring-inset ring-info-border motion-safe:animate-pulse",
                                    )}
                                >
                                    <TableCell
                                        className={cn(
                                            "px-1.5",
                                            discrepant && "border-l-2",
                                            discrepant && marker,
                                        )}
                                    >
                                        <NativeCheckbox
                                            id={`${rowId}-select`}
                                            aria-label={`选择 ${label}`}
                                            checked={checked.has(row.id)}
                                            onCheckedChange={(value) => {
                                                const next = new Set(checked)
                                                if (value) next.add(row.id)
                                                else next.delete(row.id)
                                                setChecked(next)
                                            }}
                                        />
                                    </TableCell>
                                    <TableCell className="min-w-0 whitespace-normal wrap-anywhere">
                                        <Button
                                            ref={
                                                firstOpenId === row.id
                                                    ? targetRef
                                                    : undefined
                                            }
                                            id={`${rowId}-details`}
                                            type="button"
                                            size="xs"
                                            variant="ghost"
                                            className="-ml-1.5 h-auto max-w-full justify-start gap-1 px-1.5 py-1 text-left text-xs whitespace-normal"
                                            aria-expanded={isOpen}
                                            aria-controls={
                                                isOpen
                                                    ? `${rowId}-expanded`
                                                    : undefined
                                            }
                                            onClick={() => {
                                                if (!isOpen && difference)
                                                    onSelectDifference(
                                                        difference.differenceId,
                                                    )
                                                setExpanded(
                                                    isOpen ? undefined : row.id,
                                                )
                                            }}
                                        >
                                            <ChevronDownIcon
                                                className={cn(
                                                    "size-3.5 shrink-0",
                                                    !isOpen && "-rotate-90",
                                                )}
                                                aria-hidden="true"
                                            />
                                            <span className="min-w-0 wrap-anywhere">
                                                {label}
                                            </span>
                                        </Button>
                                        {item && (
                                            <span className="num block text-tiny text-muted-foreground">
                                                {item.supplierOrderNo}
                                            </span>
                                        )}
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="num whitespace-normal wrap-anywhere"
                                    >
                                        {settlementMoney(
                                            item?.erpAmountGross ??
                                                difference?.erpSideAmount,
                                        )}
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="num whitespace-normal wrap-anywhere"
                                    >
                                        {settlementMoney(
                                            item?.supplierBillLineGross ??
                                                difference?.supplierSideAmount,
                                        )}
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className={cn(
                                            "num font-medium whitespace-normal wrap-anywhere",
                                            delta.tone,
                                        )}
                                    >
                                        {delta.label}
                                    </TableCell>
                                </TableRow>
                                {discrepant && (
                                    <TableRow className="hover:bg-transparent">
                                        <TableCell
                                            colSpan={5}
                                            className="h-auto p-0 whitespace-normal"
                                        >
                                            <div
                                                className={cn(
                                                    "flex min-w-0 flex-wrap items-center gap-x-2 border-l-2 px-2.5 py-1 text-tiny wrap-anywhere",
                                                    marker,
                                                    line,
                                                )}
                                            >
                                                <span className="font-semibold">
                                                    {difference?.statusLabel ??
                                                        "金额待核对"}
                                                </span>
                                                {blocking && (
                                                    <span>阻塞复核</span>
                                                )}
                                                {difference?.requiresProcurementEvidence && (
                                                    <span>需采购举证</span>
                                                )}
                                                <span>
                                                    {difference?.amountDirectionLabel ??
                                                        (item?.supplierBillLineGross ==
                                                        null
                                                            ? "账单行金额待同步"
                                                            : "账单行与 ERP 核算金额不同")}
                                                </span>
                                            </div>
                                        </TableCell>
                                    </TableRow>
                                )}
                                {isOpen && (
                                    <TableRow
                                        id={`${rowId}-expanded`}
                                        className="hover:bg-transparent"
                                    >
                                        <TableCell
                                            colSpan={5}
                                            className="h-auto bg-surface-sunken/60 px-3 py-2 whitespace-normal wrap-anywhere"
                                        >
                                            {item ? (
                                                <SettlementItemDetails
                                                    item={item}
                                                    idPrefix={rowId}
                                                    statementId={
                                                        detail.statement.id
                                                    }
                                                />
                                            ) : (
                                                difference &&
                                                renderDifference(difference)
                                            )}
                                        </TableCell>
                                    </TableRow>
                                )}
                            </Fragment>
                        )
                    })}
                    {pageRows.length === 0 && (
                        <TableRow>
                            <TableCell
                                colSpan={5}
                                className="py-6 text-center whitespace-normal text-muted-foreground"
                            >
                                {hasFilters
                                    ? "当前条件下无匹配项"
                                    : "暂无结算明细"}
                            </TableCell>
                        </TableRow>
                    )}
                </TableBody>
                <TableFooter className="border-0 bg-surface-sunken [&_tr]:border-b-0">
                    <TableRow className="hover:bg-transparent">
                        <TableCell
                            colSpan={2}
                            className="rounded-l-lg text-xs whitespace-normal"
                        >
                            整单合计
                            {hasFilters && (
                                <span className="num block text-tiny text-muted-foreground">
                                    （当前显示 {pageRows.length} 项
                                    {shown.length > pageRows.length
                                        ? `，筛选命中 ${shown.length} 项`
                                        : ""}
                                    ，合计按整单）
                                </span>
                            )}
                        </TableCell>
                        <TableCell
                            data-align="end"
                            className="num whitespace-normal wrap-anywhere"
                        >
                            {settlementMoney(totals.erpAmountGross)}
                        </TableCell>
                        <TableCell
                            data-align="end"
                            className="num whitespace-normal wrap-anywhere"
                        >
                            {settlementMoney(totals.supplierAmountGross)}
                        </TableCell>
                        <TableCell
                            data-align="end"
                            className={cn(
                                "num rounded-r-lg whitespace-normal wrap-anywhere",
                                totalDelta.tone,
                            )}
                        >
                            {totalDelta.label}
                        </TableCell>
                    </TableRow>
                </TableFooter>
            </Table>
            {pageCount > 1 && (
                <nav
                    aria-label="结算明细分页"
                    className="flex items-center justify-end gap-2 text-xs text-muted-foreground"
                >
                    <span className="num" aria-live="polite">
                        第 {visiblePage} / {pageCount} 页
                    </span>
                    <Button
                        id={`${idPrefix}-page-prev`}
                        variant="outline"
                        size="icon-xs"
                        aria-label="上一页"
                        disabled={visiblePage === 1}
                        onClick={() => setPage(visiblePage - 1)}
                    >
                        <ChevronLeftIcon aria-hidden="true" />
                    </Button>
                    <Button
                        id={`${idPrefix}-page-next`}
                        variant="outline"
                        size="icon-xs"
                        aria-label="下一页"
                        disabled={visiblePage === pageCount}
                        onClick={() => setPage(visiblePage + 1)}
                    >
                        <ChevronRightIcon aria-hidden="true" />
                    </Button>
                </nav>
            )}
        </section>
    )
}
