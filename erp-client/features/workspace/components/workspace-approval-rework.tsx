"use client"

import { useMemo, useState } from "react"
import { useSelector } from "@tanstack/react-form"
import { z } from "zod"
import Link from "next/link"
import { useAppForm } from "@/components/form"
import {
    FormalActionConfirmDialog,
    WorkspaceTaskPane,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { useAbandonApprovalReworkMutation } from "@/features/work-items/queries"
import {
    FormalCommandKeyLedger,
    classifyFormalCommandError,
} from "@/lib/formal-command"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { buildHandlerHref } from "../lib/handler-destination"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceTaskIdentityHeader } from "./workspace-task-identity-header"

/** 驳回归提交人处理；打开原单修改重提，或明确放弃当前办理。 */
export function WorkspaceApprovalRework({
    item,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    onTaskCompleted?: (workItemId: string) => void
}) {
    const mutation = useAbandonApprovalReworkMutation()
    const ledger = useMemo(() => new FormalCommandKeyLedger(), [])
    const [abandonOpen, setAbandonOpen] = useState(false)
    const [error, setError] = useState<string | null>(null)
    const [unknown, setUnknown] = useState(false)
    const id = `workspace-rework-${toAutomationIdSegment(item.workItemId)}`
    const href = buildHandlerHref({
        ...item,
        approvalInstanceId: item.approvalProcessInstanceId,
    })
    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: {
            onChange: z.object({
                reason: z
                    .string()
                    .trim()
                    .min(1, "请填写放弃原因")
                    .max(512, "放弃原因最多 512 字"),
            }),
        },
        onSubmit: async ({ value }) => {
            if (mutation.isPending) return
            const reason = value.reason.trim()
            if (!reason) {
                setError("请填写放弃办理的原因")
                return
            }
            const command = ledger.acquire(
                "abandon",
                `rework:${item.workItemId}`,
                {
                    workItemId: item.workItemId,
                    expectedTaskVersion: item.taskVersion,
                    reason,
                },
            )
            if (!command) return
            setError(null)
            try {
                await mutation.mutateAsync({
                    ...command.payload,
                    idempotencyKey: command.idempotencyKey,
                })
                ledger.settle("abandon", "succeeded")
                setUnknown(false)
                setAbandonOpen(false)
                onTaskCompleted?.(item.workItemId)
            } catch (cause) {
                const result = classifyFormalCommandError(cause)
                ledger.settle("abandon", result)
                setUnknown(result === "unknown")
                setError(getErrorMessage(cause, "放弃办理未完成，请重试"))
                throw cause
            }
        },
    })
    const reason = useSelector(form.store, (state) =>
        state.values.reason.trim(),
    )
    return (
        <WorkspaceTaskPane
            header={
                <WorkspaceTaskIdentityHeader item={item} title="驳回待处理" />
            }
        >
            <div className="space-y-5 p-6">
                <Alert variant="destructive">
                    <AlertTitle>本次审批已驳回</AlertTitle>
                    <AlertDescription>
                        {item.approval?.lastRejectReason ||
                            item.reasonLabel ||
                            "请核对驳回意见后处理原单。"}
                    </AlertDescription>
                </Alert>
                <p className="text-sm text-muted-foreground">
                    本次审批已结束。可修改原单，也可按原内容填写说明后重新提交；重新提交将从第一个节点完整审批。
                </p>
                <div className="flex flex-wrap gap-3">
                    {href && item.allowedActions.includes("PROCESS") ? (
                        <Button
                            disabled={mutation.isPending || unknown}
                            render={<Link id={`${id}-edit`} href={href} />}
                        >
                            修改或重新提交
                        </Button>
                    ) : null}
                    {item.status === "OPEN" &&
                    item.allowedActions.includes("ABANDON_REWORK") ? (
                        <Button
                            id={`${id}-abandon`}
                            variant="outline"
                            disabled={mutation.isPending || unknown}
                            onClick={() => setAbandonOpen(!abandonOpen)}
                        >
                            放弃本次办理
                        </Button>
                    ) : null}
                </div>
                <FormalActionConfirmDialog
                    idPrefix={`${id}-confirm-abandon`}
                    open={abandonOpen}
                    onOpenChange={(open) => {
                        if (!mutation.isPending && !unknown)
                            setAbandonOpen(open)
                    }}
                    title="放弃本次办理"
                    actionLabel="放弃本次办理"
                    confirmLabel={unknown ? "重试本次操作" : "确认放弃"}
                    actionVariant="destructive"
                    fromStatus={{ label: "驳回待处理", tone: "warning" }}
                    toStatus={{ label: "已关闭", tone: "neutral" }}
                    description="放弃后关闭此待办。原单据仍为已驳回，内容和审批记录保留，后续可从单据详情再次办理。"
                    pending={mutation.isPending}
                    confirmDisabled={!reason || reason.length > 512}
                    onConfirm={() => form.handleSubmit()}
                    formContent={
                        <div className="space-y-4">
                            <form.AppField name="reason">
                                {(field) => (
                                    <field.TextareaField
                                        id={`${id}-reason`}
                                        label="放弃原因"
                                        required
                                        maxLength={512}
                                        disabled={mutation.isPending || unknown}
                                    />
                                )}
                            </form.AppField>
                            {error ? (
                                <p
                                    role="alert"
                                    className="text-sm text-destructive"
                                >
                                    {unknown
                                        ? "处理结果待确认，请保留输入并重试同一次操作。"
                                        : error}
                                </p>
                            ) : null}
                        </div>
                    }
                />
            </div>
        </WorkspaceTaskPane>
    )
}
