"use client"

import { Fragment, useEffect, useRef, useState } from "react"
import { ChevronLeftIcon, ChevronRightIcon } from "lucide-react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
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
import { compactFixed, multiplyFixed, subtractFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { PurchaseOrderCenterView } from "../types"
import {
    buildPurchaseSourceImpact,
    PurchaseSourceDelta,
} from "./purchase-source-update-context"

const PAGE_SIZE = 20
const CELL = "px-2.5 py-2 whitespace-normal wrap-anywhere"
type LineFilter = "all" | "impacted"
export type PurchaseSourceLinesView = {
    filter: LineFilter
    search: string
    page: number
    jump: number
}

const maskAmount = (
    <span className="text-muted-foreground" aria-label="金额已隐藏">
        •••
    </span>
)

export function PurchaseSourceUpdateLines({
    order,
    idPrefix = `purchase-source-update-${toAutomationIdSegment(order.identity.purchaseOrderId)}`,
    impact = buildPurchaseSourceImpact(order),
    jump = 0,
    view: controlledView,
    onViewChange,
}: {
    order: PurchaseOrderCenterView
    idPrefix?: string
    impact?: ReturnType<typeof buildPurchaseSourceImpact>
    jump?: number
    view?: PurchaseSourceLinesView
    onViewChange?: (next: PurchaseSourceLinesView) => void
}) {
    const { lines, costMasked, totals } = order.currentContent
    const [uncontrolledView, setUncontrolledView] =
        useState<PurchaseSourceLinesView>({
            filter: "all",
            search: "",
            page: 0,
            jump: 0,
        })
    const view = controlledView ?? uncontrolledView
    const setView = onViewChange ?? setUncontrolledView
    const [pulsing, setPulsing] = useState(false)
    const firstImpact = useRef<HTMLTableRowElement>(null)
    const unlinkedSection = useRef<HTMLDivElement>(null)
    const tableSection = useRef<HTMLElement>(null)
    const active =
        view.jump === jump
            ? view
            : ({
                  filter: "impacted",
                  search: "",
                  page: 0,
                  jump,
              } satisfies PurchaseSourceLinesView)
    const filter = impact.byLine.size > 0 ? active.filter : "all"
    const search = active.search.trim().toLocaleLowerCase()
    const filtered = lines.filter(
        (line) =>
            (filter === "all" || impact.byLine.has(line.lineId)) &&
            (!search ||
                [line.itemName, line.itemSku, line.salesAllocationLabel].some(
                    (value) => value?.toLocaleLowerCase().includes(search),
                )),
    )
    const pages = Math.max(1, Math.ceil(filtered.length / PAGE_SIZE))
    const page = Math.min(active.page, pages - 1)
    const visible = filtered.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE)
    const firstImpactId = visible.find((line) =>
        impact.byLine.has(line.lineId),
    )?.lineId

    useEffect(() => {
        if (jump === 0) return
        const target =
            firstImpact.current ??
            unlinkedSection.current ??
            tableSection.current
        const behavior = window.matchMedia("(prefers-reduced-motion: reduce)")
            .matches
            ? "auto"
            : "smooth"
        target?.focus({ preventScroll: true })
        target?.scrollIntoView({ behavior, block: "nearest" })

        // Allow repeated jumps to restart the highlight.
        const frame = window.requestAnimationFrame(() => setPulsing(true))
        const timeout = window.setTimeout(() => setPulsing(false), 2200)
        return () => {
            window.cancelAnimationFrame(frame)
            window.clearTimeout(timeout)
            setPulsing(false)
        }
    }, [jump])

    const amount = (value?: string) =>
        costMasked ? (
            maskAmount
        ) : (
            <MoneyValue
                value={value}
                display="exact"
                className="max-w-full wrap-anywhere"
            />
        )
    const showFilters = lines.length > 8 || impact.byLine.size > 0

    return (
        <section
            ref={tableSection}
            id={`${idPrefix}-lines`}
            aria-labelledby={`${idPrefix}-lines-title`}
            tabIndex={-1}
            className="min-w-0 scroll-mt-4 space-y-2.5"
        >
            <h2
                id={`${idPrefix}-lines-title`}
                className="text-section-title font-semibold"
            >
                采购明细
            </h2>
            {showFilters ? (
                <div
                    role="group"
                    aria-label="筛选采购明细"
                    className="flex flex-wrap items-center gap-1.5"
                >
                    {impact.byLine.size > 0 ? (
                        <>
                            <Button
                                id={`${idPrefix}-lines-all`}
                                type="button"
                                size="xs"
                                variant={
                                    filter === "all" ? "secondary" : "ghost"
                                }
                                aria-pressed={filter === "all"}
                                onClick={() =>
                                    setView({
                                        ...active,
                                        filter: "all",
                                        page: 0,
                                    })
                                }
                            >
                                全部 <span className="num">{lines.length}</span>
                            </Button>
                            <Button
                                id={`${idPrefix}-lines-impacted`}
                                type="button"
                                size="xs"
                                variant={
                                    filter === "impacted"
                                        ? "secondary"
                                        : "ghost"
                                }
                                aria-pressed={filter === "impacted"}
                                onClick={() =>
                                    setView({
                                        ...active,
                                        filter: "impacted",
                                        page: 0,
                                    })
                                }
                            >
                                受影响{" "}
                                <span className="num">
                                    {impact.byLine.size}
                                </span>
                            </Button>
                        </>
                    ) : null}
                    {lines.length > 8 ? (
                        <Input
                            id={`${idPrefix}-lines-search`}
                            type="search"
                            aria-label="搜索采购商品或规格"
                            placeholder="搜索商品或规格"
                            value={active.search}
                            className="h-control-sm max-w-full border-transparent bg-surface-sunken shadow-none hover:bg-card focus-visible:bg-card @[480px]/source-update:ml-auto @[480px]/source-update:w-52"
                            onChange={(event) =>
                                setView({
                                    ...active,
                                    search: event.target.value,
                                    page: 0,
                                })
                            }
                        />
                    ) : null}
                </div>
            ) : null}

            <Table className="min-w-0 table-fixed border-separate border-spacing-0">
                <caption className="sr-only">
                    当前采购明细与销售变更影响
                </caption>
                <colgroup>
                    <col className="w-[37%]" />
                    <col className="w-[19%]" />
                    <col className="w-[20%]" />
                    <col className="w-[24%]" />
                </colgroup>
                <TableHeader className="bg-transparent [&_tr]:border-0">
                    <TableRow className="border-0">
                        <TableHead
                            scope="col"
                            className={cn(
                                CELL,
                                "rounded-l-lg border-0 bg-surface-sunken hover:bg-surface-sunken",
                            )}
                        >
                            商品 / 费用
                        </TableHead>
                        <TableHead
                            scope="col"
                            className={cn(
                                CELL,
                                "border-0 bg-surface-sunken text-right hover:bg-surface-sunken",
                            )}
                        >
                            采购数量
                        </TableHead>
                        <TableHead
                            scope="col"
                            className={cn(
                                CELL,
                                "border-0 bg-surface-sunken text-right hover:bg-surface-sunken",
                            )}
                        >
                            含税单价
                        </TableHead>
                        <TableHead
                            scope="col"
                            className={cn(
                                CELL,
                                "rounded-r-lg border-0 bg-surface-sunken text-right hover:bg-surface-sunken",
                            )}
                        >
                            含税小计
                        </TableHead>
                    </TableRow>
                </TableHeader>
                <TableBody>
                    {visible.length === 0 ? (
                        <TableRow>
                            <TableCell
                                colSpan={4}
                                className="py-6 text-center text-xs text-muted-foreground"
                            >
                                {lines.length
                                    ? "没有符合条件的采购明细"
                                    : "暂无采购明细"}
                            </TableCell>
                        </TableRow>
                    ) : null}
                    {visible.map((line) => {
                        const id = `${idPrefix}-line-${toAutomationIdSegment(line.lineId)}`
                        const delta = impact.byLine.get(line.lineId)
                        const isFee = line.lineType === "LOGISTICS_FEE"
                        const border = delta?.destructive
                            ? "border-l-destructive"
                            : "border-l-warning"
                        const pulse =
                            delta && pulsing
                                ? "bg-info-soft/25 ring-1 ring-info-border ring-inset motion-safe:animate-pulse"
                                : undefined
                        const amountDelta =
                            delta?.targetQuantity != null && !costMasked
                                ? subtractFixed(
                                      multiplyFixed(
                                          delta.targetQuantity,
                                          compactFixed(line.unitCostGross),
                                          {
                                              leftMaxScale: 6,
                                              rightMaxScale: 4,
                                              outputScale: 2,
                                          },
                                      ),
                                      compactFixed(line.grossAmount),
                                      { maxScale: 2, outputScale: 2 },
                                  )
                                : undefined
                        const lineRule = !delta && "border-b border-grid/75"

                        return (
                            <Fragment key={line.lineId}>
                                <TableRow
                                    ref={
                                        firstImpactId === line.lineId
                                            ? firstImpact
                                            : undefined
                                    }
                                    id={id}
                                    tabIndex={delta ? -1 : undefined}
                                    aria-describedby={
                                        delta ? `${id}-impact` : undefined
                                    }
                                    className={cn(
                                        "scroll-mt-4 border-0",
                                        pulse,
                                    )}
                                >
                                    <TableCell
                                        className={cn(
                                            CELL,
                                            lineRule,
                                            delta && ["border-l-2", border],
                                        )}
                                    >
                                        <div className="font-medium">
                                            {line.itemName}
                                        </div>
                                        {line.itemSku ||
                                        line.salesAllocationLabel ||
                                        isFee ? (
                                            <div className="num mt-0.5 text-xs text-muted-foreground">
                                                {[
                                                    line.itemSku,
                                                    isFee
                                                        ? "费用"
                                                        : line.salesAllocationLabel,
                                                ]
                                                    .filter(Boolean)
                                                    .join(" · ")}
                                            </div>
                                        ) : null}
                                        {line.expectedDeliveryDate ? (
                                            <div className="num mt-0.5 text-xs text-muted-foreground">
                                                预计交付{" "}
                                                {line.expectedDeliveryDate}
                                            </div>
                                        ) : null}
                                    </TableCell>
                                    <TableCell
                                        className={cn(
                                            CELL,
                                            lineRule,
                                            "text-right",
                                        )}
                                    >
                                        {isFee || line.quantity == null ? (
                                            "—"
                                        ) : (
                                            <QuantityValue
                                                value={line.quantity}
                                                unit={line.unitName}
                                                className="max-w-full flex-wrap justify-end"
                                            />
                                        )}
                                    </TableCell>
                                    <TableCell
                                        className={cn(
                                            CELL,
                                            lineRule,
                                            "text-right",
                                        )}
                                    >
                                        {isFee
                                            ? "—"
                                            : amount(line.unitCostGross)}
                                        {!isFee &&
                                        line.salesUnitPriceGross != null ? (
                                            <span className="mt-0.5 block text-xs text-muted-foreground">
                                                销售{" "}
                                                <MoneyValue
                                                    value={
                                                        line.salesUnitPriceGross
                                                    }
                                                    display="exact"
                                                    inheritColor
                                                />
                                            </span>
                                        ) : null}
                                    </TableCell>
                                    <TableCell
                                        className={cn(
                                            CELL,
                                            lineRule,
                                            "text-right",
                                        )}
                                    >
                                        {amount(line.grossAmount)}
                                        <span className="mt-0.5 block text-xs text-muted-foreground">
                                            税率{" "}
                                            <RateValue
                                                value={multiplyFixed(
                                                    compactFixed(
                                                        line.inputTaxRate,
                                                    ),
                                                    "100",
                                                    {
                                                        leftMaxScale: 6,
                                                        rightMaxScale: 0,
                                                        outputScale: 2,
                                                    },
                                                )}
                                                precision={2}
                                                className="text-inherit"
                                            />
                                            <span className="block">
                                                税额 {amount(line.taxAmount)}
                                            </span>
                                        </span>
                                    </TableCell>
                                </TableRow>
                                {delta ? (
                                    <TableRow className={cn("border-0", pulse)}>
                                        <TableCell
                                            id={`${id}-impact`}
                                            colSpan={4}
                                            className={cn(
                                                CELL,
                                                "h-auto border-b border-l-2 border-grid/75 py-1.5 text-xs",
                                                border,
                                                !pulse &&
                                                    (delta.destructive
                                                        ? "bg-destructive-soft/65"
                                                        : "bg-warning-soft/65"),
                                            )}
                                        >
                                            <div className="flex flex-wrap items-baseline gap-x-4 gap-y-1">
                                                <span
                                                    className={cn(
                                                        "font-medium",
                                                        delta.destructive
                                                            ? "text-destructive"
                                                            : "text-warning-soft-foreground",
                                                    )}
                                                >
                                                    受影响
                                                </span>
                                                {delta.targetQuantity != null &&
                                                line.quantity != null ? (
                                                    <span className="num inline-flex flex-wrap items-baseline gap-1">
                                                        采购数量
                                                        <QuantityValue
                                                            value={
                                                                line.quantity
                                                            }
                                                            unit={line.unitName}
                                                        />
                                                        →
                                                        <QuantityValue
                                                            value={
                                                                delta.targetQuantity
                                                            }
                                                            unit={line.unitName}
                                                        />
                                                        {delta.quantityDelta !=
                                                        null ? (
                                                            <PurchaseSourceDelta
                                                                value={
                                                                    delta.quantityDelta
                                                                }
                                                                unit={
                                                                    line.unitName
                                                                }
                                                            />
                                                        ) : null}
                                                    </span>
                                                ) : null}
                                                {amountDelta != null ? (
                                                    <span>
                                                        含税差额{" "}
                                                        <PurchaseSourceDelta
                                                            value={amountDelta}
                                                            money
                                                        />
                                                    </span>
                                                ) : null}
                                            </div>
                                            {delta.reasons.length ? (
                                                <ul className="num mt-0.5 text-muted-foreground">
                                                    {[
                                                        ...new Set(
                                                            delta.reasons.map(
                                                                (reason) =>
                                                                    reason.description,
                                                            ),
                                                        ),
                                                    ].map((reason) => (
                                                        <li key={reason}>
                                                            {reason}
                                                        </li>
                                                    ))}
                                                </ul>
                                            ) : null}
                                        </TableCell>
                                    </TableRow>
                                ) : null}
                            </Fragment>
                        )
                    })}
                </TableBody>
                <TableFooter className="border-0 bg-surface-sunken">
                    <TableRow className="border-0 hover:bg-transparent">
                        <TableCell
                            colSpan={2}
                            className={cn(CELL, "rounded-l-lg font-semibold")}
                        >
                            整单合计{" "}
                            <span className="num ml-1 text-xs font-normal text-muted-foreground">
                                {filtered.length < lines.length
                                    ? `（当前显示 ${filtered.length} 行，合计按整单共 ${lines.length} 行）`
                                    : `${lines.length} 行`}
                            </span>
                        </TableCell>
                        <TableCell
                            colSpan={2}
                            className={cn(CELL, "rounded-r-lg text-right")}
                        >
                            含税 {amount(totals.gross)}
                            <div className="flex flex-wrap justify-end gap-x-2 text-xs text-muted-foreground">
                                <span>未税 {amount(totals.net)}</span>
                                <span>税额 {amount(totals.tax)}</span>
                            </div>
                        </TableCell>
                    </TableRow>
                </TableFooter>
            </Table>

            {pages > 1 ? (
                <nav
                    aria-label="采购明细分页"
                    className="flex flex-wrap items-center justify-end gap-2 text-xs"
                >
                    <span
                        className="num mr-1 text-muted-foreground"
                        aria-live="polite"
                    >
                        共 {filtered.length} 行 · 第 {page + 1} / {pages} 页
                    </span>
                    <Button
                        id={`${idPrefix}-lines-previous`}
                        size="icon-xs"
                        variant="outline"
                        aria-label="上一页"
                        disabled={page === 0}
                        onClick={() => setView({ ...active, page: page - 1 })}
                    >
                        <ChevronLeftIcon aria-hidden="true" />
                    </Button>
                    <Button
                        id={`${idPrefix}-lines-next`}
                        size="icon-xs"
                        variant="outline"
                        aria-label="下一页"
                        disabled={page + 1 >= pages}
                        onClick={() => setView({ ...active, page: page + 1 })}
                    >
                        <ChevronRightIcon aria-hidden="true" />
                    </Button>
                </nav>
            ) : null}

            {impact.unmatched.length ? (
                <div
                    ref={unlinkedSection}
                    id={`${idPrefix}-unlinked-impact`}
                    tabIndex={-1}
                    className={cn(
                        "space-y-1 border-l-2 border-l-warning px-3 py-2 text-xs",
                        pulsing
                            ? "bg-info-soft motion-safe:animate-[pulse_1.1s_ease-in-out_2]"
                            : "bg-warning-soft",
                    )}
                >
                    <h3 className="font-medium">
                        未关联到当前采购行的销售变更
                    </h3>
                    <ul className="num space-y-0.5 text-muted-foreground">
                        {impact.unmatched.map((reason) => (
                            <li
                                key={`${reason.lineNo}-${reason.itemName}-${reason.description}`}
                            >
                                销售第 {reason.lineNo} 行 · {reason.itemName} ·{" "}
                                {reason.description}
                            </li>
                        ))}
                    </ul>
                </div>
            ) : null}
        </section>
    )
}
