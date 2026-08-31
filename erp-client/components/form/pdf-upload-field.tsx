"use client"

import { FileTextIcon, Trash2Icon } from "lucide-react"

import { useFieldContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"
import {
    Attachment,
    AttachmentAction,
    AttachmentActions,
    AttachmentContent,
    AttachmentDescription,
    AttachmentGroup,
    AttachmentMedia,
    AttachmentTitle,
} from "@/components/ui/attachment"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { FileUpload } from "@/components/ui/file-upload"

type PdfUploadFieldProps = {
    label: string
    hideLabel?: boolean
    description?: string
    disabled?: boolean
    required?: boolean
    id?: string
}

function formatFileSize(size: number): string {
    if (size < 1024 * 1024) return `${Math.ceil(size / 1024)} KB`
    return `${(size / (1024 * 1024)).toFixed(1)} MB`
}

/** 绑定 TanStack Form 的单 PDF 上传字段。 */
export function PdfUploadField({
    label,
    hideLabel = false,
    description = "仅支持单个 PDF，文件不超过 20 MB。",
    disabled,
    required,
    id,
}: PdfUploadFieldProps) {
    const field = useFieldContext<File | null>()
    const file = field.state.value
    const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid
    const errors = toFieldErrors(field.state.meta.errors)
    const resolvedId = id ?? field.name
    const descriptionId = `${resolvedId}-description`
    const errorId = `${resolvedId}-error`
    const describedBy = [
        description ? descriptionId : undefined,
        isInvalid ? errorId : undefined,
    ]
        .filter(Boolean)
        .join(" ")

    return (
        <Field data-invalid={isInvalid || undefined}>
            <FieldLabel
                htmlFor={`${resolvedId}-input`}
                className={hideLabel ? "sr-only" : undefined}
            >
                {label}
                {required ? <span className="text-destructive">*</span> : null}
            </FieldLabel>
            {file ? (
                <AttachmentGroup>
                    <Attachment aria-label={file.name}>
                        <AttachmentMedia>
                            <FileTextIcon aria-hidden="true" />
                        </AttachmentMedia>
                        <AttachmentContent>
                            <AttachmentTitle>{file.name}</AttachmentTitle>
                            <AttachmentDescription>
                                PDF · {formatFileSize(file.size)}
                            </AttachmentDescription>
                        </AttachmentContent>
                        <AttachmentActions>
                            <AttachmentAction
                                id={`${resolvedId}-remove`}
                                type="button"
                                variant="destructive"
                                disabled={disabled}
                                onClick={() => field.handleChange(null)}
                                aria-label={`移除 ${file.name}`}
                            >
                                <Trash2Icon aria-hidden="true" />
                            </AttachmentAction>
                        </AttachmentActions>
                    </Attachment>
                </AttachmentGroup>
            ) : (
                <FileUpload
                    idPrefix={resolvedId}
                    accept="application/pdf,.pdf"
                    multiple={false}
                    disabled={disabled}
                    label="上传合同 PDF"
                    description="点击选择文件，或将单个 PDF 拖到此处"
                    onFilesSelected={(files) => {
                        field.handleChange(files[0] ?? null)
                        field.handleBlur()
                    }}
                    aria-describedby={describedBy || undefined}
                    aria-invalid={isInvalid || undefined}
                />
            )}
            {description ? (
                <FieldDescription id={descriptionId}>
                    {description}
                </FieldDescription>
            ) : null}
            {isInvalid ? <FieldError id={errorId} errors={errors} /> : null}
        </Field>
    )
}
