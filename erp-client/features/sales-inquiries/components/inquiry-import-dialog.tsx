"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { useMutation } from "@tanstack/react-query"
import {
    DownloadIcon,
    FileSpreadsheetIcon,
    RotateCwIcon,
    Trash2Icon,
    UploadIcon,
} from "lucide-react"
import type { ReactNode } from "react"
import { CustomerSearchCombobox } from "@/features/entity-selectors/components/customer-search-combobox"

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
import { FileUpload } from "@/components/ui/file-upload"
import { StatusBadge } from "@/components/ui/status-badge"
import { getErrorMessage } from "@/lib/api/errors"
import { fileSelectionError, formatFileSize } from "@/lib/file-selection"
import { cn } from "@/lib/utils"
import {
    INQUIRY_IMPORT_MAX_BYTES,
    INQUIRY_WORKBOOK_MIME,
} from "../lib/customer-workbook"
import {
    InquiryImportPreviewPanel,
    type InquiryImportConfirmResult,
    type InquiryImportPreview,
} from "./inquiry-import-preview-panel"

export {
    InquiryImportPreviewPanel,
    type InquiryImportConfirmResult,
    type InquiryImportPreview,
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
    initialCustomerId?: string
    finalFocusId?: string
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
    initialCustomerId = "",
    finalFocusId = "sales-inquiries-import-create",
    recovery,
}: InquiryImportDialogProps) {
    const replaceInputRef = React.useRef<HTMLInputElement>(null)
    const [replaceError, setReplaceError] = React.useState<string | null>(null)
    const downloadTemplate = useMutation({
        mutationFn: async () => onDownloadTemplate?.(),
    })
    const form = useAppForm({
        defaultValues: {
            customer_id: initialCustomerId,
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
                setReplaceError(null)
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
            setReplaceError(null)
            onOpenChange(false)
        },
    })
    const values = useStore(form.store, (state) => state.values)
    const submitting = useStore(form.store, (state) => state.isSubmitting)
    const locked = busy || submitting || disabled
    const preview = values.preview
    const hasErrors = Boolean(preview?.errors.length)
    const hasMerges = Boolean(preview?.merges.length)

    React.useEffect(() => {
        if (!open || !creation || !initialCustomerId) return
        if (!form.getFieldValue("customer_id")) {
            form.setFieldValue("customer_id", initialCustomerId)
        }
    }, [creation, form, initialCustomerId, open])

    const handleReplaceFile = (files: FileList | null) => {
        if (!files?.length || locked) return
        const selected = Array.from(files)
        const validation = fileSelectionError(selected, {
            accept: `${INQUIRY_WORKBOOK_MIME},.xlsx`,
            multiple: false,
            maxFileSize: INQUIRY_IMPORT_MAX_BYTES,
        })
        if (validation) {
            setReplaceError(validation)
            return
        }
        setReplaceError(null)
        form.setFieldValue("file", selected[0] ?? null)
        form.setFieldValue("preview", null)
        form.setFieldValue("confirm_merge_duplicates", false)
    }

    const handleRemoveFile = () => {
        setReplaceError(null)
        form.setFieldValue("file", null)
        form.setFieldValue("preview", null)
        form.setFieldValue("confirm_merge_duplicates", false)
    }

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
                showCloseButton={!busy && !submitting}
                className={cn(
                    "flex max-h-[90dvh] flex-col gap-0 overflow-hidden p-0",
                    preview ? "sm:max-w-5xl" : "sm:max-w-2xl",
                )}
                finalFocus={
                    creation
                        ? () => document.getElementById(finalFocusId)
                        : undefined
                }
            >
                <DialogHeader className="shrink-0 border-b border-border/60 px-5 pt-4.5 pb-3.5 pr-12">
                    <div className="flex flex-wrap items-center gap-2.5">
                        <DialogTitle>导入客户选品</DialogTitle>
                        {preview ? (
                            <StatusBadge
                                tone={
                                    hasErrors
                                        ? "destructive"
                                        : hasMerges
                                          ? "warning"
                                          : "success"
                                }
                                label={
                                    hasErrors
                                        ? "校验未通过"
                                        : hasMerges
                                          ? "待确认合并"
                                          : "校验通过"
                                }
                            />
                        ) : null}
                    </div>
                    <DialogDescription>
                        {preview
                            ? hasErrors
                                ? "工作簿存在校验问题，请修改原文件后更换上传重新校验。"
                                : hasMerges
                                  ? "请核对重复配送行的合并结果及明细，确认时会再次核对商品资格与当前需求版本。"
                                  : "请核对商品、价格、交期及配送地址。确认写入时会再次核对商品资格与当前需求版本。"
                            : creation
                              ? "选择客户并上传填写好的选品 Excel，校验通过后自动创建询价草稿。存在重复配送行时需先核对合并结果。"
                              : "上传公司商品池导出的选品 Excel，或询价详情下载的客户模板。先核对商品、地址和合并结果，再写入询价草稿。"}
                    </DialogDescription>
                </DialogHeader>
                <form
                    id="sales-inquiry-import-form"
                    className="flex min-h-0 flex-1 flex-col overflow-hidden"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    {preview && values.file ? (
                        <div className="shrink-0 space-y-2.5 border-b border-border/60 bg-surface-sunken/45 px-5 py-2.5">
                            <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-2">
                                <div className="flex min-w-0 flex-1 items-center gap-2.5">
                                    <span className="flex size-7 shrink-0 items-center justify-center rounded-md border border-border/70 bg-card text-primary">
                                        <FileSpreadsheetIcon
                                            aria-hidden="true"
                                            className="size-4"
                                        />
                                    </span>
                                    <div className="flex min-w-0 flex-wrap items-baseline gap-x-2 gap-y-0.5">
                                        <span
                                            title={values.file.name}
                                            className="max-w-64 truncate font-medium text-foreground sm:max-w-md"
                                        >
                                            {values.file.name}
                                        </span>
                                        <span className="num shrink-0 text-xs text-muted-foreground">
                                            {formatFileSize(values.file.size)}
                                        </span>
                                    </div>
                                </div>
                                <div className="flex shrink-0 items-center gap-1">
                                    <input
                                        ref={replaceInputRef}
                                        id="sales-inquiry-import-file-input"
                                        type="file"
                                        accept={`${INQUIRY_WORKBOOK_MIME},.xlsx`}
                                        disabled={locked}
                                        className="sr-only"
                                        onChange={(event) => {
                                            handleReplaceFile(
                                                event.target.files,
                                            )
                                            event.target.value = ""
                                        }}
                                    />
                                    <Button
                                        id="sales-inquiry-import-file-select"
                                        type="button"
                                        variant={
                                            hasErrors ? "outline" : "ghost"
                                        }
                                        size="xs"
                                        disabled={locked}
                                        onClick={() =>
                                            replaceInputRef.current?.click()
                                        }
                                    >
                                        <UploadIcon
                                            aria-hidden="true"
                                            data-icon="inline-start"
                                        />
                                        更换文件
                                    </Button>
                                    <Button
                                        id="sales-inquiry-import-preview-again"
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        disabled={locked}
                                        onClick={() => {
                                            setReplaceError(null)
                                            form.setFieldValue("preview", null)
                                            form.setFieldValue(
                                                "confirm_merge_duplicates",
                                                false,
                                            )
                                            void form.handleSubmit()
                                        }}
                                    >
                                        <RotateCwIcon
                                            aria-hidden="true"
                                            data-icon="inline-start"
                                        />
                                        {creation
                                            ? "重新校验并创建"
                                            : "重新预览"}
                                    </Button>
                                    <Button
                                        id="sales-inquiry-import-file-remove"
                                        type="button"
                                        variant="ghost"
                                        size="icon-xs"
                                        disabled={locked}
                                        aria-label="移除已选文件"
                                        title="移除文件"
                                        className="text-muted-foreground hover:bg-destructive-soft/75 hover:text-destructive-soft-foreground"
                                        onClick={handleRemoveFile}
                                    >
                                        <Trash2Icon aria-hidden="true" />
                                    </Button>
                                </div>
                            </div>
                            {creation ? (
                                <form.AppField name="customer_id">
                                    {(field) => (
                                        <Field className="flex flex-row items-center gap-3">
                                            <FieldLabel
                                                htmlFor="sales-inquiry-import-customer"
                                                className="shrink-0"
                                            >
                                                客户
                                            </FieldLabel>
                                            <div className="min-w-0 max-w-sm flex-1">
                                                <CustomerSearchCombobox
                                                    id="sales-inquiry-import-customer"
                                                    value={
                                                        field.state.value ||
                                                        undefined
                                                    }
                                                    disabled={locked}
                                                    onValueChange={(id) => {
                                                        field.handleChange(
                                                            id ?? "",
                                                        )
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
                            {replaceError ? (
                                <p
                                    role="alert"
                                    className="text-xs text-destructive"
                                >
                                    {replaceError}
                                </p>
                            ) : null}
                        </div>
                    ) : null}

                    <div className="flex min-h-0 flex-1 flex-col gap-4 overflow-y-auto px-5 py-4">
                        {!preview ? (
                            <>
                                {creation ? (
                                    <form.AppField name="customer_id">
                                        {(field) => (
                                            <Field>
                                                <FieldLabel htmlFor="sales-inquiry-import-customer">
                                                    客户
                                                </FieldLabel>
                                                <CustomerSearchCombobox
                                                    id="sales-inquiry-import-customer"
                                                    value={
                                                        field.state.value ||
                                                        undefined
                                                    }
                                                    disabled={locked}
                                                    onValueChange={(id) => {
                                                        field.handleChange(
                                                            id ?? "",
                                                        )
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
                                ) : null}
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
                                                maxFileSize={
                                                    INQUIRY_IMPORT_MAX_BYTES
                                                }
                                                label="上传客户填写的工作簿"
                                                description="仅支持 XLSX，最大 5 MiB、2,000 条配送行，分组后最多 200 条需求。系统自动公式可直接导入，自行添加的公式须先粘贴为值。"
                                                selectedFiles={
                                                    field.state.value
                                                        ? [field.state.value]
                                                        : []
                                                }
                                                onFilesSelected={(files) => {
                                                    setReplaceError(null)
                                                    field.handleChange(
                                                        files[0] ?? null,
                                                    )
                                                    field.handleBlur()
                                                    form.setFieldValue(
                                                        "preview",
                                                        null,
                                                    )
                                                    form.setFieldValue(
                                                        "confirm_merge_duplicates",
                                                        false,
                                                    )
                                                }}
                                                onFileRemove={handleRemoveFile}
                                            />
                                            <FieldError
                                                errors={toFieldErrors(
                                                    field.state.meta.errors,
                                                )}
                                            />
                                        </Field>
                                    )}
                                </form.AppField>
                            </>
                        ) : (
                            <InquiryImportPreviewPanel preview={preview} />
                        )}

                        {error || downloadTemplate.isError ? (
                            <Alert variant="destructive" className="shrink-0">
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

                    {preview?.merges.length ? (
                        <div
                            className={cn(
                                "shrink-0 border-t border-border/60 px-5 py-2.5 transition-colors",
                                hasErrors
                                    ? "bg-muted/35"
                                    : values.confirm_merge_duplicates
                                      ? "bg-success-soft/35"
                                      : "bg-warning-soft/50",
                            )}
                        >
                            <form.AppField name="confirm_merge_duplicates">
                                {(field) => (
                                    <Field>
                                        <div className="flex items-start gap-2.5">
                                            <Checkbox
                                                id="sales-inquiry-import-confirm-merges"
                                                checked={field.state.value}
                                                disabled={locked || hasErrors}
                                                className="mt-0.5"
                                                onCheckedChange={(checked) => {
                                                    field.handleChange(
                                                        checked === true,
                                                    )
                                                    field.handleBlur()
                                                }}
                                            />
                                            <FieldLabel
                                                htmlFor="sales-inquiry-import-confirm-merges"
                                                className="text-body-compact font-medium text-foreground"
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
                        </div>
                    ) : null}

                    <DialogFooter className="shrink-0 border-t border-border/60 bg-surface-sunken/30 px-5 py-3 sm:items-center sm:justify-between">
                        <div className="flex items-center gap-2">
                            {onDownloadTemplate ? (
                                <Button
                                    id="sales-inquiry-import-download-template"
                                    type="button"
                                    variant="ghost"
                                    disabled={
                                        locked ||
                                        templateBusy ||
                                        downloadTemplate.isPending
                                    }
                                    className="text-muted-foreground hover:text-foreground"
                                    onClick={() => downloadTemplate.mutate()}
                                >
                                    <DownloadIcon data-icon="inline-start" />
                                    {templateBusy || downloadTemplate.isPending
                                        ? "正在生成…"
                                        : "下载客户模板"}
                                </Button>
                            ) : preview && !hasErrors ? (
                                <span className="num text-xs text-muted-foreground">
                                    共 {preview.data.lines.length}{" "}
                                    条需求待写入草稿
                                </span>
                            ) : null}
                        </div>
                        <div className="flex flex-col-reverse gap-2 sm:flex-row sm:items-center">
                            <Button
                                id="sales-inquiry-import-cancel"
                                type="button"
                                variant="outline"
                                disabled={busy || submitting}
                                onClick={() => onOpenChange(false)}
                            >
                                关闭
                            </Button>
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
                        </div>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}

export { InquiryCustomerTemplateButton } from "@/components/business/inquiry-customer-template-button"
