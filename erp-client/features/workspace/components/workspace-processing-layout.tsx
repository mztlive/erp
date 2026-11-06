"use client"

import type { ReactNode } from "react"
import { WorkspaceTaskPageProvider } from "@/components/business/workspace-task-pane"
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
            compact={paymentArrangement}
            fullWidth={
                item.workItemType === "PROCUREMENT_ORDER_CREATION" &&
                item.businessObjectType === "sales_order" &&
                item.allowedActions.includes("PROCESS")
            }
            summary={
                <div
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
                                    : "flex flex-wrap gap-x-8 gap-y-4 rounded-lg border border-border/60 bg-muted/15 p-4"
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
                    {fulfillment && (
                        <details
                            open={item.actionBlockers.length > 0}
                            className="text-sm leading-6 text-muted-foreground"
                        >
                            <summary
                                id="workspace-fulfillment-instructions-toggle"
                                className="w-fit cursor-pointer text-xs"
                            >
                                办理说明
                            </summary>
                            <div className="mt-2 space-y-1 border-l-2 border-border pl-3 leading-6">
                                <p>
                                    {item.nextActionHint ||
                                        "按收货地址核对商品、数量与物流后，确认本次发货。"}
                                </p>
                                {item.impactSummary && (
                                    <p>{item.impactSummary}</p>
                                )}
                                {item.actionBlockers.map((blocker) => (
                                    <p key={blocker.message}>
                                        {blocker.message}
                                    </p>
                                ))}
                            </div>
                        </details>
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
                                <h3 className="text-sm font-medium">
                                    办理状态
                                </h3>
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
                        <section className="space-y-4 border-t border-border/60 pt-5 text-sm leading-6">
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
                            {fulfillment
                                ? "确认成功后自动打开下一项"
                                : "处理后进入下一项"}
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
                <Button
                    id="workspace-processing-footer-back"
                    variant={fulfillment ? "ghost" : "outline"}
                    onClick={onBack}
                >
                    {backLabel}
                </Button>
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
            <dd className="break-words text-sm font-medium leading-6">
                {value || "—"}
            </dd>
        </div>
    )
}
