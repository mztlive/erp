"use client"

import { z } from "zod"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    FormalActionConfirmDialog,
    FormalActionResult,
    surfacePanelClassName,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { useConfirmationActions } from "@/features/import-opening/hooks/use-confirmation-actions"
import type {
    ImportBatchView,
    ImportConfirmationView,
} from "@/features/import-opening/types"
import { CONFIRMATION_SCOPE_LABEL } from "@/features/import-opening/types"
import { formatDateTime } from "@/lib/datetime"

const RETURN_REASON_OPTIONS = [
    { value: "DATA_MISMATCH", label: "试算数据与业务事实不一致" },
    { value: "RULE_MISMATCH", label: "导入口径或规则不一致" },
    { value: "MISSING_EVIDENCE", label: "缺少必要核对依据" },
    { value: "OTHER", label: "其它需修复问题" },
] as const

const returnForFixSchema = z.object({
    reasonCode: z.string().trim().min(1, "请选择退回原因"),
    comment: z.string().trim().min(3, "请填写至少 3 个字的修复说明"),
})

/** 采集退回原因；提交失败时保留输入并保持对话框打开。 */
function ReturnForFixDialog({
    confirmation,
    pending,
    onSubmit,
    onCancel,
}: {
    confirmation: ImportConfirmationView
    pending: boolean
    onSubmit: (value: { reasonCode: string; comment: string }) => Promise<void>
    onCancel: () => void
}) {
    const form = useAppForm({
        defaultValues: { reasonCode: "DATA_MISMATCH", comment: "" },
        validators: { onChange: returnForFixSchema },
        onSubmit: async ({ value }) => onSubmit(value),
    })
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !pending) onCancel()
            }}
        >
            <DialogContent className="sm:max-w-lg">
                <DialogHeader>
                    <DialogTitle>
                        退回{CONFIRMATION_SCOPE_LABEL[confirmation.scope]}修复
                    </DialogTitle>
                    <DialogDescription>
                        本次试算会形成已退回结论并完成当前任务；修复并重新试算后，系统才会创建新任务。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField
                        name="reasonCode"
                        children={(field) => (
                            <field.SelectField
                                label="退回原因"
                                options={RETURN_REASON_OPTIONS}
                                allowClear={false}
                                required
                            />
                        )}
                    />
                    <form.AppField
                        name="comment"
                        children={(field) => (
                            <field.TextareaField
                                label="修复说明"
                                rows={4}
                                placeholder="说明需要修复的数据、口径或依据"
                                required
                            />
                        )}
                    />
                    <DialogFooter>
                        <DialogClose
                            render={
                                <Button
                                    type="button"
                                    variant="outline"
                                    disabled={pending}
                                />
                            }
                        >
                            返回核对
                        </DialogClose>
                        <form.AppForm>
                            <form.SubmitButton
                                label="确认退回修复"
                                pendingLabel="正在提交"
                                disabled={pending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}

export function ConfirmSection({
    batch,
    workItemTypeMissing,
    confirmBlocked,
}: {
    batch: ImportBatchView
    workItemTypeMissing: boolean
    confirmBlocked: ImportBatchView["actionBlockers"]
}) {
    const {
        confirming,
        setConfirming,
        returning,
        setReturning,
        complete,
        isCompleting,
        error,
    } = useConfirmationActions(batch)

    return (
        <div className="space-y-4">
            {workItemTypeMissing ? (
                <FormalActionResult
                    status="blocked"
                    title="责任确认任务不完整"
                    description="当前试算缺少已登记的责任确认任务，不能提交确认或退回。请联系管理员重新生成确认任务。"
                />
            ) : null}

            {error ? (
                <BusinessFailureState title="责任确认未完成" error={error} />
            ) : null}

            <div className="grid gap-3 md:grid-cols-2">
                {batch.confirmations.map((confirmation) => {
                    const task = confirmation.workItem
                    const actions = task?.allowedActions ?? []
                    const canConfirm =
                        !workItemTypeMissing &&
                        confirmation.result === "PENDING" &&
                        actions.includes("CONFIRM_SCOPE") &&
                        actions.includes("RETURN_FOR_FIX")
                    return (
                        <Card
                            key={confirmation.confirmationId}
                            size="sm"
                            className={`${surfacePanelClassName} ${confirmation.focused ? "ring-2 ring-primary/40" : ""}`}
                        >
                            <CardHeader className="border-b border-grid">
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <CardTitle className="text-base">
                                        {
                                            CONFIRMATION_SCOPE_LABEL[
                                                confirmation.scope
                                            ]
                                        }
                                    </CardTitle>
                                    <BusinessStatusBadge
                                        context="detail"
                                        label={
                                            confirmation.result === "CONFIRMED"
                                                ? "已确认"
                                                : confirmation.result ===
                                                    "REJECTED"
                                                  ? "已退回"
                                                  : confirmation.result ===
                                                      "INVALIDATED"
                                                    ? "已失效"
                                                    : "待确认"
                                        }
                                        tone={
                                            confirmation.result === "CONFIRMED"
                                                ? "success"
                                                : confirmation.result ===
                                                        "REJECTED" ||
                                                    confirmation.result ===
                                                        "INVALIDATED"
                                                  ? "destructive"
                                                  : "warning"
                                        }
                                    />
                                </div>
                                <CardDescription>
                                    试算版本{" "}
                                    <span className="num font-mono">
                                        {confirmation.trialVersion}
                                    </span>
                                    {confirmation.focused
                                        ? " · 当前待处理入口"
                                        : confirmation.inViewerResponsibility
                                          ? " · 由本人负责"
                                          : " · 只读"}
                                </CardDescription>
                            </CardHeader>
                            <CardContent className="space-y-3 pt-4 text-sm">
                                {confirmation.confirmedByLabel ? (
                                    <p>
                                        确认人 {confirmation.confirmedByLabel}
                                        {confirmation.confirmedAt
                                            ? ` · ${formatDateTime(confirmation.confirmedAt, "dateStyle", "passthrough")}`
                                            : ""}
                                    </p>
                                ) : null}
                                {confirmation.comment ? (
                                    <p className="text-muted-foreground">
                                        {confirmation.comment}
                                    </p>
                                ) : null}
                                <div className="flex flex-wrap gap-2">
                                    {canConfirm ? (
                                        <>
                                            <Button
                                                type="button"
                                                size="sm"
                                                disabled={isCompleting}
                                                onClick={() =>
                                                    setConfirming(confirmation)
                                                }
                                            >
                                                确认本范围
                                            </Button>
                                            <Button
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                disabled={isCompleting}
                                                onClick={() =>
                                                    setReturning(confirmation)
                                                }
                                            >
                                                退回修复
                                            </Button>
                                        </>
                                    ) : null}
                                </div>
                                {!canConfirm ? (
                                    <p className="text-xs text-muted-foreground">
                                        {workItemTypeMissing
                                            ? "当前确认任务不完整，入口已阻断"
                                            : confirmation.result !== "PENDING"
                                              ? "本范围已有正式结论或已失效"
                                              : (task?.actionBlockers[0] ??
                                                "当前范围不由本人处理")}
                                    </p>
                                ) : null}
                            </CardContent>
                        </Card>
                    )
                })}
            </div>

            {confirmBlocked.length > 0 ? (
                <ul className="space-y-1 text-sm text-muted-foreground">
                    {confirmBlocked.map((blocker) => (
                        <li key={`${blocker.action}-${blocker.code}`}>
                            {blocker.message}
                        </li>
                    ))}
                </ul>
            ) : null}

            {confirming ? (
                <FormalActionConfirmDialog
                    open
                    onOpenChange={(open) => {
                        if (!open) setConfirming(undefined)
                    }}
                    title={`确认${CONFIRMATION_SCOPE_LABEL[confirming.scope]}`}
                    actionLabel="确认本范围"
                    description="系统将记录本范围正式确认事实，并在同一操作中完成当前任务。"
                    fromStatus={{ label: "待确认", tone: "warning" }}
                    toStatus={{ label: "已确认", tone: "success" }}
                    effects={["记录责任范围确认结论", "完成当前处理任务"]}
                    irreversibleEffects={[
                        "结论写入审计，试算变化后由新任务重新确认",
                    ]}
                    pending={isCompleting}
                    onConfirm={() => complete(confirming, "CONFIRM_SCOPE")}
                />
            ) : null}

            {returning ? (
                <ReturnForFixDialog
                    confirmation={returning}
                    pending={isCompleting}
                    onCancel={() => setReturning(undefined)}
                    onSubmit={async ({ reasonCode, comment }) => {
                        await complete(
                            returning,
                            "RETURN_FOR_FIX",
                            reasonCode,
                            comment,
                        )
                        setReturning(undefined)
                    }}
                />
            ) : null}
        </div>
    )
}
