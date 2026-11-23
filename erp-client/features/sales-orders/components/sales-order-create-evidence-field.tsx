"use client"

import { FileTextIcon, Trash2Icon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { FileUpload } from "@/components/ui/file-upload"
import { RequiredMark } from "@/components/ui/required-mark"
import { useSalesOrderEvidenceUpload } from "@/features/sales-orders/hooks/use-sales-order-evidence"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { fileUploadText } from "@/lib/ui-text"

/** 无合同开单的凭证上传；已上传身份保留在表单中供保存、提交和重试使用。 */
export function SalesOrderCreateEvidenceField({
    form,
    locked = false,
}: {
    form: SalesOrderCreateFormApi
    locked?: boolean
}) {
    const upload = useSalesOrderEvidenceUpload(form)
    return (
        <form.AppField name="evidenceAttachments">
            {(field) => {
                const invalid =
                    field.state.meta.isTouched && !field.state.meta.isValid
                const files = field.state.value ?? []
                return (
                    <Field data-invalid={invalid || undefined}>
                        <FieldLabel htmlFor="sales-orders-create-evidence-input">
                            开单材料
                            <RequiredMark />
                        </FieldLabel>
                        {files.length > 0 ? (
                            <ul className="divide-y divide-border/60 rounded-lg border border-border/80 bg-card">
                                {files.map(
                                    (file: {
                                        id: string
                                        fileName: string
                                    }) => (
                                        <li
                                            key={file.id}
                                            className="flex items-center justify-between gap-3 px-3 py-2"
                                        >
                                            <div className="flex min-w-0 items-center gap-2.5">
                                                <FileTextIcon
                                                    className="size-4 shrink-0 text-muted-foreground"
                                                    aria-hidden="true"
                                                />
                                                <div className="min-w-0">
                                                    <p className="truncate font-medium text-foreground">
                                                        {file.fileName}
                                                    </p>
                                                    <p className="text-xs text-muted-foreground">
                                                        {locked
                                                            ? fileUploadText.saved
                                                            : fileUploadText.uploadedUnsaved}
                                                    </p>
                                                </div>
                                            </div>
                                            {!locked ? (
                                                <Button
                                                    id={`sales-orders-create-evidence-${toAutomationIdSegment(file.id)}-remove`}
                                                    type="button"
                                                    variant="ghost"
                                                    size="icon-sm"
                                                    className={
                                                        detailWorkspaceStyles.quietDangerButton
                                                    }
                                                    disabled={upload.isPending}
                                                    aria-label={`移除 ${file.fileName}`}
                                                    onClick={() =>
                                                        field.handleChange(
                                                            files.filter(
                                                                (item: {
                                                                    id: string
                                                                    fileName: string
                                                                }) =>
                                                                    item.id !==
                                                                    file.id,
                                                            ),
                                                        )
                                                    }
                                                >
                                                    <Trash2Icon aria-hidden="true" />
                                                </Button>
                                            ) : null}
                                        </li>
                                    ),
                                )}
                            </ul>
                        ) : null}
                        {!locked ? (
                            <FileUpload
                                maxFileSize={5 * 1024 * 1024}
                                idPrefix="sales-orders-create-evidence"
                                accept="application/pdf,image/jpeg,image/png,image/webp,.pdf,.jpg,.jpeg,.png,.webp"
                                multiple={false}
                                density="compact"
                                controlClassName="border-border/80 bg-surface-sunken/70 px-3 py-2.5"
                                disabled={upload.isPending}
                                selectedFiles={
                                    (upload.isPending || upload.isError) &&
                                    upload.variables
                                        ? [upload.variables]
                                        : []
                                }
                                selectedDescription={
                                    upload.isPending
                                        ? "正在上传…"
                                        : "上传失败 · 可重试原文件"
                                }
                                onFileRemove={() => upload.reset()}
                                label={
                                    upload.isPending
                                        ? "正在上传凭证…"
                                        : "添加开单凭证"
                                }
                                description="支持 PDF、JPG、PNG、WebP，单文件不超过 5 MiB"
                                onFilesSelected={(selected) => {
                                    const file = selected[0]
                                    if (file) upload.mutate(file)
                                    field.handleBlur()
                                }}
                                aria-invalid={invalid || undefined}
                                aria-describedby="sales-orders-create-evidence-description"
                            />
                        ) : null}
                        {upload.isError && !locked ? (
                            <div className="flex flex-wrap items-center gap-3">
                                <p
                                    role="alert"
                                    className="text-xs text-destructive"
                                >
                                    {getErrorMessage(
                                        upload.error,
                                        "上传失败，请重试",
                                    )}
                                </p>
                                <Button
                                    id="sales-orders-create-evidence-retry"
                                    type="button"
                                    variant="outline"
                                    size="xs"
                                    disabled={!upload.variables}
                                    onClick={() => {
                                        if (upload.variables)
                                            upload.mutate(upload.variables)
                                    }}
                                >
                                    重试原文件
                                </Button>
                            </div>
                        ) : null}
                        <FieldDescription id="sales-orders-create-evidence-description">
                            {locked
                                ? "创建销售单时上传的开单依据已保留。"
                                : "上传订单确认单、未签章合同等业务凭证；签完合同后可在销售单详情补录。"}
                        </FieldDescription>
                        {invalid ? (
                            <FieldError
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        ) : null}
                    </Field>
                )
            }}
        </form.AppField>
    )
}
