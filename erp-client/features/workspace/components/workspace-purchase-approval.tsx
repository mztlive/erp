"use client"

import {
    useEffect,
    useImperativeHandle,
    useMemo,
    useRef,
    useState,
    type ReactNode,
    type RefObject,
} from "react"
import {
    ChevronRightIcon,
    Columns2Icon,
    FileCheck2Icon,
    FileTextIcon,
    InfoIcon,
    MapPinIcon,
    MessageSquareTextIcon,
    PaperclipIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    MoneyValue,
    WorkspaceFactItem,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import type { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import {
    countUniqueDeliveryPoints,
    formatPurchaseFulfillmentSummary,
} from "@/features/approval-workflow/purchase-fulfillment"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceApprovalResubmissionBanner } from "./workspace-approval-resubmission-banner"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"
import { PurchaseProfitValue } from "./workspace-purchase-amount-details"
import {
    buildPurchaseApprovalMetrics,
    evaluatePurchaseLines,
    parsePurchaseExtraSections,
    purchaseApprovalRiskReasons,
    type PurchaseSource,
} from "./workspace-purchase-comparison-model"
import {
    WorkspacePurchaseComparisonTable,
    type PurchaseComparisonTableHandle,
} from "./workspace-purchase-comparison-table"
import { WorkspacePurchaseDeliveryDialog } from "./workspace-purchase-delivery-dialog"
import type { PurchaseSourceComparisonTarget } from "./workspace-purchase-source-comparison"
import type {
    PurchaseSourceComparisonModel,
    SourceOrderComparison,
} from "./workspace-purchase-source-comparison-model"
import { WorkspaceSupplierRating } from "./workspace-supplier-rating"

export { purchaseApprovalRiskReasons }

export interface WorkspacePurchaseApprovalHandle {
    locateLine: (lineId: string) => void
    filterToRisks: () => void
}

function purchaseApprovalPrefix(instanceId: string) {
    return `workspace-purchase-approval-${toAutomationIdSegment(instanceId)}`
}

export function WorkspacePurchaseApprovalSources({
    instanceId,
    sources,
    orderComparisons,
    onPreview,
    onCompareSource,
}: {
    instanceId: string
    sources: PurchaseSource[]
    orderComparisons?: readonly SourceOrderComparison[]
    onPreview: (target: WorkspacePaperTarget) => void
    onCompareSource?: (target: PurchaseSourceComparisonTarget) => void
}) {
    if (!sources.length) {
        return (
            <p className="text-xs font-normal text-muted-foreground">
                本次提交未保留来源销售资料，请联系申请人核对。
            </p>
        )
    }
    return (
        <ul className="divide-y divide-border/50 text-body-compact font-normal">
            {sources.map((sales) => {
                const documentNo =
                    displayBusinessText(sales.document_no, sales.document_id) ||
                    "未记录销售单号"
                const comparison = orderComparisons?.find(
                    (entry) => entry.revisionId === sales.revision_id,
                )
                const taxCheck = comparison?.commercialChecks.find(
                    (check) => check.id === "tax",
                )
                const ledger = comparison?.ledger
                const orderRiskChecks =
                    sources.length > 1 && comparison
                        ? comparison.commercialChecks.filter(
                              (check) =>
                                  check.status === "severe" ||
                                  check.status === "warning",
                          )
                        : []
                const contractNotices = sales.source.extra_sections.filter(
                    (field) => field.label === "合同或凭证",
                )
                const ownershipParts = [
                    comparison?.projectName
                        ? `项目 ${comparison.projectName}`
                        : null,
                    comparison?.settlementPartyName
                        ? `结算 ${comparison.settlementPartyName}`
                        : null,
                ].filter((part): part is string => Boolean(part))
                const hasTax = Boolean(
                    taxCheck && taxCheck.compactSalesValue !== "未记录",
                )
                const hasLedgerCoverage = Boolean(
                    ledger &&
                    !ledger.isExpenseOrder &&
                    ledger.totalSalesLineCount > 0,
                )
                const hasSecondaryMeta =
                    ownershipParts.length > 0 || hasTax || hasLedgerCoverage

                return (
                    <li
                        key={sales.revision_id}
                        className="space-y-2 py-2.5 first:pt-0 last:pb-0"
                    >
                        <div className="flex flex-col gap-2 @[640px]/purchase-lines:flex-row @[640px]/purchase-lines:items-start @[640px]/purchase-lines:justify-between @[640px]/purchase-lines:gap-4">
                            <div className="min-w-0 flex-1 space-y-1">
                                <div className="flex flex-wrap items-center gap-x-2 gap-y-1">
                                    <span className="num select-all text-body-compact font-semibold text-foreground">
                                        {documentNo}
                                    </span>
                                    <span className="num text-xs text-muted-foreground">
                                        V{sales.revision_no}
                                    </span>
                                    <span
                                        aria-hidden="true"
                                        className="text-xs text-muted-foreground/60"
                                    >
                                        ·
                                    </span>
                                    <span className="text-body-compact font-medium text-foreground">
                                        {displayName(sales.source.customer) ||
                                            "未记录客户名称"}
                                    </span>
                                    {comparison?.hasFullOrder ? (
                                        comparison.contractNo ? (
                                            <span className="num inline-flex items-center gap-1 text-xs text-muted-foreground">
                                                <FileCheck2Icon
                                                    aria-hidden="true"
                                                    className="size-3 shrink-0"
                                                />
                                                {comparison.contractNo}
                                            </span>
                                        ) : (
                                            <Badge variant="neutral">
                                                无合同
                                            </Badge>
                                        )
                                    ) : null}
                                    {orderRiskChecks.map((check) => (
                                        <Badge
                                            key={check.id}
                                            variant={
                                                check.status === "severe"
                                                    ? "destructive"
                                                    : "warning"
                                            }
                                        >
                                            {check.badge}
                                        </Badge>
                                    ))}
                                    {contractNotices.map((field) => (
                                        <span
                                            key={`${field.label}-${field.value}`}
                                            className="text-xs text-warning-soft-foreground"
                                        >
                                            （{field.value}）
                                        </span>
                                    ))}
                                </div>

                                {hasSecondaryMeta ? (
                                    <div className="flex flex-wrap items-center gap-x-2 gap-y-1 text-xs text-muted-foreground">
                                        {ownershipParts.length > 0 ? (
                                            <span>
                                                {ownershipParts.join(" · ")}
                                            </span>
                                        ) : null}
                                        {ownershipParts.length > 0 &&
                                        (hasTax || hasLedgerCoverage) ? (
                                            <span
                                                aria-hidden="true"
                                                className="h-3 w-px bg-border/80"
                                            />
                                        ) : null}
                                        {hasTax ? (
                                            <span className="num">
                                                {taxCheck!.compactSalesValue}
                                            </span>
                                        ) : null}
                                        {hasTax && hasLedgerCoverage ? (
                                            <span aria-hidden="true">·</span>
                                        ) : null}
                                        {hasLedgerCoverage && ledger ? (
                                            ledger.excessCount > 0 ? (
                                                <span className="num font-medium text-destructive-soft-foreground">
                                                    超需求 {ledger.excessCount}{" "}
                                                    项
                                                </span>
                                            ) : (
                                                <span className="num">
                                                    {ledger.isFullOrderCovered
                                                        ? "全量覆盖"
                                                        : `覆盖 ${ledger.coveredLineCount}/${ledger.totalSalesLineCount} 行`}
                                                    {ledger.salesCoveragePercent
                                                        ? `（占额 ${ledger.salesCoveragePercent}%）`
                                                        : ""}
                                                </span>
                                            )
                                        ) : null}
                                    </div>
                                ) : null}
                            </div>

                            <div className="flex shrink-0 flex-wrap items-center gap-1.5">
                                {onCompareSource ? (
                                    <Button
                                        id={`${purchaseApprovalPrefix(instanceId)}-source-compare-${toAutomationIdSegment(sales.revision_id)}`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className={
                                            detailWorkspaceStyles.inlineLinkButton
                                        }
                                        onClick={() =>
                                            onCompareSource({
                                                revisionId: sales.revision_id,
                                            })
                                        }
                                    >
                                        <Columns2Icon aria-hidden="true" />
                                        采销对照
                                    </Button>
                                ) : null}
                                <Button
                                    id={`${purchaseApprovalPrefix(instanceId)}-source-${toAutomationIdSegment(sales.revision_id)}`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
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
                            </div>
                        </div>

                        {comparison?.businessRemark ? (
                            <div className="flex items-start gap-2 rounded-r-md border-l-2 border-info bg-info-soft/30 px-2.5 py-1.5 text-xs">
                                <MessageSquareTextIcon
                                    aria-hidden="true"
                                    className="mt-0.5 size-3.5 shrink-0 text-info-soft-foreground"
                                />
                                <p className="min-w-0 whitespace-pre-wrap wrap-anywhere text-foreground/90">
                                    <span className="font-medium text-info-soft-foreground">
                                        销售备注：
                                    </span>
                                    {comparison.businessRemark}
                                </p>
                            </div>
                        ) : null}
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
    sourceComparison,
    riskReasons,
    onPreview,
    onSelectFile,
    onOpenMaterials,
    onCompareSource,
    approvalHandleRef,
    activeFileId,
    hideEmbeddedMaterials,
    hideTopBanners = false,
}: {
    instanceId: string
    item: WorkspaceWorkItem
    materials: ReturnType<typeof useApprovalMaterials>
    sourceComparison?: PurchaseSourceComparisonModel
    riskReasons: readonly string[]
    onPreview: (target: WorkspacePaperTarget) => void
    onSelectFile?: (fileId: string) => void
    onOpenMaterials?: () => void
    onCompareSource?: (target: PurchaseSourceComparisonTarget) => void
    approvalHandleRef?: RefObject<WorkspacePurchaseApprovalHandle | null>
    activeFileId?: string
    hideEmbeddedMaterials: boolean
    hideTopBanners?: boolean
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
    const [deliveryDialogOpen, setDeliveryDialogOpen] = useState(false)
    const [focusedDeliveryLineId, setFocusedDeliveryLineId] = useState<
        string | null
    >(null)
    const [highlightedLineId, setHighlightedLineId] = useState<string | null>(
        null,
    )
    const highlightTimerRef = useRef<number | null>(null)
    const tableHandleRef = useRef<PurchaseComparisonTableHandle | null>(null)

    const openDeliveryDialog = (lineId: string | null) => {
        setFocusedDeliveryLineId(lineId)
        setDeliveryDialogOpen(true)
    }

    useEffect(() => {
        return () => {
            if (highlightTimerRef.current != null) {
                window.clearTimeout(highlightTimerRef.current)
            }
        }
    }, [])

    const evaluatedRows = useMemo(
        () =>
            lines?.length
                ? evaluatePurchaseLines(lines, sourceComparison?.lineInsights)
                : [],
        [lines, sourceComparison?.lineInsights],
    )
    const metrics = useMemo(
        () => buildPurchaseApprovalMetrics(evaluatedRows, extraSections),
        [evaluatedRows, extraSections],
    )
    const parsedExtras = useMemo(
        () => parsePurchaseExtraSections(extraSections, item),
        [extraSections, item],
    )

    const uniqueDeliveryPointCount = useMemo(
        () =>
            countUniqueDeliveryPoints(
                (lines ?? []).map((line) => line.delivery_allocations),
            ),
        [lines],
    )

    const materialsSectionId = `${prefix}-materials-section`
    const linesSectionId = `${prefix}-lines-section`

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

    useImperativeHandle(approvalHandleRef, () => ({
        locateLine: (lineId: string) => {
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
                        ?.scrollIntoView({
                            behavior: "smooth",
                            block: "center",
                        })
                })
            })
        },
        filterToRisks: handleFilterToRisks,
    }))

    const handleOpenAttachmentShortcut = () => {
        if (onOpenMaterials) {
            onOpenMaterials()
            return
        }
        document
            .getElementById(materialsSectionId)
            ?.scrollIntoView({ behavior: "smooth", block: "nearest" })
    }

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
    const fulfillmentModeLabel = formatPurchaseFulfillmentSummary(
        fulfillment,
        0,
    )
    const isPureExpenseOrder = Boolean(
        extraCost && metrics.goodsLineCount === 0,
    )
    const summary = sourceComparison?.summary
    const singleOrderComparison =
        sourceComparison?.orders.length === 1
            ? sourceComparison.orders[0]
            : undefined
    const singleOrderExtraCost = singleOrderComparison?.ledger?.extraCostSummary
    const singlePaymentCheck = singleOrderComparison?.commercialChecks.find(
        (check) => check.id === "payment",
    )
    const singleFulfillmentCheck = singleOrderComparison?.commercialChecks.find(
        (check) => check.id === "fulfillment",
    )
    const hasTaxInverted = Boolean(
        summary &&
        (summary.overallTaxInverted || summary.taxInvertedLineCount > 0),
    )
    const isSevereRisk =
        metrics.externalLossCount > 0 ||
        metrics.quantityExcessCount > 0 ||
        metrics.mismatchCount > 0 ||
        hasTaxInverted ||
        (summary?.splitMergedExcessLineCount ?? 0) > 0

    const paymentRiskChip =
        summary && summary.paymentRiskChecks.length === 1
            ? `${summary.paymentRiskChecks[0]!.check.badge}${summary.paymentRiskChecks[0]!.check.metric ? `（${summary.paymentRiskChecks[0]!.check.metric}）` : ""}`
            : summary && summary.paymentRiskChecks.length > 1
              ? `资金错配 ${summary.paymentRiskChecks.length} 单`
              : null

    const taxRiskChip =
        !hasTaxInverted && summary
            ? summary.lowerInputTaxLineCount > 0
                ? `进项偏低 ${summary.lowerInputTaxLineCount}`
                : summary.specialInvoiceRiskOrderCount > 0
                  ? "核对专票抵扣"
                  : null
            : null

    const fulfillmentRiskChip =
        summary && summary.fulfillmentRiskChecks.length === 1
            ? summary.fulfillmentRiskChecks[0]!.check.badge
            : summary && summary.fulfillmentRiskChecks.length > 1
              ? `履约/取价关注 ${summary.fulfillmentRiskChecks.length} 单`
              : null

    const riskChipsSummary = [
        metrics.externalLossCount > 0
            ? `对外负毛利 ${metrics.externalLossCount}`
            : null,
        summary?.overallTaxInverted
            ? "整单税后倒挂"
            : summary && summary.taxInvertedLineCount > 0
              ? `税后倒挂 ${summary.taxInvertedLineCount}`
              : null,
        metrics.quantityExcessCount > 0
            ? `超需求 ${metrics.quantityExcessCount}`
            : summary && summary.splitMergedExcessLineCount > 0
              ? `拆笔超需求 ${summary.splitMergedExcessLineCount}`
              : null,
        metrics.mismatchCount > 0 ? `品规差异 ${metrics.mismatchCount}` : null,
        paymentRiskChip,
        taxRiskChip,
        metrics.costAboveSupplyCount > 0
            ? `采购倒挂 ${metrics.costAboveSupplyCount}`
            : null,
        metrics.delayCount > 0 ? `交期晚 ${metrics.delayCount}` : null,
        fulfillmentRiskChip,
    ]
        .filter(Boolean)
        .join(" · ")

    const firstCommercialRiskOrder =
        summary?.paymentRiskChecks[0] ??
        summary?.taxRiskChecks[0] ??
        summary?.fulfillmentRiskChecks[0]
    const hasCommercialRisk = Boolean(firstCommercialRiskOrder)

    const overallNetMargin = summary?.overallNetMargin ?? null
    const overallTaxInverted = Boolean(summary?.overallTaxInverted)
    const hasSignificantTaxDrag = Boolean(summary?.hasSignificantTaxDrag)
    const hasPositiveVatGap = Boolean(
        summary?.overallVatGap &&
        (() => {
            try {
                return compareDecimal(summary.overallVatGap, "0", 2) > 0
            } catch {
                return false
            }
        })(),
    )

    const purchaseMarginLimited = Boolean(
        metrics.purchaseMargin &&
        (metrics.purchaseMargin.lineCount < metrics.goodsLineCount ||
            metrics.feeLineCount > 0),
    )
    const showGrossTooltip = Boolean(
        metrics.feeGrossTotal ||
        metrics.netTotal ||
        metrics.taxTotal ||
        fallbackNet ||
        fallbackTax,
    )
    const missingDeliveryAddress =
        uniqueDeliveryPointCount === 0 &&
        (fulfillment?.responsibility === "SUPPLIER_DIRECT" ||
            fulfillment?.responsibility === "SERVICE")

    return (
        <TooltipProvider>
            <div
                data-slot="workspace-purchase-approval"
                className="@container/purchase-lines flex min-w-0 flex-col"
            >
                {!hideTopBanners &&
                (parsedExtras.resubmission || riskReasons.length > 0) ? (
                    <div className="space-y-2 pt-3">
                        {parsedExtras.resubmission ? (
                            <WorkspaceApprovalResubmissionBanner
                                roundNo={parsedExtras.resubmission.roundNo}
                                resubmissionReason={
                                    parsedExtras.resubmission.resubmissionReason
                                }
                                diffBaseline={
                                    parsedExtras.resubmission.diffBaseline
                                }
                                diffSummary={
                                    parsedExtras.resubmission.diffSummary
                                }
                                diffs={parsedExtras.resubmission.diffs}
                                previousRejection={
                                    parsedExtras.resubmission.previousRejection
                                }
                            />
                        ) : null}

                        {riskReasons.length > 0 ? (
                            <div
                                role="alert"
                                data-slot="purchase-approval-risk-strip"
                                title={riskReasons.join("；")}
                                className={cn(
                                    "flex flex-wrap items-center justify-between gap-x-3 gap-y-1 rounded-md border-l-2 px-3 py-1.5 text-xs",
                                    isSevereRisk
                                        ? "border-l-destructive bg-destructive-soft/65 text-destructive-soft-foreground"
                                        : "border-l-warning bg-warning-soft/75 text-warning-soft-foreground",
                                )}
                            >
                                <div className="flex min-w-0 flex-1 items-center gap-2">
                                    <TriangleAlertIcon
                                        aria-hidden="true"
                                        className="size-3.5 shrink-0"
                                    />
                                    <p className="min-w-0 truncate">
                                        <span className="font-medium">
                                            {isSevereRisk
                                                ? "高危复核"
                                                : "需复核"}
                                        </span>
                                        <span
                                            aria-hidden="true"
                                            className="mx-1.5 opacity-60"
                                        >
                                            ·
                                        </span>
                                        <span>
                                            {riskChipsSummary ||
                                                riskReasons.join("；")}
                                        </span>
                                    </p>
                                </div>
                                <div className="flex shrink-0 items-center gap-1">
                                    {metrics.actionableRiskLineCount > 0 &&
                                    !riskOnly ? (
                                        <Button
                                            id={`${prefix}-alert-filter-risk`}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className={cn(
                                                "h-control-xs shrink-0 gap-0.5 px-1.5 text-xs font-medium shadow-none",
                                                isSevereRisk
                                                    ? "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                                    : "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
                                            )}
                                            onClick={handleFilterToRisks}
                                        >
                                            仅看风险项（
                                            {metrics.actionableRiskLineCount}）
                                            <ChevronRightIcon aria-hidden="true" />
                                        </Button>
                                    ) : null}
                                    {hasCommercialRisk && onCompareSource ? (
                                        <Button
                                            id={`${prefix}-alert-compare-source`}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className={cn(
                                                "h-control-xs shrink-0 gap-0.5 px-1.5 text-xs font-medium shadow-none",
                                                isSevereRisk
                                                    ? "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                                    : "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
                                            )}
                                            onClick={() =>
                                                onCompareSource({
                                                    revisionId:
                                                        firstCommercialRiskOrder?.revisionId ??
                                                        sourceComparison
                                                            ?.orders[0]
                                                            ?.revisionId,
                                                })
                                            }
                                        >
                                            采销对照
                                            <ChevronRightIcon aria-hidden="true" />
                                        </Button>
                                    ) : null}
                                </div>
                            </div>
                        ) : null}
                    </div>
                ) : null}

                <div
                    role="region"
                    aria-label="采购审批关键决策指标"
                    className={cn(
                        "mt-3 grid grid-cols-1 gap-x-6 gap-y-3.5 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/purchase-lines:grid-cols-2",
                        isPureExpenseOrder
                            ? "@[720px]/purchase-lines:grid-cols-3"
                            : "@[840px]/purchase-lines:grid-cols-4",
                    )}
                >
                    <PurchaseKpiCell
                        label="实际采购总额"
                        basis="含税"
                        emphasis="primary"
                        tooltipId={
                            showGrossTooltip
                                ? `${prefix}-kpi-gross-info`
                                : undefined
                        }
                        tooltipLabel="实际采购总额明细"
                        tooltipContent={
                            showGrossTooltip ? (
                                <div className="num space-y-0.5">
                                    <div>
                                        含税总额：
                                        {metrics.grossTotal ? (
                                            <MoneyValue
                                                display="exact"
                                                value={metrics.grossTotal}
                                                inheritColor
                                            />
                                        ) : (
                                            (fallbackGross ?? "—")
                                        )}
                                    </div>
                                    {metrics.feeGrossTotal ? (
                                        <div>
                                            其中费用：
                                            <MoneyValue
                                                display="exact"
                                                value={metrics.feeGrossTotal}
                                                inheritColor
                                            />
                                        </div>
                                    ) : null}
                                    {metrics.netTotal || fallbackNet ? (
                                        <div>
                                            不含税总额：
                                            {metrics.netTotal ? (
                                                <MoneyValue
                                                    display="exact"
                                                    value={metrics.netTotal}
                                                    inheritColor
                                                />
                                            ) : (
                                                fallbackNet
                                            )}
                                        </div>
                                    ) : null}
                                    {metrics.taxTotal || fallbackTax ? (
                                        <div>
                                            税额：
                                            {metrics.taxTotal ? (
                                                <MoneyValue
                                                    display="exact"
                                                    value={metrics.taxTotal}
                                                    inheritColor
                                                />
                                            ) : (
                                                fallbackTax
                                            )}
                                        </div>
                                    ) : null}
                                </div>
                            ) : undefined
                        }
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
                            metrics.netTotal ||
                            metrics.taxTotal ||
                            fallbackNet ||
                            fallbackTax ? (
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
                                    {(metrics.netTotal || fallbackNet) &&
                                    (metrics.taxTotal || fallbackTax) ? (
                                        <span aria-hidden="true">·</span>
                                    ) : null}
                                    {metrics.taxTotal ? (
                                        <span>
                                            税{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={metrics.taxTotal}
                                            />
                                        </span>
                                    ) : fallbackTax ? (
                                        <span>税 {fallbackTax}</span>
                                    ) : null}
                                </>
                            ) : metrics.feeGrossTotal &&
                              metrics.goodsLineCount > 0 ? (
                                <span>
                                    含费用{" "}
                                    <MoneyValue
                                        display="exact"
                                        value={metrics.feeGrossTotal}
                                    />
                                </span>
                            ) : null
                        }
                    />

                    {isPureExpenseOrder && extraCost ? (
                        singleOrderExtraCost?.combinedWithCurrentFeeTotal !=
                        null ? (
                            <PurchaseKpiCell
                                label="来源销售附加费"
                                basis="含本单累计"
                                tooltipId={`${prefix}-kpi-extra-cost-info`}
                                tooltipLabel="来源销售附加费累计明细"
                                tooltipContent={
                                    <div className="num space-y-0.5">
                                        <div>
                                            已有附加费（已发生）：
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    singleOrderExtraCost.occurredGrossTotal
                                                }
                                                inheritColor
                                            />
                                        </div>
                                        <div>
                                            待发生附加费：
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    singleOrderExtraCost.pendingGrossTotal
                                                }
                                                inheritColor
                                            />
                                        </div>
                                        {singleOrderComparison?.ledger
                                            ?.purchaseFeeGrossTotal ? (
                                            <div>
                                                本单费用：
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        singleOrderComparison
                                                            .ledger
                                                            .purchaseFeeGrossTotal
                                                    }
                                                    inheritColor
                                                />
                                                {singleOrderComparison.ledger
                                                    .purchaseFeeRatioPercent
                                                    ? `（占销售 ${singleOrderComparison.ledger.purchaseFeeRatioPercent}%）`
                                                    : ""}
                                            </div>
                                        ) : null}
                                        {singleOrderComparison?.ledger
                                            ?.salesGrossTotal ? (
                                            <div>
                                                来源销售总额：
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        singleOrderComparison
                                                            .ledger
                                                            .salesGrossTotal
                                                    }
                                                    inheritColor
                                                />
                                            </div>
                                        ) : null}
                                    </div>
                                }
                                value={
                                    <span className="inline-flex flex-wrap items-baseline gap-x-2">
                                        <MoneyValue
                                            display="exact"
                                            value={
                                                singleOrderExtraCost.combinedWithCurrentFeeTotal
                                            }
                                        />
                                        {singleOrderExtraCost.combinedRatioPercent ? (
                                            <span className="num text-body-compact font-medium text-muted-foreground">
                                                占销{" "}
                                                {
                                                    singleOrderExtraCost.combinedRatioPercent
                                                }
                                                %
                                            </span>
                                        ) : null}
                                    </span>
                                }
                                detail={
                                    <>
                                        <span>
                                            已有{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    singleOrderExtraCost.occurredGrossTotal
                                                }
                                            />
                                        </span>
                                        <span aria-hidden="true">·</span>
                                        <span>
                                            待发生{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    singleOrderExtraCost.pendingGrossTotal
                                                }
                                            />
                                        </span>
                                    </>
                                }
                            />
                        ) : (
                            <PurchaseKpiCell
                                label="来源销售附加费"
                                basis="本单外"
                                value={
                                    <MoneyValue
                                        display="exact"
                                        value={extraCost.occurred_gross_total}
                                    />
                                }
                                detail={
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
                                }
                            />
                        )
                    ) : (
                        <>
                            <PurchaseKpiCell
                                label="采购毛利"
                                basis="供给 − 采购"
                                tooltipId={
                                    metrics.purchaseMargin
                                        ? `${prefix}-kpi-purchase-margin-info`
                                        : undefined
                                }
                                tooltipLabel="采购毛利核算说明"
                                tooltipContent={
                                    metrics.purchaseMargin ? (
                                        <div className="num space-y-0.5">
                                            <div>
                                                口径：采购供给总额 −
                                                对应实际采购额（含税）
                                            </div>
                                            <div>
                                                采购供给总额：
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        metrics.purchaseMargin
                                                            .supply
                                                    }
                                                    inheritColor
                                                />
                                            </div>
                                            <div>
                                                对应实际采购额：
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        metrics.purchaseMargin
                                                            .cost
                                                    }
                                                    inheritColor
                                                />
                                            </div>
                                            {purchaseMarginLimited ? (
                                                <div className="text-background/75">
                                                    已核算{" "}
                                                    {
                                                        metrics.purchaseMargin
                                                            .lineCount
                                                    }
                                                    /{metrics.goodsLineCount}{" "}
                                                    项商品
                                                    {metrics.feeLineCount > 0
                                                        ? "（不含费用）"
                                                        : ""}
                                                </div>
                                            ) : null}
                                        </div>
                                    ) : undefined
                                }
                                value={
                                    metrics.purchaseMargin ? (
                                        <PurchaseProfitValue
                                            amount={
                                                metrics.purchaseMargin.margin
                                            }
                                            className="font-semibold"
                                        />
                                    ) : (
                                        <span className="text-body-compact font-medium text-muted-foreground">
                                            暂无法核算
                                        </span>
                                    )
                                }
                                detail={
                                    metrics.purchaseMargin ? (
                                        <>
                                            <span>
                                                供给总额{" "}
                                                <MoneyValue
                                                    display="exact"
                                                    value={
                                                        metrics.purchaseMargin
                                                            .supply
                                                    }
                                                />
                                            </span>
                                            {metrics.purchaseMargin.lineCount <
                                            metrics.goodsLineCount ? (
                                                <>
                                                    <span aria-hidden="true">
                                                        ·
                                                    </span>
                                                    <span className="num text-warning-soft-foreground">
                                                        {
                                                            metrics
                                                                .purchaseMargin
                                                                .lineCount
                                                        }
                                                        /
                                                        {metrics.goodsLineCount}{" "}
                                                        项
                                                    </span>
                                                </>
                                            ) : metrics.feeLineCount > 0 ? (
                                                <>
                                                    <span aria-hidden="true">
                                                        ·
                                                    </span>
                                                    <span>不含费用</span>
                                                </>
                                            ) : null}
                                        </>
                                    ) : metrics.missingSupplyCount > 0 ? (
                                        <span className="num">
                                            {metrics.missingSupplyCount}{" "}
                                            项未填供给价
                                        </span>
                                    ) : (
                                        "暂无可比对供给价"
                                    )
                                }
                            />

                            <PurchaseKpiCell
                                label="公司对外毛利"
                                basis={
                                    overallNetMargin
                                        ? "含税 / 税后"
                                        : "销售 − 采购"
                                }
                                tooltipId={
                                    metrics.externalGrossMargin != null
                                        ? `${prefix}-kpi-external-margin-info`
                                        : undefined
                                }
                                tooltipLabel="公司对外毛利核算说明"
                                tooltipContent={
                                    metrics.externalGrossMargin != null ? (
                                        <div className="num space-y-0.5">
                                            <div>
                                                口径：对应对外销售总额 −
                                                对应实际采购成本（含税 /
                                                不含税）
                                            </div>
                                            {metrics.correspondingSalesTotal ? (
                                                <div>
                                                    对应对外销售（含税）：
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            metrics.correspondingSalesTotal
                                                        }
                                                        inheritColor
                                                    />
                                                </div>
                                            ) : null}
                                            {summary?.overallCorrespondingSalesNetTotal ? (
                                                <div>
                                                    对应对外销售（不含税）：
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            summary.overallCorrespondingSalesNetTotal
                                                        }
                                                        inheritColor
                                                    />
                                                </div>
                                            ) : null}
                                            {metrics.feeGrossTotal ? (
                                                <div>
                                                    已计入本单费用：
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            metrics.feeGrossTotal
                                                        }
                                                        inheritColor
                                                    />
                                                </div>
                                            ) : null}
                                            {summary?.overallGrossMargin ? (
                                                <div>
                                                    含税毛利：
                                                    {summary.overallGrossMargin
                                                        .negative
                                                        ? "亏 "
                                                        : ""}
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            summary
                                                                .overallGrossMargin
                                                                .absAmount
                                                        }
                                                        inheritColor
                                                    />{" "}
                                                    (
                                                    {
                                                        summary
                                                            .overallGrossMargin
                                                            .percent
                                                    }
                                                    %)
                                                </div>
                                            ) : null}
                                            {overallNetMargin ? (
                                                <div>
                                                    税后（不含税）毛利：
                                                    {overallNetMargin.negative
                                                        ? "亏 "
                                                        : ""}
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            overallNetMargin.absAmount
                                                        }
                                                        inheritColor
                                                    />{" "}
                                                    ({overallNetMargin.percent}
                                                    %)
                                                </div>
                                            ) : (
                                                <div className="text-background/75">
                                                    部分进项税率或来源销售正文未完整记录，暂无法核算税后口径
                                                </div>
                                            )}
                                            {hasPositiveVatGap &&
                                            summary?.overallVatGap ? (
                                                <div>
                                                    进销项税差：
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            summary.overallVatGap
                                                        }
                                                        inheritColor
                                                    />
                                                </div>
                                            ) : null}
                                        </div>
                                    ) : undefined
                                }
                                value={
                                    metrics.externalGrossMargin != null ? (
                                        <span className="inline-flex flex-wrap items-center gap-x-2 gap-y-1">
                                            <PurchaseProfitValue
                                                amount={
                                                    metrics.externalGrossMargin
                                                }
                                                className="font-semibold"
                                            />
                                            {metrics.externalMargin ? (
                                                <span className="num text-body-compact font-medium">
                                                    {
                                                        metrics.externalMargin
                                                            .percent
                                                    }
                                                    %
                                                </span>
                                            ) : null}
                                            {overallTaxInverted ? (
                                                <Badge variant="destructive">
                                                    税后倒挂
                                                </Badge>
                                            ) : null}
                                        </span>
                                    ) : (
                                        <span className="text-body-compact font-medium text-muted-foreground">
                                            —
                                        </span>
                                    )
                                }
                                detail={
                                    overallNetMargin ||
                                    metrics.correspondingSalesTotal ? (
                                        <div className="flex w-full flex-col gap-0.5">
                                            {overallNetMargin ? (
                                                <span className="inline-flex flex-wrap items-baseline gap-x-1">
                                                    <span>税后</span>
                                                    <PurchaseProfitValue
                                                        amount={
                                                            overallNetMargin.amount
                                                        }
                                                        className={cn(
                                                            overallTaxInverted &&
                                                                "font-semibold",
                                                        )}
                                                    />
                                                    <span
                                                        className={cn(
                                                            "num",
                                                            overallNetMargin.negative
                                                                ? "font-medium text-destructive-soft-foreground"
                                                                : hasSignificantTaxDrag
                                                                  ? "font-medium text-warning-soft-foreground"
                                                                  : undefined,
                                                        )}
                                                    >
                                                        (
                                                        {
                                                            overallNetMargin.percent
                                                        }
                                                        %)
                                                    </span>
                                                    {!overallTaxInverted &&
                                                    hasSignificantTaxDrag &&
                                                    summary?.overallTaxDragPoints ? (
                                                        <span className="num font-medium text-warning-soft-foreground">
                                                            · 税损 -
                                                            {
                                                                summary.overallTaxDragPoints
                                                            }
                                                            pp
                                                        </span>
                                                    ) : null}
                                                </span>
                                            ) : null}
                                            <span className="flex flex-wrap items-center gap-x-2">
                                                {metrics.correspondingSalesTotal ? (
                                                    <>
                                                        <span>
                                                            对外销售{" "}
                                                            <MoneyValue
                                                                display="exact"
                                                                value={
                                                                    metrics.correspondingSalesTotal
                                                                }
                                                            />
                                                        </span>
                                                        {metrics.partialSalesCoverage ? (
                                                            <>
                                                                <span aria-hidden="true">
                                                                    ·
                                                                </span>
                                                                <span className="num text-warning-soft-foreground">
                                                                    {
                                                                        metrics.salesCoveredCount
                                                                    }
                                                                    /
                                                                    {
                                                                        metrics.goodsLineCount
                                                                    }{" "}
                                                                    项
                                                                </span>
                                                            </>
                                                        ) : null}
                                                    </>
                                                ) : (
                                                    "暂无可比对对外售价"
                                                )}
                                            </span>
                                        </div>
                                    ) : (
                                        "暂无可比对对外售价"
                                    )
                                }
                            />
                        </>
                    )}

                    <PurchaseKpiCell
                        label="付款与交付"
                        value={
                            <span className="inline-flex flex-wrap items-center gap-1.5 text-body-compact font-semibold text-foreground">
                                <span>{metrics.paymentTermLabel ?? "—"}</span>
                                {metrics.isPrepayGate ? (
                                    <Badge variant="warning">先款门禁</Badge>
                                ) : null}
                                {singlePaymentCheck &&
                                (singlePaymentCheck.status === "warning" ||
                                    singlePaymentCheck.status === "severe") ? (
                                    <Badge
                                        variant={
                                            singlePaymentCheck.status ===
                                            "severe"
                                                ? "destructive"
                                                : "warning"
                                        }
                                    >
                                        {singlePaymentCheck.badge}
                                    </Badge>
                                ) : (sourceComparison?.orders.length ?? 0) >
                                      1 &&
                                  summary &&
                                  summary.paymentRiskChecks.length > 0 ? (
                                    <Badge variant="warning">
                                        {summary.paymentRiskChecks.length}{" "}
                                        单资金风险
                                    </Badge>
                                ) : null}
                            </span>
                        }
                        detail={
                            <>
                                {singlePaymentCheck &&
                                singlePaymentCheck.compactSalesValue !==
                                    "未记录" ? (
                                    singlePaymentCheck.status === "warning" ||
                                    singlePaymentCheck.status === "severe" ? (
                                        <span
                                            className={cn(
                                                "w-full font-medium",
                                                singlePaymentCheck.status ===
                                                    "severe"
                                                    ? "text-destructive-soft-foreground"
                                                    : "text-warning-soft-foreground",
                                            )}
                                        >
                                            客户{" "}
                                            {
                                                singlePaymentCheck.compactSalesValue
                                            }{" "}
                                            → 采{" "}
                                            {
                                                singlePaymentCheck.compactPurchaseValue
                                            }
                                            {singlePaymentCheck.metric
                                                ? `（${singlePaymentCheck.metric}）`
                                                : ""}
                                        </span>
                                    ) : singlePaymentCheck.status === "ok" ||
                                      singlePaymentCheck.status === "info" ? (
                                        <span className="w-full">
                                            客户{" "}
                                            {
                                                singlePaymentCheck.compactSalesValue
                                            }
                                            {singlePaymentCheck.direction ===
                                            "up"
                                                ? " · 资金顺差"
                                                : ""}
                                        </span>
                                    ) : null
                                ) : null}
                                {fulfillmentModeLabel ? (
                                    <span>
                                        {fulfillmentModeLabel}
                                        {singleFulfillmentCheck?.status ===
                                        "warning" ? (
                                            <span className="text-warning-soft-foreground">
                                                {" "}
                                                · {singleFulfillmentCheck.badge}
                                            </span>
                                        ) : null}
                                    </span>
                                ) : null}
                                {uniqueDeliveryPointCount > 0 ? (
                                    <>
                                        {fulfillmentModeLabel ? (
                                            <span aria-hidden="true">·</span>
                                        ) : null}
                                        <button
                                            id={`${prefix}-kpi-delivery-addresses`}
                                            type="button"
                                            onClick={() =>
                                                openDeliveryDialog(null)
                                            }
                                            className="inline-flex cursor-pointer items-center gap-1 text-primary hover:underline"
                                        >
                                            <MapPinIcon
                                                aria-hidden="true"
                                                className="size-3 shrink-0"
                                            />
                                            <span className="num">
                                                配送地址（
                                                {uniqueDeliveryPointCount} 处）
                                            </span>
                                        </button>
                                    </>
                                ) : missingDeliveryAddress ? (
                                    <>
                                        {fulfillmentModeLabel ? (
                                            <span aria-hidden="true">·</span>
                                        ) : null}
                                        <span>未指定地址</span>
                                    </>
                                ) : null}
                                {!hideEmbeddedMaterials ? (
                                    <>
                                        {fulfillmentModeLabel ||
                                        uniqueDeliveryPointCount > 0 ||
                                        missingDeliveryAddress ? (
                                            <span aria-hidden="true">·</span>
                                        ) : null}
                                        {attachments.length > 0 ? (
                                            <button
                                                id={`${prefix}-kpi-open-materials`}
                                                type="button"
                                                onClick={
                                                    handleOpenAttachmentShortcut
                                                }
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
                                ) : null}
                            </>
                        }
                    />
                </div>

                {submission ? (
                    <section
                        aria-label="采购单商业与来源信息"
                        className="divide-y divide-border/60 border-b border-border/60"
                    >
                        <div className="py-3.5">
                            <dl
                                aria-label="单据与供应商概况"
                                className="grid grid-cols-1 gap-x-6 gap-y-3 @[480px]/purchase-lines:grid-cols-2 @[840px]/purchase-lines:grid-cols-4"
                            >
                                <WorkspaceFactItem
                                    label="供应商评估"
                                    className="@[480px]/purchase-lines:col-span-2"
                                >
                                    <WorkspaceSupplierRating
                                        rating={
                                            submission.latest_supplier_rating
                                        }
                                        compact
                                    />
                                </WorkspaceFactItem>
                                {(() => {
                                    const primaryPurchaseType =
                                        parsedExtras.purchaseType ||
                                        (!fulfillmentModeLabel
                                            ? parsedExtras.fulfillmentResponsibility
                                            : undefined)
                                    const purchaseAttributeDetails = [
                                        parsedExtras.businessCategory
                                            ? `类目：${parsedExtras.businessCategory}`
                                            : null,
                                        !fulfillmentModeLabel &&
                                        parsedExtras.purchaseType &&
                                        parsedExtras.fulfillmentResponsibility
                                            ? parsedExtras.fulfillmentResponsibility
                                            : null,
                                    ].filter((part): part is string =>
                                        Boolean(part),
                                    )
                                    return (
                                        <WorkspaceFactItem
                                            label="采购类型"
                                            tone={
                                                primaryPurchaseType
                                                    ? "default"
                                                    : "muted"
                                            }
                                        >
                                            {primaryPurchaseType ||
                                            purchaseAttributeDetails.length >
                                                0 ? (
                                                <div className="space-y-0.5">
                                                    <div>
                                                        {primaryPurchaseType ??
                                                            "—"}
                                                    </div>
                                                    {purchaseAttributeDetails.length >
                                                    0 ? (
                                                        <p className="text-xs font-normal text-muted-foreground">
                                                            {purchaseAttributeDetails.join(
                                                                " · ",
                                                            )}
                                                        </p>
                                                    ) : null}
                                                </div>
                                            ) : null}
                                        </WorkspaceFactItem>
                                    )
                                })()}
                                <WorkspaceFactItem
                                    label="提交信息"
                                    tone={submitter ? "default" : "muted"}
                                >
                                    {submitter ||
                                    (item.enteredAtLabel &&
                                        item.enteredAtLabel !== "—") ? (
                                        <div className="space-y-0.5">
                                            <div>{submitter || "—"}</div>
                                            {item.enteredAtLabel &&
                                            item.enteredAtLabel !== "—" ? (
                                                <p className="num text-xs font-normal text-muted-foreground">
                                                    {item.enteredAtLabel}{" "}
                                                    进入待办
                                                </p>
                                            ) : null}
                                        </div>
                                    ) : null}
                                </WorkspaceFactItem>
                                {parsedExtras.staticFacts.map((fact) => (
                                    <WorkspaceFactItem
                                        key={fact.label}
                                        label={fact.label}
                                        value={fact.value}
                                        numeric={fact.numeric}
                                        className={
                                            fact.value.length > 30 ||
                                            fact.value.includes("\n") ||
                                            /(?:说明|备注|地址)/.test(
                                                fact.label,
                                            )
                                                ? "col-span-full"
                                                : undefined
                                        }
                                    />
                                ))}
                            </dl>
                        </div>

                        <div
                            aria-label="来源销售单"
                            className="space-y-2 py-3.5"
                        >
                            <div className="flex items-center justify-between gap-2">
                                <h3 className="text-xs font-medium text-muted-foreground">
                                    来源销售单
                                </h3>
                                {sources.length > 1 ? (
                                    <span className="num text-xs text-muted-foreground">
                                        共 {sources.length} 笔
                                    </span>
                                ) : null}
                            </div>
                            <WorkspacePurchaseApprovalSources
                                instanceId={instanceId}
                                sources={sources}
                                orderComparisons={sourceComparison?.orders}
                                onPreview={onPreview}
                                onCompareSource={onCompareSource}
                            />

                            {extraCost && metrics.goodsLineCount > 0 ? (
                                <div className="flex items-center gap-2 rounded-md bg-info-soft/45 px-3 py-1.5 text-xs text-info-soft-foreground">
                                    <InfoIcon
                                        aria-hidden="true"
                                        className="size-3.5 shrink-0"
                                    />
                                    <span>
                                        来源销售单已有附加费用（本次提交之外）：已发生{" "}
                                        <MoneyValue
                                            display="exact"
                                            value={
                                                extraCost.occurred_gross_total
                                            }
                                            inheritColor
                                            className="font-medium"
                                        />
                                        {" · 待发生 "}
                                        <MoneyValue
                                            display="exact"
                                            value={
                                                extraCost.pending_gross_total
                                            }
                                            inheritColor
                                            className="font-medium"
                                        />
                                    </span>
                                </div>
                            ) : null}
                        </div>
                    </section>
                ) : null}

                <DetailWorkspaceSection
                    id={linesSectionId}
                    divider={false}
                    title="采购明细"
                    ariaLabel="采购明细"
                    subtitle={
                        lines
                            ? metrics.feeLineCount > 0 &&
                              metrics.goodsLineCount > 0
                                ? `共 ${lines.length} 行 · 含 ${metrics.feeLineCount} 项费用 · 金额含税`
                                : `共 ${lines.length} 行 · 金额含税`
                            : undefined
                    }
                    className="pt-4 pb-5"
                >
                    {lines?.length ? (
                        <WorkspacePurchaseComparisonTable
                            lines={lines}
                            evaluatedRows={evaluatedRows}
                            metrics={metrics}
                            sources={sources}
                            sourceComparison={sourceComparison}
                            uniqueDeliveryPointCount={uniqueDeliveryPointCount}
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
                            onInspectDeliveryLine={(lineId) =>
                                openDeliveryDialog(lineId)
                            }
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

                <WorkspacePurchaseDeliveryDialog
                    idPrefix={`${prefix}-delivery-dialog`}
                    open={deliveryDialogOpen}
                    onOpenChange={setDeliveryDialogOpen}
                    lines={lines ?? []}
                    fulfillment={fulfillment}
                    focusedLineId={focusedDeliveryLineId}
                    onClearFocus={() => setFocusedDeliveryLineId(null)}
                />
            </div>
        </TooltipProvider>
    )
}

function PurchaseKpiCell({
    label,
    basis,
    value,
    detail,
    emphasis = "default",
    tooltipId,
    tooltipLabel,
    tooltipContent,
}: {
    label: string
    basis?: string
    value: ReactNode
    detail?: ReactNode
    emphasis?: "primary" | "default"
    tooltipId?: string
    tooltipLabel?: string
    tooltipContent?: ReactNode
}) {
    return (
        <div className="min-w-0">
            <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                <span>{label}</span>
                {basis ? (
                    <span className="text-tiny text-muted-foreground/80">
                        {basis}
                    </span>
                ) : null}
                {tooltipContent && tooltipId ? (
                    <Tooltip>
                        <TooltipTrigger
                            render={
                                <button
                                    id={tooltipId}
                                    type="button"
                                    aria-label={tooltipLabel ?? `${label}说明`}
                                    className="inline-flex size-4 cursor-help items-center justify-center rounded-sm text-muted-foreground/80 transition-colors hover:text-foreground"
                                />
                            }
                        >
                            <InfoIcon aria-hidden="true" className="size-3" />
                        </TooltipTrigger>
                        <TooltipContent className="block">
                            {tooltipContent}
                        </TooltipContent>
                    </Tooltip>
                ) : null}
            </div>
            <div
                className={cn(
                    "num mt-1 max-w-full wrap-anywhere font-semibold tracking-tight text-foreground",
                    emphasis === "primary"
                        ? "text-value-section"
                        : "text-value-compact",
                )}
            >
                {value}
            </div>
            {detail ? (
                <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-xs text-muted-foreground">
                    {detail}
                </div>
            ) : null}
        </div>
    )
}
