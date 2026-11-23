"use client"

import type { ReactNode } from "react"
import {
    WorkspaceTaskPageProvider,
    WorkspaceTaskPane,
    type WorkspaceTaskPaneProps,
} from "@/components/business/workspace-task-pane"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { StatusBadge } from "@/components/ui/status-badge"
import type { WorkspaceWorkItem } from "../types"
import { isApprovalWorkbenchTask } from "../lib/navigation-eligibility"
import { WorkspaceApprovalProgress } from "./workspace-approval-progress"
import { WorkspaceTaskInstructions } from "./workspace-task-instructions"
import { WorkspaceMoney } from "./workspace-money"

/** 完整办理页直接复用纯白通栏骨架，弹窗继续使用卡片布局。 */
export function WorkspaceProcessingPane(props: WorkspaceTaskPaneProps) {
    return <WorkspaceTaskPane {...props} />
}

/** 所有审批、执行与只读跟踪入口共用纯白通栏办理页，动作仍由各业务模块提供。 */
export function WorkspaceProcessingLayout({
    item,
    reworkTask,
    children,
    autoNext,
    allowAutoNext,
    onAutoNextChange,
    onBack,
    backLabel,
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
    onBack: () => void
    backLabel: string
    queueNavigation?: ReactNode
    onRefresh?: () => void
    refreshing?: boolean
}) {
    const fulfillment = item.workItemType === "FULFILLMENT_OPERATION"
    const approval = item.approval
    const approvalDocument =
        Boolean(approval || item.approvalProcessInstanceId) ||
        isApprovalWorkbenchTask(
            item.allowedActions,
            item.approvalProcessInstanceId,
            item.approvalNodeExecutionId,
        ) ||
        item.workItemType === "DOCUMENT_APPROVAL" ||
        item.workItemType === "APPROVAL_INSTANCE" ||
        item.workItemType === "APPROVAL_REWORK"
    const payment = item.businessObjectType === "purchase_payment_request"
    const paymentReview =
        payment && item.workItemType !== "SUPPLIER_PAYMENT_EXECUTION"
    const paymentArrangement =
        item.workItemType === "PURCHASE_PAYMENT_ARRANGEMENT"
    const inquiryProcessing =
        item.workItemType === "SALES_INQUIRY_PROCESSING" &&
        item.handlerKey === "sales_inquiry_processing" &&
        item.businessObjectType === "sales_inquiry"
    const actionable = (reworkTask ?? item).allowedActions.some((action) =>
        ["PROCESS", "APPROVE", "REJECT"].includes(action),
    )
    const showAutoNext =
        allowAutoNext &&
        actionable &&
        (Boolean(reworkTask) || item.workItemType !== "APPROVAL_INSTANCE")

    return (
        <WorkspaceTaskPageProvider
            flat
            navigation={{
                back: {
                    id: "workspace-processing-back",
                    label: backLabel,
                    onClick: onBack,
                    visibility: "always",
                },
                meta: queueNavigation,
                onRefresh,
                refreshing,
            }}
            singleColumn={
                fulfillment ||
                approvalDocument ||
                paymentReview ||
                inquiryProcessing
            }
            summaryInHeader={fulfillment}
            denseHeader={fulfillment}
            fillBody={fulfillment && item.businessObjectType === "delivery"}
            compact={paymentArrangement}
            fullWidth={
                inquiryProcessing ||
                (item.workItemType === "PROCUREMENT_ORDER_CREATION" &&
                    item.businessObjectType === "sales_order" &&
                    item.allowedActions.includes("PROCESS"))
            }
            summary={
                payment || paymentArrangement || approvalDocument ? null : (
                    <div
                        data-slot="workspace-processing-summary"
                        className={
                            fulfillment
                                ? "flex flex-wrap items-start justify-between gap-x-6 gap-y-2"
                                : undefined
                        }
                    >
                        <dl
                            className={
                                fulfillment
                                    ? "flex flex-wrap items-center gap-x-7 gap-y-2"
                                    : "flex flex-wrap gap-x-8 gap-y-4"
                            }
                        >
                            {item.amountSummary && (
                                <Fact
                                    inline={fulfillment}
                                    label={item.amountSummary.label}
                                    value={
                                        <WorkspaceMoney
                                            value={item.amountSummary.value}
                                        />
                                    }
                                />
                            )}
                            <Fact
                                inline={fulfillment}
                                label="当前处理人"
                                value={
                                    reworkTask?.ownerUserLabel ||
                                    item.ownerUserLabel
                                }
                            />
                            {!fulfillment && (
                                <Fact
                                    inline={fulfillment}
                                    label="任务类型"
                                    value={item.workItemTypeLabel}
                                />
                            )}
                            {fulfillment && (
                                <Fact
                                    inline={fulfillment}
                                    label="进入时间"
                                    value={item.enteredAtLabel}
                                />
                            )}
                            {(!fulfillment || item.dueAt) && (
                                <Fact
                                    inline={fulfillment}
                                    label="截止时间"
                                    value={item.dueAtLabel || "未设截止时间"}
                                />
                            )}
                        </dl>
                    </div>
                )
            }
            aside={
                <>
                    {approval ? (
                        <WorkspaceApprovalProgress
                            vertical
                            instance={{
                                id: approval.instanceId,
                                status: approval.status,
                                currentRoundNo: approval.currentRoundNo,
                                currentNodeName: approval.currentNodeLabel,
                                currentAssigneeName:
                                    approval.currentAssigneeLabel,
                                latestRejection: approval.lastRejectReason,
                                latestRejectionByName:
                                    approval.lastRejectorLabel,
                                processName: approval.processName,
                                processVersion: approval.processVersion,
                            }}
                        />
                    ) : (
                        <section className="space-y-4">
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <h3 className="font-semibold">办理状态</h3>
                                <StatusBadge
                                    label={item.statusLabel}
                                    tone={item.statusTone}
                                />
                            </div>
                            <dl className="space-y-4">
                                <Fact
                                    label="当前责任"
                                    value={[
                                        item.ownerRoleLabel,
                                        item.ownerUserLabel,
                                    ]
                                        .filter(Boolean)
                                        .join(" · ")}
                                />
                                <Fact
                                    label="进入时间"
                                    value={item.enteredAtLabel}
                                />
                                {paymentArrangement && item.dueAt && (
                                    <Fact
                                        label="截止时间"
                                        value={item.dueAtLabel}
                                    />
                                )}
                            </dl>
                        </section>
                    )}
                    {(item.nextActionHint ||
                        item.impactSummary ||
                        item.actionBlockers.length > 0) && (
                        <section className="space-y-4 border-t border-border/60 pt-5 text-body-compact leading-6">
                            <h3 className="font-semibold">处理说明</h3>
                            <WorkspaceTaskInstructions item={item} />
                        </section>
                    )}
                </>
            }
            footerLeading={
                showAutoNext ? (
                    <label
                        htmlFor="workspace-processing-auto-next"
                        className="flex cursor-pointer items-center gap-2 text-xs text-muted-foreground select-none hover:text-foreground"
                    >
                        <NativeCheckbox
                            id="workspace-processing-auto-next"
                            checked={autoNext}
                            onCheckedChange={(value) =>
                                onAutoNextChange(value === true)
                            }
                        />
                        {fulfillment ? "完成后下一项" : "处理后自动进入下一项"}
                    </label>
                ) : null
            }
            footer={null}
        >
            {children}
        </WorkspaceTaskPageProvider>
    )
}

function Fact({
    label,
    value,
    inline = false,
}: {
    label: string
    value: ReactNode
    inline?: boolean
}) {
    return (
        <div
            className={
                inline
                    ? "flex min-w-0 items-baseline gap-2"
                    : "min-w-[9rem] flex-1 space-y-1.5"
            }
        >
            <dt className="shrink-0 text-xs text-muted-foreground">{label}</dt>
            <dd className="break-words text-body-compact font-medium leading-6">
                {value || "—"}
            </dd>
        </div>
    )
}
