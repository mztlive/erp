import { useState } from "react"
import { MapPinIcon } from "lucide-react"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { TableCell, TableRow } from "@/components/ui/table"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { inputTaxRatePercent } from "@/features/purchase-orders/lib/input-tax-rate"
import { displayName } from "@/lib/display-name"
import { cn } from "@/lib/utils"
import { PurchaseProfitValue } from "./workspace-purchase-amount-details"
import {
    purchaseSpecification,
    type PurchaseComparison,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"
import { PurchaseLinePriceDetails } from "./workspace-purchase-line-price-details"
import {
    purchaseLineRiskNotes,
    PurchaseLineRiskStrip,
} from "./workspace-purchase-line-supplement"
import type { PurchaseSourceComparisonTarget } from "./workspace-purchase-source-comparison"

export function PurchaseComparisonRow({
    rowId,
    highlighted = false,
    line,
    comparison,
    externalLoss,
    source,
    deliveryPointCount,
    prefix,
    onPreview,
    onCompareSource,
    onInspectDelivery,
}: {
    rowId?: string
    highlighted?: boolean
    line: ApprovalPurchaseLine
    comparison: PurchaseComparison
    externalLoss: boolean
    source?: PurchaseSource
    deliveryPointCount?: number
    prefix: string
    onPreview: (revisionId: string) => void
    onCompareSource?: (target: PurchaseSourceComparisonTarget) => void
    onInspectDelivery?: () => void
}) {
    const [inlineDetailsOpen, setInlineDetailsOpen] = useState(false)
    const title = displayName(line.title) || "未记录商品名称"
    const spec = purchaseSpecification(line.specification)
    const sales = line.source
    const useSideComparison = Boolean(onCompareSource && sales)
    const showInlineDetails = !useSideComparison && inlineDetailsOpen
    const compareLabel = showInlineDetails ? "收起对照" : "对照本行"
    const canCompare =
        sales != null || line.input_tax_rate != null || line.tax_amount != null
    const lineNotes = purchaseLineRiskNotes(comparison, externalLoss)
    const showRiskStrip = Boolean(
        lineNotes.externalPriceRisk || lineNotes.warnings.length || source,
    )
    const inlineDetailsId = `${prefix}-prices`

    return (
        <>
            <TableRow
                id={rowId}
                className={cn(
                    comparison.highWarnings.length > 0 &&
                        "bg-destructive-soft/35 hover:bg-destructive-soft/50",
                    highlighted &&
                        "bg-info-soft/50 ring-1 ring-inset ring-info-border",
                    showRiskStrip && "border-b-0",
                )}
            >
                <TableCell className="num py-3 align-top text-muted-foreground">
                    {line.line_no}
                </TableCell>

                <TableCell className="min-w-40 py-3 align-top whitespace-normal">
                    <div className="flex flex-wrap items-center gap-1.5">
                        <span className="font-medium wrap-anywhere text-foreground">
                            {title}
                        </span>
                        {comparison.fee ? (
                            <Badge variant="neutral">费用</Badge>
                        ) : null}
                        {comparison.quantityExcess ? (
                            <Badge variant="destructive">超需求</Badge>
                        ) : null}
                        {comparison.mismatchedProduct ? (
                            <Badge variant="destructive">品规差异</Badge>
                        ) : null}
                        {comparison.unitMismatch ? (
                            <Badge variant="destructive">单位差异</Badge>
                        ) : null}
                    </div>
                    {spec || !comparison.fee ? (
                        <p className="num mt-1 flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs wrap-anywhere text-muted-foreground">
                            {spec ? <span>{spec}</span> : null}
                            {spec && !comparison.fee ? (
                                <span
                                    aria-hidden="true"
                                    className="text-muted-foreground/45"
                                >
                                    ·
                                </span>
                            ) : null}
                            {!comparison.fee ? (
                                <span
                                    className={cn(
                                        comparison.delayDays > 0 &&
                                            "font-medium text-warning-soft-foreground",
                                    )}
                                >
                                    预计 {line.expected_delivery_date ?? "未填"}
                                    {comparison.delayDays > 0
                                        ? `（晚 ${comparison.delayDays} 天）`
                                        : comparison.salesDate
                                          ? ` / 承诺 ${comparison.salesDate}`
                                          : ""}
                                </span>
                            ) : null}
                            {deliveryPointCount != null &&
                            deliveryPointCount > 0 ? (
                                <>
                                    <span
                                        aria-hidden="true"
                                        className="text-muted-foreground/45"
                                    >
                                        ·
                                    </span>
                                    {onInspectDelivery ? (
                                        <button
                                            id={`${prefix}-delivery`}
                                            type="button"
                                            onClick={onInspectDelivery}
                                            className="inline-flex cursor-pointer items-center gap-1 text-primary hover:underline"
                                        >
                                            <MapPinIcon
                                                aria-hidden="true"
                                                className="size-3 shrink-0"
                                            />
                                            <span>
                                                {deliveryPointCount} 处收货点
                                            </span>
                                        </button>
                                    ) : (
                                        <span>
                                            {deliveryPointCount} 处收货点
                                        </span>
                                    )}
                                </>
                            ) : null}
                        </p>
                    ) : null}
                </TableCell>

                <TableCell
                    data-align="end"
                    className="py-3 align-top whitespace-nowrap"
                >
                    {line.quantity == null ? (
                        "—"
                    ) : (
                        <QuantityValue
                            value={line.quantity}
                            unit={line.unitName}
                        />
                    )}
                    {comparison.quantityExcess ? (
                        <p className="mt-1 text-xs font-medium text-destructive-soft-foreground">
                            超需求 +
                            <QuantityValue
                                value={comparison.quantityExcess}
                                unit={line.unitName}
                            />
                        </p>
                    ) : comparison.partial && sales ? (
                        <p className="mt-1 text-xs text-muted-foreground">
                            销售{" "}
                            <QuantityValue
                                value={sales.quantity}
                                unit={sales.unitName}
                            />
                            （部分）
                        </p>
                    ) : null}
                </TableCell>

                <TableCell
                    data-align="end"
                    className="py-3 align-top whitespace-nowrap"
                >
                    <MoneyValue display="exact" value={line.gross_amount} />
                    {line.unit_cost_gross != null ? (
                        <div className="mt-0.5 text-xs text-muted-foreground">
                            {line.unitName ? null : "单价 "}
                            <MoneyValue
                                display="exact"
                                value={line.unit_cost_gross}
                                inheritColor
                            />
                            {line.unitName ? `/${line.unitName}` : null}
                        </div>
                    ) : null}
                </TableCell>

                <TableCell data-align="end" className="py-3 align-top">
                    {line.input_tax_rate == null ? (
                        "—"
                    ) : (
                        <RateValue
                            value={inputTaxRatePercent(line.input_tax_rate)}
                            precision={2}
                        />
                    )}
                </TableCell>

                <TableCell data-align="end" className="py-3 align-top">
                    <MoneyValue
                        display="exact"
                        value={sales?.procurement_supply_price_gross}
                    />
                </TableCell>

                <TableCell data-align="end" className="py-3 align-top">
                    <MoneyValue
                        display="exact"
                        value={sales?.unit_price_gross}
                    />
                </TableCell>

                <TableCell data-align="end" className="py-3 align-top">
                    <PurchaseProfitValue
                        amount={comparison.purchaseMargin?.margin ?? null}
                    />
                </TableCell>

                <TableCell data-align="end" className="py-3 align-top">
                    <PurchaseProfitValue
                        amount={comparison.externalGrossMargin}
                    />
                    {comparison.externalGrossMargin != null &&
                    comparison.externalMargin ? (
                        <div className="num mt-0.5 text-xs text-muted-foreground">
                            {comparison.externalMargin.percent}%
                        </div>
                    ) : null}
                </TableCell>

                <TableCell data-align="end" className="py-3 align-top">
                    {canCompare ? (
                        <Button
                            id={`${prefix}-source-compare`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className="h-control-xs px-2 text-xs font-normal text-primary shadow-none hover:bg-sidebar-accent/70 hover:text-primary"
                            aria-label={`${compareLabel}，第 ${line.line_no} 行价格`}
                            aria-expanded={
                                useSideComparison
                                    ? undefined
                                    : inlineDetailsOpen
                            }
                            aria-controls={
                                showInlineDetails ? inlineDetailsId : undefined
                            }
                            onClick={() => {
                                if (onCompareSource && sales) {
                                    onCompareSource({
                                        revisionId: sales.revision_id,
                                        purchaseLineId: line.id,
                                    })
                                } else {
                                    setInlineDetailsOpen((open) => !open)
                                }
                            }}
                        >
                            {compareLabel}
                        </Button>
                    ) : null}
                    {comparison.salesPriceWarnings.length > 0 ? (
                        <div className="mt-0.5 text-xs text-warning-soft-foreground">
                            价格需核对
                        </div>
                    ) : null}
                </TableCell>
            </TableRow>

            {showRiskStrip ? (
                <TableRow className="hover:bg-transparent">
                    <TableCell
                        colSpan={10}
                        className="pt-0 pb-3 whitespace-normal"
                    >
                        <PurchaseLineRiskStrip
                            line={line}
                            comparison={comparison}
                            notes={lineNotes}
                            source={source}
                            prefix={prefix}
                            onPreview={onPreview}
                        />
                    </TableCell>
                </TableRow>
            ) : null}
            {showInlineDetails ? (
                <TableRow className="hover:bg-transparent">
                    <TableCell
                        id={inlineDetailsId}
                        colSpan={10}
                        className="pt-0 pb-3 whitespace-normal"
                    >
                        <div className="rounded-md bg-surface-sunken p-3">
                            <PurchaseLinePriceDetails
                                line={line}
                                comparison={comparison}
                            />
                        </div>
                    </TableCell>
                </TableRow>
            ) : null}
        </>
    )
}
