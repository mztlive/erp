"use client"

import * as React from "react"
import { z } from "zod"

import { DatePicker } from "@/components/ui/date-picker"
import { FileUpload, imagePreviewSource } from "@/components/ui/file-upload"
import { Label } from "@/components/ui/label"
import { toast } from "@/components/ui/toast"
import { pendingFileReference } from "@/features/master-data/api/pending-assets"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import type {
    BrandFields,
    MasterDataMutationResult,
    PendingAssetUpload,
} from "@/features/master-data/types"
import { getErrorMessage } from "@/lib/api/errors"

export function newIdempotencyKey(prefix: string): string {
    return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

export function notifySuccess(
    title: string,
    result: Extract<MasterDataMutationResult, { outcome: "succeeded" }>,
) {
    toast.add({
        title,
        description: `${masterDataCopy.resultNo} ${result.stableNo} · v${result.revisionNo}`,
        type: "success",
        timeout: 4000,
    })
}

export function prepareBrandLogoFields(
    fields: BrandFields,
    pendingFile: File | undefined,
    existingAssetId: string,
    existingUrl: string,
): {
    fields: BrandFields
    pendingAssetUploads: readonly PendingAssetUpload[]
} {
    if (!fields.logo) {
        return {
            fields: {
                ...fields,
                logoAssetId: undefined,
                logoPreviewUrl: undefined,
            },
            pendingAssetUploads: [],
        }
    }
    if (pendingFile) {
        const reference = pendingFileReference("brand", "logo")
        return {
            fields: {
                ...fields,
                logoAssetId: reference,
            },
            pendingAssetUploads: [{ reference, file: pendingFile }],
        }
    }
    return {
        fields: {
            ...fields,
            logoAssetId: existingAssetId || undefined,
            logoPreviewUrl: existingUrl || undefined,
        },
        pendingAssetUploads: [],
    }
}

export type FieldApi = {
    TextField: React.ComponentType<{ label: string }>
    TextareaField: React.ComponentType<{ label: string }>
    SelectField: React.ComponentType<{
        label: string
        options: readonly { value: string; label: string }[]
        allowClear?: boolean
        placeholder?: string
    }>
    state: {
        value: string
        meta: { errors: readonly unknown[]; isTouched: boolean }
    }
    handleChange: (value: string) => void
    handleBlur: () => void
}

export function DateField({
    label,
    field,
    id,
    required,
}: {
    label: string
    field: FieldApi
    id: string
    required?: boolean
}) {
    const error = field.state.meta.errors[0]
    return (
        <div className="space-y-1.5">
            <Label htmlFor={id}>
                {label}
                {required ? <span className="text-destructive">*</span> : null}
            </Label>
            <DatePicker
                id={id}
                value={field.state.value || undefined}
                onValueChange={(next) => field.handleChange(next ?? "")}
                className="w-full"
                aria-invalid={Boolean(error)}
            />
            {error ? (
                <p className="text-xs text-destructive" role="alert">
                    {getErrorMessage(error, "日期未通过检查，请重新选择。")}
                </p>
            ) : null}
        </div>
    )
}

export function MediaSingleField({
    id,
    label,
    hint,
    value,
    onChange,
    required,
    selectedHint = "已选择",
    aspectRatio,
    previewUrl,
    onFilesSelected,
}: {
    id?: string
    label: string
    hint?: string
    value: string
    onChange: (next: string) => void
    required?: boolean
    selectedHint?: string
    aspectRatio?: "1:1"
    previewUrl?: string
    onFilesSelected?: (files: File[]) => void
}) {
    const [file, setFile] = React.useState<File | null>(null)
    return (
        <div className="space-y-2">
            <Label htmlFor={id ? `${id}-input` : undefined}>
                {label}
                {required ? (
                    <span className="ml-1 text-destructive">*</span>
                ) : null}
            </Label>
            <FileUpload
                idPrefix={id}
                label={label}
                description={hint ?? masterDataCopy.mediaUploadHint}
                density={aspectRatio === "1:1" ? "tile" : "compact"}
                className={aspectRatio === "1:1" ? "w-24" : undefined}
                controlClassName={
                    aspectRatio === "1:1" ? "aspect-square" : undefined
                }
                accept="image/jpeg,image/png,image/webp"
                multiple={false}
                previewSelectedImage
                selectedImageFile={file?.name === value ? file : null}
                preview={
                    value
                        ? {
                              src:
                                  previewUrl?.trim() ||
                                  imagePreviewSource(value),
                              name: value,
                          }
                        : null
                }
                onPreviewRemove={() => {
                    setFile(null)
                    onChange("")
                }}
                onFilesSelected={(files) => {
                    if (!files[0]) return
                    setFile(files[0])
                    onFilesSelected?.(files)
                    onChange(files[0].name)
                }}
            />
            {value ? (
                <p className="text-xs text-muted-foreground">{selectedHint}</p>
            ) : null}
        </div>
    )
}

export const disableSchema = z.object({
    changeReason: z.string().trim().min(2, "请填写停用原因"),
    effectiveFrom: z
        .string()
        .min(1, "请填写停用时间")
        .refine(
            (value) => /^\d{4}-\d{2}-\d{2}$/.test(value),
            "停用时间格式不正确，请使用 YYYY-MM-DD",
        ),
})

export function DialogScrollBody({ children }: { children: React.ReactNode }) {
    return <div className="min-h-0 flex-1 overflow-y-auto pr-1">{children}</div>
}
