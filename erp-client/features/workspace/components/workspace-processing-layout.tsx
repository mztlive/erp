"use client"

import type { ReactNode } from "react"
import { WorkspaceTaskPageProvider } from "@/components/business/workspace-task-pane"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceApprovalProgress } from "./workspace-approval-progress"
import { WorkspaceTaskInstructions } from "./workspace-task-instructions"
import { WorkspaceMoney } from "./workspace-money"

/** 所有审批、执行与只读跟踪入口共用完整办理页，动作仍由各业务模块提供。 */
export function WorkspaceProcessingLayout({
    item,
    children,
    autoNext,
    onAutoNextChange,
    onBack,
    backLabel,
}: {
    item: WorkspaceWorkItem
    children: ReactNode
    autoNext: boolean
    onAutoNextChange: (value: boolean) => void
    onBack: () => void
    backLabel: string
}) {
    const fulfillment = item.workItemType === "FULFILLMENT_OPERATION"
    const approval = item.approval
    const payment = item.businessObjectType === "purchase_payment_request"
    const paymentArrangement =
        item.workItemType === "PURCHASE_PAYMENT_ARRANGEMENT"
    const actionable = item.allowedActions.some((action) =>
        ["PROCESS", "APPROVE", "REJECT"].includes(action),
    )
    return (
        <WorkspaceTaskPageProvider
            compact={paymentArrangement}
            summary={
                <>
                    {payment || paymentArrangement ? undefined : (
                        <dl className="flex flex-wrap gap-x-8 gap-y-4 rounded-lg border border-border/60 bg-muted/15 p-4">
                            {item.amountSummary && (
                                <Fact
                                    label={item.amountSummary.label}
                                    value={
                                        <WorkspaceMoney
                                            value={item.amountSummary.value}
                                        />
                                    }
                                />
                            )}
                            <Fact
                                label={approval ? "当前审批人" : "当前处理人"}
                                value={
                                    approval?.currentAssigneeLabel ||
                                    item.ownerUserLabel
                                }
                            />
                            <Fact
                                label="任务类型"
                                value={item.workItemTypeLabel}
                            />
                            <Fact
                                label="截止时间"
                                value={item.dueAtLabel || "未设截止时间"}
                            />
                        </dl>
                    )}
                </>
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
                    {actionable && item.workItemType !== "APPROVAL_INSTANCE" ? (
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
                </>
            }
            footer={
                <Button
                    id="workspace-processing-footer-back"
                    variant="outline"
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

function Fact({ label, value }: { label: string; value: ReactNode }) {
    return (
        <div className="min-w-[9rem] flex-1 space-y-1.5">
            <dt className="text-xs text-muted-foreground">{label}</dt>
            <dd className="break-words text-sm font-medium leading-6">
                {value || "—"}
            </dd>
        </div>
    )
}
