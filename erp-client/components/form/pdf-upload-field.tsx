"use client"

import { useFieldContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"
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
            <FileUpload
                idPrefix={resolvedId}
                accept="application/pdf,.pdf"
                multiple={false}
                maxFileSize={20 * 1024 * 1024}
                disabled={disabled}
                selectedFiles={file ? [file] : []}
                onFileRemove={() => field.handleChange(null)}
                label="上传合同 PDF"
                description="点击选择文件，或将单个 PDF 拖到此处"
                onFilesSelected={(files) => {
                    field.handleChange(files[0] ?? null)
                    field.handleBlur()
                }}
                aria-describedby={describedBy || undefined}
                aria-invalid={isInvalid || undefined}
            />
            {description ? (
                <FieldDescription id={descriptionId}>
                    {description}
                </FieldDescription>
            ) : null}
            {isInvalid ? <FieldError id={errorId} errors={errors} /> : null}
        </Field>
    )
}
