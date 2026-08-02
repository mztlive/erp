"use client"

import * as React from "react"
import Link from "next/link"
import { z } from "zod"

import { OptionCombobox } from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { FileUpload } from "@/components/ui/file-upload"
import { Label } from "@/components/ui/label"
import {
  useCompanySkuOptionsQuery,
  useCreateSupplierCatalogItemMutation,
  usePromoteSupplierProductMutation,
} from "@/features/supplier-catalog/queries"
import type {
  SupplierCatalogItemView,
  SupplierCatalogSourceType,
} from "@/features/supplier-catalog/types"
import { useMasterDataListQuery } from "@/features/master-data/queries"

type FixedSku = Readonly<{
  skuId: string
  skuCode: string
  skuName: string
  specification: string
  baseUnit: string
  salesVisiblePrice?: string
}>

const money = z
  .string()
  .trim()
  .regex(/^\d+(?:\.\d{1,4})?$/, "请输入正确金额，最多 4 位小数")

const intakeSchema = z.object({
  supplierId: z.string().min(1, "请选择供应商"),
  sourceReference: z.string(),
  supplierSpuCode: z.string(),
  supplierSkuCode: z.string().trim().min(1, "请填写供应商 SKU 编码"),
  name: z.string().trim().min(2, "请填写商品名称"),
  specification: z.string().trim().min(1, "请填写规格"),
  category: z.string().trim().min(1, "请填写来源分类"),
  sourceQuotedPriceGross: money,
  inputTaxRate: z.string().trim().regex(/^0(?:\.\d{1,6})?$|^1(?:\.0+)?$/, "请输入 0 到 1 的税率"),
  supplyRegionText: z.string().trim().min(1, "请填写可供区域"),
  minimumOrderQuantity: z.string().trim().regex(/^\d+(?:\.\d{1,6})?$/, "请输入正确起订量"),
  supplyMode: z.enum(["DROPSHIP", "BULK"]),
  validFrom: z.string().min(1, "请选择生效日期"),
  salesVisiblePrice: z.string(),
})

function idempotencyKey(prefix: string) {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

function splitValues(value: string) {
  return value
    .split(/[，,、]/)
    .map((item) => item.trim())
    .filter(Boolean)
}

export function SupplierCatalogIntakeDialog({
  open,
  onOpenChange,
  sourceType,
  fixedSku,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  sourceType: Exclude<SupplierCatalogSourceType, "API">
  fixedSku?: FixedSku
}) {
  const supplierQuery = useMasterDataListQuery({
    resource: "suppliers",
    lifecycleStatus: "enabled",
    revisionTiming: "current",
  })
  const createMutation = useCreateSupplierCatalogItemMutation()
  const [result, setResult] = React.useState<string | null>(null)
  const form = useAppForm({
    defaultValues: {
      supplierId: "",
      sourceReference: "",
      supplierSpuCode: "",
      supplierSkuCode: "",
      name: "",
      specification: fixedSku?.specification ?? "",
      category: "",
      sourceQuotedPriceGross: "",
      inputTaxRate: "0.13",
      supplyRegionText: "全国",
      minimumOrderQuantity: "1",
      supplyMode: "BULK" as "DROPSHIP" | "BULK",
      validFrom: "2026-08-02",
      salesVisiblePrice: fixedSku?.salesVisiblePrice ?? "",
    },
    validators: {
      onSubmit: fixedSku
        ? intakeSchema.extend({ salesVisiblePrice: money })
        : intakeSchema,
    },
    onSubmit: async ({ value }) => {
      const supplier = supplierQuery.data?.rows.find(
        (row) => row.stableId === value.supplierId
      )
      if (!supplier) return
      const response = await createMutation.mutateAsync({
        sourceType,
        supplierId: supplier.stableId,
        supplierName: supplier.name,
        supplierSpuCode: value.supplierSpuCode.trim() || undefined,
        supplierSkuCode: value.supplierSkuCode.trim(),
        name: value.name.trim(),
        specification: value.specification.trim(),
        category: value.category.trim(),
        sourceQuotedPriceGross: value.sourceQuotedPriceGross.trim(),
        confirmedCostGross: fixedSku
          ? value.sourceQuotedPriceGross.trim()
          : undefined,
        inputTaxRate: value.inputTaxRate.trim(),
        supplyRegion: splitValues(value.supplyRegionText),
        sourceReference: value.sourceReference.trim() || undefined,
        targetSkuId: fixedSku?.skuId,
        targetSkuCode: fixedSku?.skuCode,
        targetSkuName: fixedSku?.skuName,
        baseUnit: fixedSku?.baseUnit,
        salesVisiblePrice: fixedSku
          ? value.salesVisiblePrice.trim() || undefined
          : undefined,
        minimumOrderQuantity: value.minimumOrderQuantity.trim(),
        supplyMode: value.supplyMode,
        validFrom: value.validFrom,
        idempotencyKey: idempotencyKey("supplier-catalog-intake"),
      })
      setResult(response.reference)
    },
  })

  React.useEffect(() => {
    if (!open) setResult(null)
  }, [open])

  const supplierOptions = (supplierQuery.data?.rows ?? []).map((supplier) => ({
    value: supplier.stableId,
    label: `${supplier.name} · ${supplier.stableNo}`,
  }))

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[88vh] overflow-y-auto sm:max-w-3xl">
        <DialogHeader>
          <DialogTitle>
            {sourceType === "EXCEL" ? "导入供应商商品 Excel" : "手工录入供应商商品"}
          </DialogTitle>
          <DialogDescription>
            三种来源使用同一套供应商 SPU/SKU 结构。采购成本只对采购授权角色返回；销售只看到公司商品池价格。
          </DialogDescription>
        </DialogHeader>

        {result ? (
          <Alert>
            <AlertTitle>供应商商品已保存</AlertTitle>
            <AlertDescription>业务记录 {result} 已形成。</AlertDescription>
          </Alert>
        ) : null}

        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault()
            void form.handleSubmit()
          }}
        >
          {sourceType === "EXCEL" ? (
            <form.AppField name="sourceReference">
              {(field) => (
                <FileUpload
                  accept=".xlsx,.xls,.csv"
                  multiple={false}
                  label="供应商商品表"
                  description={field.state.value || "支持 xlsx、xls、csv；按供应商模板校验 SPU/SKU、报价和规格"}
                  onFilesSelected={(files) => {
                    if (files[0]) field.handleChange(files[0].name)
                  }}
                />
              )}
            </form.AppField>
          ) : null}

          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="supplierId">
              {(field) => (
                <div className="space-y-1.5 sm:col-span-2">
                  <Label>供应商 *</Label>
                  <OptionCombobox
                    value={field.state.value || null}
                    onValueChange={(value) => field.handleChange(value ?? "")}
                    options={supplierOptions}
                    placeholder="选择已启用供应商"
                    className="w-full"
                  />
                </div>
              )}
            </form.AppField>
            <form.AppField name="supplierSpuCode">
              {(field) => <field.TextField label="供应商 SPU 编码（可空）" />}
            </form.AppField>
            <form.AppField name="supplierSkuCode">
              {(field) => <field.TextField label="供应商 SKU 编码 *" />}
            </form.AppField>
            <form.AppField name="name">
              {(field) => <field.TextField label="供应商商品名称 *" />}
            </form.AppField>
            <form.AppField name="specification">
              {(field) => <field.TextField label="供应商规格 *" />}
            </form.AppField>
            <form.AppField name="category">
              {(field) => <field.TextField label="来源分类 *" />}
            </form.AppField>
            <form.AppField name="sourceQuotedPriceGross">
              {(field) => (
                <field.TextField
                  label={fixedSku ? "采购确认含税成本 *" : "供应商含税报价 *"}
                  description={
                    fixedSku
                      ? "同时保留为本次手工来源报价；后续成本变化形成新的供给修订"
                      : "来源报价只保存到供应商商品修订，入池时再由采购确认成本"
                  }
                />
              )}
            </form.AppField>
            <form.AppField name="inputTaxRate">
              {(field) => <field.TextField label="进项税率 *" />}
            </form.AppField>
            <form.AppField name="minimumOrderQuantity">
              {(field) => <field.TextField label="最小起订量 *" />}
            </form.AppField>
            <form.AppField name="supplyMode">
              {(field) => (
                <div className="space-y-1.5">
                  <Label>供给方式 *</Label>
                  <OptionCombobox
                    value={field.state.value}
                    onValueChange={(value) =>
                      field.handleChange((value ?? "BULK") as "DROPSHIP" | "BULK")
                    }
                    options={[
                      { value: "DROPSHIP", label: "一件代发" },
                      { value: "BULK", label: "集采" },
                    ]}
                    allowClear={false}
                    className="w-full"
                  />
                </div>
              )}
            </form.AppField>
            <form.AppField name="supplyRegionText">
              {(field) => <field.TextField label="可供区域（逗号分隔）" />}
            </form.AppField>
            <form.AppField name="validFrom">
              {(field) => <field.TextField label="生效日期 *" type="date" />}
            </form.AppField>
            {fixedSku ? (
              <form.AppField name="salesVisiblePrice">
                {(field) => (
                  <field.TextField
                    label="销售可见价"
                    description={`加入 ${fixedSku.skuCode} 的公司商品池价格；不等于采购成本`}
                  />
                )}
              </form.AppField>
            ) : null}
          </div>

          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              关闭
            </DialogClose>
            <form.AppForm>
              <form.SubmitButton
                label={fixedSku ? "保存商品与供给" : "保存到供应商商品库"}
                disabled={createMutation.isPending || Boolean(result)}
              />
            </form.AppForm>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

const promoteSchema = z.object({
  targetSkuId: z.string().min(1, "请选择公司 SKU"),
  confirmedCostGross: money,
  salesVisiblePrice: money,
  inputTaxRate: z.string().trim().min(1, "请填写税率"),
  minimumOrderQuantity: z.string().trim().min(1, "请填写起订量"),
  supplyMode: z.enum(["DROPSHIP", "BULK"]),
  supplyRegionText: z.string().trim().min(1, "请填写可供区域"),
  validFrom: z.string().min(1, "请选择生效日期"),
})

export function PromoteSupplierProductDialog({
  item,
  open,
  onOpenChange,
}: {
  item?: SupplierCatalogItemView
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const skuQuery = useCompanySkuOptionsQuery()
  const promoteMutation = usePromoteSupplierProductMutation()
  const [result, setResult] = React.useState<string | null>(null)
  const sourceRevision = item?.supplierProduct.incomingRevision ?? item?.supplierProduct.currentRevision
  const initialSku = item?.skuCandidates[0]?.skuId ?? ""
  const form = useAppForm({
    defaultValues: {
      targetSkuId: initialSku,
      confirmedCostGross: sourceRevision?.sourceQuotedPriceGross ?? "",
      salesVisiblePrice: "",
      inputTaxRate: sourceRevision?.inputTaxRate ?? "0.13",
      minimumOrderQuantity:
        item?.offering?.proposedDefaults?.minimumOrderQuantity ?? "1",
      supplyMode:
        item?.offering?.proposedDefaults?.supplyMode ??
        ("BULK" as "DROPSHIP" | "BULK"),
      supplyRegionText: sourceRevision?.supplyRegion.join("、") ?? "全国",
      validFrom: "2026-08-02",
    },
    validators: { onSubmit: promoteSchema },
    onSubmit: async ({ value }) => {
      if (!item) return
      const sku = skuQuery.data?.find((candidate) => candidate.skuId === value.targetSkuId)
      if (!sku) return
      const response = await promoteMutation.mutateAsync({
        supplierProductId: item.supplierProduct.id,
        targetSkuId: sku.skuId,
        targetSkuCode: sku.skuCode,
        targetSkuName: sku.skuName,
        specification: sku.specification,
        baseUnit: sku.baseUnit,
        confirmedCostGross: value.confirmedCostGross,
        inputTaxRate: value.inputTaxRate,
        minimumOrderQuantity: value.minimumOrderQuantity,
        supplyMode: value.supplyMode,
        supplyRegion: splitValues(value.supplyRegionText),
        validFrom: value.validFrom,
        salesVisiblePrice: value.salesVisiblePrice,
        idempotencyKey: idempotencyKey("promote-supplier-product"),
      })
      setResult(response.reference)
    },
  })

  React.useEffect(() => {
    if (!open) setResult(null)
  }, [open])

  const skuOptions = (skuQuery.data ?? []).map((sku) => ({
    value: sku.skuId,
    label: `${sku.skuCode} · ${sku.skuName} · ${sku.specification}`,
  }))

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>加入公司商品池</DialogTitle>
          <DialogDescription>
            关联公司 SKU，同时确认该供应商的采购成本，并设置销售人员可见的商品池价格。
          </DialogDescription>
        </DialogHeader>
        {result ? (
          <Alert>
            <AlertTitle>已加入公司商品池</AlertTitle>
            <AlertDescription>业务记录 {result} 已形成。</AlertDescription>
          </Alert>
        ) : null}
        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault()
            void form.handleSubmit()
          }}
        >
          <form.AppField name="targetSkuId">
            {(field) => (
              <div className="space-y-1.5">
                <Label>公司 SKU *</Label>
                <OptionCombobox
                  value={field.state.value || null}
                  onValueChange={(value) => field.handleChange(value ?? "")}
                  options={skuOptions}
                  placeholder="选择已有公司 SKU"
                  className="w-full"
                />
              </div>
            )}
          </form.AppField>
          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="confirmedCostGross">
              {(field) => (
                <field.TextField
                  label="采购确认含税成本 *"
                  description="采购私密字段；销售查询和导出不返回原值"
                />
              )}
            </form.AppField>
            <form.AppField name="salesVisiblePrice">
              {(field) => (
                <field.TextField
                  label="销售可见价 *"
                  description="公司商品池报价基准，与供应商成本独立"
                />
              )}
            </form.AppField>
            <form.AppField name="inputTaxRate">
              {(field) => <field.TextField label="进项税率 *" />}
            </form.AppField>
            <form.AppField name="minimumOrderQuantity">
              {(field) => <field.TextField label="最小起订量 *" />}
            </form.AppField>
            <form.AppField name="supplyMode">
              {(field) => (
                <div className="space-y-1.5">
                  <Label>供给方式 *</Label>
                  <OptionCombobox
                    value={field.state.value}
                    onValueChange={(value) =>
                      field.handleChange((value ?? "BULK") as "DROPSHIP" | "BULK")
                    }
                    options={[
                      { value: "DROPSHIP", label: "一件代发" },
                      { value: "BULK", label: "集采" },
                    ]}
                    allowClear={false}
                    className="w-full"
                  />
                </div>
              )}
            </form.AppField>
            <form.AppField name="supplyRegionText">
              {(field) => <field.TextField label="可供区域" />}
            </form.AppField>
            <form.AppField name="validFrom">
              {(field) => <field.TextField label="生效日期 *" type="date" />}
            </form.AppField>
          </div>
          <DialogFooter>
            {item ? (
              <Button
                type="button"
                variant="outline"
                render={
                  <Link
                    href={`/master-data/products/new?from=W21&supplierProductId=${encodeURIComponent(item.supplierProduct.id)}`}
                  />
                }
              >
                先新建公司商品
              </Button>
            ) : null}
            <DialogClose render={<Button type="button" variant="outline" />}>
              关闭
            </DialogClose>
            <form.AppForm>
              <form.SubmitButton
                label="确认加入商品池"
                disabled={promoteMutation.isPending || Boolean(result)}
              />
            </form.AppForm>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export type { FixedSku }
