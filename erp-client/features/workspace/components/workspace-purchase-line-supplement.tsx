import { FileTextIcon, TriangleAlertIcon } from "lucide-react"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { cn } from "@/lib/utils"
import {
    purchaseSpecification,
    type PurchaseComparison,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"

const warningDetails: Record<string, string> = {
    来源销售缺失: "未保留可核对的来源销售明细，请联系申请人补充依据",
    数值格式需核对: "价格或数量格式异常，请核对原始资料",
    采购单价缺失: "未记录采购单价，请核对计价依据",
    采购数量缺失: "未记录采购数量，请核对采购范围",
}

/**
 * 仅在存在可操作审批风险（负毛利、超预估、超量、延期、品规/单位差异）或多来源销售单链接时
 * 展示紧贴主行的核对副条；纯参考缺省（未填预估价、交期资料不完整）收进单元格弱化提示，避免满屏黄条。
 */
export function hasPurchaseRiskStrip(
    comparison: PurchaseComparison,
    negativeMargin: boolean,
    source?: PurchaseSource,
): boolean {
    return Boolean(
        negativeMargin || comparison.actionableWarnings.length > 0 || source,
    )
}

/**
 * 紧跟数据行的紧凑核对副条：
 * 审批人无需逐行点击「展开」，零点击即可直读负毛利、超预估差额、超量、延期及品规差异证据。
 */
export function PurchaseLineRiskStrip({
    line,
    comparison,
    negativeMargin,
    source,
    prefix,
    onPreview,
}: {
    line: ApprovalPurchaseLine
    comparison: PurchaseComparison
    negativeMargin: boolean
    source?: PurchaseSource
    prefix: string
    onPreview: (revisionId: string) => void
}) {
    const mismatchedProduct = comparison.mismatchedProduct
    const otherWarnings = comparison.actionableWarnings.filter(
        (warning) => warning !== "商品 / 规格不一致",
    )
    const isHighRisk = negativeMargin || comparison.highWarnings.length > 0
    const hasRisk = negativeMargin || comparison.actionableWarnings.length > 0

    return (
        <div
            id={`${prefix}-detail`}
            role="region"
            aria-label={`第 ${line.line_no} 项核对提示`}
            className={cn(
                "flex flex-wrap items-start justify-between gap-x-4 gap-y-2 border-l-2 py-2 pr-3 pl-3 text-xs",
                isHighRisk
                    ? "border-destructive bg-destructive-soft/25 text-destructive-soft-foreground"
                    : hasRisk
                      ? "border-warning bg-warning-soft/35 text-warning-soft-foreground"
                      : "border-border bg-muted/25 text-muted-foreground",
            )}
        >
            <div className="min-w-0 flex-1 space-y-1">
                {hasRisk ? (
                    <div className="flex items-start gap-1.5">
                        <TriangleAlertIcon
                            aria-hidden="true"
                            className="mt-0.5 size-3.5 shrink-0"
                        />
                        <ul className="flex flex-wrap items-baseline gap-x-3 gap-y-1 wrap-anywhere">
                            {negativeMargin ? (
                                <li className="font-medium">
                                    采购单价高于销售含税单价（负毛利）
                                </li>
                            ) : null}
                            {otherWarnings.map((warning) => (
                                <li key={warning}>
                                    {warning === "高于销售预估采购价" &&
                                    comparison.priceDifference ? (
                                        <>
                                            单价超销售预估{" "}
                                            <MoneyValue
                                                value={
                                                    comparison.priceDifference
                                                }
                                                display="exact"
                                                inheritColor
                                            />
                                            {comparison.pricePercent ? (
                                                <>
                                                    （+
                                                    <RateValue
                                                        value={
                                                            comparison.pricePercent
                                                        }
                                                        precision={1}
                                                    />
                                                    ）
                                                </>
                                            ) : null}
                                            {comparison.priceDifferenceTotal ? (
                                                <>
                                                    ，本行合计多{" "}
                                                    <MoneyValue
                                                        value={
                                                            comparison.priceDifferenceTotal
                                                        }
                                                        display="exact"
                                                        inheritColor
                                                    />
                                                </>
                                            ) : null}
                                        </>
                                    ) : warning === "本次数量超过销售需求" &&
                                      comparison.quantityExcess ? (
                                        <span className="font-medium">
                                            超来源销售需求{" "}
                                            <QuantityValue
                                                value={
                                                    comparison.quantityExcess
                                                }
                                                unit={line.unitName}
                                            />
                                            ，请核对超量依据
                                        </span>
                                    ) : warning === "单位需核对" ? (
                                        <span className="font-medium">
                                            单位不一致：销售「
                                            {line.source?.unitName || "未记录"}
                                            」vs 采购「
                                            {line.unitName || "未记录"}
                                            」（暂不比对单价与数量）
                                        </span>
                                    ) : warning.startsWith("交期晚 ") ? (
                                        <>
                                            预计交期晚于客户承诺{" "}
                                            {comparison.delayDays} 天（承诺{" "}
                                            <span className="num">
                                                {comparison.salesDate}
                                            </span>
                                            ）
                                        </>
                                    ) : (
                                        (warningDetails[warning] ?? warning)
                                    )}
                                </li>
                            ))}
                        </ul>
                    </div>
                ) : null}

                {mismatchedProduct && line.source ? (
                    <p className="pl-5 wrap-anywhere text-foreground/85">
                        <span className="font-medium text-destructive-soft-foreground">
                            品规差异：
                        </span>
                        销售要求「
                        {displayName(line.source.title) || "未记录商品名称"}
                        {purchaseSpecification(line.source.specification)
                            ? ` · ${purchaseSpecification(line.source.specification)}`
                            : ""}
                        」→ 本次采购「
                        {displayName(line.title) || "未记录商品名称"}
                        {purchaseSpecification(line.specification)
                            ? ` · ${purchaseSpecification(line.specification)}`
                            : ""}
                        」
                    </p>
                ) : null}
            </div>

            {source ? (
                <div className="flex shrink-0 items-center gap-2 text-muted-foreground">
                    <span className="num">
                        来源{" "}
                        {displayBusinessText(
                            source.document_no,
                            source.document_id,
                        ) || "未记录销售单号"}{" "}
                        · V{source.revision_no}
                    </span>
                    <Button
                        id={`${prefix}-source`}
                        type="button"
                        variant="link"
                        size="xs"
                        className="h-auto px-0 text-xs text-primary"
                        onClick={() => onPreview(source.revision_id)}
                    >
                        <FileTextIcon aria-hidden="true" />
                        预览来源销售单
                    </Button>
                </div>
            ) : null}
        </div>
    )
}
