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
  PackageOpenIcon,
  PlusIcon,
  SaveIcon,
  XIcon,
} from "lucide-react"

import {
  BrandCombobox,
  BusinessFailureState,
  CategoryCombobox,
  DocumentSection,
  FormalActionResult,
  OptionCombobox,
  PageHeader,
  RevisionTimeline,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { FileUpload } from "@/components/ui/file-upload"
import { HoverCard, HoverCardContent, HoverCardTrigger } from "@/components/ui/hover-card"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Progress, ProgressLabel } from "@/components/ui/progress"
import { StatusBadge } from "@/components/ui/status-badge"
import { Switch } from "@/components/ui/switch"
import { Textarea } from "@/components/ui/textarea"
import { MasterDataDisableDialog } from "@/features/master-data/master-data-action-dialog"
import {
  RegisterSupplyForSkuDialog,
  type FixedSku,
} from "@/features/supplier-catalog/catalog-write-dialogs"
import { useSupplierCatalogQueueQuery } from "@/features/supplier-catalog/queries"
import { masterDataCopy } from "@/features/master-data/copy"
import { formatEffectiveRange } from "@/features/master-data/filter"
import {
  toBrandComboboxItems,
  toCategoryComboboxItems,
} from "@/features/master-data/category-tree-model"
import {
  BASE_UNIT_DICTIONARY,
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
} from "@/features/master-data/queries"
import type {
  MasterDataCenterView,
  MasterDataMutationResult,
  ProductDetailView,
  ProductFields,
  ProductSkuFields,
  ProductSpecDimension,
} from "@/features/master-data/types"
import { cn } from "@/lib/utils"

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
  "aria-label": ariaLabel,
}: {
  value: string
  onChange: (next: string) => void
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
        onChange={(event) => onChange(event.target.value)}
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
    description: detail.description ?? "",
    baseUnitId: detail.baseUnitId,
    baseUnitCode: detail.baseUnitCode,
    baseUnit: detail.baseUnit,
    categoryId: detail.categoryId,
    category: detail.category,
    brandId: detail.brandId,
    brand: detail.brand,
    carouselImages: [...detail.carouselImages],
    detailImages: [...detail.detailImages],
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
  mode = "carousel",
}: {
  label: string
  hint?: string
  value: readonly string[]
  onChange: (next: string[]) => void
  mode?: "carousel" | "detail"
}) {
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
        {value.map((name, index) => (
          <div
            key={`${name}-${index}`}
            className="group relative overflow-hidden rounded-xl border border-border bg-surface-sunken"
          >
            <div
              className={cn(
                "flex flex-col items-center justify-center gap-2 p-3 text-center",
                mode === "carousel" ? "aspect-square" : "aspect-[4/5]",
              )}
            >
              <ImageIcon className="size-7 text-muted-foreground" aria-hidden />
              <span className="line-clamp-2 break-all text-xs text-muted-foreground">
                {name}
              </span>
            </div>
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
        ))}
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
    </div>
  )
}

function SkuMainImageField({
  value,
  onChange,
}: {
  value: string
  onChange: (next: string) => void
}) {
  return (
    <div className="space-y-1.5">
      {value ? (
        <div className="relative size-14 overflow-hidden rounded-md border border-border bg-surface-sunken">
          <div className="flex size-full flex-col items-center justify-center gap-0.5 p-1 text-center">
            <ImageIcon className="size-4 text-muted-foreground" aria-hidden />
            <span className="line-clamp-2 w-full break-all text-[10px] leading-tight text-muted-foreground">
              {value}
            </span>
          </div>
          <Button
            type="button"
            variant="secondary"
            size="icon-xs"
            className="absolute right-0.5 top-0.5 size-5"
            onClick={() => onChange("")}
            aria-label={`移除主图 ${value}`}
          >
            <XIcon className="size-3" />
          </Button>
        </div>
      ) : (
        <FileUpload
          accept="image/jpeg,image/png,image/webp"
          multiple={false}
          label={masterDataCopy.fMainImage}
          description="1:1"
          density="compact"
          className="aspect-square size-14 gap-0.5 p-1 text-[10px] [&_[data-slot=button]]:mt-0"
          onFilesSelected={(files) => {
            if (files[0]) onChange(files[0].name)
          }}
        />
      )}
    </div>
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
    fields,
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
    effectiveFrom: "2026-08-01",
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
  const createMutation = useCreateMasterDataMutation()
  const reviseMutation = useCreateRevisionMutation()
  const supplierCatalogQuery = useSupplierCatalogQueueQuery({
    mode: "list",
    changeType: "all",
  })

  const data = detailQuery.data
  const lockVersion = data?.lockVersion
  const revisionId = data?.currentRevision.revisionId
  const [formError, setFormError] = React.useState<string | null>(null)
  const [checkPassed, setCheckPassed] = React.useState(false)
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null,
  )
  const [simulate, setSimulate] = React.useState<
    "ok" | "overlap" | "base_unit"
  >("ok")
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey(isCreate ? "create-product" : "revise-product"),
  )
  const [disableOpen, setDisableOpen] = React.useState(false)
  const [supplierDialogSku, setSupplierDialogSku] =
    React.useState<FixedSku>()
  const [activeSection, setActiveSection] =
    React.useState<ProductEditorSectionId>("basic")
  const stickyHeaderRef = React.useRef<HTMLElement>(null)
  const [stickyHeaderHeight, setStickyHeaderHeight] = React.useState(64)
  const hydratedKeyRef = React.useRef<string | null>(null)
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
        setFormError(validation)
        return
      }

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
          fields: nextFields,
          idempotencyKey,
          simulate,
        })
        setResult(response)
        if (response.outcome === "succeeded") {
          setIdempotencyKey(newIdempotencyKey("revise-product"))
          hydratedKeyRef.current = null
          await detailQuery.refetch()
        }
        return
      }

      const response = await createMutation.mutateAsync({
        resource: "products",
        name: value.name.trim(),
        effectiveFrom: value.effectiveFrom,
        effectiveTo: value.effectiveTo.trim() || undefined,
        changeReason: value.changeReason.trim(),
        fields: nextFields,
        idempotencyKey,
        simulate: simulate === "base_unit" ? "ok" : simulate,
      })
      setResult(response)
      if (response.outcome === "succeeded") {
        router.replace(`/master-data/products/${response.stableId}`)
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

  const listHref = "/master-data/products"
  const stickyOffsetPx = stickyHeaderHeight
  const sectionScrollMarginPx = stickyHeaderHeight + 56
  const pending = createMutation.isPending || reviseMutation.isPending
  const canRevise =
    isCreate || (data?.allowedActions.includes("CREATE_REVISION") ?? false)
  const canDisable = data?.allowedActions.includes("DISABLE") ?? false
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
      setFormError(validation)
      return
    }
    setCheckPassed(true)
  }

  if (!isCreate && detailQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="商品详情"
          description={masterDataCopy.centerLoading}
        />
        <div className="h-40 animate-pulse rounded-lg bg-muted" aria-busy />
      </div>
    )
  }

  if (!isCreate && (detailQuery.isError || !data)) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="商品详情" />
        <BusinessFailureState
          kind="system"
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
      </div>
    )
  }

  const formId = "product-detail-form"

  return (
    <form.Subscribe selector={(state) => state.values}>
      {(values) => {
        const title = isCreate
          ? masterDataCopy.productCreateTitle
          : values.name || data?.name || "商品详情"
        const requiredChecks = [
          values.name.trim().length >= 2,
          Boolean(values.fields.baseUnit.trim()),
          Boolean(values.fields.category.trim()),
          Boolean(values.fields.brand.trim()),
          values.fields.skus.length > 0,
          values.fields.skus.every(
            (sku) =>
              sku.lifecycleStatus !== "ENABLED" ||
              Boolean(sku.mainImage.trim()),
          ),
          values.changeReason.trim().length >= 2,
        ]
        const completedChecks = requiredChecks.filter(Boolean).length
        const completionPercent = Math.round(
          (completedChecks / requiredChecks.length) * 100,
        )
        const assistantIssues: ReadonlyArray<{
          section: ProductEditorSectionId
          title: string
          description: string
        }> = [
          ...(!requiredChecks[0] ||
          !requiredChecks[1] ||
          !requiredChecks[2] ||
          !requiredChecks[3]
            ? [
                {
                  section: "basic" as const,
                  title: "基础信息待完善",
                  description: "名称、单位、分类和品牌是必填项。",
                },
              ]
            : []),
          ...(!requiredChecks[4] || !requiredChecks[5]
            ? [
                {
                  section: "sku" as const,
                  title: "SKU 主图待完善",
                  description: "至少保留一个 SKU，启用 SKU 必须有主图。",
                },
              ]
            : []),
          ...(!requiredChecks[6]
            ? [
                {
                  section: "effective" as const,
                  title: "变更原因待填写",
                  description: "保存后会形成新版本，需要留下变更依据。",
                },
              ]
            : []),
        ]
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
        const fields = values.fields
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
          <div
            className="mx-auto w-full max-w-shell"
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
                className="sticky top-0 z-30 border-b border-border bg-background/95 px-4 py-3 shadow-sm backdrop-blur md:px-5"
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
                        <span className="num rounded-md bg-muted px-1.5 py-0.5 text-[11px] text-foreground">
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
                        title={disableBlocker?.message}
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
                      render={<Link href={listHref} />}
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

              <div className="flex flex-col gap-4 p-4 md:p-5">
                {!isCreate && !canRevise && reviseBlocker ? (
                  <p className="text-xs text-muted-foreground">
                    {masterDataCopy.centerUpdateBlocked(reviseBlocker.message)}
                  </p>
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

                {result?.outcome === "succeeded" ? (
                  <FormalActionResult
                    status="succeeded"
                    title={
                      isCreate
                        ? masterDataCopy.createSuccessTitle
                        : masterDataCopy.reviseSuccessTitle
                    }
                    description={
                      isCreate
                        ? masterDataCopy.createSuccessDesc
                        : masterDataCopy.reviseSuccessDesc
                    }
                    reference={result.reference}
                    facts={[
                      {
                        label: masterDataCopy.resultNo,
                        value: result.stableNo,
                      },
                      {
                        label: masterDataCopy.resultVersion,
                        value: `v${result.revisionNo}`,
                      },
                    ]}
                  />
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

                <div className="grid gap-4 xl:grid-cols-4">
                  <aside
                    className="space-y-4 xl:sticky xl:self-start xl:max-h-[calc(100dvh-var(--product-sticky-offset)-1rem)] xl:overflow-y-auto"
                    style={{ top: stickyOffsetPx + 16 }}
                  >
                  <Card size="sm">
                    <CardHeader>
                      <div className="flex items-center justify-between gap-2">
                        <CardTitle className="flex items-center gap-2">
                          <ClipboardCheckIcon className="size-4" aria-hidden />
                          填写助手
                        </CardTitle>
                        <Badge
                          variant={
                            assistantIssues.length === 0
                              ? "success"
                              : "secondary"
                          }
                        >
                          {completedChecks}/{requiredChecks.length}
                        </Badge>
                      </div>
                      <CardDescription>
                        保存前核对必填项，点击问题可直接定位。
                      </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-4">
                      <Progress value={completionPercent}>
                        <ProgressLabel>完成度</ProgressLabel>
                        <span className="ml-auto text-sm text-muted-foreground tabular-nums">
                          {completionPercent}%
                        </span>
                      </Progress>
                      {assistantIssues.length > 0 ? (
                        <div className="space-y-2">
                          {assistantIssues.map((issue) => (
                            <button
                              key={issue.section}
                              type="button"
                              className="flex w-full gap-2 rounded-lg border border-border bg-background p-3 text-left transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                              onClick={() => {
                                setActiveSection(issue.section)
                                scrollToProductSection(issue.section)
                              }}
                            >
                              <CircleAlertIcon
                                className="mt-0.5 size-4 shrink-0 text-warning-foreground"
                                aria-hidden
                              />
                              <span className="min-w-0">
                                <span className="block text-sm font-medium">
                                  {issue.title}
                                </span>
                                <span className="mt-1 block text-xs text-muted-foreground">
                                  {issue.description}
                                </span>
                              </span>
                            </button>
                          ))}
                        </div>
                      ) : (
                        <Alert variant="success">
                          <CheckCircle2Icon aria-hidden />
                          <AlertTitle>必填项已完成</AlertTitle>
                          <AlertDescription>
                            可以继续检查并保存当前版本。
                          </AlertDescription>
                        </Alert>
                      )}
                    </CardContent>
                  </Card>

                  <Card size="sm">
                    <CardHeader>
                      <CardTitle className="flex items-center gap-2">
                        <PackageOpenIcon className="size-4" aria-hidden />
                        商品摘要
                      </CardTitle>
                    </CardHeader>
                    <CardContent className="space-y-3 text-sm">
                      <div className="flex items-center justify-between gap-3">
                        <span className="text-muted-foreground">商品层级</span>
                        <span>SPU</span>
                      </div>
                      <div className="flex items-center justify-between gap-3">
                        <span className="text-muted-foreground">SKU 数量</span>
                        <span className="num">{fields.skus.length}</span>
                      </div>
                      <div className="flex items-center justify-between gap-3">
                        <span className="text-muted-foreground">基础单位</span>
                        <span>{fields.baseUnit || "待选择"}</span>
                      </div>
                      <p className="border-t border-border pt-3 text-xs text-muted-foreground">
                        规格身份由系统派生；图片、价格和条码仍沿用当前商品数据结构。
                      </p>
                    </CardContent>
                  </Card>
                </aside>

                <div className="min-w-0 space-y-4 xl:col-span-3">
                  <nav
                    aria-label="商品编辑分区"
                    className={cn(
                      "sticky z-10 grid grid-cols-2 gap-1 rounded-2xl border border-border bg-background/95 p-1 shadow-sm backdrop-blur",
                      isCreate ? "sm:grid-cols-4" : "sm:grid-cols-5",
                    )}
                    style={{ top: stickyOffsetPx }}
                  >
                    {PRODUCT_EDITOR_SECTIONS.filter(
                      (section) => !isCreate || section.id !== "history",
                    ).map((section) => (
                      <Button
                        key={section.id}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={cn(
                          "relative rounded-xl",
                          activeSection === section.id &&
                            "bg-accent text-accent-foreground",
                        )}
                        aria-current={
                          activeSection === section.id ? "location" : undefined
                        }
                        onClick={() => {
                          setActiveSection(section.id)
                          scrollToProductSection(section.id)
                        }}
                      >
                        {section.label}
                      </Button>
                    ))}
                  </nav>

                  {formError ? (
                    <Alert variant="destructive">
                      <CircleAlertIcon aria-hidden />
                      <AlertTitle>填写检查未通过</AlertTitle>
                      <AlertDescription>{formError}</AlertDescription>
                    </Alert>
                  ) : null}

                  {checkPassed ? (
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
                    className="scroll-mt-[var(--product-section-scroll-margin)] space-y-3 rounded-2xl border border-border bg-card p-5 shadow-sm"
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
                      <div className="space-y-1.5">
                        <Label>{masterDataCopy.fBaseUnit}</Label>
                        <OptionCombobox
                          value={fields.baseUnitId || null}
                          onValueChange={(id) => {
                            const unit = BASE_UNIT_DICTIONARY.find((item) => item.id === id)
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
                          options={BASE_UNIT_DICTIONARY.map((unit) => ({
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
                    className="scroll-mt-[var(--product-section-scroll-margin)] space-y-5 rounded-2xl border border-border bg-card p-5 shadow-sm"
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
                        onChange={(next) =>
                          setFields((prev) => ({
                            ...prev,
                            carouselImages: next,
                          }))
                        }
                      />
                    </section>
                    <div className="border-t border-border" />
                    <section className="space-y-3">
                      <MediaListEditor
                        label={masterDataCopy.fDetailImages}
                        hint="支持批量上传与顺序调整，保存时仍写入原详情图数组"
                        value={fields.detailImages}
                        mode="detail"
                        onChange={(next) =>
                          setFields((prev) => ({ ...prev, detailImages: next }))
                        }
                      />
                    </section>
                  </fieldset>

                  <fieldset
                    id="product-section-sku"
                    className="scroll-mt-[var(--product-section-scroll-margin)] space-y-4 rounded-2xl border border-border bg-card p-5 shadow-sm"
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
                                onClick={() =>
                                  syncSpecDrafts(
                                    specDrafts.filter((_, i) => i !== index),
                                  )
                                }
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
                    className="min-w-0 max-w-full space-y-4 overflow-hidden rounded-2xl border border-border bg-card p-5 shadow-sm"
                    disabled={!canRevise}
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
                        {fields.skus.length} / {fields.skus.length} 行已生成
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
                          !values.batchSalePrice.trim() &&
                          !values.batchMarketPrice.trim()
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
                        render={<Link href="/inventory?view=balance" />}
                      >
                        查看库存台账
                      </Button>
                    </div>
                    {fields.skus.length === 0 ? (
                      <p className="text-sm text-muted-foreground">
                        {masterDataCopy.productNoSkus}
                      </p>
                    ) : (
                      <div className="w-full max-w-full overflow-x-auto overscroll-x-contain rounded-xl border border-border">
                        <table className="w-full min-w-[88rem] border-collapse text-sm">
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
                              <th className="min-w-36 border-l border-border px-3 py-2 font-medium">
                                {masterDataCopy.fProductCode}
                              </th>
                              <th className="min-w-32 px-3 py-2 font-medium">
                                {masterDataCopy.fBarcode}
                              </th>
                              <th className="min-w-44 px-3 py-2 font-medium">
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
                              const supplierItems = sku.skuId
                                ? (supplierCatalogQuery.data?.items ?? []).filter(
                                    (item) => item.mapping?.skuId === sku.skuId,
                                  )
                                : []
                              const supplierNames = Array.from(
                                new Set(
                                  supplierItems.map(
                                    (item) => item.supplierProduct.supplier.name,
                                  ),
                                ),
                              )
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
                                    onChange={(mainImage) =>
                                      updateSku(index, { mainImage })
                                    }
                                  />
                                </td>
                                <td className="border-l border-border px-3 py-3">
                                  <MoneyInput
                                    value={sku.salePrice ?? ""}
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
                                          {supplierItems.length} 家供应商
                                        </HoverCardTrigger>
                                        <HoverCardContent
                                          align="start"
                                          className="w-64 space-y-3"
                                        >
                                          <div>
                                            <p className="text-sm font-medium">
                                              供应商列表
                                            </p>
                                            {supplierNames.length > 0 ? (
                                              <ul className="mt-2 space-y-1.5">
                                                {supplierNames.map((supplierName) => (
                                                  <li
                                                    key={supplierName}
                                                    className="text-sm text-muted-foreground"
                                                  >
                                                    {supplierName}
                                                  </li>
                                                ))}
                                              </ul>
                                            ) : (
                                              <p className="mt-2 text-sm text-muted-foreground">
                                                暂无供应商
                                              </p>
                                            )}
                                          </div>
                                          <div className="flex flex-wrap items-center gap-2 border-t border-border pt-3">
                                            <Button
                                              type="button"
                                              variant="outline"
                                              size="sm"
                                              onClick={() =>
                                                setSupplierDialogSku({
                                                  skuId: sku.skuId!,
                                                  skuCode: sku.skuNo,
                                                  skuName: values.name,
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
                                                  mainImage:
                                                    sku.mainImage || undefined,
                                                  salesVisiblePriceGross:
                                                    sku.salePrice,
                                                  hasPoolEntry: Boolean(
                                                    sku.salePrice,
                                                  ),
                                                })
                                              }
                                            >
                                              添加供应商
                                            </Button>
                                            <Link
                                              className="text-xs text-primary hover:underline"
                                              href={`/procurement/supplier-catalog?mode=list&skuId=${encodeURIComponent(sku.skuId)}&from=W14&returnTo=${encodeURIComponent(`/master-data/products/${stableId}#product-section-sku`)}`}
                                            >
                                              查看全部供给
                                            </Link>
                                          </div>
                                        </HoverCardContent>
                                      </HoverCard>
                                    ) : (
                                      <Badge variant="outline">
                                        {supplierItems.length} 家供应商
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
                                  {sku.skuId ? (
                                    <Link
                                      className="block text-xs text-primary hover:underline"
                                      href={`/inventory?view=balance&skuId=${encodeURIComponent(sku.skuId)}`}
                                    >
                                      查看库存
                                    </Link>
                                  ) : (
                                    <span className="block text-xs text-muted-foreground">
                                      保存后维护
                                    </span>
                                  )}
                                </td>
                                <td className="px-3 py-3">
                                  <div className="flex items-center gap-2">
                                    <Switch
                                      size="sm"
                                      checked={
                                        sku.lifecycleStatus === "ENABLED"
                                      }
                                      onCheckedChange={(checked) =>
                                        updateSku(index, {
                                          lifecycleStatus: checked
                                            ? "ENABLED"
                                            : "DISABLED",
                                        })
                                      }
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
                    className="scroll-mt-[var(--product-section-scroll-margin)] space-y-3 rounded-2xl border border-border bg-card p-5 shadow-sm"
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
                        <Input
                          id="ef-from"
                          value={effectiveFrom}
                          onChange={(e) => setEffectiveFrom(e.target.value)}
                        />
                      </div>
                      <div className="space-y-1.5">
                        <Label htmlFor="ef-to">
                          {masterDataCopy.fieldEffectiveTo}
                        </Label>
                        <Input
                          id="ef-to"
                          value={effectiveTo}
                          onChange={(e) => setEffectiveTo(e.target.value)}
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
                      <div className="space-y-1.5 sm:col-span-2">
                        <Label>{masterDataCopy.demoSimulateLabel}</Label>
                        <OptionCombobox
                          value={simulate}
                          onValueChange={(v) =>
                            setSimulate((v ?? "ok") as typeof simulate)
                          }
                          options={[
                            { value: "ok", label: masterDataCopy.demoOk },
                            {
                              value: "overlap",
                              label: masterDataCopy.demoOverlap,
                            },
                            ...(!isCreate
                              ? [
                                  {
                                    value: "base_unit" as const,
                                    label: masterDataCopy.demoBaseUnit,
                                  },
                                ]
                              : []),
                          ]}
                          allowClear={false}
                          className="w-full max-w-md"
                        />
                      </div>
                    </div>
                  </fieldset>

                  {!isCreate && data ? (
                    <section
                      id="product-section-history"
                      aria-label="历史与引用"
                      className="scroll-mt-[var(--product-section-scroll-margin)] overflow-hidden rounded-2xl border border-border bg-card px-5 shadow-sm"
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
                                        供应商、供给方式、成本、税费和起订量由 W21 的供给版本独立留痕，不写入 SKU 版本。
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
                                    {ev.at.slice(0, 19).replace("T", " ")}
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
          </div>
        )
      }}
    </form.Subscribe>
  )
}

/** @deprecated 使用 ProductDetailPage；保留别名以免旧引用断裂 */
export const ProductFormPage = ProductDetailPage
