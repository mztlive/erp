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
  ArrowLeftIcon,
  BanIcon,
  ImageIcon,
  PlusIcon,
  SaveIcon,
  XIcon,
} from "lucide-react"

import {
  BusinessFailureState,
  DocumentHeader,
  DocumentSection,
  FormalActionResult,
  OptionCombobox,
  PageHeader,
  RevisionTimeline,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { FileUpload } from "@/components/ui/file-upload"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import { MasterDataDisableDialog } from "@/features/master-data/master-data-action-dialog"
import { masterDataCopy } from "@/features/master-data/copy"
import { formatEffectiveRange } from "@/features/master-data/filter"
import {
  BASE_UNIT_OPTIONS,
  BRAND_OPTIONS,
  CATEGORY_OPTIONS,
  SUPPLIER_OPTIONS,
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
} from "@/features/master-data/queries"
import type {
  MasterDataCenterView,
  MasterDataMutationResult,
  ProductDetailView,
  ProductFields,
  ProductSkuFields,
  ProductSpecDimension,
} from "@/features/master-data/types"

function newIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

function parseSpecValues(raw: string): string[] {
  return raw
    .split(/[,，、|/]/)
    .map((s) => s.trim())
    .filter(Boolean)
}

function productDetailToFields(detail: ProductDetailView): ProductFields {
  return {
    baseUnit: detail.baseUnit,
    category: detail.category,
    brand: detail.brand,
    supplier: detail.supplier,
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
}: {
  label: string
  hint?: string
  value: readonly string[]
  onChange: (next: string[]) => void
}) {
  return (
    <div className="space-y-2">
      <div className="flex items-center justify-between gap-2">
        <Label className="text-sm font-medium">{label}</Label>
        <span className="text-xs text-muted-foreground">
          {masterDataCopy.mediaCount(value.length)}
          {hint ? ` · ${hint}` : null}
        </span>
      </div>
      {value.length > 0 ? (
        <ul className="space-y-1.5">
          {value.map((name, index) => (
            <li
              key={`${name}-${index}`}
              className="flex items-center gap-2 rounded-md border border-border px-2.5 py-1.5"
            >
              <ImageIcon
                className="size-4 shrink-0 text-muted-foreground"
                aria-hidden
              />
              <span className="min-w-0 flex-1 truncate text-sm">{name}</span>
              <Button
                type="button"
                variant="ghost"
                size="icon-sm"
                aria-label={`${masterDataCopy.mediaRemove} ${name}`}
                onClick={() => onChange(value.filter((_, i) => i !== index))}
              >
                <XIcon className="size-3.5" />
              </Button>
            </li>
          ))}
        </ul>
      ) : (
        <p className="text-xs text-muted-foreground">
          {masterDataCopy.mediaEmpty}（{masterDataCopy.mediaAllowEmpty}）
        </p>
      )}
      <FileUpload
        accept="image/jpeg,image/png,image/webp"
        multiple
        label={`添加${label}`}
        description={masterDataCopy.mediaUploadHint}
        onFilesSelected={(files) => {
          onChange([...value, ...files.map((f) => f.name)])
        }}
        className="p-3"
      />
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
        <div className="flex items-center gap-2 rounded-md border border-border px-2 py-1.5">
          <ImageIcon className="size-4 text-muted-foreground" aria-hidden />
          <span className="min-w-0 flex-1 truncate text-xs">{value}</span>
          <Button
            type="button"
            variant="ghost"
            size="xs"
            onClick={() => onChange("")}
          >
            {masterDataCopy.mediaRemove}
          </Button>
        </div>
      ) : (
        <FileUpload
          accept="image/jpeg,image/png,image/webp"
          multiple={false}
          label={masterDataCopy.fMainImage}
          description={masterDataCopy.productMainImageHint}
          onFilesSelected={(files) => {
            if (files[0]) onChange(files[0].name)
          }}
          className="p-2"
        />
      )}
    </div>
  )
}

function hydrateFromCenter(data: MasterDataCenterView) {
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
      valuesText: s.values.join("、"),
    })),
  }
}

export function ProductDetailPage({ stableId }: { stableId: string }) {
  const router = useRouter()
  const isCreate = stableId === "new"
  const detailQuery = useMasterDataCenterQuery(
    "products",
    isCreate ? "" : stableId
  )
  const createMutation = useCreateMasterDataMutation()
  const reviseMutation = useCreateRevisionMutation()

  const [name, setName] = React.useState("")
  const [effectiveFrom, setEffectiveFrom] = React.useState("2026-08-01")
  const [effectiveTo, setEffectiveTo] = React.useState("")
  const [changeReason, setChangeReason] = React.useState(
    isCreate ? "新建商品" : ""
  )
  const [fields, setFields] = React.useState<ProductFields>(emptyProductFields)
  const [specDrafts, setSpecDrafts] = React.useState<
    { name: string; valuesText: string }[]
  >([])
  const [formError, setFormError] = React.useState<string | null>(null)
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null
  )
  const [simulate, setSimulate] = React.useState<"ok" | "overlap" | "base_unit">(
    "ok"
  )
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey(isCreate ? "create-product" : "revise-product")
  )
  const [disableOpen, setDisableOpen] = React.useState(false)
  const hydratedKeyRef = React.useRef<string | null>(null)

  const data = detailQuery.data
  const lockVersion = data?.lockVersion
  const revisionId = data?.currentRevision.revisionId

  React.useEffect(() => {
    if (isCreate || !data) return
    const key = `${data.stableId}:${data.lockVersion}:${data.currentRevision.revisionId}`
    if (hydratedKeyRef.current === key) return
    const next = hydrateFromCenter(data)
    setName(next.name)
    setEffectiveFrom(next.effectiveFrom)
    setEffectiveTo(next.effectiveTo)
    setChangeReason(next.changeReason)
    setFields(next.fields)
    setSpecDrafts(next.specDrafts)
    hydratedKeyRef.current = key
  }, [data, isCreate])

  const listHref = "/master-data/products"
  const pending = createMutation.isPending || reviseMutation.isPending
  const canRevise =
    isCreate || (data?.allowedActions.includes("CREATE_REVISION") ?? false)
  const canDisable = data?.allowedActions.includes("DISABLE") ?? false
  const reviseBlocker = data?.actionBlockers.find(
    (b) => b.action === "CREATE_REVISION"
  )
  const disableBlocker = data?.actionBlockers.find((b) => b.action === "DISABLE")

  const applySpecsFromDrafts = React.useCallback(
    (drafts: { name: string; valuesText: string }[], current: ProductFields) => {
      const specs: ProductSpecDimension[] = drafts
        .map((d) => ({
          name: d.name.trim(),
          values: parseSpecValues(d.valuesText),
        }))
        .filter((s) => s.name)
      const skus = rebuildSkusFromSpecs({
        specs,
        existing: current.skus,
        baseUnit: current.baseUnit,
        supplier: current.supplier,
        skuNoPrefix: "SKU",
      })
      return { ...current, specs, skus }
    },
    []
  )

  const updateSku = (index: number, patch: Partial<ProductSkuFields>) => {
    setFields((prev) => ({
      ...prev,
      skus: prev.skus.map((sku, i) =>
        i === index ? { ...sku, ...patch } : sku
      ),
    }))
  }

  const handleSubmit = async (e?: React.FormEvent) => {
    e?.preventDefault()
    setFormError(null)
    setResult(null)

    const nextFields = applySpecsFromDrafts(specDrafts, fields)
    setFields(nextFields)

    if (name.trim().length < 2) {
      setFormError("请填写商品名称")
      return
    }
    if (changeReason.trim().length < 2) {
      setFormError(
        isCreate ? "请填写变更原因" : "请填写本次保存的变更原因"
      )
      return
    }
    const validation = validateProductFields(nextFields)
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
        name: name.trim(),
        effectiveFrom,
        effectiveTo: effectiveTo.trim() || undefined,
        changeReason: changeReason.trim(),
        fields: nextFields,
        idempotencyKey,
        simulate,
      })
      setResult(response)
      if (response.outcome === "succeeded") {
        setIdempotencyKey(newIdempotencyKey("revise-product"))
        setChangeReason("")
        await detailQuery.refetch()
        // 允许按新 lockVersion 再 hydrate
        hydratedKeyRef.current = null
      }
      return
    }

    const response = await createMutation.mutateAsync({
      resource: "products",
      name: name.trim(),
      effectiveFrom,
      effectiveTo: effectiveTo.trim() || undefined,
      changeReason: changeReason.trim(),
      fields: nextFields,
      idempotencyKey,
      simulate: simulate === "base_unit" ? "ok" : simulate,
    })
    setResult(response)
    if (response.outcome === "succeeded") {
      router.replace(`/master-data/products/${response.stableId}`)
    }
  }

  if (!isCreate && detailQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="商品详情" description={masterDataCopy.centerLoading} />
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

  const title = isCreate
    ? masterDataCopy.productCreateTitle
    : (name || data?.name || "商品详情")
  const formId = "product-detail-form"

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
      <PageHeader
        variant="object-chrome"
        breadcrumbs={[
          { id: "md", label: "基础资料", href: "/master-data" },
          { id: "resource", label: "商品与 SKU", href: listHref },
          {
            id: "object",
            label: isCreate ? "新建" : title,
            current: true,
          },
        ]}
        actions={
          <Button
            type="button"
            variant="outline"
            size="sm"
            render={<Link href={listHref} />}
          >
            <ArrowLeftIcon data-icon="inline-start" aria-hidden />
            {masterDataCopy.actionBackList}
          </Button>
        }
      />

      {!isCreate && data ? (
        <DocumentHeader
          density="compact"
          title={title}
          documentNumber={data.stableNo}
          version={data.currentRevision.revisionNo}
          primaryStatus={{
            label: data.lifecycleStatusLabel,
            tone: data.lifecycleTone,
          }}
          meta={
            <span className="num text-muted-foreground">
              {formatEffectiveRange(
                data.currentRevision.effectiveFrom,
                data.currentRevision.effectiveTo
              )}
            </span>
          }
          statuses={[
            {
              id: "timing",
              label: masterDataCopy.centerVersionState,
              status: {
                label: data.revisionTimingLabel,
                tone: data.revisionTiming === "FUTURE" ? "warning" : "info",
              },
            },
          ]}
          primaryAction={
            <Button
              type="submit"
              form={formId}
              size="sm"
              disabled={!canRevise || pending}
              title={reviseBlocker?.message}
            >
              <SaveIcon data-icon="inline-start" aria-hidden />
              {masterDataCopy.reviseSubmit}
            </Button>
          }
          secondaryActions={
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
          }
        />
      ) : (
        <div className="space-y-1">
          <h1 className="text-lg font-semibold tracking-tight">{title}</h1>
          <p className="text-sm text-muted-foreground">
            {masterDataCopy.productCreateDesc}
          </p>
        </div>
      )}

      {!isCreate && !canRevise && reviseBlocker ? (
        <p className="text-xs text-muted-foreground">
          {masterDataCopy.centerUpdateBlocked(reviseBlocker.message)}
        </p>
      ) : null}

      {!isCreate && data?.productConstraints ? (
        <div className="rounded-lg bg-muted/50 p-3 text-xs">
          <p>
            基础单位{" "}
            <span className="num">{data.productConstraints.baseUnit}</span>
            {" · "}
            SKU{" "}
            <span className="num">{data.productConstraints.skuCount}</span> 个
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
            { label: masterDataCopy.resultNo, value: result.stableNo },
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
          description={result.message || masterDataCopy.reviseConflictHint}
        />
      ) : null}

      {formError ? (
        <Alert variant="destructive">
          <AlertTitle>无法保存</AlertTitle>
          <AlertDescription>{formError}</AlertDescription>
        </Alert>
      ) : null}

      <form
        id={formId}
        className="space-y-5"
        onSubmit={(e) => void handleSubmit(e)}
      >
        <fieldset
          className="space-y-3 rounded-lg border border-border p-4"
          disabled={!canRevise}
        >
          <legend className="px-1 text-xs text-muted-foreground">
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
            <div className="space-y-1.5">
              <Label>{masterDataCopy.fBaseUnit}</Label>
              <OptionCombobox
                value={fields.baseUnit || null}
                onValueChange={(v) =>
                  setFields((prev) => ({ ...prev, baseUnit: v ?? "" }))
                }
                options={BASE_UNIT_OPTIONS.map((o) => ({
                  value: o,
                  label: o,
                }))}
                allowClear={false}
                placeholder="请选择基础单位"
                className="w-full"
              />
            </div>
            <div className="space-y-1.5">
              <Label>{masterDataCopy.fSupplier}</Label>
              <OptionCombobox
                value={fields.supplier ?? null}
                onValueChange={(v) =>
                  setFields((prev) => ({
                    ...prev,
                    supplier: v || undefined,
                  }))
                }
                options={SUPPLIER_OPTIONS.map((o) => ({
                  value: o,
                  label: o,
                }))}
                allowClear
                placeholder="可选"
                className="w-full"
              />
            </div>
            <div className="space-y-1.5">
              <Label>{masterDataCopy.fCategory}</Label>
              <OptionCombobox
                value={fields.category || null}
                onValueChange={(v) =>
                  setFields((prev) => ({ ...prev, category: v ?? "" }))
                }
                options={CATEGORY_OPTIONS.map((o) => ({
                  value: o,
                  label: o,
                }))}
                allowClear={false}
                placeholder="请选择分类"
                className="w-full"
              />
            </div>
            <div className="space-y-1.5">
              <Label>{masterDataCopy.fBrand}</Label>
              <OptionCombobox
                value={fields.brand || null}
                onValueChange={(v) =>
                  setFields((prev) => ({ ...prev, brand: v ?? "" }))
                }
                options={BRAND_OPTIONS.map((o) => ({ value: o, label: o }))}
                allowClear={false}
                placeholder="请选择品牌"
                className="w-full"
              />
            </div>
          </div>
        </fieldset>

        <fieldset
          className="space-y-3 rounded-lg border border-border p-4"
          disabled={!canRevise}
        >
          <legend className="px-1 text-xs text-muted-foreground">
            {masterDataCopy.fieldMediaSection}
          </legend>
          <p className="text-xs text-muted-foreground">
            {masterDataCopy.productSpuMediaHint}
          </p>
          <div className="grid gap-4 lg:grid-cols-2">
            <MediaListEditor
              label={masterDataCopy.fCarouselImages}
              hint={masterDataCopy.mediaAllowEmpty}
              value={fields.carouselImages}
              onChange={(next) =>
                setFields((prev) => ({ ...prev, carouselImages: next }))
              }
            />
            <MediaListEditor
              label={masterDataCopy.fDetailImages}
              hint={masterDataCopy.mediaAllowEmpty}
              value={fields.detailImages}
              onChange={(next) =>
                setFields((prev) => ({ ...prev, detailImages: next }))
              }
            />
          </div>
        </fieldset>

        <fieldset
          className="space-y-3 rounded-lg border border-border p-4"
          disabled={!canRevise}
        >
          <legend className="px-1 text-xs text-muted-foreground">
            {masterDataCopy.fieldSpecSection}
          </legend>
          <p className="text-xs text-muted-foreground">
            {masterDataCopy.productSpecsHint}
          </p>
          <div className="space-y-3">
            {specDrafts.map((draft, index) => (
              <div
                key={index}
                className="grid gap-2 rounded-md border border-border p-3 sm:grid-cols-[1fr_2fr_auto]"
              >
                <div className="space-y-1.5">
                  <Label>{masterDataCopy.fSpecName}</Label>
                  <Input
                    value={draft.name}
                    onChange={(e) => {
                      const next = [...specDrafts]
                      next[index] = { ...draft, name: e.target.value }
                      setSpecDrafts(next)
                    }}
                    placeholder="如：颜色"
                  />
                </div>
                <div className="space-y-1.5">
                  <Label>{masterDataCopy.fSpecValues}</Label>
                  <Input
                    value={draft.valuesText}
                    onChange={(e) => {
                      const next = [...specDrafts]
                      next[index] = { ...draft, valuesText: e.target.value }
                      setSpecDrafts(next)
                    }}
                    placeholder={masterDataCopy.productSpecValuesPlaceholder}
                  />
                </div>
                <div className="flex items-end">
                  <Button
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={() =>
                      setSpecDrafts(specDrafts.filter((_, i) => i !== index))
                    }
                  >
                    {masterDataCopy.productRemoveSpec}
                  </Button>
                </div>
              </div>
            ))}
          </div>
          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              variant="outline"
              size="sm"
              onClick={() =>
                setSpecDrafts([...specDrafts, { name: "", valuesText: "" }])
              }
            >
              <PlusIcon data-icon="inline-start" aria-hidden />
              {masterDataCopy.productAddSpec}
            </Button>
            <Button
              type="button"
              variant="secondary"
              size="sm"
              onClick={() => {
                const next = applySpecsFromDrafts(specDrafts, fields)
                setFields(next)
              }}
            >
              {masterDataCopy.productRebuildSkus}
            </Button>
          </div>
        </fieldset>

        <fieldset
          className="space-y-3 rounded-lg border border-border p-4"
          disabled={!canRevise}
        >
          <legend className="px-1 text-xs text-muted-foreground">
            {masterDataCopy.fieldSkuSection}
          </legend>
          <p className="text-xs text-muted-foreground">
            {masterDataCopy.productSkuHint}
          </p>
          {fields.skus.length === 0 ? (
            <p className="text-sm text-muted-foreground">
              {masterDataCopy.productNoSkus}
            </p>
          ) : (
            <div className="overflow-x-auto">
              <table className="w-full min-w-[52rem] border-collapse text-sm">
                <thead>
                  <tr className="border-b border-border text-left text-xs text-muted-foreground">
                    <th className="px-2 py-2 font-medium">
                      {masterDataCopy.fSku}
                    </th>
                    <th className="px-2 py-2 font-medium">
                      {masterDataCopy.fSpecLabel}
                    </th>
                    <th className="px-2 py-2 font-medium">
                      {masterDataCopy.fMainImage}
                    </th>
                    <th className="px-2 py-2 font-medium">
                      {masterDataCopy.fBarcode}
                    </th>
                    <th className="px-2 py-2 font-medium">
                      {masterDataCopy.fCostPrice}
                    </th>
                    <th className="px-2 py-2 font-medium">
                      {masterDataCopy.fSalePrice}
                    </th>
                  </tr>
                </thead>
                <tbody>
                  {fields.skus.map((sku, index) => (
                    <tr
                      key={`${sku.skuNo}-${index}`}
                      className="border-b border-border/70 align-top"
                    >
                      <td className="px-2 py-2">
                        <Input
                          className="h-8"
                          value={sku.skuNo}
                          onChange={(e) =>
                            updateSku(index, { skuNo: e.target.value })
                          }
                        />
                      </td>
                      <td className="px-2 py-2">
                        <Badge
                          variant="secondary"
                          className="whitespace-normal"
                        >
                          {sku.specLabel || masterDataCopy.productDefaultSpec}
                        </Badge>
                      </td>
                      <td className="px-2 py-2 min-w-[12rem]">
                        <SkuMainImageField
                          value={sku.mainImage}
                          onChange={(mainImage) =>
                            updateSku(index, { mainImage })
                          }
                        />
                      </td>
                      <td className="px-2 py-2">
                        <Input
                          className="h-8"
                          value={sku.barcode ?? ""}
                          onChange={(e) =>
                            updateSku(index, {
                              barcode: e.target.value || undefined,
                            })
                          }
                        />
                      </td>
                      <td className="px-2 py-2">
                        <Input
                          className="h-8"
                          value={sku.costPrice ?? ""}
                          onChange={(e) =>
                            updateSku(index, {
                              costPrice: e.target.value || undefined,
                            })
                          }
                        />
                      </td>
                      <td className="px-2 py-2">
                        <Input
                          className="h-8"
                          value={sku.salePrice ?? ""}
                          onChange={(e) =>
                            updateSku(index, {
                              salePrice: e.target.value || undefined,
                            })
                          }
                        />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          )}
        </fieldset>

        <fieldset
          className="space-y-3 rounded-lg border border-border p-4"
          disabled={!canRevise}
        >
          <legend className="px-1 text-xs text-muted-foreground">
            生效与原因
          </legend>
          <div className="grid gap-3 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label htmlFor="ef-from">{masterDataCopy.fieldEffectiveFrom}</Label>
              <Input
                id="ef-from"
                value={effectiveFrom}
                onChange={(e) => setEffectiveFrom(e.target.value)}
              />
            </div>
            <div className="space-y-1.5">
              <Label htmlFor="ef-to">{masterDataCopy.fieldEffectiveTo}</Label>
              <Input
                id="ef-to"
                value={effectiveTo}
                onChange={(e) => setEffectiveTo(e.target.value)}
              />
            </div>
            <div className="space-y-1.5 sm:col-span-2">
              <Label htmlFor="reason">{masterDataCopy.fieldChangeReason}</Label>
              <Textarea
                id="reason"
                value={changeReason}
                onChange={(e) => setChangeReason(e.target.value)}
                rows={2}
                placeholder={
                  isCreate ? "新建原因" : "说明本次修改内容，保存后形成新版本"
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
                  { value: "overlap", label: masterDataCopy.demoOverlap },
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

        <div className="flex flex-wrap gap-2">
          <Button type="submit" disabled={!canRevise || pending}>
            <SaveIcon data-icon="inline-start" aria-hidden />
            {isCreate
              ? masterDataCopy.createSubmit
              : masterDataCopy.reviseSubmit}
          </Button>
          <Button
            type="button"
            variant="outline"
            render={<Link href={listHref} />}
          >
            返回列表
          </Button>
        </div>
      </form>

      {!isCreate && data ? (
        <div className="space-y-6 border-t border-border pt-6">
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
                    rev.effectiveTo
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
                      <Badge variant="outline">{rev.timingLabel}</Badge>
                      <Badge variant="secondary">
                        {rev.lifecycleAtRevision === "ENABLED"
                          ? "启用"
                          : "停用"}
                      </Badge>
                    </div>
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
                data.usageSummary.historicalReferenceCount
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
                  <Badge variant={s.eligible ? "success" : "destructive"}>
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
                    <div className="mt-1 text-muted-foreground">{ev.detail}</div>
                  </li>
                ))}
              </ul>
            )}
          </DocumentSection>
        </div>
      ) : null}

      {!isCreate && data ? (
        <MasterDataDisableDialog
          open={disableOpen}
          onOpenChange={setDisableOpen}
          resource="products"
          target={data}
        />
      ) : null}
    </div>
  )
}

/** @deprecated 使用 ProductDetailPage；保留别名以免旧引用断裂 */
export const ProductFormPage = ProductDetailPage
