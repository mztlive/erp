"use client"

import type { ReactNode } from "react"

import { useFieldA11y } from "@/components/form/field-a11y"
import { FieldFrame } from "@/components/form/field-frame"
import { FileUpload } from "@/components/ui/file-upload"
import { cn } from "@/lib/utils"

export type SingleFileFieldProps = {
    label: string
    hideLabel?: boolean
    description?: ReactNode
    className?: string
    density?: "default" | "compact"
    disabled?: boolean
    required?: boolean
    id?: string
    accept: string
    maxFileSize: number
    uploadLabel: string
    uploadDescription: string
    selectedDescription?: string
}

/** 绑定 TanStack Form 的单文件上传字段；业务侧只提供文件类型与文案。 */
export function SingleFileField({
    label,
    hideLabel = false,
    description,
    className,
    density = "default",
    disabled,
    required,
    id,
    accept,
    maxFileSize,
    uploadLabel,
    uploadDescription,
    selectedDescription,
}: SingleFileFieldProps) {
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
        <FieldFrame
            className={cn(className)}
            label={label}
            labelClassName={hideLabel ? "sr-only" : undefined}
            htmlFor={`${resolvedId}-input`}
            required={required}
            description={description}
            descriptionId={descriptionId}
            isInvalid={isInvalid}
            disabled={disabled}
            errorId={errorId}
            errors={errors}
        >
            <FileUpload
                selectedDescription={selectedDescription}
                density={density}
                idPrefix={resolvedId}
                name={field.name}
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
        </FieldFrame>
    )
}
