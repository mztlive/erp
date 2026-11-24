"use client"

import {
    useCallback,
    useEffect,
    useLayoutEffect,
    useMemo,
    useRef,
    useState,
} from "react"
import { createPortal } from "react-dom"
import { PanelRightCloseIcon, PaperclipIcon, XIcon } from "lucide-react"
import {
    DetailWorkspaceMeta,
    detailWorkspaceStyles,
} from "@/components/business"
import {
    UnifiedMaterialViewer,
    type MaterialFileItem,
} from "@/components/business/unified-material-viewer"
import { useWorkspaceTaskPage } from "@/components/business/workspace-task-pane"
import { Button } from "@/components/ui/button"
import { ApprovalRouteStepper } from "@/features/approval-workflow/components/approval-route-stepper"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { TaskTimeline } from "@/features/document-timeline/components/task-timeline"
import {
    timelineDocumentTypes,
    type TimelineTarget,
} from "@/features/document-timeline/types"
import { fetchMaterialPreview } from "@/features/purchase-payment-requests/api"
import { PurchasePaymentRequestTask } from "@/features/purchase-payment-requests/components/purchase-payment-request-task"
import {
    usePaymentMaterialsQuery,
    usePaymentRequestQuery,
    usePaymentSubmissionQuery,
    useRequestDownloadMutation,
} from "@/features/purchase-payment-requests/hooks/queries"
import type { PaymentRequest } from "@/features/purchase-payment-requests/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { compareDecimal, formatCurrencyFixed } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"
import type { WorkspaceWorkItem } from "../types"
import {
    WorkspaceApprovalContextRail,
    type ApprovalContextRailTab,
} from "./workspace-approval-context-rail"
import { WorkspaceApprovalReworkActions } from "./workspace-approval-rework"
import {
    WorkspaceApprovalResubmissionBanner,
    type ResubmissionFieldDiff,
} from "./workspace-approval-resubmission-banner"
import { WorkspaceDocumentBadge } from "./workspace-document-badge"
import { WorkspaceProcessingPane } from "./workspace-processing-layout"
import { parsePurchaseExtraSections } from "./workspace-purchase-comparison-model"
import {
    WorkspaceRejectionNotice,
    useWorkspaceRejectionInfo,
} from "./workspace-rejection-notice"
import { WorkspaceTaskHeaderActions } from "./workspace-task-context"
import {
    WorkspaceCompactTaskHeader,
    WorkspaceTaskIdentityHeader,
} from "./workspace-task-identity-header"
import { WorkspaceTaskResponsibilityActions } from "./workspace-task-responsibility-actions"

const approvalRailCollapsedKey = "erp.workspace.approvalRailCollapsed"
const executionRailWidthKey = "erp.workspace.paymentExecutionRailWidth"

const useIsomorphicLayoutEffect =
    typeof window !== "undefined" ? useLayoutEffect : useEffect

type PaymentRequestSurfaceProps = {
    item: WorkspaceWorkItem
    reworkTask?: WorkspaceWorkItem
    canReadSensitive: boolean
    inspectorMode: boolean
    onDecisionApplied?: (
        view: ApprovalCommandView,
        completedWorkItemId: string,
    ) => void
    onTaskCompleted?: (workItemId: string, preferredWorkItemId?: string) => void
}

export function WorkspacePurchasePaymentRequestSurface(
    props: PaymentRequestSurfaceProps,
) {
    return <PaymentRequestSurface key={props.item.workItemId} {...props} />
}

function PaymentRequestSurface({
    item,
    reworkTask,
    canReadSensitive,
    inspectorMode,
    onDecisionApplied,
    onTaskCompleted,
}: PaymentRequestSurfaceProps) {
    const fullPage = useWorkspaceTaskPage()
    const isPaymentExecution =
        item.workItemType === "SUPPLIER_PAYMENT_EXECUTION"
    const isApprovalInstance = item.workItemType === "APPROVAL_INSTANCE"
    const isRework = item.workItemType === "APPROVAL_REWORK"
    const instanceId =
        item.approvalProcessInstanceId ?? item.approval?.instanceId
    const canUseDecisionRail = Boolean(
        fullPage &&
        !inspectorMode &&
        !isPaymentExecution &&
        instanceId &&
        canReadSensitive,
    )
    const hasExecutionReference = Boolean(
        isPaymentExecution && canReadSensitive,
    )
    const id = `workspace-ppr-${toAutomationIdSegment(item.workItemId)}`
    const railId = `${id}-rail`
    const rootRef = useRef<HTMLDivElement>(null)
    const [wide, setWide] = useState(false)
    const showDecisionRail = canUseDecisionRail
    const [railTab, setRailTab] = useState<ApprovalContextRailTab>("trail")
    const [railCollapsed, setRailCollapsedState] = useState(false)
    const [referenceOpen, setReferenceOpen] = useState(false)
    const [activeFileId, setActiveFileId] = useState<string>()
    const [abandonOpen, setAbandonOpen] = useState(false)
    const rejectionInfo = useWorkspaceRejectionInfo(item)
    const bodyHostRef = useRef<HTMLDivElement | null>(null)
    const [bodyHost, setBodyHost] = useState<HTMLDivElement | null>(null)

    // 工作台在拖拽分栏与单栏间切换时重建正文槽，移动同一 DOM 容器以保留表单和提交身份。
    const attachBody = useCallback((anchor: HTMLDivElement | null) => {
        if (!anchor) return
        if (!bodyHostRef.current) {
            const host = document.createElement("div")
            host.className = "min-w-0"
            bodyHostRef.current = host
            setBodyHost(host)
        }
        if (bodyHostRef.current.parentNode !== anchor) {
            anchor.append(bodyHostRef.current)
        }
    }, [])

    useEffect(() => {
        if (!canUseDecisionRail) return
        try {
            setRailCollapsedState(
                window.localStorage.getItem(approvalRailCollapsedKey) === "1",
            )
        } catch {
            // 本地存储不可用时保持默认展开。
        }
    }, [canUseDecisionRail])

    const setRailCollapsed = useCallback((collapsed: boolean) => {
        setRailCollapsedState(collapsed)
        try {
            window.localStorage.setItem(
                approvalRailCollapsedKey,
                collapsed ? "1" : "0",
            )
        } catch {
            // 本地存储不可用时仍更新当前界面。
        }
    }, [])

    useIsomorphicLayoutEffect(() => {
        if (!hasExecutionReference || !fullPage || inspectorMode) return
        const root = rootRef.current
        const target =
            root?.closest<HTMLElement>("#main-content") ?? root?.parentElement
        if (!target) return
        const sync = () => {
            const width = target.clientWidth
            if (width > 0) setWide(width >= 960)
        }
        sync()
        if (typeof ResizeObserver === "undefined") return
        const observer = new ResizeObserver(sync)
        observer.observe(target)
        return () => observer.disconnect()
    }, [fullPage, hasExecutionReference, inspectorMode])

    const showExecutionRail = Boolean(
        hasExecutionReference && fullPage && !inspectorMode && wide,
    )
    useEffect(() => {
        if (!showDecisionRail && !hasExecutionReference) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (hasExecutionReference) {
                if (
                    action === "materials" ||
                    (action === "toggle-rail" && showExecutionRail)
                ) {
                    setReferenceOpen((open) => !open)
                }
                return
            }
            if (action === "toggle-rail") {
                setRailCollapsed(!railCollapsed)
                return
            }
            if (action === "materials" || action === "history") {
                const tab = action === "materials" ? "materials" : "trail"
                if (!railCollapsed && railTab === tab) {
                    setRailCollapsed(true)
                    return
                }
                setRailTab(tab)
                setRailCollapsed(false)
                window.requestAnimationFrame(() => {
                    document.getElementById(`${railId}-tab-${tab}`)?.focus()
                })
            }
        })
    }, [
        hasExecutionReference,
        railCollapsed,
        railId,
        railTab,
        setRailCollapsed,
        showDecisionRail,
        showExecutionRail,
    ])

    const loadFiles = showDecisionRail || hasExecutionReference
    const scopedWorkItemId =
        !isApprovalInstance &&
        !isRework &&
        item.allowedActions.some(
            (action) =>
                action === "PROCESS" ||
                action === "APPROVE" ||
                action === "REJECT",
        )
            ? item.workItemId
            : undefined
    const parsedVersion = Number(item.subjectVersion)
    const submissionVersion =
        isApprovalInstance &&
        Number.isSafeInteger(parsedVersion) &&
        parsedVersion > 0
            ? parsedVersion
            : undefined
    const requestQuery = usePaymentRequestQuery(
        item.businessObjectId,
        scopedWorkItemId,
        (loadFiles ||
            (isRework && fullPage && !inspectorMode && canReadSensitive)) &&
            !isApprovalInstance,
    )
    const submissionQuery = usePaymentSubmissionQuery(
        item.businessObjectId,
        loadFiles && submissionVersion ? submissionVersion : null,
    )
    const submission = submissionQuery.data
    const resolvedRequest = isApprovalInstance
        ? submission?.submission_version === submissionVersion &&
          submission?.approval_instance_id === instanceId
            ? submission
            : undefined
        : requestQuery.data
    const materialId =
        loadFiles &&
        (!isApprovalInstance || (submissionVersion && resolvedRequest))
            ? item.businessObjectId
            : ""
    const materialsQuery = usePaymentMaterialsQuery(
        materialId,
        scopedWorkItemId,
        submissionVersion,
    )
    const { mutateAsync: download } = useRequestDownloadMutation()
    const requestFiles = resolvedRequest?.attachments
    const salesMaterials = materialsQuery.data?.sales
    const receipts = resolvedRequest?.payments
    const files = useMemo<readonly MaterialFileItem[]>(() => {
        if (!loadFiles) return []
        const rawFiles = new Map(
            [
                ...(requestFiles ?? []),
                ...(salesMaterials?.flatMap((sales) => sales.materials) ?? []),
                ...(receipts?.flatMap((payment) =>
                    payment.receipt ? [payment.receipt] : [],
                ) ?? []),
            ].map((file) => [file.file_asset_id, file]),
        )
        return Array.from(rawFiles.values(), (file) => ({
            id: file.file_asset_id,
            name: file.file_name,
            mimeType: file.content_type,
            loadPreview: () =>
                fetchMaterialPreview(
                    item.businessObjectId,
                    file,
                    scopedWorkItemId,
                    submissionVersion,
                ),
            onDownload: () =>
                download({
                    requestId: item.businessObjectId,
                    attachment: file,
                    workItemId: scopedWorkItemId,
                    submissionVersion,
                }),
        }))
    }, [
        download,
        item.businessObjectId,
        loadFiles,
        receipts,
        requestFiles,
        salesMaterials,
        scopedWorkItemId,
        submissionVersion,
    ])
    const materialsLoading = Boolean(materialId && materialsQuery.isPending)

    const compactStepperHistory = useApprovalHistoryInfiniteQuery(
        { instanceId: instanceId ?? "", order: "desc" },
        !isPaymentExecution && !showDecisionRail && Boolean(instanceId),
    )
    const compactRoute = compactStepperHistory.data?.pages[0]?.route
    const selectFile = (fileId: string) => {
        setActiveFileId(fileId)
        setRailTab("materials")
        setRailCollapsed(false)
    }
    const timelineTarget: TimelineTarget | undefined =
        timelineDocumentTypes.has(item.businessObjectType)
            ? {
                  kind: isApprovalInstance ? "approval" : "task",
                  id: item.workItemId,
              }
            : undefined
    const documentNumber = resolvedRequest?.request_no || item.stableNumber
    const title = canReadSensitive
        ? item.counterpartyName || "付款申请"
        : "业务详情"
    const meta = (
        <DetailWorkspaceMeta
            items={[
                documentNumber && (
                    <span key="number" className="num select-all">
                        {documentNumber}
                    </span>
                ),
                resolvedRequest?.submission_version ? (
                    <span key="version" className="num">
                        提交版本 {resolvedRequest.submission_version}
                    </span>
                ) : null,
                item.dueAt && (
                    <span key="due">
                        截止 <span className="num">{item.dueAtLabel}</span>
                    </span>
                ),
            ]}
        />
    )
    const attachmentsActive = showDecisionRail
        ? !railCollapsed && railTab === "materials"
        : referenceOpen
    const attachmentShortcut = (showDecisionRail || hasExecutionReference) && (
        <Button
            id={`${id}-attachments`}
            type="button"
            variant={
                hasExecutionReference && referenceOpen && showExecutionRail
                    ? "secondary"
                    : "ghost"
            }
            size="xs"
            aria-pressed={attachmentsActive}
            className={cn(
                detailWorkspaceStyles.quietButton,
                showDecisionRail &&
                    !materialsLoading &&
                    files.length === 0 &&
                    "text-warning-soft-foreground hover:text-warning-soft-foreground",
                showDecisionRail &&
                    attachmentsActive &&
                    "bg-muted/80 text-foreground",
            )}
            onClick={() => {
                if (hasExecutionReference) {
                    setReferenceOpen(!referenceOpen)
                    return
                }
                setRailCollapsed(!railCollapsed && railTab === "materials")
                setRailTab("materials")
            }}
        >
            {hasExecutionReference && referenceOpen && showExecutionRail ? (
                <PanelRightCloseIcon aria-hidden="true" />
            ) : (
                <PaperclipIcon aria-hidden="true" />
            )}
            <span className="num">
                {hasExecutionReference && referenceOpen && showExecutionRail
                    ? "收起附件"
                    : files.length > 0 || materialsLoading
                      ? `附件 (${files.length})`
                      : "无附件"}
            </span>
        </Button>
    )
    const approvalRoundNo = item.approval?.currentRoundNo
    const isActionableReworkPage = Boolean(
        isRework &&
        fullPage &&
        !inspectorMode &&
        canReadSensitive &&
        item.status === "OPEN" &&
        item.allowedActions.includes("PROCESS"),
    )
    const canInlineRework = Boolean(
        isActionableReworkPage &&
        resolvedRequest &&
        (resolvedRequest.status === "rejected" ||
            resolvedRequest.status === "draft" ||
            resolvedRequest.status === "withdrawn") &&
        resolvedRequest.allowed_actions.some((action) => {
            const upper = action.toUpperCase()
            return upper === "EDIT" || upper === "SUBMIT"
        }),
    )
    const canAbandonRework = Boolean(
        reworkTask &&
        reworkTask.status === "OPEN" &&
        reworkTask.allowedActions.includes("ABANDON_REWORK"),
    )
    const renderReworkAbandonAction =
        canInlineRework && reworkTask
            ? (variant: "outline" | "default" = "outline") => (
                  <WorkspaceApprovalReworkActions
                      item={reworkTask}
                      abandonOnly
                      abandonVariant={variant}
                      abandonOpen={abandonOpen}
                      onAbandonOpenChange={setAbandonOpen}
                      onTaskCompleted={onTaskCompleted}
                  />
              )
            : undefined
    const body = canReadSensitive ? (
        <PurchasePaymentRequestTask
            item={
                item.approvalProcessInstanceId === instanceId
                    ? item
                    : { ...item, approvalProcessInstanceId: instanceId }
            }
            onSelectFile={showDecisionRail ? selectFile : undefined}
            activeFileId={activeFileId}
            hideEmbeddedMaterials={showDecisionRail}
            hasExternalPrimaryAction={Boolean(
                reworkTask && canReadSensitive && !canInlineRework,
            )}
            approvalRoundNo={approvalRoundNo}
            inlineRework={canInlineRework}
            reworkCanAbandon={canAbandonRework}
            reworkRejection={canInlineRework ? rejectionInfo : undefined}
            onRequestReworkAbandon={
                canInlineRework && canAbandonRework
                    ? () => setAbandonOpen(true)
                    : undefined
            }
            renderReworkAbandonAction={renderReworkAbandonAction}
            renderResubmission={
                isPaymentExecution
                    ? undefined
                    : (request) => (
                          <PaymentResubmissionBanner
                              item={item}
                              request={request}
                          />
                      )
            }
            onDecisionApplied={(view) =>
                onDecisionApplied?.(view, item.workItemId)
            }
            onTaskCompleted={() => onTaskCompleted?.(item.workItemId)}
        />
    ) : (
        <p className="py-5 text-xs text-muted-foreground">
            当前账号无权查看付款申请详情。
        </p>
    )

    return (
        <div
            ref={rootRef}
            className="flex h-full min-h-0 w-full min-w-0 flex-1 flex-col"
        >
            <WorkspaceProcessingPane
                archetype={
                    isPaymentExecution
                        ? "action"
                        : showDecisionRail
                          ? "decision"
                          : "informational"
                }
                idPrefix={id}
                className="@container/document"
                contextRail={
                    showDecisionRail && instanceId
                        ? {
                              defaultTab: "trail",
                              collapsible: true,
                              resizable: true,
                              defaultWidthPercent: 28,
                              minWidthPercent: 24,
                              maxWidthPercent: 52,
                              storageKey: "erp.workspace.approvalRailWidth",
                              preferredMinWidthPercent:
                                  railTab === "materials" ? 42 : undefined,
                              collapsed: railCollapsed,
                              onCollapsedChange: setRailCollapsed,
                              label: "审批进度、轨迹与附件对照",
                              content: (
                                  <WorkspaceApprovalContextRail
                                      instanceId={instanceId}
                                      timelineTarget={timelineTarget}
                                      currentDocument={{
                                          type: item.businessObjectType,
                                          id: item.businessObjectId,
                                      }}
                                      materialFiles={files}
                                      materialsLoading={materialsLoading}
                                      activeTab={railTab}
                                      onActiveTabChange={setRailTab}
                                      activeFileId={activeFileId}
                                      onActiveFileChange={setActiveFileId}
                                      idPrefix={railId}
                                  />
                              ),
                          }
                        : showExecutionRail
                          ? {
                                allowOnFullWidth: true,
                                collapsible: true,
                                resizable: true,
                                defaultWidthPercent: 38,
                                minWidthPercent: 26,
                                maxWidthPercent: 52,
                                storageKey: executionRailWidthKey,
                                collapsed: !referenceOpen,
                                onCollapsedChange: (collapsed) =>
                                    setReferenceOpen(!collapsed),
                                label: "付款凭据对照",
                                content: (
                                    <div className="flex h-full min-h-0 flex-col bg-card">
                                        <div className="flex shrink-0 items-center justify-between gap-2 border-b border-border/60 py-2.5 pr-1">
                                            <div className="min-w-0">
                                                <h2 className="truncate text-body-compact font-semibold text-foreground">
                                                    付款凭据对照
                                                </h2>
                                                {documentNumber ? (
                                                    <p className="num truncate text-xs text-muted-foreground">
                                                        {documentNumber} ·{" "}
                                                        {files.length} 份附件
                                                    </p>
                                                ) : null}
                                            </div>
                                            <Button
                                                id={`${railId}-reference-close`}
                                                type="button"
                                                variant="ghost"
                                                size="icon-sm"
                                                aria-label="收起付款凭据对照栏"
                                                onClick={() =>
                                                    setReferenceOpen(false)
                                                }
                                            >
                                                <XIcon aria-hidden="true" />
                                            </Button>
                                        </div>
                                        <div className="flex min-h-0 flex-1 flex-col overflow-hidden py-3">
                                            {materialsLoading &&
                                            !files.length ? (
                                                <p
                                                    role="status"
                                                    className="py-4 text-xs text-muted-foreground"
                                                >
                                                    正在读取付款附件…
                                                </p>
                                            ) : (
                                                <UnifiedMaterialViewer
                                                    idPrefix={`${railId}-reference`}
                                                    mode="docked"
                                                    files={files}
                                                    activeFileId={activeFileId}
                                                    onActiveFileChange={
                                                        setActiveFileId
                                                    }
                                                    emptyMessage="暂无可对照的付款附件。"
                                                    className="min-h-0 flex-1"
                                                />
                                            )}
                                        </div>
                                    </div>
                                ),
                            }
                          : undefined
                }
                header={
                    fullPage && !inspectorMode ? (
                        <WorkspaceCompactTaskHeader
                            title={title}
                            primaryStatus={{
                                label:
                                    isRework && item.status === "OPEN"
                                        ? item.workItemTypeLabel
                                        : isApprovalInstance &&
                                            !item.statusLabel.startsWith("审批")
                                          ? `审批${item.statusLabel}`
                                          : item.statusLabel,
                                tone: item.statusTone,
                            }}
                            documentNumber={documentNumber}
                            badges={<WorkspaceDocumentBadge item={item} />}
                            meta={meta}
                            actions={
                                <WorkspaceTaskHeaderActions item={item}>
                                    <WorkspaceTaskResponsibilityActions
                                        item={item}
                                        onCompleted={onTaskCompleted}
                                    />
                                    {attachmentShortcut}
                                    {showExecutionRail && timelineTarget && (
                                        <TaskTimeline
                                            taskId={item.workItemId}
                                            approval={Boolean(item.approval)}
                                            instance={false}
                                            title={title}
                                            document={{
                                                type: item.businessObjectType,
                                                id: item.businessObjectId,
                                                number: documentNumber,
                                            }}
                                        />
                                    )}
                                </WorkspaceTaskHeaderActions>
                            }
                        />
                    ) : (
                        <WorkspaceTaskIdentityHeader
                            item={item}
                            title={title}
                            subtitle={meta}
                        >
                            {attachmentShortcut}
                        </WorkspaceTaskIdentityHeader>
                    )
                }
                headerSupplement={
                    !showDecisionRail &&
                    !isPaymentExecution &&
                    compactRoute?.nodes.length ? (
                        <ApprovalRouteStepper
                            route={compactRoute}
                            instanceStatus={item.approval?.status}
                            latestRejection={
                                canReadSensitive
                                    ? item.approval?.lastRejectReason
                                    : undefined
                            }
                            latestRejectionByName={
                                canReadSensitive
                                    ? item.approval?.lastRejectorLabel
                                    : undefined
                            }
                            hideRejectionBanner={
                                isRework || item.approval?.status === "REJECTED"
                            }
                            idPrefix={`${id}-compact-stepper`}
                        />
                    ) : null
                }
                alertBanner={
                    canReadSensitive &&
                    !canInlineRework &&
                    (isRework || item.approval?.status === "REJECTED") ? (
                        <WorkspaceRejectionNotice
                            item={item}
                            actionable={Boolean(
                                reworkTask?.allowedActions.includes("PROCESS"),
                            )}
                        />
                    ) : null
                }
                footer={
                    <>
                        {reworkTask &&
                            canReadSensitive &&
                            !canInlineRework &&
                            !(
                                isActionableReworkPage && requestQuery.isPending
                            ) && (
                                <WorkspaceApprovalReworkActions
                                    item={reworkTask}
                                    onTaskCompleted={onTaskCompleted}
                                />
                            )}
                        {bodyHost && createPortal(body, bodyHost)}
                    </>
                }
                aria-label="采购付款申请任务"
            >
                <div ref={attachBody} className="min-w-0" />
            </WorkspaceProcessingPane>
            {hasExecutionReference && !showExecutionRail && (
                <UnifiedMaterialViewer
                    idPrefix={`${id}-reference-dialog`}
                    mode="dialog"
                    files={files}
                    open={referenceOpen}
                    onOpenChange={setReferenceOpen}
                    activeFileId={activeFileId}
                    onActiveFileChange={setActiveFileId}
                    emptyMessage="暂无可对照的付款附件。"
                />
            )}
        </div>
    )
}

function PaymentResubmissionBanner({
    item,
    request,
}: {
    item: WorkspaceWorkItem
    request: PaymentRequest
}) {
    const roundNo = Math.max(
        item.approval?.currentRoundNo ??
            request.approval?.instance?.current_round_no ??
            0,
        request.submission_version,
    )
    const instanceId =
        item.approvalProcessInstanceId ?? item.approval?.instanceId
    const snapshot = useApprovalMaterials(instanceId ?? "", roundNo >= 2, item)
    const extraSections = new Map(
        (item.summarySections ?? []).map(
            ({ label, value, numeric, objectId }) => [
                label,
                {
                    label,
                    value,
                    numeric: numeric === true,
                    object_id: objectId ?? null,
                },
            ],
        ),
    )
    for (const field of snapshot.data?.display.source.extra_sections ?? []) {
        extraSections.set(field.label, field)
    }
    const parsed = parsePurchaseExtraSections(
        Array.from(extraSections.values()),
        item,
    ).resubmission
    const baseline =
        request.submission_version > 1 && !parsed?.diffs.length
            ? request.submission_version - 1
            : null
    const previousQuery = usePaymentSubmissionQuery(request.id, baseline)
    const previous = previousQuery.data
    const previousInstance = previous?.approval_instance_id ?? ""
    const previousHistory = useApprovalHistoryInfiniteQuery(
        { instanceId: previousInstance, order: "desc" },
        roundNo >= 2 && Boolean(previousInstance) && !parsed?.previousRejection,
    )
    const rejection = previousHistory.data?.pages
        .flatMap((page) => page.items)
        .find((entry) => entry.result === "REJECTED" && entry.decisionReason)
    const previousRejection =
        parsed?.previousRejection ??
        (rejection?.decisionReason
            ? {
                  reason: rejection.decisionReason,
                  actor: rejection.assigneeName || rejection.decidedBy,
              }
            : undefined)
    const diffs: ResubmissionFieldDiff[] = [...(parsed?.diffs ?? [])]
    if (previous && previous.submission_version === baseline) {
        if (
            compareDecimal(previous.total_amount, request.total_amount, 4) !== 0
        ) {
            const options = { maxScale: 4, minimumFractionDigits: 2 }
            diffs.push({
                label: "申请金额",
                before: formatCurrencyFixed(previous.total_amount, options),
                after: formatCurrencyFixed(request.total_amount, options),
                numeric: true,
            })
        }
        for (const [label, before, after] of [
            [
                "希望付款日",
                previous.desired_payment_date,
                request.desired_payment_date,
            ],
            ["付款主体", previous.our_company_name, request.our_company_name],
            ["申请说明", previous.description, request.description],
            [
                "收款账号",
                previous.payment_recipient?.account_number_masked,
                request.payment_recipient?.account_number_masked,
            ],
        ] as const) {
            if (before !== after)
                diffs.push({
                    label,
                    before: before || "—",
                    after: after || "—",
                })
        }
    }

    if (roundNo < 2) return null
    return (
        <WorkspaceApprovalResubmissionBanner
            roundNo={roundNo}
            resubmissionReason={parsed?.resubmissionReason}
            diffBaseline={
                parsed?.diffBaseline ??
                (previous ? `第 ${baseline} 次提交` : undefined)
            }
            diffSummary={
                parsed?.diffSummary ??
                (diffs.length || parsed?.resubmissionReason
                    ? undefined
                    : previous
                      ? "金额、付款日期及收款账号等摘要字段未变更"
                      : "原提交摘要待核对")
            }
            diffs={diffs}
            previousRejection={previousRejection}
        />
    )
}
