"use client"

import { FileIcon, Trash2Icon } from "lucide-react"

import {
    Attachment,
    AttachmentAction,
    AttachmentActions,
    AttachmentContent,
    AttachmentGroup,
    AttachmentMedia,
    AttachmentTitle,
} from "@/components/ui/attachment"
import { FileUpload } from "@/components/ui/file-upload"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    INVOICE_FILE_ACCEPT,
    invoiceFileReference,
} from "@/features/customer-receivables/lib/invoice-files"

/** 受控的开票附件选择；文件与登记命令同时上传。 */
export function InvoiceFilesField({
    files,
    onChange,
    disabled,
    errors,
}: {
    files: readonly File[]
    onChange: (files: File[]) => void
    disabled: boolean
    errors: Array<{ message?: string } | undefined>
}) {
    const baseId = "customer-receivables-session-invoice-files"
    const invalid = errors.length > 0
    return (
        <Field data-invalid={invalid || undefined}>
            <FieldLabel htmlFor={`${baseId}-input`}>
                发票附件（可选）
            </FieldLabel>
            {files.length > 0 ? (
                <AttachmentGroup>
                    {files.map((file) => {
                        const reference = invoiceFileReference(file)
                        return (
                            <Attachment key={reference} aria-label={file.name}>
                                <AttachmentMedia>
                                    <FileIcon aria-hidden="true" />
                                </AttachmentMedia>
                                <AttachmentContent>
                                    <AttachmentTitle>
                                        {file.name}
                                    </AttachmentTitle>
                                </AttachmentContent>
                                <AttachmentActions>
                                    <AttachmentAction
                                        id={`${baseId}-${toAutomationIdSegment(reference)}-remove`}
                                        type="button"
                                        variant="destructive"
                                        disabled={disabled}
                                        aria-label={`移除 ${file.name}`}
                                        onClick={() =>
                                            onChange(
                                                files.filter(
                                                    (item) =>
                                                        invoiceFileReference(
                                                            item,
                                                        ) !== reference,
                                                ),
                                            )
                                        }
                                    >
                                        <Trash2Icon aria-hidden="true" />
                                    </AttachmentAction>
                                </AttachmentActions>
                            </Attachment>
                        )
                    })}
                </AttachmentGroup>
            ) : null}
            <FileUpload
                idPrefix={baseId}
                accept={INVOICE_FILE_ACCEPT}
                multiple
                disabled={disabled}
                label="添加发票附件"
                description="支持 PDF、JPG、PNG、WebP，最多 32 个，单个不超过 5 MB"
                aria-describedby={`${baseId}-description${invalid ? ` ${baseId}-error` : ""}`}
                aria-invalid={invalid || undefined}
                onFilesSelected={(selected) => {
                    const next = new Map(
                        files.map((file) => [invoiceFileReference(file), file]),
                    )
                    for (const file of selected) {
                        next.set(invoiceFileReference(file), file)
                    }
                    onChange([...next.values()])
                }}
            />
            <FieldDescription id={`${baseId}-description`}>
                登记成功后，销售单详情可下载已上传的发票。
            </FieldDescription>
            {invalid ? (
                <FieldError id={`${baseId}-error`} errors={errors} />
            ) : null}
        </Field>
    )
}
