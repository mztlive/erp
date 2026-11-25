"use client"

import { RequiredMark } from "@/components/ui/required-mark"
import { fileUploadText } from "@/lib/ui-text"
import { FileUpload } from "@/components/ui/file-upload"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import {
    INVOICE_FILE_ACCEPT,
    invoiceFileReference,
    invoiceFilesError,
} from "@/lib/invoice-files"

/**
 * 受控的开票附件选择；文件与登记命令同时上传。
 *
 * 兼容说明：默认 `idPrefix` 暂保留 `"customer-receivables-session-invoice-files"`，
 * 因为 `features/customer-receivables/components/session-fact-fields.tsx` 未显式传参，
 * 且 `features/customer-receivables/hooks/use-allocation-session.ts` 与
 * `e2e/helpers/invoices.ts` 均依赖 `#customer-receivables-session-invoice-files-input`。
 * 新调用方请始终显式传入页面专属 `idPrefix`；待 Phase 4-D 迁移完调用方后再移除旧默认值。
 */
export function InvoiceFilesField({
    files,
    onChange,
    disabled,
    errors,
    label,
    uploadLabel = "添加发票附件",
    fileKind = "发票附件",
    density = "default",
    idPrefix = "customer-receivables-session-invoice-files",
    description = "",
    required = false,
}: {
    /** 建议显式传入页面唯一前缀；缺省暂保留历史兼容默认值以免破坏 E2E 选择器。 */
    idPrefix?: string
    description?: string
    density?: "default" | "compact"
    label?: string
    uploadLabel?: string
    fileKind?: string
    files: readonly File[]
    onChange: (files: File[]) => void
    disabled: boolean
    errors: Array<{ message?: string } | undefined>
    required?: boolean
}) {
    const baseId = idPrefix
    const invalid = errors.length > 0
    const resolvedLabel = label ?? (required ? "发票附件" : "发票附件（可选）")
    const hasDescription = description.trim().length > 0
    const describedBy =
        [
            hasDescription ? `${baseId}-description` : undefined,
            invalid ? `${baseId}-error` : undefined,
        ]
            .filter(Boolean)
            .join(" ") || undefined

    return (
        <Field
            data-invalid={invalid || undefined}
            data-disabled={disabled ? "true" : undefined}
        >
            <FieldLabel htmlFor={`${baseId}-input`}>
                {resolvedLabel}
                {required ? <RequiredMark /> : null}
            </FieldLabel>
            <FileUpload
                selectedDescription={fileUploadText.pendingSubmit}
                idPrefix={baseId}
                density={density}
                accept={INVOICE_FILE_ACCEPT}
                multiple
                validateFiles={(selected) => {
                    const next = new Map(
                        files.map((file) => [invoiceFileReference(file), file]),
                    )
                    for (const file of selected)
                        next.set(invoiceFileReference(file), file)
                    return invoiceFilesError([...next.values()], fileKind)
                }}
                selectedFiles={files}
                fileKey={invoiceFileReference}
                onFileRemove={(file) =>
                    onChange(
                        files.filter(
                            (item) =>
                                invoiceFileReference(item) !==
                                invoiceFileReference(file),
                        ),
                    )
                }
                maxFileSize={5 * 1024 * 1024}
                disabled={disabled}
                label={uploadLabel}
                description="支持 PDF、JPG、PNG、WebP，最多 32 个，单个不超过 5 MB"
                aria-describedby={describedBy}
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
            {hasDescription ? (
                <FieldDescription id={`${baseId}-description`}>
                    {description}
                </FieldDescription>
            ) : null}
            {invalid ? (
                <FieldError id={`${baseId}-error`} errors={errors} />
            ) : null}
        </Field>
    )
}
