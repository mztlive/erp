"use client"

import * as React from "react"
import {
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    DetailWorkspaceMetricItem,
    DetailWorkspaceMetrics,
    DetailWorkspacePage,
    DetailWorkspaceSaveDock,
    MoneyValue,
    QuickPreviewSheet,
} from "@/components/business"
import {
    WorkspaceTaskFooter,
    WorkspaceTaskPane,
    type WorkspaceTaskPaneProps,
} from "@/components/business/workspace-task-pane"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { summarizeFeedback } from "../lib/feedback-summary"
import { inquiryDeadlineMeta } from "../lib/format"
import { resolveInquiryBackLink } from "../lib/navigation"
import {
    INQUIRY_STATUS_LABEL,
    INQUIRY_STATUS_TONE,
    type ProcurementInquiry,
} from "../types"
import { InquiryActions } from "./inquiry-actions"
import { InquiryDemandOverview } from "./inquiry-demand-header"
import { InquiryDetailTabs, InquiryHistoryButton } from "./inquiry-detail-tabs"
import {
    InquiryFeedbackEditor,
    type InquiryFeedbackEditorHandle,
    type InquiryFeedbackEditorStatus,
} from "./inquiry-feedback-editor"
import { InquiryFeedbackResults } from "./inquiry-feedback-results"
import { InquiryFees } from "./inquiry-fees"
import { InquiryInternalQuotes } from "./inquiry-internal-quotes"
import { InquiryReinquiryNotice } from "./inquiry-reinquiry-notice"
import { InquiryRequirementTable } from "./inquiry-requirement-table"
import { SupplierQuoteExportButton } from "./supplier-quote-export-dialog"
import { SupplierQuoteImportButton } from "./supplier-quote-import-dialog"

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
    const feedbackEditor = React.useRef<InquiryFeedbackEditorHandle>(null)
    const [quoteLineId, setQuoteLineId] = React.useState<string | null>(null)
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

    const task =
        inquiry.work_items.find(
            (item) => item.id === taskId && item.status === "OPEN",
        ) ?? inquiry.work_items.find((item) => item.status === "OPEN")
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

    const quoteWorkbookActions =
        canProcess && task ? (
            <>
                {hasPermission(permissions, "supplier_offering:list") &&
                    hasPermission(permissions, "sellable_sku:list") && (
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
                {hasPermission(
                    permissions,
                    "procurement_inquiry:internal_quote",
                ) &&
                    hasPermission(permissions, "supplier_offering:list") &&
                    hasPermission(permissions, "sellable_sku:list") && (
                        <SupplierQuoteImportButton
                            inquiry={inquiry}
                            task={task}
                            userId={userId}
                            onSaved={onSaved}
                        />
                    )}
            </>
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
                        <InquiryHistoryButton inquiry={inquiry} />
                        {assignmentActions}
                        {secondaryActions}
                        {embedded ? taskHeaderActions : null}
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
                                    publishing={publishing}
                                    onDirtyChange={setFeedbackDirty}
                                    onStatusChange={setFeedbackStatus}
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
                                <QuickPreviewSheet
                                    idPrefix="procurement-feedback-internal"
                                    size="preview"
                                    open={quoteLineId !== null}
                                    onOpenChange={(open) => {
                                        if (!open) setQuoteLineId(null)
                                    }}
                                    title={
                                        inquiry.draft.lines.find(
                                            (line) =>
                                                line.line_id === quoteLineId,
                                        )?.product_name ?? "供应商报价"
                                    }
                                    description="仅采购可见。采用报价带入含运费供给单价，不代表已选定采购供应商。"
                                >
                                    {quoteLineId && (
                                        <InquiryInternalQuotes
                                            key={quoteLineId}
                                            inquiry={inquiry}
                                            task={task}
                                            userId={userId}
                                            permissions={permissions}
                                            onSaved={onSaved}
                                            lineId={quoteLineId}
                                            onAdopt={(quote) =>
                                                feedbackEditor.current?.adoptQuote(
                                                    quote,
                                                ) ?? false
                                            }
                                        />
                                    )}
                                </QuickPreviewSheet>
                            </>
                        )}
                        {!canProcess &&
                            (showReadonlyDemand ? (
                                <InquiryRequirementTable
                                    idPrefix="sales-inquiry-requirement"
                                    requirement={inquiry.draft}
                                    procurement
                                />
                            ) : (
                                <InquiryFeedbackResults
                                    content="products"
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
                            inquiry={inquiry}
                            selectedRound={selectedRound}
                            onRoundChange={setChosenRound}
                            idPrefix="procurement-inquiry-published"
                        />
                    ) : undefined

                const taskFooterSummary = canProcess ? (
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                        <span className="inline-flex items-center gap-2">
                            <ProgressBar
                                complete={feedbackStatus.completeCount}
                                total={feedbackStatus.totalCount}
                                className="w-16"
                            />
                            <span className="num font-medium text-foreground">
                                {feedbackStatus.completeCount} /{" "}
                                {feedbackStatus.totalCount}
                            </span>
                            <span>项完整</span>
                        </span>
                        {feedbackStatus.goodsTotal !== null && (
                            <span className="border-l border-border/75 pl-3">
                                含税合计{" "}
                                <MoneyValue
                                    value={feedbackStatus.goodsTotal}
                                    display="exact"
                                    showHint={false}
                                    className="font-semibold text-foreground"
                                />
                            </span>
                        )}
                        {feedbackStatus.feeCount > 0 &&
                            feedbackStatus.feesTotal !== null && (
                                <span className="border-l border-border/75 pl-3">
                                    额外成本{" "}
                                    <MoneyValue
                                        value={feedbackStatus.feesTotal}
                                        display="exact"
                                        showHint={false}
                                        className="font-medium text-foreground"
                                    />
                                </span>
                            )}
                    </div>
                ) : publishedSummary ? (
                    <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                        <span>
                            共{" "}
                            <span className="num font-medium text-foreground">
                                {publishedSummary.totalCount}
                            </span>{" "}
                            项商品
                        </span>
                        {publishedSummary.goodsTotal !== null && (
                            <span className="border-l border-border/75 pl-3">
                                含税合计{" "}
                                <MoneyValue
                                    value={publishedSummary.goodsTotal}
                                    display="exact"
                                    showHint={false}
                                    className="font-semibold text-foreground"
                                />
                            </span>
                        )}
                    </div>
                ) : undefined

                const taskFooterActions = canProcess ? (
                    <>
                        <Button
                            id="procurement-inquiry-save-feedback"
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={feedbackStatus.disabled}
                            onClick={() => feedbackEditor.current?.save()}
                        >
                            {feedbackStatus.saving
                                ? "正在保存…"
                                : "保存处理进度"}
                        </Button>
                        {primaryAction}
                    </>
                ) : (
                    primaryAction
                )

                const headerTitleExtra = (
                    <>
                        {embedded &&
                        isActiveInquiry &&
                        deadline.tone === "destructive" &&
                        deadline.hint ? (
                            <StatusBadge
                                label={deadline.hint}
                                tone="destructive"
                            />
                        ) : null}
                        {canProcess && feedbackDirty ? (
                            <DetailWorkspaceDirtyBadge />
                        ) : null}
                    </>
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
                                    <TaskPane
                                        aria-label="采购询价处理任务"
                                        header={
                                            <DetailWorkspaceChrome
                                                className="shrink-0"
                                                title={inquiry.customer_name}
                                                documentNumber={
                                                    inquiry.inquiry_no
                                                }
                                                numberLabel="询价单号"
                                                primaryStatus={{
                                                    label: INQUIRY_STATUS_LABEL[
                                                        inquiry.status
                                                    ],
                                                    tone: INQUIRY_STATUS_TONE[
                                                        inquiry.status
                                                    ],
                                                }}
                                                titleExtra={headerTitleExtra}
                                                meta={headerMeta}
                                                secondaryActions={
                                                    headerSecondary
                                                }
                                                tabs={navigation}
                                            />
                                        }
                                    >
                                        <div className="flex min-h-0 flex-1 flex-col overflow-auto overscroll-contain">
                                            <div className="space-y-2.5 pt-3 empty:hidden">
                                                {alerts}
                                            </div>
                                            {overview}
                                            <div className="min-w-0 flex-1">
                                                {panels}
                                            </div>
                                        </div>
                                        {taskFooterActions ? (
                                            <WorkspaceTaskFooter
                                                summary={taskFooterSummary}
                                            >
                                                {taskFooterActions}
                                            </WorkspaceTaskFooter>
                                        ) : null}
                                    </TaskPane>
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
        <div className="flex h-full min-h-0 min-w-0 flex-1 flex-col overflow-hidden">
            {content}
        </div>
    ) : (
        <DetailWorkspacePage>{content}</DetailWorkspacePage>
    )
}
