"use client"

import { WorkspaceProcessingPane } from "@/components/business"
import { WorkspaceInspectorModeProvider } from "@/components/business/workspace-task-pane"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import type { ApprovalCommandView } from "@/features/approval-workflow/types"
import { SalesInquiryDetailPage } from "@/features/sales-inquiries/pages/sales-inquiry-detail-page"
import { hasPermission } from "@/lib/permissions"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceAcceptanceTask } from "./workspace-acceptance-task"
import { WorkspaceDocumentTaskDetail } from "./workspace-document-task-detail"
import { WorkspaceFulfillmentTask } from "./workspace-fulfillment-task"
import { WorkspaceImportTask } from "./workspace-import-task"
import { WorkspaceIntegrationTask } from "./workspace-integration-task"
import { WorkspaceInvoiceTask } from "./workspace-invoice-task"
import { WorkspaceProcurementTask } from "./workspace-procurement-task"
import { WorkspacePurchasePaymentArrangementTask } from "./workspace-purchase-payment-arrangement-task"
import { WorkspacePurchasePaymentRequestSurface } from "./workspace-purchase-payment-request-surface"
import { WorkspacePurchaseSourceUpdateTask } from "./workspace-purchase-source-update-task"
import { WorkspaceReturnFinance } from "./workspace-return-finance"
import { WorkspaceSettlementTask } from "./workspace-settlement-task"
import { WorkspaceSupplierInvestigationTask } from "./workspace-supplier-investigation-task"
import { WorkspaceSupplierPortalTask } from "./workspace-supplier-portal-task"
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
        if (
            item.businessObjectType === "purchase_payment_request" &&
            canReadSensitive &&
            hasPermission(grantedPermissions, "purchase_payment_request:detail")
        ) {
            return (
                <WorkspacePurchasePaymentRequestSurface
                    item={item}
                    reworkTask={item}
                    canReadSensitive={canReadSensitive}
                    inspectorMode={inspectorMode}
                    onDecisionApplied={onDecisionApplied}
                    onTaskCompleted={onTaskCompleted}
                />
            )
        }
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
        canReadSensitive &&
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
            <WorkspacePurchasePaymentRequestSurface
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
        item.workItemType === "SUPPLIER_PORTAL_REVIEW" &&
        item.handlerKey === "supplier_portal_review" &&
        item.businessObjectType === "supplier_portal_request"
    ) {
        return (
            <WorkspaceSupplierPortalTask
                key={item.workItemId}
                item={item}
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
                grantedPermissions={grantedPermissions}
                onTaskCompleted={onTaskCompleted}
                inspectorMode={inspectorMode}
            />
        )
    }

    // 待供给分配：销售单生成采购 → WorkspaceProcurementTask
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
            <WorkspacePurchasePaymentArrangementTask
                item={item}
                onTaskCompleted={onTaskCompleted}
            />
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
