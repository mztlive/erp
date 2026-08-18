"use client"

import type { CardSalesApprovalActions } from "@/features/sales-orders/hooks/use-card-approval-actions"

type RejectForm = CardSalesApprovalActions["rejectForm"]
type TerminateForm = CardSalesApprovalActions["terminateForm"]

export function CardApprovalRejectForm({ form }: { form: RejectForm }) {
    return (
        <form
            className="max-w-md space-y-3 rounded-lg border border-border p-3"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <h3 className="text-sm font-semibold">驳回给销售</h3>
            <form.AppField name="reasonCode">
                {(field) => (
                    <field.TextField
                        label="驳回原因分类"
                        placeholder="例如：资料不齐"
                    />
                )}
            </form.AppField>
            <form.AppField name="comment">
                {(field) => (
                    <field.TextareaField
                        label="驳回说明"
                        rows={2}
                        placeholder="写清需要修改的内容"
                    />
                )}
            </form.AppField>
            <form.AppForm>
                <form.SubmitButton label="驳回" pendingLabel="校验中" />
            </form.AppForm>
        </form>
    )
}

export function CardApprovalTerminateForm({ form }: { form: TerminateForm }) {
    return (
        <form
            className="max-w-md space-y-3 rounded-lg border border-destructive/40 p-3"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <h3 className="text-sm font-semibold">终止本次审批</h3>
            <p className="text-xs text-muted-foreground">
                终止会结束审批并将冻结提交置为已失效，不会形成驳回记录。
            </p>
            <form.AppField name="reasonCode">
                {(field) => (
                    <field.TextField
                        label="终止原因分类"
                        placeholder="例如：业务取消"
                    />
                )}
            </form.AppField>
            <form.AppField name="comment">
                {(field) => (
                    <field.TextareaField
                        label="终止说明"
                        rows={2}
                        placeholder="写清终止审批的原因"
                    />
                )}
            </form.AppField>
            <form.AppForm>
                <form.SubmitButton label="终止审批" pendingLabel="校验中" />
            </form.AppForm>
        </form>
    )
}
