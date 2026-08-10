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
    description?: string
    disabled?: boolean
}

function formatFileSize(size: number): string {
    if (size < 1024 * 1024) return `${Math.ceil(size / 1024)} KB`
    return `${(size / (1024 * 1024)).toFixed(1)} MB`
}

/** 绑定 TanStack Form 的单 PDF 上传字段。 */
export function PdfUploadField({
    label,
    description = "仅支持单个 PDF，文件不超过 20 MB。",
    disabled,
}: PdfUploadFieldProps) {
    const field = useFieldContext<File | null>()
    const file = field.state.value
    const isInvalid = field.state.meta.isTouched && !field.state.meta.isValid
    const errors = toFieldErrors(field.state.meta.errors)

    return (
        <Field data-invalid={isInvalid || undefined}>
            <FieldLabel>{label}</FieldLabel>
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
                    accept="application/pdf,.pdf"
                    multiple={false}
                    disabled={disabled}
                    label="上传合同 PDF"
                    description="点击选择文件，或将单个 PDF 拖到此处"
                    onFilesSelected={(files) => {
                        field.handleChange(files[0] ?? null)
                        field.handleBlur()
                    }}
                />
            )}
            {description ? (
                <FieldDescription>{description}</FieldDescription>
            ) : null}
            {isInvalid ? <FieldError errors={errors} /> : null}
        </Field>
    )
}
