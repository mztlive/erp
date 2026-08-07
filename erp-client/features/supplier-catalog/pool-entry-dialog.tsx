"use client"

import * as React from "react"
import Link from "next/link"

import { OptionCombobox } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
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

type EntryMode = "link" | "reverse"

type LinkRowState = {
  supplierCatalogSkuId: string
  selected: boolean
  supplierSkuCode: string
  specification: string
  poolStatus: PoolMatchStatus
  mappedCompanySkuNo?: string
  companySkuId: string
  dropshipSupply: string
  bulkSupply: string
  moq: string
  candidateOptions: { value: string; label: string }[]
  blockedReason?: string
}

type ReverseRowState = {
  supplierCatalogSkuId: string
  selected: boolean
  supplierSkuCode: string
  specification: string
  dropshipSupply: string
  bulkSupply: string
  moq: string
  salesVisiblePriceGross: string
  marketPrice: string
  blockedReason?: string
}

/**
 * 入池弹窗：关联已有公司 SKU | 反向新建公司商品。
 * 打开时拉取池内匹配状态，默认分支按候选情况选择。
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

  const [mode, setMode] = React.useState<EntryMode>("reverse")
  const [result, setResult] = React.useState<SupplierCatalogWriteResult | null>(
    null
  )
  const [submitError, setSubmitError] = React.useState<string | null>(null)
  const [productKind, setProductKind] = React.useState("")
  const [categoryId, setCategoryId] = React.useState("")
  const [brandId, setBrandId] = React.useState("")
  const [baseUnitId, setBaseUnitId] = React.useState("")
  const [inputTaxRate, setInputTaxRate] = React.useState("")
  const [supplyRegionText, setSupplyRegionText] = React.useState("")
  const [linkRows, setLinkRows] = React.useState<LinkRowState[]>([])
  const [reverseRows, setReverseRows] = React.useState<ReverseRowState[]>([])
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
        setResult(null)
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
    setResult(null)
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

    const matchItems = poolMatchQuery.data.items
    const hasLinkable = matchItems.some(
      (row) =>
        row.poolStatus === "HAS_CANDIDATES" || row.poolStatus === "MAPPED"
    )
    // 有候选时默认关联；全部未匹配时默认反向
    const defaultMode: EntryMode = matchItems.some(
      (row) => row.poolStatus === "HAS_CANDIDATES"
    )
      ? "link"
      : hasLinkable && !matchItems.some((row) => row.poolStatus === "UNMATCHED")
        ? "link"
        : "reverse"
    setMode(defaultMode)

    const nextLink: LinkRowState[] = catalogSkuRowsFromItem(item).map((sku) => {
      const match = matchBySku.get(sku.id)
      const rev = sku.currentRevision
      const moq = rev.bulkMinimumOrderQuantity?.trim() ?? ""
      const candidates = match?.candidates ?? []
      const top = candidates[0]
      const mapped = match?.poolStatus === "MAPPED"
      return {
        supplierCatalogSkuId: sku.id,
        selected:
          !mapped &&
          Boolean(top) &&
          Boolean(moq || rev.dropshipFloorPriceGross || rev.bulkFloorPriceGross),
        supplierSkuCode: sku.supplierSkuCode,
        specification: rev.specification,
        poolStatus: match?.poolStatus ?? "UNMATCHED",
        mappedCompanySkuNo: match?.mappedCompanySkuNo,
        companySkuId: mapped
          ? (match?.mappedCompanySkuId ?? "")
          : (top?.skuId ?? ""),
        dropshipSupply: rev.dropshipFloorPriceGross ?? "",
        bulkSupply: rev.bulkFloorPriceGross ?? "",
        moq: moq || "—",
        candidateOptions: candidates.map((c) => ({
          value: c.skuId,
          label: `${c.skuNo} · ${c.name}${c.specification ? ` · ${c.specification}` : ""}${c.matchSignals.length ? ` · ${c.matchSignals.join("/")}` : ""}`,
        })),
        blockedReason: mapped
          ? "已映射，无需重复关联"
          : !moq
            ? "缺少集采起订量"
            : undefined,
      }
    })
    setLinkRows(nextLink)

    const nextReverse: ReverseRowState[] = catalogSkuRowsFromItem(item).map(
      (sku) => {
        const match = matchBySku.get(sku.id)
        const rev = sku.currentRevision
        const moq = rev.bulkMinimumOrderQuantity?.trim() ?? ""
        const mapped = match?.poolStatus === "MAPPED"
        const hasCandidates = match?.poolStatus === "HAS_CANDIDATES"
        let blockedReason: string | undefined
        if (mapped) blockedReason = "已映射，请用关联入池或改供给"
        else if (!moq) blockedReason = "缺少集采起订量，请先在商品中心补齐"
        return {
          supplierCatalogSkuId: sku.id,
          selected: !blockedReason && !hasCandidates,
          supplierSkuCode: sku.supplierSkuCode,
          specification: rev.specification,
          dropshipSupply: rev.dropshipFloorPriceGross ?? "",
          bulkSupply: rev.bulkFloorPriceGross ?? "",
          moq: moq || "—",
          salesVisiblePriceGross: "",
          marketPrice: "",
          blockedReason,
        }
      }
    )
    setReverseRows(nextReverse)
  }, [
    open,
    item,
    poolMatchQuery.data,
    poolMatchQuery.dataUpdatedAt,
    matchBySku,
  ])

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

    try {
      if (mode === "link") {
        const selected = linkRows.filter(
          (row) => row.selected && !row.blockedReason
        )
        if (selected.length === 0) {
          setSubmitError("请至少勾选一个可关联的供应商 SKU")
          return
        }
        for (const row of selected) {
          if (!row.companySkuId) {
            setSubmitError(`SKU ${row.supplierSkuCode}：请选择公司 SKU`)
            return
          }
          const hasDropship = row.dropshipSupply.trim()
          const hasBulk = row.bulkSupply.trim()
          if (!hasDropship || !hasBulk) {
            setSubmitError(
              `SKU ${row.supplierSkuCode}：请填写代发/集采供给价`
            )
            return
          }
        }
        const response = await linkMutation.mutateAsync({
          supplierProductId: item.supplierProduct.id,
          expectedSourceRevisionNo: expectedRevisionNo,
          inputTaxRate: inputTaxRateDecimal,
          supplyRegion: regions,
          items: selected.map((row) => ({
            supplierCatalogSkuId: row.supplierCatalogSkuId,
            companySkuId: row.companySkuId,
            dropshipSupplyPriceGross: row.dropshipSupply.trim() || undefined,
            bulkSupplyPriceGross: row.bulkSupply.trim() || undefined,
          })),
          idempotencyKey: idempotencyKey("link-promote"),
        })
        setResult(response)
        return
      }

      if (!productKind.trim()) {
        setSubmitError("请选择商品类型")
        return
      }
      if (!categoryId || !brandId || !baseUnitId) {
        setSubmitError("请选择公司分类、品牌与基础单位")
        return
      }
      const selected = reverseRows.filter(
        (row) => row.selected && !row.blockedReason
      )
      if (selected.length === 0) {
        setSubmitError("请至少勾选一个可反向入池的供应商 SKU")
        return
      }
      for (const row of selected) {
        if (!money.safeParse(row.salesVisiblePriceGross).success) {
          setSubmitError(`SKU ${row.supplierSkuCode}：请填写合法销售可见价`)
          return
        }
        if (!money.safeParse(row.marketPrice).success) {
          setSubmitError(`SKU ${row.supplierSkuCode}：请填写合法市场价`)
          return
        }
      }
      const withCandidates = selected.filter((row) => {
        const match = matchBySku.get(row.supplierCatalogSkuId)
        return match?.poolStatus === "HAS_CANDIDATES"
      })
      if (withCandidates.length > 0) {
        const ok = window.confirm(
          `有 ${withCandidates.length} 个 SKU 存在公司商品候选，仍要反向新建吗？建议改用「关联已有」。`
        )
        if (!ok) return
      }

      const response = await reverseMutation.mutateAsync({
        supplierProductId: item.supplierProduct.id,
        expectedSourceRevisionNo: expectedRevisionNo,
        productKind: productKind.trim() as ProductKind,
        categoryId,
        brandId,
        baseUnitId,
        inputTaxRate: inputTaxRateDecimal,
        supplyRegion: regions,
        items: selected.map((row) => ({
          supplierCatalogSkuId: row.supplierCatalogSkuId,
          dropshipSupplyPriceGross: row.dropshipSupply.trim() || undefined,
          bulkSupplyPriceGross: row.bulkSupply.trim() || undefined,
          salesVisiblePriceGross: row.salesVisiblePriceGross.trim(),
          marketPrice: row.marketPrice.trim(),
        })),
        idempotencyKey: idempotencyKey("reverse-promote"),
      })
      setResult(response)
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
      <DialogContent className="flex max-h-[90vh] flex-col sm:max-w-5xl">
        <DialogHeader>
          <DialogTitle>入池</DialogTitle>
          <DialogDescription>
            系统给出池内状态与匹配证据；有同款请关联已有公司 SKU，无同款再反向新建。确认即生效，不填生效日期。
          </DialogDescription>
        </DialogHeader>

        {result ? (
          <Alert>
            <AlertTitle>
              {result.poolEntryChange === "UNCHANGED"
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
        ) : null}

        <form
          className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1"
          onSubmit={(event) => {
            void handleSubmit(event)
          }}
        >
          <div className="space-y-2">
            <Label>池内状态</Label>
            {poolMatchQuery.isLoading ? (
              <p className="text-sm text-muted-foreground">正在匹配公司商品…</p>
            ) : poolMatchQuery.isError ? (
              <Alert variant="destructive">
                <AlertTitle>匹配失败</AlertTitle>
                <AlertDescription>
                  无法加载池内候选，仍可手动选择分支后提交。
                </AlertDescription>
              </Alert>
            ) : (
              <div className="overflow-x-auto rounded-md border">
                <table className="w-full min-w-[640px] text-left text-sm">
                  <thead className="bg-muted/50 text-xs text-muted-foreground">
                    <tr>
                      <th className="px-2 py-2">供应商 SKU</th>
                      <th className="px-2 py-2">规格</th>
                      <th className="px-2 py-2">状态</th>
                      <th className="px-2 py-2">匹配证据 / 映射</th>
                    </tr>
                  </thead>
                  <tbody>
                    {(poolMatchQuery.data?.items ?? []).map((row) => (
                      <tr key={row.supplierCatalogSkuId} className="border-t">
                        <td className="px-2 py-2 font-medium">
                          {row.supplierSkuCode}
                        </td>
                        <td className="px-2 py-2 text-muted-foreground">
                          {row.specification || "—"}
                        </td>
                        <td className="px-2 py-2">
                          {poolStatusLabel(row.poolStatus)}
                        </td>
                        <td className="px-2 py-2 text-xs text-muted-foreground">
                          {row.poolStatus === "MAPPED"
                            ? `已映射 ${row.mappedCompanySkuNo ?? row.mappedCompanySkuId}`
                            : row.candidates.length
                              ? row.candidates
                                  .slice(0, 2)
                                  .map(
                                    (c) =>
                                      `${c.skuNo}（${c.matchSignals.join("、") || "弱匹配"}）`
                                  )
                                  .join("；")
                              : "无可靠候选"}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>

          <div className="flex flex-wrap gap-2">
            <Button
              type="button"
              size="sm"
              variant={mode === "link" ? "default" : "outline"}
              onClick={() => setMode("link")}
            >
              关联已有
            </Button>
            <Button
              type="button"
              size="sm"
              variant={mode === "reverse" ? "default" : "outline"}
              onClick={() => setMode("reverse")}
            >
              反向新建
            </Button>
          </div>

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

          {mode === "link" ? (
            <div className="space-y-2">
              <Label>关联公司 SKU</Label>
              <div className="overflow-x-auto rounded-md border">
                <table className="w-full min-w-[900px] text-left text-sm">
                  <thead className="bg-muted/50 text-xs text-muted-foreground">
                    <tr>
                      <th className="px-2 py-2">选</th>
                      <th className="px-2 py-2">供应商 SKU</th>
                      <th className="px-2 py-2">状态</th>
                      <th className="px-2 py-2">公司 SKU</th>
                      <th className="px-2 py-2">代发供给价</th>
                      <th className="px-2 py-2">集采供给价</th>
                      <th className="px-2 py-2">起订量</th>
                    </tr>
                  </thead>
                  <tbody>
                    {linkRows.map((row) => (
                      <tr
                        key={row.supplierCatalogSkuId}
                        className="border-t align-top"
                      >
                        <td className="px-2 py-2">
                          <Checkbox
                            checked={row.selected && !row.blockedReason}
                            disabled={Boolean(row.blockedReason)}
                            onCheckedChange={(checked) =>
                              setLinkRows((rows) =>
                                rows.map((r) =>
                                  r.supplierCatalogSkuId ===
                                  row.supplierCatalogSkuId
                                    ? { ...r, selected: checked === true }
                                    : r
                                )
                              )
                            }
                          />
                        </td>
                        <td className="px-2 py-2">
                          <div className="font-medium">{row.supplierSkuCode}</div>
                          <div className="text-xs text-muted-foreground">
                            {row.specification || "—"}
                          </div>
                          {row.blockedReason ? (
                            <p className="text-xs text-destructive">
                              {row.blockedReason}
                            </p>
                          ) : null}
                        </td>
                        <td className="px-2 py-2">
                          {poolStatusLabel(row.poolStatus)}
                          {row.mappedCompanySkuNo
                            ? ` · ${row.mappedCompanySkuNo}`
                            : ""}
                        </td>
                        <td className="px-2 py-2 min-w-[220px]">
                          <OptionCombobox
                            value={row.companySkuId || null}
                            onValueChange={(value) =>
                              setLinkRows((rows) =>
                                rows.map((r) =>
                                  r.supplierCatalogSkuId ===
                                  row.supplierCatalogSkuId
                                    ? { ...r, companySkuId: value ?? "" }
                                    : r
                                )
                              )
                            }
                            options={row.candidateOptions}
                            placeholder={
                              row.candidateOptions.length
                                ? "选择候选公司 SKU"
                                : "无候选（请反向新建或先建公司 SKU）"
                            }
                            className="w-full"
                            disabled={
                              Boolean(row.blockedReason) ||
                              row.candidateOptions.length === 0
                            }
                          />
                        </td>
                        <td className="px-2 py-2">
                          <Input
                            className="h-8"
                            value={row.dropshipSupply}
                            onChange={(event) =>
                              setLinkRows((rows) =>
                                rows.map((r) =>
                                  r.supplierCatalogSkuId ===
                                  row.supplierCatalogSkuId
                                    ? {
                                        ...r,
                                        dropshipSupply: event.target.value,
                                      }
                                    : r
                                )
                              )
                            }
                          />
                        </td>
                        <td className="px-2 py-2">
                          <Input
                            className="h-8"
                            value={row.bulkSupply}
                            onChange={(event) =>
                              setLinkRows((rows) =>
                                rows.map((r) =>
                                  r.supplierCatalogSkuId ===
                                  row.supplierCatalogSkuId
                                    ? { ...r, bulkSupply: event.target.value }
                                    : r
                                )
                              )
                            }
                          />
                        </td>
                        <td className="px-2 py-2 num">{row.moq}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              <p className="text-xs text-muted-foreground">
                关联不修改公司销售可见价；只追加本供应商映射与供给。
              </p>
            </div>
          ) : (
            <div className="space-y-3">
              <div className="grid gap-4 sm:grid-cols-2">
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
              </div>
              <div className="overflow-x-auto rounded-md border">
                <table className="w-full min-w-[900px] text-left text-sm">
                  <thead className="bg-muted/50 text-xs text-muted-foreground">
                    <tr>
                      <th className="px-2 py-2">选</th>
                      <th className="px-2 py-2">供应商 SKU</th>
                      <th className="px-2 py-2">起订量</th>
                      <th className="px-2 py-2">代发供给价</th>
                      <th className="px-2 py-2">集采供给价</th>
                      <th className="px-2 py-2">销售可见价 *</th>
                      <th className="px-2 py-2">市场价 *</th>
                    </tr>
                  </thead>
                  <tbody>
                    {reverseRows.map((row) => (
                      <tr
                        key={row.supplierCatalogSkuId}
                        className="border-t align-top"
                      >
                        <td className="px-2 py-2">
                          <Checkbox
                            checked={row.selected && !row.blockedReason}
                            disabled={Boolean(row.blockedReason)}
                            onCheckedChange={(checked) =>
                              setReverseRows((rows) =>
                                rows.map((r) =>
                                  r.supplierCatalogSkuId ===
                                  row.supplierCatalogSkuId
                                    ? { ...r, selected: checked === true }
                                    : r
                                )
                              )
                            }
                          />
                        </td>
                        <td className="px-2 py-2">
                          <div className="font-medium">{row.supplierSkuCode}</div>
                          <div className="text-xs text-muted-foreground">
                            {row.specification || "—"}
                          </div>
                          {row.blockedReason ? (
                            <p className="text-xs text-destructive">
                              {row.blockedReason}
                            </p>
                          ) : matchBySku.get(row.supplierCatalogSkuId)
                              ?.poolStatus === "HAS_CANDIDATES" ? (
                            <p className="text-xs text-amber-700 dark:text-amber-400">
                              有候选，建议改用关联
                            </p>
                          ) : null}
                        </td>
                        <td className="px-2 py-2 num">{row.moq}</td>
                        <td className="px-2 py-2">
                          <Input
                            className="h-8"
                            value={row.dropshipSupply}
                            onChange={(event) =>
                              setReverseRows((rows) =>
                                rows.map((r) =>
                                  r.supplierCatalogSkuId ===
                                  row.supplierCatalogSkuId
                                    ? {
                                        ...r,
                                        dropshipSupply: event.target.value,
                                      }
                                    : r
                                )
                              )
                            }
                          />
                        </td>
                        <td className="px-2 py-2">
                          <Input
                            className="h-8"
                            value={row.bulkSupply}
                            onChange={(event) =>
                              setReverseRows((rows) =>
                                rows.map((r) =>
                                  r.supplierCatalogSkuId ===
                                  row.supplierCatalogSkuId
                                    ? { ...r, bulkSupply: event.target.value }
                                    : r
                                )
                              )
                            }
                          />
                        </td>
                        <td className="px-2 py-2">
                          <Input
                            className="h-8"
                            value={row.salesVisiblePriceGross}
                            onChange={(event) =>
                              setReverseRows((rows) =>
                                rows.map((r) =>
                                  r.supplierCatalogSkuId ===
                                  row.supplierCatalogSkuId
                                    ? {
                                        ...r,
                                        salesVisiblePriceGross:
                                          event.target.value,
                                      }
                                    : r
                                )
                              )
                            }
                          />
                        </td>
                        <td className="px-2 py-2">
                          <Input
                            className="h-8"
                            value={row.marketPrice}
                            onChange={(event) =>
                              setReverseRows((rows) =>
                                rows.map((r) =>
                                  r.supplierCatalogSkuId ===
                                  row.supplierCatalogSkuId
                                    ? { ...r, marketPrice: event.target.value }
                                    : r
                                )
                              )
                            }
                          />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}

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
            <Button type="submit" disabled={pending || Boolean(result)}>
              {pending
                ? "提交中…"
                : mode === "link"
                  ? "确认关联入池"
                  : "确认反向入池"}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
