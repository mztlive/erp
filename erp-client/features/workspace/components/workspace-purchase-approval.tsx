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
    FileTextIcon,
    InfoIcon,
    MapPinIcon,
    PaperclipIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    MoneyValue,
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
import { WorkspaceSupplierRating } from "./workspace-supplier-rating"

export { purchaseApprovalRiskReasons }

export interface WorkspacePurchaseApprovalHandle {
    locateLine: (lineId: string) => void
}

function purchaseApprovalPrefix(instanceId: string) {
    return `workspace-purchase-approval-${toAutomationIdSegment(instanceId)}`
}

export function WorkspacePurchaseApprovalSources({
    instanceId,
    sources,
    onPreview,
    onCompareSource,
}: {
    instanceId: string
    sources: PurchaseSource[]
    onPreview: (target: WorkspacePaperTarget) => void
    onCompareSource?: (target: PurchaseSourceComparisonTarget) => void
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
    approvalHandleRef,
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
    onCompareSource?: (target: PurchaseSourceComparisonTarget) => void
    approvalHandleRef?: RefObject<WorkspacePurchaseApprovalHandle | null>
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

    const uniqueDeliveryPointCount = useMemo(
        () =>
            countUniqueDeliveryPoints(
                (lines ?? []).map((line) => line.delivery_allocations),
            ),
        [lines],
    )

    const materialsSectionId = `${prefix}-materials-section`
    const linesSectionId = `${prefix}-lines-section`

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
    }))

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
    const isSevereRisk =
        metrics.externalLossCount > 0 ||
        metrics.quantityExcessCount > 0 ||
        metrics.mismatchCount > 0

    const riskChipsSummary = [
        metrics.externalLossCount > 0
            ? `对外负毛利 ${metrics.externalLossCount}`
            : null,
        metrics.quantityExcessCount > 0
            ? `超需求 ${metrics.quantityExcessCount}`
            : null,
        metrics.mismatchCount > 0 ? `品规差异 ${metrics.mismatchCount}` : null,
        metrics.costAboveSupplyCount > 0
            ? `采购倒挂 ${metrics.costAboveSupplyCount}`
            : null,
        metrics.delayCount > 0 ? `交期晚 ${metrics.delayCount}` : null,
    ]
        .filter(Boolean)
        .join(" · ")

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
        <div
            data-slot="workspace-purchase-approval"
            className="@container/purchase-lines flex min-w-0 flex-col"
        >
            {parsedExtras.resubmission || riskReasons.length > 0 ? (
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
                            diffSummary={parsedExtras.resubmission.diffSummary}
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
                                        {isSevereRisk ? "高危复核" : "需复核"}
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
                        </div>
                    ) : null}
                </div>
            ) : null}

            <TooltipProvider>
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
                                basis="销售 − 采购"
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
                                                对应实际采购成本（含税）
                                            </div>
                                            {metrics.correspondingSalesTotal ? (
                                                <div>
                                                    对应对外销售总额：
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            metrics.correspondingSalesTotal
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
                                            <div className="text-background/75">
                                                税后口径可在右侧「采销对照」查看
                                            </div>
                                        </div>
                                    ) : undefined
                                }
                                value={
                                    metrics.externalGrossMargin != null ? (
                                        <span className="inline-flex flex-wrap items-baseline gap-x-2">
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
                                        </span>
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
                                                        {metrics.goodsLineCount}{" "}
                                                        项
                                                    </span>
                                                </>
                                            ) : null}
                                        </>
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
                            </span>
                        }
                        detail={
                            <>
                                {fulfillmentModeLabel ? (
                                    <span>{fulfillmentModeLabel}</span>
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
            </TooltipProvider>

            {submission ? (
                <section
                    aria-label="采购单商业与来源信息"
                    className="space-y-2 pt-3.5 pb-1"
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

                    <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1 text-xs">
                        <span className="shrink-0 text-muted-foreground">
                            来源销售单
                        </span>
                        <WorkspacePurchaseApprovalSources
                            instanceId={instanceId}
                            sources={sources}
                            onPreview={onPreview}
                            onCompareSource={onCompareSource}
                        />
                    </div>

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
