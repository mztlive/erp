import { ChevronDownIcon, ChevronUpIcon } from "lucide-react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { TableCell, TableRow } from "@/components/ui/table"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { displayName } from "@/lib/display-name"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { salesLinePriceComparison } from "@/lib/sales-line-prices"
import { cn } from "@/lib/utils"
import {
    purchaseSpecification,
    type PurchaseComparison,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"
import {
    hasPurchaseSupplement,
    PurchaseLineSupplement,
} from "./workspace-purchase-line-supplement"

export function PurchaseComparisonRow({
    line,
    comparison,
    negativeMargin,
    source,
    prefix,
    expanded,
    onToggle,
    onPreview,
}: {
    line: ApprovalPurchaseLine
    comparison: PurchaseComparison
    negativeMargin: boolean
    source?: PurchaseSource
    prefix: string
    expanded: boolean
    onToggle: () => void
    onPreview: (revisionId: string) => void
}) {
    const attention = comparison.warnings.length > 0 || negativeMargin
    const title = displayName(line.title) || "未记录商品名称"
    const spec = purchaseSpecification(line.specification)
    const canExpand =
        hasPurchaseSupplement(line, comparison, source) || negativeMargin
    const sales = line.source

    return (
        <>
            <TableRow
                className={cn(
                    negativeMargin &&
                        "bg-destructive-soft/35 hover:bg-destructive-soft/50",
                    expanded && !negativeMargin && "bg-muted/30",
                )}
            >
                <TableCell className="num text-muted-foreground">
                    {line.line_no}
                </TableCell>
                <TableCell className="min-w-36 max-w-64 whitespace-normal">
                    <div className="flex flex-wrap items-center gap-1.5">
                        <p className="font-medium wrap-anywhere">{title}</p>
                        {comparison.fee ? (
                            <Badge variant="neutral">费用</Badge>
                        ) : null}
                        {negativeMargin ? (
                            <Badge variant="destructive">负毛利</Badge>
                        ) : null}
                    </div>
                    {spec ? (
                        <p className="mt-1 text-xs text-muted-foreground wrap-anywhere">
                            {spec}
                        </p>
                    ) : null}
                    {!comparison.fee ? (
                        <div className="num mt-0.5 space-y-0.5 text-xs text-muted-foreground">
                            <p
                                className={cn(
                                    comparison.delayDays > 0 &&
                                        "font-medium text-warning-soft-foreground",
                                )}
                            >
                                预计交付 {line.expected_delivery_date ?? "—"}
                            </p>
                            {sales ? (
                                <p>承诺交期 {comparison.salesDate ?? "—"}</p>
                            ) : null}
                        </div>
                    ) : null}
                </TableCell>
                <TableCell data-align="end">
                    {line.quantity == null ? (
                        "—"
                    ) : (
                        <QuantityValue
                            value={line.quantity}
                            unit={line.unitName}
                        />
                    )}
                    {sales ? (
                        <p className="mt-1 text-xs text-muted-foreground">
                            销售{" "}
                            <QuantityValue
                                value={sales.quantity}
                                unit={sales.unitName}
                            />
                        </p>
                    ) : null}
                </TableCell>
                <TableCell className="whitespace-normal">
                    <div className="space-y-1">
                        <MoneyValue
                            display="exact"
                            value={line.unit_cost_gross}
                            className={cn(
                                "font-medium",
                                negativeMargin &&
                                    "text-destructive-soft-foreground",
                            )}
                        />
                        {sales ? <PurchaseSourcePrices source={sales} /> : null}
                    </div>
                </TableCell>
                <TableCell data-align="end">
                    <p className="font-medium">
                        <span className="mr-1 text-xs font-normal text-muted-foreground">
                            含税
                        </span>
                        <MoneyValue display="exact" value={line.gross_amount} />
                    </p>
                    {line.input_tax_rate != null ? (
                        <p className="mt-1 text-xs text-muted-foreground">
                            进项税率{" "}
                            <RateValue
                                value={multiplyFixed(
                                    line.input_tax_rate,
                                    "100",
                                    {
                                        leftMaxScale: 6,
                                        rightMaxScale: 0,
                                        outputScale: 2,
                                    },
                                )}
                                precision={2}
                            />
                        </p>
                    ) : null}
                </TableCell>
                <TableCell
                    data-align="end"
                    className="max-w-44 whitespace-normal"
                >
                    {comparison.warnings.length > 0 || comparison.partial ? (
                        <div className="mb-1 flex flex-wrap justify-end gap-1">
                            {comparison.warnings.map((warning) => (
                                <Badge
                                    key={warning}
                                    variant="warning"
                                    className="h-auto max-w-full whitespace-normal"
                                >
                                    {warning}
                                </Badge>
                            ))}
                            {comparison.partial ? (
                                <Badge variant="neutral">部分采购</Badge>
                            ) : null}
                        </div>
                    ) : null}
                    {canExpand ? (
                        <Button
                            id={`${prefix}-toggle`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            className="text-primary hover:text-primary"
                            aria-label={`${expanded ? "收起" : "展开"}第 ${line.line_no} 项 ${title}的补充信息`}
                            aria-expanded={expanded}
                            aria-controls={
                                expanded ? `${prefix}-detail` : undefined
                            }
                            onClick={onToggle}
                        >
                            {expanded ? "收起" : "展开"}
                            {expanded ? (
                                <ChevronUpIcon aria-hidden="true" />
                            ) : (
                                <ChevronDownIcon aria-hidden="true" />
                            )}
                        </Button>
                    ) : (
                        "—"
                    )}
                </TableCell>
            </TableRow>
            {expanded && canExpand ? (
                <TableRow
                    className={cn(
                        "hover:bg-transparent",
                        negativeMargin
                            ? "bg-destructive-soft/30"
                            : attention
                              ? "bg-warning-soft/40"
                              : "bg-muted/35",
                    )}
                >
                    <TableCell
                        colSpan={6}
                        className="whitespace-normal px-5 py-4"
                    >
                        <div
                            id={`${prefix}-detail`}
                            role="region"
                            aria-label={`第 ${line.line_no} 项补充信息`}
                        >
                            <PurchaseLineSupplement
                                line={line}
                                comparison={comparison}
                                negativeMargin={negativeMargin}
                                source={source}
                                prefix={prefix}
                                onPreview={onPreview}
                            />
                        </div>
                    </TableCell>
                </TableRow>
            ) : null}
        </>
    )
}

function PurchaseSourcePrices({
    source,
}: {
    source: NonNullable<ApprovalPurchaseLine["source"]>
}) {
    const prices = salesLinePriceComparison(source.sales_prices ?? source)
    return (
        <div className="space-y-0.5 border-t border-border/50 pt-1 text-xs text-muted-foreground">
            <p>
                售价{" "}
                <MoneyValue display="exact" value={source.unit_price_gross} />
            </p>
            <div className="flex flex-wrap items-center gap-x-1.5">
                <span>供给价</span>
                <MoneyValue
                    display="exact"
                    value={source.procurement_supply_price_gross}
                />
                {prices.change != null ? (
                    <Badge variant="warning">
                        {prices.adjustment === "increased" ? "调高" : "调低"}
                    </Badge>
                ) : null}
            </div>
        </div>
    )
}
