import type { MouseEvent } from "react"
import { ChevronDownIcon, ChevronUpIcon } from "lucide-react"
import { MoneyValue, QuantityValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { TableCell, TableRow } from "@/components/ui/table"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { displayName } from "@/lib/display-name"
import { salesLinePriceComparison } from "@/lib/sales-line-prices"
import { cn } from "@/lib/utils"
import {
    purchaseAttentionClass,
    purchaseSpecification,
    type PurchaseComparison,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"

import {
    hasPurchaseSupplement,
    PurchaseLineSupplement,
} from "./workspace-purchase-line-supplement"

function isInteractiveTarget(target: EventTarget | null) {
    return (
        target instanceof Element &&
        Boolean(target.closest("button, a, input, select, textarea"))
    )
}

export function PurchaseComparisonRow({
    line,
    comparison,
    negativeMargin = false,
    source,
    prefix,
    expanded,
    onToggle,
    onPreview,
}: {
    line: ApprovalPurchaseLine
    comparison: PurchaseComparison
    negativeMargin?: boolean
    source?: PurchaseSource
    prefix: string
    expanded: boolean
    onToggle: () => void
    onPreview: (revisionId: string) => void
}) {
    const attention = comparison.warnings.length > 0 || negativeMargin
    const title = displayName(line.title) || "未记录商品名称"
    const canExpand =
        hasPurchaseSupplement(line, comparison, source) || negativeMargin
    const handleRowClick = (event: MouseEvent<HTMLTableRowElement>) => {
        if (!canExpand || isInteractiveTarget(event.target)) return
        onToggle()
    }
    const sourcePrices = line.source
        ? salesLinePriceComparison(line.source.sales_prices ?? line.source)
        : null
    const toggle = canExpand ? (
        <Button
            id={`${prefix}-toggle`}
            type="button"
            variant="ghost"
            size="icon-sm"
            aria-label={`${expanded ? "收起" : "展开"}第 ${line.line_no} 项 ${title}的补充信息`}
            aria-expanded={expanded}
            aria-controls={expanded ? `${prefix}-detail` : undefined}
            onClick={onToggle}
        >
            {expanded ? (
                <ChevronUpIcon aria-hidden="true" />
            ) : (
                <ChevronDownIcon aria-hidden="true" />
            )}
        </Button>
    ) : null
    const supplement =
        expanded && canExpand ? (
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
                <TableCell colSpan={8} className="whitespace-normal px-5 py-4">
                    <div
                        id={`${prefix}-detail`}
                        role="region"
                        aria-label={`第 ${line.line_no} 项补充信息`}
                    >
                        <PurchaseLineSupplement
                            line={line}
                            comparison={comparison}
                            source={source}
                            prefix={prefix}
                            onPreview={onPreview}
                        />
                    </div>
                </TableCell>
            </TableRow>
        ) : null
    if (comparison.fee)
        return (
            <>
                <TableRow
                    onClick={handleRowClick}
                    className={cn(canExpand && "cursor-pointer")}
                >
                    <TableCell className="num text-muted-foreground">
                        {line.line_no}
                    </TableCell>
                    <TableCell className="max-w-56 whitespace-normal wrap-anywhere">
                        <span className="mr-2 text-xs text-muted-foreground">
                            费用
                        </span>
                        {title}
                    </TableCell>
                    <TableCell colSpan={3} className="text-muted-foreground">
                        不对应销售商品明细 · 按行含税金额核对
                    </TableCell>
                    <TableCell colSpan={2} data-align="end">
                        含税金额{" "}
                        <MoneyValue value={line.gross_amount} display="exact" />
                    </TableCell>
                    <TableCell>{toggle}</TableCell>
                </TableRow>
                {supplement}
            </>
        )
    return (
        <>
            <TableRow
                onClick={handleRowClick}
                className={cn(
                    canExpand && "cursor-pointer",
                    negativeMargin
                        ? "bg-destructive-soft/35 hover:bg-destructive-soft/55"
                        : attention &&
                              "bg-warning-soft/50 hover:bg-warning-soft/80",
                    expanded && "has-aria-expanded:bg-muted/45",
                    expanded &&
                        attention &&
                        !negativeMargin &&
                        "has-aria-expanded:bg-warning-soft/80",
                )}
            >
                <TableCell className="num text-muted-foreground">
                    {line.line_no}
                </TableCell>
                <TableCell className="min-w-36 max-w-56 whitespace-normal wrap-anywhere">
                    <p className="font-medium">{title}</p>
                    {purchaseSpecification(line.specification) && (
                        <p className="mt-1 text-xs text-muted-foreground">
                            {purchaseSpecification(line.specification)}
                        </p>
                    )}
                </TableCell>
                <TableCell data-align="end">
                    <div className="flex flex-wrap items-baseline justify-end gap-1">
                        {line.source ? (
                            <QuantityValue
                                value={line.source.quantity}
                                unit={
                                    line.unit === line.source.unit
                                        ? ""
                                        : line.source.unitName
                                }
                            />
                        ) : (
                            "—"
                        )}
                        <span className="text-muted-foreground">/</span>
                        {line.quantity != null ? (
                            <QuantityValue
                                value={line.quantity}
                                unit={line.unitName}
                            />
                        ) : (
                            "—"
                        )}
                    </div>
                </TableCell>
                <TableCell className="whitespace-normal">
                    {line.source ? (
                        <div className="space-y-1 text-xs">
                            <div className="flex flex-wrap items-baseline gap-x-1.5">
                                <span className="text-muted-foreground">
                                    销
                                </span>
                                <MoneyValue
                                    value={line.source.unit_price_gross}
                                    display="exact"
                                    className="text-body-compact"
                                />
                            </div>
                            <div className="flex flex-wrap items-center gap-x-1.5 text-muted-foreground">
                                <span>供</span>
                                {line.source.procurement_supply_price_gross !=
                                    null &&
                                line.source.procurement_supply_price_gross !==
                                    "" ? (
                                    <MoneyValue
                                        value={
                                            line.source
                                                .procurement_supply_price_gross
                                        }
                                        display="exact"
                                    />
                                ) : (
                                    <span>未填</span>
                                )}
                                {sourcePrices?.change != null ? (
                                    <Badge
                                        variant="warning"
                                        className="px-1.5 py-0 text-tiny"
                                    >
                                        {sourcePrices.adjustment === "increased"
                                            ? "调高"
                                            : "调低"}
                                    </Badge>
                                ) : null}
                            </div>
                        </div>
                    ) : (
                        <span className="text-muted-foreground">—</span>
                    )}
                </TableCell>
                <TableCell data-align="end" className="font-semibold">
                    <MoneyValue
                        value={line.unit_cost_gross}
                        display="exact"
                        className={
                            negativeMargin
                                ? "text-destructive-soft-foreground"
                                : undefined
                        }
                    />
                </TableCell>
                <TableCell className="num text-xs">
                    <p>
                        <span className="mr-1.5 text-muted-foreground">销</span>
                        {comparison.salesDate ?? "—"}
                    </p>
                    <p
                        className={cn(
                            "mt-1",
                            comparison.delayDays > 0 &&
                                "font-medium text-warning-soft-foreground",
                        )}
                    >
                        <span className="mr-1.5 text-muted-foreground">采</span>
                        {line.expected_delivery_date ?? "—"}
                    </p>
                </TableCell>
                <TableCell className="max-w-48 whitespace-normal">
                    <div className="flex flex-wrap gap-1">
                        {negativeMargin ? (
                            <Badge variant="destructive">高于售价</Badge>
                        ) : null}
                        {comparison.warnings.map((warning) => (
                            <span
                                key={warning}
                                className={cn(
                                    "rounded-md border px-1.5 py-0.5 text-xs",
                                    purchaseAttentionClass,
                                )}
                            >
                                {warning}
                            </span>
                        ))}
                        {comparison.partial && (
                            <span className="rounded-md bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
                                部分采购
                            </span>
                        )}
                        {!attention && !comparison.partial && (
                            <span className="text-muted-foreground">—</span>
                        )}
                    </div>
                </TableCell>
                <TableCell>{toggle}</TableCell>
            </TableRow>
            {supplement}
        </>
    )
}
