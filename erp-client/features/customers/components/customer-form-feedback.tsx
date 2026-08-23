"use client"

import { FormalActionResult } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { CustomerMutationResult } from "@/features/customers/types"
import type { CustomerFormApi } from "@/features/customers/components/customer-form-values"

/** 仅保留需要继续核对的未知结果；成功反馈由调用容器用 Toast 承载。 */
export function CustomerFormResultPanel({
    result,
    mode,
    isQueryingIdempotency,
    onQueryFinalResult,
}: {
    result: CustomerMutationResult | null
    mode: "create" | "edit"
    isQueryingIdempotency: boolean
    onQueryFinalResult: (idempotencyKey: string) => void
}) {
    if (result?.outcome !== "unknown") return null

    return (
        <FormalActionResult
            status="unknown"
            title={mode === "create" ? "创建结果不确定" : "保存结果不确定"}
            description={result.message}
            reference={result.idempotencyKey}
            referenceLabel="原任务号"
            actions={
                <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={isQueryingIdempotency}
                    onClick={() => onQueryFinalResult(result.idempotencyKey)}
                >
                    查询最终结果
                </Button>
            }
        />
    )
}

/** 表单底部操作条：取消/关闭、提交与完成。 */
export function CustomerFormActionBar({
    form,
    result,
    isPending,
    submitLabel,
    dirty,
    onCancel,
    onDiscardRequest,
    onResetSession,
}: {
    form: Pick<CustomerFormApi, "AppForm" | "SubmitButton">
    result: CustomerMutationResult | null
    isPending: boolean
    submitLabel: string
    dirty: boolean
    onCancel: () => void
    onDiscardRequest: () => void
    onResetSession: () => void
}) {
    const succeeded = result?.outcome === "succeeded"

    return (
        <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
            <Button
                type="button"
                variant="outline"
                onClick={() => {
                    if (succeeded) {
                        onResetSession()
                        onCancel()
                        return
                    }
                    if (dirty) {
                        onDiscardRequest()
                        return
                    }
                    onCancel()
                }}
            >
                {succeeded ? "关闭" : "取消"}
            </Button>
            {!succeeded ? (
                <form.AppForm>
                    <form.SubmitButton
                        label={submitLabel}
                        disabled={isPending}
                    />
                </form.AppForm>
            ) : (
                <Button
                    type="button"
                    onClick={() => {
                        onResetSession()
                        onCancel()
                    }}
                >
                    完成
                </Button>
            )}
        </div>
    )
}
