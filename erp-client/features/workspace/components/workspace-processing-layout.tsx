"use client"

import type { ReactNode } from "react"
import { detailWorkspaceStyles } from "@/components/business"
import {
    WorkspaceTaskPageProvider,
    WorkspaceTaskPane,
    type WorkspaceTaskPaneProps,
    useWorkspaceTaskPage,
    useWorkspaceTaskPane,
} from "@/components/business/workspace-task-pane"
import { cn } from "@/lib/utils"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import type { WorkspaceWorkItem } from "../types"
import {
    isApprovalWorkbenchTask,
    isPurchaseApprovalTask,
} from "../lib/navigation-eligibility"
import { WorkspaceApprovalProgress } from "./workspace-approval-progress"
import { WorkspaceTaskInstructions } from "./workspace-task-instructions"
import { WorkspaceMoney } from "./workspace-money"

const processingPaneClassName = cn(
    "[&>div:first-child]:-mx-(--scaffold-px) [&>div:first-child]:gap-0 [&>div:first-child]:bg-card [&>div:first-child]:px-(--scaffold-px) [&>div:first-child]:py-0",
    "[&>div:first-child>div:first-child]:contents [&>div:first-child>div:first-child>[data-slot=workspace-task-header]]:hidden",
    "[&>div:first-child>div:nth-child(2)]:mx-0 [&>div:first-child>div:nth-child(2)]:max-w-none [&>div:first-child>div:nth-child(2)]:gap-x-8 [&>div:first-child>div:nth-child(2)]:gap-y-0",
    "[&>div:first-child>div:nth-child(2)>[data-slot=workspace-task-body]]:rounded-none [&>div:first-child>div:nth-child(2)>[data-slot=workspace-task-body]]:border-0 [&>div:first-child>div:nth-child(2)>[data-slot=workspace-task-body]]:shadow-none",
    "[&>div:first-child>div:nth-child(2)>aside]:rounded-none [&>div:first-child>div:nth-child(2)>aside]:border-0 [&>div:first-child>div:nth-child(2)>aside]:border-t [&>div:first-child>div:nth-child(2)>aside]:border-border/60 [&>div:first-child>div:nth-child(2)>aside]:px-0 [&>div:first-child>div:nth-child(2)>aside]:shadow-none xl:[&>div:first-child>div:nth-child(2)>aside]:static xl:[&>div:first-child>div:nth-child(2)>aside]:self-stretch xl:[&>div:first-child>div:nth-child(2)>aside]:border-t-0 xl:[&>div:first-child>div:nth-child(2)>aside]:border-l xl:[&>div:first-child>div:nth-child(2)>aside]:pl-6",
    "[&>div:first-child>div:nth-child(2)>[data-slot=workspace-task-body]>div:last-child]:px-0",
    "[&>footer]:-mx-(--scaffold-px) [&>footer]:w-auto [&>footer]:px-(--scaffold-px)",
    "[&_:has(>[data-slot=workspace-processing-summary])]:border-b [&_:has(>[data-slot=workspace-processing-summary])]:border-border/60 [&_:has(>[data-slot=workspace-processing-summary])]:px-0 [&_:has(>[data-slot=workspace-processing-summary])]:py-3.5 [&_:has(>[data-slot=workspace-processing-summary]:empty+div:empty)]:hidden",
)

// 完整办理页保留原面板的滚动与插槽；弹窗继续使用原面板布局。
export function WorkspaceProcessingPane({
    header,
    headerSupplement,
    className,
    ...props
}: WorkspaceTaskPaneProps) {
    const fullPage = useWorkspaceTaskPage()
    const nested = useWorkspaceTaskPane()
    if (!fullPage || nested)
        return (
            <WorkspaceTaskPane
                header={header}
                headerSupplement={headerSupplement}
                className={className}
                {...props}
            />
        )
    return (
        <WorkspaceTaskPane
            {...props}
            className={cn(processingPaneClassName, className)}
            header={null}
            headerSupplement={
                <>
                    {header}
                    {headerSupplement}
                </>
            }
        />
    )
}

/** 所有审批、执行与只读跟踪入口共用完整办理页，动作仍由各业务模块提供。 */
export function WorkspaceProcessingLayout({
    item,
    reworkTask,
    children,
    autoNext,
    allowAutoNext,
    onAutoNextChange,
    onBack,
    backLabel,
}: {
    item: WorkspaceWorkItem
    reworkTask?: WorkspaceWorkItem
    children: ReactNode
    autoNext: boolean
    allowAutoNext: boolean
    onAutoNextChange: (value: boolean) => void
    onBack: () => void
    backLabel: string
}) {
    const fulfillment = item.workItemType === "FULFILLMENT_OPERATION"
    const approval = item.approval
    const purchaseApproval = isPurchaseApprovalTask(item)
    const salesApproval =
        ["sales_order", "voucher_sales_order"].includes(
            item.businessObjectType,
        ) &&
        (isApprovalWorkbenchTask(
            item.allowedActions,
            item.approvalProcessInstanceId,
            item.approvalNodeExecutionId,
        ) ||
            item.workItemType === "APPROVAL_INSTANCE" ||
            item.workItemType === "DOCUMENT_APPROVAL")
    const payment = item.businessObjectType === "purchase_payment_request"
    const paymentReview =
        payment && item.workItemType !== "SUPPLIER_PAYMENT_EXECUTION"
    const paymentArrangement =
        item.workItemType === "PURCHASE_PAYMENT_ARRANGEMENT"
    const actionable = (reworkTask ?? item).allowedActions.some((action) =>
        ["PROCESS", "APPROVE", "REJECT"].includes(action),
    )
    return (
        <WorkspaceTaskPageProvider
            singleColumn={
                fulfillment ||
                purchaseApproval ||
                salesApproval ||
                paymentReview
            }
            summaryInHeader={fulfillment}
            denseHeader={fulfillment}
            fillBody={fulfillment && item.businessObjectType === "delivery"}
            compact={paymentArrangement}
            fullWidth={
                item.workItemType === "PROCUREMENT_ORDER_CREATION" &&
                item.businessObjectType === "sales_order" &&
                item.allowedActions.includes("PROCESS")
            }
            summary={
                <div
                    data-slot="workspace-processing-summary"
                    className={
                        fulfillment
                            ? "flex flex-wrap items-start justify-between gap-x-6 gap-y-2"
                            : undefined
                    }
                >
                    {payment ||
                    paymentArrangement ||
                    purchaseApproval ? undefined : (
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
                                label={
                                    reworkTask || !approval
                                        ? "当前处理人"
                                        : ["RUNNING", "BLOCKED"].includes(
                                                approval.status,
                                            )
                                          ? "当前审批人"
                                          : "审批状态"
                                }
                                value={
                                    reworkTask?.ownerUserLabel ||
                                    (approval &&
                                    !["RUNNING", "BLOCKED"].includes(
                                        approval.status,
                                    )
                                        ? item.statusLabel
                                        : approval?.currentAssigneeLabel ||
                                          item.ownerUserLabel)
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
                    )}
                </div>
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
                                <h3 className="font-medium">办理状态</h3>
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
                            <h3 className="font-medium">处理说明</h3>
                            <WorkspaceTaskInstructions item={item} />
                        </section>
                    )}
                </>
            }
            footerLeading={
                <>
                    {allowAutoNext &&
                    actionable &&
                    (reworkTask ||
                        item.workItemType !== "APPROVAL_INSTANCE") ? (
                        <label
                            htmlFor="workspace-processing-auto-next"
                            className="mr-auto flex items-center gap-2 text-xs text-muted-foreground"
                        >
                            <NativeCheckbox
                                id="workspace-processing-auto-next"
                                checked={autoNext}
                                onCheckedChange={(value) =>
                                    onAutoNextChange(value === true)
                                }
                            />
                            {fulfillment ? "完成后下一项" : "处理后进入下一项"}
                        </label>
                    ) : (
                        <span className="mr-auto text-xs text-muted-foreground">
                            {item.workItemType === "APPROVAL_INSTANCE"
                                ? "审批进度与提交记录"
                                : "当前任务资料"}
                        </span>
                    )}
                    {purchaseApproval && actionable && !reworkTask && (
                        <span className="text-xs text-muted-foreground">
                            本次操作审批整张采购单
                        </span>
                    )}
                </>
            }
            footer={
                fulfillment ? null : (
                    <Button
                        id="workspace-processing-footer-back"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.quietButton}
                        onClick={onBack}
                    >
                        {backLabel}
                    </Button>
                )
            }
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
