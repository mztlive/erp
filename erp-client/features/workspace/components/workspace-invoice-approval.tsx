"use client"

import { useEffect, useState, type ReactNode } from "react"
import {
    ChevronRightIcon,
    FileTextIcon,
    PaperclipIcon,
    RefreshCwIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    MoneyValue,
    RateValue,
    WorkspaceFactGrid,
    WorkspaceFactGroup,
    WorkspaceFactItem,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import { displayUnixSeconds } from "@/features/approval-workflow/display"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { InvoiceApprovalReview } from "../api/invoice-review"
import { useInvoiceApprovalReview } from "../hooks/use-invoice-review"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceApprovalResubmissionBanner } from "./workspace-approval-resubmission-banner"
import type { WorkspacePaperTarget } from "./workspace-document-paper-dialog"
import {
    InvoiceHistoryEvidence,
    InvoiceReceiptEvidence,
    InvoiceRequestEvidence,
    InvoiceSourceEvidence,
} from "./workspace-invoice-approval-evidence"
import { InvoiceApprovalKpis } from "./workspace-invoice-approval-kpis"
import {
    invoiceApprovalResubmission,
    invoiceApprovalRisks,
    invoiceApprovalSubmission,
    invoiceSourceNumber,
} from "./workspace-invoice-approval-model"

export { buildInvoiceDynamicRejectPhrases } from "./workspace-invoice-approval-model"

type EvidenceTabId = "source-sales" | "receipts" | "invoices"
type RiskTarget = "source-warnings" | "quota-evidence" | "materials"

const pulseDuration = 2200

export function WorkspaceInvoiceApproval({
    instanceId,
    item,
    onPreview,
    onSelectFile,
    activeFileId,
    hideEmbeddedMaterials = false,
    onOpenMaterials,
}: {
    instanceId: string
    item: WorkspaceWorkItem
    onPreview: (target: WorkspacePaperTarget) => void
    onSelectFile?: (fileId: string) => void
    activeFileId?: string
    hideEmbeddedMaterials?: boolean
    onOpenMaterials?: () => void
}) {
    const review = useInvoiceApprovalReview(instanceId)
    const materials = useApprovalMaterials(instanceId, true, item)
    const prefix = `workspace-invoice-approval-${toAutomationIdSegment(instanceId)}`
    const [activeTab, setActiveTab] = useState<EvidenceTabId>("source-sales")
    const [jump, setJump] = useState<{
        target: RiskTarget
        sequence: number
    } | null>(null)

    useEffect(() => {
        if (!jump) return
        const reducedMotion = window.matchMedia?.(
            "(prefers-reduced-motion: reduce)",
        ).matches
        const frame = window.requestAnimationFrame(() => {
            const target = document.getElementById(`${prefix}-${jump.target}`)
            target?.focus({ preventScroll: true })
            target?.scrollIntoView({
                behavior: reducedMotion ? "instant" : "smooth",
                block: "center",
            })
        })
        const timer = window.setTimeout(() => setJump(null), pulseDuration)
        return () => {
            window.cancelAnimationFrame(frame)
            window.clearTimeout(timer)
        }
    }, [jump, prefix])

    const locateRisk = (target: RiskTarget) => {
        if (target === "quota-evidence") {
            setActiveTab("invoices")
        } else if (target === "source-warnings") {
            setActiveTab("source-sales")
        }
        setJump((previous) => ({
            target,
            sequence: (previous?.sequence ?? 0) + 1,
        }))
    }
    const submission = invoiceApprovalSubmission(
        materials.data,
        item,
        review.data,
    )
    const resubmission = invoiceApprovalResubmission(materials.data, item)
    const { shortage, quotaWarnings, sourceWarnings } = invoiceApprovalRisks(
        review.data,
    )
    const submittedSales = materials.data?.display.source_sales ?? []
    const currentBasis = materials.data?.current_sales_basis
    const showCurrent = Boolean(
        currentBasis &&
        (!submittedSales.length ||
            submittedSales.some(
                (source) =>
                    source.revision_id !== currentBasis.source.revision_id,
            )),
    )
    const primarySource = submittedSales[0] ?? currentBasis?.source
    const submittedFiles = materials.data?.attachments
    const currentFiles =
        currentBasis && (showCurrent || !submittedFiles?.length)
            ? currentBasis
            : undefined
    const attachmentCount = new Set([
        ...(submittedFiles ?? []).map((file) => file.file_asset_id),
        ...(
            (hideEmbeddedMaterials ? currentBasis : currentFiles)
                ?.attachments ?? []
        ).map((file) => file.file_asset_id),
    ]).size
    const evidenceTabs: { id: EvidenceTabId; label: string }[] = [
        { id: "source-sales", label: "来源销售单" },
        {
            id: "receipts",
            label: review.data
                ? `关联回款（${review.data.records.receipt_count}）`
                : "关联回款",
        },
        {
            id: "invoices",
            label: review.data
                ? `开票记录（${review.data.records.invoice_count}）`
                : "开票记录",
        },
    ]
    const readAt = review.data && displayUnixSeconds(review.data.read_at)
    const decimalTaxPoint =
        submission.taxPoint && /^\d+(?:\.\d+)?%$/.test(submission.taxPoint)
            ? submission.taxPoint.slice(0, -1)
            : null

    const preview = (
        view: "source-sales" | "current-source-sales",
        revisionId: string,
    ) =>
        onPreview({
            kind: "approval_snapshot",
            objectId: instanceId,
            materialsView: view,
            sourceRevisionId: revisionId,
        })
    const refresh = () => {
        void review.refetch()
        void materials.refetch()
    }
    const openEmbeddedMaterials =
        !hideEmbeddedMaterials && attachmentCount > 0
            ? () => locateRisk("materials")
            : undefined
    const openMaterialsAction = onOpenMaterials ?? openEmbeddedMaterials
    const isFetching = review.isFetching || materials.isFetching

    const activeTabSegment = toAutomationIdSegment(activeTab)

    return (
        <div
            data-slot="workspace-invoice-approval"
            className="@container/invoice-approval flex min-w-0 flex-col gap-3 pt-3"
        >
            {resubmission ? (
                <WorkspaceApprovalResubmissionBanner {...resubmission} />
            ) : null}

            {shortage || sourceWarnings.length ? (
                <div
                    role="alert"
                    data-slot="invoice-approval-risk-strip"
                    className={cn(
                        "flex flex-wrap items-center justify-between gap-x-3 gap-y-1 rounded-md border-l-2 px-3 py-1.5 text-xs",
                        shortage
                            ? "border-l-destructive bg-destructive-soft/65 text-destructive-soft-foreground"
                            : "border-l-warning bg-warning-soft/75 text-warning-soft-foreground",
                    )}
                >
                    <p className="num flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-0.5">
                        <TriangleAlertIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0"
                        />
                        {shortage ? (
                            <span className="font-medium">
                                开票占用超额{" "}
                                <MoneyValue
                                    value={shortage}
                                    display="exact"
                                    inheritColor
                                />
                            </span>
                        ) : null}
                        {shortage && sourceWarnings.length ? (
                            <span aria-hidden="true" className="opacity-60">
                                ·
                            </span>
                        ) : null}
                        {sourceWarnings.length ? (
                            <span>需核对 {sourceWarnings.length} 项</span>
                        ) : null}
                    </p>
                    <div className="flex flex-wrap items-center gap-1">
                        {shortage ? (
                            <RiskJumpButton
                                id={`${prefix}-locate-quota`}
                                severe
                                onClick={() => locateRisk("quota-evidence")}
                            >
                                定位超额说明
                            </RiskJumpButton>
                        ) : null}
                        {sourceWarnings.length ? (
                            <RiskJumpButton
                                id={`${prefix}-locate-warnings`}
                                severe={Boolean(shortage)}
                                onClick={() => locateRisk("source-warnings")}
                            >
                                查看异常依据
                            </RiskJumpButton>
                        ) : null}
                    </div>
                </div>
            ) : null}

            <InvoiceApprovalKpis
                prefix={prefix}
                submission={submission}
                review={review.data}
                shortage={shortage}
            />
            {review.isPending ? (
                <Loading>正在读取开票额度与回款…</Loading>
            ) : review.isError ? (
                <BusinessFailureState
                    id={`${prefix}-retry`}
                    title="票款核对依据读取失败"
                    error={review.error}
                    onRetry={() => void review.refetch()}
                />
            ) : null}

            <WorkspaceFactGroup
                title="开票申请与票税要素"
                ariaLabel="开票申请信息"
                className="pt-1"
                actions={
                    <>
                        {readAt ? (
                            <time
                                className="num text-tiny text-muted-foreground"
                                dateTime={readAt.dateTime}
                            >
                                核对于 {readAt.label}
                            </time>
                        ) : null}
                        <Button
                            id={`${prefix}-refresh`}
                            type="button"
                            size="xs"
                            variant="ghost"
                            className={cn(
                                detailWorkspaceStyles.inlineLinkButton,
                                "h-control-xs px-1.5 text-xs",
                            )}
                            disabled={isFetching}
                            onClick={refresh}
                        >
                            <RefreshCwIcon
                                aria-hidden="true"
                                className={cn(
                                    isFetching && "motion-safe:animate-spin",
                                )}
                            />
                            刷新票款
                        </Button>
                        {openMaterialsAction ? (
                            <Button
                                id={`${prefix}-open-materials`}
                                type="button"
                                size="xs"
                                variant="ghost"
                                className={cn(
                                    detailWorkspaceStyles.inlineLinkButton,
                                    "num h-control-xs px-1.5 text-xs",
                                )}
                                onClick={openMaterialsAction}
                            >
                                <PaperclipIcon aria-hidden="true" />
                                凭证（{attachmentCount}）
                            </Button>
                        ) : null}
                    </>
                }
            >
                <WorkspaceFactGrid columns={3} ariaLabel="开票申请信息">
                    <WorkspaceFactItem
                        label="购方抬头"
                        value={submission.buyerTitle}
                        selectable={Boolean(submission.buyerTitle)}
                    />
                    <WorkspaceFactItem
                        label="购方税号"
                        value={submission.buyerTaxNumber}
                        numeric
                        selectable={Boolean(submission.buyerTaxNumber)}
                    />
                    <WorkspaceFactItem
                        label="开票类型"
                        value={submission.invoiceType}
                    />
                    <WorkspaceFactItem
                        label="销方主体"
                        value={submission.sellerName}
                    />
                    <WorkspaceFactItem
                        label="销方税号"
                        value={submission.sellerTaxNumber}
                        numeric
                        selectable={Boolean(submission.sellerTaxNumber)}
                    />
                    <WorkspaceFactItem label="约定税点" numeric>
                        {submission.taxPoint ? (
                            decimalTaxPoint === null ? (
                                submission.taxPoint
                            ) : (
                                <RateValue
                                    value={decimalTaxPoint}
                                    precision={2}
                                    className="font-medium text-foreground"
                                />
                            )
                        ) : null}
                    </WorkspaceFactItem>
                    <WorkspaceFactItem
                        label="开票时点"
                        value={submission.invoiceTiming}
                    />
                    <WorkspaceFactItem
                        label="付款条件"
                        value={submission.paymentTerm}
                    />
                    <WorkspaceFactItem
                        label="申请人"
                        value={submission.submitter}
                    />
                    {submission.reason ? (
                        <WorkspaceFactItem
                            label="申请事由"
                            value={submission.reason}
                            span="full"
                        />
                    ) : null}
                    {primarySource ? (
                        <WorkspaceFactItem label="来源销售单" span="full">
                            <Button
                                id={`${prefix}-primary-source-preview-${toAutomationIdSegment(primarySource.revision_id)}`}
                                type="button"
                                size="xs"
                                variant="ghost"
                                className={cn(
                                    detailWorkspaceStyles.inlineLinkButton,
                                    "-ml-1.5 h-auto max-w-full px-1.5 text-left text-body-compact whitespace-normal wrap-anywhere",
                                )}
                                onClick={() =>
                                    preview(
                                        submittedSales.length
                                            ? "source-sales"
                                            : "current-source-sales",
                                        primarySource.revision_id,
                                    )
                                }
                            >
                                <FileTextIcon aria-hidden="true" />
                                <span>
                                    预览销售单{" "}
                                    <span className="num">
                                        {invoiceSourceNumber(primarySource)} · V
                                        {primarySource.revision_no}
                                    </span>
                                </span>
                            </Button>
                        </WorkspaceFactItem>
                    ) : null}
                </WorkspaceFactGrid>
            </WorkspaceFactGroup>

            <section aria-label="开票核对依据" className="min-w-0 space-y-3">
                <h2 className="sr-only">开票核对依据</h2>
                <nav
                    aria-label="开票核对依据切换"
                    className="flex flex-wrap gap-x-5 border-b border-border/60"
                >
                    {evidenceTabs.map(({ id, label }) => {
                        const tabSegment = toAutomationIdSegment(id)
                        return (
                            <button
                                key={id}
                                id={`${prefix}-tab-${tabSegment}`}
                                type="button"
                                aria-pressed={activeTab === id}
                                aria-controls={
                                    activeTab === id
                                        ? `${prefix}-panel-${tabSegment}`
                                        : undefined
                                }
                                data-active={activeTab === id || undefined}
                                className={cn(detailWorkspaceStyles.tab, "num")}
                                onClick={() => {
                                    setActiveTab(id)
                                    setJump(null)
                                }}
                            >
                                {label}
                            </button>
                        )
                    })}
                </nav>
                <div
                    id={`${prefix}-panel-${activeTabSegment}`}
                    role="region"
                    aria-labelledby={`${prefix}-tab-${activeTabSegment}`}
                    className="min-w-0 space-y-4"
                >
                    {activeTab === "source-sales" ? (
                        <>
                            {sourceWarnings.length ? (
                                <RiskEvidence
                                    id={`${prefix}-source-warnings`}
                                    title="销售依据与票税资料异常"
                                    pulse={
                                        jump?.target === "source-warnings"
                                            ? jump.sequence
                                            : undefined
                                    }
                                >
                                    <WarningList warnings={sourceWarnings} />
                                </RiskEvidence>
                            ) : null}
                            {materials.isPending ? (
                                <Loading>正在读取开票资料与销售依据…</Loading>
                            ) : materials.isError ? (
                                <BusinessFailureState
                                    id={`${prefix}-materials-retry`}
                                    title="开票资料与销售依据读取失败"
                                    error={materials.error}
                                    onRetry={() => void materials.refetch()}
                                />
                            ) : materials.data ? (
                                <>
                                    <InvoiceRequestEvidence
                                        prefix={prefix}
                                        submission={submission}
                                    />
                                    {submittedSales.map((source) => (
                                        <InvoiceSourceEvidence
                                            key={source.revision_id}
                                            prefix={prefix}
                                            source={source}
                                            frozen
                                            onOpen={() =>
                                                preview(
                                                    "source-sales",
                                                    source.revision_id,
                                                )
                                            }
                                        />
                                    ))}
                                    {showCurrent && currentBasis ? (
                                        <InvoiceSourceEvidence
                                            prefix={prefix}
                                            source={currentBasis.source}
                                            frozen={false}
                                            onOpen={() =>
                                                preview(
                                                    "current-source-sales",
                                                    currentBasis.source
                                                        .revision_id,
                                                )
                                            }
                                        />
                                    ) : null}
                                    {(!submittedSales.length ||
                                        materials.data
                                            .current_sales_basis_error) &&
                                    !currentBasis ? (
                                        <p
                                            role="status"
                                            className="text-xs text-muted-foreground"
                                        >
                                            {materials.data
                                                .current_sales_basis_error ??
                                                "销售依据暂不可读取，请刷新后重试。"}
                                        </p>
                                    ) : null}
                                </>
                            ) : null}
                        </>
                    ) : review.data ? (
                        activeTab === "receipts" ? (
                            <InvoiceReceiptEvidence
                                prefix={prefix}
                                review={review.data}
                            />
                        ) : (
                            <InvoiceHistoryEvidence
                                prefix={prefix}
                                review={review.data}
                                quotaEvidence={
                                    <QuotaEvidence
                                        id={`${prefix}-quota-evidence`}
                                        review={review.data}
                                        shortage={shortage}
                                        warnings={quotaWarnings}
                                        pulse={
                                            jump?.target === "quota-evidence"
                                                ? jump.sequence
                                                : undefined
                                        }
                                    />
                                }
                            />
                        )
                    ) : null}
                </div>
            </section>

            {!hideEmbeddedMaterials ? (
                <DetailWorkspaceSection
                    id={`${prefix}-materials`}
                    tabIndex={-1}
                    divider
                    title="合同与提交凭证"
                    subtitle={
                        attachmentCount > 0 ? (
                            <span className="num">
                                共 {attachmentCount} 个文件
                            </span>
                        ) : undefined
                    }
                    ariaLabel="合同与提交凭证"
                    className={cn(
                        "scroll-mt-16 rounded-md pt-4 pb-5 transition-colors",
                        jump?.target === "materials" &&
                            "bg-info-soft/25 ring-1 ring-info-border motion-safe:animate-pulse",
                    )}
                >
                    {submittedFiles?.length ? (
                        <ApprovalMaterialFiles
                            instanceId={instanceId}
                            attachments={submittedFiles}
                            idPrefix={`${prefix}-submitted-file`}
                            onSelectFile={onSelectFile}
                            activeFileId={activeFileId}
                        />
                    ) : null}
                    {currentFiles ? (
                        <>
                            <p className="num text-xs text-muted-foreground">
                                来源销售单 · V{currentFiles.source.revision_no}{" "}
                                · 当前生效版本
                            </p>
                            <ApprovalMaterialFiles
                                instanceId={instanceId}
                                attachments={currentFiles.attachments}
                                sourceRevisionId={
                                    currentFiles.source.revision_id
                                }
                                idPrefix={`${prefix}-current-file`}
                                onSelectFile={onSelectFile}
                                activeFileId={activeFileId}
                                emptyMessage={
                                    currentFiles.unavailable_reason ??
                                    "此销售版本未关联可读取的合同或凭证，请联系销售核对归档资料。"
                                }
                            />
                        </>
                    ) : !submittedFiles?.length ? (
                        <p className="text-xs text-muted-foreground">
                            {materials.isPending
                                ? "正在读取合同与凭证…"
                                : materials.isError
                                  ? "合同与凭证读取失败。"
                                  : "暂无提交附件或销售合同凭证。"}
                        </p>
                    ) : null}
                </DetailWorkspaceSection>
            ) : null}
        </div>
    )
}

function RiskJumpButton({
    id,
    severe,
    onClick,
    children,
}: {
    id: string
    severe: boolean
    onClick: () => void
    children: ReactNode
}) {
    return (
        <Button
            id={id}
            type="button"
            variant="ghost"
            size="xs"
            className={cn(
                "h-control-xs gap-0.5 px-1.5 text-xs font-medium shadow-none",
                severe
                    ? "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                    : "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
            )}
            onClick={onClick}
        >
            {children}
            <ChevronRightIcon aria-hidden="true" />
        </Button>
    )
}

function RiskEvidence({
    id,
    title,
    pulse,
    children,
}: {
    id: string
    title: string
    pulse?: number
    children: ReactNode
}) {
    return (
        <section
            id={id}
            tabIndex={-1}
            aria-label={title}
            className={cn(
                "relative scroll-mt-16 space-y-1.5 rounded-md outline-none transition-colors",
                pulse != null &&
                    "-mx-2 bg-info-soft/25 px-2 py-1.5 ring-1 ring-info-border",
            )}
        >
            {pulse != null ? (
                <div
                    key={pulse}
                    aria-hidden="true"
                    className="pointer-events-none absolute inset-0 rounded-md bg-info-soft/25 motion-safe:animate-pulse"
                />
            ) : null}
            <h3 className="text-body-compact font-semibold">{title}</h3>
            {children}
        </section>
    )
}

function QuotaEvidence({
    id,
    review,
    shortage,
    warnings,
    pulse,
}: {
    id: string
    review: InvoiceApprovalReview
    shortage?: string
    warnings: readonly string[]
    pulse?: number
}) {
    const { amounts } = review
    return (
        <RiskEvidence id={id} title="开票额度说明" pulse={pulse}>
            <WorkspaceFactGrid columns={3} ariaLabel="开票额度说明">
                <WorkspaceFactItem label="可开票总额" numeric>
                    <MoneyValue
                        value={review.invoiceable_total}
                        display="exact"
                    />
                </WorkspaceFactItem>
                <WorkspaceFactItem label="已开票净额" numeric>
                    <MoneyValue value={review.invoiced_total} display="exact" />
                </WorkspaceFactItem>
                <WorkspaceFactItem label="其他审批中占用" numeric>
                    <MoneyValue value={amounts.other_pending} display="exact" />
                </WorkspaceFactItem>
                <WorkspaceFactItem label="其他已批准待开票" numeric>
                    <MoneyValue
                        value={amounts.other_approved}
                        display="exact"
                    />
                </WorkspaceFactItem>
                <WorkspaceFactItem label="本次有效占用" numeric>
                    <MoneyValue
                        value={amounts.current_reserved}
                        display="exact"
                    />
                </WorkspaceFactItem>
                <WorkspaceFactItem
                    label={shortage ? "超额" : "扣除本次后可再申请"}
                    numeric
                    tone={shortage ? "destructive" : "default"}
                >
                    <MoneyValue
                        value={shortage ?? amounts.available}
                        display="exact"
                        inheritColor={Boolean(shortage)}
                    />
                </WorkspaceFactItem>
            </WorkspaceFactGrid>
            <p className="text-xs text-muted-foreground">
                可开票总额扣除已开票净额及全部有效占用后，余额为正时可继续申请，余额为负时计为超额。已批准申请按未开票金额占用。
            </p>
            {warnings.length ? <WarningList warnings={warnings} /> : null}
        </RiskEvidence>
    )
}

function WarningList({ warnings }: { warnings: readonly string[] }) {
    return (
        <ul className="list-disc space-y-0.5 pl-5 text-xs wrap-anywhere text-warning-soft-foreground">
            {warnings.map((warning) => (
                <li key={warning}>{warning}</li>
            ))}
        </ul>
    )
}

function Loading({ children }: { children: ReactNode }) {
    return (
        <p
            role="status"
            className="flex items-center gap-2 text-xs text-muted-foreground"
        >
            <Spinner />
            {children}
        </p>
    )
}
