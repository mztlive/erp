import { MapPinIcon } from "lucide-react"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { TableCell, TableRow } from "@/components/ui/table"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { displayName } from "@/lib/display-name"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import {
    purchaseSpecification,
    type PurchaseComparison,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"
import {
    hasPurchaseRiskStrip,
    PurchaseLineRiskStrip,
} from "./workspace-purchase-line-supplement"

export function PurchaseComparisonRow({
    line,
    comparison,
    negativeMargin,
    source,
    deliveryHint,
    prefix,
    onPreview,
    onFocusDelivery,
}: {
    line: ApprovalPurchaseLine
    comparison: PurchaseComparison
    negativeMargin: boolean
    source?: PurchaseSource
    deliveryHint: string | null
    prefix: string
    onPreview: (revisionId: string) => void
    onFocusDelivery: () => void
}) {
    const title = displayName(line.title) || "未记录商品名称"
    const spec = purchaseSpecification(line.specification)
    const sales = line.source
    const overSupply = Boolean(comparison.priceDifference)
    const showRiskStrip = hasPurchaseRiskStrip(
        comparison,
        negativeMargin,
        source,
    )

    const inputTaxRatePercent =
        line.input_tax_rate != null
            ? multiplyFixed(line.input_tax_rate, "100", {
                  leftMaxScale: 6,
                  rightMaxScale: 0,
                  outputScale: 2,
              })
            : null

    return (
        <>
            <TableRow
                className={cn(
                    negativeMargin &&
                        "bg-destructive-soft/35 hover:bg-destructive-soft/50",
                    showRiskStrip && "border-b-0",
                )}
            >
                <TableCell className="num py-3 align-top text-muted-foreground">
                    {line.line_no}
                </TableCell>

                <TableCell className="min-w-44 py-3 align-top whitespace-normal">
                    <div className="flex flex-wrap items-center gap-1.5">
                        <span className="font-medium wrap-anywhere text-foreground">
                            {title}
                        </span>
                        {comparison.fee ? (
                            <Badge variant="neutral">费用</Badge>
                        ) : null}
                        {negativeMargin ? (
                            <Badge variant="destructive">负毛利</Badge>
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
                                    预计 {line.expected_delivery_date ?? "—"}
                                    {comparison.delayDays > 0
                                        ? `（晚 ${comparison.delayDays} 天）`
                                        : ""}
                                </span>
                            ) : null}
                        </p>
                    ) : null}
                    {deliveryHint ? (
                        <button
                            id={`${prefix}-delivery-jump`}
                            type="button"
                            onClick={onFocusDelivery}
                            className="mt-1 inline-flex max-w-full cursor-pointer items-center gap-1 text-left text-xs text-primary/90 hover:text-primary hover:underline"
                        >
                            <MapPinIcon
                                aria-hidden="true"
                                className="size-3 shrink-0"
                            />
                            <span className="truncate">{deliveryHint}</span>
                        </button>
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
                        <p className="mt-1 text-xs font-medium text-warning-soft-foreground">
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
                    {line.unit_cost_gross != null ? (
                        <MoneyValue
                            display="exact"
                            value={line.unit_cost_gross}
                            className={cn(
                                "font-medium",
                                negativeMargin
                                    ? "text-destructive-soft-foreground"
                                    : overSupply
                                      ? "text-warning-soft-foreground"
                                      : undefined,
                            )}
                        />
                    ) : (
                        <span className="text-muted-foreground">—</span>
                    )}
                    {inputTaxRatePercent != null ? (
                        <p className="mt-1 text-xs text-muted-foreground">
                            进项{" "}
                            <RateValue
                                value={inputTaxRatePercent}
                                precision={2}
                            />
                        </p>
                    ) : null}
                </TableCell>

                <TableCell
                    data-align="end"
                    className="py-3 align-top whitespace-normal"
                >
                    {sales ? (
                        <div className="space-y-1 text-xs">
                            <div className="flex flex-wrap items-baseline justify-end gap-x-1.5">
                                <span className="text-muted-foreground">
                                    售
                                </span>
                                <MoneyValue
                                    display="exact"
                                    value={sales.unit_price_gross}
                                    className="font-medium text-foreground/90"
                                />
                                {comparison.marginRate ? (
                                    <span
                                        className={cn(
                                            "num",
                                            comparison.marginRate.negative
                                                ? "font-medium text-destructive-soft-foreground"
                                                : "text-muted-foreground",
                                        )}
                                    >
                                        {comparison.marginRate.negative
                                            ? `亏 ${comparison.marginRate.percent}%`
                                            : `毛利 ${comparison.marginRate.percent}%`}
                                    </span>
                                ) : null}
                            </div>
                            <div className="flex flex-wrap items-center justify-end gap-x-1.5 text-muted-foreground">
                                <span>供</span>
                                {sales.procurement_supply_price_gross ? (
                                    <MoneyValue
                                        display="exact"
                                        value={
                                            sales.procurement_supply_price_gross
                                        }
                                        className="font-normal [&_[data-slot=money-value-number]]:text-inherit"
                                    />
                                ) : (
                                    <span>未填写</span>
                                )}
                                {comparison.salesSupplyAdjustment ===
                                    "increased" ||
                                comparison.salesSupplyAdjustment ===
                                    "decreased" ? (
                                    <Badge
                                        variant="warning"
                                        className="text-tiny"
                                    >
                                        {comparison.salesSupplyAdjustment ===
                                        "increased"
                                            ? "调高"
                                            : "调低"}
                                    </Badge>
                                ) : null}
                            </div>
                        </div>
                    ) : (
                        <span className="text-xs text-muted-foreground">—</span>
                    )}
                </TableCell>

                <TableCell
                    data-align="end"
                    className="py-3 align-top whitespace-nowrap"
                >
                    <div className="font-medium text-foreground">
                        <MoneyValue display="exact" value={line.gross_amount} />
                    </div>
                    {comparison.priceDifferenceTotal ? (
                        <p className="mt-1 text-xs font-medium text-warning-soft-foreground">
                            超预估 +
                            <MoneyValue
                                display="exact"
                                value={comparison.priceDifferenceTotal}
                                className="[&_[data-slot=money-value-number]]:text-inherit"
                            />
                        </p>
                    ) : line.tax_amount != null ? (
                        <p className="mt-1 text-xs text-muted-foreground">
                            税额{" "}
                            <MoneyValue
                                display="exact"
                                value={line.tax_amount}
                            />
                        </p>
                    ) : null}
                </TableCell>
            </TableRow>

            {showRiskStrip ? (
                <TableRow className="hover:bg-transparent">
                    <TableCell
                        colSpan={6}
                        className="pt-0 pb-3 whitespace-normal"
                    >
                        <PurchaseLineRiskStrip
                            line={line}
                            comparison={comparison}
                            negativeMargin={negativeMargin}
                            source={source}
                            prefix={prefix}
                            onPreview={onPreview}
                        />
                    </TableCell>
                </TableRow>
            ) : null}
        </>
    )
}
