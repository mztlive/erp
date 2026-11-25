"use client"

import { Fragment, useEffect, useMemo, useRef, useState } from "react"

import {
    APPROVAL_TABLE_DEFAULT_PAGE_SIZE,
    APPROVAL_TABLE_FILTER_THRESHOLD,
    ApprovalLineFilterBar,
    ApprovalLinePagination,
    MoneyValue,
    PurchaseProfitValue,
    QuantityValue,
    RateValue,
    TaskRiskSummaryBar,
} from "@/components/business"
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
    ApprovalPurchaseFulfillment,
    ApprovalPurchaseLine,
} from "@/features/approval-workflow/api/materials"
import {
    countUniqueDeliveryPoints,
    formatPurchaseFulfillmentSummary,
} from "@/features/approval-workflow/purchase-fulfillment"
import { evaluatePurchaseLines } from "@/features/workspace/components/workspace-purchase-comparison-model"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import { multiplyFixed, sumFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

const LOCATE_HIGHLIGHT_MS = 2200

/** 仅使用抄送接口返回的采购行，履约信息收敛为非地址摘要。 */
export function CopyPurchaseLines({
    lines,
    fulfillment,
    idPrefix = "copy-purchase-lines",
}: {
    lines: ApprovalPurchaseLine[]
    fulfillment?: ApprovalPurchaseFulfillment | null
    idPrefix?: string
}) {
    const { names } = useProductUnitNames([], lines.length > 0)
    const [search, setSearch] = useState("")
    const [riskOnly, setRiskOnly] = useState(false)
    const [page, setPage] = useState(1)
    const [pageSize, setPageSize] = useState(APPROVAL_TABLE_DEFAULT_PAGE_SIZE)
    const [highlightRisks, setHighlightRisks] = useState(false)
    const tableRef = useRef<HTMLDivElement>(null)
    const highlightTimerRef = useRef<number | null>(null)
    const scrollFrameRef = useRef<number | null>(null)

    useEffect(() => {
        return () => {
            if (highlightTimerRef.current !== null) {
                window.clearTimeout(highlightTimerRef.current)
            }
            if (scrollFrameRef.current !== null) {
                window.cancelAnimationFrame(scrollFrameRef.current)
            }
        }
    }, [])

    const rows = useMemo(() => evaluatePurchaseLines(lines), [lines])
    const riskCount = rows.filter((row) => row.hasActionableRisk).length
    const hasHighRisk = rows.some((row) => row.effectiveSeverity === "high")
    const showFilters =
        rows.length > APPROVAL_TABLE_FILTER_THRESHOLD ||
        riskCount > 0 ||
        riskOnly ||
        Boolean(search.trim())
    const term = showFilters ? search.trim().toLowerCase() : ""
    const filtered = rows.filter(
        ({ line, hasActionableRisk }) =>
            (!showFilters || !riskOnly || hasActionableRisk) &&
            (!term ||
                [line.title, line.specification, String(line.line_no)].some(
                    (value) => value?.toLowerCase().includes(term),
                )),
    )
    const currentPage = Math.min(
        page,
        Math.max(1, Math.ceil(filtered.length / pageSize)),
    )
    const start = (currentPage - 1) * pageSize
    const visible = filtered.slice(start, start + pageSize)

    const locateRisks = () => {
        setSearch("")
        setRiskOnly(true)
        setPage(1)
        setHighlightRisks(true)
        if (highlightTimerRef.current !== null) {
            window.clearTimeout(highlightTimerRef.current)
        }
        highlightTimerRef.current = window.setTimeout(() => {
            highlightTimerRef.current = null
            setHighlightRisks(false)
        }, LOCATE_HIGHLIGHT_MS)

        if (scrollFrameRef.current !== null) {
            window.cancelAnimationFrame(scrollFrameRef.current)
        }
        scrollFrameRef.current = window.requestAnimationFrame(() => {
            scrollFrameRef.current = window.requestAnimationFrame(() => {
                scrollFrameRef.current = null
                const target = tableRef.current
                if (!target) return
                const reducedMotion = window.matchMedia(
                    "(prefers-reduced-motion: reduce)",
                ).matches
                target.scrollIntoView({
                    behavior: reducedMotion ? "auto" : "smooth",
                    block: "nearest",
                })
                target.focus({ preventScroll: true })
            })
        })
    }

    const fulfillmentSummary = useMemo(
        () =>
            formatPurchaseFulfillmentSummary(
                fulfillment,
                countUniqueDeliveryPoints(
                    lines.map((line) => line.delivery_allocations),
                ),
            ),
        [fulfillment, lines],
    )
    const grossTotal = useMemo(() => {
        const amounts = lines
            .map((line) => line.gross_amount)
            .filter((value): value is string => value != null && value !== "")
        return amounts.length
            ? sumFixed(amounts, { maxScale: 2, outputScale: 2 })
            : null
    }, [lines])

    if (!lines.length) return null

    return (
        <section className="min-w-0 space-y-3">
            <div className="flex flex-wrap items-baseline justify-between gap-x-3 gap-y-1">
                <h3 className="text-section-title font-semibold">
                    本次采购提交明细
                </h3>
                <span className="num text-xs text-muted-foreground">
                    共 {lines.length} 项
                    {fulfillmentSummary ? ` · ${fulfillmentSummary}` : null}
                </span>
            </div>
            <TaskRiskSummaryBar
                riskCount={riskCount}
                severity={hasHighRisk ? "destructive" : "warning"}
                aria-label="采购明细风险提示"
                leadLabel={null}
                summary={
                    <span className="font-medium">
                        存在 <span className="num">{riskCount}</span>{" "}
                        项采购风险明细
                    </span>
                }
                actions={[
                    {
                        id: "locate-risks",
                        buttonId: `${idPrefix}-locate-risks`,
                        label: "仅看风险项",
                        count: riskCount,
                        onClick: locateRisks,
                    },
                ]}
            />
            {showFilters ? (
                <ApprovalLineFilterBar
                    idPrefix={idPrefix}
                    totalCount={lines.length}
                    filteredCount={filtered.length}
                    riskCount={riskCount}
                    hasHighRisk={hasHighRisk}
                    riskOnly={riskOnly}
                    onRiskOnlyChange={(value) => {
                        setRiskOnly(value)
                        setPage(1)
                    }}
                    search={search}
                    onSearchChange={(value) => {
                        setSearch(value)
                        setPage(1)
                    }}
                    onClearFilters={() => {
                        setSearch("")
                        setRiskOnly(false)
                        setPage(1)
                    }}
                    searchPlaceholder="搜索商品、规格或序号"
                />
            ) : null}

            <div
                ref={tableRef}
                tabIndex={-1}
                className="min-w-0 scroll-mt-16 focus:outline-none"
            >
                <Table
                    aria-label="本次采购提交明细"
                    className="table-fixed [&_:is(th,td)]:px-1.5 @[520px]/copy-detail:[&_:is(th,td)]:px-2.5 [&_[data-slot=money-value-number]]:whitespace-normal [&_[data-slot=money-value-number]]:wrap-anywhere"
                >
                    <TableHeader className="bg-transparent [&_th]:bg-surface-sunken [&_th]:hover:bg-surface-sunken [&_th]:whitespace-normal [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md [&_tr]:border-b-0">
                        <TableRow>
                            <TableHead scope="col" className="w-1/3">
                                商品 / 规格
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-1/6"
                            >
                                数量
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-1/4"
                            >
                                含税单价
                            </TableHead>
                            <TableHead
                                scope="col"
                                data-align="end"
                                className="w-1/4"
                            >
                                含税金额
                            </TableHead>
                        </TableRow>
                    </TableHeader>
                    <TableBody>
                        {visible.map(
                            ({
                                line,
                                comparison,
                                externalLoss,
                                effectiveSeverity,
                                hasActionableRisk,
                            }) => {
                                const unit =
                                    line.unitName ??
                                    names.get(line.unit?.trim() ?? "")
                                const spec =
                                    line.specification === "无规格"
                                        ? null
                                        : line.specification
                                const taxRate =
                                    line.input_tax_rate != null
                                        ? multiplyFixed(
                                              line.input_tax_rate,
                                              "100",
                                              {
                                                  leftMaxScale: 6,
                                                  rightMaxScale: 0,
                                                  outputScale: 2,
                                              },
                                          )
                                        : null
                                return (
                                    <Fragment key={line.id}>
                                        <TableRow
                                            className={cn(
                                                "border-border/60",
                                                hasActionableRisk &&
                                                    "border-b-0",
                                                highlightRisks &&
                                                    hasActionableRisk &&
                                                    "bg-info-soft/35 ring-1 ring-inset ring-info-border motion-safe:animate-pulse",
                                            )}
                                        >
                                            <TableCell className="py-2.5 align-top whitespace-normal wrap-anywhere">
                                                <div className="font-medium text-foreground">
                                                    <span className="num mr-1.5 text-tiny text-muted-foreground">
                                                        {line.line_no}
                                                    </span>
                                                    {line.title}
                                                </div>
                                                {spec ? (
                                                    <p className="text-xs text-muted-foreground">
                                                        {spec}
                                                    </p>
                                                ) : null}
                                                {line.expected_delivery_date ? (
                                                    <p className="num text-tiny text-muted-foreground">
                                                        交付{" "}
                                                        {
                                                            line.expected_delivery_date
                                                        }
                                                    </p>
                                                ) : null}
                                            </TableCell>
                                            <TableCell
                                                data-align="end"
                                                className="num py-2.5 align-top whitespace-normal wrap-anywhere"
                                            >
                                                {line.quantity == null ? (
                                                    "—"
                                                ) : (
                                                    <QuantityValue
                                                        value={line.quantity}
                                                        unit={unit}
                                                        className="whitespace-normal"
                                                    />
                                                )}
                                            </TableCell>
                                            <TableCell
                                                data-align="end"
                                                className="num py-2.5 align-top whitespace-normal"
                                            >
                                                <MoneyValue
                                                    display="exact"
                                                    value={line.unit_cost_gross}
                                                />
                                                {taxRate != null ? (
                                                    <p className="mt-0.5 text-tiny text-muted-foreground">
                                                        进项{" "}
                                                        <RateValue
                                                            value={taxRate}
                                                            precision={2}
                                                        />
                                                    </p>
                                                ) : null}
                                            </TableCell>
                                            <TableCell
                                                data-align="end"
                                                className="num py-2.5 align-top whitespace-normal"
                                            >
                                                <MoneyValue
                                                    display="exact"
                                                    value={line.gross_amount}
                                                />
                                                {comparison.purchaseMargin ? (
                                                    <p className="mt-0.5 text-tiny text-muted-foreground">
                                                        <span>采购毛利 </span>
                                                        <PurchaseProfitValue
                                                            className="whitespace-normal"
                                                            amount={
                                                                comparison
                                                                    .purchaseMargin
                                                                    .margin
                                                            }
                                                        />
                                                    </p>
                                                ) : null}
                                            </TableCell>
                                        </TableRow>
                                        {hasActionableRisk ? (
                                            <TableRow className="border-border/60 hover:bg-transparent">
                                                <TableCell
                                                    colSpan={4}
                                                    className="h-auto pb-2 whitespace-normal"
                                                >
                                                    <p
                                                        className={cn(
                                                            "num rounded-r-md border-l-2 px-2 py-1 text-tiny wrap-anywhere",
                                                            effectiveSeverity ===
                                                                "high"
                                                                ? "border-l-destructive bg-destructive-soft/65 text-destructive-soft-foreground"
                                                                : "border-l-warning bg-warning-soft/75 text-warning-soft-foreground",
                                                        )}
                                                    >
                                                        {[
                                                            ...(externalLoss
                                                                ? [
                                                                      "对外售价低于采购价",
                                                                  ]
                                                                : []),
                                                            ...comparison.actionableWarnings,
                                                        ].join(" · ")}
                                                    </p>
                                                </TableCell>
                                            </TableRow>
                                        ) : null}
                                    </Fragment>
                                )
                            },
                        )}
                        {visible.length === 0 ? (
                            <TableRow>
                                <TableCell
                                    colSpan={4}
                                    className="h-16 text-center text-xs text-muted-foreground"
                                >
                                    没有符合条件的采购明细
                                </TableCell>
                            </TableRow>
                        ) : null}
                    </TableBody>
                    <TableFooter className="border-t-0 bg-transparent [&_td]:bg-surface-sunken [&_td:first-child]:rounded-l-md [&_td:last-child]:rounded-r-md [&_tr]:border-b-0">
                        <TableRow>
                            <TableCell
                                colSpan={3}
                                className="py-2 text-xs whitespace-normal text-muted-foreground"
                            >
                                整单合计 ·{" "}
                                <span className="num">{lines.length}</span> 项
                                {visible.length !== lines.length ? (
                                    <>
                                        （当前显示{" "}
                                        <span className="num">
                                            {visible.length}
                                        </span>{" "}
                                        项，合计按整单）
                                    </>
                                ) : null}
                            </TableCell>
                            <TableCell
                                data-align="end"
                                className="num py-2 font-semibold"
                            >
                                <MoneyValue
                                    display="exact"
                                    value={grossTotal}
                                />
                            </TableCell>
                        </TableRow>
                    </TableFooter>
                </Table>
            </div>
            {filtered.length > APPROVAL_TABLE_DEFAULT_PAGE_SIZE ||
            pageSize !== APPROVAL_TABLE_DEFAULT_PAGE_SIZE ? (
                <ApprovalLinePagination
                    idPrefix={idPrefix}
                    page={currentPage}
                    pageSize={pageSize}
                    filteredCount={filtered.length}
                    onPageChange={setPage}
                    onPageSizeChange={(value) => {
                        setPageSize(value)
                        setPage(1)
                    }}
                    ariaLabel="采购明细分页"
                />
            ) : null}
        </section>
    )
}
