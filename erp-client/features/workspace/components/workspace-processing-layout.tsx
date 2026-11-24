"use client"

import type { ReactNode } from "react"
import type { TaskWorkbenchArchetype } from "@/components/business/unified-task-workbench"
import {
    WorkspaceTaskPageProvider,
    WorkspaceTaskPane,
    type WorkspaceTaskBackLink,
    type WorkspaceTaskPaneProps,
} from "@/components/business/workspace-task-pane"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { AUTO_NEXT_LABEL } from "@/lib/task-status-dictionary"
import type { WorkspaceWorkItem } from "../types"
import { isApprovalWorkbenchTask } from "../lib/navigation-eligibility"

const ACTION_TASK_TYPES = new Set([
    "PURCHASE_PAYMENT_ARRANGEMENT",
    "PURCHASE_SOURCE_UPDATE",
    "PURCHASE_RETURN_REFUND",
    "PURCHASE_RETURN_INVOICE",
    "CUSTOMER_ACCEPTANCE_REGISTRATION",
    "SUPPLIER_PAYMENT_EXECUTION",
    "SALES_INVOICE_EXECUTION",
    "SUPPLIER_SETTLEMENT_REVIEW",
    "SUPPLIER_PORTAL_REVIEW",
    "IMPORT_BUSINESS_CONFIRMATION",
    "FULFILLMENT_OPERATION",
    "PROCUREMENT_ORDER_CREATION",
    "SALES_INQUIRY_PROCESSING",
])

const INVESTIGATION_TASK_TYPES = new Set([
    "INTEGRATION_RESULT_UNKNOWN",
    "BUSINESS_EXCEPTION",
])

/** 完整办理页直接复用统一工作台骨架（UnifiedTaskWorkbench）。 */
export function WorkspaceProcessingPane(props: WorkspaceTaskPaneProps) {
    return <WorkspaceTaskPane {...props} />
}

/**
 * 所有审批、执行与只读跟踪入口共用纯白通栏工作台骨架：
 * - Archetype 1 (`decision`)：全部 13 类审批单据与退回重办解锁 2 栏决策工作台（右轨挂载路线/审计轨迹/侧边附件对照）；
 * - Archetype 2 (`action`)：全部执行类任务撤除冗余 376px 通用 metadata aside，释放 100% 通栏作业面；
 * - Archetype 3 (`investigation`)：异常排查类任务保持单主壳，不重复堆砌 20rem 元数据侧栏。
 */
export function WorkspaceProcessingLayout({
    item,
    reworkTask,
    children,
    autoNext,
    allowAutoNext,
    onAutoNextChange,
    back,
    queueNavigation,
    onRefresh,
    refreshing = false,
}: {
    item: WorkspaceWorkItem
    reworkTask?: WorkspaceWorkItem
    children: ReactNode
    autoNext: boolean
    allowAutoNext: boolean
    onAutoNextChange: (value: boolean) => void
    back?: WorkspaceTaskBackLink
    queueNavigation?: ReactNode
    onRefresh?: () => void
    refreshing?: boolean
}) {
    const fulfillment = item.workItemType === "FULFILLMENT_OPERATION"
    const canProcessFulfillment =
        fulfillment && item.allowedActions.includes("PROCESS")
    const isActionTask = ACTION_TASK_TYPES.has(item.workItemType)
    const isInvestigationTask = INVESTIGATION_TASK_TYPES.has(item.workItemType)
    const approval = item.approval
    const approvalDocument =
        isApprovalWorkbenchTask(
            item.allowedActions,
            item.approvalProcessInstanceId,
            item.approvalNodeExecutionId,
        ) ||
        item.workItemType === "DOCUMENT_APPROVAL" ||
        item.workItemType === "APPROVAL_INSTANCE" ||
        item.workItemType === "APPROVAL_REWORK" ||
        (!isActionTask &&
            !isInvestigationTask &&
            Boolean(approval || item.approvalProcessInstanceId))
    const payment = item.businessObjectType === "purchase_payment_request"
    const paymentReview =
        payment && item.workItemType !== "SUPPLIER_PAYMENT_EXECUTION"
    const inquiryProcessing =
        item.workItemType === "SALES_INQUIRY_PROCESSING" &&
        item.handlerKey === "sales_inquiry_processing" &&
        item.businessObjectType === "sales_inquiry"

    const archetype: TaskWorkbenchArchetype =
        approvalDocument || paymentReview
            ? "decision"
            : isInvestigationTask
              ? "investigation"
              : "action"

    const actionable = (reworkTask ?? item).allowedActions.some((action) =>
        [
            "PROCESS",
            "APPROVE",
            "REJECT",
            "CANCEL",
            "CANCEL_APPROVAL",
            "RESUME_CURRENT_APPROVER",
            "CANCEL_BLOCKED_APPROVAL",
        ].includes(action),
    )
    const showAutoNext = allowAutoNext && actionable

    return (
        <WorkspaceTaskPageProvider
            flat
            archetype={archetype}
            navigation={{
                back,
                meta: queueNavigation,
                onRefresh,
                refreshing,
            }}
            singleColumn={
                archetype === "action" || archetype === "investigation"
            }
            summaryInHeader={false}
            denseHeader={canProcessFulfillment}
            fillBody={
                canProcessFulfillment && item.businessObjectType === "delivery"
            }
            fullWidth={
                inquiryProcessing ||
                (item.workItemType === "PROCUREMENT_ORDER_CREATION" &&
                    item.businessObjectType === "sales_order" &&
                    item.allowedActions.includes("PROCESS"))
            }
            summary={null}
            aside={null}
            footerLeading={
                showAutoNext ? (
                    <label
                        htmlFor="workspace-processing-auto-next"
                        className="flex shrink-0 cursor-pointer items-center gap-2 text-xs text-muted-foreground select-none hover:text-foreground"
                    >
                        <NativeCheckbox
                            id="workspace-processing-auto-next"
                            checked={autoNext}
                            onCheckedChange={(value) =>
                                onAutoNextChange(value === true)
                            }
                        />
                        {AUTO_NEXT_LABEL}
                    </label>
                ) : null
            }
            footer={null}
        >
            {children}
        </WorkspaceTaskPageProvider>
    )
}
