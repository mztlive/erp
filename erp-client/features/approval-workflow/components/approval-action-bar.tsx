"use client"

import * as React from "react"

import {
    ApprovalActionBar as SharedApprovalActionBar,
    type ApprovalDecisionContext,
} from "@/components/business/task/approval-action-bar"
import { useRouter } from "@/lib/navigation"

import { approvalConflictMessage, isApprovalConflict } from "../api"
import { useSubmitDecisionMutation } from "../queries"
import {
    buildDecisionRequest,
    type ApprovalCommandView,
    type ApprovalDefinitionBinding,
    type ApprovalRuntimeInstance,
    type RecoveryOption,
} from "../types"
import { CancelApprovalDialog } from "./cancel-approval-dialog"
import { ResumeApproverDialog } from "./resume-approver-dialog"
import { UpgradeBindingDialog } from "./upgrade-binding-dialog"

type DomainDialogKind =
    | "resume"
    | "cancel-blocked"
    | "withdraw"
    | "upgrade"
    | null

export type ApprovalActionBarProps = Readonly<{
    allowedActions: readonly string[]
    recoveryOptions?: readonly RecoveryOption[]
    workItemId?: string
    expectedTaskVersion?: string
    instance?: ApprovalRuntimeInstance
    definition?: ApprovalDefinitionBinding
    documentType?: string
    documentId?: string
    documentHref?: string | null
    documentVersion?: number
    editDocumentHref?: string
    afterCancelStatusLabel?: string
    emergencyWithdraw?: boolean
    canReadSensitive?: boolean
    /** 由对象中心页头承接的动作，审批区内不再重复渲染。 */
    hiddenActions?: readonly string[]
    /** 工作台提供的审批事实；高危核对弹窗只展示，不参与命令构造。 */
    decisionContext?: ApprovalDecisionContext
    /** ADR-1 高危单据标记：冲正单自动启用；负毛利、超信用/预算、大额或阻断告警由调用方传入。 */
    highRisk?: boolean
    /** 高危核对原因列表（非空时自动触发 ADR-1 单层高危确认弹窗）。 */
    highRiskReasons?: readonly string[]
    /** 自定义高危核对摘要区块，展示在单层 FormalActionConfirmDialog 顶部。 */
    highRiskSummary?: React.ReactNode
    /** 结合当前单据检测出的具体异常，前置展示的驳回快捷短语。 */
    dynamicRejectPhrases?: readonly string[]
    onDecisionApplied?: (view: ApprovalCommandView) => void
    onRefreshTask?: () => void
    /** 展示承载形态：`"sheet"` 用于 768px 轻作业抽屉底栏，约束内联输入框与展开意见区宽度避免溢出。 */
    presentation?: "default" | "workspace" | "sheet"
    id?: string
}>

/**
 * 审批流业务域适配壳：基于 `@/components/business/task/approval-action-bar` 绑定
 * `useSubmitDecisionMutation` 及受阻恢复 / 撤回审批 / 升级流程定义弹窗。
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
    documentVersion,
    editDocumentHref,
    afterCancelStatusLabel = "未提交",
    emergencyWithdraw = false,
    canReadSensitive = true,
    hiddenActions = [],
    decisionContext,
    highRisk = false,
    highRiskReasons,
    highRiskSummary,
    dynamicRejectPhrases,
    onDecisionApplied,
    onRefreshTask,
    presentation = "default",
    id = "governance-approval-action-bar",
}: ApprovalActionBarProps) {
    const router = useRouter()
    const [dialog, setDialog] = React.useState<DomainDialogKind>(null)
    const submitDecision = useSubmitDecisionMutation()

    React.useEffect(() => {
        setDialog(null)
    }, [workItemId])

    const hidden = new Set(hiddenActions)
    const actions = new Set(
        allowedActions.filter((action) => !hidden.has(action)),
    )
    const recoveries = new Set(recoveryOptions)
    const showResume =
        recoveries.has("RESUME_CURRENT_APPROVER") &&
        actions.has("RESUME_CURRENT_APPROVER")
    const showCancelBlocked =
        recoveries.has("CANCEL_BLOCKED") &&
        actions.has("CANCEL_BLOCKED_APPROVAL")
    const showWithdraw = actions.has("CANCEL") || actions.has("CANCEL_APPROVAL")
    const reviseRejected = showWithdraw && Boolean(instance?.latestRejection)
    const showUpgrade = actions.has("UPGRADE_BINDING") && Boolean(definition)

    return (
        <SharedApprovalActionBar<ApprovalCommandView>
            allowedActions={allowedActions}
            recoveryOptions={recoveryOptions}
            workItemId={workItemId}
            expectedTaskVersion={expectedTaskVersion}
            documentType={documentType}
            documentHref={documentHref}
            reviseRejected={reviseRejected}
            emergencyWithdraw={emergencyWithdraw}
            withdrawDisabled={!documentVersion}
            withdrawDisabledTitle={
                !documentVersion ? "请打开单据详情后修改或撤回" : undefined
            }
            canReadSensitive={canReadSensitive}
            hiddenActions={hiddenActions}
            decisionContext={decisionContext}
            highRisk={highRisk}
            highRiskReasons={highRiskReasons}
            highRiskSummary={highRiskSummary}
            dynamicRejectPhrases={dynamicRejectPhrases}
            onSubmitDecision={(payload) =>
                submitDecision.mutateAsync(buildDecisionRequest(payload))
            }
            onDecisionApplied={onDecisionApplied}
            submitting={submitDecision.isPending}
            isConflictError={isApprovalConflict}
            getConflictMessage={approvalConflictMessage}
            onRefreshTask={onRefreshTask}
            onResumeApprover={() => setDialog("resume")}
            onCancelBlocked={() => setDialog("cancel-blocked")}
            onWithdraw={() => setDialog("withdraw")}
            onUpgradeBinding={
                definition ? () => setDialog("upgrade") : undefined
            }
            externalDialogOpen={dialog !== null}
            presentation={presentation}
            id={id}
            dialogs={
                <>
                    {instance ? (
                        <ResumeApproverDialog
                            id={`${id}-resume-dialog`}
                            open={showResume && dialog === "resume"}
                            onOpenChange={(open) =>
                                setDialog(open ? "resume" : null)
                            }
                            instanceId={instance.id}
                            expectedInstanceVersion={
                                instance.instanceVersion ?? ""
                            }
                            expectedExecutionVersion={
                                instance.executionVersion ?? ""
                            }
                            expectedAssignmentVersion={
                                instance.assignmentVersion ?? ""
                            }
                            onApplied={onDecisionApplied}
                        />
                    ) : null}
                    {instance ? (
                        <CancelApprovalDialog
                            id={`${id}-cancel-dialog`}
                            open={
                                (showWithdraw && dialog === "withdraw") ||
                                (showCancelBlocked &&
                                    dialog === "cancel-blocked")
                            }
                            onOpenChange={(open) =>
                                setDialog(open ? dialog : null)
                            }
                            mode={
                                dialog === "cancel-blocked"
                                    ? "cancel-blocked"
                                    : "withdraw"
                            }
                            instanceId={instance.id}
                            documentType={documentType}
                            documentId={documentId}
                            documentVersion={documentVersion}
                            currentRoundNo={instance.currentRoundNo}
                            reviseRejected={
                                reviseRejected && dialog === "withdraw"
                            }
                            currentNodeName={instance.currentNodeName}
                            afterStatusLabel={afterCancelStatusLabel}
                            expectedInstanceVersion={
                                instance.instanceVersion ?? ""
                            }
                            expectedExecutionVersion={
                                instance.executionVersion ?? ""
                            }
                            expectedTaskVersion={expectedTaskVersion}
                            emergency={
                                emergencyWithdraw && dialog === "withdraw"
                            }
                            onApplied={(view) => {
                                onDecisionApplied?.(view)
                                if (
                                    reviseRejected &&
                                    editDocumentHref &&
                                    dialog === "withdraw"
                                ) {
                                    router.push(editDocumentHref)
                                }
                            }}
                        />
                    ) : null}
                    {definition && documentType && documentId ? (
                        <UpgradeBindingDialog
                            id={`${id}-upgrade-dialog`}
                            open={showUpgrade && dialog === "upgrade"}
                            onOpenChange={(open) =>
                                setDialog(open ? "upgrade" : null)
                            }
                            documentType={documentType}
                            documentId={documentId}
                            definition={definition}
                        />
                    ) : null}
                </>
            }
        />
    )
}
