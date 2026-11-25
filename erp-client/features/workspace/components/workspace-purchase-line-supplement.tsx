import { FileTextIcon, TriangleAlertIcon } from "lucide-react"

import { MoneyValue, QuantityValue, RateValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ApprovalPurchaseLine } from "@/features/approval-workflow/api/materials"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { cn } from "@/lib/utils"
import {
    PURCHASE_SUPPLY_PRICE_RISK,
    purchaseSpecification,
    type PurchaseComparison,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"
import type { PurchaseLineSourceInsight } from "./workspace-purchase-source-comparison-model"

const warningDetails: Record<string, string> = {
    来源销售缺失: "未保留可核对的来源销售明细，请联系申请人补充依据",
    数值格式需核对: "价格或数量格式异常，请核对原始资料",
    采购单价缺失: "未记录采购单价，请核对计价依据",
    采购数量缺失: "未记录采购数量，请核对采购范围",
}

/**
 * 行内提示保留需要复核的差异，盈亏金额由主表展示，价格依据在行对照中查看。
 */
export function purchaseLineRiskNotes(
    comparison: PurchaseComparison,
    externalLoss: boolean,
) {
    const externalPriceRisk =
        externalLoss && !comparison.externalGrossMargin?.startsWith("-")
    const purchaseLossShown = comparison.purchaseMargin?.margin.startsWith("-")
    const warnings = comparison.actionableWarnings.filter(
        (warning) =>
            !comparison.salesPriceWarnings.includes(warning) &&
            !(purchaseLossShown && warning === PURCHASE_SUPPLY_PRICE_RISK),
    )
    return { externalPriceRisk, warnings }
}

/**
 * 紧跟数据行的紧凑核对副条：
 * 审批人可以直接核对对外负毛利、采购倒挂、超量、延期及品规差异。
 */
export function PurchaseLineRiskStrip({
    line,
    comparison,
    notes,
    sourceInsight,
    source,
    prefix,
    onPreview,
}: {
    line: ApprovalPurchaseLine
    comparison: PurchaseComparison
    notes: ReturnType<typeof purchaseLineRiskNotes>
    sourceInsight?: PurchaseLineSourceInsight
    source?: PurchaseSource
    prefix: string
    onPreview: (revisionId: string) => void
}) {
    const mismatchedProduct = comparison.mismatchedProduct
    const otherWarnings = notes.warnings.filter(
        (warning) => warning !== "商品 / 规格不一致",
    )
    const taxInverted = Boolean(sourceInsight?.taxInverted)
    const splitMergedExcess = Boolean(
        !comparison.quantityExcess &&
        sourceInsight &&
        sourceInsight.splitPurchaseLineCount > 1 &&
        sourceInsight.mergedQuantityExcess != null,
    )
    const isHighRisk =
        notes.externalPriceRisk ||
        comparison.highWarnings.length > 0 ||
        taxInverted ||
        splitMergedExcess
    const hasRisk =
        notes.externalPriceRisk ||
        notes.warnings.length > 0 ||
        taxInverted ||
        splitMergedExcess

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
                            {notes.externalPriceRisk ? (
                                <li className="font-medium">
                                    实际采购单价高于销售单含税成交单价
                                </li>
                            ) : null}
                            {taxInverted && sourceInsight?.netMargin ? (
                                <li className="font-medium">
                                    对外税后倒挂：含税盈利，扣税后不含税亏{" "}
                                    <MoneyValue
                                        value={
                                            sourceInsight.netMargin.absAmount
                                        }
                                        display="exact"
                                        inheritColor
                                    />
                                    （{sourceInsight.netMargin.percent}%，销项{" "}
                                    {sourceInsight.salesTaxRatePercent}% vs 进项{" "}
                                    {sourceInsight.inputTaxRatePercent ??
                                        "未填"}
                                    %）
                                </li>
                            ) : null}
                            {splitMergedExcess &&
                            sourceInsight?.mergedQuantityExcess ? (
                                <li className="font-medium">
                                    同销售行拆分为{" "}
                                    {sourceInsight.splitPurchaseLineCount}{" "}
                                    笔采购，合并超来源销售需求 +
                                    <QuantityValue
                                        value={
                                            sourceInsight.mergedQuantityExcess
                                        }
                                        unit={line.unitName}
                                    />
                                    ，请核对数量依据
                                </li>
                            ) : null}
                            {otherWarnings.map((warning) => (
                                <li key={warning}>
                                    {warning === PURCHASE_SUPPLY_PRICE_RISK &&
                                    comparison.supplyUnitShortfall ? (
                                        <>
                                            实际采购单价高出采购供给价{" "}
                                            <MoneyValue
                                                value={
                                                    comparison.supplyUnitShortfall
                                                }
                                                display="exact"
                                                inheritColor
                                            />
                                            {comparison.supplyUnitShortfallPercent ? (
                                                <>
                                                    （+
                                                    <RateValue
                                                        value={
                                                            comparison.supplyUnitShortfallPercent
                                                        }
                                                        precision={1}
                                                    />
                                                    ）
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
