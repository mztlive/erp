"use client"

/**
 * 供应商商品中心 = 查看 + 编辑（与 W14 公司商品详情同构）。
 * - /procurement/supplier-catalog/new  手工新建
 * - /procurement/supplier-catalog/:id  详情即编辑，保存形成新来源修订
 * 保留供应商独有字段；入池/建公司品时携带同构内容字段。
 */

import * as React from "react"
import Link from "next/link"
import { useRouter, useSearchParams } from "next/navigation"
import {
  ArrowDownIcon,
  ArrowLeftIcon,
  ArrowUpIcon,
  CheckCircle2Icon,
  CircleAlertIcon,
  ClipboardCheckIcon,
  GripVerticalIcon,
  ImageIcon,
  PackageOpenIcon,
  PlusIcon,
  SaveIcon,
  ShoppingBasketIcon,
  TriangleAlertIcon,
  XIcon,
} from "lucide-react"

import {
  BrandCombobox,
  BusinessEmptyState,
  BusinessStatusBadge,
  CategoryCombobox,
  FormalActionResult,
  OptionCombobox,
} from "@/components/business"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { Checkbox } from "@/components/ui/checkbox"
import { FileUpload } from "@/components/ui/file-upload"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Progress, ProgressLabel } from "@/components/ui/progress"
import { StatusBadge } from "@/components/ui/status-badge"
import { Textarea } from "@/components/ui/textarea"
import { PromoteSupplierProductDialog } from "@/features/supplier-catalog/catalog-write-dialogs"
import {
  deriveSpecification,
  emptySupplierProductFormFields,
  formToMediaPayload,
  hydrateSupplierProductForm,
  PRODUCT_CAPABILITY_OPTIONS,
  specDraftsToAttributes,
  splitRegionText,
  supplierProductCompleteness,
  SUPPLIER_PRODUCT_EDITOR_SECTIONS,
  validateSupplierProductForm,
  type SupplierProductEditorSectionId,
  type SupplierProductFormFields,
  type SupplierSpecDraft,
} from "@/features/supplier-catalog/supplier-product-form-model"
import {
  useCreateSupplierCatalogItemMutation,
  useReviseSupplierCatalogProductMutation,
  useSupplierCatalogCenterQuery,
} from "@/features/supplier-catalog/queries"
import type {
  DemoRole,
  SupplierCatalogWriteResult,
} from "@/features/supplier-catalog/types"
import {
  CHANGE_TYPE_LABEL,
  DEMO_ROLE_LABEL,
} from "@/features/supplier-catalog/types"
import {
  toBrandComboboxItems,
  toCategoryComboboxItems,
} from "@/features/master-data/category-tree-model"
import { BASE_UNIT_DICTIONARY } from "@/features/master-data/resource-fields"
import { useMasterDataListQuery } from "@/features/master-data/queries"
import { cn } from "@/lib/utils"

function newIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
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

function scrollToSection(id: SupplierProductEditorSectionId) {
  document.getElementById(`supplier-product-section-${id}`)?.scrollIntoView({
    behavior: "smooth",
    block: "start",
  })
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
            {hint ?? "支持多选与排序；入池时仅已归档媒体会预填到公司商品"}
          </p>
        </div>
        <Badge variant="secondary">{value.length} 张</Badge>
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
                aria-label={`移除 ${name}`}
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
            mode === "carousel" ? "支持多选，首张作为首图" : "支持多选，按顺序展示"
          }
          onFilesSelected={(files) => {
            onChange([...value, ...files.map((file) => file.name)])
          }}
          className={cn(
            "gap-1.5 p-3 [&_[data-slot=button]]:mt-1",
            mode === "carousel" ? "aspect-square" : "aspect-[4/5]",
          )}
        />
      </div>
    </div>
  )
}

export function SupplierProductCenterPage({
  supplierProductId,
}: {
  supplierProductId: string
}) {
  const router = useRouter()
  const searchParams = useSearchParams()
  const isCreate = supplierProductId === "new"
  const demoRoleParam = searchParams.get("demoRole")
  const demoRole: DemoRole =
    demoRoleParam === "operations" ||
    demoRoleParam === "admin" ||
    demoRoleParam === "ops_tech"
      ? demoRoleParam
      : "procurement"
  const maskCost = searchParams.get("maskCost") === "1"
  const returnTo =
    searchParams.get("returnTo") ?? "/procurement/supplier-catalog?mode=list"
  const queueContextId = searchParams.get("queueContextId") ?? undefined

  const centerQuery = useSupplierCatalogCenterQuery({
    supplierProductId: isCreate ? "" : supplierProductId,
    section: "overview",
    demoRole,
    maskCost,
    enabled: !isCreate,
  })
  const supplierQuery = useMasterDataListQuery({
    resource: "suppliers",
    lifecycleStatus: "enabled",
    revisionTiming: "current",
  })
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
  const createMutation = useCreateSupplierCatalogItemMutation()
  const reviseMutation = useReviseSupplierCatalogProductMutation()

  const [fields, setFields] = React.useState<SupplierProductFormFields>(() =>
    emptySupplierProductFormFields({
      changeReason: isCreate ? "手工录入供应商商品" : "",
    }),
  )
  const [formError, setFormError] = React.useState<string | null>(null)
  const [checkPassed, setCheckPassed] = React.useState(false)
  const [result, setResult] = React.useState<SupplierCatalogWriteResult | null>(
    null,
  )
  const [promoteOpen, setPromoteOpen] = React.useState(false)
  const [activeSection, setActiveSection] =
    React.useState<SupplierProductEditorSectionId>("basic")
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey(isCreate ? "create-supplier-product" : "revise-supplier-product"),
  )
  const stickyHeaderRef = React.useRef<HTMLElement>(null)
  const [stickyHeaderHeight, setStickyHeaderHeight] = React.useState(64)
  const hydratedKeyRef = React.useRef<string | null>(null)

  React.useEffect(() => {
    if (isCreate || !centerQuery.data) return
    if (categoryListQuery.isPending || brandListQuery.isPending) return
    const item = centerQuery.data.item
    const revision =
      item.supplierProduct.incomingRevision ??
      item.supplierProduct.currentRevision
    const key = `${item.supplierProduct.id}:${revision.revisionNo}`
    if (hydratedKeyRef.current === key) return
    const next = hydrateSupplierProductForm({
      supplierId: item.supplierProduct.supplier.id,
      supplierName: item.supplierProduct.supplier.name,
      sourceType: item.supplierProduct.source.type,
      sourceReference:
        item.supplierProduct.source.fileName ??
        item.sourceContext.sourceReference,
      supplierSpuCode: item.supplierProduct.supplierSpuCode,
      supplierSkuCode: item.supplierProduct.supplierSkuCode,
      revision,
      categoryOptions,
      brandOptions,
    })
    setFields(next)
    hydratedKeyRef.current = key
  }, [
    brandListQuery.isPending,
    brandOptions,
    categoryListQuery.isPending,
    categoryOptions,
    centerQuery.data,
    isCreate,
  ])

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
  }, [isCreate, centerQuery.data?.item.supplierProduct.id])

  const patchFields = (
    next: React.SetStateAction<SupplierProductFormFields>,
  ) => {
    setFields((previous) =>
      typeof next === "function" ? next(previous) : next,
    )
    setCheckPassed(false)
    setResult(null)
  }

  const pending = createMutation.isPending || reviseMutation.isPending
  const canEdit = demoRole === "procurement"
  const item = centerQuery.data?.item
  const costFieldVisibility =
    centerQuery.data?.costFieldVisibility ?? (maskCost ? "masked" : "visible")

  const contentPayload = (nextFields: SupplierProductFormFields) => {
    const attributes = specDraftsToAttributes(nextFields.specDrafts)
    return {
      name: nextFields.name.trim(),
      description: nextFields.description.trim() || undefined,
      specification: deriveSpecification(nextFields.specDrafts),
      category: nextFields.category.trim(),
      brand: nextFields.brand.trim() || undefined,
      sourceBaseUnit: nextFields.baseUnit.trim() || undefined,
      barcode: nextFields.barcode.trim() || undefined,
      attributes,
      media: formToMediaPayload(nextFields),
      sourceQuotedPriceGross: nextFields.sourceQuotedPriceGross.trim(),
      inputTaxRate: nextFields.inputTaxRate.trim(),
      freightAmount: nextFields.freightAmount.trim() || "0.00",
      otherFeeAmount: nextFields.otherFeeAmount.trim() || "0.00",
      supplyRegion: splitRegionText(nextFields.supplyRegionText),
      availableQuantity: nextFields.availableQuantity.trim() || undefined,
      availabilityStatus: nextFields.availabilityStatus,
      expectedShipTime: nextFields.expectedShipTime.trim() || undefined,
      afterSalesNote: nextFields.afterSalesNote.trim() || undefined,
      capabilitySnapshot: [...nextFields.capabilities],
    }
  }

  const runLocalCheck = () => {
    setFormError(null)
    setCheckPassed(false)
    setResult(null)
    const validation = validateSupplierProductForm(fields, {
      isCreate,
      requireChangeReason: !isCreate,
    })
    if (validation) {
      setFormError(validation)
      return
    }
    setCheckPassed(true)
  }

  const handleSubmit = async (event?: React.FormEvent) => {
    event?.preventDefault()
    setFormError(null)
    setCheckPassed(false)
    setResult(null)
    const validation = validateSupplierProductForm(fields, {
      isCreate,
      requireChangeReason: !isCreate,
    })
    if (validation) {
      setFormError(validation)
      return
    }

    try {
      const payload = contentPayload(fields)
      if (isCreate) {
        const supplier = supplierQuery.data?.rows.find(
          (row) => row.stableId === fields.supplierId,
        )
        if (!supplier) {
          setFormError("请选择已启用供应商")
          return
        }
        const response = await createMutation.mutateAsync({
          sourceType: "MANUAL",
          supplierId: supplier.stableId,
          supplierName: supplier.name,
          supplierSpuCode: fields.supplierSpuCode.trim() || undefined,
          supplierSkuCode: fields.supplierSkuCode.trim(),
          ...payload,
          sourceReference: fields.sourceReference.trim() || undefined,
          minimumOrderQuantity: "1",
          supplyMode: "BULK",
          validFrom: "2026-08-02",
          idempotencyKey,
        })
        setResult(response)
        setIdempotencyKey(newIdempotencyKey("create-supplier-product"))
        router.replace(
          `/procurement/supplier-catalog/${response.supplierProductId}?returnTo=${encodeURIComponent(returnTo)}`,
        )
        return
      }

      if (!item) return
      const expected =
        item.supplierProduct.incomingRevision?.revisionNo ??
        item.supplierProduct.currentRevision.revisionNo
      const response = await reviseMutation.mutateAsync({
        supplierProductId: item.supplierProduct.id,
        expectedSourceRevisionNo: expected,
        supplierSpuCode: fields.supplierSpuCode.trim() || undefined,
        supplierSkuCode: fields.supplierSkuCode.trim(),
        ...payload,
        changeReason: fields.changeReason.trim(),
        idempotencyKey,
      })
      setResult(response)
      setIdempotencyKey(newIdempotencyKey("revise-supplier-product"))
      hydratedKeyRef.current = null
      await centerQuery.refetch()
    } catch (error) {
      setFormError(error instanceof Error ? error.message : "保存失败")
    }
  }

  const syncSpecDrafts = (next: readonly SupplierSpecDraft[]) => {
    patchFields((previous) => ({ ...previous, specDrafts: next }))
  }

  const toggleCapability = (code: string, checked: boolean) => {
    patchFields((previous) => {
      const set = new Set(previous.capabilities)
      if (checked) set.add(code)
      else set.delete(code)
      return { ...previous, capabilities: [...set] }
    })
  }

  if (!isCreate && centerQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
        <div className="h-40 animate-pulse rounded-2xl bg-muted" />
      </div>
    )
  }

  if (!isCreate && (centerQuery.isError || !item)) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <BusinessEmptyState
          kind="no-data"
          title="未找到供应商商品"
          description={`供应商商品 ${supplierProductId} 不在当前目录范围内。`}
          action={
            <Button render={<Link href={returnTo} />}>返回列表</Button>
          }
        />
      </div>
    )
  }

  const completeness = supplierProductCompleteness(fields)
  const stickyOffsetPx = stickyHeaderHeight
  const sectionScrollMarginPx = stickyHeaderHeight + 56
  const ep = item?.supplierProduct
  const rev = ep
    ? (ep.incomingRevision ?? ep.currentRevision)
    : undefined
  const title = isCreate
    ? "手工录入供应商商品"
    : fields.name || rev?.name || "供应商商品"
  const sections = SUPPLIER_PRODUCT_EDITOR_SECTIONS.filter(
    (section) => !(isCreate && section.editOnly),
  )
  const derivedSpec = deriveSpecification(fields.specDrafts)
  const assistantIssues = completeness.checks
    .filter((check) => !check.ok)
    .map((check) => ({
      section: check.section,
      title: check.label,
      description: "补齐后更便于加入公司商品池时核对与匹配。",
    }))

  const supplierOptions = (supplierQuery.data?.rows ?? []).map((supplier) => ({
    value: supplier.stableId,
    label: `${supplier.name} · ${supplier.stableNo}`,
  }))

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
      <form id="supplier-product-detail-form" onSubmit={(e) => void handleSubmit(e)}>
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
                {!isCreate && item ? (
                  <StatusBadge
                    tone={
                      item.changeType === "STOPPED" ||
                      item.changeType === "ERROR"
                        ? "destructive"
                        : item.changeType === "CHANGED"
                          ? "warning"
                          : "info"
                    }
                    label={CHANGE_TYPE_LABEL[item.changeType]}
                  />
                ) : (
                  <Badge variant="outline">供应商商品 · 来源资料</Badge>
                )}
              </div>
              {!isCreate && ep && rev ? (
                <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                  <span>
                    供应商{" "}
                    <span className="font-medium text-foreground">
                      {ep.supplier.name}
                    </span>
                  </span>
                  <span className="num rounded-md bg-muted px-1.5 py-0.5 text-[11px] text-foreground">
                    来源 r{rev.revisionNo}
                  </span>
                  <span>
                    {ep.supplierSpuCode ?? "—"} / {ep.supplierSkuCode}
                  </span>
                  <span>
                    角色 {DEMO_ROLE_LABEL[demoRole]}
                    {queueContextId ? ` · 队列 ${queueContextId}` : ""}
                  </span>
                </div>
              ) : (
                <p className="text-sm text-muted-foreground">
                  布局与公司商品一致，便于入池时预填名称、规格、图文与条码；报价与供给条件仅属供应商侧。
                </p>
              )}
            </div>

            <div className="flex shrink-0 flex-wrap items-center gap-2">
              {!isCreate && item && item.changeType !== "ERROR" ? (
                <Button
                  type="button"
                  size="sm"
                  variant="secondary"
                  disabled={!canEdit}
                  onClick={() => setPromoteOpen(true)}
                >
                  <ShoppingBasketIcon data-icon="inline-start" aria-hidden />
                  加入公司商品池
                </Button>
              ) : null}
              <Button
                type="button"
                size="sm"
                variant="outline"
                render={<Link href={returnTo} />}
              >
                <ArrowLeftIcon data-icon="inline-start" aria-hidden />
                返回
              </Button>
              <Button
                type="button"
                size="sm"
                variant="outline"
                disabled={!canEdit || pending}
                onClick={runLocalCheck}
              >
                <ClipboardCheckIcon data-icon="inline-start" aria-hidden />
                填写检查
              </Button>
              <Button type="submit" size="sm" disabled={!canEdit || pending}>
                <SaveIcon data-icon="inline-start" aria-hidden />
                {isCreate ? "保存到供应商商品库" : "保存来源版本"}
              </Button>
            </div>
          </div>
        </header>

        <div className="flex flex-col gap-4 p-4 md:p-5">
          {!isCreate &&
          item &&
          (item.changeType === "NEW" || item.changeType === "CHANGED") ? (
            <Alert>
              <TriangleAlertIcon aria-hidden />
              <AlertTitle>待采购复核入池</AlertTitle>
              <AlertDescription>
                {item.registrationBlocker?.message}
              </AlertDescription>
            </Alert>
          ) : null}

          {costFieldVisibility === "masked" ? (
            <Badge variant="outline">价格/税率/费用字段已按权限隐藏</Badge>
          ) : null}

          {!canEdit ? (
            <Alert>
              <AlertTitle>当前角色只读</AlertTitle>
              <AlertDescription>
                仅采购可维护供应商商品来源内容；运营可看发布准备，销售不进入本页。
              </AlertDescription>
            </Alert>
          ) : null}

          {result ? (
            <FormalActionResult
              status="succeeded"
              title={isCreate ? "供应商商品已保存" : "来源版本已更新"}
              description="内容保存在供应商商品库；不会自动改写公司商品或商品池价格。"
              reference={result.reference}
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
                      入池完整度
                    </CardTitle>
                    <Badge
                      variant={
                        assistantIssues.length === 0 ? "success" : "secondary"
                      }
                    >
                      {completeness.completed}/{completeness.total}
                    </Badge>
                  </div>
                  <CardDescription>
                    同构字段越完整，入池匹配与人工核对越省事。
                  </CardDescription>
                </CardHeader>
                <CardContent className="space-y-4">
                  <Progress value={completeness.percent}>
                    <ProgressLabel>完成度</ProgressLabel>
                    <span className="ml-auto text-sm text-muted-foreground tabular-nums">
                      {completeness.percent}%
                    </span>
                  </Progress>
                  {assistantIssues.length > 0 ? (
                    <div className="space-y-2">
                      {assistantIssues.map((issue) => (
                        <button
                          key={issue.title}
                          type="button"
                          className="flex w-full gap-2 rounded-lg border border-border bg-background p-3 text-left transition-colors hover:bg-accent focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          onClick={() => {
                            setActiveSection(issue.section)
                            scrollToSection(issue.section)
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
                      <AlertTitle>内容较完整</AlertTitle>
                      <AlertDescription>
                        可继续检查后保存，或加入公司商品池。
                      </AlertDescription>
                    </Alert>
                  )}
                </CardContent>
              </Card>

              <Card size="sm">
                <CardHeader>
                  <CardTitle className="flex items-center gap-2">
                    <PackageOpenIcon className="size-4" aria-hidden />
                    资料摘要
                  </CardTitle>
                </CardHeader>
                <CardContent className="space-y-3 text-sm">
                  <div className="flex items-center justify-between gap-3">
                    <span className="text-muted-foreground">结构</span>
                    <span>供应商 SPU + SKU</span>
                  </div>
                  <div className="flex items-center justify-between gap-3">
                    <span className="text-muted-foreground">规格维度</span>
                    <span className="num">{fields.specDrafts.length}</span>
                  </div>
                  <div className="flex items-center justify-between gap-3">
                    <span className="text-muted-foreground">图文</span>
                    <span className="num">
                      {fields.carouselImages.length +
                        fields.detailImages.length +
                        (fields.skuMainImage ? 1 : 0)}
                    </span>
                  </div>
                  <div className="flex items-center justify-between gap-3">
                    <span className="text-muted-foreground">映射</span>
                    <span>
                      {item?.mapping?.mappingStatus === "ACTIVE"
                        ? item.mapping.skuCode
                        : "未映射"}
                    </span>
                  </div>
                  <p className="border-t border-border pt-3 text-xs text-muted-foreground">
                    保存只更新供应商来源修订；公司商品图文与销售价独立维护。
                  </p>
                </CardContent>
              </Card>
            </aside>

            <div className="min-w-0 space-y-4 xl:col-span-3">
              <nav
                aria-label="供应商商品编辑分区"
                className={cn(
                  "sticky z-10 grid grid-cols-2 gap-1 rounded-2xl border border-border bg-background/95 p-1 shadow-sm backdrop-blur",
                  isCreate ? "sm:grid-cols-4" : "sm:grid-cols-5",
                )}
                style={{ top: stickyOffsetPx }}
              >
                {sections.map((section) => (
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
                      scrollToSection(section.id)
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
                    必填项完整，保存时仍以服务端校验结果为准。
                  </AlertDescription>
                </Alert>
              ) : null}

              <fieldset
                id="supplier-product-section-basic"
                className="scroll-mt-[var(--product-section-scroll-margin)] space-y-3 rounded-2xl border border-border bg-card p-5 shadow-sm"
                disabled={!canEdit}
              >
                <legend className="px-1 text-base font-semibold">基础信息</legend>
                <p className="text-xs text-muted-foreground">
                  分类、品牌、单位与公司商品相同，均从启用字典选择；规格在下方 SKU
                  分区按维度维护。
                </p>
                <div className="grid gap-3 sm:grid-cols-2">
                  {isCreate ? (
                    <div className="space-y-1.5 sm:col-span-2">
                      <Label>供应商 *</Label>
                      <OptionCombobox
                        value={fields.supplierId || null}
                        onValueChange={(value) => {
                          const hit = supplierQuery.data?.rows.find(
                            (row) => row.stableId === value,
                          )
                          patchFields((previous) => ({
                            ...previous,
                            supplierId: value ?? "",
                            supplierName: hit?.name ?? "",
                          }))
                        }}
                        options={supplierOptions}
                        placeholder="选择已启用供应商"
                        className="w-full"
                      />
                    </div>
                  ) : (
                    <div className="space-y-1.5 sm:col-span-2">
                      <Label>供应商</Label>
                      <Input value={fields.supplierName} disabled readOnly />
                    </div>
                  )}
                  <div className="space-y-1.5">
                    <Label htmlFor="spu-code">供应商 SPU 编码</Label>
                    <Input
                      id="spu-code"
                      value={fields.supplierSpuCode}
                      onChange={(event) =>
                        patchFields((previous) => ({
                          ...previous,
                          supplierSpuCode: event.target.value,
                        }))
                      }
                      placeholder="可空；空时由 ERP 生成来源内稳定代码"
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label htmlFor="sku-code">供应商 SKU 编码 *</Label>
                    <Input
                      id="sku-code"
                      value={fields.supplierSkuCode}
                      onChange={(event) =>
                        patchFields((previous) => ({
                          ...previous,
                          supplierSkuCode: event.target.value,
                        }))
                      }
                    />
                  </div>
                  <div className="space-y-1.5 sm:col-span-2">
                    <Label htmlFor="sp-name">名称 *</Label>
                    <Input
                      id="sp-name"
                      value={fields.name}
                      onChange={(event) =>
                        patchFields((previous) => ({
                          ...previous,
                          name: event.target.value,
                        }))
                      }
                      placeholder="供应商商品名称（与公司 SPU 名称对应）"
                    />
                  </div>
                  <div className="space-y-1.5 sm:col-span-2">
                    <Label htmlFor="sp-desc">商品描述</Label>
                    <Textarea
                      id="sp-desc"
                      value={fields.description}
                      onChange={(event) =>
                        patchFields((previous) => ({
                          ...previous,
                          description: event.target.value,
                        }))
                      }
                      placeholder="供应商来源描述"
                      rows={3}
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label>基础单位 *</Label>
                    <OptionCombobox
                      value={fields.baseUnitId || null}
                      onValueChange={(id) => {
                        const unit = BASE_UNIT_DICTIONARY.find(
                          (candidate) => candidate.id === id,
                        )
                        patchFields((previous) => ({
                          ...previous,
                          baseUnitId: unit?.id ?? "",
                          baseUnitCode: unit?.code ?? "",
                          baseUnit: unit?.label ?? "",
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
                    <Label>分类 *</Label>
                    <CategoryCombobox
                      categories={categoryOptions}
                      value={fields.categoryId || undefined}
                      onValueChange={(id) => {
                        const hit = categoryOptions.find(
                          (candidate) => candidate.categoryId === id,
                        )
                        patchFields((previous) => ({
                          ...previous,
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
                    <Label>品牌 *</Label>
                    <BrandCombobox
                      brands={brandOptions}
                      value={fields.brandId || undefined}
                      onValueChange={(id) => {
                        const hit = brandOptions.find(
                          (candidate) => candidate.brandId === id,
                        )
                        patchFields((previous) => ({
                          ...previous,
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
                  <div className="space-y-1.5">
                    <Label htmlFor="sp-barcode">商品条码</Label>
                    <Input
                      id="sp-barcode"
                      value={fields.barcode}
                      onChange={(event) =>
                        patchFields((previous) => ({
                          ...previous,
                          barcode: event.target.value,
                        }))
                      }
                    />
                  </div>
                </div>
              </fieldset>

              <fieldset
                id="supplier-product-section-media"
                className="scroll-mt-[var(--product-section-scroll-margin)] space-y-5 rounded-2xl border border-border bg-card p-5 shadow-sm"
                disabled={!canEdit}
              >
                <legend className="px-1 text-base font-semibold">图文信息</legend>
                <p className="text-xs text-muted-foreground">
                  轮播/详情在 SPU，主图在 SKU——与公司商品一致。入池仅复制已归档媒体。
                </p>
                <section className="space-y-3">
                  <div className="space-y-1.5">
                    <Label>来源 SKU 主图</Label>
                    {fields.skuMainImage ? (
                      <div className="flex items-center gap-2 rounded-md border border-border px-2 py-1.5">
                        <ImageIcon
                          className="size-4 text-muted-foreground"
                          aria-hidden
                        />
                        <span className="min-w-0 flex-1 truncate text-xs">
                          {fields.skuMainImage}
                        </span>
                        <Button
                          type="button"
                          variant="ghost"
                          size="xs"
                          onClick={() =>
                            patchFields((previous) => ({
                              ...previous,
                              skuMainImage: "",
                            }))
                          }
                        >
                          移除
                        </Button>
                      </div>
                    ) : (
                      <FileUpload
                        accept="image/jpeg,image/png,image/webp"
                        multiple={false}
                        label="上传 SKU 主图"
                        description="建公司品并启用 SKU 时必填"
                        density="compact"
                        onFilesSelected={(files) => {
                          if (files[0]) {
                            patchFields((previous) => ({
                              ...previous,
                              skuMainImage: files[0]!.name,
                            }))
                          }
                        }}
                      />
                    )}
                  </div>
                </section>
                <div className="border-t border-border" />
                <MediaListEditor
                  label="轮播图"
                  value={fields.carouselImages}
                  onChange={(next) =>
                    patchFields((previous) => ({
                      ...previous,
                      carouselImages: next,
                    }))
                  }
                />
                <div className="border-t border-border" />
                <MediaListEditor
                  label="详情图"
                  mode="detail"
                  value={fields.detailImages}
                  onChange={(next) =>
                    patchFields((previous) => ({
                      ...previous,
                      detailImages: next,
                    }))
                  }
                />
              </fieldset>

              <fieldset
                id="supplier-product-section-sku"
                className="scroll-mt-[var(--product-section-scroll-margin)] space-y-4 rounded-2xl border border-border bg-card p-5 shadow-sm"
                disabled={!canEdit}
              >
                <legend className="px-1 text-base font-semibold">
                  SKU / 规格
                </legend>
                <div className="flex flex-wrap items-center justify-between gap-3">
                  <p className="text-xs text-muted-foreground">
                    与公司商品相同：添加规格维度与取值。无规格时按默认规格保存；建公司品时按维度预填。
                  </p>
                  <Badge variant="secondary">
                    {fields.specDrafts.length} 个规格项
                  </Badge>
                </div>
                <div className="space-y-3">
                  {fields.specDrafts.map((draft, index) => (
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
                          <Badge variant="outline">规格项 {index + 1}</Badge>
                        </div>
                        <div className="min-w-48 flex-1 space-y-1.5 sm:max-w-sm">
                          <Label
                            htmlFor={`supplier-spec-name-${index}`}
                            className="text-sm font-medium text-foreground"
                          >
                            规格名称
                          </Label>
                          <Input
                            id={`supplier-spec-name-${index}`}
                            className="bg-background font-medium shadow-sm"
                            value={draft.name}
                            onChange={(event) => {
                              const next = [...fields.specDrafts]
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
                                moveListItem(
                                  fields.specDrafts,
                                  index,
                                  index - 1,
                                ),
                              )
                            }
                          >
                            <ArrowUpIcon />
                          </Button>
                          <Button
                            type="button"
                            variant="ghost"
                            size="icon-xs"
                            disabled={index === fields.specDrafts.length - 1}
                            aria-label={`规格项 ${index + 1} 下移`}
                            onClick={() =>
                              syncSpecDrafts(
                                moveListItem(
                                  fields.specDrafts,
                                  index,
                                  index + 1,
                                ),
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
                                fields.specDrafts.filter((_, i) => i !== index),
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
                                  nextValues[valueIndex] = event.target.value
                                  const next = [...fields.specDrafts]
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
                                  const next = [...fields.specDrafts]
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
                            const next = [...fields.specDrafts]
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
                      ...fields.specDrafts,
                      { name: "", values: [""] },
                    ])
                  }
                >
                  <PlusIcon data-icon="inline-start" aria-hidden />
                  添加规格项
                </Button>
                <div className="overflow-x-auto rounded-xl border border-border">
                  <table className="w-full min-w-[40rem] text-sm">
                    <thead className="bg-muted/40 text-left text-xs text-muted-foreground">
                      <tr>
                        <th className="px-3 py-2 font-medium">供应商 SKU</th>
                        <th className="px-3 py-2 font-medium">规格摘要</th>
                        <th className="px-3 py-2 font-medium">条码</th>
                        <th className="px-3 py-2 font-medium">主图</th>
                        <th className="px-3 py-2 font-medium">单位</th>
                      </tr>
                    </thead>
                    <tbody>
                      <tr className="border-t border-border">
                        <td className="px-3 py-3 num">
                          {fields.supplierSkuCode || "—"}
                        </td>
                        <td className="px-3 py-3">{derivedSpec}</td>
                        <td className="px-3 py-3 num">
                          {fields.barcode || "—"}
                        </td>
                        <td className="px-3 py-3">
                          {fields.skuMainImage || (
                            <span className="text-muted-foreground">未上传</span>
                          )}
                        </td>
                        <td className="px-3 py-3">
                          {fields.baseUnit || "—"}
                        </td>
                      </tr>
                    </tbody>
                  </table>
                </div>
              </fieldset>

              <fieldset
                id="supplier-product-section-supply"
                className="scroll-mt-[var(--product-section-scroll-margin)] space-y-3 rounded-2xl border border-border bg-card p-5 shadow-sm"
                disabled={!canEdit}
              >
                <legend className="px-1 text-base font-semibold">
                  来源供给（供应商独有）
                </legend>
                <p className="text-xs text-muted-foreground">
                  报价与可供条件只属于供应商来源；入池时采购另确认成本，销售可见价写商品池。
                </p>
                <div className="grid gap-3 sm:grid-cols-2">
                  <div className="space-y-1.5">
                    <Label htmlFor="sp-quote">供应商含税报价 *</Label>
                    <Input
                      id="sp-quote"
                      value={
                        costFieldVisibility === "masked"
                          ? "***"
                          : fields.sourceQuotedPriceGross
                      }
                      disabled={costFieldVisibility === "masked"}
                      onChange={(event) =>
                        patchFields((previous) => ({
                          ...previous,
                          sourceQuotedPriceGross: event.target.value,
                        }))
                      }
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label htmlFor="sp-tax">进项税率 *</Label>
                    <Input
                      id="sp-tax"
                      value={
                        costFieldVisibility === "masked"
                          ? "***"
                          : fields.inputTaxRate
                      }
                      disabled={costFieldVisibility === "masked"}
                      onChange={(event) =>
                        patchFields((previous) => ({
                          ...previous,
                          inputTaxRate: event.target.value,
                        }))
                      }
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label htmlFor="sp-freight">运费</Label>
                    <Input
                      id="sp-freight"
                      value={
                        costFieldVisibility === "masked"
                          ? "***"
                          : fields.freightAmount
                      }
                      disabled={costFieldVisibility === "masked"}
                      onChange={(event) =>
                        patchFields((previous) => ({
                          ...previous,
                          freightAmount: event.target.value,
                        }))
                      }
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label htmlFor="sp-other-fee">其他费用</Label>
                    <Input
                      id="sp-other-fee"
                      value={
                        costFieldVisibility === "masked"
                          ? "***"
                          : fields.otherFeeAmount
                      }
                      disabled={costFieldVisibility === "masked"}
                      onChange={(event) =>
                        patchFields((previous) => ({
                          ...previous,
                          otherFeeAmount: event.target.value,
                        }))
                      }
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label htmlFor="sp-region">可供区域 *</Label>
                    <Input
                      id="sp-region"
                      value={fields.supplyRegionText}
                      onChange={(event) =>
                        patchFields((previous) => ({
                          ...previous,
                          supplyRegionText: event.target.value,
                        }))
                      }
                      placeholder="逗号或顿号分隔"
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label>可供状态</Label>
                    <OptionCombobox
                      value={fields.availabilityStatus}
                      onValueChange={(value) =>
                        patchFields((previous) => ({
                          ...previous,
                          availabilityStatus: (value ??
                            "AVAILABLE") as SupplierProductFormFields["availabilityStatus"],
                        }))
                      }
                      options={[
                        { value: "AVAILABLE", label: "可供" },
                        { value: "UNAVAILABLE", label: "不可供" },
                        { value: "STOPPED", label: "停供" },
                        { value: "STALE", label: "过期未刷新" },
                      ]}
                      allowClear={false}
                      className="w-full"
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label htmlFor="sp-qty">来源库存/可供数量</Label>
                    <Input
                      id="sp-qty"
                      value={fields.availableQuantity}
                      onChange={(event) =>
                        patchFields((previous) => ({
                          ...previous,
                          availableQuantity: event.target.value,
                        }))
                      }
                    />
                  </div>
                  <div className="space-y-1.5">
                    <Label htmlFor="sp-ship">预计发货</Label>
                    <Input
                      id="sp-ship"
                      value={fields.expectedShipTime}
                      onChange={(event) =>
                        patchFields((previous) => ({
                          ...previous,
                          expectedShipTime: event.target.value,
                        }))
                      }
                    />
                  </div>
                  <div className="space-y-1.5 sm:col-span-2">
                    <Label htmlFor="sp-after-sales">售后说明</Label>
                    <Textarea
                      id="sp-after-sales"
                      value={fields.afterSalesNote}
                      onChange={(event) =>
                        patchFields((previous) => ({
                          ...previous,
                          afterSalesNote: event.target.value,
                        }))
                      }
                      rows={2}
                    />
                  </div>
                  <div className="space-y-2 sm:col-span-2">
                    <Label>商品能力</Label>
                    <p className="text-xs text-muted-foreground">
                      多选；入池时作为来源能力快照，不写入公司商品主档。
                    </p>
                    <div className="grid gap-2 sm:grid-cols-2 lg:grid-cols-3">
                      {PRODUCT_CAPABILITY_OPTIONS.map((option) => {
                        const checked = fields.capabilities.includes(
                          option.value,
                        )
                        return (
                          <label
                            key={option.value}
                            className="flex cursor-pointer items-center gap-2 rounded-lg border border-border bg-background px-3 py-2 text-sm"
                          >
                            <Checkbox
                              checked={checked}
                              onCheckedChange={(value) =>
                                toggleCapability(option.value, value === true)
                              }
                              aria-label={option.label}
                            />
                            <span>{option.label}</span>
                          </label>
                        )
                      })}
                    </div>
                  </div>
                  {!isCreate ? (
                    <div className="space-y-1.5 sm:col-span-2">
                      <Label htmlFor="sp-reason">变更原因 *</Label>
                      <Textarea
                        id="sp-reason"
                        value={fields.changeReason}
                        onChange={(event) =>
                          patchFields((previous) => ({
                            ...previous,
                            changeReason: event.target.value,
                          }))
                        }
                        rows={2}
                        placeholder="说明本次修改内容，保存后形成新来源修订"
                      />
                    </div>
                  ) : null}
                </div>
              </fieldset>

              {!isCreate && item ? (
                <section
                  id="supplier-product-section-mapping"
                  className="scroll-mt-[var(--product-section-scroll-margin)] space-y-4"
                >
                  <Card size="sm">
                    <CardHeader className="border-b py-3">
                      <CardTitle className="text-base">映射与商品池</CardTitle>
                      <CardDescription>
                        映射到公司 SKU 后，销售只看到商品池价；本页不改公司主档内容。
                      </CardDescription>
                    </CardHeader>
                    <CardContent className="space-y-3 pt-4 text-sm">
                      <div className="flex flex-wrap items-center gap-2">
                        <BusinessStatusBadge
                          context="list"
                          label={
                            item.mapping?.mappingStatus === "ACTIVE"
                              ? `已映射 ${item.mapping.skuCode}`
                              : "待映射"
                          }
                          tone={
                            item.mapping?.mappingStatus === "ACTIVE"
                              ? "success"
                              : "warning"
                          }
                        />
                        {item.poolEntry ? (
                          <Badge variant="secondary">
                            商品池价 ¥{item.poolEntry.salesVisiblePrice}
                          </Badge>
                        ) : (
                          <Badge variant="outline">未入池</Badge>
                        )}
                      </div>
                      {item.mapping?.history?.length ? (
                        item.mapping.history.map((history) => (
                          <div
                            key={history.id}
                            className="rounded-lg border px-3 py-2"
                          >
                            {history.at} · {history.skuCode} · {history.status}{" "}
                            · {history.note}
                          </div>
                        ))
                      ) : (
                        <p className="text-muted-foreground">暂无映射历史</p>
                      )}
                      {item.mapping?.skuCode ? (
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          render={
                            <Link
                              href={`/master-data/products?q=${encodeURIComponent(item.mapping.skuCode)}`}
                            />
                          }
                        >
                          在商品与 SKU 中查找 {item.mapping.skuCode}
                        </Button>
                      ) : null}
                      {item.skuCandidates[0]?.productId ? (
                        <Button
                          type="button"
                          size="sm"
                          variant="outline"
                          render={
                            <Link
                              href={`/master-data/products/${item.skuCandidates[0].productId}`}
                            />
                          }
                        >
                          打开公司商品
                        </Button>
                      ) : null}
                    </CardContent>
                  </Card>
                </section>
              ) : null}
            </div>
          </div>
        </div>
      </form>

      {!isCreate && item ? (
        <PromoteSupplierProductDialog
          key={item.supplierProduct.id}
          item={item}
          open={promoteOpen}
          onOpenChange={setPromoteOpen}
        />
      ) : null}
    </div>
  )
}
