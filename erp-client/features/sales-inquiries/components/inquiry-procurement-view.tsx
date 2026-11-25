"use client"

import * as React from "react"
import { createPortal } from "react-dom"
import Link from "next/link"
import {
    ArrowLeftIcon,
    ChevronDownIcon,
    DownloadIcon,
    FileTextIcon,
    FileSpreadsheetIcon,
    PanelRightCloseIcon,
    TriangleAlertIcon,
    UploadIcon,
    XIcon,
} from "lucide-react"
import {
    DecisionKpiBar,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspacePage,
    DetailWorkspaceSaveDock,
    MoneyValue,
    WorkspaceTaskFooter,
    WorkspaceTaskPane,
    detailWorkspaceStyles,
    useWorkspaceTaskNavigation,
    type WorkspaceTaskPaneProps,
} from "@/components/business"
import { useWorkspaceTab } from "@/components/layout/workspace-tab-context"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button, buttonVariants } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { StatusBadge } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"
import { summarizeFeedback } from "../lib/feedback-summary"
import { inquiryDeadlineMeta } from "../lib/format"
import { resolveInquiryBackLink } from "../lib/navigation"
import {
    INQUIRY_STATUS_LABEL,
    INQUIRY_STATUS_TONE,
    type InquiryFeedbackLine,
    type ProcurementInquiry,
} from "../types"
import { InquiryActions } from "./inquiry-actions"
import { InquiryDemandOverview } from "./inquiry-demand-header"
import { InquiryDetailTabs, InquiryHistoryButton } from "./inquiry-detail-tabs"
import { InquiryDemandReference } from "./inquiry-feedback-demand"
import {
    InquiryFeedbackEditor,
    type InquiryFeedbackEditorHandle,
    type InquiryFeedbackEditorStatus,
} from "./inquiry-feedback-editor"
import { InquiryFeedbackResults } from "./inquiry-feedback-results"
import { InquiryFees } from "./inquiry-fees"
import { InquiryInternalQuotesDialog } from "./inquiry-internal-quotes"
import { InquiryReinquiryNotice } from "./inquiry-reinquiry-notice"
import { InquiryRequirementTable } from "./inquiry-requirement-table"
import { SupplierQuoteExportButton } from "./supplier-quote-export-dialog"
import { SupplierQuoteImportButton } from "./supplier-quote-import-dialog"

export function InquiryTaskHeader({
    title,
    status,
    meta,
    actions,
}: {
    title: string
    status?: React.ComponentProps<typeof StatusBadge>
    meta?: React.ReactNode
    actions?: React.ReactNode
}) {
    const navigation = useWorkspaceTaskNavigation()
    const inWorkspaceTab = useWorkspaceTab()
    const back =
        !inWorkspaceTab || navigation?.back?.visibility === "always"
            ? navigation?.back
            : undefined

    return (
        <header
            data-slot="detail-workspace-chrome"
            className="flex min-h-14 min-w-0 flex-wrap items-center justify-between gap-x-3 gap-y-1 border-b border-border/60 bg-card py-1.5"
        >
            <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1">
                {back?.href ? (
                    <>
                        <Link
                            id={back.id}
                            href={back.href}
                            className={cn(
                                buttonVariants({
                                    variant: "ghost",
                                    size: "xs",
                                }),
                                "-ml-1.5 shrink-0 text-muted-foreground hover:text-foreground",
                            )}
                        >
                            <ArrowLeftIcon aria-hidden="true" />
                            {back.label}
                        </Link>
                        <span
                            aria-hidden="true"
                            className="h-4 w-px shrink-0 bg-border/70"
                        />
                    </>
                ) : back?.onClick ? (
                    <>
                        <Button
                            id={back.id}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className="-ml-1.5 shrink-0 text-muted-foreground hover:text-foreground"
                            onClick={back.onClick}
                        >
                            <ArrowLeftIcon aria-hidden="true" />
                            {back.label}
                        </Button>
                        <span
                            aria-hidden="true"
                            className="h-4 w-px shrink-0 bg-border/70"
                        />
                    </>
                ) : null}
                <h1
                    className="min-w-0 truncate text-section-title font-semibold tracking-tight"
                    title={title}
                >
                    {title}
                </h1>
                {status && <StatusBadge {...status} />}
                {meta}
            </div>
            <div className="ml-auto flex min-w-0 flex-wrap items-center justify-end gap-1.5">
                {navigation?.meta}
                {actions}
            </div>
        </header>
    )
}

function ProgressBar({
    complete,
    total,
    className,
}: {
    complete: number
    total: number
    className?: string
}) {
    const percent =
        total > 0 ? Math.min(100, Math.round((complete / total) * 100)) : 0
    return (
        <span
            aria-hidden="true"
            className={cn(
                "inline-flex h-1.5 overflow-hidden rounded-full bg-muted",
                className,
            )}
        >
            <span
                className={cn(
                    "h-full rounded-full transition-[width] duration-200",
                    percent === 100 ? "bg-success" : "bg-primary",
                )}
                style={{ width: `${percent}%` }}
            />
        </span>
    )
}

export function InquiryProcurementView({
    inquiry,
    taskId,
    embedded,
    userId,
    permissions,
    returnTo,
    onSaved,
    taskHeaderActions,
    TaskPane = WorkspaceTaskPane,
}: {
    inquiry: ProcurementInquiry
    taskId?: string
    embedded: boolean
    userId: string
    permissions: readonly string[]
    returnTo?: string | null
    onSaved: () => void
    taskHeaderActions?: React.ReactNode
    TaskPane?: React.ComponentType<WorkspaceTaskPaneProps>
}) {
    const [feedbackDirty, setFeedbackDirty] = React.useState(false)
    const [publishing, setPublishing] = React.useState(false)
    const [exportQuotesOpen, setExportQuotesOpen] = React.useState(false)
    const [importQuotesOpen, setImportQuotesOpen] = React.useState(false)
    const [referenceOpen, setReferenceOpen] = React.useState(false)
    const [wide, setWide] = React.useState(false)
    const embeddedRoot = React.useRef<HTMLDivElement>(null)
    const [bodyHost, setBodyHost] = React.useState<HTMLDivElement | null>(null)
    const bodyHostRef = React.useRef<HTMLDivElement>(null)
    const referenceTrigger = React.useRef<HTMLButtonElement>(null)
    const feedbackPanelVisible = React.useRef(true)
    const locationFrame = React.useRef(0)
    const referencePrefix = `procurement-inquiry-reference-${toAutomationIdSegment(taskId ?? inquiry.id)}`
    const feedbackEditor = React.useRef<InquiryFeedbackEditorHandle>(null)
    const mountBodyHost = React.useCallback((slot: HTMLDivElement | null) => {
        if (!slot) return
        // 共享壳切换单栏和双栏时重建槽位，编辑器继续使用同一个挂载点。
        if (!bodyHostRef.current) {
            const host = document.createElement("div")
            host.className =
                "min-h-0 min-w-0 flex-1 overflow-y-auto overscroll-contain"
            bodyHostRef.current = host
            setBodyHost(host)
        }
        slot.append(bodyHostRef.current)
    }, [])
    const [quoteLineId, setQuoteLineId] = React.useState<string | null>(null)
    const [feedbackLines, setFeedbackLines] = React.useState<
        readonly InquiryFeedbackLine[]
    >([])
    const [feedbackStatus, setFeedbackStatus] =
        React.useState<InquiryFeedbackEditorStatus>({
            dirty: false,
            saving: false,
            disabled: false,
            completeCount: 0,
            totalCount: inquiry.draft.lines.length,
            availableCount: 0,
            partialCount: 0,
            unavailableCount: 0,
            pendingCount: inquiry.draft.lines.length,
            pricedCount: 0,
            goodsTotal: null,
            feeCount: 0,
            feesTotal: null,
            grandTotal: null,
            saved: false,
        })

    React.useEffect(() => {
        const root = embeddedRoot.current
        if (!root) return
        const viewport = window.matchMedia("(min-width: 960px)")
        const canvas = root.closest<HTMLElement>("#main-content") ?? root
        const sync = () => {
            const canSplit = viewport.matches && canvas.clientWidth >= 960
            setWide(canSplit)
            if (!canSplit) setReferenceOpen(false)
        }
        sync()
        const observer =
            typeof ResizeObserver === "undefined"
                ? null
                : new ResizeObserver(sync)
        observer?.observe(canvas)
        viewport.addEventListener("change", sync)
        return () => {
            observer?.disconnect()
            viewport.removeEventListener("change", sync)
        }
    }, [])

    React.useEffect(() => {
        if (!embedded) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action === "toggle-rail") setReferenceOpen((open) => !open)
        })
    }, [embedded])

    React.useEffect(() => {
        if (!bodyHostRef.current) return
        bodyHostRef.current.className = cn(
            "min-h-0 min-w-0 flex-1 overflow-y-auto overscroll-contain",
            wide && referenceOpen && "pr-4",
        )
    }, [wide, referenceOpen])

    React.useEffect(() => () => cancelAnimationFrame(locationFrame.current), [])

    const onFeedbackVisibilityChange = React.useCallback((visible: boolean) => {
        feedbackPanelVisible.current = visible
    }, [])

    const inFeedbackPanel = (locate: () => void) => {
        if (feedbackPanelVisible.current) {
            locate()
            return
        }
        document.getElementById("sales-inquiry-tab-feedback")?.click()
        cancelAnimationFrame(locationFrame.current)
        locationFrame.current = requestAnimationFrame(locate)
    }

    const roundContext = `${inquiry.requirement_version}:${inquiry.feedback_round}:${inquiry.status}`
    const [roundSelection, setRoundSelection] = React.useState<{
        context: string
        round: number | null
    } | null>(null)
    const chosenRound =
        roundSelection?.context === roundContext ? roundSelection.round : null
    const setChosenRound = (round: number | null) =>
        setRoundSelection({ context: roundContext, round })

    const selectedFeedback =
        inquiry.feedbacks.find((item) => item.round === chosenRound) ??
        [...inquiry.feedbacks].sort(
            (left, right) => right.round - left.round,
        )[0]
    const selectedRound = selectedFeedback?.round ?? inquiry.feedback_round

    const matchingOpenTask = inquiry.work_items.find(
        (item) => item.id === taskId && item.status === "OPEN",
    )
    const task =
        embedded && taskId
            ? matchingOpenTask
            : (matchingOpenTask ??
              inquiry.work_items.find((item) => item.status === "OPEN"))
    const completedTask = inquiry.work_items.find(
        (item) => item.id === taskId && item.status === "COMPLETED",
    )

    const canProcess = Boolean(
        task &&
        ["awaiting_procurement", "procurement_processing"].includes(
            inquiry.status,
        ) &&
        hasPermission(permissions, "procurement_inquiry:save_feedback"),
    )
    const showReadonlyDemand =
        !canProcess &&
        !inquiry.feedbacks.some(
            (feedback) =>
                feedback.requirement_version === inquiry.requirement_version,
        )

    useWorkspaceDirtyState({
        dirty: canProcess && feedbackDirty,
        busy: publishing || feedbackStatus.saving,
    })

    // 嵌入式任务面同步支持 Cmd/Ctrl + S 快捷保存，与独立详情页 SaveDock 手感一致。
    React.useEffect(() => {
        if (!embedded || !canProcess || !feedbackDirty) return
        const onKeyDown = (event: KeyboardEvent) => {
            if (
                event.defaultPrevented ||
                event.isComposing ||
                event.repeat ||
                event.altKey ||
                event.shiftKey ||
                !(event.metaKey || event.ctrlKey) ||
                event.key.toLowerCase() !== "s" ||
                (event.target instanceof Element &&
                    event.target.closest(
                        '[role="dialog"], [role="alertdialog"]',
                    ))
            )
                return
            event.preventDefault()
            if (!feedbackStatus.saving && !feedbackStatus.disabled) {
                feedbackEditor.current?.save()
            }
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [
        embedded,
        canProcess,
        feedbackDirty,
        feedbackStatus.saving,
        feedbackStatus.disabled,
    ])

    const back = resolveInquiryBackLink({
        id: "sales-inquiry-detail-back",
        procurement: true,
        returnTo,
    })

    const canExportQuotes =
        hasPermission(permissions, "supplier_offering:list") &&
        hasPermission(permissions, "sellable_sku:list")
    const canImportQuotes =
        hasPermission(permissions, "procurement_inquiry:internal_quote") &&
        hasPermission(permissions, "supplier_offering:list") &&
        hasPermission(permissions, "sellable_sku:list")

    const quoteWorkbookActions =
        canProcess && task && (canExportQuotes || canImportQuotes) ? (
            canExportQuotes && canImportQuotes ? (
                <>
                    <DropdownMenu>
                        <DropdownMenuTrigger
                            render={
                                <Button
                                    id="procurement-inquiry-quote-workbook-menu"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={cn(
                                        detailWorkspaceStyles.subtleButton,
                                        "gap-1.5",
                                    )}
                                />
                            }
                        >
                            <FileSpreadsheetIcon
                                aria-hidden="true"
                                className="size-3.5 text-muted-foreground"
                            />
                            <span>报价表</span>
                            <ChevronDownIcon
                                aria-hidden="true"
                                className="size-3.5 text-muted-foreground/80"
                            />
                        </DropdownMenuTrigger>
                        <DropdownMenuContent align="end" className="w-56 p-1.5">
                            <DropdownMenuItem
                                id="procurement-inquiry-export-supplier-quotes"
                                className="items-start gap-2.5 py-2"
                                onClick={() => setExportQuotesOpen(true)}
                            >
                                <DownloadIcon
                                    aria-hidden="true"
                                    className="mt-0.5 size-4 text-muted-foreground"
                                />
                                <div className="min-w-0">
                                    <p className="font-medium">
                                        生成供应商报价表
                                    </p>
                                    <p className="text-xs text-muted-foreground">
                                        按供给关系导出发给供应商
                                    </p>
                                </div>
                            </DropdownMenuItem>
                            <DropdownMenuItem
                                id="procurement-inquiry-import-quotes"
                                className="items-start gap-2.5 py-2"
                                onClick={() => setImportQuotesOpen(true)}
                            >
                                <UploadIcon
                                    aria-hidden="true"
                                    className="mt-0.5 size-4 text-muted-foreground"
                                />
                                <div className="min-w-0">
                                    <p className="font-medium">
                                        导入供应商报价
                                    </p>
                                    <p className="text-xs text-muted-foreground">
                                        上传供应商回填表格登记报价
                                    </p>
                                </div>
                            </DropdownMenuItem>
                        </DropdownMenuContent>
                    </DropdownMenu>
                    <SupplierQuoteExportButton
                        hideTrigger
                        open={exportQuotesOpen}
                        onOpenChange={setExportQuotesOpen}
                        request={{
                            inquiryId: inquiry.id,
                            taskId: task.id,
                            version: inquiry.version,
                            taskVersion: task.version,
                        }}
                        userId={userId}
                        onRefresh={onSaved}
                    />
                    <SupplierQuoteImportButton
                        hideTrigger
                        open={importQuotesOpen}
                        onOpenChange={setImportQuotesOpen}
                        inquiry={inquiry}
                        task={task}
                        userId={userId}
                        onSaved={onSaved}
                    />
                </>
            ) : (
                <>
                    {canExportQuotes && (
                        <SupplierQuoteExportButton
                            request={{
                                inquiryId: inquiry.id,
                                taskId: task.id,
                                version: inquiry.version,
                                taskVersion: task.version,
                            }}
                            userId={userId}
                            onRefresh={onSaved}
                        />
                    )}
                    {canImportQuotes && (
                        <SupplierQuoteImportButton
                            inquiry={inquiry}
                            task={task}
                            userId={userId}
                            onSaved={onSaved}
                        />
                    )}
                </>
            )
        ) : undefined

    const saveDockOpen = !embedded && canProcess && feedbackDirty
    const isActiveInquiry = !["confirmed", "closed"].includes(inquiry.status)
    const deadline = inquiryDeadlineMeta(inquiry.deadline_at, isActiveInquiry)
    const feeOwnerLabel = inquiry.shared_fee_owner_user_id
        ? inquiry.shared_fee_owner_name ||
          (inquiry.shared_fee_owner_user_id === userId
              ? "由我维护"
              : "已指定责任采购")
        : null

    const selectedRequirementLines =
        inquiry.requirements.find(
            (item) => item.version === selectedFeedback?.requirement_version,
        )?.data.lines ?? inquiry.draft.lines
    const publishedSummary = React.useMemo(
        () =>
            selectedFeedback
                ? summarizeFeedback(
                      selectedFeedback.data.lines,
                      selectedRequirementLines,
                      selectedFeedback.data.fees,
                  )
                : null,
        [selectedFeedback, selectedRequirementLines],
    )

    const feeCount = selectedFeedback?.data.fees.length ?? 0
    const displayFeeCount = canProcess ? feedbackStatus.feeCount : feeCount
    const displayFeeTotal = canProcess
        ? feedbackStatus.feesTotal
        : (publishedSummary?.feesTotal ?? null)
    const displayGoodsTotal = canProcess
        ? feedbackStatus.goodsTotal
        : (publishedSummary?.goodsTotal ?? null)
    const scopeLineCount = task
        ? task.line_ids.length
        : inquiry.draft.lines.length

    const summaryBar = embedded ? undefined : (
        <DetailWorkspaceMetrics
            columns={4}
            variant="bar"
            ariaLabel="采购询价概览"
        >
            <DetailWorkspaceMetricItem
                label={canProcess ? "报价进度" : "商品与反馈"}
                emphasis="primary"
                value={
                    canProcess ? (
                        <span className="inline-flex items-baseline gap-1.5">
                            <span className="num">
                                {feedbackStatus.completeCount}
                            </span>
                            <span className="text-body-compact font-normal text-muted-foreground">
                                /{" "}
                                <span className="num">
                                    {feedbackStatus.totalCount}
                                </span>{" "}
                                项
                            </span>
                        </span>
                    ) : (
                        <span>
                            <span className="num">{scopeLineCount}</span>{" "}
                            <span className="text-body-compact font-normal text-muted-foreground">
                                项
                            </span>
                        </span>
                    )
                }
                detail={
                    canProcess ? (
                        <span className="flex w-full flex-col gap-1.5">
                            <ProgressBar
                                complete={feedbackStatus.completeCount}
                                total={feedbackStatus.totalCount}
                                className="w-28"
                            />
                            <span>
                                可供{" "}
                                <span className="num text-foreground">
                                    {feedbackStatus.availableCount}
                                </span>
                                {feedbackStatus.partialCount > 0 ? (
                                    <>
                                        {" · "}部分{" "}
                                        <span className="num text-warning-soft-foreground">
                                            {feedbackStatus.partialCount}
                                        </span>
                                    </>
                                ) : null}
                                {feedbackStatus.unavailableCount > 0 ? (
                                    <>
                                        {" · "}不供应{" "}
                                        <span className="num">
                                            {feedbackStatus.unavailableCount}
                                        </span>
                                    </>
                                ) : null}
                                {feedbackStatus.pendingCount > 0 ? (
                                    <>
                                        {" · "}待确认{" "}
                                        <span className="num text-warning-soft-foreground">
                                            {feedbackStatus.pendingCount}
                                        </span>
                                    </>
                                ) : null}
                            </span>
                        </span>
                    ) : publishedSummary ? (
                        `可供 ${publishedSummary.availableCount} · 部分 ${publishedSummary.partialCount} · 不供应 ${publishedSummary.unavailableCount}`
                    ) : (
                        "等待报价处理"
                    )
                }
            />
            <DetailWorkspaceMetricItem
                label="含税供给合计"
                value={
                    displayGoodsTotal !== null ? (
                        <MoneyValue
                            value={displayGoodsTotal}
                            display="exact"
                            showHint={false}
                        />
                    ) : (
                        <span className="text-muted-foreground">—</span>
                    )
                }
                detail={
                    canProcess
                        ? feedbackStatus.pricedCount > 0
                            ? `已定价 ${feedbackStatus.pricedCount} / ${feedbackStatus.totalCount} 项`
                            : "填写单价后自动汇总"
                        : selectedFeedback
                          ? `第 ${selectedFeedback.round} 轮商品供给小计`
                          : "暂无商品报价"
                }
            />
            <DetailWorkspaceMetricItem
                label="额外成本"
                value={
                    displayFeeTotal !== null ? (
                        <MoneyValue
                            value={displayFeeTotal}
                            display="exact"
                            showHint={false}
                        />
                    ) : (
                        <span className="text-muted-foreground">—</span>
                    )
                }
                detail={
                    canProcess ? (
                        <span className="inline-flex items-center gap-2">
                            <span>
                                {displayFeeCount > 0
                                    ? `共 ${displayFeeCount} 项`
                                    : "暂无额外成本"}
                            </span>
                            <button
                                id="procurement-inquiry-metric-fees"
                                type="button"
                                className="text-primary underline-offset-4 hover:underline"
                                onClick={() =>
                                    feedbackEditor.current?.openFees()
                                }
                            >
                                {displayFeeCount > 0 ? "管理" : "去登记"}
                            </button>
                        </span>
                    ) : displayFeeCount > 0 ? (
                        `共 ${displayFeeCount} 项额外费用`
                    ) : (
                        "暂无额外费用"
                    )
                }
            />
            <DetailWorkspaceMetricItem
                label="询价截止"
                value={deadline.label}
                detail={
                    <span className="flex flex-wrap items-center gap-x-1.5">
                        {deadline.hint ? (
                            <span
                                className={cn(
                                    deadline.tone === "destructive" &&
                                        "font-medium text-destructive",
                                    deadline.tone === "warning" &&
                                        "font-medium text-warning-soft-foreground",
                                )}
                            >
                                {deadline.hint}
                            </span>
                        ) : null}
                        {deadline.hint ? (
                            <span
                                aria-hidden="true"
                                className="text-muted-foreground/40"
                            >
                                ·
                            </span>
                        ) : null}
                        <span className="num">
                            {inquiry.feedback_round > 0
                                ? `第 ${inquiry.feedback_round} 轮反馈`
                                : `需求第 ${inquiry.requirement_version} 版`}
                        </span>
                    </span>
                }
            />
        </DetailWorkspaceMetrics>
    )

    const content = (
        <InquiryActions
            workspace
            fillHeight={embedded}
            inquiry={inquiry}
            procurement
            task={task ?? completedTask}
            userId={userId}
            permissions={permissions}
            onSaved={onSaved}
            feedbackDirty={feedbackDirty}
            validateBeforePublish={
                canProcess
                    ? () =>
                          feedbackEditor.current?.validateBeforePublish() ??
                          true
                    : undefined
            }
            beforePublish={
                canProcess
                    ? async () => {
                          if (!feedbackEditor.current)
                              throw new Error(
                                  "采购反馈正在恢复，请稍候再发布。",
                              )
                          return feedbackEditor.current.saveForPublish()
                      }
                    : undefined
            }
            onPublishingChange={setPublishing}
            feedbackRound={selectedRound}
            renderActions={({
                primaryAction,
                secondaryActions,
                assignmentActions,
                alerts: actionAlerts,
            }) => {
                const headerMetaItems = [
                    inquiry.requirement_version > 0 && (
                        <span key="req" className="num">
                            需求第 {inquiry.requirement_version} 版
                        </span>
                    ),
                    inquiry.feedback_round > 0 && (
                        <span key="round" className="num">
                            反馈第 {inquiry.feedback_round} 轮
                        </span>
                    ),
                    inquiry.deadline_at > 0 && (
                        <span key="deadline">
                            截止：
                            <span className="num">{deadline.label}</span>
                            {embedded && deadline.hint ? (
                                <span
                                    className={cn(
                                        "ml-1",
                                        deadline.tone === "destructive" &&
                                            "font-medium text-destructive",
                                        deadline.tone === "warning" &&
                                            "font-medium text-warning-soft-foreground",
                                    )}
                                >
                                    （{deadline.hint}）
                                </span>
                            ) : null}
                        </span>
                    ),
                    feeOwnerLabel && (
                        <span key="fee-owner">
                            整单费用：
                            <span className="text-foreground/85">
                                {feeOwnerLabel}
                            </span>
                        </span>
                    ),
                ].filter(Boolean)

                const headerMeta =
                    headerMetaItems.length > 0 ? (
                        <DetailWorkspaceMeta items={headerMetaItems} />
                    ) : undefined

                const headerSecondary = (
                    <>
                        <InquiryHistoryButton
                            inquiry={inquiry}
                            variant={embedded ? "ghost" : "outline"}
                        />
                        {assignmentActions}
                        {!embedded && secondaryActions}
                    </>
                )

                const alerts = (
                    <>
                        {actionAlerts}
                        {!task && inquiry.feedbacks.length > 0 && (
                            <Alert variant="info">
                                <AlertDescription>
                                    当前展示您在本轮提交的报价反馈；若销售发起新一轮询价，请前往新的待办任务处理。
                                </AlertDescription>
                            </Alert>
                        )}
                    </>
                )

                const overview = (
                    <InquiryDemandOverview
                        inquiry={inquiry}
                        userId={userId}
                        isCurrentNote={
                            !selectedFeedback ||
                            selectedFeedback.requirement_version ===
                                inquiry.requirement_version
                        }
                    />
                )

                const feedbackPanel = (
                    <div className="space-y-4">
                        <InquiryReinquiryNotice inquiry={inquiry} />
                        {canProcess && task && (
                            <>
                                <InquiryFeedbackEditor
                                    ref={feedbackEditor}
                                    workbench={embedded}
                                    publishing={publishing}
                                    onDirtyChange={setFeedbackDirty}
                                    onStatusChange={setFeedbackStatus}
                                    onLinesChange={setFeedbackLines}
                                    inquiry={inquiry}
                                    task={task}
                                    userId={userId}
                                    onSaved={onSaved}
                                    quoteWorkbookActions={quoteWorkbookActions}
                                    onInternalQuote={
                                        hasPermission(
                                            permissions,
                                            "procurement_inquiry:internal_quote",
                                        )
                                            ? setQuoteLineId
                                            : undefined
                                    }
                                />
                                {hasPermission(
                                    permissions,
                                    "procurement_inquiry:internal_quote",
                                ) && (
                                    <InquiryInternalQuotesDialog
                                        open={quoteLineId !== null}
                                        onOpenChange={(open) => {
                                            if (!open) setQuoteLineId(null)
                                        }}
                                        lineId={quoteLineId}
                                        onLineIdChange={setQuoteLineId}
                                        inquiry={inquiry}
                                        task={task}
                                        userId={userId}
                                        permissions={permissions}
                                        feedbackLines={feedbackLines}
                                        onSaved={onSaved}
                                        onAdopt={(quote) =>
                                            feedbackEditor.current?.adoptQuote(
                                                quote,
                                            ) ?? false
                                        }
                                    />
                                )}
                            </>
                        )}
                        {!canProcess &&
                            (showReadonlyDemand ? (
                                embedded ? (
                                    <InquiryDemandReference
                                        idPrefix={`${referencePrefix}-readonly`}
                                        requirement={inquiry.draft}
                                    />
                                ) : (
                                    <InquiryRequirementTable
                                        idPrefix="sales-inquiry-requirement"
                                        requirement={inquiry.draft}
                                        procurement
                                    />
                                )
                            ) : (
                                <InquiryFeedbackResults
                                    content="products"
                                    workbench={embedded}
                                    inquiry={inquiry}
                                    selectedRound={selectedRound}
                                    onRoundChange={setChosenRound}
                                />
                            ))}
                    </div>
                )

                const publishedPanel =
                    (canProcess || showReadonlyDemand) &&
                    inquiry.feedbacks.length > 0 ? (
                        <InquiryFeedbackResults
                            content="products"
                            workbench={embedded}
                            inquiry={inquiry}
                            selectedRound={selectedRound}
                            onRoundChange={setChosenRound}
                            idPrefix="procurement-inquiry-published"
                        />
                    ) : undefined

                const footerTotal =
                    feedbackStatus.feeCount > 0 &&
                    feedbackStatus.grandTotal !== null
                        ? feedbackStatus.grandTotal
                        : feedbackStatus.goodsTotal
                const remainingCount = Math.max(
                    0,
                    feedbackStatus.totalCount - feedbackStatus.completeCount,
                )

                const embeddedAnomalyCount = Math.max(
                    0,
                    remainingCount - feedbackStatus.pendingCount,
                )
                const embeddedKpiBand = embedded ? (
                    <DecisionKpiBar
                        columns={4}
                        ariaLabel="询价处理关键指标"
                        items={[
                            {
                                id: "progress",
                                label: canProcess ? "报价进度" : "商品与反馈",
                                emphasis: "primary",
                                formulaTooltipId: `${referencePrefix}-kpi-progress-help`,
                                formulaTooltipLabel: "报价进度口径",
                                formulaTooltip:
                                    "已完善项数 = 已确定供给结论且价格、数量与原因无缺漏的商品行数。",
                                value: canProcess
                                    ? feedbackStatus.completeCount
                                    : scopeLineCount,
                                valueMeta: (
                                    <span className="text-xs font-normal text-muted-foreground">
                                        {canProcess ? (
                                            <>
                                                /{" "}
                                                <span className="num">
                                                    {feedbackStatus.totalCount}
                                                </span>{" "}
                                                项
                                            </>
                                        ) : (
                                            "项"
                                        )}
                                    </span>
                                ),
                                footnote:
                                    canProcess && feedbackStatus.totalCount > 0
                                        ? undefined
                                        : publishedSummary
                                          ? `可供 ${publishedSummary.availableCount} · 部分 ${publishedSummary.partialCount} · 不供应 ${publishedSummary.unavailableCount}`
                                          : "等待报价处理",
                                extra:
                                    canProcess &&
                                    feedbackStatus.totalCount > 0 ? (
                                        <div className="space-y-1">
                                            <div
                                                aria-hidden="true"
                                                className="flex h-1.5 overflow-hidden rounded-full bg-border"
                                            >
                                                {feedbackStatus.completeCount >
                                                0 ? (
                                                    <span
                                                        className="bg-success"
                                                        style={{
                                                            flexGrow:
                                                                feedbackStatus.completeCount,
                                                        }}
                                                    />
                                                ) : null}
                                                {feedbackStatus.pendingCount >
                                                0 ? (
                                                    <span
                                                        className="bg-warning"
                                                        style={{
                                                            flexGrow:
                                                                feedbackStatus.pendingCount,
                                                        }}
                                                    />
                                                ) : null}
                                                {embeddedAnomalyCount > 0 ? (
                                                    <span
                                                        className="bg-destructive"
                                                        style={{
                                                            flexGrow:
                                                                embeddedAnomalyCount,
                                                        }}
                                                    />
                                                ) : null}
                                            </div>
                                            <p className="text-tiny text-muted-foreground">
                                                可供{" "}
                                                <span className="num text-foreground">
                                                    {
                                                        feedbackStatus.availableCount
                                                    }
                                                </span>
                                                {feedbackStatus.partialCount >
                                                0 ? (
                                                    <>
                                                        {" · "}部分{" "}
                                                        <span className="num text-warning-soft-foreground">
                                                            {
                                                                feedbackStatus.partialCount
                                                            }
                                                        </span>
                                                    </>
                                                ) : null}
                                                {feedbackStatus.unavailableCount >
                                                0 ? (
                                                    <>
                                                        {" · "}不供{" "}
                                                        <span className="num">
                                                            {
                                                                feedbackStatus.unavailableCount
                                                            }
                                                        </span>
                                                    </>
                                                ) : null}
                                            </p>
                                        </div>
                                    ) : undefined,
                            },
                            {
                                id: "goods-total",
                                label: "含税供给合计",
                                emphasis: "secondary",
                                formulaTooltipId: `${referencePrefix}-kpi-total-help`,
                                formulaTooltipLabel: "含税供给合计计算公式",
                                formulaTooltip:
                                    "含税供给合计 = ∑(可供数量 × 含税供给单价)，不含整单额外成本。",
                                value:
                                    displayGoodsTotal !== null ? (
                                        <MoneyValue
                                            value={displayGoodsTotal}
                                            display="exact"
                                            showHint={false}
                                        />
                                    ) : (
                                        <span className="text-muted-foreground">
                                            —
                                        </span>
                                    ),
                                footnote: canProcess
                                    ? feedbackStatus.pricedCount > 0
                                        ? `已定价 ${feedbackStatus.pricedCount} / ${feedbackStatus.totalCount} 项 · 含税`
                                        : "填写单价后自动汇总"
                                    : selectedFeedback
                                      ? `第 ${selectedFeedback.round} 轮商品供给小计`
                                      : "暂无商品报价",
                            },
                            {
                                id: "fees",
                                label: "额外成本",
                                emphasis: "secondary",
                                value:
                                    displayFeeTotal !== null ? (
                                        <MoneyValue
                                            value={displayFeeTotal}
                                            display="exact"
                                            showHint={false}
                                        />
                                    ) : (
                                        <span className="text-muted-foreground">
                                            —
                                        </span>
                                    ),
                                footnote: (
                                    <>
                                        <span>
                                            {displayFeeCount > 0
                                                ? `共 ${displayFeeCount} 项`
                                                : "暂无额外成本"}
                                        </span>
                                        {canProcess ? (
                                            <button
                                                id="procurement-inquiry-metric-fees"
                                                type="button"
                                                className="cursor-pointer text-primary underline-offset-4 hover:underline"
                                                onClick={() =>
                                                    feedbackEditor.current?.openFees()
                                                }
                                            >
                                                {displayFeeCount > 0
                                                    ? "管理"
                                                    : "去登记"}
                                            </button>
                                        ) : null}
                                    </>
                                ),
                            },
                            {
                                id: "deadline",
                                label: "询价截止与轮次",
                                emphasis: "terms",
                                value: deadline.label,
                                footnote: (
                                    <>
                                        {deadline.hint ? (
                                            <span
                                                className={cn(
                                                    deadline.tone ===
                                                        "destructive" &&
                                                        "font-medium text-destructive",
                                                    deadline.tone ===
                                                        "warning" &&
                                                        "font-medium text-warning-soft-foreground",
                                                )}
                                            >
                                                {deadline.hint}
                                            </span>
                                        ) : null}
                                        {deadline.hint ? (
                                            <span
                                                aria-hidden="true"
                                                className="text-muted-foreground/40"
                                            >
                                                ·
                                            </span>
                                        ) : null}
                                        <span className="num">
                                            {inquiry.feedback_round > 0
                                                ? `第 ${inquiry.feedback_round} 轮反馈`
                                                : `需求第 ${inquiry.requirement_version} 版`}
                                        </span>
                                    </>
                                ),
                            },
                        ]}
                    />
                ) : null

                const footerMetrics = canProcess
                    ? [
                          { label: "商品", value: feedbackStatus.totalCount },
                          {
                              label: "已完善",
                              value: feedbackStatus.completeCount,
                          },
                          {
                              label: "待回复",
                              value: feedbackStatus.pendingCount,
                          },
                          {
                              label: "异常",
                              value: embeddedAnomalyCount,
                          },
                      ]
                    : publishedSummary
                      ? [{ label: "商品", value: publishedSummary.totalCount }]
                      : []
                const estimatedTotal = canProcess
                    ? footerTotal
                    : publishedSummary?.goodsTotal
                const taskFooterSummary =
                    footerMetrics.length > 0 ? (
                        <dl className="flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-1 text-xs text-muted-foreground">
                            {footerMetrics.map(({ label, value }) => (
                                <div
                                    key={label}
                                    className="flex items-baseline gap-1"
                                >
                                    <dt>{label}</dt>
                                    <dd
                                        className={cn(
                                            "num font-medium text-foreground",
                                            label === "异常" &&
                                                value > 0 &&
                                                "text-destructive",
                                            label === "待回复" &&
                                                value > 0 &&
                                                "text-warning-soft-foreground",
                                        )}
                                    >
                                        {value}
                                    </dd>
                                </div>
                            ))}
                            {estimatedTotal != null && (
                                <div className="flex items-baseline gap-1 border-l border-border/60 pl-3">
                                    <dt>
                                        {canProcess
                                            ? "预估含税总计"
                                            : "含税合计"}
                                    </dt>
                                    <dd className="font-semibold text-foreground">
                                        <MoneyValue
                                            value={estimatedTotal}
                                            display="exact"
                                            showHint={false}
                                        />
                                    </dd>
                                </div>
                            )}
                        </dl>
                    ) : undefined

                const taskFooterActions = canProcess ? (
                    feedbackStatus.unresolved ? (
                        <Button
                            id="procurement-inquiry-footer-recover"
                            type="button"
                            size="sm"
                            variant="default"
                            disabled={feedbackStatus.saving}
                            onClick={() => feedbackEditor.current?.recover()}
                        >
                            {feedbackStatus.saving
                                ? "正在核对…"
                                : "核对提交结果"}
                        </Button>
                    ) : (
                        <>
                            {remainingCount > 0 && (
                                <Button
                                    id="procurement-inquiry-focus-incomplete"
                                    type="button"
                                    size="sm"
                                    variant="ghost"
                                    className="text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground"
                                    onClick={() =>
                                        inFeedbackPanel(() =>
                                            feedbackEditor.current?.focusFirstIncomplete(),
                                        )
                                    }
                                >
                                    <TriangleAlertIcon aria-hidden="true" />
                                    <span>
                                        <span className="num">
                                            {remainingCount}
                                        </span>{" "}
                                        项待完善 · 定位
                                    </span>
                                </Button>
                            )}
                            {secondaryActions}
                            <Button
                                id="procurement-inquiry-save-feedback"
                                type="button"
                                size="sm"
                                variant={feedbackDirty ? "outline" : "ghost"}
                                className={
                                    feedbackDirty
                                        ? undefined
                                        : detailWorkspaceStyles.quietButton
                                }
                                disabled={feedbackStatus.disabled}
                                onClick={() => feedbackEditor.current?.save()}
                            >
                                {feedbackStatus.saving
                                    ? "正在保存…"
                                    : "保存处理进度"}
                            </Button>
                            {primaryAction}
                        </>
                    )
                ) : secondaryActions || primaryAction ? (
                    <>
                        {secondaryActions}
                        {primaryAction}
                    </>
                ) : undefined

                const locateDemand = (lineId: string) => {
                    if (!wide) setReferenceOpen(false)
                    inFeedbackPanel(() =>
                        feedbackEditor.current?.locateLine(lineId),
                    )
                }
                const referenceProps = {
                    idPrefix: referencePrefix,
                    requirement: inquiry.draft,
                    onLocate: canProcess ? locateDemand : undefined,
                }
                const inquiryMeta = (
                    <DetailWorkspaceMeta
                        items={[
                            inquiry.customer_name !== "询价采购处理"
                                ? inquiry.customer_name
                                : null,
                            <span
                                key="number"
                                className="num select-all"
                                data-workspace-document-number={
                                    inquiry.inquiry_no
                                }
                            >
                                {inquiry.inquiry_no}
                            </span>,
                            inquiry.deadline_at > 0 ? (
                                <span
                                    key="deadline"
                                    className={cn(
                                        deadline.tone === "destructive" &&
                                            "text-destructive",
                                        deadline.tone === "warning" &&
                                            "text-warning-soft-foreground",
                                    )}
                                >
                                    截止{" "}
                                    <span className="num">
                                        {deadline.label}
                                    </span>
                                    {deadline.hint && ` · ${deadline.hint}`}
                                </span>
                            ) : null,
                        ]}
                    />
                )

                return (
                    <>
                        <InquiryDetailTabs
                            workspace={!embedded}
                            inquiry={inquiry}
                            procurement
                            feeCount={displayFeeCount}
                            feedback={feedbackPanel}
                            publishedFeedback={publishedPanel}
                            onFeedbackVisibilityChange={
                                embedded
                                    ? onFeedbackVisibilityChange
                                    : undefined
                            }
                            fees={
                                <InquiryFees
                                    inquiry={inquiry}
                                    selectedRound={selectedRound}
                                    onRoundChange={setChosenRound}
                                    userId={userId}
                                    permissions={permissions}
                                    procurement
                                    handlingEnabled={Boolean(task)}
                                    onChanged={onSaved}
                                />
                            }
                            renderLayout={(navigation, panels) =>
                                embedded ? (
                                    <>
                                        {bodyHost &&
                                            createPortal(
                                                <>
                                                    <div className="space-y-2 pt-2 empty:hidden">
                                                        {alerts}
                                                    </div>
                                                    {embeddedKpiBand ? (
                                                        <div className="pt-3">
                                                            {embeddedKpiBand}
                                                        </div>
                                                    ) : null}
                                                    <div className="min-w-0">
                                                        {panels}
                                                    </div>
                                                </>,
                                                bodyHost,
                                            )}
                                        <TaskPane
                                            aria-label="询价采购处理任务"
                                            idPrefix={referencePrefix}
                                            archetype="action"
                                            className="@container/inquiry-workbench bg-card"
                                            contextRail={
                                                wide
                                                    ? {
                                                          label: "对照询价需求",
                                                          allowOnFullWidth: true,
                                                          collapsible: true,
                                                          resizable: true,
                                                          defaultWidthPercent: 38,
                                                          minWidthPercent: 26,
                                                          maxWidthPercent: 52,
                                                          storageKey:
                                                              "erp.workspace.salesInquiryRefRailWidth",
                                                          collapsed:
                                                              !referenceOpen,
                                                          onCollapsedChange: (
                                                              collapsed,
                                                          ) =>
                                                              setReferenceOpen(
                                                                  !collapsed,
                                                              ),
                                                          content:
                                                              referenceOpen ? (
                                                                  <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col bg-card">
                                                                      <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 py-2.5 pr-1">
                                                                          <div className="min-w-0">
                                                                              <h2 className="truncate text-body-compact font-semibold text-foreground">
                                                                                  询价需求对照
                                                                              </h2>
                                                                              <p className="num truncate text-xs text-muted-foreground select-all">
                                                                                  {
                                                                                      inquiry.inquiry_no
                                                                                  }{" "}
                                                                                  ·
                                                                                  第{" "}
                                                                                  {
                                                                                      inquiry.requirement_version
                                                                                  }{" "}
                                                                                  版
                                                                              </p>
                                                                          </div>
                                                                          <Button
                                                                              id={`${referencePrefix}-rail-close`}
                                                                              type="button"
                                                                              variant="ghost"
                                                                              size="icon-sm"
                                                                              aria-label="收起询价需求对照栏"
                                                                              onClick={() => {
                                                                                  setReferenceOpen(
                                                                                      false,
                                                                                  )
                                                                                  referenceTrigger.current?.focus()
                                                                              }}
                                                                          >
                                                                              <XIcon aria-hidden="true" />
                                                                          </Button>
                                                                      </div>
                                                                      <div className="min-h-0 min-w-0 flex-1 overflow-y-auto overscroll-contain">
                                                                          <InquiryDemandReference
                                                                              {...referenceProps}
                                                                          />
                                                                      </div>
                                                                  </div>
                                                              ) : null,
                                                      }
                                                    : undefined
                                            }
                                            header={
                                                <InquiryTaskHeader
                                                    title="询价采购处理"
                                                    status={{
                                                        label: INQUIRY_STATUS_LABEL[
                                                            inquiry.status
                                                        ],
                                                        tone: INQUIRY_STATUS_TONE[
                                                            inquiry.status
                                                        ],
                                                    }}
                                                    meta={inquiryMeta}
                                                    actions={
                                                        <>
                                                            {canProcess &&
                                                                feedbackDirty && (
                                                                    <DetailWorkspaceDirtyBadge />
                                                                )}
                                                            <Button
                                                                id={`${referencePrefix}-open`}
                                                                ref={
                                                                    referenceTrigger
                                                                }
                                                                type="button"
                                                                size="sm"
                                                                variant={
                                                                    referenceOpen &&
                                                                    wide
                                                                        ? "secondary"
                                                                        : "ghost"
                                                                }
                                                                aria-pressed={
                                                                    wide
                                                                        ? referenceOpen
                                                                        : undefined
                                                                }
                                                                aria-haspopup={
                                                                    wide
                                                                        ? undefined
                                                                        : "dialog"
                                                                }
                                                                onClick={() =>
                                                                    setReferenceOpen(
                                                                        (
                                                                            open,
                                                                        ) =>
                                                                            !open,
                                                                    )
                                                                }
                                                            >
                                                                {referenceOpen &&
                                                                wide ? (
                                                                    <PanelRightCloseIcon
                                                                        aria-hidden="true"
                                                                        data-icon="inline-start"
                                                                    />
                                                                ) : (
                                                                    <FileTextIcon
                                                                        aria-hidden="true"
                                                                        data-icon="inline-start"
                                                                    />
                                                                )}
                                                                {referenceOpen &&
                                                                wide
                                                                    ? "收起询价需求"
                                                                    : "对照询价需求"}
                                                            </Button>
                                                            {headerSecondary}
                                                            <div className="[&_[data-testid=workspace-workbench-rail-toggle]]:hidden">
                                                                {
                                                                    taskHeaderActions
                                                                }
                                                            </div>
                                                        </>
                                                    }
                                                />
                                            }
                                            headerSupplement={
                                                <div className="border-b border-border/60">
                                                    {navigation}
                                                </div>
                                            }
                                        >
                                            <div
                                                ref={mountBodyHost}
                                                className="flex min-h-0 min-w-0 flex-1 overflow-hidden"
                                            />
                                            {taskFooterActions ||
                                            taskFooterSummary ? (
                                                <WorkspaceTaskFooter
                                                    summary={taskFooterSummary}
                                                >
                                                    {taskFooterActions}
                                                </WorkspaceTaskFooter>
                                            ) : null}
                                            {!wide && (
                                                <Dialog
                                                    open={referenceOpen}
                                                    onOpenChange={
                                                        setReferenceOpen
                                                    }
                                                >
                                                    <DialogContent
                                                        id={`${referencePrefix}-dialog`}
                                                        size="xl"
                                                        scroll="body"
                                                        closeButtonId={`${referencePrefix}-close`}
                                                        finalFocus={
                                                            referenceTrigger
                                                        }
                                                    >
                                                        <DialogHeader>
                                                            <DialogTitle>
                                                                对照询价需求
                                                            </DialogTitle>
                                                        </DialogHeader>
                                                        <DialogBody>
                                                            <InquiryDemandReference
                                                                {...referenceProps}
                                                            />
                                                        </DialogBody>
                                                    </DialogContent>
                                                </Dialog>
                                            )}
                                        </TaskPane>
                                    </>
                                ) : (
                                    <>
                                        <DetailWorkspaceChrome
                                            title={inquiry.customer_name}
                                            documentNumber={inquiry.inquiry_no}
                                            numberLabel="询价单号"
                                            primaryStatus={{
                                                label: INQUIRY_STATUS_LABEL[
                                                    inquiry.status
                                                ],
                                                tone: INQUIRY_STATUS_TONE[
                                                    inquiry.status
                                                ],
                                            }}
                                            titleExtra={
                                                canProcess && feedbackDirty ? (
                                                    <DetailWorkspaceDirtyBadge />
                                                ) : undefined
                                            }
                                            meta={headerMeta}
                                            back={back}
                                            secondaryActions={headerSecondary}
                                            primaryAction={primaryAction}
                                            tabs={navigation}
                                        />
                                        <DetailWorkspaceBody
                                            summary={summaryBar}
                                            alerts={alerts}
                                            saveDockOpen={saveDockOpen}
                                        >
                                            {overview}
                                            {panels}
                                        </DetailWorkspaceBody>
                                    </>
                                )
                            }
                        />
                        {!embedded && canProcess && (
                            <DetailWorkspaceSaveDock
                                open={saveDockOpen}
                                changeCount={0}
                                description={
                                    feedbackStatus.goodsTotal !== null
                                        ? `已完成 ${feedbackStatus.completeCount} / ${feedbackStatus.totalCount} 项 · 含税合计 ¥${feedbackStatus.goodsTotal}`
                                        : `已填写完整 ${feedbackStatus.completeCount} / ${feedbackStatus.totalCount} 项`
                                }
                                onSave={() => feedbackEditor.current?.save()}
                                saving={feedbackStatus.saving}
                                actions={
                                    <Button
                                        id="procurement-inquiry-save-feedback"
                                        type="button"
                                        variant="default"
                                        size="sm"
                                        className="px-3.5"
                                        disabled={feedbackStatus.disabled}
                                        onClick={() =>
                                            feedbackEditor.current?.save()
                                        }
                                    >
                                        {feedbackStatus.saving
                                            ? "正在保存…"
                                            : "保存处理进度"}
                                    </Button>
                                }
                            />
                        )}
                    </>
                )
            }}
        />
    )

    return embedded ? (
        <div
            ref={embeddedRoot}
            className="flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden bg-card"
        >
            {content}
        </div>
    ) : (
        <DetailWorkspacePage>{content}</DetailWorkspacePage>
    )
}
