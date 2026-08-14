"use client"

import { z } from "zod"

import {
    BusinessFailureState,
    FormalActionConfirmDialog,
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
import { useExecutionActions } from "@/features/import-opening/hooks/use-execution-actions"
import type {
    BatchSection,
    ImportBatchView,
} from "@/features/import-opening/types"

const CANCEL_PENDING_REASON_OPTIONS = [
    { value: "OPERATOR_CANCELLED", label: "操作人终止本批未应用项" },
    { value: "DATA_SCOPE_CHANGED", label: "导入数据范围已变化" },
    { value: "BUSINESS_WINDOW_CLOSED", label: "业务执行窗口已关闭" },
    { value: "OTHER", label: "其它终止原因" },
] as const

const cancelPendingSchema = z.object({
    reasonCode: z.string().trim().min(1, "请选择取消原因"),
    comment: z.string().trim().max(1024, "操作说明不能超过 1024 个字符"),
})

/** 采集取消未应用项原因；已形成的业务事实不会被本动作回滚。 */
function CancelPendingDialog({
    pending,
    onSubmit,
    onCancel,
}: {
    pending: boolean
    onSubmit: (value: { reasonCode: string; comment: string }) => Promise<void>
    onCancel: () => void
}) {
    const form = useAppForm({
        defaultValues: { reasonCode: "OPERATOR_CANCELLED", comment: "" },
        validators: { onChange: cancelPendingSchema },
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
                    <DialogTitle>取消尚未应用项</DialogTitle>
                    <DialogDescription>
                        系统只停止本批尚未应用的项；已成功、已跳过及已形成的业务事实保持不变。
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
                                label="取消原因"
                                options={CANCEL_PENDING_REASON_OPTIONS}
                                allowClear={false}
                            />
                        )}
                    />
                    <form.AppField
                        name="comment"
                        children={(field) => (
                            <field.TextareaField
                                label="操作说明（可选）"
                                rows={4}
                                placeholder="补充取消范围或业务窗口信息"
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
                            返回
                        </DialogClose>
                        <form.AppForm>
                            <form.SubmitButton
                                label="确认取消未应用项"
                                pendingLabel="正在取消"
                                disabled={pending}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}

/** 独立提交应用、取消未应用项与重新准备失败项。 */
export function ImportExecutionActions({
    batch,
    onGoSection,
}: {
    batch: ImportBatchView
    onGoSection: (section: BatchSection) => void
}) {
    const {
        confirming,
        setConfirming,
        cancelling,
        setCancelling,
        canStart,
        canCancel,
        canRetry,
        visible,
        execute,
        isExecuting,
        error,
    } = useExecutionActions(batch, onGoSection)

    if (!visible) return null

    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-border/30">
                <CardTitle>导入执行</CardTitle>
                <CardDescription>
                    责任确认只形成待应用状态；只有“提交应用”会启动后台任务。取消和失败项重试均保留已形成的业务事实。
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-3 pt-4">
                {error ? (
                    <BusinessFailureState
                        title="导入执行命令未完成"
                        error={error}
                    />
                ) : null}
                <div className="flex flex-wrap gap-2">
                    {canStart ? (
                        <Button
                            type="button"
                            size="sm"
                            disabled={isExecuting}
                            onClick={() => setConfirming("START_APPLY")}
                        >
                            提交应用
                        </Button>
                    ) : null}
                    {canCancel ? (
                        <Button
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={isExecuting}
                            onClick={() => setCancelling(true)}
                        >
                            取消未应用项
                        </Button>
                    ) : null}
                    {canRetry ? (
                        <Button
                            type="button"
                            size="sm"
                            disabled={isExecuting}
                            onClick={() => setConfirming("RETRY_FAILED")}
                        >
                            重新准备失败项
                        </Button>
                    ) : null}
                </div>
            </CardContent>

            {confirming === "START_APPLY" ? (
                <FormalActionConfirmDialog
                    open
                    onOpenChange={(open) => {
                        if (!open) setConfirming(undefined)
                    }}
                    title="提交导入应用"
                    actionLabel="确认提交应用"
                    description="系统将再次核验批次和试算版本，随后把批次推进为导入中并启动后台任务。"
                    fromStatus={{ label: "待应用", tone: "success" }}
                    toStatus={{ label: "导入中", tone: "info" }}
                    effects={["启动关联后台任务", "只处理当前仍待应用的项"]}
                    irreversibleEffects={["已形成的业务对象不会由本批自动回滚"]}
                    pending={isExecuting}
                    onConfirm={() => execute("START_APPLY")}
                />
            ) : null}

            {confirming === "RETRY_FAILED" ? (
                <FormalActionConfirmDialog
                    open
                    onOpenChange={(open) => {
                        if (!open) setConfirming(undefined)
                    }}
                    title="重新准备失败项"
                    actionLabel="确认重新准备"
                    description="系统只把上一轮失败行重新准备为待应用，不会在本动作中启动后台任务。"
                    fromStatus={{ label: "失败结果", tone: "destructive" }}
                    toStatus={{ label: "待应用", tone: "success" }}
                    effects={[
                        "保留已成功与已跳过结果",
                        "仅清理失败行的上次失败诊断",
                    ]}
                    irreversibleEffects={["准备完成后仍需再次点击“提交应用”"]}
                    pending={isExecuting}
                    onConfirm={() => execute("RETRY_FAILED")}
                />
            ) : null}

            {cancelling ? (
                <CancelPendingDialog
                    pending={isExecuting}
                    onCancel={() => setCancelling(false)}
                    onSubmit={async ({ reasonCode, comment }) => {
                        await execute("CANCEL_PENDING", reasonCode, comment)
                    }}
                />
            ) : null}
        </Card>
    )
}
