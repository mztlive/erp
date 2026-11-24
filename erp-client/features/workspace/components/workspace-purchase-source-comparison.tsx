"use client"

import { useEffect, useMemo, useState } from "react"
import {
    AlertTriangleIcon,
    ArrowDownIcon,
    ArrowRightIcon,
    ArrowUpIcon,
    Building2Icon,
    CalendarIcon,
    CheckIcon,
    ChevronDownIcon,
    CrosshairIcon,
    FileCheck2Icon,
    // FileTextIcon,
    InfoIcon,
    MessageSquareTextIcon,
    ReceiptTextIcon,
    TrendingDownIcon,
    TrendingUpIcon,
    TruckIcon,
    WalletIcon,
} from "lucide-react"

import {
    MoneyValue,
    QuantityValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import type {
    ApprovalMaterials,
    ApprovalPurchaseLine,
} from "@/features/approval-workflow/api/materials"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { PurchaseProfitValue } from "./workspace-purchase-amount-details"
import { purchaseComparison } from "./workspace-purchase-comparison-model"
import { PurchaseLinePriceDetails } from "./workspace-purchase-line-price-details"
import {
    buildPurchaseSourceComparisonModel,
    type CommercialComparisonItem,
    type ComparisonSeverity,
    type ExtraPurchaseLineItem,
    type PurchaseSourceComparisonModel,
    type SalesLineCoverageRow,
    type SalesLineCoverageStatus,
    type SourceOrderBusinessLedger,
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

const commercialSeverityRank: Record<ComparisonSeverity, number> = {
    severe: 0,
    warning: 1,
    info: 2,
    unknown: 3,
    ok: 4,
}

const coverageStatusMeta: Record<
    SalesLineCoverageStatus,
    { label: string; variant: "success" | "info" | "destructive" | "neutral" }
> = {
    full: { label: "全量", variant: "success" },
    partial: { label: "部分", variant: "info" },
    excess: { label: "超需求", variant: "destructive" },
    mismatch: { label: "待核对", variant: "destructive" },
    uncovered: { label: "未含", variant: "neutral" },
}

const commercialCheckIcons = {
    payment: WalletIcon,
    tax: ReceiptTextIcon,
    fulfillment: TruckIcon,
} as const

function formatCompactDate(
    date: string | null | undefined,
    referenceDate?: string | null,
): string {
    if (!date) return ""
    if (
        date.length === 10 &&
        referenceDate?.length === 10 &&
        date.slice(0, 4) === referenceDate.slice(0, 4)
    ) {
        return date.slice(5)
    }
    return date
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
    // onPreview,
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

    const activeOrderIndex = activeOrder
        ? model.orders.findIndex(
              (entry) => entry.revisionId === activeOrder.revisionId,
          )
        : -1

    // 行对照使用被选中的采购行，拆分采购时仍能看到该行自己的金额。
    const selectedPurchaseLineId = target?.purchaseLineId
    const selectedPurchaseLine = selectedPurchaseLineId
        ? purchaseLines?.find(
              (line) =>
                  line.id === selectedPurchaseLineId &&
                  line.source?.revision_id === activeOrder?.revisionId,
          )
        : undefined
    const selectedPriceDetailsId = selectedPurchaseLine
        ? `${idPrefix}-purchase-prices-${toAutomationIdSegment(selectedPurchaseLine.id)}`
        : undefined
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
        if (!selectedPriceDetailsId) return
        const frame = window.requestAnimationFrame(() => {
            document
                .getElementById(selectedPriceDetailsId)
                ?.scrollIntoView({ behavior: "smooth", block: "nearest" })
        })
        return () => window.cancelAnimationFrame(frame)
    }, [selectedPriceDetailsId])

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

    const ledger = activeOrder.ledger
    const showTopSelectedPriceDetails = Boolean(
        selectedPurchaseLine &&
        (!selectedSalesRowId || !activeOrder.hasFullOrder),
    )

    return (
        <TooltipProvider>
            <div
                data-slot="workspace-purchase-source-comparison"
                className="@container/source-comparison min-w-0 space-y-4 wrap-anywhere [&_[data-slot=money-value]]:max-w-full [&_[data-slot=money-value-number]]:whitespace-normal"
            >
                {/* 顶部：来源销售单切换 + 原单预览 + 图标化元信息 */}
                <header className="space-y-1.5">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        {model.orders.length > 1 ? (
                            <div className="flex min-w-0 flex-1 items-center gap-1.5">
                                <Select
                                    value={activeOrder.revisionId}
                                    onValueChange={(value) => {
                                        if (value) {
                                            onTargetChange({
                                                revisionId: value,
                                            })
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
                                <Tooltip>
                                    <TooltipTrigger
                                        render={
                                            <button
                                                id={`${idPrefix}-merged-orders-info`}
                                                type="button"
                                                aria-label={`合并关联 ${model.orders.length} 张销售单说明`}
                                                className="num inline-flex h-control-xs shrink-0 cursor-help items-center gap-1 rounded-sm bg-muted/65 px-1.5 text-xs text-muted-foreground transition-colors hover:bg-muted hover:text-foreground"
                                            />
                                        }
                                    >
                                        {activeOrderIndex + 1}/
                                        {model.orders.length}
                                        <InfoIcon
                                            aria-hidden="true"
                                            className="size-3"
                                        />
                                    </TooltipTrigger>
                                    <TooltipContent className="block">
                                        本采购单合并关联 {model.orders.length}{" "}
                                        张销售单，下方仅统计当前所选销售单。
                                    </TooltipContent>
                                </Tooltip>
                            </div>
                        ) : (
                            <p className="num text-body-compact font-semibold wrap-anywhere text-foreground">
                                {activeOrder.documentNo}
                                <span className="ml-1.5 text-xs font-normal text-muted-foreground">
                                    V{activeOrder.revisionNo}
                                </span>
                            </p>
                        )}

                        {/* {activeOrder.hasFullOrder ? (
                            <Button
                                id={`${idPrefix}-preview-${toAutomationIdSegment(activeOrder.revisionId)}`}
                                type="button"
                                variant="outline"
                                size="xs"
                                aria-label="预览完整销售单"
                                onClick={() =>
                                    onPreview(activeOrder.revisionId)
                                }
                            >
                                <FileTextIcon aria-hidden="true" />
                                销售原单
                            </Button>
                        ) : null} */}
                    </div>

                    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
                        <span className="inline-flex min-w-0 items-center gap-1 font-medium text-foreground/90">
                            <Building2Icon
                                aria-hidden="true"
                                className="size-3 shrink-0 text-muted-foreground"
                            />
                            <span className="min-w-0">
                                {activeOrder.customerName}
                            </span>
                        </span>
                        {activeOrder.projectName ? (
                            <>
                                <span aria-hidden="true">·</span>
                                <span className="wrap-anywhere">
                                    {activeOrder.projectName}
                                </span>
                            </>
                        ) : null}
                        {activeOrder.contractNo ? (
                            <span className="num inline-flex min-w-0 items-center gap-1">
                                <FileCheck2Icon
                                    aria-hidden="true"
                                    className="size-3 shrink-0"
                                />
                                <span className="min-w-0">
                                    {activeOrder.contractNo}
                                </span>
                            </span>
                        ) : (
                            <Badge variant="neutral">无合同</Badge>
                        )}
                        {activeOrder.settlementPartyName ? (
                            <>
                                <span aria-hidden="true">·</span>
                                <span className="wrap-anywhere">
                                    {activeOrder.settlementPartyName}
                                </span>
                            </>
                        ) : null}
                    </div>

                    {activeOrder.contractNotices.length > 0 ? (
                        <p className="flex items-start gap-1.5 text-xs text-warning-soft-foreground">
                            <AlertTriangleIcon
                                aria-hidden="true"
                                className="mt-0.5 size-3.5 shrink-0"
                            />
                            <span className="wrap-anywhere">
                                {activeOrder.contractNotices.join("；")}
                            </span>
                        </p>
                    ) : null}
                </header>

                {/* 仅当选中行不在下方销售明细覆盖列表中（如费用/未匹配行或无完整销售正文）时，在顶部兜底展示 */}
                {showTopSelectedPriceDetails && selectedPurchaseLine ? (
                    <section
                        id={selectedPriceDetailsId}
                        aria-labelledby={`${selectedPriceDetailsId}-title`}
                        className="space-y-2 rounded-md border border-border/60 bg-muted/20 p-3"
                    >
                        <h3
                            id={`${selectedPriceDetailsId}-title`}
                            className="text-body-compact font-semibold wrap-anywhere"
                        >
                            采购第 {selectedPurchaseLine.line_no} 行 ·{" "}
                            {selectedPurchaseLine.title}
                        </h3>
                        <PurchaseLinePriceDetails
                            line={selectedPurchaseLine}
                            comparison={purchaseComparison(
                                selectedPurchaseLine,
                            )}
                        />
                    </section>
                ) : null}

                {!activeOrder.hasFullOrder || !ledger ? (
                    <p className="border-t border-border/60 pt-3 text-xs text-muted-foreground">
                        未冻结完整来源销售正文，暂无法展开商务条款与覆盖对照。
                    </p>
                ) : (
                    <>
                        {/* 模块 1：商务风控（图标流向 + 异常聚焦 + 正常项收拢） */}
                        <CommercialChecksSection
                            idPrefix={idPrefix}
                            checks={activeOrder.commercialChecks}
                            businessRemark={activeOrder.businessRemark}
                        />

                        {/* 模块 2A：覆盖进度条 + 采销结构条 + 双口径利润卡 */}
                        <OrderLedgerSection
                            idPrefix={idPrefix}
                            ledger={ledger}
                        />

                        {/* 模块 2B：来源销售单全量明细覆盖清单 */}
                        <section
                            aria-label="销售明细覆盖清单"
                            className="space-y-2"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <h3 className="text-xs font-semibold text-foreground">
                                    明细覆盖
                                    <span className="num ml-1 font-normal text-muted-foreground">
                                        ({activeOrder.coverageRows.length})
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
                                            effectiveFilter === "all" ||
                                            undefined
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
                                                effectiveFilter ===
                                                    "attention" || undefined
                                            }
                                            className={
                                                detailWorkspaceStyles.subTab
                                            }
                                            onClick={() =>
                                                setFilter("attention")
                                            }
                                        >
                                            关注{" "}
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
                                                effectiveFilter ===
                                                    "uncovered" || undefined
                                            }
                                            className={
                                                detailWorkspaceStyles.subTab
                                            }
                                            onClick={() =>
                                                setFilter("uncovered")
                                            }
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
                                            className={
                                                detailWorkspaceStyles.subTab
                                            }
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
                                <ol className="space-y-2">
                                    {visibleRows.map((row) => {
                                        const isSelectedRow =
                                            row.salesLineId ===
                                            selectedSalesRowId
                                        return (
                                            <SalesCoverageLineItem
                                                key={row.salesLineId}
                                                idPrefix={idPrefix}
                                                row={row}
                                                selected={isSelectedRow}
                                                selectedPurchaseLine={
                                                    isSelectedRow
                                                        ? selectedPurchaseLine
                                                        : undefined
                                                }
                                                selectedPriceDetailsId={
                                                    isSelectedRow
                                                        ? selectedPriceDetailsId
                                                        : undefined
                                                }
                                                onLocatePurchaseLine={
                                                    onLocatePurchaseLine
                                                }
                                            />
                                        )
                                    })}
                                </ol>
                            ) : (
                                <p className="border-t border-border/60 pt-3 text-xs text-muted-foreground">
                                    当前筛选条件下暂无销售明细。
                                </p>
                            )}
                        </section>

                        {/* 本单费用行 / 未匹配行兜底 */}
                        {activeOrder.extraPurchaseLines.length > 0 ? (
                            <ExtraPurchaseLinesSection
                                idPrefix={idPrefix}
                                items={activeOrder.extraPurchaseLines}
                                onLocatePurchaseLine={onLocatePurchaseLine}
                            />
                        ) : null}
                    </>
                )}
            </div>
        </TooltipProvider>
    )
}

function CommercialChecksSection({
    idPrefix,
    checks,
    businessRemark,
}: {
    idPrefix: string
    checks: readonly CommercialComparisonItem[]
    businessRemark: string | null
}) {
    const [okExpanded, setOkExpanded] = useState(false)

    const sortedChecks = useMemo(
        () =>
            checks.toSorted(
                (left, right) =>
                    commercialSeverityRank[left.status] -
                    commercialSeverityRank[right.status],
            ),
        [checks],
    )
    const activeChecks = sortedChecks.filter((item) => item.status !== "ok")
    const okChecks = sortedChecks.filter((item) => item.status === "ok")
    const attentionCount = checks.filter(
        (item) => item.status === "severe" || item.status === "warning",
    ).length
    const hasSevere = checks.some((item) => item.status === "severe")

    return (
        <section aria-label="商务与风控对照" className="space-y-2">
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 className="text-xs font-semibold text-foreground">
                    商务风控
                </h3>
                <Badge
                    variant={
                        hasSevere
                            ? "destructive"
                            : attentionCount > 0
                              ? "warning"
                              : "success"
                    }
                >
                    {attentionCount > 0
                        ? `${attentionCount} 项关注`
                        : "条款匹配"}
                </Badge>
            </div>

            <div className="space-y-1.5">
                {activeChecks.length > 0 ? (
                    <>
                        <div
                            aria-hidden="true"
                            className="grid grid-cols-[3.25rem_minmax(0,1fr)_auto] items-center gap-2 px-2 text-xs text-muted-foreground"
                        >
                            <span>维度</span>
                            <span className="inline-flex items-center gap-1">
                                <span className="rounded-sm bg-muted/75 px-1 text-tiny font-medium">
                                    销
                                </span>
                                <ArrowRightIcon className="size-3" />
                                <span className="rounded-sm bg-muted/75 px-1 text-tiny font-medium">
                                    采
                                </span>
                            </span>
                            <span>状态</span>
                        </div>
                        <ol className="space-y-1.5">
                            {activeChecks.map((check) => (
                                <CommercialCheckCard
                                    key={check.id}
                                    idPrefix={idPrefix}
                                    check={check}
                                />
                            ))}
                        </ol>
                    </>
                ) : null}

                {okChecks.length > 0 ? (
                    <Collapsible open={okExpanded} onOpenChange={setOkExpanded}>
                        <CollapsibleTrigger
                            id={`${idPrefix}-commercial-ok-toggle`}
                            type="button"
                            className="flex w-full cursor-pointer items-center justify-between gap-2 rounded-md border border-border/60 bg-card px-2.5 py-1.5 text-left text-xs transition-colors hover:bg-muted/40"
                        >
                            <span className="flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-0.5">
                                <CheckIcon
                                    aria-hidden="true"
                                    className="size-3.5 shrink-0 text-success-soft-foreground"
                                />
                                <span className="font-medium text-foreground">
                                    {okChecks.length === checks.length
                                        ? "资金 · 票税 · 履约均匹配"
                                        : `已匹配：${okChecks
                                              .map((item) => item.shortTitle)
                                              .join(" · ")}`}
                                </span>
                                {okChecks.length === 1 ? (
                                    <span className="num truncate text-muted-foreground">
                                        {okChecks[0]!.compactSalesValue} →{" "}
                                        {okChecks[0]!.compactPurchaseValue}
                                    </span>
                                ) : null}
                            </span>
                            <ChevronDownIcon
                                aria-hidden="true"
                                className={cn(
                                    "size-3.5 shrink-0 text-muted-foreground transition-transform",
                                    okExpanded && "rotate-180",
                                )}
                            />
                        </CollapsibleTrigger>
                        <CollapsibleContent className="pt-1.5">
                            <ol className="space-y-1.5">
                                {okChecks.map((check) => (
                                    <CommercialCheckCard
                                        key={check.id}
                                        idPrefix={idPrefix}
                                        check={check}
                                    />
                                ))}
                            </ol>
                        </CollapsibleContent>
                    </Collapsible>
                ) : null}
            </div>

            {businessRemark ? (
                <div className="flex items-start gap-2 rounded-r-md border-l-2 border-info bg-info-soft/30 px-2.5 py-1.5 text-xs">
                    <MessageSquareTextIcon
                        aria-hidden="true"
                        className="mt-0.5 size-3.5 shrink-0 text-info-soft-foreground"
                    />
                    <div className="min-w-0 space-y-0.5">
                        <p className="font-medium text-info-soft-foreground">
                            销售备注
                        </p>
                        <p className="whitespace-pre-wrap wrap-anywhere text-foreground/90">
                            {businessRemark}
                        </p>
                    </div>
                </div>
            ) : null}
        </section>
    )
}

function CommercialCheckCard({
    idPrefix,
    check,
}: {
    idPrefix: string
    check: CommercialComparisonItem
}) {
    const [detailOpen, setDetailOpen] = useState(false)
    const Icon = commercialCheckIcons[check.id]
    const isSevere = check.status === "severe"
    const isWarning = check.status === "warning"
    const isInfo = check.status === "info"

    const DirectionIcon =
        check.direction === "down"
            ? TrendingDownIcon
            : check.direction === "up"
              ? TrendingUpIcon
              : ArrowRightIcon

    return (
        <li
            className={cn(
                "rounded-md border px-2.5 py-2 text-xs",
                isSevere &&
                    "border-destructive-border/70 border-l-2 border-l-destructive bg-destructive-soft/25",
                isWarning &&
                    "border-warning-border/70 border-l-2 border-l-warning bg-warning-soft/25",
                isInfo &&
                    "border-info-border/60 border-l-2 border-l-info bg-info-soft/20",
                !isSevere &&
                    !isWarning &&
                    !isInfo &&
                    "border-border/60 bg-muted/20",
            )}
        >
            <div className="grid grid-cols-[3.25rem_minmax(0,1fr)_auto] items-center gap-2">
                <span className="inline-flex items-center gap-1 font-medium text-foreground">
                    <Icon
                        aria-hidden="true"
                        className={cn(
                            "size-3.5 shrink-0",
                            isSevere
                                ? "text-destructive-soft-foreground"
                                : isWarning
                                  ? "text-warning-soft-foreground"
                                  : isInfo
                                    ? "text-info-soft-foreground"
                                    : "text-muted-foreground",
                        )}
                    />
                    {check.shortTitle}
                </span>

                <div className="num flex min-w-0 flex-wrap items-center gap-1 wrap-anywhere">
                    <span className="text-foreground/85">
                        {check.compactSalesValue}
                    </span>
                    <DirectionIcon
                        aria-hidden="true"
                        className={cn(
                            "size-3.5 shrink-0",
                            check.direction === "down"
                                ? isSevere
                                    ? "text-destructive-soft-foreground"
                                    : "text-warning-soft-foreground"
                                : check.direction === "up"
                                  ? "text-success-soft-foreground"
                                  : "text-muted-foreground",
                        )}
                    />
                    <span
                        className={cn(
                            "font-medium",
                            isSevere
                                ? "text-destructive-soft-foreground"
                                : "text-foreground",
                        )}
                    >
                        {check.compactPurchaseValue}
                    </span>
                </div>

                <div className="flex shrink-0 items-center gap-1">
                    <Badge variant={commercialBadgeVariant[check.status]}>
                        {check.badge}
                    </Badge>
                    <Tooltip>
                        <TooltipTrigger
                            render={
                                <button
                                    id={`${idPrefix}-commercial-detail-${check.id}`}
                                    type="button"
                                    aria-label={`${check.title}详细说明`}
                                    aria-expanded={detailOpen}
                                    onClick={() =>
                                        setDetailOpen((open) => !open)
                                    }
                                    className="inline-flex size-5 cursor-pointer items-center justify-center rounded-sm text-muted-foreground transition-colors hover:bg-muted/70 hover:text-foreground"
                                />
                            }
                        >
                            <InfoIcon aria-hidden="true" className="size-3.5" />
                        </TooltipTrigger>
                        <TooltipContent className="block">
                            {check.detail}
                        </TooltipContent>
                    </Tooltip>
                </div>
            </div>

            {check.metric ? (
                <p
                    className={cn(
                        "num mt-1.5 border-t pt-1",
                        isSevere
                            ? "border-destructive-border/40 font-medium text-destructive-soft-foreground"
                            : isWarning
                              ? "border-warning-border/40 text-warning-soft-foreground"
                              : "border-border/50 text-muted-foreground",
                    )}
                >
                    {check.metric}
                </p>
            ) : null}

            {detailOpen ? (
                <p className="mt-1.5 border-t border-border/50 pt-1 text-muted-foreground">
                    {check.detail}
                </p>
            ) : null}
        </li>
    )
}

function OrderLedgerSection({
    idPrefix,
    ledger,
}: {
    idPrefix: string
    ledger: SourceOrderBusinessLedger
}) {
    const hasSignificantTaxDrag = Boolean(
        ledger.taxDragPoints &&
        (() => {
            try {
                return compareDecimal(ledger.taxDragPoints, "0.5", 1) >= 0
            } catch {
                return false
            }
        })(),
    )
    const hasPositiveVatGap = Boolean(
        ledger.vatGap &&
        (() => {
            try {
                return compareDecimal(ledger.vatGap, "0", 2) > 0
            } catch {
                return false
            }
        })(),
    )

    const purchaseMarginLimited = Boolean(
        ledger.purchaseMargin &&
        (ledger.purchaseMargin.lineCount < ledger.purchaseGoodsLineCount ||
            ledger.purchaseFeeGrossTotal ||
            ledger.unallocatedSharedFeeTotal),
    )

    const externalMarginNegative = Boolean(
        ledger.taxInverted ||
        ledger.grossMargin?.negative ||
        ledger.netMargin?.negative,
    )

    const coverageSegments = [
        {
            key: "full",
            label: "全量",
            count: ledger.fullCount,
            barClass: "bg-success",
            dotClass: "bg-success",
            textClass: "text-muted-foreground",
        },
        {
            key: "partial",
            label: "部分",
            count: ledger.partialCount,
            barClass: "bg-info",
            dotClass: "bg-info",
            textClass: "text-muted-foreground",
        },
        {
            key: "excess",
            label: "超量",
            count: ledger.excessCount,
            barClass: "bg-destructive",
            dotClass: "bg-destructive",
            textClass: "font-medium text-destructive-soft-foreground",
        },
        {
            key: "mismatch",
            label: "待核",
            count: ledger.mismatchCount,
            barClass: "bg-warning",
            dotClass: "bg-warning",
            textClass: "font-medium text-warning-soft-foreground",
        },
        {
            key: "uncovered",
            label: "未含",
            count: ledger.uncoveredCount,
            barClass: "bg-muted-foreground/20",
            dotClass: "border border-muted-foreground/60 bg-transparent",
            textClass: "text-muted-foreground",
        },
    ].filter((segment) => segment.count > 0)

    return (
        <section aria-label="整单覆盖与生意账" className="space-y-2">
            <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex items-center gap-1">
                    <h3 className="text-xs font-semibold text-foreground">
                        覆盖与生意账
                    </h3>
                    {!ledger.isFullOrderCovered ? (
                        <Tooltip>
                            <TooltipTrigger
                                render={
                                    <button
                                        id={`${idPrefix}-ledger-scope-info`}
                                        type="button"
                                        aria-label="统计口径说明"
                                        className="inline-flex size-4 cursor-help items-center justify-center rounded-sm text-muted-foreground hover:text-foreground"
                                    />
                                }
                            >
                                <InfoIcon
                                    aria-hidden="true"
                                    className="size-3.5"
                                />
                            </TooltipTrigger>
                            <TooltipContent className="block">
                                基于本次审批冻结资料统计，不含同销售单下其他独立采购单或库存分配。
                            </TooltipContent>
                        </Tooltip>
                    ) : null}
                </div>
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
                            ? "全量覆盖"
                            : `覆盖 ${ledger.coveredLineCount}/${ledger.totalSalesLineCount} 行`}
                </Badge>
            </div>

            <div className="space-y-3 border-y border-border/60 py-2.5 text-xs">
                {ledger.isExpenseOrder ? (
                    <ExpenseOrderLedgerBody ledger={ledger} />
                ) : (
                    <>
                        {/* 1. 多段式覆盖进度条 + 色点图例 */}
                        {ledger.totalSalesLineCount > 0 ? (
                            <div className="space-y-1.5">
                                <div className="flex flex-wrap items-baseline justify-between gap-x-2 gap-y-0.5">
                                    <span className="text-muted-foreground">
                                        明细覆盖
                                    </span>
                                    <div className="num flex flex-wrap items-baseline gap-x-1 font-semibold text-foreground">
                                        <span>
                                            {ledger.coveredLineCount}/
                                            {ledger.totalSalesLineCount} 行
                                        </span>
                                        {ledger.salesCoveragePercent ? (
                                            <span className="font-normal text-muted-foreground">
                                                · 占额{" "}
                                                {ledger.salesCoveragePercent}%
                                            </span>
                                        ) : null}
                                    </div>
                                </div>
                                <div
                                    aria-hidden="true"
                                    className="flex h-2 w-full gap-0.5 overflow-hidden rounded-full bg-muted"
                                >
                                    {coverageSegments.map((segment) => (
                                        <div
                                            key={segment.key}
                                            className={cn(
                                                "h-full basis-0",
                                                segment.barClass,
                                            )}
                                            style={{ flexGrow: segment.count }}
                                        />
                                    ))}
                                </div>
                                <div className="num flex flex-wrap items-center gap-x-3 gap-y-1">
                                    {coverageSegments.map((segment) => (
                                        <span
                                            key={segment.key}
                                            className={cn(
                                                "inline-flex items-center gap-1",
                                                segment.textClass,
                                            )}
                                        >
                                            <span
                                                aria-hidden="true"
                                                className={cn(
                                                    "size-2 rounded-full",
                                                    segment.dotClass,
                                                )}
                                            />
                                            {segment.label} {segment.count}
                                        </span>
                                    ))}
                                </div>
                            </div>
                        ) : null}

                        {/* 2. 采/销金额结构条 */}
                        <div className="space-y-1.5">
                            <div className="num flex flex-wrap items-baseline justify-between gap-x-2 gap-y-0.5">
                                <span className="text-muted-foreground">
                                    本单采{" "}
                                    <MoneyValue
                                        display="exact"
                                        value={ledger.purchaseGoodsGrossTotal}
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
                                </span>
                                <div className="flex min-w-0 flex-wrap items-baseline gap-x-1 gap-y-0.5 text-muted-foreground">
                                    <span className="inline-flex min-w-0 items-baseline gap-1">
                                        <span className="shrink-0">
                                            {ledger.isFullOrderCovered
                                                ? "销售"
                                                : "对应销"}
                                        </span>
                                        <MoneyValue
                                            display="exact"
                                            value={
                                                ledger.correspondingSalesGrossTotal ??
                                                ledger.salesGrossTotal
                                            }
                                        />
                                    </span>
                                    {!ledger.isFullOrderCovered &&
                                    ledger.correspondingSalesGrossTotal ? (
                                        <span className="inline-flex min-w-0 items-baseline gap-1">
                                            <span className="shrink-0">
                                                / 整单
                                            </span>
                                            <MoneyValue
                                                display="exact"
                                                value={ledger.salesGrossTotal}
                                                inheritColor
                                                className="font-normal"
                                            />
                                        </span>
                                    ) : null}
                                </div>
                            </div>

                            {ledger.costBarPercent ? (
                                <div
                                    aria-hidden="true"
                                    className="flex h-1.5 w-full overflow-hidden rounded-full bg-muted"
                                >
                                    {ledger.grossMargin?.negative ? (
                                        <div className="h-full w-full bg-destructive" />
                                    ) : (
                                        <>
                                            {ledger.goodsCostBarPercent ? (
                                                <div
                                                    className="h-full bg-primary/75"
                                                    style={{
                                                        width: `${ledger.goodsCostBarPercent}%`,
                                                    }}
                                                />
                                            ) : null}
                                            {ledger.feeCostBarPercent ? (
                                                <div
                                                    className="h-full bg-warning"
                                                    style={{
                                                        width: `${ledger.feeCostBarPercent}%`,
                                                    }}
                                                />
                                            ) : null}
                                            <div className="h-full flex-1 bg-success/75" />
                                        </>
                                    )}
                                </div>
                            ) : null}
                        </div>

                        {/* 3. 双栏利润卡：采购毛利（供-采） vs 对外毛利（含税 → 税后） */}
                        <div className="grid grid-cols-1 gap-2 @xs/source-comparison:grid-cols-2">
                            <div className="min-w-0 space-y-0.5 rounded-md bg-muted/35 px-2.5 py-2">
                                <div className="flex items-center justify-between gap-1 text-muted-foreground">
                                    <span>采购毛利</span>
                                    {purchaseMarginLimited &&
                                    ledger.purchaseMargin ? (
                                        <Tooltip>
                                            <TooltipTrigger
                                                render={
                                                    <button
                                                        id={`${idPrefix}-ledger-purchase-margin-scope`}
                                                        type="button"
                                                        aria-label="采购毛利核算范围"
                                                        className="inline-flex size-4 cursor-help items-center justify-center rounded-sm hover:text-foreground"
                                                    />
                                                }
                                            >
                                                <InfoIcon
                                                    aria-hidden="true"
                                                    className="size-3"
                                                />
                                            </TooltipTrigger>
                                            <TooltipContent className="block">
                                                已核算{" "}
                                                {
                                                    ledger.purchaseMargin
                                                        .lineCount
                                                }
                                                /{ledger.purchaseGoodsLineCount}{" "}
                                                条采购明细
                                                {ledger.purchaseFeeGrossTotal ||
                                                ledger.unallocatedSharedFeeTotal
                                                    ? "，不含费用"
                                                    : ""}
                                                。
                                            </TooltipContent>
                                        </Tooltip>
                                    ) : null}
                                </div>
                                {ledger.purchaseMargin ? (
                                    <>
                                        <div className="text-body-compact">
                                            <PurchaseProfitValue
                                                amount={
                                                    ledger.purchaseMargin.margin
                                                }
                                            />
                                        </div>
                                        <p className="num text-muted-foreground">
                                            供{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    ledger.purchaseMargin.supply
                                                }
                                                inheritColor
                                                className="font-normal"
                                            />
                                        </p>
                                    </>
                                ) : (
                                    <p className="text-muted-foreground">
                                        暂无法核算
                                    </p>
                                )}
                            </div>

                            <div
                                className={cn(
                                    "min-w-0 space-y-0.5 rounded-md border px-2.5 py-2",
                                    externalMarginNegative
                                        ? "border-destructive-border/60 bg-destructive-soft/25"
                                        : hasSignificantTaxDrag
                                          ? "border-warning-border/60 bg-warning-soft/25"
                                          : "border-transparent bg-muted/35",
                                )}
                            >
                                <div className="flex flex-wrap items-baseline justify-between gap-1">
                                    <span className="text-muted-foreground">
                                        对外毛利
                                    </span>
                                    {ledger.taxInverted ? (
                                        <span className="font-medium text-destructive-soft-foreground">
                                            税后倒挂
                                        </span>
                                    ) : hasSignificantTaxDrag &&
                                      ledger.taxDragPoints ? (
                                        <span className="num font-medium text-warning-soft-foreground">
                                            -{ledger.taxDragPoints}pp
                                        </span>
                                    ) : null}
                                </div>

                                {ledger.grossMargin ? (
                                    <Tooltip>
                                        <TooltipTrigger
                                            render={
                                                <div className="num flex w-fit cursor-help flex-wrap items-center gap-1 text-body-compact font-semibold" />
                                            }
                                        >
                                            <span
                                                className={cn(
                                                    ledger.grossMargin.negative
                                                        ? "text-destructive-soft-foreground"
                                                        : "text-foreground",
                                                )}
                                            >
                                                {ledger.grossMargin.negative
                                                    ? `-${ledger.grossMargin.absPercent}%`
                                                    : `${ledger.grossMargin.percent}%`}
                                            </span>
                                            {ledger.netMargin ? (
                                                <>
                                                    <ArrowRightIcon
                                                        aria-hidden="true"
                                                        className={cn(
                                                            "size-3 shrink-0",
                                                            externalMarginNegative
                                                                ? "text-destructive-soft-foreground"
                                                                : hasSignificantTaxDrag
                                                                  ? "text-warning-soft-foreground"
                                                                  : "text-muted-foreground",
                                                        )}
                                                    />
                                                    <span
                                                        className={cn(
                                                            ledger.netMargin
                                                                .negative
                                                                ? "text-destructive-soft-foreground"
                                                                : hasSignificantTaxDrag
                                                                  ? "text-warning-soft-foreground"
                                                                  : "text-foreground",
                                                        )}
                                                    >
                                                        {ledger.netMargin
                                                            .negative
                                                            ? `-${ledger.netMargin.absPercent}%`
                                                            : `${ledger.netMargin.percent}%`}
                                                    </span>
                                                </>
                                            ) : null}
                                        </TooltipTrigger>
                                        <TooltipContent className="num block space-y-0.5">
                                            <div>
                                                对外含税毛利：
                                                {ledger.grossMargin.negative
                                                    ? "亏 "
                                                    : ""}
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        ledger.grossMargin
                                                            .absAmount
                                                    }
                                                    inheritColor
                                                />{" "}
                                                ({ledger.grossMargin.percent}%)
                                            </div>
                                            {ledger.netMargin ? (
                                                <div>
                                                    对外不含税毛利：
                                                    {ledger.netMargin.negative
                                                        ? "亏 "
                                                        : ""}
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            ledger.netMargin
                                                                .absAmount
                                                        }
                                                        inheritColor
                                                    />{" "}
                                                    ({ledger.netMargin.percent}
                                                    %)
                                                </div>
                                            ) : null}
                                            {ledger.correspondingSalesNetTotal ? (
                                                <div>
                                                    对应不含税销售：
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            ledger.correspondingSalesNetTotal
                                                        }
                                                        inheritColor
                                                    />
                                                </div>
                                            ) : null}
                                        </TooltipContent>
                                    </Tooltip>
                                ) : (
                                    <p className="text-body-compact text-muted-foreground">
                                        —
                                    </p>
                                )}

                                <p className="num text-muted-foreground">
                                    {ledger.netMargin ? (
                                        <span
                                            className={cn(
                                                ledger.netMargin.negative &&
                                                    "font-medium text-destructive-soft-foreground",
                                            )}
                                        >
                                            税后{" "}
                                            {ledger.netMargin.negative
                                                ? "亏 "
                                                : ""}
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    ledger.netMargin.absAmount
                                                }
                                                inheritColor
                                                className="font-normal"
                                            />
                                        </span>
                                    ) : ledger.grossMargin ? (
                                        <span>
                                            含税{" "}
                                            {ledger.grossMargin.negative
                                                ? "亏 "
                                                : ""}
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    ledger.grossMargin.absAmount
                                                }
                                                inheritColor
                                                className="font-normal"
                                            />{" "}
                                            · 待补税率
                                        </span>
                                    ) : (
                                        "暂无可比销售"
                                    )}
                                    {hasPositiveVatGap && ledger.vatGap ? (
                                        <span
                                            className={cn(
                                                hasSignificantTaxDrag &&
                                                    !ledger.netMargin
                                                        ?.negative &&
                                                    "text-warning-soft-foreground",
                                            )}
                                        >
                                            {" "}
                                            · 税差{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={ledger.vatGap}
                                                inheritColor
                                                className="font-normal"
                                            />
                                        </span>
                                    ) : null}
                                </p>
                            </div>
                        </div>

                        {ledger.unallocatedSharedFeeTotal ? (
                            <div className="flex flex-wrap items-center gap-1.5">
                                <Tooltip>
                                    <TooltipTrigger
                                        render={
                                            <button
                                                id={`${idPrefix}-ledger-shared-fee-info`}
                                                type="button"
                                                aria-label="跨单费用未分摊说明"
                                                className="num inline-flex cursor-help items-center gap-1 rounded-full bg-muted px-2 py-0.5 text-xs text-muted-foreground"
                                            />
                                        }
                                    >
                                        <InfoIcon
                                            aria-hidden="true"
                                            className="size-3 shrink-0"
                                        />
                                        跨单费{" "}
                                        <MoneyValue
                                            display="exact"
                                            value={
                                                ledger.unallocatedSharedFeeTotal
                                            }
                                            inheritColor
                                            className="font-normal"
                                        />{" "}
                                        未分摊
                                    </TooltipTrigger>
                                    <TooltipContent className="block">
                                        本采购单含跨单费用行，未分摊计入单张销售单对外毛利。
                                    </TooltipContent>
                                </Tooltip>
                            </div>
                        ) : null}
                    </>
                )}
            </div>
        </section>
    )
}

function ExpenseOrderLedgerBody({
    ledger,
}: {
    ledger: SourceOrderBusinessLedger
}) {
    return (
        <div className="space-y-2.5">
            <div className="space-y-1.5">
                <div className="num flex flex-wrap items-baseline justify-between gap-2">
                    <span className="text-muted-foreground">
                        本单费用{" "}
                        <MoneyValue
                            display="exact"
                            value={ledger.purchaseFeeGrossTotal}
                        />
                        {ledger.purchaseFeeRatioPercent ? (
                            <span className="ml-1 font-medium text-foreground">
                                ({ledger.purchaseFeeRatioPercent}%)
                            </span>
                        ) : null}
                    </span>
                    <span className="text-muted-foreground">
                        销售 {ledger.totalSalesLineCount} 项 ·{" "}
                        <MoneyValue
                            display="exact"
                            value={ledger.salesGrossTotal}
                            inheritColor
                            className="font-normal"
                        />
                    </span>
                </div>
                {ledger.expenseFeeBarPercent ? (
                    <div
                        aria-hidden="true"
                        className="flex h-1.5 w-full overflow-hidden rounded-full bg-muted"
                    >
                        <div
                            className="h-full bg-warning"
                            style={{ width: `${ledger.expenseFeeBarPercent}%` }}
                        />
                    </div>
                ) : null}
            </div>

            {ledger.extraCostSummary ? (
                <div className="grid grid-cols-1 gap-2 @xs/source-comparison:grid-cols-2">
                    <div className="min-w-0 space-y-0.5 rounded-md bg-muted/35 px-2.5 py-2">
                        <p className="text-muted-foreground">已有附加费</p>
                        <p className="num text-body-compact font-semibold">
                            <MoneyValue
                                display="exact"
                                value={
                                    ledger.extraCostSummary.occurredGrossTotal
                                }
                            />
                        </p>
                        <p className="num text-muted-foreground">
                            待发生{" "}
                            <MoneyValue
                                display="exact"
                                value={
                                    ledger.extraCostSummary.pendingGrossTotal
                                }
                                inheritColor
                                className="font-normal"
                            />
                        </p>
                    </div>
                    <div className="min-w-0 space-y-0.5 rounded-md bg-muted/35 px-2.5 py-2">
                        <p className="text-muted-foreground">含本单累计</p>
                        <p className="num text-body-compact font-semibold">
                            <MoneyValue
                                display="exact"
                                value={
                                    ledger.extraCostSummary
                                        .combinedWithCurrentFeeTotal
                                }
                            />
                        </p>
                        <p className="num text-muted-foreground">
                            {ledger.extraCostSummary.combinedRatioPercent
                                ? `占销售 ${ledger.extraCostSummary.combinedRatioPercent}%`
                                : "—"}
                        </p>
                    </div>
                </div>
            ) : null}
        </div>
    )
}

function SalesCoverageLineItem({
    idPrefix,
    row,
    selected,
    selectedPurchaseLine,
    selectedPriceDetailsId,
    onLocatePurchaseLine,
}: {
    idPrefix: string
    row: SalesLineCoverageRow
    selected: boolean
    selectedPurchaseLine?: ApprovalPurchaseLine
    selectedPriceDetailsId?: string
    onLocatePurchaseLine?: (purchaseLineId: string) => void
}) {
    const [manualOpen, setManualOpen] = useState<boolean | null>(null)
    const selectedLineKey = selected
        ? (selectedPurchaseLine?.id ?? "selected")
        : null
    const [prevSelectedKey, setPrevSelectedKey] = useState(selectedLineKey)
    if (prevSelectedKey !== selectedLineKey) {
        setPrevSelectedKey(selectedLineKey)
        setManualOpen(null)
    }

    const isOpen = manualOpen ?? selected
    const lineSegment = toAutomationIdSegment(row.salesLineId)
    const status = coverageStatusMeta[row.coverageStatus]
    const isUncovered = row.coverageStatus === "uncovered"
    const unit = row.unitLabel ?? ""
    const singlePurchase =
        row.matchedPurchaseLines.length === 1
            ? row.matchedPurchaseLines[0]
            : undefined

    // 「超需求」「晚 N 天」「进项低 X%」已由进度条、日历标签、税率符号直接呈现，此处只保留额外高危/关键风险标签避免重复。
    const extraAttentionTags = row.attentionTags.filter(
        (tag) =>
            tag.label !== "超需求" &&
            !tag.label.startsWith("晚 ") &&
            !tag.label.startsWith("进项低 "),
    )

    const leftAccentClass =
        row.severity === "severe"
            ? "border-l-destructive"
            : row.severity === "warning"
              ? "border-l-warning"
              : row.coverageStatus === "full"
                ? "border-l-success"
                : row.coverageStatus === "partial"
                  ? "border-l-info"
                  : "border-l-border"

    const qtyBarClass =
        row.coverageStatus === "excess"
            ? "bg-destructive"
            : row.coverageStatus === "full"
              ? "bg-success"
              : "bg-info"

    const hasPriceChain =
        singlePurchase?.unitCostGross != null ||
        row.supplyPrice != null ||
        row.salesUnitPriceGross != null

    const hasExpandableDetails = Boolean(
        (selected && selectedPurchaseLine) ||
        row.purchaseMargin ||
        row.purchaseGrossTotal ||
        row.originalSupplyPrice ||
        row.duplicateGroupSize > 1 ||
        row.matchedPurchaseLines.length > 1,
    )

    const compactExpectedDate = formatCompactDate(
        singlePurchase?.expectedDeliveryDate,
        row.salesDueDate,
    )
    const compactDueDate = formatCompactDate(
        row.salesDueDate,
        singlePurchase?.expectedDeliveryDate,
    )

    return (
        <li
            id={`${idPrefix}-sales-line-${lineSegment}`}
            className={cn(
                "space-y-2 rounded-md border border-border/60 border-l-2 bg-card px-2.5 py-2 text-xs",
                leftAccentClass,
                selected &&
                    "bg-info-soft/25 ring-1 ring-inset ring-info-border",
                isUncovered && !selected && "opacity-75",
            )}
        >
            {/* 行头：序号 + 品名 + 定位捷径 + 状态 */}
            <div className="flex items-start justify-between gap-2">
                <div className="min-w-0 flex-1 space-y-0.5">
                    <div className="flex flex-wrap items-center gap-1.5">
                        <span className="num text-muted-foreground">
                            #{row.salesLineNo}
                        </span>
                        <span className="text-body-compact font-medium wrap-anywhere text-foreground">
                            {row.title}
                        </span>
                    </div>
                    {row.specification || row.welfareLabel ? (
                        <p className="wrap-anywhere text-muted-foreground">
                            {[row.specification, row.welfareLabel]
                                .filter(Boolean)
                                .join(" · ")}
                        </p>
                    ) : null}
                </div>

                <div className="flex shrink-0 items-center gap-1">
                    {onLocatePurchaseLine && row.matchedPurchaseLines.length > 0
                        ? row.matchedPurchaseLines.map((pLine) => (
                              <Button
                                  key={pLine.id}
                                  id={`${idPrefix}-locate-${toAutomationIdSegment(pLine.id)}`}
                                  type="button"
                                  variant="ghost"
                                  size="xs"
                                  aria-label={`定位采购第 ${pLine.lineNo} 行`}
                                  className={cn(
                                      detailWorkspaceStyles.inlineLinkButton,
                                      "h-5.5 gap-1 px-1.5 text-xs",
                                  )}
                                  onClick={() => onLocatePurchaseLine(pLine.id)}
                              >
                                  <CrosshairIcon
                                      aria-hidden="true"
                                      className="size-3"
                                  />
                                  <span className="num">采#{pLine.lineNo}</span>
                              </Button>
                          ))
                        : null}
                    <Badge variant={status.variant}>{status.label}</Badge>
                </div>
            </div>

            {isUncovered ? (
                <div className="num flex flex-wrap items-center gap-x-2.5 gap-y-0.5 text-muted-foreground">
                    {row.salesQuantity ? (
                        <QuantityValue
                            value={row.salesQuantity}
                            unit={unit || undefined}
                        />
                    ) : null}
                    <MoneyValue
                        display="exact"
                        value={row.salesGrossAmount}
                        inheritColor
                        className="font-normal"
                    />
                    <span>销项 {row.salesTaxRatePercent}%</span>
                    {row.salesDueDate ? (
                        <span className="inline-flex items-center gap-1">
                            <CalendarIcon
                                aria-hidden="true"
                                className="size-3"
                            />
                            {row.salesDueDate}
                        </span>
                    ) : null}
                </div>
            ) : (
                <>
                    {/* 去重后的关键风险标签 */}
                    {extraAttentionTags.length > 0 ? (
                        <div className="flex flex-wrap items-center gap-1">
                            {extraAttentionTags.map((tag) => (
                                <Badge key={tag.label} variant={tag.tone}>
                                    {tag.label}
                                </Badge>
                            ))}
                        </div>
                    ) : null}

                    {/* 数量进度条 + 日历交期 */}
                    <div className="num flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
                        <div className="flex min-w-0 items-center gap-1.5">
                            {row.coverageRatioPercent != null ? (
                                <div
                                    aria-hidden="true"
                                    className="h-1.5 w-14 shrink-0 overflow-hidden rounded-full bg-muted"
                                >
                                    <div
                                        className={cn("h-full", qtyBarClass)}
                                        style={{
                                            width: `${row.coverageRatioPercent}%`,
                                        }}
                                    />
                                </div>
                            ) : null}
                            <span className="text-muted-foreground">
                                <span className="font-medium text-foreground">
                                    {row.purchasedQuantityTotal ?? "待核"}
                                </span>
                                {row.salesQuantity
                                    ? `/${row.salesQuantity}`
                                    : ""}
                                {unit ? ` ${unit}` : ""}
                            </span>
                            {row.excessQuantity ? (
                                <span className="rounded-sm bg-destructive-soft px-1 font-medium text-destructive-soft-foreground">
                                    +{row.excessQuantity}
                                </span>
                            ) : row.remainingQuantity ? (
                                <span className="text-muted-foreground">
                                    余 {row.remainingQuantity}
                                </span>
                            ) : null}
                            {row.matchedPurchaseLines.length > 1 ? (
                                <span className="text-muted-foreground">
                                    · {row.matchedPurchaseLines.length} 笔
                                </span>
                            ) : null}
                        </div>

                        {singlePurchase?.expectedDeliveryDate ||
                        row.salesDueDate ? (
                            <Tooltip>
                                <TooltipTrigger
                                    render={
                                        <span
                                            className={cn(
                                                "inline-flex shrink-0 cursor-help items-center gap-1",
                                                row.maxDelayDays > 0
                                                    ? "text-warning-soft-foreground"
                                                    : "text-muted-foreground",
                                            )}
                                        />
                                    }
                                >
                                    <CalendarIcon
                                        aria-hidden="true"
                                        className="size-3 shrink-0"
                                    />
                                    <span>
                                        {compactExpectedDate || "多笔"}
                                        {row.maxDelayDays === 0 &&
                                        compactDueDate
                                            ? ` / ${compactDueDate}`
                                            : ""}
                                    </span>
                                    {row.maxDelayDays > 0 ? (
                                        <span className="rounded-sm bg-warning-soft px-1 font-medium text-warning-soft-foreground">
                                            晚{row.maxDelayDays}天
                                        </span>
                                    ) : null}
                                </TooltipTrigger>
                                <TooltipContent className="block">
                                    预计交期：
                                    {singlePurchase?.expectedDeliveryDate ??
                                        "多笔分批"}
                                    {row.salesDueDate
                                        ? ` · 销售承诺：${row.salesDueDate}`
                                        : ""}
                                </TooltipContent>
                            </Tooltip>
                        ) : null}
                    </div>

                    {/* 三价阶梯条：采购价 → 供给价 → 对外价 */}
                    {hasPriceChain ? (
                        <div className="num grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-1 rounded-md bg-muted/35 px-2 py-1.5 text-center">
                            <div className="min-w-0">
                                <div className="text-muted-foreground">
                                    采购价
                                </div>
                                <div
                                    className={cn(
                                        "truncate font-medium",
                                        row.costAboveSupply
                                            ? "text-warning-soft-foreground"
                                            : "text-foreground",
                                    )}
                                >
                                    {singlePurchase?.unitCostGross != null ? (
                                        <MoneyValue
                                            display="exact"
                                            value={singlePurchase.unitCostGross}
                                            inheritColor
                                        />
                                    ) : row.matchedPurchaseLines.length > 1 ? (
                                        "多笔"
                                    ) : (
                                        "—"
                                    )}
                                </div>
                            </div>

                            <ArrowRightIcon
                                aria-hidden="true"
                                className="size-3 shrink-0 text-muted-foreground/70"
                            />

                            <div className="min-w-0">
                                <div className="inline-flex items-center justify-center gap-0.5 text-muted-foreground">
                                    <span>供给价</span>
                                    {row.priceBasisLabel ? (
                                        <span className="rounded-sm bg-muted px-1 text-tiny">
                                            {row.priceBasisLabel === "一件代发"
                                                ? "代发"
                                                : row.priceBasisLabel}
                                        </span>
                                    ) : null}
                                    {row.originalSupplyPrice &&
                                    (row.supplyAdjustment === "increased" ||
                                        row.supplyAdjustment ===
                                            "decreased") ? (
                                        <Tooltip>
                                            <TooltipTrigger
                                                render={
                                                    <span className="inline-flex cursor-help items-center text-info-soft-foreground" />
                                                }
                                            >
                                                {row.supplyAdjustment ===
                                                "increased" ? (
                                                    <ArrowUpIcon
                                                        aria-hidden="true"
                                                        className="size-3"
                                                    />
                                                ) : (
                                                    <ArrowDownIcon
                                                        aria-hidden="true"
                                                        className="size-3"
                                                    />
                                                )}
                                            </TooltipTrigger>
                                            <TooltipContent className="block">
                                                原参考价{" "}
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        row.originalSupplyPrice
                                                    }
                                                    inheritColor
                                                />
                                            </TooltipContent>
                                        </Tooltip>
                                    ) : null}
                                </div>
                                <div className="truncate font-medium text-foreground">
                                    {row.supplyPrice ? (
                                        <MoneyValue
                                            display="exact"
                                            value={row.supplyPrice}
                                            inheritColor
                                        />
                                    ) : (
                                        "—"
                                    )}
                                </div>
                            </div>

                            <ArrowRightIcon
                                aria-hidden="true"
                                className="size-3 shrink-0 text-muted-foreground/70"
                            />

                            <div className="min-w-0">
                                <div className="text-muted-foreground">
                                    对外价
                                </div>
                                <div
                                    className={cn(
                                        "truncate font-medium",
                                        row.grossMargin?.negative
                                            ? "text-destructive-soft-foreground"
                                            : "text-foreground",
                                    )}
                                >
                                    <MoneyValue
                                        display="exact"
                                        value={row.salesUnitPriceGross ?? "0"}
                                        inheritColor
                                    />
                                </div>
                            </div>
                        </div>
                    ) : null}

                    {/* 票税 + 双毛利 + 可折叠金额核算 */}
                    <Collapsible
                        open={isOpen}
                        onOpenChange={(nextOpen) => setManualOpen(nextOpen)}
                    >
                        <div className="num flex flex-wrap items-center justify-between gap-2">
                            <div className="flex flex-wrap items-center gap-x-2.5 gap-y-0.5">
                                <Tooltip>
                                    <TooltipTrigger
                                        render={
                                            <span
                                                className={cn(
                                                    "inline-flex cursor-help items-center gap-0.5",
                                                    row.lowerInputTaxGapPoints !=
                                                        null
                                                        ? "font-medium text-warning-soft-foreground"
                                                        : "text-muted-foreground",
                                                )}
                                            />
                                        }
                                    >
                                        <span>
                                            税 {row.salesTaxRatePercent}%
                                        </span>
                                        {row.lowerInputTaxGapPoints != null ? (
                                            <TrendingDownIcon
                                                aria-hidden="true"
                                                className="size-3 shrink-0"
                                            />
                                        ) : (
                                            <ArrowRightIcon
                                                aria-hidden="true"
                                                className="size-3 shrink-0"
                                            />
                                        )}
                                        <span>
                                            {row.inputTaxRatesPercent.length > 0
                                                ? row.inputTaxRatesPercent
                                                      .map((rate) => `${rate}%`)
                                                      .join("/")
                                                : "未填"}
                                        </span>
                                    </TooltipTrigger>
                                    <TooltipContent className="block">
                                        销项 {row.salesTaxRatePercent}% vs 进项{" "}
                                        {row.inputTaxRatesPercent.length > 0
                                            ? row.inputTaxRatesPercent
                                                  .map((rate) => `${rate}%`)
                                                  .join("/")
                                            : "未填"}
                                        {row.lowerInputTaxGapPoints != null
                                            ? `（进项低 ${row.lowerInputTaxGapPoints}%）`
                                            : ""}
                                    </TooltipContent>
                                </Tooltip>

                                {row.grossMargin || row.netMargin ? (
                                    <span className="inline-flex items-center gap-1 text-muted-foreground">
                                        <span>毛利</span>
                                        {row.grossMargin ? (
                                            <span
                                                className={cn(
                                                    row.grossMargin.negative
                                                        ? "font-medium text-destructive-soft-foreground"
                                                        : "text-foreground/90",
                                                )}
                                            >
                                                {row.grossMargin.negative
                                                    ? `-${row.grossMargin.absPercent}%`
                                                    : `${row.grossMargin.percent}%`}
                                            </span>
                                        ) : null}
                                        {row.grossMargin && row.netMargin ? (
                                            <ArrowRightIcon
                                                aria-hidden="true"
                                                className="size-3 shrink-0"
                                            />
                                        ) : null}
                                        {row.netMargin ? (
                                            <span
                                                className={cn(
                                                    row.netMargin.negative
                                                        ? "font-semibold text-destructive-soft-foreground"
                                                        : row.lowerInputTaxGapPoints !=
                                                            null
                                                          ? "font-medium text-warning-soft-foreground"
                                                          : "font-medium text-foreground",
                                                )}
                                            >
                                                {row.netMargin.negative
                                                    ? `-${row.netMargin.absPercent}%`
                                                    : `${row.netMargin.percent}%`}
                                            </span>
                                        ) : null}
                                    </span>
                                ) : null}
                            </div>

                            {hasExpandableDetails ? (
                                <CollapsibleTrigger
                                    id={`${idPrefix}-sales-line-${lineSegment}-toggle`}
                                    type="button"
                                    className="inline-flex shrink-0 cursor-pointer items-center gap-0.5 text-primary hover:underline"
                                >
                                    <span>核算</span>
                                    <ChevronDownIcon
                                        aria-hidden="true"
                                        className={cn(
                                            "size-3.5 transition-transform",
                                            isOpen && "rotate-180",
                                        )}
                                    />
                                </CollapsibleTrigger>
                            ) : null}
                        </div>

                        {/* 品规差异证据保持直出，避免关键风险被折叠 */}
                        {row.mismatchedProduct ? (
                            <p className="mt-1.5 flex items-start gap-1 wrap-anywhere text-destructive-soft-foreground">
                                <AlertTriangleIcon
                                    aria-hidden="true"
                                    className="mt-0.5 size-3.5 shrink-0"
                                />
                                <span>
                                    采购录入「
                                    {row.matchedPurchaseLines
                                        .map((line) =>
                                            [line.title, line.specification]
                                                .filter(Boolean)
                                                .join(" · "),
                                        )
                                        .join(" / ")}
                                    」
                                </span>
                            </p>
                        ) : null}

                        {hasExpandableDetails ? (
                            <CollapsibleContent className="mt-2 space-y-2 border-t border-border/60 pt-2">
                                {selected && selectedPurchaseLine ? (
                                    <div
                                        id={selectedPriceDetailsId}
                                        className="space-y-1.5"
                                    >
                                        <p className="font-medium text-foreground">
                                            采购第{" "}
                                            {selectedPurchaseLine.line_no}{" "}
                                            行价格核算
                                        </p>
                                        <PurchaseLinePriceDetails
                                            line={selectedPurchaseLine}
                                            comparison={purchaseComparison(
                                                selectedPurchaseLine,
                                            )}
                                        />
                                    </div>
                                ) : row.purchaseMargin ? (
                                    <div className="num grid grid-cols-3 gap-2">
                                        <div className="min-w-0">
                                            <div className="text-muted-foreground">
                                                供给额
                                            </div>
                                            <div className="font-medium text-foreground">
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        row.purchaseMargin
                                                            .supply
                                                    }
                                                />
                                            </div>
                                        </div>
                                        <div className="min-w-0">
                                            <div className="text-muted-foreground">
                                                采购额
                                            </div>
                                            <div className="font-medium text-foreground">
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        row.purchaseMargin.cost
                                                    }
                                                />
                                            </div>
                                        </div>
                                        <div className="min-w-0">
                                            <div className="text-muted-foreground">
                                                采购毛利
                                            </div>
                                            <PurchaseProfitValue
                                                amount={
                                                    row.purchaseMargin.margin
                                                }
                                            />
                                        </div>
                                    </div>
                                ) : row.purchaseGrossTotal ? (
                                    <div className="num flex items-center justify-between text-muted-foreground">
                                        <span>实际采购额</span>
                                        <MoneyValue
                                            display="exact"
                                            value={row.purchaseGrossTotal}
                                        />
                                    </div>
                                ) : null}

                                {row.matchedPurchaseLines.length > 1 ? (
                                    <ul className="num space-y-1 border-t border-border/50 pt-1.5 text-muted-foreground">
                                        {row.matchedPurchaseLines.map(
                                            (pLine) => (
                                                <li
                                                    key={pLine.id}
                                                    className="flex flex-wrap items-center justify-between gap-2"
                                                >
                                                    <span>
                                                        采#{pLine.lineNo} ·{" "}
                                                        {pLine.quantity ?? "—"}
                                                        {unit ? ` ${unit}` : ""}
                                                        {pLine.expectedDeliveryDate
                                                            ? ` · ${formatCompactDate(pLine.expectedDeliveryDate, row.salesDueDate)}`
                                                            : ""}
                                                    </span>
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            pLine.grossAmount
                                                        }
                                                        inheritColor
                                                        className="font-normal"
                                                    />
                                                </li>
                                            ),
                                        )}
                                    </ul>
                                ) : null}

                                {row.originalSupplyPrice &&
                                (row.supplyAdjustment === "increased" ||
                                    row.supplyAdjustment === "decreased") ? (
                                    <p className="num text-muted-foreground">
                                        原采购参考价{" "}
                                        <MoneyValue
                                            display="exact"
                                            value={row.originalSupplyPrice}
                                            inheritColor
                                            className="font-normal"
                                        />
                                        （已
                                        {row.supplyAdjustment === "increased"
                                            ? "调高"
                                            : "调低"}
                                        ）
                                    </p>
                                ) : null}

                                {row.duplicateGroupSize > 1 ? (
                                    <p className="num text-muted-foreground">
                                        销售单含 {row.duplicateGroupSize}{" "}
                                        项同品规单价明细，请核验汇总数量。
                                    </p>
                                ) : null}
                            </CollapsibleContent>
                        ) : null}
                    </Collapsible>
                </>
            )}
        </li>
    )
}

function ExtraPurchaseLinesSection({
    idPrefix,
    items,
    onLocatePurchaseLine,
}: {
    idPrefix: string
    items: readonly ExtraPurchaseLineItem[]
    onLocatePurchaseLine?: (purchaseLineId: string) => void
}) {
    return (
        <section aria-label="本单费用与其他采购行" className="space-y-1.5">
            <h3 className="text-xs font-semibold text-foreground">
                附加费用与未匹配项
                <span className="num ml-1 font-normal text-muted-foreground">
                    ({items.length})
                </span>
            </h3>
            <ul className="divide-y divide-border/60 border-y border-border/60">
                {items.map((extra) => (
                    <li
                        key={extra.id}
                        className="flex flex-wrap items-center justify-between gap-2 py-2 text-xs"
                    >
                        <div className="min-w-0 space-y-0.5">
                            <div className="flex flex-wrap items-center gap-1.5">
                                <span className="num text-muted-foreground">
                                    采#{extra.lineNo}
                                </span>
                                <Badge
                                    variant={
                                        extra.isFee ? "neutral" : "warning"
                                    }
                                >
                                    {extra.isFee ? "费用" : "未匹配"}
                                </Badge>
                                <span className="font-medium wrap-anywhere text-foreground">
                                    {extra.title}
                                </span>
                            </div>
                            <p className="num text-muted-foreground">
                                <MoneyValue
                                    display="exact"
                                    value={extra.grossAmount}
                                />
                                {extra.inputTaxRatePercent != null
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
                                aria-label={`定位采购第 ${extra.lineNo} 行`}
                                className={cn(
                                    detailWorkspaceStyles.inlineLinkButton,
                                    "h-5.5 gap-1 px-1.5 text-xs",
                                )}
                                onClick={() => onLocatePurchaseLine(extra.id)}
                            >
                                <CrosshairIcon
                                    aria-hidden="true"
                                    className="size-3"
                                />
                                定位
                            </Button>
                        ) : null}
                    </li>
                ))}
            </ul>
        </section>
    )
}
