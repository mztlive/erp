"use client"

import { CheckCircle2Icon } from "lucide-react"

import { MoneyValue, WorkspaceTaskFooter } from "@/components/business"
import { useAppForm, toFieldErrors } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { LoadingButton } from "@/components/ui/loading-button"
import { InvoiceFilesField } from "@/features/customer-receivables/components/invoice-files-field"
import { invoiceFilesError } from "@/features/customer-receivables/lib/invoice-files"
import { useCompleteInvoice } from "../hooks/use-complete-invoice"
import { getErrorMessage } from "@/lib/api/errors"
import { commandFailureDisposition } from "@/lib/api/command-recovery"
import type { InvoiceRequest } from "@/features/invoice-requests/api"
import type { WorkspaceWorkItem } from "../types"

/** 附件随确认命令上传；同一次操作重试保留原操作号和文件。 */
export function WorkspaceInvoiceUpload({
    item,
    request,
    amount,
    onTaskCompleted,
}: {
    item: WorkspaceWorkItem
    request: InvoiceRequest
    amount: string
    onTaskCompleted?: (id: string) => void
}) {
    const complete = useCompleteInvoice(item, request, onTaskCompleted)
    const uncertain =
        complete.isError &&
        commandFailureDisposition(complete.error) === "unknown"
    const locked = complete.isPending || complete.isSuccess || uncertain
    const form = useAppForm({
        defaultValues: { files: [] as File[] },
        onSubmit: async ({ value }) => {
            try {
                await complete.mutateAsync(value.files)
            } catch {
                /* 错误显示在操作区，重试沿用操作号。 */
            }
        },
    })
    return (
        <section
            className="space-y-5 border-t border-grid pt-6"
            aria-label="实际发票信息"
        >
            <div>
                <h3 className="text-base font-semibold">实际发票信息</h3>
                <p className="mt-1 text-sm text-muted-foreground">
                    上传本次开票的 PDF 或图片，确认后按剩余批准金额完成开票。
                </p>
            </div>
            <form.AppField
                name="files"
                validators={{
                    onChange: ({ value }) =>
                        value.length
                            ? invoiceFilesError(value)
                            : "请上传至少一个发票附件",
                    onSubmit: ({ value }) =>
                        value.length
                            ? invoiceFilesError(value)
                            : "请上传至少一个发票附件",
                }}
            >
                {(field) => (
                    <InvoiceFilesField
                        files={field.state.value}
                        disabled={locked}
                        label="发票附件"
                        density="compact"
                        errors={toFieldErrors(field.state.meta.errors)}
                        onChange={(files) => {
                            complete.reset()
                            field.handleChange(files)
                            field.handleBlur()
                        }}
                    />
                )}
            </form.AppField>
            {complete.isError ? (
                <Alert variant="destructive">
                    <AlertTitle>
                        {uncertain ? "登记结果待确认" : "发票登记失败"}
                    </AlertTitle>
                    <AlertDescription>
                        {uncertain
                            ? "请保留当前附件，点击“重试确认登记”核对本次结果，避免重复登记。"
                            : getErrorMessage(complete.error, "请重试")}
                    </AlertDescription>
                </Alert>
            ) : null}
            {complete.isSuccess ? (
                <Alert>
                    <CheckCircle2Icon />
                    <AlertTitle>本次开票已完成</AlertTitle>
                    <AlertDescription>
                        附件已保存，可在销售单详情下载。
                    </AlertDescription>
                </Alert>
            ) : null}
            <WorkspaceTaskFooter>
                <div className="mr-auto min-w-0 space-y-1 text-sm">
                    <p>
                        本次登记{" "}
                        <MoneyValue value={amount} className="font-semibold" />
                    </p>
                    <p className="text-xs text-muted-foreground">
                        确认后完成本次申请，并核销关联应收。
                    </p>
                </div>
                <form.Subscribe
                    selector={(state) =>
                        [state.values.files, state.isSubmitting] as const
                    }
                >
                    {([files, submitting]) => (
                        <LoadingButton
                            id="workspace-invoice-confirm"
                            type="button"
                            loading={complete.isPending || submitting}
                            disabled={
                                !files.length ||
                                Boolean(invoiceFilesError(files)) ||
                                complete.isPending ||
                                complete.isSuccess ||
                                submitting
                            }
                            onClick={() => void form.handleSubmit()}
                        >
                            {uncertain ? "重试确认登记" : "确认登记发票"}
                        </LoadingButton>
                    )}
                </form.Subscribe>
            </WorkspaceTaskFooter>
        </section>
    )
}
