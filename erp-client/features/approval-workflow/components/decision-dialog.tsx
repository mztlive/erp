"use client"

import * as React from "react"
import { RefreshCwIcon } from "lucide-react"

import { useAppForm } from "@/components/form"
import { detailWorkspaceStyles } from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { toast } from "@/components/ui/toast"

import { approvalConflictMessage, isApprovalConflict } from "../api"
import { displayReadableName } from "../display"
import {
    decisionIntentFingerprint,
    slotForIntent,
    type IdempotencySlot,
} from "../idempotency"
import { useSubmitDecisionMutation } from "../queries"
import { decisionFormSchema } from "../schema"
import {
    buildDecisionRequest,
    type ApprovalCommandView,
    type ApprovalDecision,
} from "../types"

const QUICK_REJECT_REASONS = [
    { key: "price", label: "金额或单价有误" },
    { key: "attachment", label: "缺少合同或凭证附件" },
    { key: "info", label: "开票或收款信息需核对" },
    { key: "delivery", label: "交期或数量需调整" },
] as const

/**
 * 审批决定弹窗。
 *
 * 只提交 work_item_id、APPROVE|REJECT、原因、expected_task_version 和幂等键。
 * 请求进行中禁用重复提交；409 刷新事实且不自动重放。
 */
export function DecisionDialog({
    open,
    onOpenChange,
    workItemId,
    expectedTaskVersion,
    defaultDecision,
    allowedActions,
    context,
    onRefreshTask,
    onApplied,
    id = "governance-approval-decision-dialog",
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    workItemId: string
    expectedTaskVersion: string
    defaultDecision: ApprovalDecision
    documentType?: string
    allowedActions: readonly string[]
    context?: Readonly<{
        documentLabel?: string
        amountLabel?: string
        currentNodeLabel?: string
        impactSummary?: string
    }>
    onRefreshTask?: () => void
    onApplied?: (view: ApprovalCommandView) => void
    id?: string
}) {
    const isReject = defaultDecision === "REJECT"
    const submitDecision = useSubmitDecisionMutation((view) => {
        setConflictMessage(null)
        const docLabel = context?.documentLabel?.trim()
        toast.add({
            title: isReject ? "审批已驳回" : "审批已通过",
            description: docLabel
                ? isReject
                    ? `${docLabel} 已退回原提交人处理。`
                    : `${docLabel} 已提交通过决定。`
                : isReject
                  ? "已退回原提交人处理。"
                  : "已提交通过决定。",
            type: isReject ? "info" : "success",
        })
        onOpenChange(false)
        onApplied?.(view)
    })
    const [slot, setSlot] = React.useState<IdempotencySlot | null>(null)
    const [conflictMessage, setConflictMessage] = React.useState<string | null>(
        null,
    )
    const canApprove = allowedActions.includes("APPROVE")
    const canReject = allowedActions.includes("REJECT")
    const amountLabel = context?.amountLabel?.trim()
    const nodeLabel = displayReadableName(context?.currentNodeLabel)

    const form = useAppForm({
        defaultValues: {
            decision: defaultDecision,
            reason: "",
        },
        validators: {
            onChange: decisionFormSchema,
        },
        onSubmit: async ({ value }) => {
            const fingerprint = decisionIntentFingerprint(
                value.decision,
                value.reason,
            )
            const nextSlot = slotForIntent(
                slot,
                "decision",
                workItemId,
                fingerprint,
            )
            setSlot(nextSlot)
            try {
                await submitDecision.mutateAsync(
                    buildDecisionRequest({
                        workItemId,
                        decision: value.decision,
                        reason: value.reason,
                        expectedTaskVersion,
                        idempotencyKey: nextSlot.key,
                    }),
                )
            } catch (error) {
                if (isApprovalConflict(error)) {
                    setConflictMessage(approvalConflictMessage(error))
                    return
                }
                throw error
            }
        },
    })

    React.useEffect(() => {
        if (!open) return
        form.reset({ decision: defaultDecision, reason: "" })
        setSlot(null)
        setConflictMessage(null)
    }, [defaultDecision, form, open])

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!next && submitDecision.isPending) return
                onOpenChange(next)
            }}
        >
            <DialogContent
                closeButtonId={`${id}-close`}
                showCloseButton={!submitDecision.isPending}
                className="sm:max-w-lg"
            >
                <DialogHeader>
                    <DialogTitle>
                        {isReject ? "确认驳回" : "确认通过"}
                    </DialogTitle>
                    <DialogDescription>
                        {isReject
                            ? "驳回后结束本次审批并退回原提交人修改，重新提交须从首节点重新审批。"
                            : "请核对以下审批信息后确认通过。"}
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    {context ? (
                        <dl className="grid grid-cols-[auto_minmax(0,1fr)] gap-x-4 gap-y-2 rounded-lg border border-border/60 bg-muted/30 p-3.5 text-body-compact">
                            <dt className="text-xs text-muted-foreground">
                                单据
                            </dt>
                            <dd className="min-w-0 break-words font-medium">
                                {context.documentLabel?.trim() || "当前任务"}
                            </dd>
                            {amountLabel ? (
                                <>
                                    <dt className="text-xs text-muted-foreground">
                                        金额
                                    </dt>
                                    <dd className="num font-medium">
                                        {amountLabel}
                                    </dd>
                                </>
                            ) : null}
                            {nodeLabel ? (
                                <>
                                    <dt className="text-xs text-muted-foreground">
                                        当前节点
                                    </dt>
                                    <dd className="min-w-0 break-words">
                                        {nodeLabel}
                                    </dd>
                                </>
                            ) : null}
                            {!isReject ? (
                                <>
                                    <dt className="text-xs text-muted-foreground">
                                        结果影响
                                    </dt>
                                    <dd className="min-w-0 break-words text-muted-foreground">
                                        {context.impactSummary?.trim() ||
                                            "通过后进入下一审批节点；如无后续节点，则完成本轮审批。"}
                                    </dd>
                                </>
                            ) : null}
                        </dl>
                    ) : null}
                    <form.AppField
                        name="decision"
                        children={(field) => (
                            <input
                                type="hidden"
                                value={field.state.value}
                                readOnly
                            />
                        )}
                    />
                    <form.AppField
                        name="reason"
                        children={(field) => (
                            <div className="space-y-2">
                                {isReject ? (
                                    <div
                                        role="group"
                                        aria-label="常用驳回原因"
                                        className="flex flex-wrap gap-1.5"
                                    >
                                        {QUICK_REJECT_REASONS.map((preset) => (
                                            <Button
                                                key={preset.key}
                                                id={`${id}-quick-reason-${preset.key}`}
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                className={
                                                    detailWorkspaceStyles.subtleButton
                                                }
                                                disabled={
                                                    submitDecision.isPending
                                                }
                                                onClick={() => {
                                                    const current = (
                                                        field.state.value ?? ""
                                                    ).trim()
                                                    if (!current) {
                                                        field.handleChange(
                                                            preset.label,
                                                        )
                                                    } else if (
                                                        !current.includes(
                                                            preset.label,
                                                        )
                                                    ) {
                                                        field.handleChange(
                                                            `${current}；${preset.label}`,
                                                        )
                                                    }
                                                }}
                                            >
                                                {preset.label}
                                            </Button>
                                        ))}
                                    </div>
                                ) : null}
                                <field.TextareaField
                                    id={`${id}-reason`}
                                    label={
                                        isReject
                                            ? "驳回原因"
                                            : "审批意见（可选）"
                                    }
                                    placeholder={
                                        isReject
                                            ? "请说明驳回原因及需修改的内容"
                                            : "可补充审批意见（选填）"
                                    }
                                    description="按 Ctrl / ⌘ + Enter 快速提交"
                                    required={isReject}
                                    disabled={submitDecision.isPending}
                                    onKeyDown={(event) => {
                                        if (
                                            (event.metaKey || event.ctrlKey) &&
                                            event.key === "Enter" &&
                                            !event.nativeEvent.isComposing
                                        ) {
                                            event.preventDefault()
                                            void form.handleSubmit()
                                        }
                                    }}
                                />
                            </div>
                        )}
                    />
                    {conflictMessage ? (
                        <Alert variant="destructive">
                            <AlertDescription className="flex flex-wrap items-center justify-between gap-2">
                                <span>{conflictMessage}</span>
                                {onRefreshTask ? (
                                    <Button
                                        id={`${id}-conflict-refresh`}
                                        type="button"
                                        variant="outline"
                                        size="xs"
                                        onClick={() => {
                                            onOpenChange(false)
                                            onRefreshTask()
                                        }}
                                    >
                                        <RefreshCwIcon aria-hidden="true" />
                                        刷新当前任务
                                    </Button>
                                ) : null}
                            </AlertDescription>
                        </Alert>
                    ) : null}
                    <DialogFooter>
                        <Button
                            id={`${id}-cancel`}
                            type="button"
                            variant="outline"
                            disabled={submitDecision.isPending}
                            onClick={() => onOpenChange(false)}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                loading={submitDecision.isPending}
                                id={`${id}-submit`}
                                variant={isReject ? "destructive" : "default"}
                                label={isReject ? "确认驳回" : "确认通过"}
                                disabled={
                                    submitDecision.isPending ||
                                    (!isReject && !canApprove) ||
                                    (isReject && !canReject)
                                }
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
