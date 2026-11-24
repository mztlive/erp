"use client"

import {
    Fragment,
    useEffect,
    useImperativeHandle,
    useMemo,
    useRef,
    useState,
    type RefObject,
} from "react"
import { MapPinIcon, TriangleAlertIcon } from "lucide-react"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Table,
    TableBody,
    TableCaption,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { displayUnixDate } from "@/features/approval-workflow/display"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import type { DeliveryItemGroup } from "@/lib/delivery-allocation-summary"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type {
    EvaluatedSalesLine,
    MarginSummary,
    SalesApprovalMetrics,
    VoucherDiscountSummary,
} from "../lib/sales-approval-model"
import {
    APPROVAL_TABLE_DEFAULT_PAGE_SIZE,
    ApprovalLineFilterBar,
    ApprovalLinePagination,
} from "./workspace-approval-table-controls"

export type SalesComparisonTableHandle = {
    filterToRisks: () => void
    locateLine: (lineId: string) => void
}

type OpenLineDelivery = (lineId: string, trigger: HTMLElement) => void

export function WorkspaceSalesComparisonTable({
    rows,
    metrics,
    grossTotal,
    deliveryItems,
    showLineDelivery,
    idPrefix,
    riskOnly: controlledRiskOnly,
    onRiskOnlyChange,
    tableHandle,
    onOpenLineDelivery,
}: {
    rows: readonly EvaluatedSalesLine[]
    metrics: SalesApprovalMetrics
    grossTotal: string
    deliveryItems: ReadonlyMap<string, DeliveryItemGroup>
    showLineDelivery: boolean
    idPrefix: string
    riskOnly?: boolean
    onRiskOnlyChange?: (next: boolean) => void
    tableHandle: RefObject<SalesComparisonTableHandle | null>
    onOpenLineDelivery: OpenLineDelivery
}) {
    const [search, setSearch] = useState("")
    const [internalRiskOnly, setInternalRiskOnly] = useState(false)
    const [page, setPage] = useState(1)
    const [pageSize, setPageSize] = useState(APPROVAL_TABLE_DEFAULT_PAGE_SIZE)
    const [location, setLocation] = useState<{ lineId: string } | null>(null)
    const tableSection = useRef<HTMLElement>(null)

    const riskOnly = controlledRiskOnly ?? internalRiskOnly
    const setRiskOnly = (next: boolean) => {
        if (controlledRiskOnly === undefined) {
            setInternalRiskOnly(next)
        }
        onRiskOnlyChange?.(next)
    }

    const needle = search.trim().toLocaleLowerCase()
    const filtered = useMemo(
        () =>
            rows.filter((entry) => {
                if (riskOnly && !entry.hasRisk) return false
                if (!needle) return true
                const { line, name, spec, welfare } = entry
                return (
                    [line.line_no, name, spec, welfare]
                        .join(" ")
                        .toLocaleLowerCase()
                        .includes(needle) ||
                    (line.delivery_allocations ?? []).some((address) =>
                        `${address.recipient} ${address.phone} ${fullDeliveryAddress(address)}`
                            .toLocaleLowerCase()
                            .includes(needle),
                    )
                )
            }),
        [needle, riskOnly, rows],
    )

    const currentPage = Math.min(
        page,
        Math.max(1, Math.ceil(filtered.length / pageSize)),
    )
    const visible = filtered.slice(
        (currentPage - 1) * pageSize,
        currentPage * pageSize,
    )
    const clearFilters = () => {
        setSearch("")
        setRiskOnly(false)
        setPage(1)
    }

    useImperativeHandle(tableHandle, () => ({
        filterToRisks: () => {
            setSearch("")
            setRiskOnly(true)
            setPage(1)
            const firstRisk = rows.find((entry) => entry.hasRisk)
            if (firstRisk) {
                setLocation({ lineId: firstRisk.line.id })
                return
            }
            const reducedMotion = window.matchMedia?.(
                "(prefers-reduced-motion: reduce)",
            ).matches
            tableSection.current?.scrollIntoView({
                behavior: reducedMotion ? "instant" : "smooth",
                block: "start",
            })
        },
        locateLine: (lineId) => {
            const filteredIndex = filtered.findIndex(
                ({ line }) => line.id === lineId,
            )
            const index =
                filteredIndex >= 0
                    ? filteredIndex
                    : rows.findIndex(({ line }) => line.id === lineId)
            if (index < 0) return
            if (filteredIndex < 0) clearFilters()
            setPage(Math.floor(index / pageSize) + 1)
            setLocation({ lineId })
        },
    }))

    useEffect(() => {
        if (!location) return
        const reducedMotion = window.matchMedia?.(
            "(prefers-reduced-motion: reduce)",
        ).matches
        const frame = window.requestAnimationFrame(() => {
            const row = document.getElementById(
                `${idPrefix}-row-${toAutomationIdSegment(location.lineId)}`,
            )
            row?.focus({ preventScroll: true })
            row?.scrollIntoView({
                behavior: reducedMotion ? "instant" : "smooth",
                block: "center",
            })
        })
        const timer = window.setTimeout(() => setLocation(null), 2200)
        return () => {
            window.cancelAnimationFrame(frame)
            window.clearTimeout(timer)
        }
    }, [idPrefix, location])

    return (
        <section
            ref={tableSection}
            id={`${idPrefix}-lines-section`}
            aria-label="销售明细"
            className="min-w-0 scroll-mt-16 space-y-3 py-4"
        >
            <div className="flex flex-wrap items-baseline gap-x-2 gap-y-1">
                <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                    销售明细
                </h2>
                <span className="num text-xs text-muted-foreground">
                    共 {rows.length} 项
                    {metrics.giftCount > 0
                        ? ` · 含 ${metrics.giftCount} 项赠品`
                        : ""}{" "}
                    · 金额含税
                </span>
            </div>
            <ApprovalLineFilterBar
                idPrefix={idPrefix}
                totalCount={rows.length}
                filteredCount={filtered.length}
                riskCount={metrics.riskLineCount}
                hasHighRisk={metrics.highRiskLineCount > 0}
                riskOnly={riskOnly}
                onRiskOnlyChange={(next) => {
                    setRiskOnly(next)
                    setPage(1)
                }}
                search={search}
                onSearchChange={(next) => {
                    setSearch(next)
                    setPage(1)
                }}
                onClearFilters={clearFilters}
                ariaLabel="销售明细筛选"
            />

            {rows.length > 0 ? (
                <Table
                    data-density="comfortable"
                    aria-label="销售明细对照"
                    className="table-fixed [--table-cell-inline:--spacing(1.5)] @[480px]/sales-lines:[--table-cell-inline:--spacing(2)] [&_[data-slot=money-value]]:max-w-full [&_[data-slot=money-value-number]]:wrap-anywhere [&_[data-slot=money-value-number]]:whitespace-normal [&_[data-slot=quantity-value]]:flex-wrap [&_[data-slot=rate-value]]:whitespace-normal"
                >
                    <TableCaption className="sr-only">
                        单价和小计均含税。预估毛利按含税销售收入减含税供给成本计算。
                    </TableCaption>
                    <TableHeader className="bg-surface-sunken [&_th]:bg-surface-sunken [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md [&_tr]:border-b-0">
                        <TableRow>
                            <TableHead
                                scope="col"
                                className="w-[32%] whitespace-normal"
                            >
                                商品 / 规格
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-[13%] whitespace-normal"
                            >
                                数量
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-[33%] whitespace-normal"
                            >
                                {metrics.isCard
                                    ? "含税单价 / 面额"
                                    : "含税售价 / 供给价与毛利"}
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-[22%] whitespace-normal"
                            >
                                含税小计
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {visible.map((entry) => {
                            const lineSegment = toAutomationIdSegment(
                                entry.line.id,
                            )
                            return (
                                <SalesComparisonRow
                                    key={entry.line.id}
                                    idPrefix={`${idPrefix}-line-${lineSegment}`}
                                    rowId={`${idPrefix}-row-${lineSegment}`}
                                    entry={entry}
                                    isCard={metrics.isCard}
                                    highlighted={
                                        location?.lineId === entry.line.id
                                    }
                                    delivery={
                                        showLineDelivery
                                            ? deliveryItems.get(entry.line.id)
                                            : undefined
                                    }
                                    onOpenLineDelivery={onOpenLineDelivery}
                                />
                            )
                        })}
                        {visible.length === 0 ? (
                            <TableRow>
                                <TableCell
                                    colSpan={4}
                                    className="py-8 text-center text-xs whitespace-normal text-muted-foreground"
                                >
                                    没有匹配的销售明细，请调整筛选条件。
                                </TableCell>
                            </TableRow>
                        ) : null}
                    </TableBody>
                    <TableFooter className="border-t-0 bg-surface-sunken [&_td:first-child]:rounded-l-md [&_td:last-child]:rounded-r-md">
                        <TableRow className="hover:bg-transparent">
                            <TableCell className="py-2.5 text-xs whitespace-normal">
                                <span className="num">
                                    合计 · 共 {rows.length} 项
                                </span>
                                {filtered.length !== rows.length ? (
                                    <span className="num block font-normal text-muted-foreground">
                                        （当前显示 {filtered.length}{" "}
                                        项，合计按整单）
                                    </span>
                                ) : null}
                            </TableCell>
                            <TableCell
                                data-align="end"
                                className="py-2.5 text-xs wrap-anywhere whitespace-normal"
                            >
                                {metrics.totalQuantity ? (
                                    <QuantityValue
                                        value={metrics.totalQuantity.value}
                                        unit={metrics.totalQuantity.unit}
                                    />
                                ) : (
                                    "—"
                                )}
                            </TableCell>
                            <TableCell
                                data-align="end"
                                className="py-2.5 text-xs wrap-anywhere whitespace-normal"
                            >
                                {metrics.estimatedMargin ? (
                                    <>
                                        <SalesMarginValue
                                            margin={metrics.estimatedMargin}
                                        />
                                        {metrics.partialCostCoverage ? (
                                            <span className="num block font-normal text-muted-foreground">
                                                毛利覆盖{" "}
                                                {metrics.costCoveredCount}/
                                                {metrics.costExpectedCount} 项
                                            </span>
                                        ) : null}
                                    </>
                                ) : null}
                            </TableCell>
                            <TableCell
                                data-align="end"
                                className="py-2.5 font-semibold"
                            >
                                <MoneyValue
                                    display="exact"
                                    value={grossTotal}
                                />
                            </TableCell>
                        </TableRow>
                    </TableFooter>
                </Table>
            ) : (
                <p className="text-xs text-muted-foreground">
                    本次提交没有销售明细。
                </p>
            )}

            <ApprovalLinePagination
                idPrefix={idPrefix}
                filteredCount={filtered.length}
                page={currentPage}
                pageSize={pageSize}
                onPageChange={setPage}
                onPageSizeChange={(next) => {
                    setPageSize(next)
                    setPage(1)
                }}
                ariaLabel="销售明细分页"
            />
        </section>
    )
}

export function SalesMarginValue({
    margin,
    className,
    rateClassName,
}: {
    margin: MarginSummary
    className?: string
    rateClassName?: string
}) {
    const tone = margin.negative
        ? "text-destructive-soft-foreground"
        : margin.positive
          ? "text-success-soft-foreground"
          : "text-muted-foreground"

    return (
        <span
            className={cn(
                "num inline-flex max-w-full flex-wrap items-baseline justify-end gap-x-1.5",
                tone,
                className,
            )}
        >
            <span>
                {margin.negative ? "亏 " : margin.positive ? "赚 " : null}
                <MoneyValue
                    display="exact"
                    value={margin.absAmount}
                    inheritColor
                />
            </span>
            <RateValue
                value={margin.absPercent}
                precision={1}
                className={cn("text-tiny", tone, rateClassName)}
            />
        </span>
    )
}

export function SalesVoucherDiscountValue({
    discount,
}: {
    discount: VoucherDiscountSummary
}) {
    if (discount.kind === "premium" && discount.faceValuePercent) {
        return (
            <span className="text-warning-soft-foreground">
                溢价{" "}
                <RateValue
                    value={discount.faceValuePercent}
                    precision={1}
                    className="text-warning-soft-foreground"
                />
            </span>
        )
    }
    return <span className="num">{discount.label}</span>
}

function SalesComparisonRow({
    idPrefix,
    rowId,
    entry,
    isCard,
    highlighted,
    delivery,
    onOpenLineDelivery,
}: {
    idPrefix: string
    rowId: string
    entry: EvaluatedSalesLine
    isCard: boolean
    highlighted: boolean
    delivery?: DeliveryItemGroup
    onOpenLineDelivery: OpenLineDelivery
}) {
    const { line, unit, margin, riskDetails } = entry
    const quantity =
        line.quantity ??
        (line.card_count == null ? null : String(line.card_count))
    const dueDate =
        !isCard && line.fulfillment_due_at != null
            ? displayUnixDate(line.fulfillment_due_at)
            : null
    const meta = [
        entry.spec,
        entry.welfare,
        dueDate ? `交付 ${dueDate}` : null,
    ].filter(Boolean)
    let taxRate: string | null = null
    if (line.sales_tax_rate) {
        try {
            taxRate = multiplyFixed(line.sales_tax_rate, "100", {
                leftMaxScale: 6,
                rightMaxScale: 0,
                outputScale: 2,
            })
        } catch {
            taxRate = null
        }
    }
    const basis =
        line.procurement_price_basis === "BULK"
            ? "集采"
            : line.procurement_price_basis === "DROPSHIP"
              ? "一件代发"
              : null
    const hasSupplyPrice = Boolean(line.procurement_supply_price_gross)
    const cardForm =
        line.card_form === "PHYSICAL"
            ? "实体卡"
            : line.card_form === "ELECTRONIC"
              ? "电子券"
              : null

    return (
        <Fragment>
            <TableRow
                id={rowId}
                tabIndex={-1}
                className={cn(
                    "border-border/60 [&_td]:py-2.5 [&_td]:align-top [&_td]:wrap-anywhere [&_td]:whitespace-normal",
                    entry.severity === "high" &&
                        !highlighted &&
                        "bg-destructive-soft/35 hover:bg-destructive-soft/50",
                    entry.hasRisk && "border-b-0 [&_td]:pb-1.5",
                    highlighted &&
                        "bg-info-soft/25 ring-1 ring-info-border ring-inset motion-safe:animate-pulse",
                )}
            >
                <TableCell>
                    <div className="flex flex-wrap items-baseline gap-x-1.5">
                        <span className="num text-tiny text-muted-foreground">
                            {line.line_no}
                        </span>
                        <span className="font-medium">{entry.name}</span>
                        {entry.isGift ? (
                            <Badge variant="neutral">赠品</Badge>
                        ) : null}
                    </div>
                    {meta.length ? (
                        <p className="num mt-0.5 text-xs text-muted-foreground">
                            {meta.join(" · ")}
                        </p>
                    ) : null}
                    {delivery && delivery.uniqueAddressCount > 0 ? (
                        <button
                            id={`${idPrefix}-delivery-jump`}
                            type="button"
                            aria-haspopup="dialog"
                            className="num mt-1 inline-flex max-w-full cursor-pointer items-center gap-1 text-left text-xs text-primary hover:underline"
                            onClick={(event) =>
                                onOpenLineDelivery(line.id, event.currentTarget)
                            }
                        >
                            <MapPinIcon
                                aria-hidden="true"
                                className="size-3 shrink-0"
                            />
                            配送地址（{delivery.uniqueAddressCount} 处）
                        </button>
                    ) : null}
                </TableCell>
                <TableCell data-align="end">
                    {quantity == null ? (
                        "—"
                    ) : (
                        <QuantityValue
                            value={quantity}
                            unit={unit}
                            className="justify-end"
                        />
                    )}
                </TableCell>
                <TableCell data-align="end">
                    <div
                        className={cn(
                            "min-w-0 space-y-0.5",
                            !isCard &&
                                "@[640px]/sales-lines:grid @[640px]/sales-lines:grid-cols-2 @[640px]/sales-lines:gap-x-2",
                        )}
                    >
                        <div className="min-w-0">
                            <span className="mr-1 text-tiny text-muted-foreground">
                                售价
                            </span>
                            <MoneyValue
                                display="exact"
                                value={line.unit_price_gross}
                            />
                        </div>
                        {isCard ? (
                            <div className="text-xs text-muted-foreground">
                                面额{" "}
                                <MoneyValue
                                    display="exact"
                                    value={line.face_value}
                                />
                                {cardForm ? ` · ${cardForm}` : null}
                                {entry.voucherDiscount ? (
                                    <div>
                                        <SalesVoucherDiscountValue
                                            discount={entry.voucherDiscount}
                                        />
                                    </div>
                                ) : null}
                            </div>
                        ) : (
                            <div className="min-w-0 text-xs">
                                <div>
                                    <span className="mr-1 text-tiny text-muted-foreground">
                                        {entry.isGift ? "赠品成本" : "供给价"}
                                    </span>
                                    {hasSupplyPrice ? (
                                        <MoneyValue
                                            display="exact"
                                            value={
                                                line.procurement_supply_price_gross
                                            }
                                        />
                                    ) : (
                                        <span className="text-muted-foreground">
                                            未填
                                        </span>
                                    )}
                                </div>
                                {basis ? (
                                    <p className="text-tiny text-muted-foreground">
                                        {basis}
                                    </p>
                                ) : null}
                                {margin ? (
                                    <div>
                                        {entry.marginBasis === "unit" ? (
                                            <span className="mr-1 text-tiny text-muted-foreground">
                                                单件
                                            </span>
                                        ) : null}
                                        <SalesMarginValue margin={margin} />
                                    </div>
                                ) : null}
                            </div>
                        )}
                    </div>
                </TableCell>
                <TableCell data-align="end">
                    <MoneyValue display="exact" value={line.gross_amount} />
                    {taxRate != null ? (
                        <p className="mt-0.5 text-tiny text-muted-foreground">
                            税{" "}
                            <RateValue
                                value={taxRate}
                                precision={2}
                                className="text-muted-foreground"
                            />
                        </p>
                    ) : null}
                    <p className="text-tiny text-muted-foreground">
                        <MoneyValue
                            display="exact"
                            value={line.tax_amount}
                            inheritColor
                        />
                    </p>
                </TableCell>
            </TableRow>
            {riskDetails.length > 0 ? (
                <TableRow className="border-border/60 hover:bg-transparent">
                    <TableCell
                        colSpan={4}
                        className="h-auto pt-0 pb-2.5 whitespace-normal"
                    >
                        <ul
                            className={cn(
                                "num flex flex-wrap items-baseline gap-x-3 gap-y-0.5 rounded-sm border-l-2 px-2 py-1 text-xs wrap-anywhere",
                                entry.severity === "high"
                                    ? "border-l-destructive bg-destructive-soft/65 text-destructive-soft-foreground"
                                    : "border-l-warning bg-warning-soft/75 text-warning-soft-foreground",
                            )}
                        >
                            {riskDetails.map(
                                ({ label, difference, reference }, index) => (
                                    <li
                                        key={label}
                                        className="inline-flex max-w-full flex-wrap items-baseline gap-x-1"
                                    >
                                        {index === 0 ? (
                                            <TriangleAlertIcon
                                                aria-hidden="true"
                                                className="size-3 shrink-0 self-center"
                                            />
                                        ) : null}
                                        <span>{label}</span>
                                        {difference ? (
                                            <MoneyValue
                                                display="exact"
                                                value={difference}
                                                inheritColor
                                            />
                                        ) : null}
                                        {reference ? (
                                            <span>
                                                （原底价{" "}
                                                <MoneyValue
                                                    display="exact"
                                                    value={reference}
                                                    inheritColor
                                                />
                                                ）
                                            </span>
                                        ) : null}
                                    </li>
                                ),
                            )}
                        </ul>
                    </TableCell>
                </TableRow>
            ) : null}
        </Fragment>
    )
}
