"use client"

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import {
    ChevronRightIcon,
    CircleQuestionMarkIcon,
    MapPinIcon,
    PaperclipIcon,
    TriangleAlertIcon,
    XIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    MoneyValue,
    QuantityValue,
    RateValue,
    WorkspaceFactGrid,
    WorkspaceFactGroup,
    WorkspaceFactItem,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import type { ApprovalSalesSubmission } from "@/features/approval-workflow/api/materials"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import {
    approvalSalesTaxPoint,
    approvalSalesUnit,
    displayUnixDate,
    displayUnixSeconds,
} from "@/features/approval-workflow/display"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    buildDeliveryAllocationSummary,
    type DeliveryAllocationSummary,
} from "@/lib/delivery-allocation-summary"
import { displayName } from "@/lib/display-name"
import { cn } from "@/lib/utils"
import {
    assessSalesApprovalRisk,
    buildSalesApprovalMetrics,
    evaluateSalesApprovalLines,
    missingSalesApprovalEvidence,
    parseSalesResubmission,
    type SalesApprovalMetrics,
} from "../lib/sales-approval-model"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceApprovalResubmissionBanner } from "./workspace-approval-resubmission-banner"
import {
    SalesMarginValue,
    SalesVoucherDiscountValue,
    WorkspaceSalesComparisonTable,
    type SalesComparisonTableHandle,
} from "./workspace-sales-comparison-table"
import {
    SalesDeliveryViews,
    WorkspaceSalesDeliveryDialog,
} from "./workspace-sales-delivery-dialog"

export { assessSalesApprovalRisk }

/** 审批页、分栏检查器与办理抽屉共用本次提交的销售明细。 */
export function WorkspaceSalesApproval({
    instanceId,
    item,
    onSelectFile,
    onOpenMaterials,
    activeFileId,
    hideEmbeddedMaterials = false,
}: {
    instanceId: string
    item: WorkspaceWorkItem
    onSelectFile?: (fileId: string) => void
    onOpenMaterials?: () => void
    activeFileId?: string
    hideEmbeddedMaterials?: boolean
}) {
    const query = useApprovalMaterials(instanceId, true, item)
    const submission = query.data?.sales_order
    const attachments = query.data?.attachments ?? []
    const prefix = `workspace-sales-approval-${toAutomationIdSegment(instanceId)}`
    const isCard = submission?.business_type === "VOUCHER"
    const { names } = useProductUnitNames(
        submission?.lines.map((line) => line.sku_id),
        Boolean(!isCard && submission?.lines.length),
    )

    const tableHandle = useRef<SalesComparisonTableHandle>(null)
    const deliveryOpener = useRef<HTMLElement>(null)
    const pendingLineLocation = useRef<string>(null)
    const [riskOnly, setRiskOnly] = useState(false)
    const [deliveryOpen, setDeliveryOpen] = useState(false)
    const [deliveryInlineOpen, setDeliveryInlineOpen] = useState(false)
    const [focusedDeliveryLineId, setFocusedDeliveryLineId] = useState<
        string | null
    >(null)
    const [sectionPulse, setSectionPulse] = useState<{ id: string } | null>(
        null,
    )

    useEffect(() => {
        if (!sectionPulse) return
        const reducedMotion = window.matchMedia?.(
            "(prefers-reduced-motion: reduce)",
        ).matches
        const frame = window.requestAnimationFrame(() => {
            const target = document.getElementById(sectionPulse.id)
            target?.focus({ preventScroll: true })
            target?.scrollIntoView({
                behavior: reducedMotion ? "instant" : "smooth",
                block: "center",
            })
        })
        const timer = window.setTimeout(() => setSectionPulse(null), 2200)
        return () => {
            window.cancelAnimationFrame(frame)
            window.clearTimeout(timer)
        }
    }, [sectionPulse])

    const rows = useMemo(
        () =>
            evaluateSalesApprovalLines(submission, (line) =>
                approvalSalesUnit(line, isCard, names),
            ),
        [submission, isCard, names],
    )
    const metrics = useMemo(
        () => buildSalesApprovalMetrics(submission, rows),
        [submission, rows],
    )
    const deliverySummary = useMemo(
        () =>
            buildDeliveryAllocationSummary(
                rows.map(({ line, name, spec, unit }) => ({
                    id: line.id,
                    lineNo: line.line_no,
                    itemName: name,
                    specification: spec,
                    unit,
                    quantity:
                        line.quantity ??
                        (line.card_count == null
                            ? null
                            : String(line.card_count)),
                    allocations: line.delivery_allocations,
                })),
            ),
        [rows],
    )
    const deliveryItems = useMemo(
        () =>
            new Map(
                deliverySummary.itemGroups.map((group) => [
                    group.lineId,
                    group,
                ]),
            ),
        [deliverySummary.itemGroups],
    )

    if (query.isPending) {
        return (
            <DetailWorkspaceSection divider={false} ariaLabel="销售审批资料">
                <p
                    role="status"
                    className="flex items-center gap-2 text-xs text-muted-foreground"
                >
                    <Spinner />
                    正在读取销售审批资料…
                </p>
            </DetailWorkspaceSection>
        )
    }

    if (query.isError) {
        return (
            <DetailWorkspaceSection divider={false} ariaLabel="销售审批资料">
                <BusinessFailureState
                    id={`${prefix}-retry`}
                    title="销售审批资料读取失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            </DetailWorkspaceSection>
        )
    }

    if (!submission || !metrics) {
        return (
            <DetailWorkspaceSection
                divider={false}
                title="销售明细"
                ariaLabel="销售明细"
            >
                <Alert variant="warning">
                    <AlertDescription>
                        本次提交未保留完整销售明细。
                    </AlertDescription>
                </Alert>
            </DetailWorkspaceSection>
        )
    }

    const resubmission = parseSalesResubmission(
        query.data.display.source.extra_sections,
        item,
    )
    const risk = assessSalesApprovalRisk(submission)
    const evidenceMissing = missingSalesApprovalEvidence(
        submission,
        attachments.length,
    )
    const riskCounts = [
        metrics.negativeMarginCount
            ? `负毛利 ${metrics.negativeMarginCount}`
            : null,
        metrics.belowFloorCount
            ? `低于原底价 ${metrics.belowFloorCount}`
            : null,
        metrics.priceAdjustedCount
            ? `供给价调整 ${metrics.priceAdjustedCount}`
            : null,
        metrics.otherPriceRiskCount
            ? `价格格式待核 ${metrics.otherPriceRiskCount}`
            : null,
    ]
        .filter(Boolean)
        .join(" · ")
    const riskSummary = [
        metrics.riskLineCount
            ? `${metrics.riskLineCount} 项商品风险（${riskCounts}）`
            : null,
        evidenceMissing ? "整单依据 1 项（无合同且无凭证附件）" : null,
    ]
        .filter(Boolean)
        .join(" · ")

    const commercialFacts = [
        {
            label: "客户",
            value:
                displayName(submission.customer_name) ??
                displayName(query.data.display.source.customer) ??
                "未记录客户名称",
        },
        {
            label: "结算主体",
            value: displayName(submission.settlement_party_name),
        },
        { label: "项目", value: displayName(submission.project_name) },
        {
            label: "合同",
            value: displayName(submission.contract_no),
            numeric: true,
            selectable: true,
        },
        {
            label: "申请人",
            value: displayName(
                query.data.display.source.submitter_name,
                submission.submitted_by,
            ),
        },
        {
            label: "提交时间",
            value: displayUnixSeconds(submission.submitted_at)?.label,
            numeric: true,
        },
        { label: "业务备注", value: submission.business_remark?.trim() },
    ].filter(({ value }) => Boolean(value && value !== "—"))
    const materialsSectionId = `${prefix}-materials-section`
    const inlineDeliverySectionId = `${prefix}-delivery-inline-section`
    const canOpenMaterials = Boolean(onOpenMaterials || !hideEmbeddedMaterials)
    const showAttachmentShortcut = attachments.length > 0 && canOpenMaterials

    const openMaterialsShortcut = () => {
        if (onOpenMaterials) {
            onOpenMaterials()
            return
        }
        setSectionPulse({ id: materialsSectionId })
    }

    const openDelivery = (lineId: string | null, trigger: HTMLElement) => {
        deliveryOpener.current = trigger
        setFocusedDeliveryLineId(lineId)
        if (trigger.closest('[data-slot="sheet-content"]')) {
            setDeliveryInlineOpen(true)
            setSectionPulse({ id: inlineDeliverySectionId })
            return
        }
        setDeliveryOpen(true)
    }

    return (
        <div
            data-slot="workspace-sales-approval"
            className="@container/sales-lines flex min-w-0 flex-col"
        >
            {resubmission || riskSummary ? (
                <div className="space-y-2 pt-3">
                    {resubmission ? (
                        <WorkspaceApprovalResubmissionBanner
                            roundNo={resubmission.roundNo}
                            resubmissionReason={resubmission.resubmissionReason}
                            diffBaseline={resubmission.diffBaseline}
                            diffSummary={resubmission.diffSummary}
                            diffs={resubmission.diffs}
                            previousRejection={resubmission.previousRejection}
                        />
                    ) : null}
                    {riskSummary ? (
                        <div
                            role="alert"
                            data-slot="sales-approval-risk-strip"
                            title={
                                risk.reasons.length
                                    ? risk.reasons.join("；")
                                    : undefined
                            }
                            className={cn(
                                "flex flex-wrap items-center justify-between gap-x-3 gap-y-1 rounded-md border-l-2 px-3 py-1.5 text-xs",
                                risk.highRisk
                                    ? "border-l-destructive bg-destructive-soft/65 text-destructive-soft-foreground"
                                    : "border-l-warning bg-warning-soft/75 text-warning-soft-foreground",
                            )}
                        >
                            <div className="flex min-w-0 flex-1 items-start gap-2">
                                <TriangleAlertIcon
                                    aria-hidden="true"
                                    className="mt-0.5 size-3.5 shrink-0"
                                />
                                <p className="num min-w-0 wrap-anywhere">
                                    <span className="font-medium">
                                        {risk.highRisk ? "高危复核" : "需复核"}
                                    </span>
                                    <span
                                        aria-hidden="true"
                                        className="mx-1.5 opacity-60"
                                    >
                                        ·
                                    </span>
                                    <span>{riskSummary}</span>
                                </p>
                            </div>
                            <div className="flex shrink-0 flex-wrap items-center gap-1">
                                {metrics.riskLineCount > 0 && !riskOnly ? (
                                    <Button
                                        id={`${prefix}-alert-filter-risk`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className={cn(
                                            "num h-control-xs shrink-0 gap-0.5 px-1.5 text-xs font-medium shadow-none",
                                            risk.highRisk
                                                ? "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                                : "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
                                        )}
                                        onClick={() =>
                                            tableHandle.current?.filterToRisks()
                                        }
                                    >
                                        仅看风险项（{metrics.riskLineCount}）
                                        <ChevronRightIcon aria-hidden="true" />
                                    </Button>
                                ) : null}
                                {evidenceMissing && canOpenMaterials ? (
                                    <Button
                                        id={`${prefix}-alert-open-materials`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className={cn(
                                            "h-control-xs shrink-0 gap-0.5 px-1.5 text-xs font-medium shadow-none",
                                            risk.highRisk
                                                ? "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                                : "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
                                        )}
                                        onClick={openMaterialsShortcut}
                                    >
                                        核对凭证附件
                                        <ChevronRightIcon aria-hidden="true" />
                                    </Button>
                                ) : null}
                            </div>
                        </div>
                    ) : null}
                </div>
            ) : null}

            <SalesDecisionBand
                idPrefix={prefix}
                submission={submission}
                metrics={metrics}
                deliverySummary={deliverySummary}
                onOpenDelivery={(trigger) => openDelivery(null, trigger)}
            />

            <WorkspaceFactGroup
                ariaLabel="销售单商业信息"
                className="pt-3.5 pb-1"
                actions={
                    showAttachmentShortcut ? (
                        <Button
                            id={`${prefix}-kpi-open-materials`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={cn(
                                "num",
                                detailWorkspaceStyles.subtleButton,
                            )}
                            onClick={openMaterialsShortcut}
                        >
                            <PaperclipIcon
                                aria-hidden="true"
                                data-icon="inline-start"
                            />
                            附件（{attachments.length} 份）
                        </Button>
                    ) : undefined
                }
            >
                <WorkspaceFactGrid columns={3}>
                    {commercialFacts.map(
                        ({ label, value, numeric, selectable }) => {
                            const text = value ?? ""
                            const isLong =
                                text.length > 30 ||
                                text.includes("\n") ||
                                /(?:说明|备注|地址)/.test(label)
                            return (
                                <WorkspaceFactItem
                                    key={label}
                                    label={label}
                                    value={text}
                                    numeric={numeric}
                                    selectable={selectable}
                                    span={isLong ? "full" : 1}
                                />
                            )
                        },
                    )}
                </WorkspaceFactGrid>
            </WorkspaceFactGroup>

            <WorkspaceSalesComparisonTable
                rows={rows}
                metrics={metrics}
                grossTotal={submission.gross_amount}
                deliveryItems={deliveryItems}
                showLineDelivery={!deliverySummary.isUniformSingleAddress}
                idPrefix={prefix}
                riskOnly={riskOnly}
                onRiskOnlyChange={setRiskOnly}
                tableHandle={tableHandle}
                onOpenLineDelivery={openDelivery}
            />

            {deliverySummary.uniqueAddressCount > 0 && deliveryInlineOpen ? (
                <DetailWorkspaceSection
                    id={inlineDeliverySectionId}
                    tabIndex={-1}
                    title="配送地址明细"
                    subtitle={
                        <span className="num">
                            共 {deliverySummary.uniqueAddressCount} 处收货地址 ·{" "}
                            {deliverySummary.totalRows} 条分配
                        </span>
                    }
                    ariaLabel="配送地址明细"
                    actions={
                        <Button
                            id={`${prefix}-delivery-inline-close`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={detailWorkspaceStyles.quietButton}
                            onClick={() => {
                                setDeliveryInlineOpen(false)
                                setFocusedDeliveryLineId(null)
                                deliveryOpener.current?.focus()
                            }}
                        >
                            <XIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            收起
                        </Button>
                    }
                    className={cn(
                        "scroll-mt-16 rounded-md transition-colors",
                        sectionPulse?.id === inlineDeliverySectionId &&
                            "bg-info-soft/25 ring-1 ring-info-border motion-safe:animate-pulse",
                    )}
                >
                    <SalesDeliveryViews
                        idPrefix={`${prefix}-delivery`}
                        summary={deliverySummary}
                        focusedLineId={focusedDeliveryLineId}
                        onClearFocus={() => setFocusedDeliveryLineId(null)}
                        onLocateLine={(lineId) => {
                            tableHandle.current?.locateLine(lineId)
                        }}
                    />
                </DetailWorkspaceSection>
            ) : null}

            {deliverySummary.uniqueAddressCount > 0 ? (
                <WorkspaceSalesDeliveryDialog
                    idPrefix={`${prefix}-delivery`}
                    summary={deliverySummary}
                    open={deliveryOpen}
                    onOpenChange={setDeliveryOpen}
                    onCloseComplete={() => {
                        const lineId = pendingLineLocation.current
                        pendingLineLocation.current = null
                        if (lineId) tableHandle.current?.locateLine(lineId)
                    }}
                    returnFocus={deliveryOpener}
                    focusedLineId={focusedDeliveryLineId}
                    onClearFocus={() => setFocusedDeliveryLineId(null)}
                    onLocateLine={(lineId) => {
                        pendingLineLocation.current = lineId
                        setDeliveryOpen(false)
                    }}
                />
            ) : null}

            {!hideEmbeddedMaterials ? (
                <DetailWorkspaceSection
                    id={materialsSectionId}
                    tabIndex={-1}
                    title="合同、凭证及附件"
                    subtitle={
                        attachments.length
                            ? `共 ${attachments.length} 个文件`
                            : undefined
                    }
                    ariaLabel="合同、凭证及附件"
                    className={cn(
                        "scroll-mt-16 rounded-md transition-colors",
                        sectionPulse?.id === materialsSectionId &&
                            "bg-info-soft/25 ring-1 ring-info-border motion-safe:animate-pulse",
                    )}
                >
                    <ApprovalMaterialFiles
                        instanceId={instanceId}
                        attachments={attachments}
                        idPrefix={`${prefix}-file`}
                        emptyMessage="本次提交未附带合同或凭证文件。"
                        onSelectFile={onSelectFile}
                        activeFileId={activeFileId}
                    />
                </DetailWorkspaceSection>
            ) : null}
        </div>
    )
}

function SalesDecisionBand({
    idPrefix,
    submission,
    metrics,
    deliverySummary,
    onOpenDelivery,
}: {
    idPrefix: string
    submission: ApprovalSalesSubmission
    metrics: SalesApprovalMetrics
    deliverySummary: DeliveryAllocationSummary
    onOpenDelivery: (trigger: HTMLElement) => void
}) {
    const { isCard, estimatedMargin, overallVoucherDiscount } = metrics
    const paymentTerm = displayName(submission.payment_term_name)
    const invoiceType = displayName(submission.invoice_type)
    const taxPoint = approvalSalesTaxPoint(submission.tax_point)
    const decimalTaxPoint = /^\d+(?:\.\d+)?%$/.test(taxPoint)
        ? taxPoint.slice(0, -1)
        : null
    const lastDeliveryDate = displayUnixDate(
        submission.lines.reduce<number | null>(
            (latest, line) =>
                line.fulfillment_due_at != null &&
                (latest == null || line.fulfillment_due_at > latest)
                    ? line.fulfillment_due_at
                    : latest,
            null,
        ),
    )
    const voucherExpiryDate = displayUnixDate(submission.voucher_expiry_at)
    const fulfillmentDate = isCard ? voucherExpiryDate : lastDeliveryDate

    return (
        <TooltipProvider>
            <div
                role="region"
                aria-label="销售审批关键决策指标"
                className="mt-3 grid min-w-0 grid-cols-1 gap-x-6 gap-y-3.5 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/sales-lines:grid-cols-2 @[840px]/sales-lines:grid-cols-4 [&_[data-slot=money-value]]:max-w-full [&_[data-slot=money-value-number]]:wrap-anywhere [&_[data-slot=money-value-number]]:whitespace-normal"
            >
                <div className="min-w-0">
                    <SalesKpiHeading
                        label="含税总额"
                        basis="整单成交"
                        tooltipId={`${idPrefix}-kpi-gross-info`}
                        tooltip={
                            <div className="num space-y-0.5">
                                <p>含税总额＝不含税净额＋税额</p>
                                <p>
                                    <MoneyValue
                                        display="exact"
                                        value={submission.net_amount}
                                        inheritColor
                                    />
                                    {" ＋ "}
                                    <MoneyValue
                                        display="exact"
                                        value={submission.tax_amount}
                                        inheritColor
                                    />
                                    {" ＝ "}
                                    <MoneyValue
                                        display="exact"
                                        value={submission.gross_amount}
                                        inheritColor
                                    />
                                </p>
                            </div>
                        }
                    />
                    <div className="num mt-1 text-value-section font-semibold tracking-tight">
                        <MoneyValue
                            display="exact"
                            value={submission.gross_amount}
                            className="font-semibold"
                        />
                    </div>
                    <p className="mt-1 flex flex-wrap gap-x-2 gap-y-0.5 text-tiny text-muted-foreground">
                        <span>
                            净额{" "}
                            <MoneyValue
                                display="exact"
                                value={submission.net_amount}
                                inheritColor
                            />
                        </span>
                        <span>
                            税额{" "}
                            <MoneyValue
                                display="exact"
                                value={submission.tax_amount}
                                inheritColor
                            />
                        </span>
                    </p>
                </div>

                <div className="min-w-0">
                    <SalesKpiHeading
                        label={isCard ? "面额折率" : "预估毛利"}
                        basis={isCard ? "成交 ÷ 面额" : "含税收入 − 供给成本"}
                        tooltipId={`${idPrefix}-kpi-margin-info`}
                        tooltip={
                            isCard ? (
                                <div className="num space-y-0.5">
                                    <p>面额折率＝销售含税金额 ÷ 卡券总面额</p>
                                    <p>总面额＝各行单张面额 × 张数后求和</p>
                                    <p>整单折率需要全部商品的面额和张数。</p>
                                    <p>溢价显示售价占面额的百分比。</p>
                                </div>
                            ) : (
                                <div className="num space-y-0.5">
                                    <p>
                                        毛利额＝已核算销售行含税小计 −
                                        含税供给成本
                                    </p>
                                    <p>
                                        成本按供给价 ×
                                        数量汇总，含已记录的赠品成本。
                                    </p>
                                    <p>毛利率＝毛利额 ÷ 已核算销售行含税小计</p>
                                    {metrics.coveredRevenue != null &&
                                    metrics.estimatedCostTotal != null ? (
                                        <p>
                                            <MoneyValue
                                                display="exact"
                                                value={metrics.coveredRevenue}
                                                inheritColor
                                            />
                                            {" − "}
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    metrics.estimatedCostTotal
                                                }
                                                inheritColor
                                            />
                                        </p>
                                    ) : null}
                                    {metrics.giftCostTotal != null ? (
                                        <p>
                                            其中赠品成本{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={metrics.giftCostTotal}
                                                inheritColor
                                            />
                                        </p>
                                    ) : null}
                                </div>
                            )
                        }
                    />
                    <div className="num mt-1 text-value-compact font-semibold tracking-tight wrap-anywhere">
                        {isCard && overallVoucherDiscount ? (
                            <SalesVoucherDiscountValue
                                discount={overallVoucherDiscount}
                            />
                        ) : estimatedMargin ? (
                            <SalesMarginValue
                                margin={estimatedMargin}
                                className="justify-start"
                                rateClassName="text-xs font-medium"
                            />
                        ) : (
                            <span className="text-body-compact font-medium text-muted-foreground">
                                暂无法核算
                            </span>
                        )}
                    </div>
                    <p className="num mt-1 flex flex-wrap gap-x-2 gap-y-0.5 text-tiny text-muted-foreground">
                        {isCard ? (
                            <>
                                {metrics.totalFaceValue != null ? (
                                    <span>
                                        已记录面额{" "}
                                        <MoneyValue
                                            display="exact"
                                            value={metrics.totalFaceValue}
                                            inheritColor
                                        />
                                    </span>
                                ) : null}
                                {metrics.totalCards != null ? (
                                    <QuantityValue
                                        value={metrics.totalCards}
                                        unit="张"
                                    />
                                ) : null}
                                <span
                                    className={cn(
                                        metrics.voucherFaceValueCoveredCount <
                                            metrics.lineCount &&
                                            "text-warning-soft-foreground",
                                    )}
                                >
                                    面额覆盖{" "}
                                    {metrics.voucherFaceValueCoveredCount}/
                                    {metrics.lineCount} 项
                                </span>
                            </>
                        ) : (
                            <>
                                <span
                                    className={cn(
                                        metrics.partialCostCoverage &&
                                            "text-warning-soft-foreground",
                                    )}
                                >
                                    成本覆盖 {metrics.costCoveredCount}/
                                    {metrics.costExpectedCount} 项销售明细
                                </span>
                                {metrics.giftCostTotal != null ? (
                                    <span>含已记录赠品成本</span>
                                ) : null}
                            </>
                        )}
                    </p>
                </div>

                <div className="min-w-0">
                    <SalesKpiHeading label="回款与票税" />
                    <p className="mt-1 text-body-compact font-semibold wrap-anywhere">
                        {paymentTerm ?? "未记录结算条件"}
                    </p>
                    <p className="num mt-1 flex flex-wrap gap-x-2 gap-y-0.5 text-tiny text-muted-foreground">
                        {submission.receivable_due_date ? (
                            <span>
                                收款到期 {submission.receivable_due_date}
                            </span>
                        ) : null}
                        {invoiceType ? <span>{invoiceType}</span> : null}
                        {taxPoint !== "—" ? (
                            <span>
                                约定税点{" "}
                                {decimalTaxPoint === null ? (
                                    taxPoint
                                ) : (
                                    <RateValue
                                        value={decimalTaxPoint}
                                        precision={2}
                                        className="text-muted-foreground"
                                    />
                                )}
                            </span>
                        ) : null}
                    </p>
                </div>

                <div className="min-w-0">
                    <SalesKpiHeading label="履约与配送" />
                    <p className="num mt-1 text-body-compact font-semibold wrap-anywhere">
                        {fulfillmentDate === "—" ? (
                            isCard ? (
                                "卡券交付"
                            ) : (
                                "未记录交付期限"
                            )
                        ) : (
                            <>
                                {isCard ? "卡券履约期限" : "最晚交付"}{" "}
                                {fulfillmentDate}
                            </>
                        )}
                    </p>
                    {deliverySummary.uniqueAddressCount > 0 ? (
                        <div className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5">
                            <button
                                id={`${idPrefix}-delivery-open`}
                                type="button"
                                aria-haspopup="dialog"
                                className="num inline-flex cursor-pointer items-center gap-1 text-left text-xs font-medium text-primary hover:underline"
                                onClick={(event) =>
                                    onOpenDelivery(event.currentTarget)
                                }
                            >
                                <MapPinIcon
                                    aria-hidden="true"
                                    className="size-3 shrink-0"
                                />
                                配送地址（{deliverySummary.uniqueAddressCount}{" "}
                                处）
                            </button>
                            {!isCard &&
                            deliverySummary.unassignedLineCount > 0 ? (
                                <span className="num text-tiny text-muted-foreground">
                                    {deliverySummary.unassignedLineCount}{" "}
                                    项未指定地址
                                </span>
                            ) : null}
                        </div>
                    ) : !isCard ? (
                        <p className="mt-1 text-tiny text-muted-foreground">
                            未记录配送地址
                        </p>
                    ) : null}
                </div>
            </div>
        </TooltipProvider>
    )
}

function SalesKpiHeading({
    label,
    basis,
    tooltipId,
    tooltip,
}: {
    label: string
    basis?: string
    tooltipId?: string
    tooltip?: ReactNode
}) {
    return (
        <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
            <span>{label}</span>
            {basis ? (
                <span className="text-tiny text-muted-foreground/80">
                    {basis}
                </span>
            ) : null}
            {tooltip && tooltipId ? (
                <Tooltip>
                    <TooltipTrigger
                        render={
                            <button
                                id={tooltipId}
                                type="button"
                                aria-label={`${label}口径说明`}
                                className="inline-flex size-4 cursor-help items-center justify-center rounded-sm text-muted-foreground/80 transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-primary"
                            />
                        }
                    >
                        <CircleQuestionMarkIcon
                            aria-hidden="true"
                            className="size-3"
                        />
                    </TooltipTrigger>
                    <TooltipContent className="block">{tooltip}</TooltipContent>
                </Tooltip>
            ) : null}
        </div>
    )
}
