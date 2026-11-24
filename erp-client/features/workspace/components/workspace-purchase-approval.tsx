"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import {
    Columns2Icon,
    FileTextIcon,
    InfoIcon,
    PaperclipIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DeliveryAllocationOverview,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspaceSection,
    MoneyValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import type { ApprovalPurchaseFulfillment } from "@/features/approval-workflow/api/materials"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import type { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    buildDeliveryAllocationSummary,
    type DeliveryAllocationLineInput,
} from "@/lib/delivery-allocation-summary"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { cn } from "@/lib/utils"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceApprovalResubmissionBanner } from "./workspace-approval-resubmission-banner"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"
import {
    buildPurchaseApprovalMetrics,
    evaluatePurchaseLines,
    parsePurchaseExtraSections,
    purchaseApprovalRiskReasons,
    purchaseSpecification,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"
import {
    WorkspacePurchaseComparisonTable,
    type PurchaseComparisonTableHandle,
} from "./workspace-purchase-comparison-table"
import { WorkspaceSupplierRating } from "./workspace-supplier-rating"

export { purchaseApprovalRiskReasons }

function purchaseApprovalPrefix(instanceId: string) {
    return `workspace-purchase-approval-${toAutomationIdSegment(instanceId)}`
}

function fulfillmentSummaryLabel(
    fulfillment?: ApprovalPurchaseFulfillment | null,
): string | null {
    if (!fulfillment) return null
    switch (fulfillment.responsibility) {
        case "WAREHOUSE":
            return fulfillment.target_warehouse_name
                ? `入仓 · ${fulfillment.target_warehouse_name}`
                : "采购入库"
        case "SUPPLIER_DIRECT":
            return "供应商直发"
        case "ELECTRONIC":
            return "电子交付"
        case "SERVICE":
            return "线下服务"
        case "NONE":
            return "费用单（无需履约）"
        default:
            return null
    }
}

export function WorkspacePurchaseApprovalSources({
    instanceId,
    sources,
    availableCompareRevisionIds,
    onPreview,
    onCompareSource,
}: {
    instanceId: string
    sources: PurchaseSource[]
    availableCompareRevisionIds?: ReadonlySet<string>
    onPreview: (target: WorkspacePaperTarget) => void
    onCompareSource?: (revisionId: string) => void
}) {
    if (!sources.length) {
        return (
            <p className="text-xs text-muted-foreground">
                本次提交未保留来源销售资料，请联系申请人核对。
            </p>
        )
    }
    return (
        <ul className="flex flex-wrap items-center gap-x-4 gap-y-1.5 text-xs">
            {sources.map((sales) => {
                const documentNo =
                    displayBusinessText(sales.document_no, sales.document_id) ||
                    "未记录销售单号"
                const canCompareInRail = Boolean(
                    onCompareSource &&
                    (!availableCompareRevisionIds ||
                        availableCompareRevisionIds.has(sales.revision_id)),
                )
                const contractNotices = sales.source.extra_sections.filter(
                    (field) => field.label === "合同或凭证",
                )
                return (
                    <li
                        key={sales.revision_id}
                        className="flex flex-wrap items-center gap-x-2 gap-y-1"
                    >
                        <span className="num font-medium text-foreground">
                            {documentNo}
                        </span>
                        <span className="text-muted-foreground">
                            {displayName(sales.source.customer) ||
                                "未记录客户名称"}{" "}
                            · V{sales.revision_no}
                        </span>
                        {canCompareInRail ? (
                            <Button
                                id={`${purchaseApprovalPrefix(instanceId)}-source-compare-${toAutomationIdSegment(sales.revision_id)}`}
                                type="button"
                                variant="ghost"
                                size="xs"
                                className={
                                    detailWorkspaceStyles.inlineLinkButton
                                }
                                onClick={() =>
                                    onCompareSource?.(sales.revision_id)
                                }
                            >
                                <Columns2Icon aria-hidden="true" />
                                右侧对照
                            </Button>
                        ) : null}
                        <Button
                            id={`${purchaseApprovalPrefix(instanceId)}-source-${toAutomationIdSegment(sales.revision_id)}`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={detailWorkspaceStyles.subtleButton}
                            onClick={() =>
                                onPreview({
                                    kind: "approval_snapshot",
                                    objectId: instanceId,
                                    materialsView: "source-sales",
                                    sourceRevisionId: sales.revision_id,
                                })
                            }
                        >
                            <FileTextIcon aria-hidden="true" />
                            预览销售单
                        </Button>
                        {contractNotices.map((field) => (
                            <span
                                key={`${field.label}-${field.value}`}
                                className="text-warning-soft-foreground"
                            >
                                （{field.value}）
                            </span>
                        ))}
                    </li>
                )
            })}
        </ul>
    )
}

/** 明细与预览均读取审批绑定的采购提交；来源链接固定到冻结销售版本。 */
export function WorkspacePurchaseApproval({
    instanceId,
    item,
    materials,
    riskReasons,
    onPreview,
    onSelectFile,
    onOpenMaterials,
    onCompareSource,
    activeFileId,
    hideEmbeddedMaterials,
}: {
    instanceId: string
    item: WorkspaceWorkItem
    materials: ReturnType<typeof useApprovalMaterials>
    riskReasons: readonly string[]
    onPreview: (target: WorkspacePaperTarget) => void
    onSelectFile?: (fileId: string) => void
    onOpenMaterials?: () => void
    onCompareSource?: (revisionId: string) => void
    activeFileId?: string
    hideEmbeddedMaterials: boolean
}) {
    const submission = materials.data
    const lines = submission?.purchase_lines
    const fulfillment = submission?.purchase_fulfillment
    const extraCost = submission?.extra_cost_summary
    const prefix = purchaseApprovalPrefix(instanceId)
    const attachments = submission?.attachments ?? []
    const sources = submission?.display.source_sales ?? []
    const extraSections = useMemo(
        () => submission?.display.source.extra_sections ?? [],
        [submission?.display.source.extra_sections],
    )

    const [riskOnly, setRiskOnly] = useState(false)
    const [focusedDeliveryLineId, setFocusedDeliveryLineId] = useState<
        string | null
    >(null)
    const [highlightedLineId, setHighlightedLineId] = useState<string | null>(
        null,
    )
    const highlightTimerRef = useRef<number | null>(null)
    const tableHandleRef = useRef<PurchaseComparisonTableHandle | null>(null)

    useEffect(() => {
        return () => {
            if (highlightTimerRef.current != null) {
                window.clearTimeout(highlightTimerRef.current)
            }
        }
    }, [])

    const evaluatedRows = useMemo(
        () => (lines?.length ? evaluatePurchaseLines(lines) : []),
        [lines],
    )
    const metrics = useMemo(
        () => buildPurchaseApprovalMetrics(evaluatedRows, extraSections),
        [evaluatedRows, extraSections],
    )
    const parsedExtras = useMemo(
        () => parsePurchaseExtraSections(extraSections, item),
        [extraSections, item],
    )

    const availableCompareRevisionIds = useMemo(
        () =>
            new Set(
                (submission?.source_sales_orders ?? []).map(
                    (entry) => entry.revision.id,
                ),
            ),
        [submission?.source_sales_orders],
    )

    const deliveryLines = useMemo<
        readonly DeliveryAllocationLineInput[]
    >(() => {
        if (!lines?.length) return []
        return lines.map((line) => ({
            id: line.id,
            lineNo: line.line_no,
            itemName: displayName(line.title) || "未记录商品名称",
            specification: purchaseSpecification(line.specification),
            unit: line.unitName ?? line.unit ?? "",
            quantity: line.quantity,
            allocations: line.delivery_allocations,
        }))
    }, [lines])

    const deliverySummary = useMemo(
        () => buildDeliveryAllocationSummary(deliveryLines),
        [deliveryLines],
    )

    const deliverySectionId = `${prefix}-delivery-section`
    const materialsSectionId = `${prefix}-materials-section`
    const linesSectionId = `${prefix}-lines-section`

    const handleFocusDelivery = (lineId: string) => {
        setFocusedDeliveryLineId(lineId)
        window.requestAnimationFrame(() => {
            document
                .getElementById(deliverySectionId)
                ?.scrollIntoView({ behavior: "smooth", block: "nearest" })
        })
    }

    const handleReturnToLine = (lineId: string) => {
        tableHandleRef.current?.revealLine(lineId)
        setHighlightedLineId(lineId)
        if (highlightTimerRef.current != null) {
            window.clearTimeout(highlightTimerRef.current)
        }
        highlightTimerRef.current = window.setTimeout(() => {
            setHighlightedLineId(null)
        }, 2200)
        window.requestAnimationFrame(() => {
            window.requestAnimationFrame(() => {
                document
                    .getElementById(
                        `${prefix}-row-${toAutomationIdSegment(lineId)}`,
                    )
                    ?.scrollIntoView({ behavior: "smooth", block: "center" })
            })
        })
    }

    const handleFilterToRisks = () => {
        if (tableHandleRef.current) {
            tableHandleRef.current.filterToRisks()
        } else {
            setRiskOnly(true)
        }
        window.requestAnimationFrame(() => {
            document
                .getElementById(linesSectionId)
                ?.scrollIntoView({ behavior: "smooth", block: "nearest" })
        })
    }

    const handleOpenAttachmentShortcut = () => {
        if (onOpenMaterials) {
            onOpenMaterials()
            return
        }
        document
            .getElementById(materialsSectionId)
            ?.scrollIntoView({ behavior: "smooth", block: "nearest" })
    }

    const showDeliverySection = Boolean(
        submission &&
        (fulfillment != null ||
            (lines && lines.length > 0 && deliverySummary.totalRows > 0)),
    )

    if (materials.isPending) {
        return (
            <DetailWorkspaceSection divider={false} ariaLabel="采购审批资料">
                <p
                    role="status"
                    className="flex items-center gap-2 text-xs text-muted-foreground"
                >
                    <Spinner />
                    正在读取采购审批资料…
                </p>
            </DetailWorkspaceSection>
        )
    }

    if (materials.isError) {
        return (
            <DetailWorkspaceSection divider={false} ariaLabel="采购审批资料">
                <BusinessFailureState
                    id={`${prefix}-retry`}
                    title="采购明细读取失败"
                    error={materials.error}
                    onRetry={() => void materials.refetch()}
                />
            </DetailWorkspaceSection>
        )
    }

    const fallbackGross =
        extraSections.find((field) => field.label === "含税金额")?.value ??
        submission?.display.source.amount_label ??
        item.amountSummary?.value ??
        null
    const fallbackNet =
        extraSections.find((field) => field.label === "不含税金额")?.value ??
        null
    const fallbackTax =
        extraSections.find((field) => field.label === "税额")?.value ?? null
    const submitter = displayName(submission?.display.source.submitter_name)
    const fulfillmentLabel = fulfillmentSummaryLabel(fulfillment)
    const isSevereRisk =
        metrics.negativeMarginCount > 0 ||
        metrics.quantityExcessCount > 0 ||
        metrics.mismatchCount > 0

    const riskChipsSummary = [
        metrics.negativeMarginCount > 0
            ? `负毛利 ${metrics.negativeMarginCount}`
            : null,
        metrics.quantityExcessCount > 0
            ? `超需求 ${metrics.quantityExcessCount}`
            : null,
        metrics.mismatchCount > 0 ? `品规差异 ${metrics.mismatchCount}` : null,
        metrics.overSupplyCount > 0
            ? `超预估 ${metrics.overSupplyCount}`
            : null,
        metrics.delayCount > 0 ? `交期晚 ${metrics.delayCount}` : null,
    ]
        .filter(Boolean)
        .join(" · ")

    return (
        <div
            data-slot="workspace-purchase-approval"
            className="@container/purchase-lines flex min-w-0 flex-col"
        >
            {parsedExtras.resubmission || riskReasons.length > 0 ? (
                <div className="space-y-2.5 pt-3.5">
                    {parsedExtras.resubmission ? (
                        <WorkspaceApprovalResubmissionBanner
                            roundNo={parsedExtras.resubmission.roundNo}
                            resubmissionReason={
                                parsedExtras.resubmission.resubmissionReason
                            }
                            diffBaseline={
                                parsedExtras.resubmission.diffBaseline
                            }
                            diffSummary={parsedExtras.resubmission.diffSummary}
                            diffs={parsedExtras.resubmission.diffs}
                            previousRejection={
                                parsedExtras.resubmission.previousRejection
                            }
                        />
                    ) : null}

                    {riskReasons.length > 0 ? (
                        <Alert
                            variant={isSevereRisk ? "destructive" : "warning"}
                            role="alert"
                        >
                            <TriangleAlertIcon aria-hidden="true" />
                            <AlertTitle className="flex flex-wrap items-center justify-between gap-2">
                                <span>
                                    {isSevereRisk
                                        ? "采购存在高危复核项"
                                        : "采购价格或交期需复核"}
                                </span>
                                {metrics.actionableRiskLineCount > 0 &&
                                !riskOnly ? (
                                    <Button
                                        id={`${prefix}-alert-filter-risk`}
                                        type="button"
                                        variant="outline"
                                        size="xs"
                                        className={cn(
                                            "h-control-xs bg-card/80 px-2 text-xs hover:bg-card",
                                            isSevereRisk
                                                ? "border-destructive-border text-destructive-soft-foreground"
                                                : "border-warning-border text-warning-soft-foreground",
                                        )}
                                        onClick={handleFilterToRisks}
                                    >
                                        仅看风险项（
                                        {metrics.actionableRiskLineCount}）
                                    </Button>
                                ) : null}
                            </AlertTitle>
                            <AlertDescription>
                                {riskReasons.join("；")}。
                            </AlertDescription>
                        </Alert>
                    ) : null}
                </div>
            ) : null}

            <div className="border-b border-border/60">
                <DetailWorkspaceMetrics
                    columns={4}
                    variant="bar"
                    ariaLabel="采购审批关键决策指标"
                >
                    <DetailWorkspaceMetricItem
                        label="采购含税总额"
                        emphasis="primary"
                        value={
                            metrics.grossTotal ? (
                                <MoneyValue
                                    display="exact"
                                    value={metrics.grossTotal}
                                />
                            ) : (
                                (fallbackGross ?? "—")
                            )
                        }
                        detail={
                            <>
                                {metrics.netTotal ? (
                                    <span>
                                        不含税{" "}
                                        <MoneyValue
                                            display="exact"
                                            value={metrics.netTotal}
                                        />
                                    </span>
                                ) : fallbackNet ? (
                                    <span>不含税 {fallbackNet}</span>
                                ) : null}
                                {metrics.taxTotal ? (
                                    <>
                                        <span aria-hidden="true">·</span>
                                        <span>
                                            税{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={metrics.taxTotal}
                                            />
                                        </span>
                                    </>
                                ) : fallbackTax ? (
                                    <>
                                        <span aria-hidden="true">·</span>
                                        <span>税 {fallbackTax}</span>
                                    </>
                                ) : null}
                                {metrics.feeGrossTotal &&
                                metrics.goodsLineCount > 0 ? (
                                    <>
                                        <span aria-hidden="true">·</span>
                                        <span>
                                            含费用{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={metrics.feeGrossTotal}
                                            />
                                        </span>
                                    </>
                                ) : null}
                            </>
                        }
                    />

                    <DetailWorkspaceMetricItem
                        label={
                            extraCost && metrics.goodsLineCount === 0
                                ? "来源销售附加费（本单外）"
                                : "对应销售与毛利"
                        }
                        value={
                            metrics.orderMargin ? (
                                <span
                                    className={cn(
                                        "inline-flex flex-wrap items-baseline gap-x-2",
                                        metrics.orderMargin.negative &&
                                            "text-destructive-soft-foreground",
                                    )}
                                >
                                    <span>
                                        {metrics.orderMargin.negative
                                            ? "亏 "
                                            : ""}
                                        <MoneyValue
                                            display="exact"
                                            value={
                                                metrics.orderMargin.absAmount
                                            }
                                            inheritColor={
                                                metrics.orderMargin.negative
                                            }
                                        />
                                    </span>
                                    <span className="num text-body-compact font-medium">
                                        {metrics.orderMargin.negative
                                            ? `-${metrics.orderMargin.absPercent}%`
                                            : `${metrics.orderMargin.percent}%`}
                                    </span>
                                </span>
                            ) : extraCost ? (
                                <MoneyValue
                                    display="exact"
                                    value={extraCost.occurred_gross_total}
                                />
                            ) : (
                                <span className="text-body-compact font-medium text-muted-foreground">
                                    —
                                </span>
                            )
                        }
                        detail={
                            metrics.correspondingSalesTotal ? (
                                <>
                                    <span>
                                        对应销售{" "}
                                        <MoneyValue
                                            display="exact"
                                            value={
                                                metrics.correspondingSalesTotal
                                            }
                                        />
                                    </span>
                                    {metrics.partialSalesCoverage ? (
                                        <>
                                            <span aria-hidden="true">·</span>
                                            <span className="num text-warning-soft-foreground">
                                                已比对{" "}
                                                {metrics.salesCoveredCount}/
                                                {metrics.goodsLineCount} 项
                                            </span>
                                        </>
                                    ) : null}
                                </>
                            ) : extraCost ? (
                                <>
                                    <span>已发生如上</span>
                                    <span aria-hidden="true">·</span>
                                    <span>
                                        待发生{" "}
                                        <MoneyValue
                                            display="exact"
                                            value={
                                                extraCost.pending_gross_total
                                            }
                                        />
                                    </span>
                                </>
                            ) : (
                                "暂无可比对的销售单价"
                            )
                        }
                    />

                    <DetailWorkspaceMetricItem
                        label="相对预估与风险"
                        value={
                            metrics.netSupplyVarianceSign === "over" &&
                            metrics.absNetSupplyVariance ? (
                                <span className="text-warning-soft-foreground">
                                    超预估 +
                                    <MoneyValue
                                        display="exact"
                                        value={metrics.absNetSupplyVariance}
                                        inheritColor
                                    />
                                </span>
                            ) : metrics.netSupplyVarianceSign === "saved" &&
                              metrics.absNetSupplyVariance ? (
                                <span className="text-success-soft-foreground">
                                    省{" "}
                                    <MoneyValue
                                        display="exact"
                                        value={metrics.absNetSupplyVariance}
                                        inheritColor
                                    />
                                </span>
                            ) : metrics.netSupplyVarianceSign === "equal" ? (
                                <span className="text-body-compact font-medium text-foreground">
                                    与预估持平
                                </span>
                            ) : metrics.actionableRiskLineCount > 0 ? (
                                <span
                                    className={
                                        isSevereRisk
                                            ? "text-destructive-soft-foreground"
                                            : "text-warning-soft-foreground"
                                    }
                                >
                                    {metrics.actionableRiskLineCount} 项需复核
                                </span>
                            ) : (
                                <span className="text-body-compact font-medium text-success-soft-foreground">
                                    无异常项
                                </span>
                            )
                        }
                        detail={
                            riskChipsSummary ? (
                                <button
                                    id={`${prefix}-kpi-filter-risk`}
                                    type="button"
                                    onClick={handleFilterToRisks}
                                    className="cursor-pointer text-left text-primary hover:underline"
                                >
                                    {riskChipsSummary} · 点击筛选
                                </button>
                            ) : metrics.missingSupplyCount > 0 ? (
                                <span className="num">
                                    {metrics.missingSupplyCount}{" "}
                                    项销售未填预估价
                                </span>
                            ) : (
                                "价格、数量与交期均正常"
                            )
                        }
                    />

                    <DetailWorkspaceMetricItem
                        label="付款门禁与附件"
                        value={
                            <span className="inline-flex flex-wrap items-center gap-1.5 text-body-compact font-semibold text-foreground">
                                <span>{metrics.paymentTermLabel ?? "—"}</span>
                                {metrics.isPrepayGate ? (
                                    <Badge variant="warning">先款门禁</Badge>
                                ) : null}
                            </span>
                        }
                        detail={
                            <>
                                {fulfillmentLabel ? (
                                    <span>{fulfillmentLabel}</span>
                                ) : null}
                                {fulfillmentLabel ? (
                                    <span aria-hidden="true">·</span>
                                ) : null}
                                {attachments.length > 0 ? (
                                    <button
                                        id={`${prefix}-kpi-open-materials`}
                                        type="button"
                                        onClick={handleOpenAttachmentShortcut}
                                        className="inline-flex cursor-pointer items-center gap-1 text-primary hover:underline"
                                    >
                                        <PaperclipIcon
                                            aria-hidden="true"
                                            className="size-3 shrink-0"
                                        />
                                        <span className="num">
                                            附件 {attachments.length} 份
                                        </span>
                                    </button>
                                ) : (
                                    <span className="text-warning-soft-foreground">
                                        未附凭证附件
                                    </span>
                                )}
                            </>
                        }
                    />
                </DetailWorkspaceMetrics>
            </div>

            {submission ? (
                <section
                    aria-label="采购单商业与来源信息"
                    className="space-y-2 border-b border-border/60 py-3"
                >
                    <div className="flex flex-wrap items-center gap-x-5 gap-y-1.5 text-xs">
                        <div className="inline-flex items-center gap-1.5">
                            <span className="shrink-0 text-muted-foreground">
                                供应商评估
                            </span>
                            <WorkspaceSupplierRating
                                rating={submission.latest_supplier_rating}
                                compact
                            />
                        </div>
                        {parsedExtras.staticFacts.map((fact) => (
                            <div
                                key={fact.label}
                                className="inline-flex items-baseline gap-1.5"
                            >
                                <span className="shrink-0 text-muted-foreground">
                                    {fact.label}
                                </span>
                                <span
                                    className={cn(
                                        "font-medium text-foreground",
                                        fact.numeric && "num",
                                    )}
                                >
                                    {fact.value}
                                </span>
                            </div>
                        ))}
                        {submitter ? (
                            <div className="inline-flex items-baseline gap-1.5">
                                <span className="shrink-0 text-muted-foreground">
                                    申请人
                                </span>
                                <span className="font-medium text-foreground">
                                    {submitter}
                                </span>
                            </div>
                        ) : null}
                        {item.enteredAtLabel ? (
                            <div className="inline-flex items-baseline gap-1.5">
                                <span className="shrink-0 text-muted-foreground">
                                    进入时间
                                </span>
                                <span className="num font-medium text-foreground">
                                    {item.enteredAtLabel}
                                </span>
                            </div>
                        ) : null}
                    </div>

                    <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1 border-t border-border/45 pt-2 text-xs">
                        <span className="shrink-0 text-muted-foreground">
                            来源销售单
                        </span>
                        <WorkspacePurchaseApprovalSources
                            instanceId={instanceId}
                            sources={sources}
                            availableCompareRevisionIds={
                                availableCompareRevisionIds
                            }
                            onPreview={onPreview}
                            onCompareSource={onCompareSource}
                        />
                    </div>

                    {extraCost && metrics.goodsLineCount > 0 ? (
                        <div className="flex items-center gap-2 rounded-md border border-info-border/70 bg-info-soft/35 px-3 py-1.5 text-xs text-info-soft-foreground">
                            <InfoIcon
                                aria-hidden="true"
                                className="size-3.5 shrink-0"
                            />
                            <span>
                                来源销售单已有附加费用（本次提交之外）：已发生{" "}
                                <MoneyValue
                                    display="exact"
                                    value={extraCost.occurred_gross_total}
                                    inheritColor
                                    className="font-medium"
                                />
                                {" · 待发生 "}
                                <MoneyValue
                                    display="exact"
                                    value={extraCost.pending_gross_total}
                                    inheritColor
                                    className="font-medium"
                                />
                            </span>
                        </div>
                    ) : null}
                </section>
            ) : null}

            <DetailWorkspaceSection
                id={linesSectionId}
                divider={false}
                title="采购明细"
                ariaLabel="采购明细"
                subtitle={
                    lines
                        ? metrics.feeLineCount > 0 && metrics.goodsLineCount > 0
                            ? `共 ${lines.length} 行 · 含 ${metrics.feeLineCount} 项费用`
                            : `共 ${lines.length} 行`
                        : undefined
                }
                className="py-5 first:pt-4"
            >
                {lines?.length ? (
                    <WorkspacePurchaseComparisonTable
                        lines={lines}
                        evaluatedRows={evaluatedRows}
                        metrics={metrics}
                        sources={sources}
                        deliverySummary={deliverySummary}
                        prefix={prefix}
                        riskOnly={riskOnly}
                        onRiskOnlyChange={setRiskOnly}
                        highlightedLineId={highlightedLineId}
                        tableHandleRef={tableHandleRef}
                        onPreview={(revisionId) =>
                            onPreview({
                                kind: "approval_snapshot",
                                objectId: instanceId,
                                materialsView: "source-sales",
                                sourceRevisionId: revisionId,
                            })
                        }
                        onCompareSource={onCompareSource}
                        onFocusDelivery={handleFocusDelivery}
                    />
                ) : lines ? (
                    <p className="text-xs text-muted-foreground">
                        本次提交没有采购明细。
                    </p>
                ) : (
                    <Alert variant="warning">
                        <AlertDescription>
                            本次提交的采购明细暂不可用，请联系申请人核对。
                        </AlertDescription>
                    </Alert>
                )}
            </DetailWorkspaceSection>

            {showDeliverySection ? (
                <DeliveryAllocationOverview
                    idPrefix={`${prefix}-delivery`}
                    sectionId={deliverySectionId}
                    lines={deliveryLines}
                    summary={deliverySummary}
                    density="compact"
                    compactSimpleStates
                    fulfillment={
                        fulfillment
                            ? {
                                  responsibility: fulfillment.responsibility,
                                  targetWarehouseName:
                                      fulfillment.target_warehouse_name,
                              }
                            : undefined
                    }
                    focusedLineId={focusedDeliveryLineId}
                    onClearFocus={() => setFocusedDeliveryLineId(null)}
                    onReturnToLine={handleReturnToLine}
                />
            ) : null}

            {submission && !hideEmbeddedMaterials ? (
                <DetailWorkspaceSection
                    id={materialsSectionId}
                    title="合同、凭证及附件"
                    subtitle={
                        attachments.length
                            ? `共 ${attachments.length} 个文件`
                            : undefined
                    }
                    ariaLabel="合同、凭证及附件"
                >
                    <ApprovalMaterialFiles
                        instanceId={instanceId}
                        attachments={attachments}
                        idPrefix={`${prefix}-file`}
                        emptyMessage="本次提交未附带合同、凭证或其他附件。"
                        onSelectFile={onSelectFile}
                        activeFileId={activeFileId}
                    />
                </DetailWorkspaceSection>
            ) : null}
        </div>
    )
}
