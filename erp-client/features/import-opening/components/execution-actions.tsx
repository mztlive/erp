"use client"

import { z } from "zod"

import {
    BusinessFailureState,
    FormalActionConfirmDialog,
    DetailWorkspaceSection,
} from "@/components/business"
import { useAppForm } from "@/components/form"
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
import { useExecutionActions } from "@/features/import-opening/hooks/use-execution-actions"
import type {
    BatchSection,
    ImportBatchView,
} from "@/features/import-opening/types"

const CANCEL_PENDING_REASON_OPTIONS = [
    { value: "OPERATOR_CANCELLED", label: "操作人终止本批未导入数据" },
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
            <DialogContent
                className="sm:max-w-lg"
                closeButtonId="operations-import-batch-detail-execution-cancel-dialog-close"
            >
                <DialogHeader>
                    <DialogTitle>取消未导入数据</DialogTitle>
                    <DialogDescription>
                        仅取消本批尚未导入的数据，已成功和已跳过的记录保留。
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
                                id="operations-import-batch-detail-execution-cancel-reason"
                                label="取消原因"
                                options={CANCEL_PENDING_REASON_OPTIONS}
                                allowClear={false}
                                required
                            />
                        )}
                    />
                    <form.AppField
                        name="comment"
                        children={(field) => (
                            <field.TextareaField
                                id="operations-import-batch-detail-execution-cancel-comment"
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
                                    id="operations-import-batch-detail-execution-cancel-close"
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
                                id="operations-import-batch-detail-execution-cancel-submit"
                                label="确认取消未导入数据"
                                pendingLabel="正在取消"
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
        <DetailWorkspaceSection
            title="导入执行"
            subtitle="期初数据正式写入功能暂未开放。当前预检与部门确认记录已保存；已开始的批次仍可取消未导入的数据。"
        >
            <div className="space-y-3">
                {error ? (
                    <BusinessFailureState
                        title="导入操作未完成"
                        error={error}
                    />
                ) : null}
                <div className="flex flex-wrap gap-2">
                    {canStart ? (
                        <Button
                            id="operations-import-batch-detail-execution-start-apply"
                            type="button"
                            size="sm"
                            disabled
                            onClick={() => setConfirming("START_APPLY")}
                        >
                            开始正式导入
                        </Button>
                    ) : null}
                    {canCancel ? (
                        <Button
                            id="operations-import-batch-detail-execution-cancel-pending"
                            type="button"
                            size="sm"
                            variant="outline"
                            disabled={isExecuting}
                            onClick={() => setCancelling(true)}
                        >
                            取消未导入数据
                        </Button>
                    ) : null}
                    {canRetry ? (
                        <Button
                            id="operations-import-batch-detail-execution-retry-failed"
                            type="button"
                            size="sm"
                            disabled
                            onClick={() => setConfirming("RETRY_FAILED")}
                        >
                            重试失败数据
                        </Button>
                    ) : null}
                </div>
            </div>

            {confirming === "START_APPLY" ? (
                <FormalActionConfirmDialog
                    actionVariant="default"
                    id="operations-import-batch-detail-execution-start-apply"
                    open
                    onOpenChange={(open) => {
                        if (!open) setConfirming(undefined)
                    }}
                    title="开始正式导入"
                    actionLabel="确认开始导入"
                    confirmLabel="确认开始导入"
                    description="提交后将开始正式导入当前待处理数据。"
                    fromStatus={{ label: "待导入", tone: "success" }}
                    toStatus={{ label: "导入中", tone: "info" }}
                    effects={["启动后台导入任务", "仅处理当前待导入的数据"]}
                    irreversibleEffects={[
                        "已成功导入的业务数据会自动保留，不会撤回",
                    ]}
                    pending={isExecuting}
                    onConfirm={() => execute("START_APPLY")}
                />
            ) : null}

            {confirming === "RETRY_FAILED" ? (
                <FormalActionConfirmDialog
                    actionVariant="default"
                    id="operations-import-batch-detail-execution-retry-failed"
                    open
                    onOpenChange={(open) => {
                        if (!open) setConfirming(undefined)
                    }}
                    title="重试失败数据"
                    actionLabel="确认重试"
                    confirmLabel="确认重试"
                    description="仅重置上轮失败的数据；重置完成后仍需点击“开始正式导入”。"
                    fromStatus={{ label: "失败结果", tone: "destructive" }}
                    toStatus={{ label: "待导入", tone: "success" }}
                    effects={[
                        "保留已成功与已跳过结果",
                        "重置失败记录状态以便重新导入",
                    ]}
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
        </DetailWorkspaceSection>
    )
}
