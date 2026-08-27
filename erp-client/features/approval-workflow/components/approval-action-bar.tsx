"use client"

import * as React from "react"

import { Button } from "@/components/ui/button"

import { RECOVERY_ACTION_LABEL } from "../display"
import type {
    ApprovalAllowedAction,
    ApprovalCommandView,
    ApprovalDefinitionBinding,
    ApprovalRuntimeInstance,
    RecoveryOption,
} from "../types"
import { CancelApprovalDialog } from "./cancel-approval-dialog"
import { DecisionDialog } from "./decision-dialog"
import { ResumeApproverDialog } from "./resume-approver-dialog"
import { UpgradeBindingDialog } from "./upgrade-binding-dialog"

type DialogKind =
    | "approve"
    | "reject"
    | "resume"
    | "cancel-blocked"
    | "withdraw"
    | "upgrade"
    | null

/**
 * 按服务端 `allowed_actions` 与 `recovery_options` 渲染动作入口。
 *
 * 审批任务只展示审批运行时动作，不展示通用任务责任命令。
 */
export function ApprovalActionBar({
    allowedActions,
    recoveryOptions = [],
    workItemId,
    expectedTaskVersion,
    instance,
    definition,
    documentType,
    documentId,
    documentHref,
    afterCancelStatusLabel = "未提交",
    emergencyWithdraw = false,
    canReadSensitive = true,
    hiddenActions = [],
    decisionContext,
    onDecisionApplied,
}: {
    allowedActions: readonly string[]
    recoveryOptions?: readonly RecoveryOption[]
    workItemId?: string
    expectedTaskVersion?: string
    instance?: ApprovalRuntimeInstance
    definition?: ApprovalDefinitionBinding
    documentType?: string
    documentId?: string
    documentHref?: string | null
    afterCancelStatusLabel?: string
    emergencyWithdraw?: boolean
    canReadSensitive?: boolean
    /** 由对象中心页头承接的动作，审批区内不再重复渲染。 */
    hiddenActions?: readonly string[]
    /** 工作台提供的审批事实；确认弹窗只展示，不参与命令构造。 */
    decisionContext?: Readonly<{
        documentLabel?: string
        amountLabel?: string
        currentNodeLabel?: string
        impactSummary?: string
    }>
    onDecisionApplied?: (view: ApprovalCommandView) => void
}) {
    const [dialog, setDialog] = React.useState<DialogKind>(null)
    const hidden = new Set(hiddenActions)
    const actions = new Set(
        allowedActions.filter((action) => !hidden.has(action)),
    )
    const recoveries = new Set(recoveryOptions)
    const showApprove = actions.has("APPROVE")
    const showReject = actions.has("REJECT")
    const showResume =
        recoveries.has("RESUME_CURRENT_APPROVER") &&
        actions.has("RESUME_CURRENT_APPROVER")
    const showCancelBlocked =
        recoveries.has("CANCEL_BLOCKED") &&
        actions.has("CANCEL_BLOCKED_APPROVAL")
    const showWithdraw = actions.has("CANCEL") || actions.has("CANCEL_APPROVAL")
    const showUpgrade = actions.has("UPGRADE_BINDING") && Boolean(definition)
    const showOpenDocument =
        (actions.has("OPEN_DOCUMENT") || actions.has("VIEW")) &&
        Boolean(documentHref)

    return (
        <div className="flex flex-wrap gap-2">
            {showApprove && workItemId && expectedTaskVersion ? (
                <Button type="button" onClick={() => setDialog("approve")}>
                    通过
                </Button>
            ) : null}
            {showReject && workItemId && expectedTaskVersion ? (
                <Button
                    type="button"
                    variant="destructive"
                    onClick={() => setDialog("reject")}
                >
                    驳回
                </Button>
            ) : null}
            {showOpenDocument ? (
                <Button
                    type="button"
                    variant="outline"
                    render={
                        <a
                            href={documentHref ?? undefined}
                            aria-label="打开单据"
                        />
                    }
                >
                    打开单据
                </Button>
            ) : null}
            {showResume ? (
                <Button
                    type="button"
                    variant="outline"
                    onClick={() => setDialog("resume")}
                >
                    {RECOVERY_ACTION_LABEL.RESUME_CURRENT_APPROVER}
                </Button>
            ) : null}
            {showCancelBlocked ? (
                <Button
                    type="button"
                    variant="outline"
                    onClick={() => setDialog("cancel-blocked")}
                >
                    {RECOVERY_ACTION_LABEL.CANCEL_BLOCKED}
                </Button>
            ) : null}
            {showWithdraw ? (
                <Button
                    type="button"
                    variant="outline"
                    onClick={() => setDialog("withdraw")}
                >
                    {emergencyWithdraw ? "应急撤回审批" : "撤回审批"}
                </Button>
            ) : null}
            {showUpgrade ? (
                <Button
                    type="button"
                    variant="outline"
                    onClick={() => setDialog("upgrade")}
                >
                    更新审批流程版本
                </Button>
            ) : null}
            {!canReadSensitive ? (
                <p className="w-full text-sm text-muted-foreground">
                    当前账号无权查看部分业务字段
                </p>
            ) : null}
            {workItemId && expectedTaskVersion ? (
                <DecisionDialog
                    open={dialog === "reject" || dialog === "approve"}
                    onOpenChange={(open) => setDialog(open ? dialog : null)}
                    workItemId={workItemId}
                    expectedTaskVersion={expectedTaskVersion}
                    defaultDecision={dialog === "reject" ? "REJECT" : "APPROVE"}
                    allowedActions={allowedActions}
                    context={decisionContext}
                    onApplied={onDecisionApplied}
                />
            ) : null}
            {instance ? (
                <ResumeApproverDialog
                    open={dialog === "resume"}
                    onOpenChange={(open) => setDialog(open ? "resume" : null)}
                    instanceId={instance.id}
                    expectedInstanceVersion={instance.instanceVersion ?? ""}
                    expectedExecutionVersion={instance.executionVersion ?? ""}
                    expectedAssignmentVersion={instance.assignmentVersion ?? ""}
                    onApplied={onDecisionApplied}
                />
            ) : null}
            {instance ? (
                <CancelApprovalDialog
                    open={dialog === "withdraw" || dialog === "cancel-blocked"}
                    onOpenChange={(open) => setDialog(open ? dialog : null)}
                    mode={
                        dialog === "cancel-blocked"
                            ? "cancel-blocked"
                            : "withdraw"
                    }
                    instanceId={instance.id}
                    documentType={documentType}
                    documentId={documentId}
                    currentNodeName={instance.currentNodeName}
                    afterStatusLabel={afterCancelStatusLabel}
                    expectedInstanceVersion={instance.instanceVersion ?? ""}
                    expectedExecutionVersion={instance.executionVersion ?? ""}
                    expectedTaskVersion={expectedTaskVersion}
                    emergency={emergencyWithdraw && dialog === "withdraw"}
                    onApplied={onDecisionApplied}
                />
            ) : null}
            {definition && documentType && documentId ? (
                <UpgradeBindingDialog
                    open={dialog === "upgrade"}
                    onOpenChange={(open) => setDialog(open ? "upgrade" : null)}
                    documentType={documentType}
                    documentId={documentId}
                    definition={definition}
                />
            ) : null}
        </div>
    )
}

/**
 * 判断是否应展示审批决定入口。只读服务端动作，不按单据状态推断。
 */
export const hasDecisionEntry = (allowedActions: readonly string[]): boolean =>
    allowedActions.includes("APPROVE") || allowedActions.includes("REJECT")

/**
 * 判断审批任务是否误带通用工作项动作。
 */
export const hasForbiddenWorkItemActions = (
    allowedActions: readonly ApprovalAllowedAction[] | readonly string[],
): boolean =>
    allowedActions.some((action) => ["REASSIGN", "CLOSE"].includes(action))
