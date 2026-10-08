"use client"

import { Trash2Icon } from "lucide-react"

import { toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { FileUpload } from "@/components/ui/file-upload"
import type { SalesOrderCreateFormApi } from "@/features/sales-orders/lib/sales-order-create-form-types"
import { useSalesOrderEvidenceUpload } from "@/features/sales-orders/hooks/use-sales-order-evidence"
import { toAutomationIdSegment } from "@/lib/automation-id"

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
                return (
                    <Field data-invalid={invalid || undefined}>
                        <FieldLabel htmlFor="sales-orders-create-evidence-input">
                            开单材料<span className="text-destructive">*</span>
                        </FieldLabel>
                        {(field.state.value ?? []).length > 0 ? (
                            <ul className="space-y-2">
                                {(field.state.value ?? []).map(
                                    (file: {
                                        id: string
                                        fileName: string
                                    }) => (
                                        <li
                                            key={file.id}
                                            className="flex items-center justify-between gap-3 rounded-md border px-3 py-2 text-sm"
                                        >
                                            <span className="min-w-0 break-all">
                                                {file.fileName}
                                            </span>
                                            {!locked ? (
                                                <Button
                                                    id={`sales-orders-create-evidence-${toAutomationIdSegment(file.id)}-remove`}
                                                    type="button"
                                                    variant="ghost"
                                                    size="icon-sm"
                                                    disabled={upload.isPending}
                                                    aria-label={`移除 ${file.fileName}`}
                                                    onClick={() =>
                                                        field.handleChange(
                                                            (
                                                                field.state
                                                                    .value ?? []
                                                            ).filter(
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
                                idPrefix="sales-orders-create-evidence"
                                accept="application/pdf,image/jpeg,image/png,image/webp,.pdf,.jpg,.jpeg,.png,.webp"
                                multiple={false}
                                density="compact"
                                disabled={upload.isPending}
                                label={
                                    upload.isPending
                                        ? "正在上传凭证…"
                                        : "添加开单凭证"
                                }
                                description="支持 PDF、JPG、PNG、WebP，每个文件不超过 5 MiB。"
                                onFilesSelected={(files) => {
                                    const file = files[0]
                                    if (file) upload.mutate(file)
                                    field.handleBlur()
                                }}
                                aria-invalid={invalid || undefined}
                                aria-describedby="sales-orders-create-evidence-description"
                            />
                        ) : null}
                        <FieldDescription id="sales-orders-create-evidence-description">
                            {locked
                                ? "创建销售单时上传的开单依据已保留。"
                                : "上传订单确认、未完成签署的合同或其他业务凭证；签署完成后，在销售单详情补录合同。"}
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
