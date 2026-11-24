"use client"

import { useEffect, useMemo, useState } from "react"
import {
    ArrowRightIcon,
    CrosshairIcon,
    FileTextIcon,
    InfoIcon,
    MessageSquareTextIcon,
} from "lucide-react"

import {
    MoneyValue,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import {
    buildPurchaseSourceComparisonModel,
    type CommercialComparisonItem,
    type ComparisonSeverity,
    type PurchaseSourceComparisonModel,
    type SalesLineCoverageRow,
    type SalesLineCoverageStatus,
    type SourceOrderComparison,
} from "./workspace-purchase-source-comparison-model"

export type PurchaseSourceComparisonTarget = Readonly<{
    revisionId?: string
    purchaseLineId?: string
}>

type CoverageFilter = "all" | "attention" | "uncovered" | "covered"

const commercialBadgeVariant: Record<
    ComparisonSeverity,
    "destructive" | "warning" | "info" | "success" | "neutral"
> = {
    severe: "destructive",
    warning: "warning",
    info: "info",
    ok: "success",
    unknown: "neutral",
}

const coverageStatusMeta: Record<
    SalesLineCoverageStatus,
    { label: string; variant: "success" | "info" | "destructive" | "neutral" }
> = {
    full: { label: "全量采购", variant: "success" },
    partial: { label: "部分采购", variant: "info" },
    excess: { label: "超需求", variant: "destructive" },
    mismatch: { label: "待核对", variant: "destructive" },
    uncovered: { label: "本单未含", variant: "neutral" },
}

export function WorkspacePurchaseSourceComparison({
    idPrefix,
    orders,
    snapshots,
    purchaseLines,
    purchaseFulfillment,
    purchaseExtraSections,
    extraCostSummary,
    precomputedModel,
    target,
    onTargetChange,
    onPreview,
    onLocatePurchaseLine,
}: {
    idPrefix: string
    orders: NonNullable<ApprovalMaterials["source_sales_orders"]>
    snapshots?: ApprovalMaterials["display"]["source_sales"]
    purchaseLines?: ApprovalMaterials["purchase_lines"]
    purchaseFulfillment?: ApprovalMaterials["purchase_fulfillment"]
    purchaseExtraSections?: ApprovalMaterials["display"]["source"]["extra_sections"]
    extraCostSummary?: ApprovalMaterials["extra_cost_summary"]
    /** 若父组件已预先计算模型可直接传入，避免重复计算。 */
    precomputedModel?: PurchaseSourceComparisonModel
    target?: PurchaseSourceComparisonTarget
    onTargetChange: (target: PurchaseSourceComparisonTarget) => void
    onPreview: (revisionId: string) => void
    onLocatePurchaseLine?: (purchaseLineId: string) => void
}) {
    const [filter, setFilter] = useState<CoverageFilter>("all")
    const hasSalesLines = orders.some(
        (order) => order.revision.commercial_lines.length > 0,
    )
    const { names: unitNames } = useProductUnitNames([], hasSalesLines)

    const model = useMemo(
        () =>
            precomputedModel ??
            buildPurchaseSourceComparisonModel({
                orders,
                snapshots,
                purchaseLines,
                purchaseFulfillment,
                purchaseExtraSections,
                extraCostSummary,
                unitNames,
            }),
        [
            extraCostSummary,
            orders,
            precomputedModel,
            purchaseExtraSections,
            purchaseFulfillment,
            purchaseLines,
            snapshots,
            unitNames,
        ],
    )

    const activeOrder: SourceOrderComparison | undefined = useMemo(() => {
        if (!model.orders.length) return undefined
        if (target?.revisionId) {
            const found = model.orders.find(
                (entry) => entry.revisionId === target.revisionId,
            )
            if (found) return found
        }
        return model.orders[0]
    }, [model.orders, target?.revisionId])

    // 从左表点击「对照本行」时自动滚动定位到对应销售行
    const selectedPurchaseLineId = target?.purchaseLineId
    const selectedSalesRowId = useMemo(() => {
        if (!selectedPurchaseLineId || !activeOrder) return undefined
        const matchedRow = activeOrder.coverageRows.find((row) =>
            row.matchedPurchaseLines.some(
                (line) => line.id === selectedPurchaseLineId,
            ),
        )
        return matchedRow?.salesLineId
    }, [activeOrder, selectedPurchaseLineId])

    useEffect(() => {
        if (!selectedSalesRowId) return
        const rowDomId = `${idPrefix}-sales-line-${toAutomationIdSegment(selectedSalesRowId)}`
        const frame = window.requestAnimationFrame(() => {
            document
                .getElementById(rowDomId)
                ?.scrollIntoView({ behavior: "smooth", block: "nearest" })
        })
        return () => window.cancelAnimationFrame(frame)
    }, [idPrefix, selectedSalesRowId])

    if (!activeOrder) {
        return (
            <p className="text-body-compact text-muted-foreground">
                本次采购提交未保留来源销售单资料，请联系申请人核对。
            </p>
        )
    }

    const attentionRowsCount = activeOrder.coverageRows.filter(
        (row) => row.hasAttention,
    ).length
    const uncoveredRowsCount = activeOrder.coverageRows.filter(
        (row) => row.coverageStatus === "uncovered",
    ).length
    const coveredRowsCount =
        activeOrder.coverageRows.length - uncoveredRowsCount

    const effectiveFilter: CoverageFilter =
        (filter === "attention" && attentionRowsCount === 0) ||
        (filter === "uncovered" && uncoveredRowsCount === 0) ||
        (filter === "covered" &&
            (coveredRowsCount === 0 || uncoveredRowsCount === 0))
            ? "all"
            : filter

    const visibleRows = activeOrder.coverageRows.filter((row) => {
        if (row.salesLineId === selectedSalesRowId) return true
        switch (effectiveFilter) {
            case "attention":
                return row.hasAttention
            case "uncovered":
                return row.coverageStatus === "uncovered"
            case "covered":
                return row.coverageStatus !== "uncovered"
            default:
                return true
        }
    })

    const commercialAttentionCount = activeOrder.commercialChecks.filter(
        (check) => check.status === "severe" || check.status === "warning",
    ).length

    const ledger = activeOrder.ledger
    const hasSignificantTaxDrag = Boolean(
        ledger?.taxDragPoints &&
        (() => {
            try {
                return compareDecimal(ledger.taxDragPoints, "0.5", 1) >= 0
            } catch {
                return false
            }
        })(),
    )
    const hasPositiveVatGap = Boolean(
        ledger?.vatGap &&
        (() => {
            try {
                return compareDecimal(ledger.vatGap, "0", 2) > 0
            } catch {
                return false
            }
        })(),
    )

    return (
        <div
            data-slot="workspace-purchase-source-comparison"
            className="space-y-4"
        >
            {/* 顶部：来源销售单选择 + 完整销售单预览 + 客户/项目/合同元信息 */}
            <header className="space-y-1.5">
                <div className="flex flex-wrap items-center justify-between gap-2">
                    {model.orders.length > 1 ? (
                        <Select
                            value={activeOrder.revisionId}
                            onValueChange={(value) => {
                                if (value) {
                                    onTargetChange({ revisionId: value })
                                }
                            }}
                        >
                            <SelectTrigger
                                id={`${idPrefix}-source-order`}
                                aria-label="切换来源销售单"
                                size="sm"
                                className="min-w-0 flex-1"
                            >
                                <SelectValue className="min-w-0">
                                    <span className="num truncate">
                                        {activeOrder.documentNo} · V
                                        {activeOrder.revisionNo}
                                    </span>
                                </SelectValue>
                            </SelectTrigger>
                            <SelectContent
                                align="start"
                                alignItemWithTrigger={false}
                            >
                                {model.orders.map((entry) => (
                                    <SelectItem
                                        key={entry.revisionId}
                                        idPrefix={`${idPrefix}-source-order`}
                                        value={entry.revisionId}
                                        className="num whitespace-normal"
                                    >
                                        {entry.documentNo} ·{" "}
                                        {entry.customerName} · V
                                        {entry.revisionNo}
                                        {entry.attentionCount > 0
                                            ? `（${entry.attentionCount} 项关注）`
                                            : ""}
                                    </SelectItem>
                                ))}
                            </SelectContent>
                        </Select>
                    ) : (
                        <p className="num text-body-compact font-semibold wrap-anywhere text-foreground">
                            {activeOrder.documentNo}
                            <span className="ml-1.5 text-xs font-normal text-muted-foreground">
                                V{activeOrder.revisionNo}
                            </span>
                        </p>
                    )}

                    {activeOrder.hasFullOrder ? (
                        <Button
                            id={`${idPrefix}-preview-${toAutomationIdSegment(activeOrder.revisionId)}`}
                            type="button"
                            variant="outline"
                            size="xs"
                            onClick={() => onPreview(activeOrder.revisionId)}
                        >
                            <FileTextIcon aria-hidden="true" />
                            完整销售单
                        </Button>
                    ) : null}
                </div>

                <div className="flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                    <span className="font-medium text-foreground/90">
                        {activeOrder.customerName}
                    </span>
                    {activeOrder.projectName ? (
                        <>
                            <span aria-hidden="true">·</span>
                            <span>{activeOrder.projectName}</span>
                        </>
                    ) : null}
                    <span aria-hidden="true">·</span>
                    <span
                        className={activeOrder.contractNo ? "num" : undefined}
                    >
                        {activeOrder.contractNo
                            ? `合同 ${activeOrder.contractNo}`
                            : "无合同"}
                    </span>
                    {activeOrder.settlementPartyName ? (
                        <>
                            <span aria-hidden="true">·</span>
                            <span>{activeOrder.settlementPartyName}</span>
                        </>
                    ) : null}
                </div>

                {activeOrder.contractNotices.length > 0 ? (
                    <p className="text-xs text-warning-soft-foreground">
                        合同或凭证：{activeOrder.contractNotices.join("；")}
                    </p>
                ) : null}

                {model.orders.length > 1 ? (
                    <p className="num text-xs text-muted-foreground">
                        本采购单合并关联 {model.orders.length}{" "}
                        张销售单，下方数据仅统计当前所选销售单。
                    </p>
                ) : null}
            </header>

            {!activeOrder.hasFullOrder || !ledger ? (
                <p className="border-t border-border/60 pt-3 text-xs text-muted-foreground">
                    此次审批未冻结完整的来源销售正文，暂无法展开商务条款与整单覆盖对照。
                </p>
            ) : (
                <>
                    {/* 模块 1：商务、资金与票税对照（单据级风控） */}
                    <section aria-label="商务与风控对照" className="space-y-2">
                        <div className="flex items-center justify-between gap-2">
                            <h3 className="text-xs font-semibold text-foreground">
                                商务与风控对照
                            </h3>
                            <Badge
                                variant={
                                    activeOrder.commercialChecks.some(
                                        (c) => c.status === "severe",
                                    )
                                        ? "destructive"
                                        : commercialAttentionCount > 0
                                          ? "warning"
                                          : "success"
                                }
                            >
                                {commercialAttentionCount > 0
                                    ? `${commercialAttentionCount} 项需关注`
                                    : "条款匹配"}
                            </Badge>
                        </div>

                        <ol className="divide-y divide-border/60 border-y border-border/60">
                            {activeOrder.commercialChecks.map((check) => (
                                <CommercialCheckRow
                                    key={check.id}
                                    check={check}
                                />
                            ))}
                        </ol>

                        {activeOrder.businessRemark ? (
                            <div className="flex items-start gap-2 border-l-2 border-info bg-info-soft/30 px-2.5 py-2 text-xs">
                                <MessageSquareTextIcon
                                    aria-hidden="true"
                                    className="mt-0.5 size-3.5 shrink-0 text-info-soft-foreground"
                                />
                                <div className="min-w-0 space-y-0.5">
                                    <p className="font-medium text-info-soft-foreground">
                                        销售业务备注
                                    </p>
                                    <p className="whitespace-pre-wrap wrap-anywhere text-foreground/90">
                                        {activeOrder.businessRemark}
                                    </p>
                                </div>
                            </div>
                        ) : null}
                    </section>

                    {/* 模块 2A：整张销售单覆盖与生意账 */}
                    <section
                        aria-label="整单覆盖与生意账"
                        className="space-y-2"
                    >
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <h3 className="text-xs font-semibold text-foreground">
                                整单覆盖与生意账
                            </h3>
                            <Badge
                                variant={
                                    ledger.excessCount > 0 || ledger.taxInverted
                                        ? "destructive"
                                        : ledger.isFullOrderCovered
                                          ? "success"
                                          : "info"
                                }
                            >
                                {ledger.isExpenseOrder
                                    ? "费用采购单"
                                    : ledger.excessCount > 0
                                      ? `超需求 ${ledger.excessCount} 项`
                                      : ledger.isFullOrderCovered
                                        ? "整单全量覆盖"
                                        : `本单覆盖 ${ledger.coveredLineCount}/${ledger.totalSalesLineCount} 行`}
                            </Badge>
                        </div>

                        <div className="grid grid-cols-2 gap-x-4 gap-y-3 border-y border-border/60 py-3">
                            {/* 指标 1：销售明细覆盖 */}
                            <div className="min-w-0 space-y-0.5">
                                <p className="text-xs text-muted-foreground">
                                    {ledger.isExpenseOrder
                                        ? "来源销售规模"
                                        : "本单覆盖销售明细"}
                                </p>
                                {ledger.isExpenseOrder ? (
                                    <>
                                        <p className="num text-body-compact font-semibold text-foreground">
                                            共 {ledger.totalSalesLineCount}{" "}
                                            项商品
                                        </p>
                                        <p className="num text-xs text-muted-foreground">
                                            销售含税{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={ledger.salesGrossTotal}
                                            />
                                        </p>
                                    </>
                                ) : (
                                    <>
                                        <p className="num text-body-compact font-semibold text-foreground">
                                            {ledger.coveredLineCount} /{" "}
                                            {ledger.totalSalesLineCount} 行
                                            {ledger.salesCoveragePercent ? (
                                                <span className="ml-1 text-xs font-normal text-muted-foreground">
                                                    （占额{" "}
                                                    {
                                                        ledger.salesCoveragePercent
                                                    }
                                                    %）
                                                </span>
                                            ) : null}
                                        </p>
                                        <p className="num flex flex-wrap gap-x-2 text-xs text-muted-foreground">
                                            {ledger.fullCount > 0 ? (
                                                <span>
                                                    全量 {ledger.fullCount}
                                                </span>
                                            ) : null}
                                            {ledger.partialCount > 0 ? (
                                                <span>
                                                    部分 {ledger.partialCount}
                                                </span>
                                            ) : null}
                                            {ledger.excessCount > 0 ? (
                                                <span className="font-medium text-destructive-soft-foreground">
                                                    超量 {ledger.excessCount}
                                                </span>
                                            ) : null}
                                            {ledger.uncoveredCount > 0 ? (
                                                <span>
                                                    未含 {ledger.uncoveredCount}
                                                </span>
                                            ) : null}
                                        </p>
                                    </>
                                )}
                            </div>

                            {/* 指标 2：整单销售 vs 本单采购 */}
                            <div className="min-w-0 space-y-0.5">
                                <p className="text-xs text-muted-foreground">
                                    {ledger.isExpenseOrder
                                        ? "本单费用与占比"
                                        : "销售整单 vs 本单采购"}
                                </p>
                                {ledger.isExpenseOrder ? (
                                    <>
                                        <p className="num text-body-compact font-semibold text-foreground">
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    ledger.purchaseFeeGrossTotal
                                                }
                                            />
                                        </p>
                                        <p className="num text-xs text-muted-foreground">
                                            {ledger.purchaseFeeRatioPercent ? (
                                                `占来源销售额 ${ledger.purchaseFeeRatioPercent}%`
                                            ) : (
                                                <>
                                                    不含税销售{" "}
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            ledger.salesNetTotal
                                                        }
                                                    />
                                                </>
                                            )}
                                        </p>
                                    </>
                                ) : (
                                    <>
                                        <p className="num text-body-compact font-semibold text-foreground">
                                            <MoneyValue
                                                display="exact"
                                                value={ledger.salesGrossTotal}
                                            />
                                        </p>
                                        <p className="num text-xs text-muted-foreground">
                                            本单采购{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    ledger.purchaseGoodsGrossTotal
                                                }
                                            />
                                            {ledger.purchaseFeeGrossTotal ? (
                                                <>
                                                    {" "}
                                                    + 费{" "}
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            ledger.purchaseFeeGrossTotal
                                                        }
                                                    />
                                                </>
                                            ) : null}
                                        </p>
                                    </>
                                )}
                            </div>

                            {/* 指标 3 & 4：含税毛利 vs 扣税后不含税毛利（商品单）或附加费用汇总（费用单） */}
                            {ledger.isExpenseOrder ? (
                                ledger.extraCostSummary ? (
                                    <>
                                        <div className="min-w-0 space-y-0.5">
                                            <p className="text-xs text-muted-foreground">
                                                本单外已有附加费
                                            </p>
                                            <p className="num text-body-compact font-semibold text-foreground">
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        ledger.extraCostSummary
                                                            .occurredGrossTotal
                                                    }
                                                />
                                            </p>
                                            <p className="num text-xs text-muted-foreground">
                                                待发生{" "}
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        ledger.extraCostSummary
                                                            .pendingGrossTotal
                                                    }
                                                />
                                            </p>
                                        </div>
                                        <div className="min-w-0 space-y-0.5">
                                            <p className="text-xs text-muted-foreground">
                                                计入本单累计附加费
                                            </p>
                                            <p className="num text-body-compact font-semibold text-foreground">
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        ledger.extraCostSummary
                                                            .combinedWithCurrentFeeTotal
                                                    }
                                                />
                                            </p>
                                            <p className="num text-xs text-muted-foreground">
                                                {ledger.extraCostSummary
                                                    .combinedRatioPercent
                                                    ? `累计占销售额 ${ledger.extraCostSummary.combinedRatioPercent}%`
                                                    : "—"}
                                            </p>
                                        </div>
                                    </>
                                ) : null
                            ) : (
                                <>
                                    <div className="min-w-0 space-y-0.5">
                                        <p className="text-xs text-muted-foreground">
                                            含税毛利（本单对应）
                                        </p>
                                        {ledger.grossMargin ? (
                                            <p
                                                className={cn(
                                                    "num text-body-compact font-semibold",
                                                    ledger.grossMargin.negative
                                                        ? "text-destructive-soft-foreground"
                                                        : "text-foreground",
                                                )}
                                            >
                                                {ledger.grossMargin.negative
                                                    ? "亏 "
                                                    : ""}
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        ledger.grossMargin
                                                            .absAmount
                                                    }
                                                    inheritColor={
                                                        ledger.grossMargin
                                                            .negative
                                                    }
                                                />{" "}
                                                <span className="text-xs font-medium">
                                                    {ledger.grossMargin.negative
                                                        ? `-${ledger.grossMargin.absPercent}%`
                                                        : `${ledger.grossMargin.percent}%`}
                                                </span>
                                            </p>
                                        ) : (
                                            <p className="text-body-compact text-muted-foreground">
                                                —
                                            </p>
                                        )}
                                        <p className="num text-xs text-muted-foreground">
                                            对应销售{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    ledger.correspondingSalesGrossTotal
                                                }
                                            />
                                        </p>
                                    </div>

                                    <div className="min-w-0 space-y-0.5">
                                        <p className="text-xs text-muted-foreground">
                                            不含税毛利（扣税后）
                                        </p>
                                        {ledger.netMargin ? (
                                            <p
                                                className={cn(
                                                    "num text-body-compact font-semibold",
                                                    ledger.netMargin.negative
                                                        ? "text-destructive-soft-foreground"
                                                        : hasSignificantTaxDrag
                                                          ? "text-warning-soft-foreground"
                                                          : "text-foreground",
                                                )}
                                            >
                                                {ledger.netMargin.negative
                                                    ? "亏 "
                                                    : ""}
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        ledger.netMargin
                                                            .absAmount
                                                    }
                                                    inheritColor={
                                                        ledger.netMargin
                                                            .negative ||
                                                        hasSignificantTaxDrag
                                                    }
                                                />{" "}
                                                <span className="text-xs font-medium">
                                                    {ledger.netMargin.negative
                                                        ? `-${ledger.netMargin.absPercent}%`
                                                        : `${ledger.netMargin.percent}%`}
                                                </span>
                                            </p>
                                        ) : (
                                            <p className="text-xs text-muted-foreground">
                                                待补齐进项税率
                                            </p>
                                        )}
                                        <p className="num text-xs text-muted-foreground">
                                            {ledger.taxInverted ? (
                                                <span className="font-medium text-destructive-soft-foreground">
                                                    含税盈利、税后倒挂
                                                </span>
                                            ) : hasSignificantTaxDrag &&
                                              ledger.taxDragPoints ? (
                                                <span className="text-warning-soft-foreground">
                                                    较含税低{" "}
                                                    {ledger.taxDragPoints}{" "}
                                                    个百分点
                                                    {hasPositiveVatGap &&
                                                    ledger.vatGap ? (
                                                        <>
                                                            {" "}
                                                            · 税差{" "}
                                                            <MoneyValue
                                                                display="exact"
                                                                value={
                                                                    ledger.vatGap
                                                                }
                                                                inheritColor
                                                            />
                                                        </>
                                                    ) : null}
                                                </span>
                                            ) : hasPositiveVatGap &&
                                              ledger.vatGap ? (
                                                <>
                                                    进销项税差{" "}
                                                    <MoneyValue
                                                        display="exact"
                                                        value={ledger.vatGap}
                                                    />
                                                </>
                                            ) : ledger.correspondingSalesNetTotal ? (
                                                <>
                                                    不含税销售{" "}
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            ledger.correspondingSalesNetTotal
                                                        }
                                                    />
                                                </>
                                            ) : (
                                                "按进销项不含税额核算"
                                            )}
                                        </p>
                                    </div>
                                </>
                            )}
                        </div>

                        {ledger.giftCoveredCount > 0 &&
                        ledger.giftPurchaseGrossTotal ? (
                            <p className="num flex items-start gap-1.5 text-xs text-info-soft-foreground">
                                <InfoIcon
                                    aria-hidden="true"
                                    className="mt-0.5 size-3.5 shrink-0"
                                />
                                <span>
                                    本单含 {ledger.giftCoveredCount}{" "}
                                    项销售赠品（采购成本{" "}
                                    <MoneyValue
                                        display="exact"
                                        value={ledger.giftPurchaseGrossTotal}
                                        inheritColor
                                        className="font-medium"
                                    />
                                    ），已计入上方对应毛利核算。
                                </span>
                            </p>
                        ) : null}

                        {ledger.unallocatedSharedFeeTotal ? (
                            <p className="num text-xs text-muted-foreground">
                                另：本采购单含{" "}
                                <MoneyValue
                                    display="exact"
                                    value={ledger.unallocatedSharedFeeTotal}
                                />{" "}
                                跨单费用行，未分摊计入单张销售单毛利。
                            </p>
                        ) : null}

                        {!ledger.isFullOrderCovered ? (
                            <p className="text-xs text-muted-foreground">
                                口径说明：基于本次审批冻结资料统计，不含同销售单下其他独立采购单或库存分配。
                            </p>
                        ) : null}
                    </section>

                    {/* 模块 2B：来源销售单全量明细覆盖清单 */}
                    <section
                        aria-label="销售明细覆盖清单"
                        className="space-y-2"
                    >
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <h3 className="text-xs font-semibold text-foreground">
                                销售明细覆盖
                                <span className="num ml-1 font-normal text-muted-foreground">
                                    （共 {activeOrder.coverageRows.length} 项）
                                </span>
                            </h3>
                            <div
                                role="group"
                                aria-label="筛选销售明细"
                                className={detailWorkspaceStyles.subTabs}
                            >
                                <button
                                    id={`${idPrefix}-filter-all`}
                                    type="button"
                                    aria-pressed={effectiveFilter === "all"}
                                    data-active={
                                        effectiveFilter === "all" || undefined
                                    }
                                    className={detailWorkspaceStyles.subTab}
                                    onClick={() => setFilter("all")}
                                >
                                    全部{" "}
                                    <span className="num">
                                        {activeOrder.coverageRows.length}
                                    </span>
                                </button>
                                {attentionRowsCount > 0 ? (
                                    <button
                                        id={`${idPrefix}-filter-attention`}
                                        type="button"
                                        aria-pressed={
                                            effectiveFilter === "attention"
                                        }
                                        data-active={
                                            effectiveFilter === "attention" ||
                                            undefined
                                        }
                                        className={detailWorkspaceStyles.subTab}
                                        onClick={() => setFilter("attention")}
                                    >
                                        需关注{" "}
                                        <span className="num">
                                            {attentionRowsCount}
                                        </span>
                                    </button>
                                ) : null}
                                {uncoveredRowsCount > 0 ? (
                                    <button
                                        id={`${idPrefix}-filter-uncovered`}
                                        type="button"
                                        aria-pressed={
                                            effectiveFilter === "uncovered"
                                        }
                                        data-active={
                                            effectiveFilter === "uncovered" ||
                                            undefined
                                        }
                                        className={detailWorkspaceStyles.subTab}
                                        onClick={() => setFilter("uncovered")}
                                    >
                                        未含{" "}
                                        <span className="num">
                                            {uncoveredRowsCount}
                                        </span>
                                    </button>
                                ) : null}
                                {uncoveredRowsCount > 0 &&
                                coveredRowsCount > 0 ? (
                                    <button
                                        id={`${idPrefix}-filter-covered`}
                                        type="button"
                                        aria-pressed={
                                            effectiveFilter === "covered"
                                        }
                                        data-active={
                                            effectiveFilter === "covered" ||
                                            undefined
                                        }
                                        className={detailWorkspaceStyles.subTab}
                                        onClick={() => setFilter("covered")}
                                    >
                                        本单{" "}
                                        <span className="num">
                                            {coveredRowsCount}
                                        </span>
                                    </button>
                                ) : null}
                            </div>
                        </div>

                        {visibleRows.length > 0 ? (
                            <ol className="divide-y divide-border/60 border-y border-border/60">
                                {visibleRows.map((row) => (
                                    <SalesCoverageLineItem
                                        key={row.salesLineId}
                                        idPrefix={idPrefix}
                                        row={row}
                                        selected={
                                            row.salesLineId ===
                                            selectedSalesRowId
                                        }
                                        onLocatePurchaseLine={
                                            onLocatePurchaseLine
                                        }
                                    />
                                ))}
                            </ol>
                        ) : (
                            <p className="border-t border-border/60 pt-3 text-xs text-muted-foreground">
                                当前筛选条件下暂无销售明细。
                            </p>
                        )}
                    </section>

                    {/* 本单费用行 / 未匹配行兜底 */}
                    {activeOrder.extraPurchaseLines.length > 0 ? (
                        <section
                            aria-label="本单费用与其他采购行"
                            className="space-y-1.5"
                        >
                            <h3 className="text-xs font-semibold text-foreground">
                                本单附加费用与未匹配项
                            </h3>
                            <ul className="divide-y divide-border/60 border-y border-border/60">
                                {activeOrder.extraPurchaseLines.map((extra) => (
                                    <li
                                        key={extra.id}
                                        className="flex flex-wrap items-center justify-between gap-2 py-2 text-xs"
                                    >
                                        <div className="min-w-0 space-y-0.5">
                                            <div className="flex flex-wrap items-center gap-1.5">
                                                <span className="num text-muted-foreground">
                                                    采购第 {extra.lineNo} 行
                                                </span>
                                                <Badge
                                                    variant={
                                                        extra.isFee
                                                            ? "neutral"
                                                            : "warning"
                                                    }
                                                >
                                                    {extra.isFee
                                                        ? "费用"
                                                        : "未匹配销售行"}
                                                </Badge>
                                                <span className="font-medium wrap-anywhere text-foreground">
                                                    {extra.title}
                                                </span>
                                            </div>
                                            <p className="num text-muted-foreground">
                                                含税{" "}
                                                <MoneyValue
                                                    display="exact"
                                                    value={extra.grossAmount}
                                                />
                                                {extra.inputTaxRatePercent !=
                                                null
                                                    ? ` · 进项 ${extra.inputTaxRatePercent}%`
                                                    : ""}
                                                {extra.ratioOfSalesPercent
                                                    ? ` · 占销售 ${extra.ratioOfSalesPercent}%`
                                                    : ""}
                                            </p>
                                        </div>
                                        {onLocatePurchaseLine ? (
                                            <Button
                                                id={`${idPrefix}-locate-${toAutomationIdSegment(extra.id)}`}
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                className={
                                                    detailWorkspaceStyles.inlineLinkButton
                                                }
                                                onClick={() =>
                                                    onLocatePurchaseLine(
                                                        extra.id,
                                                    )
                                                }
                                            >
                                                <CrosshairIcon aria-hidden="true" />
                                                定位
                                            </Button>
                                        ) : null}
                                    </li>
                                ))}
                            </ul>
                        </section>
                    ) : null}
                </>
            )}
        </div>
    )
}

function CommercialCheckRow({ check }: { check: CommercialComparisonItem }) {
    const isRisk = check.status === "severe" || check.status === "warning"
    return (
        <li
            className={cn(
                "space-y-1.5 py-2.5",
                check.status === "severe" &&
                    "-mx-2 rounded-md bg-destructive-soft/25 px-2",
                check.status === "warning" &&
                    "-mx-2 rounded-md bg-warning-soft/25 px-2",
            )}
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="text-xs font-medium text-foreground">
                    {check.title}
                </span>
                <Badge variant={commercialBadgeVariant[check.status]}>
                    {check.badge}
                </Badge>
            </div>

            <div className="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-start gap-x-2 text-xs wrap-anywhere">
                <div className="min-w-0">
                    <span className="mr-1 text-muted-foreground">
                        {check.salesLabel}
                    </span>
                    <span className="num text-foreground/90">
                        {check.salesValue}
                    </span>
                </div>
                <ArrowRightIcon
                    aria-hidden="true"
                    className="mt-0.5 size-3.5 shrink-0 text-muted-foreground"
                />
                <div className="min-w-0">
                    <span className="mr-1 text-muted-foreground">
                        {check.purchaseLabel}
                    </span>
                    <span className="num font-medium text-foreground">
                        {check.purchaseValue}
                    </span>
                </div>
            </div>

            <p
                className={cn(
                    "num text-xs",
                    check.status === "severe"
                        ? "font-medium text-destructive-soft-foreground"
                        : isRisk
                          ? "text-warning-soft-foreground"
                          : "text-muted-foreground",
                )}
            >
                {check.detail}
            </p>
        </li>
    )
}

function SalesCoverageLineItem({
    idPrefix,
    row,
    selected,
    onLocatePurchaseLine,
}: {
    idPrefix: string
    row: SalesLineCoverageRow
    selected: boolean
    onLocatePurchaseLine?: (purchaseLineId: string) => void
}) {
    const status = coverageStatusMeta[row.coverageStatus]
    const isUncovered = row.coverageStatus === "uncovered"
    const unit = row.unitLabel ?? ""
    const singlePurchase =
        row.matchedPurchaseLines.length === 1
            ? row.matchedPurchaseLines[0]
            : undefined

    return (
        <li
            id={`${idPrefix}-sales-line-${toAutomationIdSegment(row.salesLineId)}`}
            className={cn(
                "space-y-1.5 py-2.5",
                selected &&
                    "-mx-2 rounded-md bg-info-soft/35 px-2 ring-1 ring-inset ring-info-border",
                isUncovered && !selected && "opacity-75",
            )}
        >
            {/* 销售行头：行号 + 品名 + 赠品/规格 + 覆盖状态 */}
            <div className="flex items-start justify-between gap-2">
                <div className="min-w-0 flex-1 space-y-0.5">
                    <div className="flex flex-wrap items-center gap-1.5">
                        <span className="num text-xs text-muted-foreground">
                            销 #{row.salesLineNo}
                        </span>
                        <span className="text-body-compact font-medium wrap-anywhere text-foreground">
                            {row.title}
                        </span>
                        {row.isGift ? (
                            <Badge variant="neutral">赠品</Badge>
                        ) : null}
                    </div>
                    {row.specification || row.welfareLabel ? (
                        <p className="text-xs wrap-anywhere text-muted-foreground">
                            {[row.specification, row.welfareLabel]
                                .filter(Boolean)
                                .join(" · ")}
                        </p>
                    ) : null}
                </div>
                <Badge variant={status.variant} className="shrink-0">
                    {status.label}
                </Badge>
            </div>

            {isUncovered ? (
                <p className="num flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                    {row.salesQuantity ? (
                        <span>
                            需求{" "}
                            <QuantityValue
                                value={row.salesQuantity}
                                unit={unit || undefined}
                            />
                        </span>
                    ) : null}
                    <span>
                        销售{" "}
                        <MoneyValue
                            display="exact"
                            value={row.salesGrossAmount}
                        />
                    </span>
                    <span>销项 {row.salesTaxRatePercent}%</span>
                    {row.salesDueDate ? (
                        <span>承诺 {row.salesDueDate}</span>
                    ) : null}
                </p>
            ) : (
                <>
                    {/* 风险/关注标签 */}
                    {row.attentionTags.length > 0 ? (
                        <div className="flex flex-wrap items-center gap-1">
                            {row.attentionTags.map((tag) => (
                                <Badge key={tag.label} variant={tag.tone}>
                                    {tag.label}
                                </Badge>
                            ))}
                        </div>
                    ) : null}

                    {/* 数量与交期对照 */}
                    <div className="num flex flex-wrap items-center gap-x-3 gap-y-0.5 text-xs text-muted-foreground">
                        <span>
                            数量：本单{" "}
                            <span className="font-medium text-foreground">
                                {row.purchasedQuantityTotal ?? "待核"}
                            </span>
                            {row.salesQuantity
                                ? ` / 销售 ${row.salesQuantity}`
                                : ""}
                            {unit ? ` ${unit}` : ""}
                            {row.excessQuantity ? (
                                <span className="ml-1 font-medium text-destructive-soft-foreground">
                                    （超 +{row.excessQuantity}）
                                </span>
                            ) : row.remainingQuantity ? (
                                <span className="ml-1">
                                    （余 {row.remainingQuantity}）
                                </span>
                            ) : null}
                            {row.matchedPurchaseLines.length > 1
                                ? ` · ${row.matchedPurchaseLines.length} 笔`
                                : ""}
                        </span>
                        {singlePurchase?.expectedDeliveryDate ||
                        row.salesDueDate ? (
                            <span
                                className={cn(
                                    row.maxDelayDays > 0 &&
                                        "font-medium text-warning-soft-foreground",
                                )}
                            >
                                交期：
                                {singlePurchase?.expectedDeliveryDate ?? "多笔"}
                                {row.salesDueDate
                                    ? ` / 承诺 ${row.salesDueDate}`
                                    : ""}
                            </span>
                        ) : null}
                    </div>

                    {/* 价格链 + 票税 + 含税/不含税双口径毛利 */}
                    <div className="num space-y-1 rounded-md bg-muted/35 px-2.5 py-1.5 text-xs">
                        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-0.5 text-foreground/90">
                            <span>
                                <span className="text-muted-foreground">
                                    {row.isGift ? "赠品 " : "售 "}
                                </span>
                                <MoneyValue
                                    display="exact"
                                    value={row.salesUnitPriceGross ?? "0"}
                                />
                            </span>
                            <span>
                                <span className="text-muted-foreground">
                                    采{" "}
                                </span>
                                {singlePurchase?.unitCostGross != null ? (
                                    <MoneyValue
                                        display="exact"
                                        value={singlePurchase.unitCostGross}
                                        className="font-medium"
                                    />
                                ) : (
                                    <MoneyValue
                                        display="exact"
                                        value={row.purchaseGrossTotal}
                                        className="font-medium"
                                    />
                                )}
                            </span>
                            {row.supplyPrice ? (
                                <span className="text-muted-foreground">
                                    供{" "}
                                    <MoneyValue
                                        display="exact"
                                        value={row.supplyPrice}
                                        inheritColor
                                    />
                                    {row.priceBasisLabel
                                        ? `（${row.priceBasisLabel}）`
                                        : ""}
                                    {row.originalSupplyPrice &&
                                    (row.supplyAdjustment === "increased" ||
                                        row.supplyAdjustment ===
                                            "decreased") ? (
                                        <>
                                            {" · 原 "}
                                            <MoneyValue
                                                display="exact"
                                                value={row.originalSupplyPrice}
                                                inheritColor
                                            />
                                        </>
                                    ) : null}
                                </span>
                            ) : null}
                        </div>

                        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-0.5">
                            <span
                                className={cn(
                                    row.lowerInputTaxGapPoints != null
                                        ? "font-medium text-warning-soft-foreground"
                                        : "text-muted-foreground",
                                )}
                            >
                                销项 {row.salesTaxRatePercent}% vs 进项{" "}
                                {row.inputTaxRatesPercent.length > 0
                                    ? row.inputTaxRatesPercent
                                          .map((rate) => `${rate}%`)
                                          .join("/")
                                    : "未填"}
                            </span>

                            {row.isGift ? (
                                <span className="text-muted-foreground">
                                    赠品采购成本{" "}
                                    <MoneyValue
                                        display="exact"
                                        value={row.purchaseGrossTotal}
                                        className="font-medium text-foreground"
                                    />
                                </span>
                            ) : (
                                <>
                                    {row.grossMargin ? (
                                        <span
                                            className={cn(
                                                row.grossMargin.negative
                                                    ? "font-medium text-destructive-soft-foreground"
                                                    : "text-muted-foreground",
                                            )}
                                        >
                                            含税{" "}
                                            {row.grossMargin.negative
                                                ? `亏 ${row.grossMargin.absPercent}%`
                                                : `${row.grossMargin.percent}%`}
                                        </span>
                                    ) : null}
                                    {row.netMargin ? (
                                        <span
                                            className={cn(
                                                row.netMargin.negative
                                                    ? "font-semibold text-destructive-soft-foreground"
                                                    : row.lowerInputTaxGapPoints !=
                                                        null
                                                      ? "font-medium text-warning-soft-foreground"
                                                      : "text-foreground/90",
                                            )}
                                        >
                                            不含税{" "}
                                            {row.netMargin.negative
                                                ? `亏 ${row.netMargin.absPercent}%`
                                                : `${row.netMargin.percent}%`}
                                        </span>
                                    ) : null}
                                </>
                            )}
                        </div>
                    </div>

                    {/* 品规差异证据 */}
                    {row.mismatchedProduct ? (
                        <p className="text-xs wrap-anywhere text-destructive-soft-foreground">
                            品规差异：采购录入「
                            {row.matchedPurchaseLines
                                .map((line) =>
                                    [line.title, line.specification]
                                        .filter(Boolean)
                                        .join(" · "),
                                )
                                .join(" / ")}
                            」
                        </p>
                    ) : null}

                    {row.duplicateGroupSize > 1 ? (
                        <p className="num text-xs text-muted-foreground">
                            销售单含 {row.duplicateGroupSize}{" "}
                            项完全相同品规单价明细，请对照核验汇总数量。
                        </p>
                    ) : null}

                    {/* 定位到左侧采购行 */}
                    {onLocatePurchaseLine ? (
                        <div className="flex flex-wrap items-center gap-2 pt-0.5">
                            {row.matchedPurchaseLines.map((pLine) => (
                                <Button
                                    key={pLine.id}
                                    id={`${idPrefix}-locate-${toAutomationIdSegment(pLine.id)}`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className={
                                        detailWorkspaceStyles.inlineLinkButton
                                    }
                                    onClick={() =>
                                        onLocatePurchaseLine(pLine.id)
                                    }
                                >
                                    <CrosshairIcon aria-hidden="true" />
                                    <span className="num">
                                        定位采购第 {pLine.lineNo} 行
                                    </span>
                                </Button>
                            ))}
                        </div>
                    ) : null}
                </>
            )}
        </li>
    )
}
