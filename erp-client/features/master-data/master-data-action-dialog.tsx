"use client"

import * as React from "react"
import Link from "next/link"
import { useQueryClient } from "@tanstack/react-query"
import { ImageIcon, XIcon } from "lucide-react"
import { z } from "zod"

import {
  CategoryCombobox,
  DiscardConfirmDialog,
  FormalActionResult,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
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
import {
  fetchFileAssetPreviewBlob,
  uploadFileAssetImage,
} from "@/features/file-assets/api"
import { masterDataCopy } from "@/features/master-data/copy"
import {
  WAREHOUSE_WRITE_MESSAGE,
  resourceLabel,
} from "@/features/master-data/data"
import {
  RESOURCE_FIELDS,
  buildResourceFields,
  buildResourceSchema,
  currentResourceFieldValues,
  defaultImmediateEffectiveFrom,
  emptyResourceFieldValues,
  joinMediaList,
  parseMediaList,
  usesEffectivePeriod,
  usesWideDialog,
  type ResourceFieldDef,
  type ResourceFormValues,
} from "@/features/master-data/resource-fields"
import {
  collectDescendantIds,
  buildCategoryForest,
  toCategoryComboboxItems,
} from "@/features/master-data/category-tree-model"
import {
  masterDataKeys,
  useCreateMasterDataMutation,
  useCreateRevisionMutation,
  useDisableMasterDataMutation,
  useMasterDataListQuery,
} from "@/features/master-data/queries"
import type {
  MasterDataCenterView,
  MasterDataListItem,
  MasterDataMutationResult,
  MasterDataResource,
} from "@/features/master-data/types"
import type { BrandFields } from "@/features/master-data/types"
import { cn } from "@/lib/utils"
import { getErrorMessage } from "@/lib/api/errors"

function newIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

/** 编辑成功后以 toast 提示结果，不再在 Dialog 内展示结果面板。 */
function notifySuccess(
  title: string,
  result: Extract<MasterDataMutationResult, { outcome: "succeeded" }>
) {
  toast.add({
    title,
    description: `${masterDataCopy.resultNo} ${result.stableNo} · v${result.revisionNo}`,
    type: "success",
    timeout: 4000,
  })
}

/** 品牌 Logo 保存前上传：返回回填 asset id / URL 后的品牌字段。 */
async function resolveBrandLogoFields(
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

type FieldApi = {
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

type ResourceFormApp = {
  AppField: React.ComponentType<{
    name: string
    children: (field: FieldApi) => React.ReactNode
  }>
}

/** 根据展示文件名判定内嵌预览类型。 */
const mediaPreviewKind = (fileName: string): "image" | "pdf" | null => {
  if (/\.(?:jpe?g|png|webp)$/i.test(fileName)) return "image"
  if (/\.pdf$/i.test(fileName)) return "pdf"
  return null
}

/** 生效开始 / 结束统一用 DatePicker：格式由控件保证，避免裸文本框静默接受错误格式。 */
function DateField({
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

function MediaSingleField({
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
                  <span className="text-2xs text-muted-foreground">1:1</span>
                </>
              )}
            </div>
            <div className="min-w-0 flex-1 pt-1">
              <div className="truncate text-sm font-medium">{value}</div>
              <div className="text-xs text-muted-foreground">{selectedHint}</div>
              <div className="mt-1 text-xs text-muted-foreground">比例 1:1</div>
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
              <div className="truncate text-sm font-medium">{value}</div>
              <div className="text-xs text-muted-foreground">{selectedHint}</div>
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
            isSquare && "mx-auto aspect-square max-w-[10rem] justify-center"
          )}
        />
      )}
    </div>
  )
}

export function MediaListField({
  label,
  hint,
  value,
  onChange,
  accept = "image/jpeg,image/png,image/webp",
  urlByFileName,
  assetIdByFileName,
  onFilesSelected,
  disabled = false,
}: {
  label: string
  hint?: string
  value: string
  onChange: (next: string) => void
  /** 允许上传的文件类型；默认图片。 */
  accept?: string
  /** fileName → 可访问 URL（已上传文件回显为链接）。 */
  urlByFileName?: Readonly<Record<string, string>>
  /** fileName → 文件资产 ID（敏感文件通过受控接口预览）。 */
  assetIdByFileName?: Readonly<Record<string, string>>
  /** 选择文件时透出原始文件（供保存前上传）。 */
  onFilesSelected?: (files: File[]) => void
  /** 禁止新增和移除文件；已登记文件仍可查看。 */
  disabled?: boolean
}) {
  const queryClient = useQueryClient()
  const items = parseMediaList(value)
  const [preview, setPreview] = React.useState<{
    name: string
    src: string
    kind: "image" | "pdf"
  } | null>(null)
  const [previewLoading, setPreviewLoading] = React.useState<string | null>(null)
  const [localPreviewUrls, setLocalPreviewUrls] = React.useState<
    Readonly<Record<string, string>>
  >({})
  const localPreviewUrlsRef = React.useRef<Record<string, string>>({})

  React.useEffect(
    () => () => {
      for (const url of Object.values(localPreviewUrlsRef.current)) {
        URL.revokeObjectURL(url)
      }
      localPreviewUrlsRef.current = {}
    },
    [],
  )

  /** 打开本地图片、公开图片或经鉴权读取的敏感图片预览。 */
  const openPreview = async (name: string) => {
    const kind = mediaPreviewKind(name)
    if (!kind) return
    const directUrl = localPreviewUrls[name] ?? urlByFileName?.[name]?.trim()
    if (directUrl) {
      setPreview({ name, src: directUrl, kind })
      return
    }
    const assetId = assetIdByFileName?.[name]?.trim()
    if (!assetId) return
    setPreviewLoading(name)
    try {
      const blob = await queryClient.fetchQuery({
        queryKey: ["file-assets", "preview", assetId],
        queryFn: () => fetchFileAssetPreviewBlob(assetId),
        staleTime: 60_000,
      })
      const previous = localPreviewUrlsRef.current[name]
      if (previous) URL.revokeObjectURL(previous)
      const src = URL.createObjectURL(blob)
      localPreviewUrlsRef.current[name] = src
      setLocalPreviewUrls({ ...localPreviewUrlsRef.current })
      setPreview({ name, src, kind })
    } catch (error) {
      toast.add({
        title: "文件预览失败",
        description: getErrorMessage(error, "请稍后重试"),
        type: "error",
        timeout: 4000,
      })
    } finally {
      setPreviewLoading(null)
    }
  }

  /** 释放已经从列表移除的本地 Blob URL。 */
  const removeLocalPreview = (name: string) => {
    const url = localPreviewUrlsRef.current[name]
    if (!url) return
    URL.revokeObjectURL(url)
    delete localPreviewUrlsRef.current[name]
    setLocalPreviewUrls({ ...localPreviewUrlsRef.current })
    setPreview((current) => (current?.name === name ? null : current))
  }

  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <Label className="text-sm font-medium">{label}</Label>
        <span className="text-xs text-muted-foreground">
          {masterDataCopy.mediaCount(items.length)}
          {hint ? ` · ${hint}` : null}
        </span>
      </div>
      {items.length > 0 ? (
        <ul className="space-y-1.5">
          {items.map((name, index) => {
            const url = urlByFileName?.[name]?.trim()
            const previewUrl = localPreviewUrls[name] ?? url
            const canPreview = Boolean(
              mediaPreviewKind(name) &&
                (previewUrl || assetIdByFileName?.[name]?.trim()),
            )
            return (
              <li
                key={`${name}-${index}`}
                className="flex items-center gap-2 rounded-md border border-border px-2.5 py-1.5"
              >
                <button
                  type="button"
                  className="flex min-w-0 flex-1 items-center gap-2 text-left disabled:cursor-default"
                  disabled={!canPreview || previewLoading === name}
                  onClick={() => void openPreview(name)}
                  aria-label={canPreview ? `预览 ${name}` : name}
                >
                  <span className="flex size-9 shrink-0 items-center justify-center overflow-hidden rounded bg-muted">
                    {previewUrl && mediaPreviewKind(name) === "image" ? (
                      // eslint-disable-next-line @next/next/no-img-element -- 本地 Blob 与受控文件内容不能交给 Next Image 优化。
                      <img
                        src={previewUrl}
                        alt=""
                        className="size-full object-cover"
                      />
                    ) : (
                      <ImageIcon
                        className="size-4 text-muted-foreground"
                        aria-hidden
                      />
                    )}
                  </span>
                  <span
                    className={cn(
                      "min-w-0 flex-1 truncate text-sm",
                      canPreview && "text-primary underline-offset-2 hover:underline",
                    )}
                  >
                    {previewLoading === name ? "正在打开预览…" : name}
                  </span>
                </button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-sm"
                  aria-label={`${masterDataCopy.mediaRemove} ${name}`}
                  disabled={disabled}
                  onClick={() => {
                    removeLocalPreview(name)
                    const next = items.filter((_, i) => i !== index)
                    onChange(joinMediaList(next))
                  }}
                >
                  <XIcon className="size-3.5" />
                </Button>
              </li>
            )
          })}
        </ul>
      ) : (
        <p className="text-xs text-muted-foreground">
          {masterDataCopy.mediaEmpty}（{masterDataCopy.mediaAllowEmpty}）
        </p>
      )}
      <FileUpload
        accept={accept}
        multiple
        disabled={disabled}
        label={`添加${label}`}
        description={masterDataCopy.mediaUploadHint}
        onFilesSelected={(files) => {
          onFilesSelected?.(files)
          for (const file of files) {
            if (
              !file.type.startsWith("image/") &&
              file.type !== "application/pdf"
            ) {
              continue
            }
            const previous = localPreviewUrlsRef.current[file.name]
            if (previous) URL.revokeObjectURL(previous)
            localPreviewUrlsRef.current[file.name] = URL.createObjectURL(file)
          }
          setLocalPreviewUrls({ ...localPreviewUrlsRef.current })
          const names = files.map((f) => f.name)
          onChange(joinMediaList([...items, ...names]))
        }}
        className="p-3"
      />
      <Dialog
        open={preview != null}
        onOpenChange={(open) => {
          if (!open) setPreview(null)
        }}
      >
        <DialogContent className="sm:max-w-4xl">
          <DialogHeader>
            <DialogTitle>{preview?.name ?? "图片预览"}</DialogTitle>
            <DialogDescription>仅在当前登录会话中查看。</DialogDescription>
          </DialogHeader>
          {preview?.kind === "image" ? (
            <div className="flex max-h-[72vh] min-h-64 items-center justify-center overflow-auto rounded-md border bg-muted/30 p-3">
              {/* eslint-disable-next-line @next/next/no-img-element -- 预览来源可能是本地 Blob 或需鉴权读取的对象。 */}
              <img
                src={preview.src}
                alt={preview.name}
                className="max-h-[68vh] max-w-full object-contain"
              />
            </div>
          ) : preview?.kind === "pdf" ? (
            <iframe
              src={preview.src}
              title={preview.name}
              sandbox=""
              referrerPolicy="no-referrer"
              className="h-[72vh] w-full rounded-md border bg-background"
            />
          ) : null}
        </DialogContent>
      </Dialog>
    </div>
  )
}

function renderStandardField(
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
      listUrls?: Readonly<Record<string, Readonly<Record<string, string>>>>
    }
  }
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
        .filter(Boolean)
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
                  field.handleChange(Array.from(next).join("、"))
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
        previewUrl={def.key === "logo" ? extras?.media?.previewUrl : undefined}
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
function ResourceFieldsSection({
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
              children={(field) => renderStandardField(codeDef, field, fieldExtras)}
            />
          ) : null}
        </div>
        {logoDef ? (
          <form.AppField
            name={logoDef.key}
            children={(field) => renderStandardField(logoDef, field, fieldExtras)}
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
            children={(field) => renderStandardField(def, field, fieldExtras)}
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
              children={(field) => renderStandardField(def, field)}
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
                children={(field) => renderStandardField(def, field)}
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
              children={(field) => renderStandardField(def, field)}
            />
          ))}
        </div>
      </fieldset>
    </div>
  )
}

const disableSchema = z.object({
  changeReason: z.string().trim().min(2, "请填写停用原因"),
  effectiveFrom: z
    .string()
    .min(1, "请填写停用时间")
    .refine(
      (value) => /^\d{4}-\d{2}-\d{2}$/.test(value),
      "停用时间格式不正确，请使用 YYYY-MM-DD"
    ),
})

function dialogContentClass(resource: MasterDataResource) {
  if (usesWideDialog(resource)) {
    return cn(
      "flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-5xl"
    )
  }
  // 非 wide 对话框同样加最大高度 + 内部滚动，保证小屏与长表单下底部按钮可用。
  return cn(
    "flex max-h-[92vh] w-full flex-col gap-4 overflow-hidden sm:max-w-lg"
  )
}

function DialogScrollBody({
  children,
  wide,
}: {
  children: React.ReactNode
  wide?: boolean
}) {
  void wide
  return (
    <div className="min-h-0 flex-1 overflow-y-auto pr-1">{children}</div>
  )
}

export function MasterDataCreateDialog({
  open,
  onOpenChange,
  resource,
  defaultFieldValues,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  resource: MasterDataResource
  /** 预填资源专属字段（如新建子分类时的 parentId）。 */
  defaultFieldValues?: Partial<Record<string, string>>
}) {
  const mutation = useCreateMasterDataMutation()
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey("create")
  )
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null
  )
  const [discardOpen, setDiscardOpen] = React.useState(false)

  const isWarehouse = resource === "warehouses"
  const wide = usesWideDialog(resource)
  const showEffectivePeriod = usesEffectivePeriod(resource)

  /** 本会话选择但尚未上传的文件；key 为 `字段key::文件名`。 */
  const pendingFilesRef = React.useRef<Map<string, File>>(new Map())
  const [logoAssetId, setLogoAssetId] = React.useState("")
  const [logoPreviewUrl, setLogoPreviewUrl] = React.useState("")
  const rememberFiles = React.useCallback((defKey: string, files: File[]) => {
    for (const file of files) {
      pendingFilesRef.current.set(`${defKey}::${file.name}`, file)
    }
    // 重新选 Logo 后旧 asset 作废，保存时以上传结果为准
    if (defKey === "logo") setLogoAssetId("")
  }, [])

  const defaults: ResourceFormValues = {
    name: "",
    effectiveFrom: showEffectivePeriod
      ? defaultImmediateEffectiveFrom()
      : defaultImmediateEffectiveFrom(),
    effectiveTo: "",
    changeReason: "",
    ...emptyResourceFieldValues(resource),
    ...defaultFieldValues,
  }

  const form = useAppForm({
    defaultValues: defaults,
    validators: {
      onChange: buildResourceSchema(resource, RESOURCE_FIELDS[resource]),
    },
    onSubmit: async ({ value }) => {
      let fields = buildResourceFields(resource, value)
      if (resource === "brands") {
        try {
          fields = await resolveBrandLogoFields(
            fields as BrandFields,
            pendingFilesRef.current.get(`logo::${(fields as BrandFields).logo}`),
            logoAssetId,
            logoPreviewUrl,
          )
        } catch (error) {
          setResult({
            outcome: "blocked",
            code: "MEDIA_UPLOAD_FAILED",
            message: getErrorMessage(error, "Logo 上传失败"),
          })
          return
        }
      }
      const response = await mutation.mutateAsync({
        resource,
        name: value.name.trim(),
        effectiveFrom: showEffectivePeriod
          ? value.effectiveFrom
          : defaultImmediateEffectiveFrom(),
        effectiveTo: showEffectivePeriod
          ? value.effectiveTo.trim() || undefined
          : undefined,
        changeReason: value.changeReason.trim(),
        fields,
        idempotencyKey,
      })
      if (response.outcome === "succeeded") {
        notifySuccess(masterDataCopy.createSuccessTitle, response)
        reset()
        onOpenChange(false)
        return
      }
      setResult(response)
    },
  })

  const reset = () => {
    setResult(null)
    setIdempotencyKey(newIdempotencyKey("create"))
    setLogoAssetId("")
    setLogoPreviewUrl("")
    form.reset()
  }

  const requestClose = (next: boolean) => {
    if (next) {
      onOpenChange(true)
      return
    }
    if (result?.outcome === "succeeded") {
      reset()
      onOpenChange(false)
      return
    }
    if (form.state.isDirty || result) {
      setDiscardOpen(true)
      return
    }
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={requestClose}>
      <DialogContent className={dialogContentClass(resource)}>
        <DialogHeader>
          <DialogTitle>
            {masterDataCopy.createTitle(resourceLabel(resource))}
          </DialogTitle>
          <DialogDescription>{masterDataCopy.createDesc}</DialogDescription>
        </DialogHeader>

        <DialogScrollBody wide={wide}>
          {isWarehouse ? (
            <Alert variant="destructive">
              <AlertTitle>{masterDataCopy.warehouseWriteTitle}</AlertTitle>
              <AlertDescription>{WAREHOUSE_WRITE_MESSAGE}</AlertDescription>
            </Alert>
          ) : null}

          {result?.outcome === "blocked" ? (
            <FormalActionResult
              status="blocked"
              title={masterDataCopy.createBlockedTitle}
              description={result.message}
              facts={
                result.detail
                  ? [{ label: "说明", value: result.detail }]
                  : undefined
              }
            />
          ) : null}

          {result?.outcome !== "succeeded" ? (
            <form
              className="grid gap-3"
              onSubmit={(e) => {
                e.preventDefault()
                void form.handleSubmit()
              }}
            >
              {resource !== "brands" ? (
                <form.AppField
                  name="name"
                  children={(field) => <field.TextField label="名称" />}
                />
              ) : null}
              <ResourceFieldsSection
                form={form}
                resource={resource}
                wide={wide}
                mediaContext={{
                  rememberFiles,
                  previewUrl: logoPreviewUrl,
                }}
              />
              {showEffectivePeriod ? (
                <div className="grid gap-3 sm:grid-cols-2">
                  <form.AppField
                    name="effectiveFrom"
                    children={(field) => (
                      <DateField
                        id="create-ef-from"
                        label={masterDataCopy.fieldEffectiveFrom}
                        field={field}
                      />
                    )}
                  />
                  <form.AppField
                    name="effectiveTo"
                    children={(field) => (
                      <DateField
                        id="create-ef-to"
                        label={masterDataCopy.fieldEffectiveTo}
                        field={field}
                      />
                    )}
                  />
                </div>
              ) : null}
              <form.AppField
                name="changeReason"
                children={(field) => (
                  <field.TextareaField
                    label={masterDataCopy.fieldChangeReason}
                  />
                )}
              />
              <DialogFooter>
                <DialogClose
                  render={<Button type="button" variant="outline" />}
                >
                  关闭
                </DialogClose>
                <Button
                  type="submit"
                  disabled={mutation.isPending || isWarehouse}
                  title={isWarehouse ? WAREHOUSE_WRITE_MESSAGE : undefined}
                >
                  {isWarehouse
                    ? masterDataCopy.createSubmitRejected
                    : masterDataCopy.createSubmit}
                </Button>
              </DialogFooter>
            </form>
          ) : null}
        </DialogScrollBody>
      </DialogContent>

      <DiscardConfirmDialog
        open={discardOpen}
        onOpenChange={setDiscardOpen}
        title="放弃本次填写？"
        description="关闭后本次填写的内容将丢失。"
        confirmLabel="放弃填写"
        cancelLabel="继续编辑"
        onConfirm={() => {
          setDiscardOpen(false)
          reset()
          onOpenChange(false)
        }}
      />
    </Dialog>
  )
}

export function MasterDataReviseDialog({
  open,
  onOpenChange,
  resource,
  target,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  resource: MasterDataResource
  target: MasterDataListItem | MasterDataCenterView | null
}) {
  const mutation = useCreateRevisionMutation()
  const queryClient = useQueryClient()
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey("revise")
  )
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null
  )
  const [discardOpen, setDiscardOpen] = React.useState(false)

  /** 本会话选择但尚未上传的文件；key 为 `字段key::文件名`。 */
  const pendingFilesRef = React.useRef<Map<string, File>>(new Map())
  const [logoAssetId, setLogoAssetId] = React.useState("")
  const [logoPreviewUrl, setLogoPreviewUrl] = React.useState("")
  const rememberFiles = React.useCallback((defKey: string, files: File[]) => {
    for (const file of files) {
      pendingFilesRef.current.set(`${defKey}::${file.name}`, file)
    }
    // 重新选 Logo 后旧 asset 作废，保存时以上传结果为准
    if (defKey === "logo") setLogoAssetId("")
  }, [])

  const isWarehouse = resource === "warehouses"
  const wide = usesWideDialog(resource)
  const showEffectivePeriod = usesEffectivePeriod(resource)
  const stableId = target && "stableId" in target ? target.stableId : ""
  const baseRevisionId =
    target && "currentRevisionId" in target
      ? target.currentRevisionId
      : target && "currentRevision" in target
        ? target.currentRevision.revisionId
        : ""
  const lockVersion = target?.lockVersion ?? 0
  const nameDefault = target?.name ?? ""
  // 更新默认「当前生效日」，避免不改直接保存把修改排期到未来。
  const effectiveFromDefault =
    target && "currentRevision" in target
      ? target.currentRevision.effectiveFrom
      : target && "effectiveFrom" in target && target.effectiveFrom
        ? target.effectiveFrom
        : defaultImmediateEffectiveFrom()

  const categoryListQuery = useMasterDataListQuery({
    resource: "categories",
    lifecycleStatus: "all",
    revisionTiming: "all",
  })
  const excludeCategoryIds = React.useMemo(() => {
    if (resource !== "categories" || !stableId) return undefined
    const forest = buildCategoryForest(categoryListQuery.data?.rows ?? [])
    return collectDescendantIds(forest, stableId)
  }, [categoryListQuery.data?.rows, resource, stableId])

  const defaults: ResourceFormValues = {
    name: nameDefault,
    effectiveFrom: showEffectivePeriod
      ? effectiveFromDefault
      : defaultImmediateEffectiveFrom(),
    effectiveTo: "",
    changeReason: "",
    ...emptyResourceFieldValues(resource),
  }

  const form = useAppForm({
    defaultValues: defaults,
    validators: {
      onChange: buildResourceSchema(resource, RESOURCE_FIELDS[resource]),
    },
    onSubmit: async ({ value }) => {
      if (!stableId || !baseRevisionId) return
      let fields = buildResourceFields(resource, value)
      if (resource === "brands") {
        try {
          fields = await resolveBrandLogoFields(
            fields as BrandFields,
            pendingFilesRef.current.get(`logo::${(fields as BrandFields).logo}`),
            logoAssetId,
            logoPreviewUrl,
          )
        } catch (error) {
          setResult({
            outcome: "blocked",
            code: "MEDIA_UPLOAD_FAILED",
            message: getErrorMessage(error, "Logo 上传失败"),
          })
          return
        }
      }
      const response = await mutation.mutateAsync({
        resource,
        stableId,
        baseRevisionId,
        expectedLockVersion: lockVersion,
        name: value.name.trim(),
        effectiveFrom: showEffectivePeriod
          ? value.effectiveFrom
          : defaultImmediateEffectiveFrom(),
        effectiveTo: showEffectivePeriod
          ? value.effectiveTo.trim() || undefined
          : undefined,
        changeReason: value.changeReason.trim(),
        fields,
        idempotencyKey,
      })
      if (response.outcome === "succeeded") {
        notifySuccess(masterDataCopy.reviseSuccessTitle, response)
        onOpenChange(false)
        return
      }
      setResult(response)
    },
  })

  React.useEffect(() => {
    if (open && target) {
      form.setFieldValue("name", target.name)
      form.setFieldValue(
        "effectiveFrom",
        target && "currentRevision" in target
          ? target.currentRevision.effectiveFrom
          : target?.effectiveFrom ?? defaultImmediateEffectiveFrom()
      )
      form.setFieldValue(
        "effectiveTo",
        target && "currentRevision" in target
          ? target.currentRevision.effectiveTo ?? ""
          : ""
      )
      for (const [key, value] of Object.entries(
        currentResourceFieldValues(target)
      )) {
        form.setFieldValue(key, value)
      }
      // 品牌 Logo 回显：asset id + URL 由媒体资产映射恢复
      const logoAsset =
        target && "mediaAssets" in target
          ? target.mediaAssets?.logo?.[0]
          : undefined
      setLogoAssetId(logoAsset?.assetId ?? "")
      setLogoPreviewUrl(logoAsset?.url ?? "")
      setResult(null)
      setIdempotencyKey(newIdempotencyKey("revise"))
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reset only when target opens
  }, [open, stableId, baseRevisionId])

  const reloadLatest = React.useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: masterDataKeys.all })
    setResult(null)
    setDiscardOpen(false)
    setIdempotencyKey(newIdempotencyKey("revise"))
  }, [queryClient])

  const requestClose = (next: boolean) => {
    if (next) {
      onOpenChange(true)
      return
    }
    if (result?.outcome === "succeeded") {
      onOpenChange(false)
      return
    }
    if (form.state.isDirty || result) {
      setDiscardOpen(true)
      return
    }
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={requestClose}>
      <DialogContent className={dialogContentClass(resource)}>
        <DialogHeader>
          <DialogTitle>{masterDataCopy.reviseTitle}</DialogTitle>
          <DialogDescription>
            {masterDataCopy.reviseDesc}
            {target ? (
              <>
                {" "}
                资料编号 <span className="num">{target.stableNo}</span>
              </>
            ) : null}
          </DialogDescription>
        </DialogHeader>

        <DialogScrollBody wide={wide}>
          {isWarehouse ? (
            <Alert variant="destructive">
              <AlertTitle>{masterDataCopy.warehouseWriteTitle}</AlertTitle>
              <AlertDescription>{WAREHOUSE_WRITE_MESSAGE}</AlertDescription>
            </Alert>
          ) : null}

          {result?.outcome === "blocked" ? (
            <FormalActionResult
              status="blocked"
              title={masterDataCopy.reviseBlockedTitle}
              description={result.message}
              facts={
                result.detail
                  ? [{ label: "说明", value: result.detail }]
                  : undefined
              }
            />
          ) : null}

          {result?.outcome === "conflict" ? (
            <FormalActionResult
              status="blocked"
              title={masterDataCopy.reviseConflictTitle}
              description={result.message || masterDataCopy.reviseConflictHint}
              facts={[
                {
                  label: "当前版本",
                  value: `v${result.serverRevisionNo}`,
                },
              ]}
              actions={
                <Button
                  type="button"
                  size="sm"
                  variant="outline"
                  onClick={reloadLatest}
                >
                  {masterDataCopy.reloadAction}
                </Button>
              }
            />
          ) : null}

          {result?.outcome !== "succeeded" ? (
            <form
              className="grid gap-3"
              onSubmit={(e) => {
                e.preventDefault()
                void form.handleSubmit()
              }}
            >
              {resource !== "brands" ? (
                <form.AppField
                  name="name"
                  children={(field) => (
                    <field.TextField label={masterDataCopy.reviseNameLabel} />
                  )}
                />
              ) : null}
              <ResourceFieldsSection
                form={form}
                resource={resource}
                wide={wide}
                excludeCategoryIds={excludeCategoryIds}
                mediaContext={{
                  rememberFiles,
                  previewUrl: logoPreviewUrl,
                }}
              />
              {showEffectivePeriod ? (
                <div className="grid gap-3 sm:grid-cols-2">
                  <form.AppField
                    name="effectiveFrom"
                    children={(field) => (
                      <DateField
                        id="rev-ef-from"
                        label={masterDataCopy.fieldEffectiveFrom}
                        field={field}
                      />
                    )}
                  />
                  <form.AppField
                    name="effectiveTo"
                    children={(field) => (
                      <DateField
                        id="rev-ef-to"
                        label={masterDataCopy.fieldEffectiveTo}
                        field={field}
                      />
                    )}
                  />
                </div>
              ) : null}
              <form.AppField
                name="changeReason"
                children={(field) => (
                  <field.TextareaField
                    label={masterDataCopy.fieldChangeReason}
                  />
                )}
              />
              <DialogFooter>
                <DialogClose
                  render={<Button type="button" variant="outline" />}
                >
                  关闭
                </DialogClose>
                <Button type="submit" disabled={mutation.isPending || !target}>
                  {isWarehouse
                    ? masterDataCopy.createSubmitRejected
                    : masterDataCopy.reviseSubmit}
                </Button>
              </DialogFooter>
            </form>
          ) : null}
        </DialogScrollBody>
      </DialogContent>

      <DiscardConfirmDialog
        open={discardOpen}
        onOpenChange={setDiscardOpen}
        title="放弃本次填写？"
        description="关闭后本次填写的内容将丢失。"
        confirmLabel="放弃填写"
        cancelLabel="继续编辑"
        onConfirm={() => {
          setDiscardOpen(false)
          onOpenChange(false)
        }}
      />
    </Dialog>
  )
}

export function MasterDataDisableDialog({
  open,
  onOpenChange,
  resource,
  target,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  resource: MasterDataResource
  target: MasterDataListItem | MasterDataCenterView | null
}) {
  const mutation = useDisableMasterDataMutation()
  const queryClient = useQueryClient()
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey("disable")
  )
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null
  )
  const [discardOpen, setDiscardOpen] = React.useState(false)

  const isWarehouse = resource === "warehouses"
  const stableId = target?.stableId ?? ""
  const baseRevisionId =
    target && "currentRevisionId" in target
      ? target.currentRevisionId
      : target && "currentRevision" in target
        ? target.currentRevision.revisionId
        : ""
  const lockVersion = target?.lockVersion ?? 0

  const form = useAppForm({
    defaultValues: {
      changeReason: "",
      effectiveFrom: defaultImmediateEffectiveFrom(),
    },
    validators: { onChange: disableSchema },
    onSubmit: async ({ value }) => {
      if (!stableId || !baseRevisionId) return
      const response = await mutation.mutateAsync({
        resource,
        stableId,
        baseRevisionId,
        expectedLockVersion: lockVersion,
        changeReason: value.changeReason.trim(),
        effectiveFrom: value.effectiveFrom,
        idempotencyKey,
      })
      if (response.outcome === "succeeded") {
        notifySuccess(masterDataCopy.disableSuccessTitle, response)
        onOpenChange(false)
        return
      }
      setResult(response)
    },
  })

  React.useEffect(() => {
    if (open) {
      setResult(null)
      setIdempotencyKey(newIdempotencyKey("disable"))
      form.reset()
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, stableId])

  const reloadLatest = React.useCallback(() => {
    void queryClient.invalidateQueries({ queryKey: masterDataKeys.all })
    setResult(null)
    setIdempotencyKey(newIdempotencyKey("disable"))
  }, [queryClient])

  const requestClose = (next: boolean) => {
    if (next) {
      onOpenChange(true)
      return
    }
    if (result?.outcome === "succeeded") {
      onOpenChange(false)
      return
    }
    if (form.state.isDirty || result) {
      setDiscardOpen(true)
      return
    }
    onOpenChange(false)
  }

  return (
    <Dialog open={open} onOpenChange={requestClose}>
      <DialogContent className={dialogContentClass(resource)}>
        <DialogHeader>
          <DialogTitle>{masterDataCopy.disableTitle}</DialogTitle>
          <DialogDescription>
            {masterDataCopy.disableDesc}
            {target ? (
              <>
                {" "}
                资料编号 <span className="num">{target.stableNo}</span>
              </>
            ) : null}
          </DialogDescription>
        </DialogHeader>

        <DialogScrollBody wide={false}>
        {isWarehouse ? (
          <Alert variant="destructive">
            <AlertTitle>{masterDataCopy.warehouseWriteTitle}</AlertTitle>
            <AlertDescription>
              {WAREHOUSE_WRITE_MESSAGE}
              {target &&
              "warehouseStockSummary" in target &&
              target.warehouseStockSummary?.hasBlockingStock
                ? ` 另：在库 ${target.warehouseStockSummary.onHandQty} / 预占 ${target.warehouseStockSummary.reservedQty} 时也不可停用。`
                : null}
            </AlertDescription>
          </Alert>
        ) : null}

        {result?.outcome === "blocked" ? (
          <FormalActionResult
            status="blocked"
            title={masterDataCopy.disableBlockedTitle}
            description={result.message}
            facts={[
              ...(result.detail
                ? [{ label: "说明", value: result.detail }]
                : []),
              ...(result.drillHref
                ? [
                    {
                      label: "库存台账",
                      value: (
                        <Link
                          className="text-primary underline-offset-4 hover:underline"
                          href={result.drillHref}
                        >
                          打开库存台账
                        </Link>
                      ),
                    },
                  ]
                : []),
            ]}
          />
        ) : null}

        {result?.outcome === "conflict" ? (
          <FormalActionResult
            status="blocked"
            title={masterDataCopy.reviseConflictTitle}
            description={result.message || masterDataCopy.reviseConflictHint}
            facts={[
              {
                label: "当前版本",
                value: `v${result.serverRevisionNo}`,
              },
            ]}
            actions={
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={reloadLatest}
              >
                {masterDataCopy.reloadAction}
              </Button>
            }
          />
        ) : null}

        {result?.outcome !== "succeeded" ? (
          <form
            className="grid gap-3"
            onSubmit={(e) => {
              e.preventDefault()
              void form.handleSubmit()
            }}
          >
            <form.AppField
              name="effectiveFrom"
              children={(field) => (
                <DateField
                  id="dis-ef-from"
                  label={masterDataCopy.fieldDisableAt}
                  field={field}
                />
              )}
            />
            <form.AppField
              name="changeReason"
              children={(field) => (
                <field.TextareaField
                  label={masterDataCopy.fieldDisableReason}
                />
              )}
            />
            <DialogFooter>
              <DialogClose render={<Button type="button" variant="outline" />}>
                关闭
              </DialogClose>
              <Button type="submit" disabled={mutation.isPending || !target}>
                {isWarehouse
                  ? masterDataCopy.createSubmitRejected
                  : masterDataCopy.disableSubmit}
              </Button>
            </DialogFooter>
          </form>
        ) : null}
        </DialogScrollBody>
      </DialogContent>

      <DiscardConfirmDialog
        open={discardOpen}
        onOpenChange={setDiscardOpen}
        title="放弃本次填写？"
        description="关闭后本次填写的内容将丢失。"
        confirmLabel="放弃填写"
        cancelLabel="继续编辑"
        onConfirm={() => {
          setDiscardOpen(false)
          onOpenChange(false)
        }}
      />
    </Dialog>
  )
}
