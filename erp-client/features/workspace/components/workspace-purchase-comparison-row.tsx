import { ChevronDownIcon, ChevronUpIcon, FileTextIcon } from "lucide-react"
import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { TableCell, TableRow } from "@/components/ui/table"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { cn } from "@/lib/utils"
import {
    purchaseAttentionClass,
    purchaseSpecification,
    type PurchaseComparison,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"

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
    if (comparison.fee)
        return (
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
                <TableCell colSpan={3} className="text-right">
                    含税金额{" "}
                    <MoneyValue value={line.gross_amount} display="exact" />
                </TableCell>
            </TableRow>
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
                <TableCell>
                    <Button
                        id={`${prefix}-toggle`}
                        variant="ghost"
                        size="icon-sm"
                        aria-label={`${expanded ? "收起" : "展开"}第 ${line.line_no} 项 ${title}的对照`}
                        aria-expanded={expanded}
                        aria-controls={`${prefix}-detail`}
                        onClick={onToggle}
                    >
                        {expanded ? (
                            <ChevronUpIcon aria-hidden="true" />
                        ) : (
                            <ChevronDownIcon aria-hidden="true" />
                        )}
                    </Button>
                </TableCell>
            </TableRow>
            {expanded && (
                <TableRow
                    className={cn(
                        "hover:bg-transparent",
                        attention ? "bg-warning-soft/50" : "bg-info-soft/30",
                    )}
                >
                    <TableCell
                        colSpan={9}
                        className="whitespace-normal px-5 py-4"
                    >
                        <div
                            id={`${prefix}-detail`}
                            role="region"
                            aria-label={`第 ${line.line_no} 项完整对照`}
                            className="space-y-3"
                        >
                            <p className="font-medium">
                                第 {line.line_no} 项 · {title}
                            </p>
                            {line.source ? (
                                <div className="grid gap-4 lg:grid-cols-3">
                                    <div className="space-y-2">
                                        <h4 className="text-xs font-medium">
                                            价格核对
                                        </h4>
                                        <p className="text-xs text-muted-foreground">
                                            供给价{" "}
                                            <MoneyValue
                                                value={
                                                    line.source
                                                        .procurement_supply_price_gross
                                                }
                                                display="exact"
                                            />{" "}
                                            → 本次采购{" "}
                                            <MoneyValue
                                                value={line.unit_cost_gross}
                                                display="exact"
                                            />
                                        </p>
                                        {comparison.priceDifference && (
                                            <p className="text-xs text-warning-soft-foreground">
                                                每单位高{" "}
                                                <MoneyValue
                                                    value={
                                                        comparison.priceDifference
                                                    }
                                                    display="exact"
                                                />
                                                {comparison.pricePercent && (
                                                    <>
                                                        （
                                                        <RateValue
                                                            value={
                                                                comparison.pricePercent
                                                            }
                                                            precision={1}
                                                        />
                                                        ）
                                                    </>
                                                )}
                                            </p>
                                        )}
                                        <p className="text-xs text-muted-foreground">
                                            销售商品：
                                            {displayName(line.source.title) ||
                                                "未记录商品名称"}
                                            {purchaseSpecification(
                                                line.source.specification,
                                            ) &&
                                                ` · ${purchaseSpecification(line.source.specification)}`}
                                        </p>
                                        <p className="text-xs text-muted-foreground">
                                            采购商品：{title}
                                            {purchaseSpecification(
                                                line.specification,
                                            ) &&
                                                ` · ${purchaseSpecification(line.specification)}`}
                                        </p>
                                    </div>
                                    <div className="space-y-2">
                                        <h4 className="text-xs font-medium">
                                            交期与数量
                                        </h4>
                                        <p className="text-xs text-muted-foreground">
                                            销售{" "}
                                            {comparison.salesDate ?? "未记录"} →
                                            采购{" "}
                                            {line.expected_delivery_date ??
                                                "未记录"}
                                        </p>
                                        {comparison.delayDays > 0 && (
                                            <p className="text-xs text-warning-soft-foreground">
                                                晚于客户承诺交期{" "}
                                                {comparison.delayDays} 天
                                            </p>
                                        )}
                                        <p className="text-xs text-muted-foreground">
                                            销售需求{" "}
                                            <QuantityValue
                                                value={line.source.quantity}
                                                unit={line.source.unit}
                                            />{" "}
                                            / 本次采购{" "}
                                            {line.quantity != null ? (
                                                <QuantityValue
                                                    value={line.quantity}
                                                    unit={line.unit ?? ""}
                                                />
                                            ) : (
                                                "未记录"
                                            )}
                                        </p>
                                        {comparison.partial && (
                                            <p className="text-xs text-muted-foreground">
                                                本次仅覆盖部分销售需求；其他采购覆盖量不在此次对照中。
                                            </p>
                                        )}
                                    </div>
                                    <div className="space-y-2">
                                        <h4 className="text-xs font-medium">
                                            来源依据
                                        </h4>
                                        <p className="text-xs text-muted-foreground">
                                            {source
                                                ? `${displayBusinessText(source.document_no, source.document_id) || "未记录销售单号"} · 销售版本 V${source.revision_no}`
                                                : "未记录来源销售单号"}
                                        </p>
                                        <Button
                                            id={`${prefix}-source`}
                                            variant="link"
                                            size="sm"
                                            className="h-auto px-0 text-info"
                                            onClick={() =>
                                                onPreview(
                                                    line.source!.revision_id,
                                                )
                                            }
                                        >
                                            <FileTextIcon aria-hidden="true" />
                                            预览来源销售单
                                        </Button>
                                    </div>
                                </div>
                            ) : (
                                <p className="text-sm text-warning-soft-foreground">
                                    此次提交未保留来源销售资料，请联系申请人核对后再审批。
                                </p>
                            )}
                        </div>
                    </TableCell>
                </TableRow>
            )}
        </>
    )
}
