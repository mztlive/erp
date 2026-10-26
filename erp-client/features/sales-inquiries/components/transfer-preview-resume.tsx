"use client"

import { useAppForm, toFieldErrors } from "@/components/form"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DataTransferContext } from "@/lib/data-transfer"
import { inquiryPath } from "../api"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import { useVerifyTransferPreviewMutation } from "../hooks/use-transfer-preview"
import {
    InquiryImportPreviewPanel,
    type InquiryImportConfirmResult,
    type InquiryImportPreview,
} from "./inquiry-import-dialog"
import {
    SupplierQuotePreviewPanel,
    type SupplierQuoteImportPreview,
} from "./supplier-quote-preview-panel"

export type InquiryPreviewKind =
    | "inquiry_preview"
    | "inquiry_creation_preview"
    | "supplier_quote_preview"

export type TransferPreviewData =
    | InquiryImportPreview
    | SupplierQuoteImportPreview

type TransferPreviewResumeProps = {
    jobId: string
    kind: InquiryPreviewKind
    context: DataTransferContext
    preview: TransferPreviewData
    userId: string
    open: boolean
    onOpenChange: (open: boolean) => void
    onConfirmed: (result?: InquiryImportConfirmResult) => void
}

function requiredText(value: string | undefined, label: string) {
    if (!value) throw new Error(`任务缺少${label}，请回到业务页面重新导入。`)
    return value
}

function requiredVersion(value: number | undefined) {
    if (value === undefined)
        throw new Error("任务缺少原业务版本，请回到业务页面重新导入。")
    return value
}

/** 从原任务恢复核对内容；确认与结果恢复沿用询价原命令合同。 */
export function TransferPreviewResumeDialog({
    jobId,
    kind,
    context,
    preview,
    userId,
    open,
    onOpenChange,
    onConfirmed,
}: TransferPreviewResumeProps) {
    const creation = kind === "inquiry_creation_preview"
    const quote = kind === "supplier_quote_preview"
    const inquiryPreview = quote ? null : (preview as InquiryImportPreview)
    const quotePreview = quote ? (preview as SupplierQuoteImportPreview) : null
    const scope = creation
        ? "import-new"
        : (context.context.inquiry_id ?? jobId)
    const command = useInquiryCommand(userId, scope)
    const verification = useVerifyTransferPreviewMutation(jobId)
    const pending = command.isPending || verification.isPending
    const idPrefix = `inquiry-transfer-${toAutomationIdSegment(jobId)}`
    const commandKey = `inquiry-transfer:${jobId}`
    const rows = quotePreview?.rows.filter((row) => row.status !== "已导入")
    const hasIssues = Boolean(
        inquiryPreview?.errors.length || quotePreview?.issues.length,
    )
    const form = useAppForm({
        defaultValues: { confirm_merge_duplicates: false },
        validators: {
            onSubmit: ({ value }) =>
                inquiryPreview?.merges.length && !value.confirm_merge_duplicates
                    ? {
                          fields: {
                              confirm_merge_duplicates:
                                  "请核对并确认重复配送行的合并结果",
                          },
                      }
                    : undefined,
        },
        onSubmit: async ({ value }) => {
            if (pending || hasIssues || context.resume_state !== "ready") return
            const current = await verification.mutateAsync()
            if (current.kind !== kind)
                throw new Error("任务类型与当前预览不匹配，请刷新后重试。")
            const source = current.context
            let result: InquiryImportConfirmResult | undefined
            if (quotePreview) {
                await command.run(
                    `${inquiryPath(true, requiredText(source.inquiry_id, "关联询价"))}/internal-quotes/import`,
                    {
                        expected_version: requiredVersion(
                            source.expected_version,
                        ),
                        task_id: requiredText(source.task_id, "采购任务"),
                        task_version: requiredVersion(source.task_version),
                        requirement_version: requiredVersion(
                            source.requirement_version,
                        ),
                        feedback_round: requiredVersion(source.feedback_round),
                        rows: (rows ?? []).map(({ source, sku_id, quote }) => ({
                            source,
                            sku_id,
                            quote,
                        })),
                    },
                    "POST",
                    "command_key",
                    commandKey,
                )
            } else if (inquiryPreview) {
                result = await command.run<InquiryImportConfirmResult>(
                    creation
                        ? `${inquiryPath()}/imports/create`
                        : `${inquiryPath(false, requiredText(source.inquiry_id, "关联询价"))}/imports/confirm`,
                    {
                        ...(creation
                            ? {
                                  customer_id: requiredText(
                                      source.customer_id,
                                      "客户资料",
                                  ),
                              }
                            : {
                                  expected_version:
                                      inquiryPreview.expected_version,
                              }),
                        preview_id: inquiryPreview.preview_id,
                        input_fingerprint: inquiryPreview.input_fingerprint,
                        confirm_merge_duplicates:
                            value.confirm_merge_duplicates,
                    },
                    "POST",
                    "command_key",
                    commandKey,
                )
            }
            onConfirmed(result)
        },
    })
    return (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                closeButtonId={`${idPrefix}-close`}
                className="flex max-h-[85svh] flex-col sm:max-w-4xl"
            >
                <DialogHeader>
                    <DialogTitle>
                        {quote ? "核对供应商报价" : "核对客户选品导入"}
                    </DialogTitle>
                    <DialogDescription>
                        {quote
                            ? "核对原任务生成的报价预览，确认后登记内部报价。"
                            : creation
                              ? "核对原任务生成的商品、配送地址及合并结果，确认后创建询价草稿。"
                              : "核对原任务生成的商品、配送地址及合并结果，确认后写入询价草稿。"}
                    </DialogDescription>
                </DialogHeader>
                <form
                    id={`${idPrefix}-form`}
                    className="flex min-h-0 flex-1 flex-col gap-5"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <div className="min-h-0 space-y-4 overflow-y-auto pr-1">
                        {context.resume_state !== "ready" && (
                            <Alert variant="warning">
                                <AlertDescription>
                                    {context.resume_reason ||
                                        "预览状态已变化，请刷新任务后核对。"}
                                </AlertDescription>
                            </Alert>
                        )}
                        {inquiryPreview && (
                            <InquiryImportPreviewPanel
                                preview={inquiryPreview}
                            />
                        )}
                        {quotePreview && (
                            <SupplierQuotePreviewPanel
                                preview={quotePreview}
                                idPrefix={`${idPrefix}-quote`}
                            />
                        )}
                        {Boolean(inquiryPreview?.merges.length) && (
                            <form.AppField name="confirm_merge_duplicates">
                                {(field) => (
                                    <Field>
                                        <div className="flex items-start gap-2 rounded-lg border p-3">
                                            <Checkbox
                                                id={`${idPrefix}-confirm-merges`}
                                                checked={field.state.value}
                                                disabled={pending || hasIssues}
                                                onCheckedChange={(checked) =>
                                                    field.handleChange(
                                                        checked === true,
                                                    )
                                                }
                                            />
                                            <FieldLabel
                                                htmlFor={`${idPrefix}-confirm-merges`}
                                            >
                                                已核对并确认重复配送行的合并结果
                                            </FieldLabel>
                                        </div>
                                        <FieldError
                                            errors={toFieldErrors(
                                                field.state.meta.errors,
                                            )}
                                        />
                                    </Field>
                                )}
                            </form.AppField>
                        )}
                        {(command.error || verification.error) && (
                            <Alert variant="destructive">
                                <AlertDescription>
                                    {getErrorMessage(
                                        command.error ?? verification.error,
                                        "提交未完成，请核对后重试。",
                                    )}
                                </AlertDescription>
                            </Alert>
                        )}
                        {command.unresolved && (
                            <Alert variant="warning">
                                <AlertDescription>
                                    上次提交结果尚待核对，请先核对上次提交，避免重复登记。
                                </AlertDescription>
                            </Alert>
                        )}
                    </div>
                    <DialogFooter>
                        <Button
                            id={`${idPrefix}-cancel`}
                            type="button"
                            variant="outline"
                            onClick={() => onOpenChange(false)}
                        >
                            关闭
                        </Button>
                        {command.unresolved && (
                            <Button
                                id={`${idPrefix}-recover`}
                                type="button"
                                variant="outline"
                                disabled={pending}
                                onClick={() => {
                                    void command
                                        .recover<InquiryImportConfirmResult>()
                                        .then((result) => onConfirmed(result))
                                        .catch(() => undefined)
                                }}
                            >
                                核对上次提交
                            </Button>
                        )}
                        <form.AppForm>
                            <form.SubmitButton
                                id={`${idPrefix}-confirm`}
                                label={
                                    quote
                                        ? `确认登记 ${rows?.length ?? 0} 条报价`
                                        : creation
                                          ? "确认创建询价草稿"
                                          : "确认写入询价草稿"
                                }
                                pendingLabel="正在确认…"
                                loading={pending}
                                disabled={
                                    !command.ready ||
                                    command.unresolved ||
                                    hasIssues ||
                                    context.resume_state !== "ready" ||
                                    (quote && !rows?.length)
                                }
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
