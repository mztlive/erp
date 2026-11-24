import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { TableCell, TableRow } from "@/components/ui/table"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { displayName } from "@/lib/display-name"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { PurchaseAmountDetails } from "./workspace-purchase-amount-details"
import {
    purchaseSpecification,
    type PurchaseComparison,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"
import {
    hasPurchaseRiskStrip,
    PurchaseLineRiskStrip,
} from "./workspace-purchase-line-supplement"
import type { PurchaseSourceComparisonTarget } from "./workspace-purchase-source-comparison"

export function PurchaseComparisonRow({
    rowId,
    highlighted = false,
    line,
    comparison,
    negativeMargin,
    source,
    deliveryPointCount,
    prefix,
    onPreview,
    onCompareSource,
}: {
    rowId?: string
    highlighted?: boolean
    line: ApprovalPurchaseLine
    comparison: PurchaseComparison
    negativeMargin: boolean
    source?: PurchaseSource
    deliveryPointCount?: number
    prefix: string
    onPreview: (revisionId: string) => void
    onCompareSource?: (target: PurchaseSourceComparisonTarget) => void
}) {
    const title = displayName(line.title) || "未记录商品名称"
    const spec = purchaseSpecification(line.specification)
    const sales = line.source
    const estimatedUnitPrice = sales?.procurement_supply_price_gross
    const overSupply = Boolean(comparison.priceDifference)
    const isHighRisk = negativeMargin || comparison.highWarnings.length > 0
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
                id={rowId}
                className={cn(
                    isHighRisk &&
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
                        {negativeMargin ? (
                            <Badge variant="destructive">负毛利</Badge>
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
                                    <span>{deliveryPointCount} 处收货点</span>
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
                    {line.unit_cost_gross != null ? (
                        <MoneyValue
                            display="exact"
                            value={line.unit_cost_gross}
                            inheritColor={negativeMargin || overSupply}
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
                    {sales ? (
                        <div className="mt-0.5 flex items-center justify-end gap-x-1.5 text-xs text-muted-foreground">
                            {estimatedUnitPrice ? (
                                <span>
                                    预估{" "}
                                    <MoneyValue
                                        display="exact"
                                        value={estimatedUnitPrice}
                                    />
                                </span>
                            ) : (
                                <span>销售未填预估价</span>
                            )}
                            {comparison.salesSupplyAdjustment === "increased" ||
                            comparison.salesSupplyAdjustment === "decreased" ? (
                                <Badge variant="warning" className="text-tiny">
                                    {comparison.salesSupplyAdjustment ===
                                    "increased"
                                        ? "调高"
                                        : "调低"}
                                </Badge>
                            ) : null}
                        </div>
                    ) : null}
                    {inputTaxRatePercent != null ? (
                        <p className="mt-0.5 text-xs text-muted-foreground">
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
                    className="space-y-0.5 py-3 align-top whitespace-nowrap"
                >
                    <div className="text-xs text-muted-foreground">
                        实际采购{" "}
                        <MoneyValue
                            display="exact"
                            value={line.gross_amount}
                            className="text-body-compact"
                        />
                    </div>
                    {!comparison.fee ? (
                        <PurchaseAmountDetails
                            estimated={comparison.estimatedGross}
                            difference={comparison.grossDifference}
                        />
                    ) : null}
                    {line.tax_amount != null ? (
                        <p className="text-xs text-muted-foreground">
                            税额{" "}
                            <MoneyValue
                                display="exact"
                                value={line.tax_amount}
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
                                    销售单价
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
                                            ? `亏 ${comparison.marginRate.absPercent}%`
                                            : `毛利 ${comparison.marginRate.percent}%`}
                                    </span>
                                ) : null}
                            </div>
                            {onCompareSource ? (
                                <Button
                                    id={`${prefix}-source-compare`}
                                    type="button"
                                    variant="link"
                                    size="xs"
                                    className="h-auto px-0"
                                    aria-label={`对照第 ${line.line_no} 行采购与销售`}
                                    onClick={() =>
                                        onCompareSource({
                                            revisionId: sales.revision_id,
                                            purchaseLineId: line.id,
                                        })
                                    }
                                >
                                    对照本行
                                </Button>
                            ) : null}
                        </div>
                    ) : (
                        <span className="text-xs text-muted-foreground">—</span>
                    )}
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
