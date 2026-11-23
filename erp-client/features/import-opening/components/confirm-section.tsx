"use client"

import { z } from "zod"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    DetailWorkspaceSection,
    FormalActionConfirmDialog,
    FormalActionResult,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { WorkspaceTaskFooter } from "@/components/business/workspace-task-pane"
import { Button } from "@/components/ui/button"
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
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

const RETURN_REASON_OPTIONS = [
    { value: "DATA_MISMATCH", label: "预检数据与实际业务不一致" },
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
            <DialogContent
                size="md"
                closeButtonId="operations-import-batch-detail-confirm-return-dialog-close"
            >
                <DialogHeader>
                    <DialogTitle>
                        退回{CONFIRMATION_SCOPE_LABEL[confirmation.scope]}修复
                    </DialogTitle>
                    <DialogDescription>
                        退回修复后，需重新预检并核对确认。
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
                                id="operations-import-batch-detail-confirm-return-reason"
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
                                id="operations-import-batch-detail-confirm-return-comment"
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
                                    id="operations-import-batch-detail-confirm-return-cancel"
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
                                id="operations-import-batch-detail-confirm-return-submit"
                                label="确认退回修复"
                                pendingLabel="正在提交"
                                disabled={pending}
                                loading={pending}
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
    onTaskCompleted,
}: {
    batch: ImportBatchView
    workItemTypeMissing: boolean
    confirmBlocked: ImportBatchView["actionBlockers"]
    onTaskCompleted?: (workItemId: string) => void
}) {
    const {
        confirming,
        setConfirming,
        returning,
        setReturning,
        complete,
        isCompleting,
        error,
    } = useConfirmationActions(batch, onTaskCompleted)

    return (
        <DetailWorkspaceSection title="部门责任确认">
            {workItemTypeMissing ? (
                <FormalActionResult
                    status="blocked"
                    title="责任确认任务不完整"
                    description="当前批次缺少部门核对任务，暂无法确认或退回，请联系管理员重新生成。"
                />
            ) : null}

            {error ? (
                <BusinessFailureState title="责任确认未完成" error={error} />
            ) : null}

            <ul className="divide-y divide-border/60">
                {batch.confirmations.map((confirmation) => {
                    const task = confirmation.workItem
                    const actions = task?.allowedActions ?? []
                    const canConfirm =
                        !workItemTypeMissing &&
                        confirmation.result === "PENDING" &&
                        actions.includes("CONFIRM_SCOPE") &&
                        actions.includes("RETURN_FOR_FIX")
                    return (
                        <li
                            key={confirmation.confirmationId}
                            className={cn(
                                "space-y-3 py-4 text-body-compact first:pt-0 last:pb-0",
                                confirmation.focused &&
                                    "border-l-2 border-primary pl-4",
                            )}
                        >
                            <div className="flex flex-wrap items-start justify-between gap-2">
                                <div className="min-w-0 space-y-1">
                                    <div className="flex flex-wrap items-center gap-2">
                                        <h3 className="text-body-compact font-semibold text-foreground">
                                            {
                                                CONFIRMATION_SCOPE_LABEL[
                                                    confirmation.scope
                                                ]
                                            }
                                        </h3>
                                        <BusinessStatusBadge
                                            context="detail"
                                            label={
                                                confirmation.result ===
                                                "CONFIRMED"
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
                                                confirmation.result ===
                                                "CONFIRMED"
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
                                    <p className="text-xs text-muted-foreground">
                                        预检版本{" "}
                                        <span className="num">
                                            {confirmation.trialVersion}
                                        </span>
                                        {confirmation.focused
                                            ? " · 当前待处理入口"
                                            : confirmation.inViewerResponsibility
                                              ? " · 由本人负责"
                                              : " · 只读"}
                                    </p>
                                </div>

                                <WorkspaceTaskFooter
                                    inline={!confirmation.focused}
                                >
                                    <div className="flex flex-wrap gap-2">
                                        {canConfirm ? (
                                            <>
                                                <Button
                                                    id={`operations-import-batch-detail-confirm-${toAutomationIdSegment(confirmation.confirmationId)}-confirm`}
                                                    type="button"
                                                    size="sm"
                                                    disabled={isCompleting}
                                                    onClick={() =>
                                                        setConfirming(
                                                            confirmation,
                                                        )
                                                    }
                                                >
                                                    确认本范围
                                                </Button>
                                                <Button
                                                    id={`operations-import-batch-detail-confirm-${toAutomationIdSegment(confirmation.confirmationId)}-return`}
                                                    type="button"
                                                    size="sm"
                                                    variant="outline"
                                                    disabled={isCompleting}
                                                    onClick={() =>
                                                        setReturning(
                                                            confirmation,
                                                        )
                                                    }
                                                >
                                                    退回修复
                                                </Button>
                                            </>
                                        ) : null}
                                    </div>
                                </WorkspaceTaskFooter>
                            </div>

                            {confirmation.confirmedByLabel ? (
                                <p>
                                    确认人 {confirmation.confirmedByLabel}
                                    {confirmation.confirmedAt ? (
                                        <>
                                            {" · "}
                                            <span className="num">
                                                {formatDateTime(
                                                    confirmation.confirmedAt,
                                                    "dateStyle",
                                                    "passthrough",
                                                )}
                                            </span>
                                        </>
                                    ) : null}
                                </p>
                            ) : null}
                            {confirmation.comment ? (
                                <p className="text-muted-foreground">
                                    {confirmation.comment}
                                </p>
                            ) : null}
                            {!canConfirm ? (
                                <p className="text-xs text-muted-foreground">
                                    {workItemTypeMissing
                                        ? "当前确认任务不完整，入口已阻断"
                                        : confirmation.result !== "PENDING"
                                          ? "本范围已有核对结论或已失效"
                                          : (task?.actionBlockers[0] ??
                                            "当前范围不由本人处理")}
                                </p>
                            ) : null}
                        </li>
                    )
                })}
            </ul>

            {confirmBlocked.length > 0 ? (
                <ul className="space-y-1 text-body-compact text-muted-foreground">
                    {confirmBlocked.map((blocker) => (
                        <li key={`${blocker.action}-${blocker.code}`}>
                            {blocker.message}
                        </li>
                    ))}
                </ul>
            ) : null}

            {confirming ? (
                <FormalActionConfirmDialog
                    actionVariant="default"
                    id={`operations-import-batch-detail-confirm-${toAutomationIdSegment(confirming.confirmationId)}-dialog`}
                    open
                    onOpenChange={(open) => {
                        if (!open) setConfirming(undefined)
                    }}
                    title={`确认${CONFIRMATION_SCOPE_LABEL[confirming.scope]}`}
                    actionLabel="提交范围确认"
                    confirmLabel="确认本范围"
                    description="确认本范围的数据预检结果。"
                    fromStatus={{ label: "待确认", tone: "warning" }}
                    toStatus={{ label: "已确认", tone: "success" }}
                    effects={["记录责任范围确认结论", "完成当前处理任务"]}
                    irreversibleEffects={["预检结果变化后需要重新确认"]}
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
        </DetailWorkspaceSection>
    )
}
