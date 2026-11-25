"use client"

import type { ReactNode } from "react"

import { useFieldA11y } from "@/components/form/field-a11y"
import { FieldFrame } from "@/components/form/field-frame"
import { FileUpload } from "@/components/ui/file-upload"
import { fileUploadText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

function defaultMultiFileKey(file: File): string {
    return `${file.name}:${file.size}:${file.lastModified}`
}

export type MultiFileFieldProps = {
    label: string
    hideLabel?: boolean
    description?: ReactNode
    className?: string
    density?: "default" | "compact" | "button"
    disabled?: boolean
    required?: boolean
    id?: string
    accept?: string
    maxFileSize?: number
    /** 允许选择的最多文件数量。 */
    maxFiles?: number
    /** 自定义文件去重键；默认按文件名、大小与修改时间生成。 */
    fileKey?: (file: File) => string
    /** 接收合并后的候选文件列表，返回错误文案时拒绝本批次选择。 */
    validateFiles?: (files: readonly File[]) => string | null | undefined
    uploadLabel?: string
    uploadDescription?: string
    selectedDescription?: string
    onFilesChange?: (files: File[]) => void
}

/**
 * 绑定 TanStack Form field 的通用多文件上传字段。
 * 内置去重合并、数量上限与批次校验，不含任何页面专属默认文案或 ID。
 */
export function MultiFileField({
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
    maxFiles,
    fileKey = defaultMultiFileKey,
    validateFiles,
    uploadLabel = fileUploadText.upload,
    uploadDescription = fileUploadText.hint,
    selectedDescription = fileUploadText.pendingSubmit,
    onFilesChange,
}: MultiFileFieldProps) {
    const {
        field,
        isInvalid,
        errors,
        resolvedId,
        descriptionId,
        errorId,
        describedBy,
    } = useFieldA11y<File[]>(description, id)
    const files = field.state.value ?? []

    const mergeFiles = (
        current: readonly File[],
        incoming: readonly File[],
    ) => {
        const merged = new Map(current.map((file) => [fileKey(file), file]))
        for (const file of incoming) {
            merged.set(fileKey(file), file)
        }
        return [...merged.values()]
    }

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
                multiple
                maxFileSize={maxFileSize}
                disabled={disabled}
                selectedFiles={files}
                fileKey={fileKey}
                validateFiles={(selected) => {
                    const merged = mergeFiles(files, selected)
                    if (maxFiles !== undefined && merged.length > maxFiles) {
                        return `最多上传 ${maxFiles} 个文件`
                    }
                    return validateFiles?.(merged)
                }}
                onFileRemove={(removed) => {
                    const removedKey = fileKey(removed)
                    const next = files.filter(
                        (item) => fileKey(item) !== removedKey,
                    )
                    field.handleChange(next)
                    field.handleBlur()
                    onFilesChange?.(next)
                }}
                label={uploadLabel}
                description={uploadDescription}
                onFilesSelected={(selected) => {
                    const next = mergeFiles(files, selected)
                    field.handleChange(next)
                    field.handleBlur()
                    onFilesChange?.(next)
                }}
                aria-describedby={describedBy || undefined}
                aria-invalid={isInvalid || undefined}
            />
        </FieldFrame>
    )
}
