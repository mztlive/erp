"use client"

import { SingleFileField } from "@/components/form/single-file-field"
import { fileUploadText } from "@/lib/ui-text"

/** 业务凭证允许的文件类型；与服务端凭证校验一致。 */
export const EVIDENCE_FILE_TYPES = [
    "application/pdf",
    "image/jpeg",
    "image/png",
    "image/webp",
] as const

/** 业务凭证单文件上限 5 MB。 */
export const EVIDENCE_FILE_MAX_BYTES = 5 * 1024 * 1024

type EvidenceUploadFieldProps = {
    label: string
    description?: string
    uploadLabel?: string
    disabled?: boolean
    required?: boolean
    id?: string
}

/** 绑定 TanStack Form 的单个凭证上传字段（图片或 PDF）。 */
export function EvidenceUploadField({
    label,
    description = "支持 PDF、JPG、PNG 或 WebP，单个文件不超过 5 MB。",
    uploadLabel = "上传凭证",
    disabled,
    required,
    id,
}: EvidenceUploadFieldProps) {
    return (
        <SingleFileField
            label={label}
            description={description}
            disabled={disabled}
            required={required}
            id={id}
            accept={EVIDENCE_FILE_TYPES.join(",")}
            maxFileSize={EVIDENCE_FILE_MAX_BYTES}
            uploadLabel={uploadLabel}
            uploadDescription="点击选择文件，或将单个文件拖到此处"
            selectedDescription={fileUploadText.pendingSubmit}
        />
    )
}
