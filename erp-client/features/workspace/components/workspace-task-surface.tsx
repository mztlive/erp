"use client"

import { useEffect, useMemo, useState } from "react"
import type { MaterialFileItem } from "@/components/business/unified-material-viewer"
import {
    WorkspaceInspectorModeProvider,
    useWorkspaceTaskPage,
} from "@/components/business/workspace-task-pane"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { StatusBadge } from "@/components/ui/status-badge"
import { ApprovalRouteStepper } from "@/features/approval-workflow/components/approval-route-stepper"
import { useApprovalHistoryInfiniteQuery } from "@/features/approval-workflow/queries"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { timelineDocumentTypes } from "@/features/document-timeline/types"
import { PurchasePaymentArrangement } from "@/features/purchase-orders/components/purchase-payment-arrangement"
import { fetchMaterialPreview } from "@/features/purchase-payment-requests/api"
import { PurchasePaymentRequestTask } from "@/features/purchase-payment-requests/components/purchase-payment-request-task"
import {
    usePaymentMaterialsQuery,
    usePaymentRequestQuery,
    usePaymentSubmissionQuery,
    useRequestDownloadMutation,
} from "@/features/purchase-payment-requests/hooks/queries"
import { SalesInquiryDetailPage } from "@/features/sales-inquiries/pages/sales-inquiry-detail-page"
import { PortalAdminReviewPage } from "@/features/supplier-portal-admin/pages/review-page"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { subscribeWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceAcceptanceTask } from "./workspace-acceptance-task"
import {
    WorkspaceApprovalContextRail,
    type ApprovalContextRailTab,
} from "./workspace-approval-context-rail"
import { WorkspaceApprovalReworkActions } from "./workspace-approval-rework"
import { WorkspaceDocumentTaskDetail } from "./workspace-document-task-detail"
import { WorkspaceFulfillmentTask } from "./workspace-fulfillment-task"
import { WorkspaceImportTask } from "./workspace-import-task"
import { WorkspaceIntegrationTask } from "./workspace-integration-task"
import { WorkspaceInvoiceTask } from "./workspace-invoice-task"
import { WorkspaceProcessingPane } from "./workspace-processing-layout"
import { WorkspaceProcurementTask } from "./workspace-procurement-task"
import { WorkspacePurchaseSourceUpdateTask } from "./workspace-purchase-source-update-task"
import { WorkspaceRejectionNotice } from "./workspace-rejection-notice"
import { WorkspaceReturnFinance } from "./workspace-return-finance"
import { WorkspaceSettlementTask } from "./workspace-settlement-task"
import { WorkspaceSupplierInvestigationTask } from "./workspace-supplier-investigation-task"
import { WorkspaceSupplyExceptionTask } from "./workspace-supply-exception-task"
import { WorkspaceTaskHeaderActions } from "./workspace-task-context"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"
import { WorkspaceTaskCompletionProvider } from "./workspace-task-responsibility-actions"

export type WorkspaceTaskDetailProps = Readonly<{
    item: WorkspaceWorkItem
    reworkTask?: WorkspaceWorkItem
    canReadSensitive?: boolean
    onDecisionApplied?: (
        view: ApprovalCommandView,
        completedWorkItemId: string,
    ) => void
    grantedPermissions?: readonly string[]
    onTaskCompleted?: (workItemId: string, preferredWorkItemId?: string) => void
    /** 供列表分栏检查器（WorkspaceSplitQueue）启用精简单栏渲染。 */
    inspectorMode?: boolean
}>

export function WorkspaceTaskSurface(props: WorkspaceTaskDetailProps) {
    return (
        <WorkspaceInspectorModeProvider
            inspectorMode={Boolean(props.inspectorMode)}
        >
            <WorkspaceTaskCompletionProvider
                onTaskCompleted={props.onTaskCompleted}
            >
                <WorkspaceTaskSurfaceContent {...props} />
            </WorkspaceTaskCompletionProvider>
        </WorkspaceInspectorModeProvider>
    )
}

function WorkspaceTaskSurfaceContent({
    item,
    reworkTask,
    canReadSensitive = true,
    onDecisionApplied,
    grantedPermissions = [],
    onTaskCompleted,
    inspectorMode = false,
}: WorkspaceTaskDetailProps) {
    if (item.workItemType === "APPROVAL_REWORK") {
        return (
            <WorkspaceDocumentTaskDetail
                item={item}
                reworkTask={item}
                canReadSensitive={canReadSensitive}
                grantedPermissions={grantedPermissions}
                onTaskCompleted={onTaskCompleted}
                onDecisionApplied={onDecisionApplied}
                inspectorMode={inspectorMode}
            />
        )
    }
    if (
        item.workItemType === "SALES_INQUIRY_PROCESSING" &&
        item.handlerKey === "sales_inquiry_processing" &&
        item.businessObjectType === "sales_inquiry"
    ) {
        return (
            <SalesInquiryDetailPage
                inquiryId={item.businessObjectId}
                procurement
                workItemId={item.workItemId}
                embedded
                TaskPane={WorkspaceProcessingPane}
                taskHeaderActions={<WorkspaceTaskHeaderActions item={item} />}
                onTaskCompleted={() => onTaskCompleted?.(item.workItemId)}
            />
        )
    }
    if (
        item.businessObjectType === "purchase_payment_request" &&
        ((item.workItemType === "SUPPLIER_PAYMENT_EXECUTION" &&
            item.handlerKey === "supplier_payment_execution") ||
            ((item.workItemType === "DOCUMENT_APPROVAL" ||
                item.workItemType === "APPROVAL_INSTANCE") &&
                item.handlerKey === "document_approval"))
    ) {
        return (
            <PurchasePaymentRequestSurface
                item={item}
                reworkTask={reworkTask}
                canReadSensitive={canReadSensitive}
                inspectorMode={inspectorMode}
                onDecisionApplied={onDecisionApplied}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    if (
        canReadSensitive &&
        item.workItemType === "SALES_INVOICE_EXECUTION" &&
        item.businessObjectType === "receivable_account" &&
        (item.status === "COMPLETED" ||
            item.allowedActions.includes("PROCESS") ||
            item.allowedActions.includes("VIEW"))
    ) {
        return (
            <WorkspaceInvoiceTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 非执行者独立读取关联材料，避免挂载建单、确认或异常处理表单。
    if (
        item.workItemType !== "DOCUMENT_APPROVAL" &&
        item.workItemType !== "APPROVAL_INSTANCE" &&
        !item.allowedActions.includes("PROCESS")
    ) {
        return (
            <WorkspaceDocumentTaskDetail
                item={item}
                canReadSensitive={canReadSensitive}
                onTaskCompleted={onTaskCompleted}
                inspectorMode={inspectorMode}
            />
        )
    }

    // 待供给分配：销售单生成采购 → WorkspaceProcurementTask
    if (
        item.workItemType === "SUPPLIER_PORTAL_REVIEW" &&
        item.handlerKey === "supplier_portal_review" &&
        item.businessObjectType === "supplier_portal_request"
    ) {
        return (
            <WorkspaceProcessingPane
                archetype="action"
                header={<WorkspaceTaskIdentityHeader item={item} />}
                aria-label="供应商申请确认任务"
            >
                <PortalAdminReviewPage
                    applicationId={item.businessObjectId}
                    workItemId={item.workItemId}
                    embedded
                    onTaskCompleted={onTaskCompleted}
                />
            </WorkspaceProcessingPane>
        )
    }

    if (
        item.workItemType === "PROCUREMENT_ORDER_CREATION" &&
        item.businessObjectType === "sales_order"
    ) {
        return (
            <WorkspaceProcurementTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    if (
        item.workItemType === "PURCHASE_PAYMENT_ARRANGEMENT" &&
        item.handlerKey === "purchase_payment_arrangement" &&
        item.businessObjectType === "purchase_order"
    ) {
        return (
            <WorkspaceProcessingPane
                archetype="action"
                header={
                    <WorkspaceTaskIdentityHeader
                        item={item}
                        title={item.counterpartyName || "采购付款安排"}
                        subtitle={item.objectTitle}
                    />
                }
                aria-label="采购付款安排任务"
            >
                <PurchasePaymentArrangement
                    key={item.workItemId}
                    purchaseOrderId={item.businessObjectId}
                    workItemId={item.workItemId}
                    onTaskCompleted={onTaskCompleted}
                />
            </WorkspaceProcessingPane>
        )
    }

    if (
        item.workItemType === "PURCHASE_SOURCE_UPDATE" &&
        item.handlerKey === "purchase_source_update" &&
        item.businessObjectType === "purchase_order"
    ) {
        return (
            <WorkspacePurchaseSourceUpdateTask
                key={item.workItemId}
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    if (
        ((item.workItemType === "PURCHASE_RETURN_REFUND" &&
            item.handlerKey === "purchase_return_refund") ||
            (item.workItemType === "PURCHASE_RETURN_INVOICE" &&
                item.handlerKey === "purchase_return_invoice")) &&
        item.businessObjectType === "purchase_order"
    ) {
        return (
            <WorkspaceReturnFinance
                key={item.workItemId}
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 供应商结算复核 → WorkspaceSettlementTask
    if (
        item.workItemType === "SUPPLIER_SETTLEMENT_REVIEW" &&
        item.businessObjectType === "supplier_settlement_statement"
    ) {
        return (
            <WorkspaceSettlementTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 导入业务确认（历史导入批次）→ WorkspaceImportTask
    if (
        item.workItemType === "IMPORT_BUSINESS_CONFIRMATION" &&
        item.businessObjectType === "LEGACY_IMPORT_BATCH"
    ) {
        return (
            <WorkspaceImportTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 集成结果未知或业务异常：接口差错 / 对账差异 → WorkspaceIntegrationTask
    if (
        (item.workItemType === "INTEGRATION_RESULT_UNKNOWN" ||
            item.workItemType === "BUSINESS_EXCEPTION") &&
        (item.businessObjectType === "integration_error_task" ||
            item.businessObjectType === "reconciliation_difference")
    ) {
        return (
            <WorkspaceIntegrationTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 集成结果未知或业务异常：供应商履约单调查 → WorkspaceSupplierInvestigationTask
    if (
        (item.workItemType === "INTEGRATION_RESULT_UNKNOWN" ||
            item.workItemType === "BUSINESS_EXCEPTION") &&
        item.businessObjectType === "SUPPLIER_FULFILLMENT_ORDER"
    ) {
        return (
            <WorkspaceSupplierInvestigationTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 业务异常：供应商可供货 → WorkspaceSupplyExceptionTask
    if (
        item.workItemType === "BUSINESS_EXCEPTION" &&
        item.businessObjectType === "SUPPLIER_OFFERING"
    ) {
        return (
            <WorkspaceSupplyExceptionTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    // 客户验收登记 → WorkspaceAcceptanceTask
    if (
        item.workItemType === "CUSTOMER_ACCEPTANCE_REGISTRATION" &&
        item.businessObjectType === "sales_order"
    ) {
        return (
            <WorkspaceAcceptanceTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
        )
    }

    if (
        item.workItemType === "FULFILLMENT_OPERATION" &&
        [
            "purchase_receipt",
            "delivery",
            "electronic_delivery",
            "service_fulfillment",
        ].includes(item.businessObjectType)
    ) {
        return (
            <WorkspaceFulfillmentTask
                item={item}
                grantedPermissions={grantedPermissions}
                onTaskCompleted={(id) => onTaskCompleted?.(id)}
            />
        )
    }

    // 非审批任务且没有登记专用作业面 → 停止处理并提示
    if (
        item.workItemType !== "DOCUMENT_APPROVAL" &&
        item.workItemType !== "APPROVAL_INSTANCE"
    ) {
        return (
            <WorkspaceProcessingPane
                archetype="action"
                header={<WorkspaceTaskIdentityHeader item={item} />}
                aria-label="当前任务"
            >
                <Alert variant="destructive">
                    <AlertTitle>暂不支持在工作台直接办理</AlertTitle>
                    <AlertDescription>
                        该事项暂不支持在当前面板直接处理，请前往对应业务模块办理，或联系管理员。
                    </AlertDescription>
                </Alert>
            </WorkspaceProcessingPane>
        )
    }

    // 单据审批 / 审批实例 → WorkspaceDocumentTaskDetail（通用审批作业面）
    return (
        <WorkspaceDocumentTaskDetail
            item={item}
            reworkTask={reworkTask}
            onTaskCompleted={onTaskCompleted}
            canReadSensitive={canReadSensitive}
            grantedPermissions={grantedPermissions}
            onDecisionApplied={onDecisionApplied}
            inspectorMode={inspectorMode}
        />
    )
}

/**
 * 将 `purchase_payment_request` 审批与跟踪收编进统一 2 栏决策工作台：
 * - 保留 4 项余额 KPI 与 `SensitiveValue` 银行账号按权限显隐能力；
 * - 右轨常驻 `ApprovalRouteStepper` + `UnifiedAuditTrail` + 对接 `UnifiedMaterialViewer`；
 * - 付款执行（`SUPPLIER_PAYMENT_EXECUTION`）走 100% 通栏 `archetype="action"`。
 */
function PurchasePaymentRequestSurface({
    item,
    reworkTask,
    canReadSensitive,
    inspectorMode,
    onDecisionApplied,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    reworkTask?: WorkspaceWorkItem
    canReadSensitive: boolean
    inspectorMode: boolean
    onDecisionApplied?: (
        view: ApprovalCommandView,
        completedWorkItemId: string,
    ) => void
    onTaskCompleted?: (workItemId: string, preferredWorkItemId?: string) => void
}) {
    const fullPage = useWorkspaceTaskPage()
    const isPaymentExecution =
        item.workItemType === "SUPPLIER_PAYMENT_EXECUTION"
    const isApprovalInstance = item.workItemType === "APPROVAL_INSTANCE"
    const instanceId =
        item.approvalProcessInstanceId ?? item.approval?.instanceId
    const paymentApprovalInstance =
        !isPaymentExecution && item.approval
            ? {
                  id: item.approval.instanceId,
                  status: item.approval.status,
                  currentRoundNo: item.approval.currentRoundNo,
                  currentNodeName: item.approval.currentNodeLabel,
                  currentAssigneeName: item.approval.currentAssigneeLabel,
                  latestRejection: item.approval.lastRejectReason,
                  latestRejectionByName: item.approval.lastRejectorLabel,
                  processName: item.approval.processName,
                  processVersion: item.approval.processVersion,
              }
            : undefined

    const showDecisionRail = Boolean(
        fullPage && !inspectorMode && !isPaymentExecution && instanceId,
    )
    const railIdPrefix = `workspace-ppr-rail-${toAutomationIdSegment(item.workItemId)}`

    const [railTab, setRailTab] = useState<ApprovalContextRailTab>("trail")
    const [railCollapsed, setRailCollapsed] = useState(false)
    const [activeFileId, setActiveFileId] = useState<string | undefined>(
        undefined,
    )

    useEffect(() => {
        setRailTab("trail")
        setRailCollapsed(false)
        setActiveFileId(undefined)
    }, [item.workItemId])

    useEffect(() => {
        if (!showDecisionRail) return
        return subscribeWorkspaceHotkeyAction((action) => {
            if (action === "toggle-rail") {
                setRailCollapsed((prev) => !prev)
                return
            }
            if (action === "materials") {
                if (railCollapsed || railTab !== "materials") {
                    setRailCollapsed(false)
                    setRailTab("materials")
                    window.requestAnimationFrame(() => {
                        document
                            .getElementById(`${railIdPrefix}-tab-materials`)
                            ?.focus()
                    })
                } else {
                    setRailCollapsed(true)
                }
                return
            }
            if (action === "history") {
                if (railCollapsed || railTab !== "trail") {
                    setRailCollapsed(false)
                    setRailTab("trail")
                    window.requestAnimationFrame(() => {
                        document
                            .getElementById(`${railIdPrefix}-tab-trail`)
                            ?.focus()
                    })
                } else {
                    setRailCollapsed(true)
                }
            }
        })
    }, [railCollapsed, railIdPrefix, railTab, showDecisionRail])

    const scopedWorkItemId =
        !isApprovalInstance &&
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
        showDecisionRail && !isApprovalInstance,
    )
    const submissionQuery = usePaymentSubmissionQuery(
        item.businessObjectId,
        showDecisionRail && submissionVersion ? submissionVersion : null,
    )
    const resolvedRequest = isApprovalInstance
        ? submissionQuery.data
        : requestQuery.data

    const materialsQuery = usePaymentMaterialsQuery(
        showDecisionRail ? item.businessObjectId : "",
        scopedWorkItemId,
        submissionVersion,
    )
    const downloadMutation = useRequestDownloadMutation()

    const dockedMaterialFiles = useMemo<readonly MaterialFileItem[]>(() => {
        if (!showDecisionRail) return []
        const rawFiles = [
            ...new Map(
                [
                    ...(resolvedRequest?.attachments ?? []),
                    ...(materialsQuery.data?.sales.flatMap(
                        (sales) => sales.materials,
                    ) ?? []),
                ].map((file) => [file.file_asset_id, file]),
            ).values(),
        ]
        return rawFiles.map((file) => ({
            id: file.file_asset_id,
            name: file.file_name,
            mimeType: file.content_type ?? undefined,
            loadPreview: () =>
                fetchMaterialPreview(
                    item.businessObjectId,
                    file,
                    scopedWorkItemId,
                    submissionVersion,
                ),
            onDownload: () =>
                downloadMutation.mutateAsync({
                    requestId: item.businessObjectId,
                    attachment: file,
                    workItemId: scopedWorkItemId,
                    submissionVersion,
                }),
        }))
    }, [
        downloadMutation,
        item.businessObjectId,
        materialsQuery.data?.sales,
        resolvedRequest?.attachments,
        scopedWorkItemId,
        showDecisionRail,
        submissionVersion,
    ])

    const compactStepperHistoryQuery = useApprovalHistoryInfiniteQuery(
        { instanceId: instanceId ?? "", order: "desc" },
        !isPaymentExecution && Boolean(instanceId),
    )
    const compactStepperRoute = compactStepperHistoryQuery.data?.pages[0]?.route

    const handleSelectFile = (fileId: string) => {
        setActiveFileId(fileId)
        setRailTab("materials")
        setRailCollapsed(false)
    }

    const timelineTarget = timelineDocumentTypes.has(item.businessObjectType)
        ? {
              kind: (isApprovalInstance ? "approval" : "task") as
                  | "approval"
                  | "task",
              id: item.workItemId,
          }
        : undefined

    return (
        <WorkspaceProcessingPane
            archetype={
                isPaymentExecution
                    ? "action"
                    : showDecisionRail
                      ? "decision"
                      : "informational"
            }
            idPrefix={`workspace-ppr-${toAutomationIdSegment(item.workItemId)}`}
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
                                  instance={paymentApprovalInstance}
                                  timelineTarget={timelineTarget}
                                  currentDocument={{
                                      type: item.businessObjectType,
                                      id: item.businessObjectId,
                                  }}
                                  materialFiles={dockedMaterialFiles}
                                  materialsLoading={materialsQuery.isPending}
                                  activeTab={railTab}
                                  onActiveTabChange={setRailTab}
                                  activeFileId={activeFileId}
                                  onActiveFileChange={setActiveFileId}
                                  idPrefix={`workspace-ppr-rail-${toAutomationIdSegment(item.workItemId)}`}
                              />
                          ),
                      }
                    : undefined
            }
            header={
                <WorkspaceTaskIdentityHeader
                    item={item}
                    title={item.counterpartyName || "付款申请"}
                    badges={
                        isApprovalInstance ? (
                            <StatusBadge
                                label={
                                    item.statusLabel.startsWith("审批")
                                        ? item.statusLabel
                                        : `审批${item.statusLabel}`
                                }
                                tone={item.statusTone}
                            />
                        ) : undefined
                    }
                    subtitle={
                        <span className="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-0.5">
                            <span
                                className="num min-w-0 max-w-full truncate text-xs select-all"
                                title={item.stableNumber}
                            >
                                {item.stableNumber}
                            </span>
                            {item.dueAt ? (
                                <span>截止：{item.dueAtLabel}</span>
                            ) : null}
                        </span>
                    }
                />
            }
            headerSupplement={
                !isPaymentExecution && compactStepperRoute?.nodes.length ? (
                    <ApprovalRouteStepper
                        route={compactStepperRoute}
                        instanceStatus={paymentApprovalInstance?.status}
                        latestRejection={
                            paymentApprovalInstance?.latestRejection
                        }
                        latestRejectionByName={
                            paymentApprovalInstance?.latestRejectionByName
                        }
                        hideRejectionBanner={
                            item.approval?.status === "REJECTED"
                        }
                        idPrefix={`workspace-ppr-compact-stepper-${toAutomationIdSegment(item.workItemId)}`}
                    />
                ) : null
            }
            alertBanner={
                item.approval?.status === "REJECTED" ? (
                    <WorkspaceRejectionNotice
                        item={item}
                        actionable={Boolean(
                            reworkTask?.allowedActions.includes("PROCESS"),
                        )}
                    />
                ) : null
            }
            footer={
                reworkTask &&
                canReadSensitive && (
                    <WorkspaceApprovalReworkActions
                        item={reworkTask}
                        onTaskCompleted={onTaskCompleted}
                    />
                )
            }
            aria-label="采购付款申请任务"
        >
            <PurchasePaymentRequestTask
                item={item}
                onSelectFile={showDecisionRail ? handleSelectFile : undefined}
                activeFileId={activeFileId}
                hideEmbeddedMaterials={showDecisionRail}
                onDecisionApplied={(view: ApprovalCommandView) =>
                    onDecisionApplied?.(view, item.workItemId)
                }
                onTaskCompleted={() => onTaskCompleted?.(item.workItemId)}
            />
        </WorkspaceProcessingPane>
    )
}
