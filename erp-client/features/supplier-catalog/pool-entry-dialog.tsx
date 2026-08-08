"use client"

import * as React from "react"
import Link from "next/link"

import { OptionCombobox } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
  InputGroupText,
} from "@/components/ui/input-group"
import { Label } from "@/components/ui/label"
import {
  useLinkPromoteToCompanyPoolMutation,
  useReversePromoteToCompanyPoolMutation,
  useSupplierProductPoolMatchQuery,
} from "@/features/supplier-catalog/queries"
import type {
  PoolMatchStatus,
  SupplierCatalogItemView,
  SupplierCatalogSkuView,
  SupplierCatalogWriteResult,
  SupplierSkuPoolMatch,
} from "@/features/supplier-catalog/types"
import { useMasterDataListQuery } from "@/features/master-data/queries"
import {
  PRODUCT_KIND_LABELS,
  type ProductKind,
} from "@/features/master-data/types"
import { cn } from "@/lib/utils"

const PRODUCT_KIND_BY_LABEL: Record<string, string> = Object.fromEntries(
  Object.entries(PRODUCT_KIND_LABELS).map(([code, label]) => [label, code])
)

const money = zMoney()

function zMoney() {
  return {
    safeParse: (value: string) =>
      /^\d+(?:\.\d{1,4})?$/.test(value.trim())
        ? { success: true as const }
        : { success: false as const },
  }
}

function idempotencyKey(prefix: string) {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

function splitValues(value: string) {
  return value
    .split(/[，,、]/)
    .map((item) => item.trim())
    .filter(Boolean)
}

function percentIntegerToRate(percent: string): string | null {
  const trimmed = percent.trim()
  if (!/^\d{1,3}$/.test(trimmed)) return null
  const value = Number.parseInt(trimmed, 10)
  if (value < 0 || value > 100) return null
  return (value / 100).toFixed(2)
}

function catalogSkuRowsFromItem(
  item: SupplierCatalogItemView
): SupplierCatalogSkuView[] {
  const listed = item.supplierProduct.catalogSkus
  if (listed && listed.length > 0) return [...listed]
  return [
    {
      id: `${item.supplierProduct.id}_sku`,
      supplierSkuCode: item.supplierProduct.supplierSkuCode,
      currentRevision: item.supplierProduct.currentRevision,
    },
  ]
}

function poolStatusLabel(status: PoolMatchStatus): string {
  switch (status) {
    case "MAPPED":
      return "已映射"
    case "HAS_CANDIDATES":
      return "有候选"
    case "UNMATCHED":
      return "未入池"
  }
}

/** 该行选定的入池方式：关联已有公司 SKU，还是反向新建。 */
type PoolActionMode = "link" | "create"

type PoolEntryRowState = {
  supplierCatalogSkuId: string
  supplierSkuCode: string
  specification: string
  poolStatus: PoolMatchStatus
  mappedCompanySkuNo?: string
  moq: string
  dropshipSupply: string
  bulkSupply: string
  candidateOptions: { value: string; label: string }[]
  companySkuId: string
  salesVisiblePriceGross: string
  marketPrice: string
  selected: boolean
  mode: PoolActionMode
  blockedReason?: string
}

/**
 * 入池弹窗：按每个供应商 SKU 各自的池内状态给出默认动作——
 * 有候选默认关联、可改为新建；未匹配默认新建；已映射的直接标注去处，不参与本次提交。
 * 打开时拉取池内匹配状态做逐行预填。
 */
export function PromoteSupplierProductDialog({
  item,
  open,
  onOpenChange,
}: {
  item?: SupplierCatalogItemView
  open: boolean
  onOpenChange: (open: boolean) => void
  preferredProductId?: string
}) {
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
  const unitListQuery = useMasterDataListQuery({
    resource: "unit-of-measures",
    lifecycleStatus: "enabled",
    revisionTiming: "current",
  })
  const poolMatchQuery = useSupplierProductPoolMatchQuery(
    item?.supplierProduct.id,
    open && Boolean(item)
  )
  const reverseMutation = useReversePromoteToCompanyPoolMutation()
  const linkMutation = useLinkPromoteToCompanyPoolMutation()

  const [results, setResults] = React.useState<SupplierCatalogWriteResult[]>([])
  const [submitError, setSubmitError] = React.useState<string | null>(null)
  const [productKind, setProductKind] = React.useState("")
  const [categoryId, setCategoryId] = React.useState("")
  const [brandId, setBrandId] = React.useState("")
  const [baseUnitId, setBaseUnitId] = React.useState("")
  const [inputTaxRate, setInputTaxRate] = React.useState("")
  const [supplyRegionText, setSupplyRegionText] = React.useState("")
  const [rows, setRows] = React.useState<PoolEntryRowState[]>([])
  const seededMatchKeyRef = React.useRef<string>("")

  const sourceRevision =
    item?.supplierProduct.incomingRevision ??
    item?.supplierProduct.currentRevision

  const matchBySku = React.useMemo(() => {
    const map = new Map<string, SupplierSkuPoolMatch>()
    for (const row of poolMatchQuery.data?.items ?? []) {
      map.set(row.supplierCatalogSkuId, row)
    }
    return map
  }, [poolMatchQuery.data])

  React.useEffect(() => {
    if (!open || !item || !sourceRevision) {
      if (!open) {
        setResults([])
        setSubmitError(null)
        seededMatchKeyRef.current = ""
      }
      return
    }
    const categories = categoryListQuery.data?.rows ?? []
    const brands = brandListQuery.data?.rows ?? []
    const units = unitListQuery.data?.rows ?? []
    const matchedCategory = categories.find(
      (row) => row.name === sourceRevision.category
    )
    const matchedBrand = brands.find((row) => row.name === sourceRevision.brand)
    const matchedUnit = units.find(
      (row) =>
        row.name === sourceRevision.baseUnit ||
        row.dictionaryCode === sourceRevision.baseUnit
    )
    const sourceKind = sourceRevision.sourceProductKind
    const kindFromSource =
      sourceKind && sourceKind in PRODUCT_KIND_LABELS
        ? sourceKind
        : sourceKind
          ? PRODUCT_KIND_BY_LABEL[sourceKind]
          : undefined
    const kindFromCategory = matchedCategory?.productKind
      ? (PRODUCT_KIND_BY_LABEL[matchedCategory.productKind] ??
        (matchedCategory.productKind in PRODUCT_KIND_LABELS
          ? matchedCategory.productKind
          : undefined))
      : undefined

    setProductKind(kindFromSource ?? kindFromCategory ?? "")
    setCategoryId(matchedCategory?.stableId ?? "")
    setBrandId(matchedBrand?.stableId ?? "")
    setBaseUnitId(matchedUnit?.stableId ?? "")
    setInputTaxRate("")
    setSupplyRegionText("")
    setResults([])
    setSubmitError(null)
  }, [
    open,
    item,
    sourceRevision,
    categoryListQuery.data,
    brandListQuery.data,
    unitListQuery.data,
  ])

  React.useEffect(() => {
    if (!open || !item || !poolMatchQuery.data) return
    const key = `${item.supplierProduct.id}:${poolMatchQuery.dataUpdatedAt}`
    if (seededMatchKeyRef.current === key) return
    seededMatchKeyRef.current = key

    const nextRows: PoolEntryRowState[] = catalogSkuRowsFromItem(item).map(
      (sku) => {
        const match = matchBySku.get(sku.id)
        const rev = sku.currentRevision
        const moq = rev.bulkMinimumOrderQuantity?.trim() ?? ""
        const candidates = match?.candidates ?? []
        const top = candidates[0]
        const poolStatus = match?.poolStatus ?? "UNMATCHED"
        const mapped = poolStatus === "MAPPED"
        const hasCandidates = poolStatus === "HAS_CANDIDATES"

        let blockedReason: string | undefined
        if (mapped) {
          blockedReason = "已映射；如需调整供给价，请在列表或详情页使用「改供给价」"
        } else if (!moq) {
          blockedReason = "缺少集采起订量，请先在商品中心补齐"
        }

        return {
          supplierCatalogSkuId: sku.id,
          supplierSkuCode: sku.supplierSkuCode,
          specification: rev.specification,
          poolStatus,
          mappedCompanySkuNo: match?.mappedCompanySkuNo,
          moq: moq || "—",
          dropshipSupply: rev.dropshipFloorPriceGross ?? "",
          bulkSupply: rev.bulkFloorPriceGross ?? "",
          candidateOptions: candidates.map((c) => ({
            value: c.skuId,
            label: `${c.skuNo} · ${c.name}${c.specification ? ` · ${c.specification}` : ""}${c.matchSignals.length ? ` · ${c.matchSignals.join("/")}` : ""}`,
          })),
          companySkuId: mapped ? "" : (top?.skuId ?? ""),
          salesVisiblePriceGross: "",
          marketPrice: "",
          selected: !mapped && Boolean(moq),
          mode: hasCandidates ? "link" : "create",
          blockedReason,
        }
      }
    )
    setRows(nextRows)
  }, [open, item, poolMatchQuery.data, poolMatchQuery.dataUpdatedAt, matchBySku])

  const updateRow = (id: string, patch: Partial<PoolEntryRowState>) => {
    setRows((current) =>
      current.map((row) =>
        row.supplierCatalogSkuId === id ? { ...row, ...patch } : row
      )
    )
  }

  const productKindOptions = React.useMemo(
    () =>
      Object.entries(PRODUCT_KIND_LABELS).map(([value, label]) => ({
        value,
        label,
      })),
    []
  )
  const categoryOptions = (categoryListQuery.data?.rows ?? []).map((row) => ({
    value: row.stableId,
    label: row.productKind ? `${row.name} · ${row.productKind}` : row.name,
  }))
  const brandOptions = (brandListQuery.data?.rows ?? []).map((row) => ({
    value: row.stableId,
    label: row.name,
  }))
  const unitOptions = (unitListQuery.data?.rows ?? []).map((row) => ({
    value: row.stableId,
    label: row.dictionaryCode
      ? `${row.name} (${row.dictionaryCode})`
      : row.name,
  }))

  const pending = reverseMutation.isPending || linkMutation.isPending
  const expectedRevisionNo =
    poolMatchQuery.data?.sourceRevisionNo ?? sourceRevision?.revisionNo ?? 0

  const selectedRows = rows.filter((row) => row.selected && !row.blockedReason)
  const linkRows = selectedRows.filter((row) => row.mode === "link")
  const createRows = selectedRows.filter((row) => row.mode === "create")
  const allMapped = rows.length > 0 && rows.every((row) => row.poolStatus === "MAPPED")

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault()
    if (!item || !sourceRevision) return
    setSubmitError(null)

    const inputTaxRateDecimal = percentIntegerToRate(inputTaxRate)
    if (inputTaxRateDecimal === null) {
      setSubmitError("请填写 0–100 的整数进项税率，例如 13")
      return
    }
    const regions = splitValues(supplyRegionText)
    if (regions.length === 0) {
      setSubmitError("请填写可供区域")
      return
    }
    if (selectedRows.length === 0) {
      setSubmitError("请至少勾选一个供应商 SKU")
      return
    }

    for (const row of linkRows) {
      if (!row.companySkuId) {
        setSubmitError(`SKU ${row.supplierSkuCode}：请选择公司 SKU`)
        return
      }
      if (!row.dropshipSupply.trim() || !row.bulkSupply.trim()) {
        setSubmitError(`SKU ${row.supplierSkuCode}：请填写代发/集采供给价`)
        return
      }
    }

    if (createRows.length > 0) {
      if (!productKind.trim()) {
        setSubmitError("请选择商品类型")
        return
      }
      if (!categoryId || !brandId || !baseUnitId) {
        setSubmitError("请选择公司分类、品牌与基础单位")
        return
      }
      for (const row of createRows) {
        if (!money.safeParse(row.salesVisiblePriceGross).success) {
          setSubmitError(`SKU ${row.supplierSkuCode}：请填写合法销售可见价`)
          return
        }
        if (!money.safeParse(row.marketPrice).success) {
          setSubmitError(`SKU ${row.supplierSkuCode}：请填写合法市场价`)
          return
        }
      }
      const withCandidates = createRows.filter(
        (row) => row.poolStatus === "HAS_CANDIDATES"
      )
      if (withCandidates.length > 0) {
        const ok = window.confirm(
          `有 ${withCandidates.length} 个 SKU 存在公司商品候选，仍要新建吗？建议改为「关联」。`
        )
        if (!ok) return
      }
    }

    try {
      const nextResults: SupplierCatalogWriteResult[] = []
      if (linkRows.length > 0) {
        const response = await linkMutation.mutateAsync({
          supplierProductId: item.supplierProduct.id,
          expectedSourceRevisionNo: expectedRevisionNo,
          inputTaxRate: inputTaxRateDecimal,
          supplyRegion: regions,
          items: linkRows.map((row) => ({
            supplierCatalogSkuId: row.supplierCatalogSkuId,
            companySkuId: row.companySkuId,
            dropshipSupplyPriceGross: row.dropshipSupply.trim() || undefined,
            bulkSupplyPriceGross: row.bulkSupply.trim() || undefined,
          })),
          idempotencyKey: idempotencyKey("link-promote"),
        })
        nextResults.push(response)
      }
      if (createRows.length > 0) {
        const response = await reverseMutation.mutateAsync({
          supplierProductId: item.supplierProduct.id,
          expectedSourceRevisionNo: expectedRevisionNo,
          productKind: productKind.trim() as ProductKind,
          categoryId,
          brandId,
          baseUnitId,
          inputTaxRate: inputTaxRateDecimal,
          supplyRegion: regions,
          items: createRows.map((row) => ({
            supplierCatalogSkuId: row.supplierCatalogSkuId,
            dropshipSupplyPriceGross: row.dropshipSupply.trim() || undefined,
            bulkSupplyPriceGross: row.bulkSupply.trim() || undefined,
            salesVisiblePriceGross: row.salesVisiblePriceGross.trim(),
            marketPrice: row.marketPrice.trim(),
          })),
          idempotencyKey: idempotencyKey("reverse-promote"),
        })
        nextResults.push(response)
      }
      setResults(nextResults)
    } catch (error) {
      const message =
        error &&
        typeof error === "object" &&
        "message" in error &&
        typeof (error as { message: unknown }).message === "string"
          ? (error as { message: string }).message
          : "入池失败，请重试"
      setSubmitError(message)
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[90vh] flex-col sm:max-w-4xl">
        <DialogHeader>
          <DialogTitle>入池</DialogTitle>
          <DialogDescription>
            系统按每个供应商 SKU 给出建议动作：有候选默认关联、可改为新建；无候选默认新建；已入池的会标注去处。确认即生效，不填生效日期。
          </DialogDescription>
        </DialogHeader>

        {results.length > 0 ? (
          <div className="space-y-2">
            {results.map((result, index) => (
              <Alert key={`${result.reference}-${index}`}>
                <AlertTitle>
                  {result.companySkuChange === "UNCHANGED"
                    ? "关联入池成功"
                    : "反向入池成功"}
                </AlertTitle>
                <AlertDescription>
                  业务记录 {result.reference}
                  {result.companyProductId
                    ? ` · 公司商品 ${result.companyProductId}`
                    : ""}
                </AlertDescription>
                <div className="mt-2 flex flex-wrap gap-2">
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    render={
                      <Link
                        href={`/procurement/supplier-catalog/${result.supplierProductId}`}
                      />
                    }
                  >
                    查看供应商商品
                  </Button>
                  {result.companyProductId ? (
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      render={
                        <Link
                          href={`/master-data/products/${result.companyProductId}`}
                        />
                      }
                    >
                      打开公司商品
                    </Button>
                  ) : null}
                </div>
              </Alert>
            ))}
          </div>
        ) : null}

        <form
          className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1"
          onSubmit={(event) => {
            void handleSubmit(event)
          }}
        >
          {results.length === 0 && allMapped ? (
            <Alert>
              <AlertTitle>已全部入池</AlertTitle>
              <AlertDescription>
                当前供应商商品的所有 SKU 均已关联公司 SKU；如需调整供给价，请关闭本弹窗，在列表或详情页使用「改供给价」。
              </AlertDescription>
            </Alert>
          ) : null}

          <div className="grid gap-4 sm:grid-cols-2">
            <div className="space-y-1.5">
              <Label>进项税率 *</Label>
              <InputGroup>
                <InputGroupInput
                  inputMode="numeric"
                  pattern="[0-9]*"
                  value={inputTaxRate}
                  onChange={(event) => {
                    const digits = event.target.value
                      .replace(/\D/g, "")
                      .slice(0, 3)
                    setInputTaxRate(digits)
                  }}
                  placeholder="例如 13"
                />
                <InputGroupAddon align="inline-end">
                  <InputGroupText>%</InputGroupText>
                </InputGroupAddon>
              </InputGroup>
            </div>
            <div className="space-y-1.5">
              <Label>可供区域 *</Label>
              <Input
                value={supplyRegionText}
                onChange={(event) => setSupplyRegionText(event.target.value)}
                placeholder="例如 华东、华南"
              />
            </div>
          </div>

          {createRows.length > 0 || rows.some((row) => row.mode === "create") ? (
            <div className="grid gap-3 rounded-lg border border-dashed p-3 sm:grid-cols-2">
              <div className="space-y-1.5">
                <Label>商品类型 *</Label>
                <OptionCombobox
                  value={productKind || null}
                  onValueChange={(value) => setProductKind(value ?? "")}
                  options={productKindOptions}
                  placeholder="选择商品类型"
                  className="w-full"
                />
              </div>
              <div className="space-y-1.5">
                <Label>公司分类 *</Label>
                <OptionCombobox
                  value={categoryId || null}
                  onValueChange={(value) => setCategoryId(value ?? "")}
                  options={categoryOptions}
                  className="w-full"
                />
              </div>
              <div className="space-y-1.5">
                <Label>公司品牌 *</Label>
                <OptionCombobox
                  value={brandId || null}
                  onValueChange={(value) => setBrandId(value ?? "")}
                  options={brandOptions}
                  className="w-full"
                />
              </div>
              <div className="space-y-1.5">
                <Label>基础单位 *</Label>
                <OptionCombobox
                  value={baseUnitId || null}
                  onValueChange={(value) => setBaseUnitId(value ?? "")}
                  options={unitOptions}
                  className="w-full"
                />
              </div>
              <p className="text-xs text-muted-foreground sm:col-span-2">
                以上四项仅用于下方选择「新建」的 SKU；选择「关联」的 SKU 直接复用公司 SKU 的既有资料。
              </p>
            </div>
          ) : null}

          <div className="space-y-3">
            {rows.map((row) => (
              <div
                key={row.supplierCatalogSkuId}
                className={cn(
                  "rounded-lg border p-3",
                  row.blockedReason && "bg-muted/30"
                )}
              >
                <div className="flex flex-wrap items-start justify-between gap-2">
                  <div className="flex min-w-0 items-start gap-2">
                    <Checkbox
                      className="mt-0.5"
                      checked={row.selected && !row.blockedReason}
                      disabled={Boolean(row.blockedReason)}
                      onCheckedChange={(checked) =>
                        updateRow(row.supplierCatalogSkuId, {
                          selected: checked === true,
                        })
                      }
                    />
                    <div className="min-w-0">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="font-medium">
                          {row.supplierSkuCode}
                        </span>
                        <Badge
                          variant={
                            row.poolStatus === "MAPPED"
                              ? "secondary"
                              : row.poolStatus === "HAS_CANDIDATES"
                                ? "default"
                                : "outline"
                          }
                        >
                          {poolStatusLabel(row.poolStatus)}
                        </Badge>
                      </div>
                      <p className="mt-0.5 text-xs text-muted-foreground">
                        {row.specification || "—"} · 起订量 {row.moq}
                      </p>
                    </div>
                  </div>

                  {!row.blockedReason && row.poolStatus === "HAS_CANDIDATES" ? (
                    <div
                      role="group"
                      aria-label={`${row.supplierSkuCode} 入池方式`}
                      className="inline-flex shrink-0 rounded-lg bg-muted p-0.5 ring-1 ring-foreground/10"
                    >
                      <button
                        type="button"
                        aria-pressed={row.mode === "link"}
                        className={cn(
                          "inline-flex h-6 items-center rounded-md px-2 text-xs transition-all",
                          row.mode === "link"
                            ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                            : "text-muted-foreground hover:bg-foreground/5 hover:text-foreground"
                        )}
                        onClick={() =>
                          updateRow(row.supplierCatalogSkuId, { mode: "link" })
                        }
                      >
                        关联
                      </button>
                      <button
                        type="button"
                        aria-pressed={row.mode === "create"}
                        className={cn(
                          "inline-flex h-6 items-center rounded-md px-2 text-xs transition-all",
                          row.mode === "create"
                            ? "bg-card font-medium text-foreground shadow-sm ring-1 ring-foreground/10"
                            : "text-muted-foreground hover:bg-foreground/5 hover:text-foreground"
                        )}
                        onClick={() =>
                          updateRow(row.supplierCatalogSkuId, { mode: "create" })
                        }
                      >
                        改为新建
                      </button>
                    </div>
                  ) : null}
                </div>

                {row.blockedReason ? (
                  <p className="mt-2 text-xs text-muted-foreground">
                    {row.blockedReason}
                    {row.mappedCompanySkuNo ? ` · ${row.mappedCompanySkuNo}` : ""}
                  </p>
                ) : (
                  <div className="mt-3 grid gap-3 sm:grid-cols-2">
                    <div className="space-y-1.5">
                      <Label>代发供给价</Label>
                      <Input
                        className="h-8"
                        value={row.dropshipSupply}
                        onChange={(event) =>
                          updateRow(row.supplierCatalogSkuId, {
                            dropshipSupply: event.target.value,
                          })
                        }
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>集采供给价</Label>
                      <Input
                        className="h-8"
                        value={row.bulkSupply}
                        onChange={(event) =>
                          updateRow(row.supplierCatalogSkuId, {
                            bulkSupply: event.target.value,
                          })
                        }
                      />
                    </div>

                    {row.mode === "link" ? (
                      <div className="space-y-1.5 sm:col-span-2">
                        <Label>关联公司 SKU</Label>
                        <OptionCombobox
                          value={row.companySkuId || null}
                          onValueChange={(value) =>
                            updateRow(row.supplierCatalogSkuId, {
                              companySkuId: value ?? "",
                            })
                          }
                          options={row.candidateOptions}
                          placeholder={
                            row.candidateOptions.length
                              ? "选择候选公司 SKU"
                              : "无候选，请改为新建"
                          }
                          className="w-full"
                          disabled={row.candidateOptions.length === 0}
                        />
                      </div>
                    ) : (
                      <>
                        <div className="space-y-1.5">
                          <Label>销售可见价 *</Label>
                          <Input
                            className="h-8"
                            value={row.salesVisiblePriceGross}
                            onChange={(event) =>
                              updateRow(row.supplierCatalogSkuId, {
                                salesVisiblePriceGross: event.target.value,
                              })
                            }
                          />
                        </div>
                        <div className="space-y-1.5">
                          <Label>市场价 *</Label>
                          <Input
                            className="h-8"
                            value={row.marketPrice}
                            onChange={(event) =>
                              updateRow(row.supplierCatalogSkuId, {
                                marketPrice: event.target.value,
                              })
                            }
                          />
                        </div>
                      </>
                    )}
                  </div>
                )}
              </div>
            ))}
          </div>

          {submitError ? (
            <Alert variant="destructive">
              <AlertTitle>无法提交</AlertTitle>
              <AlertDescription>{submitError}</AlertDescription>
            </Alert>
          ) : null}

          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              关闭
            </DialogClose>
            <Button
              type="submit"
              disabled={pending || results.length > 0 || selectedRows.length === 0}
            >
              {pending
                ? "提交中…"
                : `确认入池（${selectedRows.length} 个 SKU）`}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
