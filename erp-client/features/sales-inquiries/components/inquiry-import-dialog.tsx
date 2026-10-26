"use client"

import { useStore } from "@tanstack/react-form"
import { useMutation } from "@tanstack/react-query"
import { DownloadIcon } from "lucide-react"
import type { ReactNode } from "react"
import { CustomerSearchCombobox } from "@/features/entity-selectors/components/customer-search-combobox"

import {
    ImportIssueTable,
    MoneyValue,
    QuantityValue,
} from "@/components/business"
import { useAppForm, toFieldErrors } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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
import { FileUpload } from "@/components/ui/file-upload"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { InquiryRequirement } from "../types"
import {
    INQUIRY_IMPORT_MAX_BYTES,
    INQUIRY_WORKBOOK_MIME,
} from "../lib/customer-workbook"
import { inquiryDateInput } from "../lib/format"

export type InquiryImportPreview = {
    preview_id: string
    template_version: string
    expected_version: number
    input_fingerprint: string
    errors: {
        sheet: string
        row_number: number
        column: string
        message: string
    }[]
    merges: {
        sheet: string
        row_numbers: number[]
        sku_code: string
        merged_quantity: string
    }[]
    data: InquiryRequirement
    expires_at: number
}

export type InquiryImportConfirmResult = {
    inquiry_id: string
    preview_id: string
    version: number
    line_count: number
    allocation_count: number
}

type InquiryImportDialogProps = {
    open: boolean
    onOpenChange: (open: boolean) => void
    onPreview: (file: File, customerId: string) => Promise<InquiryImportPreview>
    onConfirm: (
        preview: InquiryImportPreview,
        confirm_merge_duplicates: boolean,
        customerId: string,
    ) => Promise<unknown>
    busy?: boolean
    disabled?: boolean
    error?: unknown
    onDownloadTemplate?: () => Promise<void>
    templateBusy?: boolean
    creation?: boolean
    recovery?: ReactNode
}

export function InquiryImportDialog({
    open,
    onOpenChange,
    onPreview,
    onConfirm,
    busy = false,
    disabled = false,
    error,
    onDownloadTemplate,
    templateBusy = false,
    creation = false,
    recovery,
}: InquiryImportDialogProps) {
    const downloadTemplate = useMutation({
        mutationFn: async () => onDownloadTemplate?.(),
    })
    const form = useAppForm({
        defaultValues: {
            customer_id: "",
            file: null as File | null,
            preview: null as InquiryImportPreview | null,
            confirm_merge_duplicates: false,
        },
        validators: {
            onSubmit: ({ value }) => {
                if (creation && !value.customer_id)
                    return { fields: { customer_id: "请选择客户" } }
                if (!value.file || !/\.xlsx$/i.test(value.file.name))
                    return { fields: { file: "请选择客户选品 XLSX 文件" } }
                if (
                    !value.file.size ||
                    value.file.size > INQUIRY_IMPORT_MAX_BYTES
                )
                    return {
                        fields: { file: "请选择非空且不超过 5 MiB 的文件" },
                    }
                if (value.preview?.errors.length)
                    return { fields: { file: "请修正文件中的问题后重新预览" } }
                if (
                    value.preview?.merges.length &&
                    !value.confirm_merge_duplicates
                )
                    return {
                        fields: {
                            confirm_merge_duplicates:
                                "请先核对并确认重复配送行的合并结果",
                        },
                    }
                return undefined
            },
        },
        onSubmit: async ({ value }) => {
            if (busy || disabled || !value.file) return
            if (!value.preview) {
                const preview = await onPreview(value.file, value.customer_id)
                form.setFieldValue("preview", preview)
                form.setFieldValue("confirm_merge_duplicates", false)
                if (
                    creation &&
                    !preview.errors.length &&
                    !preview.merges.length
                ) {
                    await onConfirm(preview, false, value.customer_id)
                    form.reset()
                    onOpenChange(false)
                }
                return
            }
            await onConfirm(
                value.preview,
                value.confirm_merge_duplicates,
                value.customer_id,
            )
            form.reset()
            onOpenChange(false)
        },
    })
    const values = useStore(form.store, (state) => state.values)
    const submitting = useStore(form.store, (state) => state.isSubmitting)
    const locked = busy || submitting || disabled
    const preview = values.preview
    const hasErrors = Boolean(preview?.errors.length)

    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!busy && !submitting) onOpenChange(next)
            }}
        >
            <DialogContent
                id="sales-inquiry-import-dialog"
                closeButtonId="sales-inquiry-import-dismiss"
                className="flex max-h-[90dvh] flex-col sm:max-w-4xl"
                finalFocus={
                    creation
                        ? () =>
                              document.getElementById(
                                  "sales-inquiries-import-create",
                              )
                        : undefined
                }
            >
                <DialogHeader>
                    <DialogTitle>导入客户选品</DialogTitle>
                    <DialogDescription>
                        {creation
                            ? "选择客户并上传填写好的选品 Excel，校验通过后自动创建询价草稿。存在重复配送行时需先核对合并结果。"
                            : "上传公司商品池导出的选品 Excel，或询价详情下载的客户模板。先核对商品、地址和合并结果，再写入询价草稿。"}
                    </DialogDescription>
                </DialogHeader>
                <form
                    id="sales-inquiry-import-form"
                    className="flex min-h-0 flex-1 flex-col gap-5"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <div className="min-h-0 space-y-5 overflow-y-auto pr-1">
                        {creation && (
                            <form.AppField name="customer_id">
                                {(field) => (
                                    <Field>
                                        <FieldLabel htmlFor="sales-inquiry-import-customer">
                                            客户
                                        </FieldLabel>
                                        <CustomerSearchCombobox
                                            id="sales-inquiry-import-customer"
                                            value={
                                                field.state.value || undefined
                                            }
                                            disabled={locked}
                                            onValueChange={(id) => {
                                                field.handleChange(id ?? "")
                                                form.setFieldValue(
                                                    "preview",
                                                    null,
                                                )
                                                form.setFieldValue(
                                                    "confirm_merge_duplicates",
                                                    false,
                                                )
                                            }}
                                        />
                                        <FieldError
                                            errors={toFieldErrors(
                                                field.state.meta.errors,
                                            )}
                                        />
                                    </Field>
                                )}
                            </form.AppField>
                        )}
                        <form.AppField name="file">
                            {(field) => (
                                <Field>
                                    <FieldLabel htmlFor="sales-inquiry-import-file-input">
                                        客户选品工作簿
                                    </FieldLabel>
                                    <FileUpload
                                        idPrefix="sales-inquiry-import-file"
                                        accept={`${INQUIRY_WORKBOOK_MIME},.xlsx`}
                                        multiple={false}
                                        disabled={locked}
                                        maxFileSize={INQUIRY_IMPORT_MAX_BYTES}
                                        label="上传客户填写的工作簿"
                                        description="仅支持 XLSX，最大 5 MiB、2,000 条配送行，分组后最多 200 条需求。系统自动公式可直接导入，自行添加的公式须先粘贴为值。"
                                        selectedFiles={
                                            field.state.value
                                                ? [field.state.value]
                                                : []
                                        }
                                        onFilesSelected={(files) => {
                                            field.handleChange(files[0] ?? null)
                                            field.handleBlur()
                                            form.setFieldValue("preview", null)
                                            form.setFieldValue(
                                                "confirm_merge_duplicates",
                                                false,
                                            )
                                        }}
                                        onFileRemove={() => {
                                            field.handleChange(null)
                                            form.setFieldValue("preview", null)
                                            form.setFieldValue(
                                                "confirm_merge_duplicates",
                                                false,
                                            )
                                        }}
                                    />
                                    <FieldError
                                        errors={toFieldErrors(
                                            field.state.meta.errors,
                                        )}
                                    />
                                </Field>
                            )}
                        </form.AppField>

                        {preview ? (
                            <InquiryImportPreviewPanel preview={preview} />
                        ) : null}

                        {preview?.merges.length ? (
                            <form.AppField name="confirm_merge_duplicates">
                                {(field) => (
                                    <Field>
                                        <div className="flex items-start gap-2 rounded-lg border p-3">
                                            <Checkbox
                                                id="sales-inquiry-import-confirm-merges"
                                                checked={field.state.value}
                                                disabled={locked || hasErrors}
                                                onCheckedChange={(checked) => {
                                                    field.handleChange(
                                                        checked === true,
                                                    )
                                                    field.handleBlur()
                                                }}
                                            />
                                            <FieldLabel
                                                htmlFor="sales-inquiry-import-confirm-merges"
                                                className="leading-5"
                                            >
                                                已核对重复配送行，同意按上方结果合并数量
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
                        ) : null}
                        {error || downloadTemplate.isError ? (
                            <Alert variant="destructive">
                                <AlertDescription>
                                    {getErrorMessage(
                                        error ?? downloadTemplate.error,
                                        "处理失败，请保留文件后重试",
                                    )}
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        {recovery}
                    </div>
                    <DialogFooter className="shrink-0">
                        {onDownloadTemplate ? (
                            <Button
                                id="sales-inquiry-import-download-template"
                                type="button"
                                variant="outline"
                                disabled={
                                    locked ||
                                    templateBusy ||
                                    downloadTemplate.isPending
                                }
                                onClick={() => downloadTemplate.mutate()}
                            >
                                <DownloadIcon data-icon="inline-start" />
                                {templateBusy || downloadTemplate.isPending
                                    ? "正在生成…"
                                    : "下载客户模板"}
                            </Button>
                        ) : null}
                        <Button
                            id="sales-inquiry-import-cancel"
                            type="button"
                            variant="outline"
                            disabled={busy || submitting}
                            onClick={() => onOpenChange(false)}
                        >
                            关闭
                        </Button>
                        {preview ? (
                            <Button
                                id="sales-inquiry-import-preview-again"
                                type="button"
                                variant="outline"
                                disabled={locked}
                                onClick={() => {
                                    form.setFieldValue("preview", null)
                                    form.setFieldValue(
                                        "confirm_merge_duplicates",
                                        false,
                                    )
                                    void form.handleSubmit()
                                }}
                            >
                                {creation ? "重新校验并创建" : "重新预览"}
                            </Button>
                        ) : null}
                        <form.AppForm>
                            <form.SubmitButton
                                id="sales-inquiry-import-submit"
                                label={
                                    creation
                                        ? "创建询价草稿"
                                        : preview
                                          ? "确认写入草稿"
                                          : "校验并预览"
                                }
                                pendingLabel={
                                    preview
                                        ? creation
                                            ? "正在创建…"
                                            : "正在写入…"
                                        : "正在校验…"
                                }
                                loading={busy}
                                disabled={disabled || hasErrors}
                            />
                        </form.AppForm>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}

export function InquiryImportPreviewPanel({
    preview,
}: {
    preview: InquiryImportPreview
}) {
    return (
        <section aria-label="导入预览" className="space-y-4">
            <Alert variant={preview.errors.length ? "warning" : "success"}>
                <AlertTitle>
                    {preview.errors.length
                        ? "请修正后重新预览"
                        : "校验通过，请核对后确认"}
                </AlertTitle>
                <AlertDescription>
                    {preview.data.lines.length} 条商品需求、
                    {preview.data.lines.reduce(
                        (sum, line) => sum + line.allocations.length,
                        0,
                    )}{" "}
                    个配送地址。
                    {preview.errors.length
                        ? `发现 ${preview.errors.length} 项问题，当前文件不会写入草稿。`
                        : "确认时会再次核对商品资格与当前需求版本。"}
                </AlertDescription>
            </Alert>
            {preview.errors.length ? (
                <ImportIssueTable
                    issues={preview.errors.map((issue) => ({
                        id: `${issue.sheet}-${issue.row_number}-${issue.column}-${issue.message}`,
                        rowNumber: `${issue.sheet} · ${issue.row_number}`,
                        field: issue.column,
                        errorCode: "INQUIRY_IMPORT_INVALID",
                        message: issue.message,
                        repairable: true,
                    }))}
                    errorCodeLabel={() => "校验未通过"}
                    repairableLabel="修改文件后重新预览"
                />
            ) : null}
            {preview.merges.length ? (
                <div className="overflow-hidden rounded-lg border">
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>重复配送行</TableHead>
                                <TableHead>SKU 编码</TableHead>
                                <TableHead>合并后数量</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {preview.merges.map((merge) => (
                                <TableRow
                                    key={`${merge.sheet}-${merge.row_numbers.join("-")}`}
                                >
                                    <TableCell>
                                        {merge.sheet} · 第{" "}
                                        {merge.row_numbers.join("、")} 行
                                    </TableCell>
                                    <TableCell className="num">
                                        {merge.sku_code}
                                    </TableCell>
                                    <TableCell>
                                        <QuantityValue
                                            value={merge.merged_quantity}
                                            unit=""
                                        />
                                    </TableCell>
                                </TableRow>
                            ))}
                        </TableBody>
                    </Table>
                </div>
            ) : null}
            {preview.data.lines.map((line) => {
                const id = `sales-inquiry-import-line-${toAutomationIdSegment(line.line_id)}`
                return (
                    <details key={line.line_id} className="rounded-lg border">
                        <summary
                            id={`${id}-toggle`}
                            className="cursor-pointer space-y-1 px-4 py-3"
                        >
                            <span className="font-medium">
                                {line.product_name}
                            </span>
                            <span className="ml-3">
                                <QuantityValue
                                    value={line.quantity}
                                    unit={line.unit_code}
                                />
                            </span>
                            <span className="ml-3 text-xs text-muted-foreground">
                                {line.sku_code} · {line.allocations.length}{" "}
                                个地址
                            </span>
                        </summary>
                        <div className="space-y-3 border-t px-4 py-3">
                            <p className="text-sm text-muted-foreground">
                                {line.specification || "未标注规格"}
                                ；要求交付日期：
                                {inquiryDateInput(line.delivery_due_at) ||
                                    "未标注"}
                                ；交付条件：
                                {line.delivery_terms || "未标注"}
                            </p>
                            {line.sales_reference_price_gross ? (
                                <p className="text-sm">
                                    销售价：
                                    <MoneyValue
                                        value={line.sales_reference_price_gross}
                                        display="exact"
                                    />
                                </p>
                            ) : null}
                            <Table>
                                <TableHeader>
                                    <TableRow>
                                        <TableHead>收件人及电话</TableHead>
                                        <TableHead>收货地址</TableHead>
                                        <TableHead>数量</TableHead>
                                    </TableRow>
                                </TableHeader>
                                <TableBody>
                                    {line.allocations.map((allocation) => (
                                        <TableRow
                                            key={allocation.allocation_id}
                                        >
                                            <TableCell>
                                                {allocation.recipient}
                                                <br />
                                                <span className="num text-muted-foreground">
                                                    {allocation.phone}
                                                </span>
                                            </TableCell>
                                            <TableCell className="whitespace-normal">
                                                {allocation.region_name}
                                                {allocation.address}
                                            </TableCell>
                                            <TableCell>
                                                <QuantityValue
                                                    value={allocation.quantity}
                                                    unit={line.unit_code}
                                                />
                                            </TableCell>
                                        </TableRow>
                                    ))}
                                </TableBody>
                            </Table>
                        </div>
                    </details>
                )
            })}
        </section>
    )
}

export { InquiryCustomerTemplateButton } from "@/components/business/inquiry-customer-template-button"
