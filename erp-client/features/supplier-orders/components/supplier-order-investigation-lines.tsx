"use client"

import { Fragment, useEffect, useRef, useState } from "react"
import {
    ChevronLeftIcon,
    ChevronRightIcon,
    TriangleAlertIcon,
} from "lucide-react"

import { MoneyValue, QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
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
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compactFixed,
    compareDecimal,
    minFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

import type { SupplierExecutionOrderTree } from "../api/execution-setup"
import type { SupplierOrderDetailView } from "../types"

const quantityScale = { maxScale: 6, outputScale: 6 }
const pageSize = 20

type InvestigationLine = SupplierOrderDetailView["items"][number] & {
    remoteQuantity?: string
    mismatch: boolean
}

export function getInvestigationLines(
    items: SupplierOrderDetailView["items"],
    tree: SupplierExecutionOrderTree | null | undefined,
): InvestigationLine[] {
    const quantities = new Map<string, string[]>()
    const leaves = tree?.nodes.filter((node) => node.role === "LEAF") ?? []
    const treeVerified =
        tree &&
        !tree.pending &&
        leaves.length > 0 &&
        leaves.every(
            (node) =>
                node.confirmation === "CONFIRMED" && node.lines.length > 0,
        )
    const skuCounts = new Map<string, number>()

    for (const item of items) {
        skuCounts.set(item.skuCode, (skuCounts.get(item.skuCode) ?? 0) + 1)
    }
    if (treeVerified) {
        for (const node of leaves) {
            for (const line of node.lines) {
                // 单位必须一致，供应商包装量不能直接和 ERP 基本单位比较。
                const sku = JSON.stringify([line.sku_code, line.unit])
                const values = quantities.get(sku) ?? []
                values.push(line.quantity)
                quantities.set(sku, values)
            }
        }
    }

    return items.map((item) => {
        const values =
            item.skuCode && item.unit && skuCounts.get(item.skuCode) === 1
                ? quantities.get(JSON.stringify([item.skuCode, item.unit]))
                : undefined
        const remoteQuantity = values
            ? sumFixed(values, quantityScale)
            : undefined
        return {
            ...item,
            remoteQuantity,
            mismatch:
                remoteQuantity !== undefined &&
                compareDecimal(
                    remoteQuantity,
                    item.quantity,
                    quantityScale.maxScale,
                ) !== 0,
        }
    })
}

export function confirmedQuantity(lines: InvestigationLine[]) {
    return sumFixed(
        lines.flatMap((line) =>
            line.remoteQuantity === undefined
                ? []
                : [minFixed(line.remoteQuantity, line.quantity, quantityScale)],
        ),
        quantityScale,
    )
}

export function SupplierInvestigationLines({
    idPrefix,
    lines,
    totalQuantity,
    totalCost,
    singleUnit,
    onOpenDiagnostics,
    locateRequest,
}: {
    idPrefix: string
    lines: InvestigationLine[]
    totalQuantity: string
    totalCost: string | null
    singleUnit: string | undefined
    onOpenDiagnostics: () => void
    locateRequest: { itemId: string } | null
}) {
    const [filter, setFilter] = useState("")
    const [riskOnly, setRiskOnly] = useState(false)
    const [page, setPage] = useState(0)
    const [highlight, setHighlight] = useState<typeof locateRequest>(null)
    const appliedLocation = useRef<typeof locateRequest>(null)
    const riskCount = lines.filter((line) => line.mismatch).length
    const showSearch = lines.length > 8 || Boolean(filter)
    const showRiskFilter = riskCount > 0 || riskOnly
    const visibleFilter = showSearch || showRiskFilter
    const keyword = showSearch ? filter.trim().toLocaleLowerCase() : ""
    const filtered = lines.filter(
        (line) =>
            (!showRiskFilter || !riskOnly || line.mismatch) &&
            (!keyword ||
                `${line.productName} ${line.skuCode}`
                    .toLocaleLowerCase()
                    .includes(keyword)),
    )
    const isFiltered =
        (Boolean(keyword) || riskOnly) && filtered.length !== lines.length
    const showPagination = filtered.length > pageSize
    const lastPage = Math.max(0, Math.ceil(filtered.length / pageSize) - 1)
    const currentPage = Math.min(page, lastPage)
    const rows = filtered.slice(
        currentPage * pageSize,
        (currentPage + 1) * pageSize,
    )
    const rowId = (itemId: string) =>
        `${idPrefix}-line-${toAutomationIdSegment(itemId)}`

    useEffect(() => {
        if (!locateRequest || appliedLocation.current === locateRequest) return
        appliedLocation.current = locateRequest
        const index = lines.findIndex(
            (line) => line.itemId === locateRequest.itemId,
        )
        if (index < 0) return
        setFilter("")
        setRiskOnly(false)
        setPage(Math.floor(index / pageSize))
        setHighlight(locateRequest)
    }, [lines, locateRequest])

    useEffect(() => {
        if (!highlight) return
        const timeout = window.setTimeout(() => setHighlight(null), 2200)
        const target = document.getElementById(
            `${idPrefix}-line-${toAutomationIdSegment(highlight.itemId)}`,
        )
        if (!target) return () => window.clearTimeout(timeout)
        const reduceMotion = window.matchMedia(
            "(prefers-reduced-motion: reduce)",
        ).matches
        target.focus({ preventScroll: true })
        target.scrollIntoView({
            block: "center",
            behavior: reduceMotion ? "auto" : "smooth",
        })
        return () => window.clearTimeout(timeout)
    }, [idPrefix, highlight])

    return (
        <section
            aria-labelledby={`${idPrefix}-lines-heading`}
            className="min-w-0 space-y-2.5"
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h2
                    id={`${idPrefix}-lines-heading`}
                    className="text-section-title font-semibold"
                >
                    商品明细
                </h2>
                <span className="num text-xs text-muted-foreground">
                    共 {lines.length} 行
                </span>
            </div>
            {visibleFilter ? (
                <div className="flex flex-wrap items-center gap-2">
                    {showSearch ? (
                        <Input
                            id={`${idPrefix}-lines-filter`}
                            size="sm"
                            aria-label="按商品名称或供应商 SKU 筛选"
                            placeholder="搜索商品名称或供应商 SKU"
                            className="min-w-0 flex-[1_1_12rem]"
                            value={filter}
                            onChange={(event) => {
                                setFilter(event.target.value)
                                setPage(0)
                            }}
                        />
                    ) : null}
                    {showRiskFilter ? (
                        <Button
                            id={`${idPrefix}-lines-risk-filter`}
                            type="button"
                            variant={riskOnly ? "secondary" : "outline"}
                            size="sm"
                            aria-pressed={riskOnly}
                            onClick={() => {
                                setRiskOnly(!riskOnly)
                                setPage(0)
                            }}
                        >
                            仅看异常 <span className="num">{riskCount}</span>
                        </Button>
                    ) : null}
                </div>
            ) : null}
            <Table className="table-fixed border-separate border-spacing-0 text-left wrap-anywhere [&_td]:h-auto [&_td]:align-top [&_td]:whitespace-normal [&_th]:h-auto [&_th]:whitespace-normal">
                <TableHeader className="bg-transparent [&_tr]:border-0 [&_th]:bg-surface-sunken [&_th]:text-muted-foreground [&_th:hover]:bg-surface-sunken">
                    <TableRow>
                        <TableHead
                            scope="col"
                            className="w-[40%] rounded-l-lg px-2.5 py-2 font-medium"
                        >
                            商品 / SKU
                        </TableHead>
                        <TableHead
                            scope="col"
                            className="px-2 py-2 text-right font-medium"
                        >
                            ERP 下单量
                        </TableHead>
                        <TableHead
                            scope="col"
                            className={cn(
                                "px-2 py-2 text-right font-medium",
                                totalCost === null && "rounded-r-lg",
                            )}
                        >
                            供应商确认量
                        </TableHead>
                        {totalCost !== null ? (
                            <TableHead
                                scope="col"
                                className="rounded-r-lg px-2.5 py-2 text-right font-medium"
                            >
                                含税单价
                            </TableHead>
                        ) : null}
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {rows.map((line) => {
                        const rowBorder = line.mismatch
                            ? "border-b-0"
                            : "border-b border-border/60"
                        const quantityDelta =
                            line.mismatch && line.remoteQuantity !== undefined
                                ? subtractFixed(
                                      line.remoteQuantity,
                                      line.quantity,
                                      quantityScale,
                                  )
                                : undefined
                        const deltaSign =
                            quantityDelta !== undefined
                                ? compareDecimal(
                                      quantityDelta,
                                      "0",
                                      quantityScale.maxScale,
                                  )
                                : 0
                        const deltaLabel =
                            quantityDelta !== undefined && deltaSign !== 0
                                ? `${deltaSign > 0 ? "多确认 +" : "少确认 -"}${compactFixed(quantityDelta.replace(/^-/, ""))}${line.unit ? ` ${line.unit}` : ""}`
                                : null
                        return (
                            <Fragment key={line.itemId}>
                                <TableRow
                                    id={rowId(line.itemId)}
                                    tabIndex={-1}
                                    className={cn(
                                        "align-top focus-visible:outline-2 focus-visible:-outline-offset-2 focus-visible:outline-primary",
                                        highlight?.itemId === line.itemId &&
                                            "bg-info-soft/35 ring-1 ring-info-border ring-inset motion-safe:animate-pulse",
                                    )}
                                >
                                    <TableHead
                                        scope="row"
                                        className={cn(
                                            rowBorder,
                                            "bg-transparent px-2.5 py-2 text-left align-top font-medium text-foreground hover:bg-transparent",
                                        )}
                                    >
                                        {line.productName}
                                        <span className="num block font-normal text-muted-foreground">
                                            {line.skuCode || "—"}
                                        </span>
                                    </TableHead>
                                    <TableCell
                                        className={cn(
                                            rowBorder,
                                            "px-2 py-2 text-right",
                                        )}
                                    >
                                        <QuantityValue
                                            value={line.quantity}
                                            unit={line.unit}
                                            className="justify-end whitespace-normal [&_*]:whitespace-normal"
                                        />
                                    </TableCell>
                                    <TableCell
                                        className={cn(
                                            rowBorder,
                                            "px-2 py-2 text-right",
                                            line.mismatch &&
                                                "font-semibold text-warning-soft-foreground",
                                        )}
                                    >
                                        {line.remoteQuantity === undefined ? (
                                            <span className="text-muted-foreground">
                                                待核实
                                            </span>
                                        ) : (
                                            <QuantityValue
                                                value={line.remoteQuantity}
                                                unit={line.unit}
                                                className="justify-end whitespace-normal [&_*]:whitespace-normal"
                                            />
                                        )}
                                    </TableCell>
                                    {totalCost !== null ? (
                                        <TableCell
                                            className={cn(
                                                rowBorder,
                                                "px-2.5 py-2 text-right",
                                            )}
                                        >
                                            <MoneyValue
                                                value={line.unitCostGross}
                                                display="exact"
                                                className="[&_[data-slot=money-value-number]]:whitespace-normal"
                                            />
                                        </TableCell>
                                    ) : null}
                                </TableRow>
                                {line.mismatch ? (
                                    <TableRow
                                        className={cn(
                                            highlight?.itemId === line.itemId &&
                                                "motion-safe:animate-pulse",
                                        )}
                                    >
                                        <TableCell
                                            colSpan={totalCost === null ? 3 : 4}
                                            aria-label="商品数量差异"
                                            className="border-b border-border/60 p-0"
                                        >
                                            <div className="flex flex-wrap items-center justify-between gap-x-2 border-l-2 border-l-warning bg-warning-soft/60 px-2.5 py-1 text-warning-soft-foreground">
                                                <span className="inline-flex min-w-0 items-center gap-1.5">
                                                    <TriangleAlertIcon
                                                        aria-hidden="true"
                                                        className="size-3.5 shrink-0"
                                                    />
                                                    <span>
                                                        供应商确认量与 ERP
                                                        下单量不一致
                                                        {deltaLabel ? (
                                                            <>
                                                                （
                                                                <span className="num font-medium">
                                                                    {deltaLabel}
                                                                </span>
                                                                ）
                                                            </>
                                                        ) : null}
                                                    </span>
                                                </span>
                                                <Button
                                                    id={`${rowId(line.itemId)}-diagnostics`}
                                                    type="button"
                                                    variant="ghost"
                                                    size="xs"
                                                    onClick={onOpenDiagnostics}
                                                >
                                                    查看同步诊断
                                                </Button>
                                            </div>
                                        </TableCell>
                                    </TableRow>
                                ) : null}
                            </Fragment>
                        )
                    })}
                    {!rows.length ? (
                        <TableRow>
                            <TableCell
                                colSpan={totalCost === null ? 3 : 4}
                                className="px-2.5 py-5 text-center text-muted-foreground"
                            >
                                {lines.length
                                    ? "没有符合条件的商品"
                                    : "暂无商品明细"}
                            </TableCell>
                        </TableRow>
                    ) : null}
                </TableBody>
                {lines.length > 0 ? (
                    <TableFooter className="border-0 bg-transparent font-medium [&_tr]:border-0 [&_tr:hover]:bg-transparent [&_td]:bg-surface-sunken [&_th]:bg-surface-sunken [&_th:hover]:bg-surface-sunken">
                        <TableRow>
                            <TableHead
                                scope="row"
                                className="rounded-l-lg px-2.5 py-2 font-medium"
                            >
                                整单合计{" "}
                                <span className="num">{lines.length}</span> 行
                                {isFiltered ? (
                                    <span className="num font-normal text-muted-foreground">
                                        （当前显示 {filtered.length}{" "}
                                        行，合计按整单）
                                    </span>
                                ) : null}
                            </TableHead>
                            <TableCell
                                colSpan={totalCost === null ? 2 : 1}
                                className={cn(
                                    "px-2 py-2 text-right",
                                    totalCost === null && "rounded-r-lg",
                                )}
                            >
                                {singleUnit ? (
                                    <QuantityValue
                                        value={totalQuantity}
                                        unit={singleUnit}
                                        className="whitespace-normal [&_*]:whitespace-normal"
                                    />
                                ) : (
                                    "多单位"
                                )}
                            </TableCell>
                            {totalCost !== null ? (
                                <TableCell
                                    colSpan={2}
                                    className="rounded-r-lg px-2.5 py-2 text-right"
                                >
                                    含税成本{" "}
                                    <MoneyValue
                                        value={totalCost}
                                        display="exact"
                                        className="[&_[data-slot=money-value-number]]:whitespace-normal"
                                    />
                                </TableCell>
                            ) : null}
                        </TableRow>
                    </TableFooter>
                ) : null}
            </Table>
            {showPagination ? (
                <div className="flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                    <span className="num">筛选结果 {filtered.length} 行</span>
                    <div className="flex items-center gap-2">
                        <Button
                            id={`${idPrefix}-lines-prev`}
                            aria-label="上一页商品"
                            variant="ghost"
                            size="icon-xs"
                            disabled={currentPage === 0}
                            onClick={() => setPage(currentPage - 1)}
                        >
                            <ChevronLeftIcon aria-hidden="true" />
                        </Button>
                        <span className="num" aria-live="polite">
                            {currentPage + 1} / {lastPage + 1}
                        </span>
                        <Button
                            id={`${idPrefix}-lines-next`}
                            aria-label="下一页商品"
                            variant="ghost"
                            size="icon-xs"
                            disabled={currentPage === lastPage}
                            onClick={() => setPage(currentPage + 1)}
                        >
                            <ChevronRightIcon aria-hidden="true" />
                        </Button>
                    </div>
                </div>
            ) : null}
        </section>
    )
}
