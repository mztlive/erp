"use client"

import * as React from "react"
import {
    DetailPageHeader,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSaveDock,
    QuickPreviewSheet,
} from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { hasPermission } from "@/lib/permissions"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { inquiryDate } from "../lib/format"
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

export function InquiryProcurementView({
    inquiry,
    taskId,
    embedded,
    userId,
    permissions,
    returnTo,
    onSaved,
}: {
    inquiry: ProcurementInquiry
    taskId?: string
    embedded: boolean
    userId: string
    permissions: readonly string[]
    returnTo?: string | null
    onSaved: () => void
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

    const back = resolveInquiryBackLink({
        id: "sales-inquiry-detail-back",
        procurement: true,
        returnTo,
    })

    const feedbackActions = (
        <>
            {canProcess &&
                task &&
                hasPermission(permissions, "supplier_offering:list") &&
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
            {canProcess &&
                task &&
                hasPermission(
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
    )

    const saveDockOpen = !embedded && canProcess && feedbackDirty

    const content = (
        <InquiryActions
            workspace={!embedded}
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
                const headerMeta = (
                    <DetailWorkspaceMeta
                        items={[
                            <span key="req" className="num">
                                需求第 {inquiry.requirement_version} 版
                            </span>,
                            <span key="round" className="num">
                                反馈第 {inquiry.feedback_round} 轮
                            </span>,
                            <span key="deadline">
                                询价截止：
                                <span className="num">
                                    {inquiryDate(inquiry.deadline_at)}
                                </span>
                            </span>,
                        ]}
                    />
                )
                const headerSecondary = (
                    <>
                        <InquiryHistoryButton
                            inquiry={inquiry}
                            workspace={!embedded}
                        />
                        {assignmentActions}
                        {secondaryActions}
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
                    <div className="space-y-6">
                        <InquiryReinquiryNotice inquiry={inquiry} />
                        {canProcess && task && (
                            <>
                                <InquiryFeedbackEditor
                                    ref={feedbackEditor}
                                    publishing={publishing}
                                    embedded={embedded}
                                    onDirtyChange={setFeedbackDirty}
                                    onStatusChange={setFeedbackStatus}
                                    inquiry={inquiry}
                                    task={task}
                                    userId={userId}
                                    onSaved={onSaved}
                                    footerActions={
                                        embedded ? primaryAction : undefined
                                    }
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

                return (
                    <>
                        <InquiryDetailTabs
                            workspace={!embedded}
                            inquiry={inquiry}
                            procurement
                            feeCount={selectedFeedback?.data.fees.length ?? 0}
                            feedbackActions={feedbackActions}
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
                                    <>
                                        <div className="shrink-0 space-y-4">
                                            <DetailPageHeader
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
                                                titleExtra={
                                                    canProcess &&
                                                    feedbackDirty ? (
                                                        <DetailWorkspaceDirtyBadge />
                                                    ) : undefined
                                                }
                                                meta={headerMeta}
                                                secondaryActions={
                                                    headerSecondary
                                                }
                                                primaryAction={primaryAction}
                                            />
                                            {alerts}
                                            {overview}
                                        </div>
                                        {navigation}
                                        {panels}
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
                                description={`已填写完整 ${feedbackStatus.completeCount} / ${feedbackStatus.totalCount} 项（Cmd/Ctrl + S 保存进度）`}
                                onSave={() => feedbackEditor.current?.save()}
                                saving={feedbackStatus.saving}
                                actions={
                                    <Button
                                        id="procurement-inquiry-save-feedback"
                                        type="button"
                                        variant="default"
                                        size="sm"
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
        <div className="flex h-full min-h-0 min-w-0 flex-col overflow-hidden p-6">
            {content}
        </div>
    ) : (
        <DetailWorkspacePage>{content}</DetailWorkspacePage>
    )
}
