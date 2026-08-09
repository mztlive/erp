"use client"

/**
 * 商品详情页 = 查看 + 编辑（同一页面）。
 * - /master-data/products/new  新建
 * - /master-data/products/:id  查看并直接改，保存即形成新版本
 * 不使用侧边 sheet，也不再有单独的 ?mode=edit。
 */

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import {
  ArrowDownIcon,
  ArrowUpIcon,
  BanIcon,
  CheckCircle2Icon,
  CircleAlertIcon,
  ClipboardCheckIcon,
  GripVerticalIcon,
  ImageIcon,
  PlusIcon,
  SaveIcon,
  XIcon,
} from "lucide-react"

import {
  BrandCombobox,
  BusinessFailureState,
  CategoryCombobox,
  DiscardConfirmDialog,
  DocumentSection,
  FormalActionResult,
  OptionCombobox,
  PageHeader,
  PageScaffold,
  RevisionTimeline,
  surfacePanelClassName,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  FileUpload,
  imagePreviewSource,
} from "@/components/ui/file-upload"
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/components/ui/hover-card"
import { DatePicker } from "@/components/ui/date-picker"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { StatusBadge } from "@/components/ui/status-badge"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { toast } from "@/components/ui/toast"
import { MasterDataDisableDialog } from "@/features/master-data/master-data-action-dialog"
import {
  ProductInventoryPreviewSheet,
  type ProductInventoryPreviewSku,
} from "@/features/master-data/product-inventory-preview-sheet"
import {
  RegisterSupplyForSkuDialog,
  type FixedSku,
} from "@/features/supplier-offerings/offering-dialogs"
import { uploadFileAssetImage } from "@/features/file-assets/api"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { masterDataCopy } from "@/features/master-data/copy"
import { formatEffectiveRange } from "@/features/master-data/filter"
import {
  toBrandComboboxItems,
  toCategoryComboboxItems,
} from "@/features/master-data/category-tree-model"
import {
  defaultImmediateEffectiveFrom,
} from "@/features/master-data/resource-fields"
import {
  emptyProductFields,
  rebuildSkusFromSpecs,
  validateProductFields,
} from "@/features/master-data/product-model"
import {
  useCreateMasterDataMutation,
  useCreateRevisionMutation,
  useMasterDataCenterQuery,
  useMasterDataListQuery,
  useSkuSupplierCountsQuery,
} from "@/features/master-data/queries"
import type {
  MasterDataCenterView,
  MasterDataMutationResult,
  ProductDetailView,
  ProductFields,
  ProductKind,
  ProductSkuFields,
  ProductSpecDimension,
} from "@/features/master-data/types"
import {
  PRODUCT_KIND_LABELS,
  PRODUCT_KIND_VALUES,
} from "@/features/master-data/types"
import { cn } from "@/lib/utils"
import { formatDateTime } from "@/lib/datetime"
import { getErrorMessage, getErrorPresentation } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import { useUnitOptionsQuery } from "@/hooks/use-options"

function newIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

type ProductSpecDraft = Readonly<{
  name: string
  values: readonly string[]
}>

type ProductEditorFormValues = Readonly<{
  name: string
  effectiveFrom: string
  effectiveTo: string
  changeReason: string
  fields: ProductFields
  specDrafts: readonly ProductSpecDraft[]
  batchSalePrice: string
  batchMarketPrice: string
}>

type ProductEditorSectionId =
  "basic" | "media" | "sku" | "effective" | "history"

const PRODUCT_EDITOR_SECTIONS: ReadonlyArray<{
  id: ProductEditorSectionId
  label: string
}> = [
  { id: "basic", label: "基础信息" },
  { id: "media", label: "图文信息" },
  { id: "sku", label: "SKU" },
  { id: "effective", label: "生效信息" },
  { id: "history", label: "历史与引用" },
]

function MoneyInput({
  value,
  onChange,
  disabled = false,
  "aria-label": ariaLabel,
}: {
  value: string
  onChange: (next: string) => void
  disabled?: boolean
  "aria-label": string
}) {
  const showPrefix = !value.trim().startsWith("¥")
  return (
    <div className="relative">
      {showPrefix ? (
        <span className="pointer-events-none absolute left-2.5 top-1/2 -translate-y-1/2 text-xs text-muted-foreground">
          ¥
        </span>
      ) : null}
      <Input
        className={cn("h-8", showPrefix && "pl-6")}
        value={value}
        disabled={disabled}
        onChange={(event) =>
          onChange(event.target.value.replaceAll("¥", ""))
        }
        aria-label={ariaLabel}
      />
    </div>
  )
}

function applySpecsFromDrafts(
  drafts: readonly ProductSpecDraft[],
  current: ProductFields,
): ProductFields {
  const specs: ProductSpecDimension[] = drafts
    .map((draft) => ({
      name: draft.name.trim(),
      values: draft.values.map((value) => value.trim()).filter(Boolean),
    }))
    .filter((spec) => spec.name)
  const reorderedExisting = current.skus.map((sku) => ({
    ...sku,
    attributeValues: specs.map((spec, nextIndex) => {
      const previousIndex = current.specs.findIndex(
        (previous) => previous.name.trim() === spec.name,
      )
      return (
        sku.attributeValues[previousIndex >= 0 ? previousIndex : nextIndex] ??
        ""
      )
    }),
  }))
  const skus = rebuildSkusFromSpecs({
    specs,
    existing: reorderedExisting,
    baseUnit: current.baseUnit,
    skuNoPrefix: "SKU",
  })
  return { ...current, specs, skus }
}

function moveListItem<T>(
  items: readonly T[],
  fromIndex: number,
  toIndex: number,
): T[] {
  if (toIndex < 0 || toIndex >= items.length || fromIndex === toIndex) {
    return [...items]
  }
  const next = [...items]
  const [item] = next.splice(fromIndex, 1)
  if (item !== undefined) next.splice(toIndex, 0, item)
  return next
}

function validateProductEditor(
  values: ProductEditorFormValues,
  fields: ProductFields,
): string | null {
  if (values.name.trim().length < 2) return "请填写商品名称"
  if (values.changeReason.trim().length < 2) {
    return "请填写本次保存的变更原因"
  }
  return validateProductFields(fields)
}

function scrollToProductSection(id: ProductEditorSectionId) {
  document.getElementById(`product-section-${id}`)?.scrollIntoView({
    behavior: "smooth",
    block: "start",
  })
}

function productDetailToFields(detail: ProductDetailView): ProductFields {
  return {
    lifecycleStatus: detail.lifecycleStatus,
    productNo: detail.productNo,
    description: detail.description ?? "",
    specification: detail.specification ?? "",
    baseUnitId: detail.baseUnitId,
    baseUnitCode: detail.baseUnitCode,
    baseUnit: detail.baseUnit,
    categoryId: detail.categoryId,
    category: detail.category,
    brandId: detail.brandId,
    brand: detail.brand,
    productKind: "",
    carouselImages: [...detail.carouselImages],
    detailImages: [...detail.detailImages],
    carouselPreviewUrls: { ...detail.carouselPreviewUrls },
    detailPreviewUrls: { ...detail.detailPreviewUrls },
    carouselFileAssetIds: { ...detail.carouselFileAssetIds },
    detailFileAssetIds: { ...detail.detailFileAssetIds },
    specs: detail.specs.map((s) => ({
      name: s.name,
      values: [...s.values],
    })),
    skus: detail.skus.map((sku) => ({
      ...sku,
      attributeValues: [...sku.attributeValues],
    })),
  }
}

function MediaListEditor({
  label,
  hint,
  value,
  onChange,
  previewUrls,
  onPreviewUrlsChange,
  onFilesSelected,
  mode = "carousel",
}: {
  label: string
  hint?: string
  value: readonly string[]
  onChange: (next: string[]) => void
  /** fileName → 可访问预览地址（远程 URL 或本地 blob）。 */
  previewUrls?: Readonly<Record<string, string>>
  onPreviewUrlsChange?: (next: Record<string, string>) => void
  /** 选择文件时透出原始文件（用于保存前上传）。 */
  onFilesSelected?: (files: File[]) => void
  mode?: "carousel" | "detail"
}) {
  const [localPreviewUrls, setLocalPreviewUrls] = React.useState<
    ReadonlyMap<string, string>
  >(() => new Map())
  const localPreviewUrlsRef = React.useRef<ReadonlyMap<string, string>>(
    new Map(),
  )
  const [expandedPreview, setExpandedPreview] = React.useState<{
    name: string
    src: string
  } | null>(null)

  const updateLocalPreviewUrls = React.useCallback(
    (update: (previous: ReadonlyMap<string, string>) => Map<string, string>) => {
      setLocalPreviewUrls((previous) => {
        const next = update(previous)
        localPreviewUrlsRef.current = next
        return next
      })
    },
    [],
  )

  React.useEffect(
    () => () => {
      for (const src of localPreviewUrlsRef.current.values()) {
        URL.revokeObjectURL(src)
      }
    },
    [],
  )

  React.useEffect(() => {
    const retainedNames = new Set(value)
    const removedNames = [...localPreviewUrlsRef.current.keys()].filter(
      (name) => !retainedNames.has(name),
    )
    if (removedNames.length === 0) return
    updateLocalPreviewUrls((previous) => {
      const next = new Map(previous)
      for (const name of removedNames) {
        const src = next.get(name)
        if (src) URL.revokeObjectURL(src)
        next.delete(name)
      }
      return next
    })
  }, [updateLocalPreviewUrls, value])

  return (
    <div className="space-y-3">
      <div className="flex items-center justify-between gap-2">
        <div>
          <Label className="text-sm font-medium">{label}</Label>
          <p className="mt-1 text-xs text-muted-foreground">
            {hint ?? masterDataCopy.mediaUploadHint}
          </p>
        </div>
        <Badge variant="secondary">
          {masterDataCopy.mediaCount(value.length)}
        </Badge>
      </div>
      <div
        className={cn(
          "grid gap-3",
          mode === "carousel"
            ? "grid-cols-2 sm:grid-cols-4"
            : "grid-cols-2 sm:grid-cols-3",
        )}
      >
        {value.map((name, index) => {
          const previewSrc =
            localPreviewUrls.get(name) ??
            previewUrls?.[name] ??
            imagePreviewSource(name)
          return (
            <div
              key={`${name}-${index}`}
              className="group relative overflow-hidden rounded-xl border border-border bg-surface-sunken"
            >
              <button
                type="button"
                className={cn(
                  "flex w-full flex-col items-center justify-center gap-2 p-3 text-center focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
                  mode === "carousel" ? "aspect-square" : "aspect-[4/5]",
                  previewSrc && "cursor-zoom-in p-0",
                )}
                aria-label={previewSrc ? `放大预览 ${name}` : name}
                onClick={() => {
                  if (previewSrc) setExpandedPreview({ name, src: previewSrc })
                }}
              >
                {previewSrc ? (
                  // eslint-disable-next-line @next/next/no-img-element -- 本地待上传图片使用 blob URL。
                  <img
                    src={previewSrc}
                    alt={name}
                    className="absolute inset-0 size-full object-cover"
                  />
                ) : (
                  <>
                    <ImageIcon
                      className="size-7 text-muted-foreground"
                      aria-hidden
                    />
                    <span className="line-clamp-2 break-all text-xs text-muted-foreground">
                      {name}
                    </span>
                  </>
                )}
              </button>
              <Badge
                variant="secondary"
                className="absolute left-2 top-2 tabular-nums"
              >
                {index + 1}
              </Badge>
              {mode === "carousel" && index === 0 ? (
                <Badge className="absolute right-2 top-2">首图</Badge>
              ) : null}
              <div className="flex items-center justify-center gap-1 border-t border-border bg-background/95 p-1">
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  disabled={index === 0}
                  aria-label={`${name} 上移`}
                  onClick={() => onChange(moveListItem(value, index, index - 1))}
                >
                  <ArrowUpIcon />
                </Button>
                <GripVerticalIcon
                  className="size-3.5 text-muted-foreground"
                  aria-hidden
                />
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  disabled={index === value.length - 1}
                  aria-label={`${name} 下移`}
                  onClick={() => onChange(moveListItem(value, index, index + 1))}
                >
                  <ArrowDownIcon />
                </Button>
                <Button
                  type="button"
                  variant="ghost"
                  size="icon-xs"
                  aria-label={`${masterDataCopy.mediaRemove} ${name}`}
                  onClick={() => onChange(value.filter((_, i) => i !== index))}
                >
                  <XIcon />
                </Button>
              </div>
            </div>
          )
        })}
        <FileUpload
          accept="image/jpeg,image/png,image/webp"
          multiple
          label={`添加${label}`}
          description={
            mode === "carousel"
              ? "支持多选，首张作为首图"
              : "支持多选，按顺序展示"
          }
          onFilesSelected={(files) => {
            onFilesSelected?.(files)
            const addedUrls: Record<string, string> = {}
            updateLocalPreviewUrls((previous) => {
              const next = new Map(previous)
              for (const file of files) {
                const previousSrc = next.get(file.name)
                if (previousSrc) URL.revokeObjectURL(previousSrc)
                const blobUrl = URL.createObjectURL(file)
                next.set(file.name, blobUrl)
                addedUrls[file.name] = blobUrl
              }
              return next
            })
            if (onPreviewUrlsChange) {
              onPreviewUrlsChange({
                ...(previewUrls ?? {}),
                ...addedUrls,
              })
            }
            onChange([...value, ...files.map((f) => f.name)])
          }}
          className={cn(
            "gap-1.5 p-3 [&_[data-slot=button]]:mt-1",
            mode === "carousel" ? "aspect-square" : "aspect-[4/5]",
          )}
        />
      </div>
      {value.length === 0 ? (
        <p className="text-xs text-muted-foreground">
          {masterDataCopy.mediaEmpty}（{masterDataCopy.mediaAllowEmpty}）
        </p>
      ) : null}

      <Dialog
        open={Boolean(expandedPreview)}
        onOpenChange={(open) => {
          if (!open) setExpandedPreview(null)
        }}
      >
        <DialogContent className="gap-4 p-4 sm:max-w-4xl">
          <DialogHeader className="pr-10">
            <DialogTitle>{expandedPreview?.name ?? "图片预览"}</DialogTitle>
            <DialogDescription>图片预览</DialogDescription>
          </DialogHeader>
          <div className="flex min-h-0 items-center justify-center overflow-hidden rounded-lg bg-surface-sunken">
            {expandedPreview ? (
              // eslint-disable-next-line @next/next/no-img-element -- 本地待上传图片使用 blob URL。
              <img
                src={expandedPreview.src}
                alt={expandedPreview.name}
                className="max-h-[75dvh] max-w-full object-contain"
              />
            ) : null}
          </div>
        </DialogContent>
      </Dialog>
    </div>
  )
}

function SkuMainImageField({
  value,
  previewUrl,
  onChange,
  onFilesSelected,
  disabled = false,
}: {
  value: string
  /** 可访问预览地址（远程 URL 或本地 blob）；缺省回退文件名。 */
  previewUrl?: string
  onChange: (next: string) => void
  /** 选择文件时透出原始文件（用于保存前上传）。 */
  onFilesSelected?: (files: File[]) => void
  disabled?: boolean
}) {
  return (
    <FileUpload
      accept="image/jpeg,image/png,image/webp"
      multiple={false}
      label={masterDataCopy.fMainImage}
      description="1:1"
      density="tile"
      className="aspect-square size-14"
      disabled={disabled}
      previewSelectedImage
      preview={
        value
          ? {
              src: previewUrl ?? imagePreviewSource(value),
              name: value,
              status: "uploaded",
            }
          : null
      }
      onPreviewRemove={() => onChange("")}
      onFilesSelected={(files) => {
        onFilesSelected?.(files)
        if (files[0]) onChange(files[0].name)
      }}
    />
  )
}

const EMPTY_BATCH_REFERENCE_PRICE_FIELDS = {
  batchSalePrice: "",
  batchMarketPrice: "",
} as const

function hydrateFromCenter(
  data: MasterDataCenterView,
): ProductEditorFormValues {
  const fields = data.productDetail
    ? productDetailToFields(data.productDetail)
    : emptyProductFields()
  return {
    name: data.name,
    effectiveFrom: data.currentRevision.effectiveFrom,
    effectiveTo: data.currentRevision.effectiveTo ?? "",
    changeReason: "",
    fields: {
      ...fields,
      productKind: data.productKind ?? "",
    },
    specDrafts: fields.specs.map((s) => ({
      name: s.name,
      values: [...s.values],
    })),
    ...EMPTY_BATCH_REFERENCE_PRICE_FIELDS,
  }
}

function createProductDefaults(isCreate: boolean): ProductEditorFormValues {
  return {
    name: "",
    effectiveFrom: defaultImmediateEffectiveFrom(),
    effectiveTo: "",
    changeReason: isCreate ? "新建商品" : "",
    fields: emptyProductFields(),
    specDrafts: [],
    ...EMPTY_BATCH_REFERENCE_PRICE_FIELDS,
  }
}

export function ProductDetailPage({
  stableId,
}: {
  stableId: string
}) {
  const router = useRouter()
  const isCreate = stableId === "new"
  const accountQuery = useAccountProfileQuery()
  const detailQuery = useMasterDataCenterQuery(
    "products",
    isCreate ? "" : stableId,
  )
  const categoryListQuery = useMasterDataListQuery({
    resource: "categories",
    lifecycleStatus: "enabled",
    revisionTiming: "current",
  })
  const brandListQuery = useMasterDataListQuery({
    resource: "brands",
    lifecycleStatus: "enabled",
    revisionTiming: "current",
  })
  const categoryOptions = React.useMemo(
    () => toCategoryComboboxItems(categoryListQuery.data?.rows ?? []),
    [categoryListQuery.data?.rows],
  )
  const brandOptions = React.useMemo(
    () => toBrandComboboxItems(brandListQuery.data?.rows ?? []),
    [brandListQuery.data?.rows],
  )
  const unitOptionsQuery = useUnitOptionsQuery()
  const createMutation = useCreateMasterDataMutation()
  const reviseMutation = useCreateRevisionMutation()

  const data = detailQuery.data
  const supplierCountsQuery = useSkuSupplierCountsQuery(
    data?.productDetail?.skus.flatMap((sku) => (sku.skuId ? [sku.skuId] : [])) ?? []
  )
  const lockVersion = data?.lockVersion
  const revisionId = data?.currentRevision.revisionId
  const [formError, setFormError] = React.useState<string | null>(null)
  const [formErrorTitle, setFormErrorTitle] = React.useState(
    "填写检查未通过"
  )
  const [checkPassed, setCheckPassed] = React.useState(false)
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null,
  )
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey(isCreate ? "create-product" : "revise-product"),
  )
  const [disableOpen, setDisableOpen] = React.useState(false)
  const [discardOpen, setDiscardOpen] = React.useState(false)
  const [pendingNav, setPendingNav] = React.useState<string | null>(null)
  const [supplierDialogSku, setSupplierDialogSku] =
    React.useState<FixedSku>()
  const [inventoryOpen, setInventoryOpen] = React.useState(false)
  const [inventoryInitialSkuId, setInventoryInitialSkuId] =
    React.useState<string>()
  const [activeSection, setActiveSection] =
    React.useState<ProductEditorSectionId>("basic")
  const errorRef = React.useRef<HTMLDivElement | null>(null)
  const checkedSnapshotRef = React.useRef<string | null>(null)
  const stickyHeaderRef = React.useRef<HTMLElement>(null)
  const [stickyHeaderHeight, setStickyHeaderHeight] = React.useState(64)
  const hydratedKeyRef = React.useRef<string | null>(null)
  const [uploadingMedia, setUploadingMedia] = React.useState(false)
  /** 本会话选择但尚未上传的图片文件；保存时按 fileName / SKU 行号上传并回填。 */
  const pendingFilesRef = React.useRef<Map<string, File>>(new Map())
  const pendingSkuFilesRef = React.useRef<Map<number, File>>(new Map())
  const inventoryTriggerRef = React.useRef<HTMLButtonElement | null>(null)
  const rememberPendingFiles = React.useCallback((files: File[]) => {
    for (const file of files) {
      pendingFilesRef.current.set(file.name, file)
    }
  }, [])
  const rememberSkuFile = React.useCallback((index: number, file?: File) => {
    if (file) pendingSkuFilesRef.current.set(index, file)
  }, [])

  /** 把仍是本地 blob 预览的图片上传为文件资产，返回回填后的字段。 */
  const resolvePendingUploads = React.useCallback(
    async (current: ProductFields): Promise<ProductFields> => {
      const uploadIfPending = async (
        fileName: string,
        previewUrl: string | undefined,
        knownAssetId: string | undefined,
      ): Promise<{ url: string; assetId?: string } | null> => {
        const url = previewUrl?.trim()
        if (!url) return null
        if (url.startsWith("blob:")) {
          const file = pendingFilesRef.current.get(fileName)
          if (!file) {
            throw new Error(`找不到待上传图片「${fileName}」的文件内容，请重新选择`)
          }
          const uploaded = await uploadFileAssetImage(file)
          return { url: uploaded.url, assetId: uploaded.fileAssetId }
        }
        return {
          url,
          ...(knownAssetId?.trim() ? { assetId: knownAssetId } : {}),
        }
      }

      const carouselPreviewUrls: Record<string, string> = {}
      const carouselFileAssetIds: Record<string, string> = {}
      for (const fileName of current.carouselImages) {
        const resolved = await uploadIfPending(
          fileName,
          current.carouselPreviewUrls[fileName],
          current.carouselFileAssetIds[fileName],
        )
        if (resolved) {
          carouselPreviewUrls[fileName] = resolved.url
          if (resolved.assetId) carouselFileAssetIds[fileName] = resolved.assetId
        }
      }
      const detailPreviewUrls: Record<string, string> = {}
      const detailFileAssetIds: Record<string, string> = {}
      for (const fileName of current.detailImages) {
        const resolved = await uploadIfPending(
          fileName,
          current.detailPreviewUrls[fileName],
          current.detailFileAssetIds[fileName],
        )
        if (resolved) {
          detailPreviewUrls[fileName] = resolved.url
          if (resolved.assetId) detailFileAssetIds[fileName] = resolved.assetId
        }
      }
      const skus = [...current.skus]
      for (let index = 0; index < skus.length; index++) {
        const sku = skus[index]
        if (!sku.mainImage) continue
        const previewUrl = sku.mainImagePreviewUrl?.trim()
        if (!previewUrl) continue
        if (!previewUrl.startsWith("blob:")) continue
        const file = pendingSkuFilesRef.current.get(index)
        if (!file) {
          throw new Error(`找不到待上传主图「${sku.mainImage}」的文件内容，请重新选择`)
        }
        const uploaded = await uploadFileAssetImage(file)
        skus[index] = {
          ...sku,
          mainImagePreviewUrl: uploaded.url,
          mainImageAssetId: uploaded.fileAssetId,
        }
      }
      return {
        ...current,
        carouselPreviewUrls,
        carouselFileAssetIds,
        detailPreviewUrls,
        detailFileAssetIds,
        skus,
      }
    },
    [],
  )
  const initialFormValues = React.useMemo(
    () =>
      !isCreate && data
        ? hydrateFromCenter(data)
        : createProductDefaults(isCreate),
    [data, isCreate],
  )

  const form = useAppForm({
    defaultValues: initialFormValues,
    onSubmit: async ({ value }) => {
      setFormError(null)
      setCheckPassed(false)
      setResult(null)

      const nextFields = applySpecsFromDrafts(value.specDrafts, value.fields)
      const validation = validateProductEditor(value, nextFields)
      if (validation) {
        setFormErrorTitle("填写检查未通过")
        setFormError(validation)
        return
      }

      try {
        // 先把仍为本地 blob 的图片上传为文件资产，再携带真实 URL/asset id 保存
        setUploadingMedia(true)
        const resolvedFields = await resolvePendingUploads(nextFields)
        if (!isCreate) {
          if (!data || !revisionId || lockVersion == null) return
          const response = await reviseMutation.mutateAsync({
            resource: "products",
            stableId: data.stableId,
            baseRevisionId: revisionId,
            expectedLockVersion: lockVersion,
            name: value.name.trim(),
            effectiveFrom: value.effectiveFrom,
            effectiveTo: value.effectiveTo.trim() || undefined,
            changeReason: value.changeReason.trim(),
            fields: resolvedFields,
            idempotencyKey,
          })
          if (response.outcome === "succeeded") {
            toast.add({
              title: masterDataCopy.reviseSuccessTitle,
              description: `${masterDataCopy.resultNo} ${response.stableNo} · v${response.revisionNo}`,
              type: "success",
              timeout: 4000,
            })
            setIdempotencyKey(newIdempotencyKey("revise-product"))
            hydratedKeyRef.current = null
            await detailQuery.refetch()
            return
          }
          setResult(response)
          return
        }

        const response = await createMutation.mutateAsync({
          resource: "products",
          name: value.name.trim(),
          effectiveFrom: value.effectiveFrom,
          effectiveTo: value.effectiveTo.trim() || undefined,
          changeReason: value.changeReason.trim(),
          fields: resolvedFields,
          idempotencyKey,
        })
        if (response.outcome === "succeeded") {
          toast.add({
            title: masterDataCopy.createSuccessTitle,
            description: `${masterDataCopy.resultNo} ${response.stableNo} · v${response.revisionNo}`,
            type: "success",
            timeout: 4000,
          })
          router.replace(`/master-data/products/${response.stableId}`)
          return
        }
        setResult(response)
      } catch (error) {
        const failure = getErrorPresentation(error, "保存失败，请稍后重试。")
        setFormErrorTitle(failure.title)
        setFormError(failure.description)
      } finally {
        setUploadingMedia(false)
      }
    },
  })

  React.useEffect(() => {
    if (isCreate || !data) return
    const key = `${data.stableId}:${data.lockVersion}:${data.currentRevision.revisionId}`
    if (hydratedKeyRef.current === key) return
    form.reset(hydrateFromCenter(data))
    hydratedKeyRef.current = key
  }, [data, form, isCreate])

  React.useLayoutEffect(() => {
    const el = stickyHeaderRef.current
    if (!el) return
    const update = () => {
      setStickyHeaderHeight(Math.ceil(el.getBoundingClientRect().height))
    }
    update()
    const observer = new ResizeObserver(update)
    observer.observe(el)
    return () => observer.disconnect()
  }, [isCreate, data?.stableId, data?.lockVersion])

  // 未保存离开保护：刷新 / 关闭标签页 / 返回列表
  React.useEffect(() => {
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      if (form.state.isDirty) {
        event.preventDefault()
      }
    }
    window.addEventListener("beforeunload", onBeforeUnload)
    return () => window.removeEventListener("beforeunload", onBeforeUnload)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅挂载时注册一次
  }, [])

  // 校验错误出现时滚动到错误条（P2-15）
  React.useEffect(() => {
    if (formError) {
      errorRef.current?.scrollIntoView({ block: "center", behavior: "smooth" })
    }
  }, [formError])

  // 分区 Tab 随滚动高亮（P2-19 scroll spy）
  React.useEffect(() => {
    if (isCreate) return
    const sections = PRODUCT_EDITOR_SECTIONS.map((s) =>
      document.getElementById(`product-section-${s.id}`)
    ).filter((el): el is HTMLElement => el !== null)
    if (sections.length === 0) return
    const observer = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (entry.isIntersecting) {
            const id = entry.target.id.replace("product-section-", "")
            setActiveSection(id as ProductEditorSectionId)
          }
        }
      },
      { rootMargin: "-20% 0px -65% 0px", threshold: 0 }
    )
    for (const section of sections) observer.observe(section)
    return () => observer.disconnect()
  }, [isCreate, data?.stableId])

  const navigateAway = React.useCallback(
    (href: string) => {
      if (form.state.isDirty) {
        setPendingNav(href)
        setDiscardOpen(true)
        return
      }
      router.push(href)
    },
    [form.state.isDirty, router]
  )

  const openInventoryPreview = React.useCallback(
    (skuId: string | undefined, trigger: HTMLButtonElement) => {
      inventoryTriggerRef.current = trigger
      setInventoryInitialSkuId(skuId)
      setInventoryOpen(true)
    },
    [],
  )

  const handleInventoryOpenChange = React.useCallback((open: boolean) => {
    setInventoryOpen(open)
    if (!open) {
      globalThis.requestAnimationFrame(() => inventoryTriggerRef.current?.focus())
    }
  }, [])

  const listHref = "/master-data/products"
  const stickyOffsetPx = stickyHeaderHeight
  const sectionScrollMarginPx = stickyHeaderHeight + 56
  const pending =
    createMutation.isPending || reviseMutation.isPending || uploadingMedia
  const granted = accountQuery.data?.permissions
  const canCreate = hasPermission(granted, "product:create")
  const hasUpdatePermission = hasPermission(granted, "product:update")
  const canRevise =
    isCreate
      ? canCreate
      : hasUpdatePermission &&
        (data?.allowedActions.includes("CREATE_REVISION") ?? false)
  const canDisable =
    hasUpdatePermission && (data?.allowedActions.includes("DISABLE") ?? false)
  const reviseBlocker = data?.actionBlockers.find(
    (b) => b.action === "CREATE_REVISION",
  )
  const disableBlocker = data?.actionBlockers.find(
    (b) => b.action === "DISABLE",
  )

  const runLocalCheck = (values: ProductEditorFormValues) => {
    setFormError(null)
    setCheckPassed(false)
    setResult(null)
    const nextFields = applySpecsFromDrafts(values.specDrafts, values.fields)
    form.setFieldValue("fields", nextFields)
    const validation = validateProductEditor(values, nextFields)
    if (validation) {
      setFormErrorTitle("填写检查未通过")
      setFormError(validation)
      return
    }
    // 记录检查通过时的内容快照；后续任何字段变更都会让「通过」态失效
    checkedSnapshotRef.current = JSON.stringify({
      ...values,
      fields: nextFields,
    })
    setCheckPassed(true)
  }

  if (!isCreate && detailQuery.isPending) {
    return (
      <PageScaffold>
        <PageHeader
          title="商品详情"
          description={masterDataCopy.centerLoading}
        />
        <div className="h-40 animate-pulse rounded-lg bg-muted" aria-busy />
      </PageScaffold>
    )
  }

  if (!isCreate && (detailQuery.isError || !data)) {
    return (
      <PageScaffold>
        <PageHeader title="商品详情" />
        <BusinessFailureState
          error={detailQuery.isError ? detailQuery.error : undefined}
          description={
            detailQuery.isError
              ? masterDataCopy.centerLoadFail
              : masterDataCopy.centerMissingDesc
          }
          action={
            detailQuery.isError ? (
              <Button type="button" onClick={() => void detailQuery.refetch()}>
                重试
              </Button>
            ) : (
              <Button render={<Link href={listHref} />}>
                {masterDataCopy.actionBackList}
              </Button>
            )
          }
        />
      </PageScaffold>
    )
  }

  if (isCreate && accountQuery.isPending) {
    return (
      <PageScaffold>
        <PageHeader title="新建商品" description="正在核对创建权限" />
        <div className="h-40 animate-pulse rounded-lg bg-muted" aria-busy />
      </PageScaffold>
    )
  }

  if (isCreate && accountQuery.isError) {
    return (
      <PageScaffold>
        <PageHeader title="新建商品" />
        <BusinessFailureState
          error={accountQuery.error}
          onRetry={() => void accountQuery.refetch()}
        />
      </PageScaffold>
    )
  }

  if (isCreate && !canCreate) {
    return (
      <PageScaffold>
        <PageHeader title="新建商品" />
        <BusinessFailureState
          kind="permission"
          description="当前账号没有创建商品的权限，请联系管理员或有权限的同事。"
          action={<Button render={<Link href={listHref} />}>返回列表</Button>}
        />
      </PageScaffold>
    )
  }

  const formId = "product-detail-form"

  return (
    <form.Subscribe selector={(state) => state.values}>
      {(values) => {
        const title = isCreate
          ? masterDataCopy.productCreateTitle
          : values.name || data?.name || "商品详情"
        const fields = values.fields
        const inventoryPreviewSkus: ProductInventoryPreviewSku[] =
          fields.productKind === "PHYSICAL"
            ? fields.skus.flatMap((sku) =>
                sku.skuId
                  ? [
                      {
                        skuId: sku.skuId,
                        skuNo: sku.skuNo,
                        specLabel: sku.specLabel,
                        baseUnit: sku.baseUnit || fields.baseUnit,
                      },
                    ]
                  : [],
              )
            : []
        const inventoryActionHint =
          fields.productKind && fields.productKind !== "PHYSICAL"
            ? "仅实物商品适用公司自有库存台账"
            : inventoryPreviewSkus.length === 0
              ? "选择实物商品类型并保存 SKU 后可查看正式库存"
              : undefined
        const setName = (next: string) => form.setFieldValue("name", next)
        const setEffectiveFrom = (next: string) =>
          form.setFieldValue("effectiveFrom", next)
        const setEffectiveTo = (next: string) =>
          form.setFieldValue("effectiveTo", next)
        const setChangeReason = (next: string) =>
          form.setFieldValue("changeReason", next)
        const setFields = (next: React.SetStateAction<ProductFields>) =>
          form.setFieldValue("fields", (previous) =>
            typeof next === "function" ? next(previous) : next,
          )
        const setSpecDrafts = (
          next: React.SetStateAction<readonly ProductSpecDraft[]>,
        ) =>
          form.setFieldValue("specDrafts", (previous) =>
            typeof next === "function" ? next(previous) : next,
          )
        const syncSpecDrafts = (next: readonly ProductSpecDraft[]) => {
          setSpecDrafts(next)
          setFields((previous) => applySpecsFromDrafts(next, previous))
        }
        const updateSku = (index: number, patch: Partial<ProductSkuFields>) => {
          setFields((previous) => ({
            ...previous,
            skus: previous.skus.map((sku, skuIndex) =>
              skuIndex === index ? { ...sku, ...patch } : sku,
            ),
          }))
        }
        const handleSubmit = (event?: React.FormEvent) => {
          event?.preventDefault()
          void form.handleSubmit()
        }
        const name = values.name
        const effectiveFrom = values.effectiveFrom
        const effectiveTo = values.effectiveTo
        const changeReason = values.changeReason
        const specDrafts = values.specDrafts
        const activeSpecs = fields.specs.filter(
          (spec) =>
            spec.name.trim() && spec.values.some((value) => value.trim()),
        )
        const applyBatchReferencePrices = () => {
          const hasAny =
            values.batchSalePrice.trim() ||
            values.batchMarketPrice.trim()
          if (!hasAny) return
          const hasFilled = values.fields.skus.some(
            (sku) => sku.salePrice?.trim() || sku.marketPrice?.trim()
          )
          const message = hasFilled
            ? `将把批量价格应用到全部 ${values.fields.skus.length} 个 SKU，并覆盖已填写的销售价/市场价。确定继续？`
            : `将把批量价格应用到全部 ${values.fields.skus.length} 个 SKU。确定继续？`
          if (!window.confirm(message)) return
          setFields((previous) => ({
            ...previous,
            skus: previous.skus.map((sku) => ({
              ...sku,
              salePrice:
                values.batchSalePrice.trim() || sku.salePrice || undefined,
              marketPrice:
                values.batchMarketPrice.trim() || sku.marketPrice || undefined,
            })),
          }))
        }
        return (
          <PageScaffold
            style={
              {
                "--product-sticky-offset": `${stickyOffsetPx}px`,
                "--product-section-scroll-margin": `${sectionScrollMarginPx}px`,
              } as React.CSSProperties
            }
          >
            <form id={formId} onSubmit={handleSubmit}>
              {/*
                本页专用吸顶作业条：身份信息 + 主操作。
                与 PageHeader / DocumentHeader 职责差异过大（无面包屑、合入表单动作、吸顶），
                故不复用通用组件，避免为单页堆叠不兼容 props。
              */}
              <header
                ref={stickyHeaderRef}
                className="sticky top-0 z-30 border-b border-border/30 bg-background/95 py-3 backdrop-blur"
              >
                <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
                  <div className="min-w-0 space-y-1">
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                      <h1 className="truncate text-lg font-semibold tracking-tight">
                        {title}
                      </h1>
                      {!isCreate && data ? (
                        <StatusBadge
                          tone={data.lifecycleTone}
                          label={data.lifecycleStatusLabel}
                        />
                      ) : null}
                    </div>
                    {!isCreate && data ? (
                      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                        <span>
                          单号{" "}
                          <span className="num text-foreground">
                            {data.stableNo}
                          </span>
                        </span>
                        <span className="num rounded-md bg-muted px-1.5 py-0.5 text-tiny text-foreground">
                          版本 {data.currentRevision.revisionNo}
                        </span>
                        <span className="num">
                          {formatEffectiveRange(
                            data.currentRevision.effectiveFrom,
                            data.currentRevision.effectiveTo,
                          )}
                        </span>
                        <span className="inline-flex items-center gap-1.5">
                          <span>{masterDataCopy.centerVersionState}</span>
                          <StatusBadge
                            tone={
                              data.revisionTiming === "FUTURE"
                                ? "warning"
                                : "info"
                            }
                            label={data.revisionTimingLabel}
                          />
                        </span>
                      </div>
                    ) : (
                      <p className="text-sm text-muted-foreground">
                        {masterDataCopy.productCreateDesc}
                      </p>
                    )}
                  </div>

                  <div className="flex shrink-0 flex-wrap items-center gap-2">
                    {!isCreate && data ? (
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={!canDisable}
                        title={
                          !hasUpdatePermission
                            ? "当前账号没有维护商品资料的权限。"
                            : disableBlocker?.message
                        }
                        onClick={() => setDisableOpen(true)}
                      >
                        <BanIcon data-icon="inline-start" aria-hidden />
                        {masterDataCopy.actionDisable}
                      </Button>
                    ) : null}
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      onClick={() => navigateAway(listHref)}
                    >
                      返回列表
                    </Button>
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      disabled={!canRevise || pending}
                      onClick={() => runLocalCheck(values)}
                    >
                      <ClipboardCheckIcon
                        data-icon="inline-start"
                        aria-hidden
                      />
                      填写检查
                    </Button>
                    <Button
                      type="submit"
                      size="sm"
                      disabled={!canRevise || pending}
                    >
                      <SaveIcon data-icon="inline-start" aria-hidden />
                      {isCreate
                        ? masterDataCopy.createSubmit
                        : masterDataCopy.reviseSubmit}
                    </Button>
                  </div>
                </div>
              </header>

              <div className="flex flex-col gap-4">
                {!isCreate && !canRevise ? (
                  <Alert variant="info">
                    <AlertTitle>你只能查看</AlertTitle>
                    <AlertDescription>
                      {reviseBlocker
                        ? masterDataCopy.centerUpdateBlocked(
                            reviseBlocker.message
                          )
                        : "当前账号没有维护商品资料的权限；需要修改请联系有权限的同事。"}
                    </AlertDescription>
                  </Alert>
                ) : null}

                {!isCreate && data?.productConstraints ? (
                  <div className="rounded-lg bg-muted/50 p-3 text-xs">
                    <p>
                      基础单位{" "}
                      <span className="num">
                        {data.productConstraints.baseUnit}
                      </span>
                      {" · "}
                      SKU{" "}
                      <span className="num">
                        {data.productConstraints.skuCount}
                      </span>{" "}
                      个
                      {data.productConstraints.hasFormalReferences
                        ? " · 已被业务单据引用"
                        : null}
                    </p>
                    <p className="mt-1 text-muted-foreground">
                      {masterDataCopy.centerSpecNote}
                    </p>
                  </div>
                ) : null}

                {result?.outcome === "blocked" ? (
                  <FormalActionResult
                    status="blocked"
                    title={
                      isCreate
                        ? masterDataCopy.createBlockedTitle
                        : masterDataCopy.reviseBlockedTitle
                    }
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
                    description={
                      result.message || masterDataCopy.reviseConflictHint
                    }
                  />
                ) : null}

                <div className="min-w-0 space-y-4">
                  <nav
                    aria-label="商品编辑分区"
                    className={cn(
                      "sticky z-10 grid grid-cols-2 gap-0.5 rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10",
                      isCreate ? "sm:grid-cols-4" : "sm:grid-cols-5",
                    )}
                    style={{ top: stickyOffsetPx }}
                  >
                    {PRODUCT_EDITOR_SECTIONS.filter(
                      (section) => !isCreate || section.id !== "history",
                    ).map((section) => {
                      const active = activeSection === section.id
                      return (
                        <Button
                          key={section.id}
                          type="button"
                          variant="ghost"
                          size="sm"
                          className={cn(
                            "relative h-7 rounded-md text-sm",
                            active
                              ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10 hover:bg-card"
                              : "text-muted-foreground hover:bg-foreground/5 hover:text-foreground"
                          )}
                          aria-current={active ? "location" : undefined}
                          onClick={() => {
                            setActiveSection(section.id)
                            scrollToProductSection(section.id)
                          }}
                        >
                          {section.label}
                        </Button>
                      )
                    })}
                  </nav>

                  {formError ? (
                    <div ref={errorRef}>
                      <Alert variant="destructive">
                        <CircleAlertIcon aria-hidden />
                        <AlertTitle>{formErrorTitle}</AlertTitle>
                        <AlertDescription>{formError}</AlertDescription>
                      </Alert>
                    </div>
                  ) : null}

                  {checkPassed &&
                  checkedSnapshotRef.current ===
                    JSON.stringify({
                      ...values,
                      fields,
                    }) ? (
                    <Alert variant="success">
                      <CheckCircle2Icon aria-hidden />
                      <AlertTitle>填写检查通过</AlertTitle>
                      <AlertDescription>
                        必填项完整，保存时仍以系统校验结果为准。
                      </AlertDescription>
                    </Alert>
                  ) : null}

                  <fieldset
                    id="product-section-basic"
                    className={cn(surfacePanelClassName, "scroll-mt-[var(--product-section-scroll-margin)] space-y-3 p-5")}
                    disabled={!canRevise}
                  >
                    <legend className="px-1 text-base font-semibold">
                      {masterDataCopy.fieldIdentitySection}
                    </legend>
                    <p className="text-xs text-muted-foreground">
                      {masterDataCopy.productEditDesc}
                    </p>
                    <div className="grid gap-3 sm:grid-cols-2">
                      <div className="space-y-1.5 sm:col-span-2">
                        <Label htmlFor="product-no">商品编号</Label>
                        <Input
                          id="product-no"
                          value={fields.productNo}
                          disabled={!isCreate}
                          onChange={(event) =>
                            setFields((previous) => ({
                              ...previous,
                              productNo: event.target.value,
                            }))
                          }
                          placeholder="请输入全局唯一商品编号"
                        />
                        {!isCreate ? (
                          <p className="text-xs text-muted-foreground">
                            商品编号创建后不可修改。
                          </p>
                        ) : null}
                      </div>
                      <div className="space-y-1.5 sm:col-span-2">
                        <Label htmlFor="product-name">名称</Label>
                        <Input
                          id="product-name"
                          value={name}
                          onChange={(e) => setName(e.target.value)}
                          placeholder="商品名称（SPU）"
                        />
                      </div>
                      <div className="space-y-1.5 sm:col-span-2">
                        <Label htmlFor="product-description">商品描述</Label>
                        <Textarea
                          id="product-description"
                          value={fields.description ?? ""}
                          onChange={(event) =>
                            setFields((previous) => ({
                              ...previous,
                              description: event.target.value,
                            }))
                          }
                          placeholder="公司审核后的商品描述"
                        />
                      </div>
                      <div className="space-y-1.5 sm:col-span-2">
                        <Label>商品类型</Label>
                        {isCreate ? (
                          <OptionCombobox
                            value={fields.productKind || null}
                            onValueChange={(value) =>
                              setFields((previous) => ({
                                ...previous,
                                productKind: (value ?? "") as ProductKind,
                              }))
                            }
                            options={PRODUCT_KIND_VALUES.map((kind) => ({
                              value: kind,
                              label: PRODUCT_KIND_LABELS[kind],
                            }))}
                            allowClear={false}
                            placeholder="请选择商品类型"
                            className="w-full"
                          />
                        ) : (
                          <div className="flex h-9 items-center rounded-md border border-border bg-muted/40 px-3 text-sm">
                            {fields.productKind
                              ? PRODUCT_KIND_LABELS[fields.productKind]
                              : "—"}
                          </div>
                        )}
                        <p className="text-xs text-muted-foreground">
                          决定商品业务作用；创建后不可变，也不随分类变化。
                        </p>
                      </div>
                      <div className="space-y-1.5">
                        <Label>{masterDataCopy.fBaseUnit}</Label>
                        <OptionCombobox
                          value={fields.baseUnitId || null}
                          onValueChange={(id) => {
                            const unit = unitOptionsQuery.data?.find(
                              (item) => item.id === id,
                            )
                            setFields((prev) => ({
                              ...prev,
                              baseUnitId: unit?.id ?? "",
                              baseUnitCode: unit?.code ?? "",
                              baseUnit: unit?.label ?? "",
                              skus: prev.skus.map((sku) => ({
                                ...sku,
                                baseUnit: unit?.label ?? "",
                              })),
                            }))
                          }}
                          options={(unitOptionsQuery.data ?? []).map((unit) => ({
                            value: unit.id,
                            label: `${unit.label} · ${unit.code}`,
                          }))}
                          allowClear={false}
                          placeholder="请选择基础单位"
                          className="w-full"
                        />
                      </div>
                      <div className="space-y-1.5">
                        <Label>{masterDataCopy.fCategory}</Label>
                        <CategoryCombobox
                          categories={categoryOptions}
                          value={fields.categoryId || undefined}
                          onValueChange={(id) => {
                            const hit = categoryOptions.find(
                              (c) => c.categoryId === id,
                            )
                            setFields((prev) => ({
                              ...prev,
                              categoryId: id ?? "",
                              category: hit?.categoryName ?? "",
                            }))
                          }}
                          loading={categoryListQuery.isPending}
                          placeholder="请选择分类"
                          emptyLabel="暂无可用分类，请先在商品分类中维护"
                          className="w-full"
                        />
                      </div>
                      <div className="space-y-1.5">
                        <Label>{masterDataCopy.fBrand}</Label>
                        <BrandCombobox
                          brands={brandOptions}
                          value={fields.brandId || undefined}
                          onValueChange={(id) => {
                            const hit = brandOptions.find(
                              (b) => b.brandId === id,
                            )
                            setFields((prev) => ({
                              ...prev,
                              brandId: id ?? "",
                              brand: hit?.brandName ?? "",
                            }))
                          }}
                          loading={brandListQuery.isPending}
                          placeholder="请选择品牌"
                          emptyLabel="暂无可用品牌，请先在品牌中维护"
                          className="w-full"
                        />
                      </div>
                    </div>
                  </fieldset>

                  <fieldset
                    id="product-section-media"
                    className={cn(surfacePanelClassName, "scroll-mt-[var(--product-section-scroll-margin)] space-y-5 p-5")}
                    disabled={!canRevise}
                  >
                    <legend className="px-1 text-base font-semibold">
                      {masterDataCopy.fieldMediaSection}
                    </legend>
                    <p className="text-xs text-muted-foreground">
                      {masterDataCopy.productSpuMediaHint}
                    </p>
                    <section className="space-y-3">
                      <MediaListEditor
                        label={masterDataCopy.fCarouselImages}
                        hint="建议上传 3–5 张，支持排序；首张作为商品首图"
                        value={fields.carouselImages}
                        previewUrls={fields.carouselPreviewUrls}
                        onFilesSelected={rememberPendingFiles}
                        onChange={(next) =>
                          setFields((prev) => {
                            const retained = new Set(next)
                            return {
                              ...prev,
                              carouselImages: next,
                              carouselPreviewUrls: Object.fromEntries(
                                Object.entries(prev.carouselPreviewUrls).filter(
                                  ([name]) => retained.has(name),
                                ),
                              ),
                              carouselFileAssetIds: Object.fromEntries(
                                Object.entries(prev.carouselFileAssetIds).filter(
                                  ([name]) => retained.has(name),
                                ),
                              ),
                            }
                          })
                        }
                        onPreviewUrlsChange={(next) =>
                          setFields((prev) => ({
                            ...prev,
                            carouselPreviewUrls: next,
                          }))
                        }
                      />
                    </section>
                    <div className="border-t border-border" />
                    <section className="space-y-3">
                      <MediaListEditor
                        label={masterDataCopy.fDetailImages}
                        hint="支持批量上传与顺序调整，保存后详情图随商品版本一起保留"
                        value={fields.detailImages}
                        previewUrls={fields.detailPreviewUrls}
                        onFilesSelected={rememberPendingFiles}
                        mode="detail"
                        onChange={(next) =>
                          setFields((prev) => {
                            const retained = new Set(next)
                            return {
                              ...prev,
                              detailImages: next,
                              detailPreviewUrls: Object.fromEntries(
                                Object.entries(prev.detailPreviewUrls).filter(
                                  ([name]) => retained.has(name),
                                ),
                              ),
                              detailFileAssetIds: Object.fromEntries(
                                Object.entries(prev.detailFileAssetIds).filter(
                                  ([name]) => retained.has(name),
                                ),
                              ),
                            }
                          })
                        }
                        onPreviewUrlsChange={(next) =>
                          setFields((prev) => ({
                            ...prev,
                            detailPreviewUrls: next,
                          }))
                        }
                      />
                    </section>
                  </fieldset>

                  <fieldset
                    id="product-section-sku"
                    className={cn(surfacePanelClassName, "scroll-mt-[var(--product-section-scroll-margin)] space-y-4 p-5")}
                    disabled={!canRevise}
                  >
                    <legend className="px-1 text-base font-semibold">
                      商品规格
                    </legend>
                    <div className="flex flex-wrap items-center justify-between gap-3">
                      <p className="text-xs text-muted-foreground">
                        规格值会自动组合成 SKU；调整规格顺序时保留可匹配的原 SKU
                        数据。
                      </p>
                      <Badge variant="secondary">
                        {specDrafts.length} 个规格项 · {fields.skus.length} 个
                        SKU
                      </Badge>
                    </div>
                    <div className="space-y-3">
                      {specDrafts.map((draft, index) => (
                        <div
                          key={index}
                          className="rounded-xl border border-border bg-surface-sunken"
                        >
                          <div className="flex flex-wrap items-end gap-3 border-b border-border px-3 py-3">
                            <div className="flex items-center gap-2 self-center">
                              <GripVerticalIcon
                                className="size-4 text-muted-foreground"
                                aria-hidden
                              />
                              <Badge variant="outline">
                                规格项 {index + 1}
                              </Badge>
                            </div>
                            <div className="min-w-48 flex-1 space-y-1.5 sm:max-w-sm">
                              <Label
                                htmlFor={`product-spec-name-${index}`}
                                className="text-sm font-medium text-foreground"
                              >
                                规格名称
                              </Label>
                              <Input
                                id={`product-spec-name-${index}`}
                                className="bg-background font-medium shadow-sm"
                                value={draft.name}
                                onChange={(event) => {
                                  const next = [...specDrafts]
                                  next[index] = {
                                    ...draft,
                                    name: event.target.value,
                                  }
                                  syncSpecDrafts(next)
                                }}
                                placeholder="规格名称，如：颜色"
                              />
                            </div>
                            <div className="ml-auto flex items-center gap-1">
                              <Button
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                disabled={index === 0}
                                aria-label={`规格项 ${index + 1} 上移`}
                                onClick={() =>
                                  syncSpecDrafts(
                                    moveListItem(specDrafts, index, index - 1),
                                  )
                                }
                              >
                                <ArrowUpIcon />
                              </Button>
                              <Button
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                disabled={index === specDrafts.length - 1}
                                aria-label={`规格项 ${index + 1} 下移`}
                                onClick={() =>
                                  syncSpecDrafts(
                                    moveListItem(specDrafts, index, index + 1),
                                  )
                                }
                              >
                                <ArrowDownIcon />
                              </Button>
                              <Button
                                type="button"
                                variant="ghost"
                                size="icon-xs"
                                aria-label={`删除规格项 ${index + 1}`}
                                onClick={() => {
                                  if (
                                    !window.confirm(
                                      "删除规格项会移除对应组合生成的 SKU 行（含价格、主图、条码）。确定删除？"
                                    )
                                  ) {
                                    return
                                  }
                                  syncSpecDrafts(
                                    specDrafts.filter((_, i) => i !== index),
                                  )
                                }}
                              >
                                <XIcon />
                              </Button>
                            </div>
                          </div>
                          <div className="space-y-2 p-3">
                            <Label className="text-xs text-muted-foreground">
                              规格值
                            </Label>
                            <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
                              {draft.values.map((specValue, valueIndex) => (
                                <div
                                  key={valueIndex}
                                  className="flex items-center gap-1"
                                >
                                  <Input
                                    className="h-8 bg-background"
                                    value={specValue}
                                    onChange={(event) => {
                                      const nextValues = [...draft.values]
                                      nextValues[valueIndex] =
                                        event.target.value
                                      const next = [...specDrafts]
                                      next[index] = {
                                        ...draft,
                                        values: nextValues,
                                      }
                                      syncSpecDrafts(next)
                                    }}
                                    placeholder={`请输入${draft.name || "规格"}`}
                                    aria-label={`${draft.name || `规格项 ${index + 1}`}的第 ${valueIndex + 1} 个值`}
                                  />
                                  <Button
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label={`删除规格值 ${specValue || valueIndex + 1}`}
                                    onClick={() => {
                                      if (
                                        !window.confirm(
                                          "删除规格取值会移除对应组合生成的 SKU 行（含价格、主图、条码）。确定删除？"
                                        )
                                      ) {
                                        return
                                      }
                                      const next = [...specDrafts]
                                      next[index] = {
                                        ...draft,
                                        values: draft.values.filter(
                                          (_, i) => i !== valueIndex,
                                        ),
                                      }
                                      syncSpecDrafts(next)
                                    }}
                                  >
                                    <XIcon />
                                  </Button>
                                </div>
                              ))}
                            </div>
                            <Button
                              type="button"
                              variant="outline"
                              size="xs"
                              onClick={() => {
                                const next = [...specDrafts]
                                next[index] = {
                                  ...draft,
                                  values: [...draft.values, ""],
                                }
                                syncSpecDrafts(next)
                              }}
                            >
                              <PlusIcon data-icon="inline-start" aria-hidden />
                              添加规格值
                            </Button>
                          </div>
                        </div>
                      ))}
                    </div>
                    <Button
                      type="button"
                      variant="outline"
                      size="sm"
                      onClick={() =>
                        syncSpecDrafts([
                          ...specDrafts,
                          { name: "", values: [""] },
                        ])
                      }
                    >
                      <PlusIcon data-icon="inline-start" aria-hidden />
                      添加规格项
                    </Button>
                  </fieldset>

                  <fieldset
                    className={cn(surfacePanelClassName, "min-w-0 max-w-full space-y-4 overflow-hidden p-5")}
                  >
                    <legend className="px-1 text-base font-semibold">
                      SKU
                    </legend>
                    <div className="flex flex-wrap items-center justify-between gap-3">
                      <div className="min-w-0 space-y-1">
                        <p className="text-xs text-muted-foreground">
                          {masterDataCopy.productSkuHint}
                        </p>
                      </div>
                      <Badge variant="success">
                        共 {fields.skus.length} 个 SKU
                      </Badge>
                    </div>
                    <div className="grid gap-2 rounded-xl border border-border bg-surface-sunken p-3 sm:grid-cols-2 lg:grid-cols-[repeat(2,minmax(0,1fr))_auto_auto]">
                      <div className="space-y-1">
                        <Label htmlFor="bulk-sale-price" className="text-xs">
                          批量销售价
                        </Label>
                        <Input
                          id="bulk-sale-price"
                          className="h-8 bg-background"
                          value={values.batchSalePrice}
                          disabled={!canRevise}
                          onChange={(event) =>
                            form.setFieldValue(
                              "batchSalePrice",
                              event.target.value,
                            )
                          }
                          placeholder="可选"
                        />
                      </div>
                      <div className="space-y-1">
                        <Label htmlFor="bulk-market-price" className="text-xs">
                          批量市场价
                        </Label>
                        <Input
                          id="bulk-market-price"
                          className="h-8 bg-background"
                          value={values.batchMarketPrice}
                          disabled={!canRevise}
                          onChange={(event) =>
                            form.setFieldValue(
                              "batchMarketPrice",
                              event.target.value,
                            )
                          }
                          placeholder="可选"
                        />
                      </div>
                      <Button
                        type="button"
                        variant="secondary"
                        size="sm"
                        className="self-end"
                        disabled={
                          !canRevise ||
                          (!values.batchSalePrice.trim() &&
                            !values.batchMarketPrice.trim())
                        }
                        onClick={applyBatchReferencePrices}
                      >
                        批量设置
                      </Button>
                      <Button
                        type="button"
                        variant="outline"
                        size="sm"
                        className="self-end"
                        disabled={Boolean(inventoryActionHint)}
                        title={inventoryActionHint}
                        onClick={(event) =>
                          openInventoryPreview(
                            inventoryPreviewSkus[0]?.skuId,
                            event.currentTarget,
                          )
                        }
                      >
                        查看商品库存
                      </Button>
                    </div>
                    {fields.skus.length === 0 ? (
                      <p className="text-sm text-muted-foreground">
                        {masterDataCopy.productNoSkus}
                      </p>
                    ) : (
                      <div className="w-full max-w-full overflow-x-auto overscroll-x-contain rounded-xl border border-border">
                        <table className="w-full min-w-[64rem] border-collapse text-sm">
                          <thead className="bg-surface-sunken">
                            <tr className="border-b border-border text-left text-xs text-muted-foreground">
                              {activeSpecs.length > 0 ? (
                                <th
                                  colSpan={activeSpecs.length}
                                  className="px-3 py-2 font-medium"
                                >
                                  规格
                                </th>
                              ) : (
                                <th className="px-3 py-2 font-medium">规格</th>
                              )}
                              <th
                                colSpan={3}
                                className="border-l border-border px-3 py-2 font-medium"
                              >
                                身份
                              </th>
                              <th
                                colSpan={2}
                                className="border-l border-border px-3 py-2 font-medium"
                              >
                                公司商品池价格
                              </th>
                              <th
                                colSpan={3}
                                className="border-l border-border px-3 py-2 font-medium"
                              >
                                关联与状态
                              </th>
                            </tr>
                            <tr className="border-b border-border text-left text-xs text-muted-foreground">
                              {activeSpecs.length > 0 ? (
                                activeSpecs.map((spec) => (
                                  <th
                                    key={spec.name}
                                    className="min-w-24 px-3 py-2 font-medium"
                                  >
                                    {spec.name}
                                  </th>
                                ))
                              ) : (
                                <th className="min-w-24 px-3 py-2 font-medium">
                                  —
                                </th>
                              )}
                              <th className="min-w-32 border-l border-border px-3 py-2 font-medium">
                                {masterDataCopy.fProductCode}
                              </th>
                              <th className="min-w-32 px-3 py-2 font-medium">
                                {masterDataCopy.fBarcode}
                              </th>
                              <th className="min-w-36 px-3 py-2 font-medium">
                                {masterDataCopy.fMainImage}
                              </th>
                              <th className="min-w-28 border-l border-border px-3 py-2 font-medium">
                                {masterDataCopy.fSalePrice}
                              </th>
                              <th className="min-w-28 px-3 py-2 font-medium">
                                {masterDataCopy.fMarketPrice}
                              </th>
                              <th className="min-w-32 border-l border-border px-3 py-2 font-medium">
                                供给
                              </th>
                              <th className="min-w-28 px-3 py-2 font-medium">
                                库存
                              </th>
                              <th className="min-w-24 px-3 py-2 font-medium">
                                启用
                              </th>
                            </tr>
                          </thead>
                          <tbody>
                            {fields.skus.map((sku, index) => {
                              const supplierCount = sku.skuId
                                ? supplierCountsQuery.data?.get(sku.skuId)
                                : 0
                              return (
                                <tr
                                key={`${sku.skuNo}-${index}`}
                                className="border-b border-border/70 align-top last:border-b-0"
                              >
                                {activeSpecs.length > 0 ? (
                                  activeSpecs.map((spec, specIndex) => (
                                    <td
                                      key={`${spec.name}-${specIndex}`}
                                      className="px-3 py-3"
                                    >
                                      <Badge variant="secondary">
                                        {sku.attributeValues[specIndex] || "—"}
                                      </Badge>
                                    </td>
                                  ))
                                ) : (
                                  <td className="px-3 py-3">
                                    <Badge variant="secondary">
                                      {masterDataCopy.productDefaultSpec}
                                    </Badge>
                                  </td>
                                )}
                                <td className="border-l border-border px-3 py-3">
                                  <Input
                                    className="h-8"
                                    value={sku.skuNo}
                                    disabled={!canRevise}
                                    onChange={(event) =>
                                      updateSku(index, {
                                        skuNo: event.target.value,
                                      })
                                    }
                                    aria-label={`${sku.specLabel} 产品编码`}
                                    title="系统默认生成，可手动覆盖"
                                  />
                                </td>
                                <td className="px-3 py-3">
                                  <Input
                                    className="h-8"
                                    value={sku.barcode ?? ""}
                                    disabled={!canRevise}
                                    onChange={(event) =>
                                      updateSku(index, {
                                        barcode:
                                          event.target.value || undefined,
                                      })
                                    }
                                    aria-label={`${sku.specLabel} 条形码`}
                                  />
                                </td>
                                <td className="px-3 py-3">
                                  <SkuMainImageField
                                    value={sku.mainImage}
                                    previewUrl={sku.mainImagePreviewUrl}
                                    disabled={!canRevise}
                                    onChange={(mainImage) =>
                                      updateSku(
                                        index,
                                        mainImage
                                          ? { mainImage }
                                          : {
                                              mainImage: "",
                                              mainImagePreviewUrl: undefined,
                                              mainImageAssetId: undefined,
                                            },
                                      )
                                    }
                                    onFilesSelected={(files) => {
                                      const file = files[0]
                                      rememberSkuFile(index, file)
                                      if (file) {
                                        updateSku(index, {
                                          mainImage: file.name,
                                          mainImagePreviewUrl:
                                            URL.createObjectURL(file),
                                          mainImageAssetId: undefined,
                                        })
                                      }
                                    }}
                                  />
                                </td>
                                <td className="border-l border-border px-3 py-3">
                                  <MoneyInput
                                    value={sku.salePrice ?? ""}
                                    disabled={!canRevise}
                                    onChange={(next) =>
                                      updateSku(index, {
                                        salePrice: next || undefined,
                                      })
                                    }
                                    aria-label={`${sku.specLabel} 销售价`}
                                  />
                                </td>
                                <td className="px-3 py-3">
                                  <MoneyInput
                                    value={sku.marketPrice ?? ""}
                                    disabled={!canRevise}
                                    onChange={(next) =>
                                      updateSku(index, {
                                        marketPrice: next || undefined,
                                      })
                                    }
                                    aria-label={`${sku.specLabel} 市场价`}
                                  />
                                </td>
                                <td className="border-l border-border px-3 py-3">
                                  <div className="space-y-1.5">
                                    {sku.skuId && !isCreate ? (
                                      <HoverCard>
                                        <HoverCardTrigger
                                          render={
                                            <Badge
                                              variant="outline"
                                              className="cursor-pointer"
                                            />
                                          }
                                        >
                                          {supplierCountsQuery.isPending
                                            ? "…"
                                            : supplierCountsQuery.isError
                                              ? "供给暂不可查"
                                              : `${supplierCount ?? 0} 家供应商`}
                                        </HoverCardTrigger>
                                        <HoverCardContent
                                          align="start"
                                          className="w-64 space-y-3"
                                        >
                                          <div>
                                            <p className="text-sm font-medium">
                                              已启用供给关系
                                            </p>
                                            <p className="mt-2 text-sm text-muted-foreground">
                                              {supplierCountsQuery.isError
                                                ? getErrorMessage(
                                                    supplierCountsQuery.error,
                                                    "当前无法读取正式供给，请稍后重试。",
                                                  )
                                                : `当前共有 ${supplierCount ?? 0} 家供应商具备已启用且已形成当前修订的供给关系；供应商及有效期明细以供给中心为准。`}
                                            </p>
                                          </div>
                                          <div className="flex flex-wrap items-center gap-2 border-t border-border pt-3">
                                            <Button
                                              type="button"
                                              variant="outline"
                                              size="sm"
                                              disabled={!canRevise}
                                              onClick={() =>
                                                setSupplierDialogSku({
                                                  skuId: sku.skuId!,
                                                  skuCode: sku.skuNo,
                                                  skuName: values.name,
                                                  productKind:
                                                    fields.productKind as ProductKind,
                                                  specification: sku.specLabel,
                                                  baseUnit:
                                                    sku.baseUnit ??
                                                    fields.baseUnit,
                                                  category:
                                                    fields.category || undefined,
                                                  brand:
                                                    fields.brand || undefined,
                                                  barcode: sku.barcode,
                                                  description:
                                                    fields.description ||
                                                    undefined,
                                                  carouselImages:
                                                    fields.carouselImages,
                                                  detailImages:
                                                    fields.detailImages,
                                                  carouselFileAssetIds:
                                                    fields.carouselFileAssetIds,
                                                  detailFileAssetIds:
                                                    fields.detailFileAssetIds,
                                                  carouselPreviewUrls:
                                                    fields.carouselPreviewUrls,
                                                  detailPreviewUrls:
                                                    fields.detailPreviewUrls,
                                                  mainImage:
                                                    sku.mainImage || undefined,
                                                  mainImageAssetId:
                                                    sku.mainImageAssetId,
                                                  mainImagePreviewUrl:
                                                    sku.mainImagePreviewUrl,
                                                })
                                              }
                                            >
                                              添加供给
                                            </Button>
                                            <Link
                                              className="text-xs text-primary hover:underline"
                                              href={`/procurement/supplier-offerings?skuId=${encodeURIComponent(sku.skuId)}&returnTo=${encodeURIComponent(`/master-data/products/${stableId}#product-section-sku`)}`}
                                            >
                                              查看全部供给
                                            </Link>
                                          </div>
                                        </HoverCardContent>
                                      </HoverCard>
                                    ) : (
                                      <Badge variant="outline">
                                        {supplierCountsQuery.isPending
                                          ? "…"
                                          : supplierCountsQuery.isError
                                            ? "供给暂不可查"
                                            : `${supplierCount ?? 0} 家供应商`}
                                      </Badge>
                                    )}
                                    {!sku.skuId || isCreate ? (
                                      <span className="block text-xs text-muted-foreground">
                                        保存商品后可添加多家供应商
                                      </span>
                                    ) : null}
                                  </div>
                                </td>
                                <td className="px-3 py-3">
                                  {fields.productKind &&
                                  fields.productKind !== "PHYSICAL" ? (
                                    <span className="block text-xs text-muted-foreground">
                                      不适用
                                    </span>
                                  ) : sku.skuId ? (
                                    <Button
                                      type="button"
                                      variant="link"
                                      size="xs"
                                      className="h-auto px-0 text-xs"
                                      onClick={(event) =>
                                        openInventoryPreview(
                                          sku.skuId,
                                          event.currentTarget,
                                        )
                                      }
                                    >
                                      查看库存
                                    </Button>
                                  ) : (
                                    <span className="block text-xs text-muted-foreground">
                                      保存后可查看
                                    </span>
                                  )}
                                </td>
                                <td className="px-3 py-3">
                                  <div className="flex items-center gap-2">
                                    <Switch
                                      size="sm"
                                      disabled={!canRevise}
                                      checked={
                                        sku.lifecycleStatus === "ENABLED"
                                      }
                                      onCheckedChange={(checked) => {
                                        if (
                                          !checked &&
                                          !window.confirm(
                                            "停用该 SKU 后，新的业务单据将选不到它；历史单据不受影响。确定停用？"
                                          )
                                        ) {
                                          return
                                        }
                                        updateSku(index, {
                                          lifecycleStatus: checked
                                            ? "ENABLED"
                                            : "DISABLED",
                                        })
                                      }}
                                      aria-label={`${sku.specLabel} SKU 状态`}
                                    />
                                    <span className="text-xs text-muted-foreground">
                                      {sku.lifecycleStatus === "ENABLED"
                                        ? "启用"
                                        : "停用"}
                                    </span>
                                  </div>
                                </td>
                                </tr>
                              )
                            })}
                          </tbody>
                        </table>
                      </div>
                    )}
                  </fieldset>

                  <fieldset
                    id="product-section-effective"
                    className={cn(surfacePanelClassName, "scroll-mt-[var(--product-section-scroll-margin)] space-y-3 p-5")}
                    disabled={!canRevise}
                  >
                    <legend className="px-1 text-base font-semibold">
                      生效与原因
                    </legend>
                    <div className="grid gap-3 sm:grid-cols-2">
                      <div className="space-y-1.5">
                        <Label htmlFor="ef-from">
                          {masterDataCopy.fieldEffectiveFrom}
                        </Label>
                        <DatePicker
                          value={effectiveFrom || undefined}
                          onValueChange={(next) => setEffectiveFrom(next ?? "")}
                          className="w-full"
                        />
                      </div>
                      <div className="space-y-1.5">
                        <Label htmlFor="ef-to">
                          {masterDataCopy.fieldEffectiveTo}
                        </Label>
                        <DatePicker
                          value={effectiveTo || undefined}
                          onValueChange={(next) => setEffectiveTo(next ?? "")}
                          className="w-full"
                        />
                      </div>
                      <div className="space-y-1.5 sm:col-span-2">
                        <Label htmlFor="reason">
                          {masterDataCopy.fieldChangeReason}
                        </Label>
                        <Textarea
                          id="reason"
                          value={changeReason}
                          onChange={(e) => setChangeReason(e.target.value)}
                          rows={2}
                          placeholder={
                            isCreate
                              ? "新建原因"
                              : "说明本次修改内容，保存后形成新版本"
                          }
                        />
                      </div>
                    </div>
                  </fieldset>

                  {!isCreate && data ? (
                    <section
                      id="product-section-history"
                      aria-label="历史与引用"
                      className={cn(surfacePanelClassName, "scroll-mt-[var(--product-section-scroll-margin)] overflow-hidden px-5")}
                    >
                      <DocumentSection
                        title={masterDataCopy.centerVersions}
                        description={masterDataCopy.centerVersionsDesc}
                      >
                        <RevisionTimeline
                          revisions={data.revisionTimeline.map((rev) => ({
                            id: rev.id,
                            version: rev.revisionNo,
                            source: "erp-change" as const,
                            actor: rev.actor,
                            effectiveAt: {
                              dateTime: rev.effectiveFrom,
                              label: formatEffectiveRange(
                                rev.effectiveFrom,
                                rev.effectiveTo,
                              ),
                            },
                            reason: (
                              <div className="space-y-1">
                                <div>
                                  {masterDataCopy.centerHistoryName}：
                                  <strong>{rev.nameSnapshot}</strong>
                                </div>
                                <div>{rev.changeReason}</div>
                                <div className="flex flex-wrap gap-2">
                                  <Badge variant="outline">
                                    {rev.timingLabel}
                                  </Badge>
                                  <Badge variant="secondary">
                                    {rev.lifecycleAtRevision === "ENABLED"
                                      ? "启用"
                                      : "停用"}
                                  </Badge>
                                </div>
                                {rev.productSnapshot ? (
                                  <details className="mt-2 rounded-lg border bg-muted/30 p-2 text-xs">
                                    <summary className="cursor-pointer font-medium">
                                      查看本版本的完整 SKU 与价格明细
                                    </summary>
                                    <div className="mt-2 space-y-2">
                                      <div>
                                        单位 {rev.productSnapshot.baseUnit}（
                                        {rev.productSnapshot.baseUnitCode}） · 分类 {rev.productSnapshot.category} · 品牌 {rev.productSnapshot.brand}
                                      </div>
                                      {rev.productSnapshot.skus.map((sku) => (
                                        <div key={`${rev.id}:${sku.skuNo}`} className="rounded border bg-card p-2">
                                          <div className="font-medium">
                                            {sku.skuNo} · {sku.specLabel}
                                          </div>
                                          <div className="mt-1 text-muted-foreground">
                                            销售可见价 {sku.salePrice ?? "—"} · 市场价 {sku.marketPrice ?? "—"}
                                          </div>
                                        </div>
                                      ))}
                                      <p className="text-muted-foreground">
                                        供应商、订货编码、成本、税费和起订量按供给关系独立维护，不写入商品版本。
                                      </p>
                                    </div>
                                  </details>
                                ) : null}
                              </div>
                            ),
                            isCurrent: rev.isCurrent,
                          }))}
                        />
                      </DocumentSection>

                      <DocumentSection
                        title={masterDataCopy.centerRelations}
                        description={masterDataCopy.centerRelationsDesc}
                      >
                        <p className="text-sm">
                          {masterDataCopy.centerUsageCount(
                            data.usageSummary.historicalReferenceCount,
                          )}
                          {data.usageSummary.note}
                        </p>
                        <ul className="mt-3 space-y-2">
                          {data.selectorEligibility.map((s) => (
                            <li
                              key={s.context}
                              className="flex flex-wrap items-center gap-2 rounded-md bg-muted/40 px-2 py-1.5 text-sm"
                            >
                              <span>{s.contextLabel}</span>
                              <Badge
                                variant={s.eligible ? "success" : "destructive"}
                              >
                                {s.eligible
                                  ? masterDataCopy.eligible
                                  : masterDataCopy.ineligible}
                              </Badge>
                              {s.reason ? (
                                <span className="text-xs text-muted-foreground">
                                  {s.reason}
                                </span>
                              ) : null}
                            </li>
                          ))}
                        </ul>
                      </DocumentSection>

                      <DocumentSection
                        title={masterDataCopy.centerAudit}
                        description={masterDataCopy.centerAuditDesc}
                      >
                        {data.auditEvents.length === 0 ? (
                          <p className="text-sm text-muted-foreground">
                            {masterDataCopy.centerNoAudit}
                          </p>
                        ) : (
                          <ul className="space-y-2 text-sm">
                            {data.auditEvents.map((ev) => (
                              <li
                                key={ev.id}
                                className="rounded-md border border-border px-3 py-2"
                              >
                                <div className="flex flex-wrap gap-2">
                                  <span className="num text-xs text-muted-foreground">
                                    {formatDateTime(ev.at, "full", "passthrough")}
                                  </span>
                                  <span>{ev.actor}</span>
                                  <Badge variant="outline">{ev.action}</Badge>
                                </div>
                                <div className="mt-1 text-muted-foreground">
                                  {ev.detail}
                                </div>
                              </li>
                            ))}
                          </ul>
                        )}
                      </DocumentSection>
                    </section>
                  ) : null}
                </div>
              </div>
            </form>

            {!isCreate && data ? (
              <MasterDataDisableDialog
                open={disableOpen}
                onOpenChange={setDisableOpen}
                resource="products"
                target={data}
              />
            ) : null}
            <RegisterSupplyForSkuDialog
              key={supplierDialogSku?.skuId ?? "register-supply"}
              open={Boolean(supplierDialogSku)}
              onOpenChange={(open) => {
                if (!open) setSupplierDialogSku(undefined)
              }}
              fixedSku={supplierDialogSku}
            />
            <ProductInventoryPreviewSheet
              open={inventoryOpen}
              onOpenChange={handleInventoryOpenChange}
              productName={title}
              productKind={fields.productKind}
              skus={inventoryPreviewSkus}
              initialSkuId={inventoryInitialSkuId}
            />
            <DiscardConfirmDialog
              open={discardOpen}
              onOpenChange={setDiscardOpen}
              title="放弃未保存的更改？"
              description="本次修改尚未保存，离开后将丢失。"
              confirmLabel="放弃更改"
              cancelLabel="继续编辑"
              onConfirm={() => {
                setDiscardOpen(false)
                if (pendingNav) {
                  setPendingNav(null)
                  router.push(pendingNav)
                }
              }}
            />
          </PageScaffold>
        )
      }}
    </form.Subscribe>
  )
}
