"use client"

import * as React from "react"
import Link from "next/link"
import { ClipboardCheckIcon, ShieldAlertIcon } from "lucide-react"
import { z } from "zod"

import {
    BusinessFailureState,
    FormalActionConfirmDialog,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { WorkspaceTaskFooter } from "@/components/business/workspace-task-pane"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type {
    SupplierOfferingView,
    SupplierSupplyExceptionWorkItem,
} from "@/features/supplier-offerings/types"
import { useCompleteSupplierSupplyExceptionTaskMutation } from "@/features/supplier-offerings/hooks/queries"
import { supplyExceptionCompletionIntent } from "@/features/supplier-offerings/lib/supply-exception-command"
import type { WorkItemAllowedAction } from "@/features/work-items/types"
import { getErrorMessage } from "@/lib/api/errors"
import {
    classifyFormalCommandError,
    FormalCommandKeyLedger,
} from "@/lib/formal-command"

const ACTION_LABELS: Readonly<Partial<Record<WorkItemAllowedAction, string>>> =
    {
        VIEW: "可查看",
        PROCESS: "可核对",
        REASSIGN: "可转交",
    }

function responsibility(task: SupplierSupplyExceptionWorkItem): string {
    if (task.ownerUser) return task.ownerUser.displayName
    return "责任人信息不可用"
}

function sourceLabel(
    task: SupplierSupplyExceptionWorkItem,
    offering?: SupplierOfferingView,
): string {
    if (!offering) return task.businessObjectLabel
    return [
        offering.supplier_name ?? offering.supplier_no,
        offering.sku_name,
        offering.supplier_sku_code,
        offering.sku_no,
    ]
        .filter(Boolean)
        .join(" · ")
}

const completionSchema = z.object({
    evidenceReference: z
        .string()
        .trim()
        .min(1, "请填写核对凭据编号或链接")
        .max(256, "核对凭据不能超过 256 个字符"),
    comment: z
        .string()
        .trim()
        .min(3, "请填写至少 3 个字的核对结论")
        .max(500, "核对结论不能超过 500 个字符"),
})

/** W22 安全暂停在 W21 的强类型核对面；完成任务不恢复供给或商品发布。 */
export function SupplyExceptionTaskPanel({
    workItemId,
    task,
    offering,
    isPending,
    error,
    onRetry,
    embedded = false,
    onTaskCompleted,
}: {
    workItemId: string
    task?: SupplierSupplyExceptionWorkItem
    offering?: SupplierOfferingView
    isPending: boolean
    error?: Error | null
    onRetry: () => void
    embedded?: boolean
    onTaskCompleted?: (workItemId: string) => void
}) {
    const completeMutation = useCompleteSupplierSupplyExceptionTaskMutation()
    const commandLedger = React.useRef(new FormalCommandKeyLedger()).current
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [actionError, setActionError] = React.useState<string | null>(null)
    const form = useAppForm({
        defaultValues: { evidenceReference: "", comment: "" },
        validators: { onChange: completionSchema },
        onSubmit: async () => setConfirmOpen(true),
    })
    const returnHref = `/workspace?currentWorkItemId=${encodeURIComponent(workItemId)}`
    const canProcess = Boolean(
        task?.allowedActions.includes("PROCESS") && !task.processingBlocker,
    )

    async function completeTask() {
        if (!task || !canProcess) return
        setActionError(null)
        const intent = supplyExceptionCompletionIntent({
            offeringId: task.businessObjectId,
            workItemId: task.workItemId,
            expectedTaskVersion: task.taskVersion,
            expectedSubjectVersion: task.subjectVersion,
            evidenceReference: form.getFieldValue("evidenceReference"),
            comment: form.getFieldValue("comment"),
        })
        const command = commandLedger.acquire(
            intent.slot,
            intent.prefix,
            intent.payload,
        )
        try {
            const result = await completeMutation.mutateAsync({
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            })
            commandLedger.settle(intent.slot, "succeeded")
            setConfirmOpen(false)
            onTaskCompleted?.(result.work_item_id)
        } catch (submitError) {
            commandLedger.settle(
                intent.slot,
                classifyFormalCommandError(submitError),
            )
            setActionError(
                getErrorMessage(
                    submitError,
                    "核对结论未提交，请刷新待办后重试",
                ),
            )
        }
    }

    if (isPending) {
        return (
            <div
                className="h-44 animate-pulse rounded-xl border bg-muted/50"
                aria-label="正在核对供应停止任务"
            />
        )
    }

    if (error || !task) {
        return (
            <BusinessFailureState
                title="暂时无法办理停供核对"
                description="请刷新待办重试；仍无法办理时，联系管理员核对当前负责人和关联供应商品。"
                error={error}
                onRetry={onRetry}
                action={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            id="supplier-offerings-exception-retry"
                            type="button"
                            variant="outline"
                            onClick={onRetry}
                        >
                            刷新重试
                        </Button>
                        {!embedded ? (
                            <Button
                                id="supplier-offerings-exception-error-back-workspace"
                                type="button"
                                variant="outline"
                                render={<Link href={returnHref} />}
                            >
                                返回待办队列
                            </Button>
                        ) : null}
                    </div>
                }
            />
        )
    }

    return (
        <>
            <Card className="border-destructive/40">
                <CardHeader>
                    <div className="flex flex-wrap items-start justify-between gap-3">
                        <div>
                            <CardTitle className="flex items-center gap-2">
                                <ShieldAlertIcon
                                    className="size-4 text-destructive"
                                    aria-hidden="true"
                                />
                                供应停止核对
                                <Badge variant="destructive">待核对</Badge>
                            </CardTitle>
                            <CardDescription className="mt-1">
                                请核实哪项供应已停止、影响哪些商品，并填写核对凭据和处理结论。
                            </CardDescription>
                        </div>
                        {!embedded ? (
                            <Button
                                id="supplier-offerings-exception-header-back-workspace"
                                type="button"
                                size="sm"
                                variant="outline"
                                render={<Link href={returnHref} />}
                            >
                                返回待办队列
                            </Button>
                        ) : null}
                    </div>
                </CardHeader>
                <CardContent className="space-y-4">
                    <Alert variant="destructive">
                        <ShieldAlertIcon aria-hidden="true" />
                        <AlertTitle>
                            核对完成后，供应和商品发布仍保持暂停
                        </AlertTitle>
                        <AlertDescription>
                            提交后结束本条核对待办。如需替换供应或恢复商品发布，须另行办理。
                        </AlertDescription>
                    </Alert>

                    <dl className="grid gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-2">
                        <div className="bg-card p-3">
                            <dt className="text-xs text-muted-foreground">
                                受影响的供应商品
                            </dt>
                            <dd className="mt-1 text-sm font-medium">
                                {sourceLabel(task, offering)}
                            </dd>
                        </div>
                        <div className="bg-card p-3">
                            <dt className="text-xs text-muted-foreground">
                                当前负责人
                            </dt>
                            <dd className="mt-1 text-sm font-medium">
                                {responsibility(task)}
                            </dd>
                            <div className="mt-1 text-xs text-muted-foreground">
                                {task.ownerRoleLabel} ·{" "}
                                {task.ownerOrganization.displayName}
                            </div>
                        </div>
                    </dl>

                    <div className="grid gap-3 lg:grid-cols-2">
                        <div className="rounded-lg border p-3">
                            <div className="text-xs font-medium text-muted-foreground">
                                本次停供影响
                            </div>
                            <p className="mt-1 text-sm">{task.impactSummary}</p>
                            <p className="mt-2 text-xs text-muted-foreground">
                                原因：{task.reasonLabel}。
                            </p>
                        </div>
                        <div className="rounded-lg border p-3">
                            <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                                <ClipboardCheckIcon
                                    className="size-4"
                                    aria-hidden="true"
                                />
                                当前需要你核对
                            </div>
                            <ul className="mt-2 list-disc space-y-1 pl-4 text-sm">
                                <li>核实供应商和停止供应的商品。</li>
                                <li>核对本次记录的停供原因和影响范围。</li>
                                <li>填写核对凭据和结论，再提交完成待办。</li>
                            </ul>
                        </div>
                    </div>

                    <div>
                        <div className="text-xs font-medium text-muted-foreground">
                            你可以进行的操作
                        </div>
                        <div className="mt-2 flex flex-wrap gap-2">
                            {task.allowedActions.length > 0 ? (
                                task.allowedActions.map((action) => (
                                    <Badge key={action} variant="outline">
                                        {ACTION_LABELS[action] ?? "其他操作"}
                                    </Badge>
                                ))
                            ) : (
                                <Badge variant="secondary">当前只读</Badge>
                            )}
                        </div>
                        {task.processingBlocker ? (
                            <p className="mt-2 text-xs text-destructive">
                                {task.processingBlocker.message}
                            </p>
                        ) : null}
                        {task.actionBlockers.length > 0 ? (
                            <ul className="mt-2 list-disc space-y-1 pl-4 text-xs text-muted-foreground">
                                {task.actionBlockers.map((blocker) => (
                                    <li key={blocker}>{blocker}</li>
                                ))}
                            </ul>
                        ) : null}
                        <p className="mt-2 text-xs text-muted-foreground">
                            转交只改变当前负责人人；“确认已核对”完成任务，但不恢复发布。
                        </p>
                    </div>

                    {actionError ? (
                        <Alert variant="destructive">
                            <AlertTitle>核对结论未提交</AlertTitle>
                            <AlertDescription>{actionError}</AlertDescription>
                        </Alert>
                    ) : null}

                    <form
                        id="supplier-offerings-exception-form"
                        className="grid gap-4 rounded-lg border p-4"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <div>
                            <h3 className="text-sm font-semibold">完成核对</h3>
                            <p className="mt-1 text-xs text-muted-foreground">
                                填写可查验的凭据编号或链接及核对结论。提交后本条待办结束，供应和商品发布继续暂停。
                            </p>
                        </div>
                        <form.AppField
                            name="evidenceReference"
                            children={(field) => (
                                <field.TextField
                                    id="supplier-offerings-exception-evidence"
                                    label="核对凭据编号或链接"
                                    placeholder="例如：供应商停供函、替代采购事项或内部工单编号"
                                    required
                                />
                            )}
                        />
                        <form.AppField
                            name="comment"
                            children={(field) => (
                                <field.TextareaField
                                    id="supplier-offerings-exception-comment"
                                    label="核对结论"
                                    rows={4}
                                    placeholder="说明已核对的停供来源、受影响发布及后续安排"
                                    required
                                />
                            )}
                        />
                        <WorkspaceTaskFooter>
                            <form.AppForm>
                                <form.SubmitButton
                                    form="supplier-offerings-exception-form"
                                    id="supplier-offerings-exception-submit"
                                    label="确认已核对"
                                    pendingLabel="正在提交"
                                    loading={completeMutation.isPending}
                                    disabled={
                                        !canProcess ||
                                        completeMutation.isPending
                                    }
                                />
                            </form.AppForm>
                        </WorkspaceTaskFooter>
                        {!canProcess ? (
                            <p className="text-xs text-destructive">
                                当前账号不是责任人，或任务已被阻断；请刷新或由有权人员处理。
                            </p>
                        ) : null}
                    </form>
                </CardContent>
            </Card>

            <FormalActionConfirmDialog
                actionVariant="default"
                idPrefix="supplier-offerings-exception-complete-confirm"
                open={confirmOpen}
                onOpenChange={setConfirmOpen}
                actionLabel="确认已核对并完成任务"
                title="确认完成供应停止核对"
                description="完成核对后，供给与商品发布仍保持暂停。"
                fromStatus={{ label: "待核对", tone: "warning" }}
                toStatus={{ label: "任务已完成", tone: "success" }}
                effects={["记录核对结论与处置凭证"]}
                irreversibleEffects={["核对结论进入审计记录"]}
                pending={completeMutation.isPending}
                onConfirm={completeTask}
            />
        </>
    )
}
