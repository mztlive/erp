"use client"

import { useFieldA11y } from "@/components/form/field-a11y"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { FileUpload } from "@/components/ui/file-upload"

/** 绑定 TanStack Form 的单文件上传字段；业务侧只提供文件类型与文案。 */
export function SingleFileField({
    label,
    hideLabel = false,
    description,
    disabled,
    required,
    id,
    accept,
    maxFileSize,
    uploadLabel,
    uploadDescription,
    selectedDescription,
}: {
    label: string
    hideLabel?: boolean
    description?: string
    disabled?: boolean
    required?: boolean
    id?: string
    accept: string
    maxFileSize: number
    uploadLabel: string
    uploadDescription: string
    selectedDescription?: string
}) {
    const {
        field,
        isInvalid,
        errors,
        resolvedId,
        descriptionId,
        errorId,
        describedBy,
    } = useFieldA11y<File | null>(description, id)
    const file = field.state.value

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
                selectedDescription={selectedDescription}
                idPrefix={resolvedId}
                accept={accept}
                multiple={false}
                maxFileSize={maxFileSize}
                disabled={disabled}
                selectedFiles={file ? [file] : []}
                onFileRemove={() => field.handleChange(null)}
                label={uploadLabel}
                description={uploadDescription}
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
