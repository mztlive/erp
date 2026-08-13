"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { ImageIcon } from "lucide-react"
import { z } from "zod"

import {
    CategoryCombobox,
    DiscardConfirmDialog,
    FormalActionResult,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { DatePicker } from "@/components/ui/date-picker"
import {
    Dialog,
    DialogClose,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { FileUpload, imagePreviewSource } from "@/components/ui/file-upload"
import { Label } from "@/components/ui/label"
import { toast } from "@/components/ui/toast"
import { uploadFileAssetImage } from "@/features/file-assets/api"
import { MediaListField } from "@/features/master-data/components/shared/media-list-field"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    WAREHOUSE_WRITE_MESSAGE,
    resourceLabel,
} from "@/features/master-data/lib/data"
import {
    RESOURCE_FIELDS,
    buildResourceFields,
    buildResourceSchema,
    currentResourceFieldValues,
    defaultImmediateEffectiveFrom,
    emptyResourceFieldValues,
    usesEffectivePeriod,
    usesWideDialog,
    type ResourceFieldDef,
    type ResourceFormValues,
} from "@/features/master-data/lib/resource-fields"
import {
    collectDescendantIds,
    buildCategoryForest,
    toCategoryComboboxItems,
} from "@/features/master-data/lib/category-tree-model"
import {
    masterDataKeys,
    useCreateMasterDataMutation,
    useCreateRevisionMutation,
    useDisableMasterDataMutation,
    useMasterDataListQuery,
} from "@/features/master-data/hooks/queries"
import type {
    MasterDataCenterView,
    MasterDataListItem,
    MasterDataMutationResult,
    MasterDataResource,
} from "@/features/master-data/types"
import type { BrandFields } from "@/features/master-data/types"
import { cn } from "@/lib/utils"
import { getErrorMessage } from "@/lib/api/errors"

export function newIdempotencyKey(prefix: string): string {
    return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

/** 编辑成功后以 toast 提示结果，不再在 Dialog 内展示结果面板。 */
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

/** 品牌 Logo 保存前上传：返回回填 asset id / URL 后的品牌字段。 */
export async function resolveBrandLogoFields(
    fields: BrandFields,
    pendingFile: File | undefined,
    existingAssetId: string,
    existingUrl: string,
): Promise<BrandFields> {
    if (!fields.logo) {
        return { ...fields, logoAssetId: undefined, logoPreviewUrl: undefined }
    }
    if (pendingFile) {
        const uploaded = await uploadFileAssetImage(pendingFile)
        return {
            ...fields,
            logoAssetId: uploaded.fileAssetId,
            logoPreviewUrl: uploaded.url,
        }
    }
    return {
        ...fields,
        logoAssetId: existingAssetId || undefined,
        logoPreviewUrl: existingUrl || undefined,
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

export type ResourceFormApp = {
    AppField: React.ComponentType<{
        name: string
        children: (field: FieldApi) => React.ReactNode
    }>
}

/** 生效开始 / 结束统一用 DatePicker：格式由控件保证，避免裸文本框静默接受错误格式。 */
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
                    {String(error)}
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
    /** 品牌 Logo 等固定为正方形预览与上传区。 */
    aspectRatio,
    /** 已登记媒体的可访问预览地址（编辑回显）。 */
    previewUrl,
    /** 选择文件时透出原始文件（供保存前上传）。 */
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

export function renderStandardField(
    def: ResourceFieldDef,
    field: FieldApi,
    extras?: {
        categoryParentOptions?: ReturnType<typeof toCategoryComboboxItems>
        media?: {
            /** 选择文件时透出原始文件（dialog 暂存供保存前上传）。 */
            rememberFiles?: (defKey: string, files: File[]) => void
            /** 已登记媒体的可访问预览地址（如品牌 Logo 回显）。 */
            previewUrl?: string
            /** 媒体列表字段 key → fileName → 可访问 URL。 */
            listUrls?: Readonly<
                Record<string, Readonly<Record<string, string>>>
            >
        }
    },
) {
    if (def.kind === "textarea") {
        return <field.TextareaField label={def.label} />
    }
    if (def.kind === "select") {
        return (
            <field.SelectField
                label={def.label}
                options={(def.options ?? []).map((option) => ({
                    value: option,
                    label: option,
                }))}
                allowClear={!def.required}
                placeholder={def.required ? `请选择${def.label}` : "未填写"}
            />
        )
    }
    if (def.kind === "checkbox-group") {
        const selected = new Set(
            (field.state.value ?? "")
                .split(/[、,，]/)
                .map((s) => s.trim())
                .filter(Boolean),
        )
        return (
            <div className="space-y-1.5">
                <Label className="text-sm font-medium">{def.label}</Label>
                <div className="grid gap-2 sm:grid-cols-2">
                    {(def.options ?? []).map((option) => (
                        <label
                            key={option}
                            className="flex items-center gap-2 text-sm"
                        >
                            <Checkbox
                                checked={selected.has(option)}
                                onCheckedChange={(checked) => {
                                    const next = new Set(selected)
                                    if (checked === true) {
                                        next.add(option)
                                    } else {
                                        next.delete(option)
                                    }
                                    field.handleChange(
                                        Array.from(next).join("、"),
                                    )
                                }}
                            />
                            {option}
                        </label>
                    ))}
                </div>
            </div>
        )
    }
    if (def.kind === "category-parent") {
        return (
            <div className="space-y-1.5">
                <Label className="text-sm font-medium">{def.label}</Label>
                <CategoryCombobox
                    categories={extras?.categoryParentOptions ?? []}
                    value={field.state.value || undefined}
                    onValueChange={(id) => field.handleChange(id ?? "")}
                    placeholder="可选上级；空为根分类"
                    emptyLabel="没有可选上级分类"
                    className="w-full"
                />
                <p className="text-xs text-muted-foreground">
                    留空表示根分类；不可选择自身或下级。
                </p>
            </div>
        )
    }
    if (def.kind === "media") {
        return (
            <MediaSingleField
                label={def.label}
                hint={def.hint}
                value={field.state.value}
                onChange={(next) => field.handleChange(next)}
                required={def.required}
                selectedHint={
                    def.key === "logo" ? "Logo · 1:1 · 已选择" : "主图 · 已选择"
                }
                aspectRatio={def.key === "logo" ? "1:1" : undefined}
                previewUrl={
                    def.key === "logo" ? extras?.media?.previewUrl : undefined
                }
                onFilesSelected={(files) =>
                    extras?.media?.rememberFiles?.(def.key, files)
                }
            />
        )
    }
    if (def.kind === "media-list") {
        const listUrls = extras?.media?.listUrls
        return (
            <MediaListField
                label={def.label}
                hint={def.hint}
                value={field.state.value}
                onChange={(next) => field.handleChange(next)}
                urlByFileName={listUrls ? listUrls[def.key] : undefined}
                onFilesSelected={(files) =>
                    extras?.media?.rememberFiles?.(def.key, files)
                }
            />
        )
    }
    return <field.TextField label={def.label} />
}

/** 资源专属字段区块：窄对话框单列；商品 SKU 在宽对话框中分区双列。 */
export function ResourceFieldsSection({
    form,
    resource,
    wide,
    excludeCategoryIds,
    mediaContext,
}: {
    form: ResourceFormApp
    resource: MasterDataResource
    wide?: boolean
    /** 更新分类时排除自身与子树，避免成环。 */
    excludeCategoryIds?: ReadonlySet<string>
    /** 媒体字段上下文（文件暂存 / Logo 回显 / 链接回显）。 */
    mediaContext?: {
        rememberFiles?: (defKey: string, files: File[]) => void
        previewUrl?: string
        listUrls?: Readonly<Record<string, Readonly<Record<string, string>>>>
    }
}) {
    const categoryListQuery = useMasterDataListQuery({
        resource: "categories",
        lifecycleStatus: "all",
        revisionTiming: "all",
    })
    const categoryParentOptions = React.useMemo(() => {
        if (resource !== "categories") return []
        return toCategoryComboboxItems(categoryListQuery.data?.rows ?? [], {
            excludeIds: excludeCategoryIds,
            enabledOnly: false,
        })
    }, [categoryListQuery.data?.rows, excludeCategoryIds, resource])

    const defs = RESOURCE_FIELDS[resource]
    if (defs.length === 0) return null

    const fieldExtras = { categoryParentOptions, media: mediaContext }

    // 品牌：名称与品牌代码同行，Logo 独占一行，避免窄对话框里纵向堆叠过长。
    if (resource === "brands") {
        const codeDef = defs.find((def) => def.key === "code")
        const logoDef = defs.find((def) => def.key === "logo")
        return (
            <fieldset className="space-y-3 rounded-md border border-border p-3">
                <legend className="px-1 text-xs text-muted-foreground">
                    {masterDataCopy.fieldResourceSection}
                </legend>
                <div className="grid gap-3 sm:grid-cols-2">
                    <form.AppField
                        name="name"
                        children={(field) => <field.TextField label="名称" />}
                    />
                    {codeDef ? (
                        <form.AppField
                            name={codeDef.key}
                            children={(field) =>
                                renderStandardField(codeDef, field, fieldExtras)
                            }
                        />
                    ) : null}
                </div>
                {logoDef ? (
                    <form.AppField
                        name={logoDef.key}
                        children={(field) =>
                            renderStandardField(logoDef, field, fieldExtras)
                        }
                    />
                ) : null}
            </fieldset>
        )
    }

    if (!wide || resource !== "products") {
        return (
            <fieldset className="space-y-3 rounded-md border border-border p-3">
                <legend className="px-1 text-xs text-muted-foreground">
                    {masterDataCopy.fieldResourceSection}
                </legend>
                {defs.map((def) => (
                    <form.AppField
                        key={def.key}
                        name={def.key}
                        children={(field) =>
                            renderStandardField(def, field, fieldExtras)
                        }
                    />
                ))}
            </fieldset>
        )
    }

    const identity = defs.filter((d) => d.section === "identity")
    const catalog = defs.filter((d) => d.section === "catalog")
    const media = defs.filter((d) => d.section === "media")

    return (
        <div className="space-y-4">
            <div className="grid gap-4 lg:grid-cols-2">
                <fieldset className="space-y-3 rounded-md border border-border p-3">
                    <legend className="px-1 text-xs text-muted-foreground">
                        {masterDataCopy.fieldIdentitySection}
                    </legend>
                    {identity.map((def) => (
                        <form.AppField
                            key={def.key}
                            name={def.key}
                            children={(field) =>
                                renderStandardField(def, field)
                            }
                        />
                    ))}
                </fieldset>
                <fieldset className="space-y-3 rounded-md border border-border p-3">
                    <legend className="px-1 text-xs text-muted-foreground">
                        {masterDataCopy.fieldCatalogSection}
                    </legend>
                    <div className="grid gap-3 sm:grid-cols-2">
                        {catalog.map((def) => (
                            <form.AppField
                                key={def.key}
                                name={def.key}
                                children={(field) =>
                                    renderStandardField(def, field)
                                }
                            />
                        ))}
                    </div>
                </fieldset>
            </div>
            <fieldset className="space-y-4 rounded-md border border-border p-3">
                <legend className="px-1 text-xs text-muted-foreground">
                    {masterDataCopy.fieldMediaSection}
                </legend>
                <div className="grid gap-4 lg:grid-cols-3">
                    {media.map((def) => (
                        <form.AppField
                            key={def.key}
                            name={def.key}
                            children={(field) =>
                                renderStandardField(def, field)
                            }
                        />
                    ))}
                </div>
            </fieldset>
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

export function dialogContentClass(resource: MasterDataResource) {
    if (usesWideDialog(resource)) {
        return cn(
            "flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-5xl",
        )
    }
    // 非 wide 对话框同样加最大高度 + 内部滚动，保证小屏与长表单下底部按钮可用。
    return cn(
        "flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-lg",
    )
}

export function DialogScrollBody({
    children,
    wide,
}: {
    children: React.ReactNode
    wide?: boolean
}) {
    void wide
    return <div className="min-h-0 flex-1 overflow-y-auto pr-1">{children}</div>
}
