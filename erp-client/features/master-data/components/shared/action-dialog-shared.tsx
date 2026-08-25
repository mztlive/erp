"use client"

import * as React from "react"
import { ImageIcon } from "lucide-react"
import { z } from "zod"

import { Button } from "@/components/ui/button"
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
import { cn } from "@/lib/utils"

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
}: {
    label: string
    field: FieldApi
    id: string
}) {
    const error = field.state.meta.errors[0]
    return (
        <div className="space-y-1.5">
            <Label htmlFor={id}>{label}</Label>
            <DatePicker
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
    const isSquare = aspectRatio === "1:1"
    const [localPreview, setLocalPreview] = React.useState<string | null>(null)
    const localPreviewRef = React.useRef<string | null>(null)
    React.useEffect(
        () => () => {
            if (localPreviewRef.current) {
                URL.revokeObjectURL(localPreviewRef.current)
                localPreviewRef.current = null
            }
        },
        [],
    )
    const previewSrc =
        localPreview ?? previewUrl?.trim() ?? imagePreviewSource(value)
    return (
        <div className="space-y-2">
            <div className="flex items-center justify-between gap-2">
                <Label className="text-sm font-medium">
                    {label}
                    {required ? (
                        <span className="ml-1 text-destructive">*</span>
                    ) : null}
                </Label>
                {value ? (
                    <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        onClick={() => {
                            if (localPreviewRef.current) {
                                URL.revokeObjectURL(localPreviewRef.current)
                                localPreviewRef.current = null
                            }
                            setLocalPreview(null)
                            onChange("")
                        }}
                    >
                        {masterDataCopy.mediaRemove}
                    </Button>
                ) : null}
            </div>
            {value ? (
                isSquare ? (
                    <div className="flex items-start gap-3">
                        <div
                            className="flex size-24 shrink-0 items-center justify-center gap-1 overflow-hidden rounded-lg border border-border bg-surface-sunken aspect-square"
                            aria-label={`${label} 预览 1:1`}
                        >
                            {previewSrc ? (
                                // eslint-disable-next-line @next/next/no-img-element -- 本地待上传图片使用 blob URL。
                                <img
                                    src={previewSrc}
                                    alt={value}
                                    className="size-full object-cover"
                                />
                            ) : (
                                <>
                                    <ImageIcon
                                        className="size-8 text-muted-foreground"
                                        aria-hidden
                                    />
                                    <span className="text-2xs text-muted-foreground">
                                        1:1
                                    </span>
                                </>
                            )}
                        </div>
                        <div className="min-w-0 flex-1 pt-1">
                            <div className="truncate text-sm font-medium">
                                {value}
                            </div>
                            <div className="text-xs text-muted-foreground">
                                {selectedHint}
                            </div>
                            <div className="mt-1 text-xs text-muted-foreground">
                                比例 1:1
                            </div>
                        </div>
                    </div>
                ) : (
                    <div className="flex items-center gap-3 rounded-md border border-border bg-surface-sunken px-3 py-2">
                        <div className="flex size-10 items-center justify-center overflow-hidden rounded-md bg-muted">
                            {previewSrc ? (
                                // eslint-disable-next-line @next/next/no-img-element -- 本地待上传图片使用 blob URL。
                                <img
                                    src={previewSrc}
                                    alt={value}
                                    className="size-full object-cover"
                                />
                            ) : (
                                <ImageIcon
                                    className="size-5 text-muted-foreground"
                                    aria-hidden
                                />
                            )}
                        </div>
                        <div className="min-w-0 flex-1">
                            <div className="truncate text-sm font-medium">
                                {value}
                            </div>
                            <div className="text-xs text-muted-foreground">
                                {selectedHint}
                            </div>
                        </div>
                    </div>
                )
            ) : (
                <FileUpload
                    accept="image/jpeg,image/png,image/webp"
                    multiple={false}
                    label={label}
                    description={hint ?? masterDataCopy.mediaUploadHint}
                    previewSelectedImage
                    onFilesSelected={(files) => {
                        onFilesSelected?.(files)
                        const file = files[0]
                        if (!file) return
                        if (localPreviewRef.current) {
                            URL.revokeObjectURL(localPreviewRef.current)
                        }
                        const blobUrl = URL.createObjectURL(file)
                        localPreviewRef.current = blobUrl
                        setLocalPreview(blobUrl)
                        onChange(file.name)
                    }}
                    className={cn(
                        "p-4",
                        isSquare &&
                            "mx-auto aspect-square max-w-[10rem] justify-center",
                    )}
                />
            )}
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
