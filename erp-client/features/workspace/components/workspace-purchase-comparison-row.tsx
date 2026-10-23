import { ChevronDownIcon, ChevronUpIcon } from "lucide-react"
import { MoneyValue, QuantityValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { TableCell, TableRow } from "@/components/ui/table"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { displayName } from "@/lib/display-name"
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

export function PurchaseComparisonRow({
    line,
    comparison,
    source,
    prefix,
    expanded,
    onToggle,
    onPreview,
}: {
    line: ApprovalPurchaseLine
    comparison: PurchaseComparison
    source?: PurchaseSource
    prefix: string
    expanded: boolean
    onToggle: () => void
    onPreview: (revisionId: string) => void
}) {
    const attention = comparison.warnings.length > 0
    const title = displayName(line.title) || "未记录商品名称"
    const canExpand = hasPurchaseSupplement(line, comparison, source)
    const toggle = canExpand ? (
        <Button
            id={`${prefix}-toggle`}
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
                    attention ? "bg-warning-soft/50" : "bg-info-soft/30",
                )}
            >
                <TableCell colSpan={9} className="whitespace-normal px-5 py-4">
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
                <TableRow>
                    <TableCell className="text-muted-foreground">
                        {line.line_no}
                    </TableCell>
                    <TableCell className="max-w-64 whitespace-normal wrap-anywhere">
                        <span className="mr-2 text-xs text-muted-foreground">
                            费用
                        </span>
                        {title}
                    </TableCell>
                    <TableCell colSpan={4} className="text-muted-foreground">
                        不对应销售商品明细 · 按行含税金额核对
                    </TableCell>
                    <TableCell colSpan={2} className="text-right">
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
                className={cn(
                    attention && "bg-warning-soft/60 hover:bg-warning-soft",
                    expanded && "has-aria-expanded:bg-info-soft/40",
                    expanded &&
                        attention &&
                        "has-aria-expanded:bg-warning-soft",
                )}
            >
                <TableCell className="text-muted-foreground">
                    {line.line_no}
                </TableCell>
                <TableCell className="max-w-64 whitespace-normal wrap-anywhere">
                    <p className="font-medium">{title}</p>
                    {purchaseSpecification(line.specification) && (
                        <p className="mt-1 text-xs text-muted-foreground">
                            {purchaseSpecification(line.specification)}
                        </p>
                    )}
                </TableCell>
                <TableCell>
                    <div className="flex flex-wrap items-baseline gap-1">
                        {line.source ? (
                            <QuantityValue
                                value={line.source.quantity}
                                unit={
                                    line.unit === line.source.unit
                                        ? ""
                                        : line.source.unit
                                }
                            />
                        ) : (
                            "—"
                        )}
                        <span className="text-muted-foreground">/</span>
                        {line.quantity != null ? (
                            <QuantityValue
                                value={line.quantity}
                                unit={line.unit ?? ""}
                            />
                        ) : (
                            "—"
                        )}
                    </div>
                </TableCell>
                <TableCell>
                    <MoneyValue
                        value={line.source?.unit_price_gross}
                        display="exact"
                    />
                </TableCell>
                <TableCell>
                    {line.source ? (
                        line.source.procurement_supply_price_gross != null &&
                        line.source.procurement_supply_price_gross !== "" ? (
                            <MoneyValue
                                value={
                                    line.source.procurement_supply_price_gross
                                }
                                display="exact"
                            />
                        ) : (
                            <span className="text-xs text-muted-foreground">
                                未填写
                            </span>
                        )
                    ) : (
                        "—"
                    )}
                </TableCell>
                <TableCell className="font-semibold">
                    <MoneyValue value={line.unit_cost_gross} display="exact" />
                </TableCell>
                <TableCell className="text-xs">
                    <p>
                        <span className="mr-2 text-muted-foreground">销</span>
                        {comparison.salesDate ?? "—"}
                    </p>
                    <p
                        className={cn(
                            "mt-1",
                            comparison.delayDays > 0 &&
                                "text-warning-soft-foreground",
                        )}
                    >
                        <span className="mr-2 text-muted-foreground">采</span>
                        {line.expected_delivery_date ?? "—"}
                    </p>
                </TableCell>
                <TableCell className="max-w-52 whitespace-normal">
                    <div className="flex flex-wrap gap-1.5">
                        {comparison.warnings.map((warning) => (
                            <span
                                key={warning}
                                className={cn(
                                    "rounded-md border px-2 py-1 text-xs",
                                    purchaseAttentionClass,
                                )}
                            >
                                {warning}
                            </span>
                        ))}
                        {comparison.partial && (
                            <span className="rounded-md bg-muted px-2 py-1 text-xs text-muted-foreground">
                                部分数量采购
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
