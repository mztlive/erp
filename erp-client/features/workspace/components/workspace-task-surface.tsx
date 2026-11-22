"use client"

import { WorkspaceProcessingPane } from "./workspace-processing-layout"
import { PurchasePaymentArrangement } from "@/features/purchase-orders/components/purchase-payment-arrangement"
import { SalesInquiryDetailPage } from "@/features/sales-inquiries/pages/sales-inquiry-detail-page"
import { WorkspaceApprovalReworkActions } from "./workspace-approval-rework"
import { WorkspaceRejectionNotice } from "./workspace-rejection-notice"
import { useWorkspaceTaskPage } from "@/components/business/workspace-task-pane"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { DetailWorkspaceChrome } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { StatusBadge } from "@/components/ui/status-badge"
import { WorkspaceTaskHeaderActions } from "./workspace-task-context"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceAcceptanceTask } from "./workspace-acceptance-task"
import { WorkspaceFulfillmentTask } from "./workspace-fulfillment-task"
import { WorkspaceInvoiceTask } from "./workspace-invoice-task"
import { WorkspaceImportTask } from "./workspace-import-task"
import { WorkspaceIntegrationTask } from "./workspace-integration-task"
import { PurchasePaymentRequestTask } from "@/features/purchase-payment-requests/components/purchase-payment-request-task"
import { WorkspaceProcurementTask } from "./workspace-procurement-task"
import { WorkspaceReturnFinance } from "./workspace-return-finance"
import { WorkspacePurchaseSourceUpdateTask } from "./workspace-purchase-source-update-task"
import { WorkspaceSettlementTask } from "./workspace-settlement-task"
import { WorkspaceSupplierInvestigationTask } from "./workspace-supplier-investigation-task"
import { WorkspaceSupplyExceptionTask } from "./workspace-supply-exception-task"
import { PortalAdminReviewPage } from "@/features/supplier-portal-admin/pages/review-page"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"
import { WorkspaceDocumentTaskDetail } from "./workspace-document-task-detail"

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
}>
export function WorkspaceTaskSurface({
    item,
    reworkTask,
    canReadSensitive = true,
    onDecisionApplied,
    grantedPermissions = [],
    onTaskCompleted,
}: WorkspaceTaskDetailProps) {
    const fullPage = useWorkspaceTaskPage()
    if (item.workItemType === "APPROVAL_REWORK") {
        return (
            <WorkspaceDocumentTaskDetail
                item={item}
                reworkTask={item}
                canReadSensitive={canReadSensitive}
                grantedPermissions={grantedPermissions}
                onTaskCompleted={onTaskCompleted}
                onDecisionApplied={onDecisionApplied}
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
            <WorkspaceProcessingPane
                header={
                    <WorkspaceTaskIdentityHeader
                        item={item}
                        title={
                            !fullPage ||
                            item.workItemType === "SUPPLIER_PAYMENT_EXECUTION"
                                ? item.counterpartyName || "付款申请"
                                : "付款申请审批"
                        }
                        badges={
                            item.workItemType === "APPROVAL_INSTANCE" ? (
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
                            <span className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1">
                                <span
                                    className="num min-w-0 max-w-full truncate text-xs select-all"
                                    title={item.stableNumber}
                                >
                                    {item.stableNumber}
                                </span>
                                {item.workItemType === "DOCUMENT_APPROVAL" &&
                                    item.status === "OPEN" && (
                                        <span>
                                            当前审批人：{item.ownerUserLabel}
                                        </span>
                                    )}
                                {item.workItemType === "APPROVAL_INSTANCE" &&
                                    ["RUNNING", "BLOCKED"].includes(
                                        item.approval?.status ?? "",
                                    ) && (
                                        <span>
                                            {item.approval?.currentNodeLabel} ·{" "}
                                            {item.ownerUserLabel}
                                        </span>
                                    )}
                            </span>
                        }
                    />
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
                {item.approval?.status === "REJECTED" && (
                    <WorkspaceRejectionNotice
                        item={item}
                        actionable={Boolean(
                            reworkTask?.allowedActions.includes("PROCESS"),
                        )}
                    />
                )}
                <PurchasePaymentRequestTask
                    item={item}
                    onDecisionApplied={(view: ApprovalCommandView) =>
                        onDecisionApplied?.(view, item.workItemId)
                    }
                    onTaskCompleted={() => onTaskCompleted?.(item.workItemId)}
                />
            </WorkspaceProcessingPane>
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
                header={
                    <DetailWorkspaceChrome
                        title={item.counterpartyName || "采购付款安排"}
                        meta={
                            <span className="wrap-anywhere">
                                {item.objectTitle}
                            </span>
                        }
                        titleExtra={
                            <span className="text-xs text-muted-foreground">
                                {item.workItemTypeLabel}
                            </span>
                        }
                        secondaryActions={
                            <WorkspaceTaskHeaderActions item={item} />
                        }
                    />
                }
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
        />
    )
}
