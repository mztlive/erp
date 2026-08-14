"use client"

import { FormalActionResult } from "@/components/business"
import { Button } from "@/components/ui/button"
import type { CustomerMutationResult } from "@/features/customers/types"
import type { CustomerFormApi } from "@/features/customers/components/customer-form-values"

/** 提交结果卡：成功/结果待确认两种展示，冲突由外层对话框承载。 */
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
    return (
        <>
            {result?.outcome === "succeeded" ? (
                <FormalActionResult
                    status="succeeded"
                    title={mode === "create" ? "客户已创建" : "客户资料已保存"}
                    description={
                        mode === "create"
                            ? `客户号 ${result.customerNo} · 基础资料版本 v${result.revisionNo}`
                            : `客户号 ${result.customerNo} · 新版本 v${result.revisionNo} · 历史单据记录不变`
                    }
                    reference={result.reference}
                    facts={
                        mode === "create"
                            ? [
                                  { label: "客户号", value: result.customerNo },
                                  {
                                      label: "版本",
                                      value: `v${result.revisionNo}`,
                                  },
                                  { label: "时间", value: result.occurredAt },
                              ]
                            : [
                                  { label: "客户号", value: result.customerNo },
                                  {
                                      label: "新版本",
                                      value: `v${result.revisionNo} · 数据版本 ${result.lockVersion}`,
                                  },
                                  { label: "时间", value: result.occurredAt },
                              ]
                    }
                />
            ) : null}

            {result?.outcome === "unknown" ? (
                <FormalActionResult
                    status="unknown"
                    title={
                        mode === "create" ? "创建结果不确定" : "保存结果不确定"
                    }
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
            ) : null}
        </>
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
