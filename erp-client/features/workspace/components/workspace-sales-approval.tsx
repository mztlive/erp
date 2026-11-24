"use client"

import { useEffect, useMemo, useRef, useState } from "react"
import {
    FileTextIcon,
    MapPinIcon,
    MessageSquareTextIcon,
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
    QuantityValue,
    RateValue,
    detailWorkspaceStyles,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import {
    approvalSalesTaxPoint,
    approvalSalesUnit,
    displayReadableName,
    displayUnixDate,
    displayUnixSeconds,
} from "@/features/approval-workflow/display"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { useProductUnitNames } from "@/hooks/use-product-unit-names"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fullDeliveryAddress } from "@/lib/delivery-address"
import {
    buildDeliveryAllocationSummary,
    formatLineDeliveryHint,
    type DeliveryAllocationLineInput,
} from "@/lib/delivery-allocation-summary"
import { displayBusinessText, displayName } from "@/lib/display-name"
import { multiplyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import {
    assessSalesApprovalRisk,
    buildSalesApprovalMetrics,
    evaluateSalesApprovalLines,
    parseSalesResubmission,
    type EvaluatedSalesLine,
} from "../lib/sales-approval-model"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceApprovalResubmissionBanner } from "./workspace-approval-resubmission-banner"
import {
    APPROVAL_TABLE_DEFAULT_PAGE_SIZE,
    ApprovalLineFilterBar,
    ApprovalLinePagination,
} from "./workspace-approval-table-controls"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"

export { assessSalesApprovalRisk }

const dateLabel = (value: number | null) => displayUnixDate(value)

/** 全部明细与纸质预览读取同一审批提交，不回退到当前销售单。 */
export function WorkspaceSalesApproval({
    instanceId,
    item,
    onPreview,
    onSelectFile,
    onOpenMaterials,
    activeFileId,
    hideEmbeddedMaterials = false,
}: {
    instanceId: string
    item: WorkspaceWorkItem
    onPreview: (target: WorkspacePaperTarget) => void
    onSelectFile?: (fileId: string) => void
    onOpenMaterials?: () => void
    activeFileId?: string
    hideEmbeddedMaterials?: boolean
}) {
    const query = useApprovalMaterials(instanceId, true, item)
    const submission = query.data?.sales_order
    const attachments = query.data?.attachments ?? []
    const prefix = `workspace-sales-approval-${toAutomationIdSegment(instanceId)}`
    const risk = assessSalesApprovalRisk(submission)
    const isCard = submission?.business_type === "VOUCHER"
    const { names } = useProductUnitNames(
        submission?.lines.map((line) => line.sku_id),
        Boolean(!isCard && submission?.lines.length),
    )

    const [search, setSearch] = useState("")
    const [riskOnly, setRiskOnly] = useState(false)
    const [page, setPage] = useState(1)
    const [pageSize, setPageSize] = useState(APPROVAL_TABLE_DEFAULT_PAGE_SIZE)
    const [focusedDeliveryLineId, setFocusedDeliveryLineId] = useState<
        string | null
    >(null)
    const [highlightedLineId, setHighlightedLineId] = useState<string | null>(
        null,
    )
    const highlightTimerRef = useRef<number | null>(null)

    useEffect(() => {
        return () => {
            if (highlightTimerRef.current != null) {
                window.clearTimeout(highlightTimerRef.current)
            }
        }
    }, [])

    const deliveryLines = useMemo<
        readonly DeliveryAllocationLineInput[]
    >(() => {
        if (!submission?.lines.length) return []
        return submission.lines.map((line) => ({
            id: line.id,
            lineNo: line.line_no,
            itemName:
                displayReadableName(line.item_name_snapshot, line.sku_id) ||
                "未记录商品名称",
            specification: displayBusinessText(line.spec_snapshot, line.sku_id),
            unit: approvalSalesUnit(line, isCard, names),
            quantity:
                line.quantity ??
                (line.card_count == null ? null : String(line.card_count)),
            allocations: line.delivery_allocations,
        }))
    }, [isCard, names, submission?.lines])

    const deliverySummary = useMemo(
        () => buildDeliveryAllocationSummary(deliveryLines),
        [deliveryLines],
    )
    const deliveryItemById = useMemo(
        () =>
            new Map(
                deliverySummary.itemGroups.map((group) => [
                    group.lineId,
                    group,
                ]),
            ),
        [deliverySummary.itemGroups],
    )

    const evaluatedLines = useMemo(
        () =>
            evaluateSalesApprovalLines(submission, (line) =>
                approvalSalesUnit(line, isCard, names),
            ),
        [isCard, names, submission],
    )

    const metrics = useMemo(
        () => buildSalesApprovalMetrics(submission, evaluatedLines),
        [evaluatedLines, submission],
    )

    const resubmission = useMemo(
        () =>
            query.data
                ? parseSalesResubmission(
                      query.data.display.source.extra_sections,
                      item,
                  )
                : null,
        [item, query.data],
    )

    const needle = search.trim().toLocaleLowerCase()
    const filteredLines = useMemo(() => {
        return evaluatedLines.filter((entry) => {
            if (riskOnly && !entry.hasRisk) return false
            if (!needle) return true
            const baseText = [
                String(entry.line.line_no),
                entry.name,
                entry.spec,
                entry.welfare,
            ]
                .filter(Boolean)
                .join(" ")
                .toLocaleLowerCase()
            if (baseText.includes(needle)) return true
            return (entry.line.delivery_allocations ?? []).some((row) =>
                `${row.recipient} ${row.phone} ${fullDeliveryAddress(row)}`
                    .toLocaleLowerCase()
                    .includes(needle),
            )
        })
    }, [evaluatedLines, needle, riskOnly])

    const pageCount = Math.max(1, Math.ceil(filteredLines.length / pageSize))
    const currentPage = Math.min(page, pageCount)
    const visibleLines = filteredLines.slice(
        (currentPage - 1) * pageSize,
        currentPage * pageSize,
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
        const index = filteredLines.findIndex(
            (entry) => entry.line.id === lineId,
        )
        if (index >= 0) {
            const targetPage = Math.floor(index / pageSize) + 1
            if (targetPage !== currentPage) setPage(targetPage)
        } else {
            setRiskOnly(false)
            setSearch("")
            const rawIndex = evaluatedLines.findIndex(
                (entry) => entry.line.id === lineId,
            )
            if (rawIndex >= 0) {
                setPage(Math.floor(rawIndex / pageSize) + 1)
            }
        }
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
        setRiskOnly(true)
        setSearch("")
        setPage(1)
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

    const commercialFacts = useMemo(() => {
        if (!submission) return []
        const submitter = displayName(
            query.data?.display.source.submitter_name,
            submission.submitted_by,
        )
        const submittedAt = displayUnixSeconds(submission.submitted_at)?.label
        const items: Array<{
            label: string
            value: string
            numeric?: boolean
        }> = [
            {
                label: "业务性质",
                value: isCard ? "卡券" : "实物与服务",
            },
        ]
        const push = (
            label: string,
            value: string | null | undefined,
            numeric = false,
        ) => {
            const trimmed = value?.trim()
            if (!trimmed || trimmed === "—") return
            items.push({ label, value: trimmed, numeric })
        }
        push("结算主体", displayName(submission.settlement_party_name))
        push("项目", displayName(submission.project_name))
        push("开票要求", submission.invoice_type)
        push("约定税点", approvalSalesTaxPoint(submission.tax_point), true)
        push("收款到期日", submission.receivable_due_date, true)
        if (isCard && submission.voucher_expiry_at != null) {
            push("卡券履约期限", dateLabel(submission.voucher_expiry_at), true)
        }
        push("申请人", submitter)
        push("提交时间", submittedAt, true)
        return items
    }, [isCard, query.data?.display.source.submitter_name, submission])

    const showDeliverySection = Boolean(
        submission && (!isCard || deliverySummary.totalRows > 0),
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
                title="销售明细"
                ariaLabel="销售明细"
                actions={
                    <Button
                        id={`${prefix}-preview`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        onClick={() =>
                            onPreview({
                                kind: "approval_snapshot",
                                objectId: instanceId,
                                title: query.data?.document_no,
                            })
                        }
                    >
                        <FileTextIcon aria-hidden="true" />
                        预览完整销售单
                    </Button>
                }
            >
                <Alert variant="warning">
                    <AlertDescription>
                        本次提交未保留完整销售明细，请查看提交资料及附件核对。
                    </AlertDescription>
                </Alert>
            </DetailWorkspaceSection>
        )
    }

    const contractNo = displayName(submission.contract_no)

    return (
        <div
            data-slot="workspace-sales-approval"
            className="@container/sales-lines flex min-w-0 flex-col"
        >
            {resubmission || risk.highRisk ? (
                <div className="space-y-2.5 pt-3.5">
                    {resubmission ? (
                        <WorkspaceApprovalResubmissionBanner
                            roundNo={resubmission.roundNo}
                            resubmissionReason={resubmission.resubmissionReason}
                            previousRejection={resubmission.previousRejection}
                        />
                    ) : null}

                    {risk.highRisk ? (
                        <Alert variant="destructive" role="alert">
                            <TriangleAlertIcon aria-hidden="true" />
                            <AlertTitle className="flex flex-wrap items-center justify-between gap-2">
                                <span>销售价格需重点复核</span>
                                {metrics.riskLineCount > 0 && !riskOnly ? (
                                    <Button
                                        id={`${prefix}-alert-filter-risk`}
                                        type="button"
                                        variant="outline"
                                        size="xs"
                                        className="h-control-xs border-destructive-border bg-card/80 px-2 text-xs text-destructive-soft-foreground hover:bg-card"
                                        onClick={handleFilterToRisks}
                                    >
                                        仅看风险项（{metrics.riskLineCount}）
                                    </Button>
                                ) : null}
                            </AlertTitle>
                            <AlertDescription>
                                {risk.reasons.join("；")}
                            </AlertDescription>
                        </Alert>
                    ) : null}
                </div>
            ) : null}

            <div className="border-b border-border/60">
                <DetailWorkspaceMetrics
                    columns={4}
                    variant="bar"
                    ariaLabel="销售审批关键决策指标"
                >
                    <DetailWorkspaceMetricItem
                        label="含税总额"
                        emphasis="primary"
                        value={
                            <MoneyValue
                                display="exact"
                                value={submission.gross_amount}
                            />
                        }
                        detail={
                            <>
                                <span>
                                    不含税{" "}
                                    <MoneyValue
                                        display="exact"
                                        value={submission.net_amount}
                                    />
                                </span>
                                <span aria-hidden="true">·</span>
                                <span>
                                    税{" "}
                                    <MoneyValue
                                        display="exact"
                                        value={submission.tax_amount}
                                    />
                                </span>
                            </>
                        }
                    />

                    {isCard ? (
                        <DetailWorkspaceMetricItem
                            label="面额折率"
                            value={metrics.overallVoucherDiscount?.label ?? "—"}
                            detail={
                                metrics.totalFaceValue ? (
                                    <>
                                        <span>
                                            总面额{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={metrics.totalFaceValue}
                                            />
                                        </span>
                                        {metrics.totalCards ? (
                                            <>
                                                <span aria-hidden="true">
                                                    ·
                                                </span>
                                                <span className="num">
                                                    共 {metrics.totalCards} 张
                                                </span>
                                            </>
                                        ) : null}
                                    </>
                                ) : (
                                    "未记录卡券面额"
                                )
                            }
                        />
                    ) : (
                        <DetailWorkspaceMetricItem
                            label="预估毛利"
                            value={
                                metrics.estimatedMargin ? (
                                    <span
                                        className={cn(
                                            "inline-flex flex-wrap items-baseline gap-x-2",
                                            metrics.estimatedMargin.negative &&
                                                "text-destructive-soft-foreground",
                                        )}
                                    >
                                        <span>
                                            {metrics.estimatedMargin.negative
                                                ? "亏 "
                                                : ""}
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    metrics.estimatedMargin
                                                        .absAmount
                                                }
                                                inheritColor={
                                                    metrics.estimatedMargin
                                                        .negative
                                                }
                                            />
                                        </span>
                                        <span className="num text-body-compact font-medium">
                                            {metrics.estimatedMargin.negative
                                                ? `-${metrics.estimatedMargin.absPercent}%`
                                                : `${metrics.estimatedMargin.percent}%`}
                                        </span>
                                    </span>
                                ) : (
                                    <span className="text-body-compact font-medium text-muted-foreground">
                                        未填供给价
                                    </span>
                                )
                            }
                            detail={
                                metrics.estimatedCostTotal ? (
                                    <>
                                        <span>
                                            成本{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={
                                                    metrics.estimatedCostTotal
                                                }
                                            />
                                        </span>
                                        {metrics.giftCostTotal ? (
                                            <>
                                                <span aria-hidden="true">
                                                    ·
                                                </span>
                                                <span>
                                                    含赠品{" "}
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            metrics.giftCostTotal
                                                        }
                                                    />
                                                </span>
                                            </>
                                        ) : null}
                                        {metrics.partialCostCoverage ? (
                                            <>
                                                <span aria-hidden="true">
                                                    ·
                                                </span>
                                                <span className="num text-warning-soft-foreground">
                                                    已计{" "}
                                                    {metrics.costCoveredCount}/
                                                    {metrics.costExpectedCount}{" "}
                                                    行
                                                </span>
                                            </>
                                        ) : null}
                                    </>
                                ) : (
                                    "无法测算整单毛利"
                                )
                            }
                        />
                    )}

                    <DetailWorkspaceMetricItem
                        label="核对风险"
                        value={
                            metrics.highRiskLineCount > 0 ? (
                                <span className="text-destructive-soft-foreground">
                                    {metrics.highRiskLineCount} 项高危
                                </span>
                            ) : metrics.priceAdjustedCount > 0 ? (
                                <span className="text-warning-soft-foreground">
                                    {metrics.priceAdjustedCount} 项调价
                                </span>
                            ) : (
                                <span className="text-body-compact font-medium text-success-soft-foreground">
                                    无价格异常
                                </span>
                            )
                        }
                        detail={
                            metrics.riskLineCount > 0 ? (
                                <button
                                    id={`${prefix}-kpi-filter-risk`}
                                    type="button"
                                    onClick={handleFilterToRisks}
                                    className="cursor-pointer text-left text-primary hover:underline"
                                >
                                    {[
                                        metrics.negativeMarginCount > 0
                                            ? `负毛利 ${metrics.negativeMarginCount}`
                                            : null,
                                        metrics.belowFloorCount > 0
                                            ? `低原底价 ${metrics.belowFloorCount}`
                                            : null,
                                        metrics.priceAdjustedCount > 0
                                            ? `底价调整 ${metrics.priceAdjustedCount}`
                                            : null,
                                    ]
                                        .filter(Boolean)
                                        .join(" · ")}{" "}
                                    · 点击筛选
                                </button>
                            ) : metrics.missingSupplyCount > 0 ? (
                                <span className="num">
                                    {metrics.missingSupplyCount} 项未填供给价
                                </span>
                            ) : (
                                "全部明细价格正常"
                            )
                        }
                    />

                    <DetailWorkspaceMetricItem
                        label="付款与合同依据"
                        value={
                            <span className="text-body-compact font-semibold text-foreground">
                                {displayName(submission.payment_term_name) ??
                                    "—"}
                            </span>
                        }
                        detail={
                            <>
                                {contractNo ? (
                                    <span className="num text-foreground/85">
                                        合同 {contractNo}
                                    </span>
                                ) : (
                                    <span className="text-warning-soft-foreground">
                                        无合同
                                    </span>
                                )}
                                <span aria-hidden="true">·</span>
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

            {commercialFacts.length > 0 || submission.business_remark ? (
                <section
                    aria-label="销售单商业信息"
                    className="space-y-2.5 border-b border-border/60 py-3"
                >
                    {commercialFacts.length > 0 ? (
                        <dl className="flex flex-wrap items-baseline gap-x-5 gap-y-1.5 text-xs">
                            {commercialFacts.map((fact) => (
                                <div
                                    key={fact.label}
                                    className="inline-flex max-w-full items-baseline gap-1.5"
                                >
                                    <dt className="shrink-0 text-muted-foreground">
                                        {fact.label}
                                    </dt>
                                    <dd
                                        className={cn(
                                            "wrap-anywhere font-medium text-foreground",
                                            fact.numeric && "num",
                                        )}
                                    >
                                        {fact.value}
                                    </dd>
                                </div>
                            ))}
                        </dl>
                    ) : null}

                    {submission.business_remark ? (
                        <div className="flex items-start gap-2 rounded-md border border-border/70 bg-surface-sunken/70 px-3 py-2 text-xs">
                            <MessageSquareTextIcon
                                aria-hidden="true"
                                className="mt-0.5 size-3.5 shrink-0 text-muted-foreground"
                            />
                            <p className="min-w-0 flex-1 whitespace-pre-wrap wrap-anywhere text-foreground">
                                <span className="mr-1.5 font-medium text-muted-foreground">
                                    业务备注：
                                </span>
                                {submission.business_remark}
                            </p>
                        </div>
                    ) : null}
                </section>
            ) : null}

            <DetailWorkspaceSection
                id={linesSectionId}
                divider={false}
                title="销售明细"
                subtitle={
                    metrics.giftCount > 0
                        ? `共 ${submission.lines.length} 行 · 含 ${metrics.giftCount} 项赠品`
                        : `共 ${submission.lines.length} 行`
                }
                ariaLabel="销售明细"
                className="py-5 first:pt-4"
                actions={
                    <Button
                        id={`${prefix}-preview`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                        onClick={() =>
                            onPreview({
                                kind: "approval_snapshot",
                                objectId: instanceId,
                                title: query.data?.document_no,
                            })
                        }
                    >
                        <FileTextIcon aria-hidden="true" />
                        预览完整销售单
                    </Button>
                }
            >
                <ApprovalLineFilterBar
                    idPrefix={prefix}
                    totalCount={evaluatedLines.length}
                    filteredCount={filteredLines.length}
                    riskCount={metrics.riskLineCount}
                    hasHighRisk={metrics.highRiskLineCount > 0}
                    riskOnly={riskOnly}
                    onRiskOnlyChange={(next) => {
                        setRiskOnly(next)
                        setPage(1)
                    }}
                    search={search}
                    onSearchChange={(next) => {
                        setSearch(next)
                        setPage(1)
                    }}
                    onClearFilters={() => {
                        setSearch("")
                        setRiskOnly(false)
                        setPage(1)
                    }}
                    ariaLabel="销售明细筛选"
                />

                {submission.lines.length ? (
                    <div
                        data-slot="detail-inline-table"
                        className={detailWorkspaceStyles.inlineTableWrapper}
                    >
                        <Table
                            data-density="comfortable"
                            aria-label="销售明细"
                            className="w-full [&_:is(th,td):first-child]:ps-0"
                        >
                            <TableHeader>
                                <TableRow>
                                    <TableHead className="w-8">#</TableHead>
                                    <TableHead className="min-w-40">
                                        商品 / 规格
                                    </TableHead>
                                    <TableHead
                                        data-align="end"
                                        className="w-20"
                                    >
                                        数量
                                    </TableHead>
                                    <TableHead
                                        data-align="end"
                                        className="w-28"
                                    >
                                        {isCard
                                            ? "含税单价 / 面额"
                                            : "含税售价"}
                                    </TableHead>
                                    {!isCard ? (
                                        <TableHead
                                            data-align="end"
                                            className="w-36"
                                        >
                                            供给价 / 毛利
                                        </TableHead>
                                    ) : null}
                                    <TableHead
                                        data-align="end"
                                        className="w-28"
                                    >
                                        含税小计
                                    </TableHead>
                                </TableRow>
                            </TableHeader>
                            <TableBody>
                                {visibleLines.length > 0 ? (
                                    visibleLines.map((entry) => {
                                        const deliveryHint =
                                            formatLineDeliveryHint(
                                                deliveryItemById.get(
                                                    entry.line.id,
                                                ),
                                                deliverySummary,
                                            )
                                        const lineSegment =
                                            toAutomationIdSegment(entry.line.id)
                                        return (
                                            <SalesLineRow
                                                key={entry.line.id}
                                                rowId={`${prefix}-row-${lineSegment}`}
                                                highlighted={
                                                    highlightedLineId ===
                                                    entry.line.id
                                                }
                                                entry={entry}
                                                isCard={isCard}
                                                deliveryHint={deliveryHint}
                                                idPrefix={`${prefix}-line-${lineSegment}`}
                                                onFocusDelivery={() =>
                                                    handleFocusDelivery(
                                                        entry.line.id,
                                                    )
                                                }
                                            />
                                        )
                                    })
                                ) : (
                                    <TableRow>
                                        <TableCell
                                            colSpan={isCard ? 5 : 6}
                                            className="py-10 text-center text-xs text-muted-foreground"
                                        >
                                            没有匹配的销售明细，请调整筛选条件。
                                        </TableCell>
                                    </TableRow>
                                )}
                            </TableBody>
                            <TableFooter>
                                <TableRow className="hover:bg-transparent">
                                    <TableCell
                                        colSpan={2}
                                        className="py-2.5 align-middle text-xs font-medium text-muted-foreground"
                                    >
                                        合计 · 共{" "}
                                        <span className="num text-foreground">
                                            {submission.lines.length}
                                        </span>{" "}
                                        项
                                        {filteredLines.length !==
                                        evaluatedLines.length ? (
                                            <span className="num ml-1.5 font-normal">
                                                （当前显示{" "}
                                                {filteredLines.length}{" "}
                                                项，合计按整单）
                                            </span>
                                        ) : null}
                                    </TableCell>
                                    <TableCell
                                        data-align="end"
                                        className="py-2.5 align-middle text-xs font-medium text-foreground"
                                    >
                                        {metrics.totalQuantity ? (
                                            <QuantityValue
                                                value={
                                                    metrics.totalQuantity.value
                                                }
                                                unit={
                                                    metrics.totalQuantity.unit
                                                }
                                            />
                                        ) : (
                                            "—"
                                        )}
                                    </TableCell>
                                    <TableCell className="py-2.5" />
                                    {!isCard ? (
                                        <TableCell
                                            data-align="end"
                                            className="py-2.5 align-middle text-xs"
                                        >
                                            {metrics.estimatedMargin ? (
                                                <div
                                                    className={cn(
                                                        "num font-medium",
                                                        metrics.estimatedMargin
                                                            .negative
                                                            ? "text-destructive-soft-foreground"
                                                            : "text-foreground/85",
                                                    )}
                                                >
                                                    {metrics.estimatedMargin
                                                        .negative
                                                        ? `整单亏 ${metrics.estimatedMargin.absPercent}%`
                                                        : `整单毛利 ${metrics.estimatedMargin.percent}%`}
                                                </div>
                                            ) : (
                                                <span className="text-muted-foreground">
                                                    —
                                                </span>
                                            )}
                                            {metrics.estimatedMargin ? (
                                                <div className="mt-0.5 text-tiny font-normal text-muted-foreground">
                                                    {metrics.estimatedMargin
                                                        .negative
                                                        ? "亏 "
                                                        : "毛利额 "}
                                                    <MoneyValue
                                                        display="exact"
                                                        value={
                                                            metrics
                                                                .estimatedMargin
                                                                .absAmount
                                                        }
                                                    />
                                                    {metrics.partialCostCoverage
                                                        ? ` · ${metrics.costCoveredCount}/${metrics.costExpectedCount} 行`
                                                        : ""}
                                                </div>
                                            ) : null}
                                        </TableCell>
                                    ) : null}
                                    <TableCell
                                        data-align="end"
                                        className="py-2.5 align-middle"
                                    >
                                        <div className="font-semibold text-foreground">
                                            <MoneyValue
                                                display="exact"
                                                value={submission.gross_amount}
                                            />
                                        </div>
                                        <div className="mt-0.5 text-xs font-normal text-muted-foreground">
                                            不含税{" "}
                                            <MoneyValue
                                                display="exact"
                                                value={submission.net_amount}
                                            />
                                        </div>
                                    </TableCell>
                                </TableRow>
                            </TableFooter>
                        </Table>
                    </div>
                ) : (
                    <p className="text-xs text-muted-foreground">
                        本次提交没有销售明细。
                    </p>
                )}

                <ApprovalLinePagination
                    idPrefix={prefix}
                    filteredCount={filteredLines.length}
                    page={currentPage}
                    pageSize={pageSize}
                    onPageChange={setPage}
                    onPageSizeChange={(next) => {
                        setPageSize(next)
                        setPage(1)
                    }}
                    ariaLabel="销售明细分页"
                />
            </DetailWorkspaceSection>

            {showDeliverySection ? (
                <DeliveryAllocationOverview
                    idPrefix={`${prefix}-delivery`}
                    sectionId={deliverySectionId}
                    lines={deliveryLines}
                    summary={deliverySummary}
                    compactSimpleStates
                    focusedLineId={focusedDeliveryLineId}
                    onClearFocus={() => setFocusedDeliveryLineId(null)}
                    onReturnToLine={handleReturnToLine}
                />
            ) : null}

            {query.data && !hideEmbeddedMaterials ? (
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
                        emptyMessage="本次提交未附带合同或凭证文件。"
                        onSelectFile={onSelectFile}
                        activeFileId={activeFileId}
                    />
                </DetailWorkspaceSection>
            ) : null}
        </div>
    )
}

function SalesLineRow({
    rowId,
    highlighted,
    entry,
    isCard,
    deliveryHint,
    idPrefix,
    onFocusDelivery,
}: {
    rowId: string
    highlighted: boolean
    entry: EvaluatedSalesLine
    isCard: boolean
    deliveryHint: string | null
    idPrefix: string
    onFocusDelivery: () => void
}) {
    const {
        line,
        name,
        spec,
        welfare,
        unit,
        isGift,
        negativeMargin,
        belowOriginalFloor,
        priceAdjusted,
        priceCheck,
        margin,
        voucherDiscount,
    } = entry

    const quantity =
        line.quantity ??
        (line.card_count == null ? null : String(line.card_count))
    const dueDate =
        !isCard && line.fulfillment_due_at != null
            ? dateLabel(line.fulfillment_due_at)
            : null
    const metaParts = [
        spec,
        welfare,
        dueDate ? `交付 ${dueDate}` : null,
    ].filter(Boolean)

    const supplyPrice = line.procurement_supply_price_gross
    const originalPrice = line.original_procurement_supply_price_gross
    const hasOriginal = originalPrice != null && originalPrice !== ""
    const basisLabel =
        line.procurement_price_basis === "BULK"
            ? "集采"
            : line.procurement_price_basis === "DROPSHIP"
              ? "一件代发"
              : null
    const isHighRisk = negativeMargin || belowOriginalFloor

    const taxRatePercent = multiplyFixed(line.sales_tax_rate, "100", {
        leftMaxScale: 6,
        rightMaxScale: 0,
        outputScale: 2,
    })

    return (
        <TableRow
            id={rowId}
            className={cn(
                isHighRisk &&
                    "bg-destructive-soft/35 hover:bg-destructive-soft/50",
                highlighted &&
                    "bg-info-soft/50 ring-1 ring-inset ring-info-border",
            )}
        >
            <TableCell className="num py-3 align-top text-muted-foreground">
                {line.line_no}
            </TableCell>

            <TableCell className="min-w-40 py-3 align-top whitespace-normal">
                <div className="flex flex-wrap items-center gap-1.5">
                    <span className="font-medium wrap-anywhere text-foreground">
                        {name}
                    </span>
                    {isGift ? <Badge variant="neutral">赠品</Badge> : null}
                    {negativeMargin ? (
                        <Badge variant="destructive">负毛利</Badge>
                    ) : belowOriginalFloor ? (
                        <Badge variant="destructive">低于原底价</Badge>
                    ) : null}
                </div>
                {metaParts.length > 0 ? (
                    <p className="num mt-1 text-xs wrap-anywhere text-muted-foreground">
                        {metaParts.join(" · ")}
                    </p>
                ) : null}
                {deliveryHint ? (
                    <button
                        id={`${idPrefix}-delivery-jump`}
                        type="button"
                        onClick={onFocusDelivery}
                        className="mt-1 inline-flex max-w-full cursor-pointer items-center gap-1 text-left text-xs text-primary/90 hover:text-primary hover:underline"
                    >
                        <MapPinIcon
                            aria-hidden="true"
                            className="size-3 shrink-0"
                        />
                        <span className="truncate">{deliveryHint}</span>
                    </button>
                ) : null}
            </TableCell>

            <TableCell
                data-align="end"
                className="py-3 align-top whitespace-nowrap"
            >
                {quantity == null ? (
                    "—"
                ) : (
                    <QuantityValue value={quantity} unit={unit} />
                )}
            </TableCell>

            <TableCell
                data-align="end"
                className="py-3 align-top whitespace-nowrap"
            >
                <MoneyValue
                    display="exact"
                    value={line.unit_price_gross}
                    inheritColor={isHighRisk}
                    className={cn(
                        "font-medium",
                        isHighRisk && "text-destructive-soft-foreground",
                    )}
                />
                {isCard ? (
                    <p className="mt-1 text-xs text-muted-foreground">
                        面额{" "}
                        <MoneyValue display="exact" value={line.face_value} />
                        {voucherDiscount ? ` · ${voucherDiscount.label}` : ""}
                        {" · "}
                        {line.card_form === "PHYSICAL"
                            ? "实体卡"
                            : line.card_form === "ELECTRONIC"
                              ? "电子券"
                              : "—"}
                    </p>
                ) : null}
            </TableCell>

            {!isCard ? (
                <TableCell
                    data-align="end"
                    className="py-3 align-top whitespace-normal"
                >
                    <div className="flex flex-wrap items-center justify-end gap-1.5">
                        {supplyPrice != null && supplyPrice !== "" ? (
                            <MoneyValue
                                display="exact"
                                value={supplyPrice}
                                className="font-medium text-foreground/90"
                            />
                        ) : (
                            <span className="text-xs text-muted-foreground">
                                未填写
                            </span>
                        )}
                        {priceAdjusted && priceCheck ? (
                            <Badge variant="warning" className="text-tiny">
                                {priceCheck.adjustment === "increased"
                                    ? "调高"
                                    : "调低"}
                            </Badge>
                        ) : null}
                    </div>
                    {isGift ? (
                        <p className="mt-1 text-xs text-muted-foreground">
                            赠品成本
                        </p>
                    ) : margin ? (
                        <p
                            className={cn(
                                "num mt-1 text-xs",
                                margin.negative
                                    ? "font-medium text-destructive-soft-foreground"
                                    : "text-muted-foreground",
                            )}
                        >
                            {margin.negative
                                ? `亏 ${margin.absPercent}%`
                                : `毛利 ${margin.percent}%`}
                        </p>
                    ) : null}
                    {belowOriginalFloor && hasOriginal ? (
                        <p className="mt-0.5 text-tiny font-medium text-destructive-soft-foreground">
                            低于原底价{" "}
                            <MoneyValue
                                display="exact"
                                value={originalPrice}
                                inheritColor
                                className="font-medium"
                            />
                            {basisLabel ? ` · ${basisLabel}` : ""}
                        </p>
                    ) : priceAdjusted && hasOriginal ? (
                        <p className="mt-0.5 text-tiny text-warning-soft-foreground">
                            原底价{" "}
                            <MoneyValue
                                display="exact"
                                value={originalPrice}
                                inheritColor
                                className="font-normal"
                            />
                            {basisLabel ? ` · ${basisLabel}` : ""}
                        </p>
                    ) : !hasOriginal &&
                      supplyPrice != null &&
                      supplyPrice !== "" ? (
                        <p className="mt-0.5 text-tiny text-muted-foreground">
                            {basisLabel ? `${basisLabel} · ` : ""}无历史底价
                        </p>
                    ) : null}
                </TableCell>
            ) : null}

            <TableCell
                data-align="end"
                className="py-3 align-top whitespace-nowrap"
            >
                <div className="font-medium text-foreground">
                    <MoneyValue display="exact" value={line.gross_amount} />
                </div>
                <p className="mt-1 text-xs text-muted-foreground">
                    税 <RateValue value={taxRatePercent} precision={2} />
                    {" · "}
                    <MoneyValue display="exact" value={line.tax_amount} />
                </p>
            </TableCell>
        </TableRow>
    )
}
