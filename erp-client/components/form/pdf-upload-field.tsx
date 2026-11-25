"use client"

import type { ReactNode } from "react"

import { SingleFileField } from "@/components/form/single-file-field"

export type PdfUploadFieldProps = {
    label: string
    hideLabel?: boolean
    description?: ReactNode
    className?: string
    density?: "default" | "compact"
    uploadLabel?: string
    selectedDescription?: string
    disabled?: boolean
    required?: boolean
    id?: string
}

/** 绑定 TanStack Form 的单 PDF 上传字段。 */
export function PdfUploadField({
    label,
    hideLabel = false,
    description = "仅支持单个 PDF，文件不超过 20 MB。",
    className,
    density = "default",
    uploadLabel = "上传 PDF",
    selectedDescription,
    disabled,
    required,
    id,
}: PdfUploadFieldProps) {
    return (
        <SingleFileField
            label={label}
            hideLabel={hideLabel}
            description={description}
            className={className}
            density={density}
            disabled={disabled}
            required={required}
            id={id}
            accept="application/pdf,.pdf"
            maxFileSize={20 * 1024 * 1024}
            uploadLabel={uploadLabel}
            uploadDescription="点击选择文件，或将单个 PDF 拖到此处"
            selectedDescription={selectedDescription}
        />
    )
}
