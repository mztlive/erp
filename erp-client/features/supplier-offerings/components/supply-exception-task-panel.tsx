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
        .min(1, "请填写处置证据引用")
        .max(256, "证据引用不能超过 256 个字符"),
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
                    "核对结论未提交，请刷新任务版本后重试",
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
                title="供应停止任务已阻止"
                description="当前责任、任务版本或供给对象未通过校验。本页不会提供供给写入动作。"
                error={error}
                onRetry={onRetry}
                action={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            type="button"
                            variant="outline"
                            onClick={onRetry}
                        >
                            重试校验
                        </Button>
                        {!embedded ? (
                            <Button
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
                                核对停供来源和已固定的暂停影响，登记处置证据后完成当前责任；不选定替代供给，不发起恢复发布。
                            </CardDescription>
                        </div>
                        {!embedded ? (
                            <Button
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
                        <AlertTitle>安全暂停不会随任务完成而解除</AlertTitle>
                        <AlertDescription>
                            本动作只确认人工核对已经完成。供应商供给、暂停修订和商品发布暂停状态全部保持不变。
                        </AlertDescription>
                    </Alert>

                    <dl className="grid gap-px overflow-hidden rounded-lg border bg-border sm:grid-cols-2 lg:grid-cols-4">
                        <div className="bg-card p-3">
                            <dt className="text-xs text-muted-foreground">
                                来源供给
                            </dt>
                            <dd className="mt-1 text-sm font-medium">
                                {sourceLabel(task, offering)}
                            </dd>
                        </div>
                        <div className="bg-card p-3">
                            <dt className="text-xs text-muted-foreground">
                                当前责任
                            </dt>
                            <dd className="mt-1 text-sm font-medium">
                                {responsibility(task)}
                            </dd>
                            <div className="mt-1 text-xs text-muted-foreground">
                                {task.ownerRoleLabel} ·{" "}
                                {task.ownerOrganization.displayName}
                            </div>
                        </div>
                        <div className="bg-card p-3">
                            <dt className="text-xs text-muted-foreground">
                                任务版本
                            </dt>
                            <dd className="num mt-1 break-all text-sm">
                                {task.taskVersion}
                            </dd>
                        </div>
                        <div className="bg-card p-3">
                            <dt className="text-xs text-muted-foreground">
                                来源版本
                            </dt>
                            <dd className="num mt-1 break-all text-sm">
                                {task.subjectVersion}
                            </dd>
                        </div>
                    </dl>

                    <div className="grid gap-3 lg:grid-cols-2">
                        <div className="rounded-lg border p-3">
                            <div className="text-xs font-medium text-muted-foreground">
                                已固定影响
                            </div>
                            <p className="mt-1 text-sm">{task.impactSummary}</p>
                            <p className="mt-2 text-xs text-muted-foreground">
                                原因：{task.reasonLabel}
                                。影响只使用任务记录，不在页面重新计算。
                            </p>
                            <p className="mt-2 text-xs text-muted-foreground">
                                {offering
                                    ? "当前列表行只用于识别供给对象，不覆盖任务冻结的来源版本。"
                                    : "当前分页未加载完整供给行；不在页面推断来源记录。"}
                            </p>
                        </div>
                        <div className="rounded-lg border p-3">
                            <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                                <ClipboardCheckIcon
                                    className="size-4"
                                    aria-hidden="true"
                                />
                                核对边界
                            </div>
                            <ul className="mt-2 list-disc space-y-1 pl-4 text-sm">
                                <li>核对停供来源和来源版本。</li>
                                <li>确认安全暂停影响已由系统固定。</li>
                                <li>登记可审计的处置证据与核对结论。</li>
                            </ul>
                        </div>
                    </div>

                    <div>
                        <div className="text-xs font-medium text-muted-foreground">
                            当前允许动作
                        </div>
                        <div className="mt-2 flex flex-wrap gap-2">
                            {task.allowedActions.length > 0 ? (
                                task.allowedActions.map((action) => (
                                    <Badge key={action} variant="outline">
                                        {ACTION_LABELS[action] ?? action}
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
                            转交只改变当前责任人；“确认已核对”完成任务，但不恢复发布。
                        </p>
                    </div>

                    {actionError ? (
                        <Alert variant="destructive">
                            <AlertTitle>核对结论未提交</AlertTitle>
                            <AlertDescription>{actionError}</AlertDescription>
                        </Alert>
                    ) : null}

                    <form
                        className="grid gap-4 rounded-lg border p-4"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <div>
                            <h3 className="text-sm font-semibold">完成核对</h3>
                            <p className="mt-1 text-xs text-muted-foreground">
                                证据引用和核对结论进入审计；提交后当前任务完成，安全暂停继续生效。
                            </p>
                        </div>
                        <form.AppField
                            name="evidenceReference"
                            children={(field) => (
                                <field.TextField
                                    label="处置证据引用"
                                    placeholder="例如：供应商停供函、替代采购事项或内部工单编号"
                                    required
                                />
                            )}
                        />
                        <form.AppField
                            name="comment"
                            children={(field) => (
                                <field.TextareaField
                                    label="核对结论"
                                    rows={4}
                                    placeholder="说明已核对的停供来源、受影响发布及后续安排"
                                    required
                                />
                            )}
                        />
                        <form.AppForm>
                            <form.SubmitButton
                                label="确认已核对"
                                pendingLabel="正在提交"
                                disabled={
                                    !canProcess || completeMutation.isPending
                                }
                            />
                        </form.AppForm>
                        {!canProcess ? (
                            <p className="text-xs text-destructive">
                                当前账号不是责任人，或任务已被阻断；请刷新或由有权人员处理。
                            </p>
                        ) : null}
                    </form>
                </CardContent>
            </Card>

            <FormalActionConfirmDialog
                open={confirmOpen}
                onOpenChange={setConfirmOpen}
                actionLabel="确认已核对并完成任务"
                title="确认完成供应停止核对"
                description="本动作只完成人工核对责任，不恢复供应商供给，也不恢复任何商品发布。"
                fromStatus={{ label: "待核对", tone: "warning" }}
                toStatus={{ label: "任务已完成", tone: "success" }}
                effects={[
                    "记录处置证据引用与核对结论",
                    "完成当前 W21 工作项",
                    "安全暂停与暂停修订继续生效",
                ]}
                irreversibleEffects={["核对结论进入审计记录"]}
                pending={completeMutation.isPending}
                onConfirm={completeTask}
            />
        </>
    )
}
