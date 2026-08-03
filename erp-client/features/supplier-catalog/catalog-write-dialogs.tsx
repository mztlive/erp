"use client"

import * as React from "react"
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
import { ToggleGroup, ToggleGroupItem } from "@/components/ui/toggle-group"
import {
  useCompanySkuOptionsQuery,
  useCreateSupplierCatalogItemMutation,
  usePromoteSupplierProductMutation,
} from "@/features/supplier-catalog/queries"
import type {
  SupplierCatalogItemView,
  SupplierCatalogSourceType,
  SupplierCatalogWriteResult,
  SupplyMode,
} from "@/features/supplier-catalog/types"
import { useMasterDataListQuery } from "@/features/master-data/queries"

/**
 * W14 固定 SKU 的「添加供应商并登记成本」上下文。
 * 公司侧资料（名称/规格/分类/品牌/条码/媒体）在登记供给时反向复用
 * 为供应商商品的基础快照，对话框只补录供应商独有差异。
 */
type FixedSku = Readonly<{
  skuId: string
  skuCode: string
  skuName: string
  specification: string
  baseUnit: string
  category?: string
  brand?: string
  barcode?: string
  description?: string
  carouselImages?: readonly string[]
  detailImages?: readonly string[]
  mainImage?: string
  salesVisiblePrice?: string
  hasPoolEntry?: boolean
}>

const money = z
  .string()
  .trim()
  .regex(/^\d+(?:\.\d{1,4})?$/, "请输入正确金额，最多 4 位小数")

/** 供给方式多选；对话框不再收集生效日期，提交默认当日生效。 */
const supplyModes = (): readonly SupplyMode[] => ["BULK"]

function todayIso(): string {
  return new Date().toISOString().slice(0, 10)
}

function SupplyModeToggle({
  value,
  onChange,
}: {
  value: readonly SupplyMode[]
  onChange: (next: readonly SupplyMode[]) => void
}) {
  return (
    <ToggleGroup
      multiple
      value={value}
      onValueChange={(next) =>
        onChange(next as readonly SupplyMode[])
      }
      variant="outline"
      size="sm"
      aria-label="供给方式（可多选）"
    >
      <ToggleGroupItem value="DROPSHIP">一件代发</ToggleGroupItem>
      <ToggleGroupItem value="BULK">集采</ToggleGroupItem>
    </ToggleGroup>
  )
}

/** 金额字段在勾选对应供给方式时才必填；未勾选时允许为空。 */
function supplyPriceIssue(
  mode: SupplyMode,
  price: string,
  label: string,
): { path: "dropshipFloorPriceGross" | "bulkFloorPriceGross"; message: string } | null {
  if (!price.trim() || !money.safeParse(price.trim()).success) {
    return { path: mode === "DROPSHIP" ? "dropshipFloorPriceGross" : "bulkFloorPriceGross", message: `勾选「${label}」时必须填写对应底价（最多 4 位小数）` }
  }
  return null
}

function buildIntakeSchema(requireSalesVisiblePrice: boolean) {
  return z.object({
    supplierId: z.string().min(1, "请选择供应商"),
    sourceReference: z.string(),
    supplierSpuCode: z.string(),
    supplierSkuCode: z.string().trim().min(1, "请填写供应商 SKU 编码"),
    name: z.string().trim().min(2, "请填写商品名称"),
    description: z.string(),
    specification: z.string().trim().min(1, "请填写规格"),
    category: z.string().trim().min(1, "请填写来源分类"),
    brand: z.string(),
    sourceBaseUnit: z.string(),
    barcode: z.string(),
    attributeText: z.string(),
    carouselImages: z.array(z.string()),
    detailImages: z.array(z.string()),
    skuMainImage: z.string(),
    dropshipFloorPriceGross: z.string(),
    bulkFloorPriceGross: z.string(),
    bulkMinimumOrderQuantity: z
      .string()
      .trim()
      .regex(/^\d+(?:\.\d{1,6})?$/, "请输入正确集采起订量"),
    minimumOrderQuantity: z
      .string()
      .trim()
      .regex(/^\d+(?:\.\d{1,6})?$/, "请输入正确起订量"),
    supplyMode: z
      .array(z.enum(["DROPSHIP", "BULK"]))
      .min(1, "请至少选择一种供给方式"),
    salesVisiblePrice: z.string(),
  }).superRefine((value, context) => {
    const dropship = value.supplyMode.includes("DROPSHIP")
      ? supplyPriceIssue("DROPSHIP", value.dropshipFloorPriceGross, "一件代发")
      : null
    if (dropship) {
      context.addIssue({ code: "custom", path: [dropship.path], message: dropship.message })
    }
    const bulk = value.supplyMode.includes("BULK")
      ? supplyPriceIssue("BULK", value.bulkFloorPriceGross, "集采")
      : null
    if (bulk) {
      context.addIssue({ code: "custom", path: [bulk.path], message: bulk.message })
    }
    if (
      requireSalesVisiblePrice &&
      !money.safeParse(value.salesVisiblePrice.trim()).success
    ) {
      context.addIssue({
        code: "custom",
        path: ["salesVisiblePrice"],
        message: "该公司 SKU 尚未进入商品池，请填写销售可见价（最多 4 位小数）",
      })
    }
  })
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

function splitAttributes(value: string) {
  return value
    .split(/[；;\n]/)
    .map((item) => item.trim())
    .filter(Boolean)
    .map((item) => {
      const [name, ...rest] = item.split(/[:：]/)
      return { name: name?.trim() ?? "", value: rest.join(":").trim() }
    })
    .filter((item) => item.name && item.value)
}

function companySkuMatchSignals(
  sku: {
    barcode?: string
    brand?: string
    category?: string
    baseUnit: string
    specification: string
  },
  source: SupplierCatalogItemView["supplierProduct"]["currentRevision"],
) {
  const signals: string[] = []
  if (sku.barcode && source.barcode && sku.barcode === source.barcode) {
    signals.push("条码一致")
  }
  if (sku.brand && source.brand && sku.brand === source.brand) {
    signals.push("品牌一致")
  }
  if (sku.category && source.category && sku.category === source.category) {
    signals.push("类目一致")
  }
  if (sku.baseUnit && source.baseUnit && sku.baseUnit === source.baseUnit) {
    signals.push("单位一致")
  }
  if (
    sku.specification &&
    source.specification &&
    (sku.specification.includes(source.specification) ||
      source.specification.includes(sku.specification))
  ) {
    signals.push("规格一致")
  }
  return signals
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
      description: "",
      specification: fixedSku?.specification ?? "",
      category: "",
      brand: "",
      sourceBaseUnit: fixedSku?.baseUnit ?? "",
      barcode: "",
      attributeText: "",
      carouselImages: [] as string[],
      detailImages: [] as string[],
      skuMainImage: "",
      dropshipFloorPriceGross: "",
      bulkFloorPriceGross: "",
      bulkMinimumOrderQuantity: "1",
      minimumOrderQuantity: "1",
      supplyMode: supplyModes(),
      salesVisiblePrice: fixedSku?.hasPoolEntry ? "" : fixedSku?.salesVisiblePrice ?? "",
    },
    validators: {
      onSubmit: buildIntakeSchema(Boolean(fixedSku && !fixedSku.hasPoolEntry)),
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
        description: value.description.trim() || undefined,
        specification: value.specification.trim(),
        category: value.category.trim(),
        brand: value.brand.trim() || undefined,
        sourceBaseUnit: value.sourceBaseUnit.trim() || undefined,
        barcode: value.barcode.trim() || undefined,
        attributes: splitAttributes(value.attributeText),
        media: [
          ...value.carouselImages.map((fileName, index) => ({
            usage: "SPU_CAROUSEL" as const,
            fileName,
            sortOrder: index,
            fileAssetId: `asset:${fileName}`,
            archiveStatus: "ARCHIVED" as const,
          })),
          ...value.detailImages.map((fileName, index) => ({
            usage: "SPU_DETAIL" as const,
            fileName,
            sortOrder: index,
            fileAssetId: `asset:${fileName}`,
            archiveStatus: "ARCHIVED" as const,
          })),
          ...(value.skuMainImage
            ? [{
                usage: "SKU_MAIN" as const,
                fileName: value.skuMainImage,
                sortOrder: 0,
                fileAssetId: `asset:${value.skuMainImage}`,
                archiveStatus: "ARCHIVED" as const,
              }]
            : []),
        ],
        dropshipFloorPriceGross: value.dropshipFloorPriceGross.trim() || undefined,
        bulkFloorPriceGross: value.bulkFloorPriceGross.trim() || undefined,
        bulkMinimumOrderQuantity: value.bulkMinimumOrderQuantity.trim(),
        confirmedCostGross: fixedSku
          ? value.bulkFloorPriceGross.trim() ||
            value.dropshipFloorPriceGross.trim()
          : undefined,
        inputTaxRate: "0.13",
        supplyRegion: fixedSku ? ["全国"] : undefined,
        sourceReference: value.sourceReference.trim() || undefined,
        targetSkuId: fixedSku?.skuId,
        targetSkuCode: fixedSku?.skuCode,
        targetSkuName: fixedSku?.skuName,
        targetSpecification: fixedSku?.specification,
        baseUnit: fixedSku?.baseUnit,
        salesVisiblePrice: fixedSku
          ? value.salesVisiblePrice.trim() || undefined
          : undefined,
        poolPriceAction: fixedSku
          ? fixedSku.hasPoolEntry
            ? "KEEP_EXISTING"
            : "SET_PRICE"
          : undefined,
        minimumOrderQuantity: value.minimumOrderQuantity.trim(),
        supplyMode: value.supplyMode,
        validFrom: todayIso(),
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
            <form.AppField name="description">
              {(field) => (
                <field.TextareaField
                  label="来源商品描述"
                  description="供应商来源描述；不自动写入公司商品"
                />
              )}
            </form.AppField>
            <form.AppField name="specification">
              {(field) => <field.TextField label="供应商规格 *" />}
            </form.AppField>
            <form.AppField name="category">
              {(field) => <field.TextField label="来源分类 *" />}
            </form.AppField>
            <form.AppField name="brand">
              {(field) => <field.TextField label="来源品牌" />}
            </form.AppField>
            <form.AppField name="sourceBaseUnit">
              {(field) => <field.TextField label="来源单位" />}
            </form.AppField>
            <form.AppField name="barcode">
              {(field) => <field.TextField label="商品条码" />}
            </form.AppField>
            <form.AppField name="attributeText">
              {(field) => (
                <field.TextField
                  label="规格属性"
                  description="例如：净含量：250g；包装：铁罐"
                />
              )}
            </form.AppField>
            <form.AppField name="dropshipFloorPriceGross">
              {(field) => (
                <field.TextField
                  label="一件代发底价（含税运）"
                  description="勾选「一件代发」时必填；供应商商品资料保留双底价"
                />
              )}
            </form.AppField>
            <form.AppField name="bulkFloorPriceGross">
              {(field) => (
                <field.TextField
                  label="集采底价（含税）"
                  description={
                    fixedSku
                      ? "勾选「集采」时必填；登记供给时默认用作采购确认成本"
                      : "勾选「集采」时必填"
                  }
                />
              )}
            </form.AppField>
            <form.AppField name="bulkMinimumOrderQuantity">
              {(field) => <field.TextField label="集采起订量 *" />}
            </form.AppField>
            <form.AppField name="minimumOrderQuantity">
              {(field) => (
                <field.TextField
                  label={fixedSku ? "供给起订量 *" : "最小起订量 *"}
                />
              )}
            </form.AppField>
            <form.AppField name="supplyMode">
              {(field) => (
                <div className="space-y-1.5">
                  <Label>供给方式（可多选）*</Label>
                  <SupplyModeToggle
                    value={field.state.value}
                    onChange={(next) => field.handleChange([...next])}
                  />
                </div>
              )}
            </form.AppField>
            {fixedSku?.hasPoolEntry ? (
              <Alert className="sm:col-span-2">
                <AlertTitle>沿用现有公司商品池价格</AlertTitle>
                <AlertDescription>
                  当前销售可见价 ¥{fixedSku.salesVisiblePrice ?? "—"}；本次只新增供应商映射和供给，不形成商品池价格修订。
                </AlertDescription>
              </Alert>
            ) : fixedSku ? (
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

          <div className="grid gap-4 sm:grid-cols-3">
            <form.AppField name="skuMainImage">
              {(field) => (
                <FileUpload
                  accept="image/jpeg,image/png,image/webp"
                  multiple={false}
                  label="来源 SKU 主图"
                  description={field.state.value || "可空；首次建品时若仍缺主图，必须补齐后才能保存"}
                  onFilesSelected={(files) => field.handleChange(files[0]?.name ?? "")}
                />
              )}
            </form.AppField>
            <form.AppField name="carouselImages">
              {(field) => (
                <FileUpload
                  accept="image/jpeg,image/png,image/webp"
                  multiple
                  label="来源轮播图"
                  description={field.state.value.length ? `${field.state.value.length} 张` : "可上传多张"}
                  onFilesSelected={(files) => field.handleChange(files.map((file) => file.name))}
                />
              )}
            </form.AppField>
            <form.AppField name="detailImages">
              {(field) => (
                <FileUpload
                  accept="image/jpeg,image/png,image/webp"
                  multiple
                  label="来源详情图"
                  description={field.state.value.length ? `${field.state.value.length} 张` : "可上传多张"}
                  onFilesSelected={(files) => field.handleChange(files.map((file) => file.name))}
                />
              )}
            </form.AppField>
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

function buildRegisterSupplySchema(requireSalesVisiblePrice: boolean) {
  return z.object({
    supplierId: z.string().min(1, "请选择供应商"),
    supplierSkuCode: z.string().trim().min(1, "请填写供应商 SKU 编码"),
    dropshipFloorPriceGross: z.string(),
    bulkFloorPriceGross: z.string(),
    minimumOrderQuantity: z
      .string()
      .trim()
      .regex(/^\d+(?:\.\d{1,6})?$/, "请输入正确起订量"),
    supplyMode: z
      .array(z.enum(["DROPSHIP", "BULK"]))
      .min(1, "请至少选择一种供给方式"),
    salesVisiblePrice: z.string(),
  }).superRefine((value, context) => {
    const dropship = value.supplyMode.includes("DROPSHIP")
      ? supplyPriceIssue("DROPSHIP", value.dropshipFloorPriceGross, "一件代发")
      : null
    if (dropship) {
      context.addIssue({ code: "custom", path: [dropship.path], message: dropship.message })
    }
    const bulk = value.supplyMode.includes("BULK")
      ? supplyPriceIssue("BULK", value.bulkFloorPriceGross, "集采")
      : null
    if (bulk) {
      context.addIssue({ code: "custom", path: [bulk.path], message: bulk.message })
    }
    if (
      requireSalesVisiblePrice &&
      !money.safeParse(value.salesVisiblePrice.trim()).success
    ) {
      context.addIssue({
        code: "custom",
        path: ["salesVisiblePrice"],
        message: "该公司 SKU 尚未进入商品池，请填写销售可见价（最多 4 位小数）",
      })
    }
  })
}

/**
 * W14/W21 固定公司 SKU 的「添加供应商并登记成本」最小对话框。
 * 只补录供应商侧独有差异：供应商、供应商 SKU 编码、供给方式（多选）与
 * 双底价（一件代发含税运 / 集采含税）、供给起订量；无商品池条目时追加
 * 首次销售可见价。名称/规格/分类/品牌/单位/条码/媒体从公司 SKU 反向
 * 复用为供应商商品快照；采购确认成本取勾选方式对应底价（集采优先）。
 */
export function RegisterSupplyForSkuDialog({
  open,
  onOpenChange,
  fixedSku,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
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
      supplierSkuCode: "",
      dropshipFloorPriceGross: "",
      bulkFloorPriceGross: "",
      minimumOrderQuantity: "1",
      supplyMode: supplyModes(),
      salesVisiblePrice: fixedSku?.hasPoolEntry
        ? ""
        : fixedSku?.salesVisiblePrice ?? "",
    },
    validators: {
      onSubmit: buildRegisterSupplySchema(Boolean(
        fixedSku && !fixedSku.hasPoolEntry,
      )),
    },
    onSubmit: async ({ value }) => {
      if (!fixedSku) return
      const supplier = supplierQuery.data?.rows.find(
        (row) => row.stableId === value.supplierId,
      )
      if (!supplier) return
      const media: Omit<
        import("@/features/supplier-catalog/types").SupplierCatalogMediaView,
        "id"
      >[] = [
        ...(fixedSku.carouselImages ?? []).map((fileName, index) => ({
          usage: "SPU_CAROUSEL" as const,
          fileName,
          sortOrder: index,
          fileAssetId: `asset:${fileName}`,
          archiveStatus: "ARCHIVED" as const,
        })),
        ...(fixedSku.detailImages ?? []).map((fileName, index) => ({
          usage: "SPU_DETAIL" as const,
          fileName,
          sortOrder: index,
          fileAssetId: `asset:${fileName}`,
          archiveStatus: "ARCHIVED" as const,
        })),
        ...(fixedSku.mainImage
          ? [{
              usage: "SKU_MAIN" as const,
              fileName: fixedSku.mainImage,
              sortOrder: 0,
              fileAssetId: `asset:${fixedSku.mainImage}`,
              archiveStatus: "ARCHIVED" as const,
            }]
          : []),
      ]
      const response = await createMutation.mutateAsync({
        sourceType: "MANUAL",
        supplierId: supplier.stableId,
        supplierName: supplier.name,
        supplierSkuCode: value.supplierSkuCode.trim(),
        name: fixedSku.skuName,
        description: fixedSku.description,
        specification: fixedSku.specification,
        category: fixedSku.category ?? "",
        brand: fixedSku.brand,
        sourceBaseUnit: fixedSku.baseUnit,
        barcode: fixedSku.barcode,
        attributes: [],
        media,
        dropshipFloorPriceGross:
          value.dropshipFloorPriceGross.trim() || undefined,
        bulkFloorPriceGross: value.bulkFloorPriceGross.trim() || undefined,
        bulkMinimumOrderQuantity: value.minimumOrderQuantity.trim(),
        confirmedCostGross:
          value.bulkFloorPriceGross.trim() ||
          value.dropshipFloorPriceGross.trim(),
        inputTaxRate: "0.13",
        supplyRegion: ["全国"],
        targetSkuId: fixedSku.skuId,
        targetSkuCode: fixedSku.skuCode,
        targetSkuName: fixedSku.skuName,
        targetSpecification: fixedSku.specification,
        baseUnit: fixedSku.baseUnit,
        salesVisiblePrice: fixedSku.hasPoolEntry
          ? undefined
          : value.salesVisiblePrice.trim() || undefined,
        poolPriceAction: fixedSku.hasPoolEntry
          ? "KEEP_EXISTING"
          : "SET_PRICE",
        minimumOrderQuantity: value.minimumOrderQuantity.trim(),
        supplyMode: value.supplyMode,
        validFrom: todayIso(),
        idempotencyKey: idempotencyKey("register-supply-for-sku"),
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
      <DialogContent className="sm:max-w-xl">
        <DialogHeader>
          <DialogTitle>添加供应商并登记成本</DialogTitle>
          <DialogDescription>
            固定当前公司 SKU；名称、规格、分类、品牌、单位、条码和图文从公司资料复用为供应商商品快照，只补录供应商 SKU、供给方式、双底价（一件代发含税运 / 集采含税）和起订量。
          </DialogDescription>
        </DialogHeader>

        {result ? (
          <Alert>
            <AlertTitle>供应商供给已登记</AlertTitle>
            <AlertDescription>
              业务记录 {result} 已形成；供应商商品、映射和供给已在同一事务中保存。
            </AlertDescription>
          </Alert>
        ) : null}

        {fixedSku ? (
          <div className="rounded-xl border border-border bg-muted/50 px-3 py-2.5">
            <div className="text-xs font-medium text-muted-foreground">
              固定公司 SKU（资料将复用为供应商商品基础）
            </div>
            <div className="mt-1 font-medium text-foreground">
              {fixedSku.skuName}
            </div>
            <div className="mt-0.5 text-xs text-muted-foreground">
              {fixedSku.skuCode} · {fixedSku.specification} · {fixedSku.baseUnit}
              {fixedSku.category ? ` · ${fixedSku.category}` : ""}
              {fixedSku.brand ? ` · ${fixedSku.brand}` : ""}
            </div>
            {fixedSku.hasPoolEntry ? (
              <p className="mt-1 text-xs text-muted-foreground">
                当前销售可见价 ¥{fixedSku.salesVisiblePrice ?? "—"}；本次只新增供应商映射和供给，不形成商品池价格修订。
              </p>
            ) : (
              <p className="mt-1 text-xs text-muted-foreground">
                该公司 SKU 尚未进入商品池，本次需同时设置首次销售可见价。
              </p>
            )}
          </div>
        ) : null}

        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault()
            void form.handleSubmit()
          }}
        >
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
            <form.AppField name="supplierSkuCode">
              {(field) => (
                <field.TextField
                  label="供应商 SKU 编码 *"
                  description="供应商侧对该商品的编码，用于后续映射与对账"
                />
              )}
            </form.AppField>
            <form.AppField name="supplyMode">
              {(field) => (
                <div className="space-y-1.5">
                  <Label>供给方式（可多选）*</Label>
                  <SupplyModeToggle
                    value={field.state.value}
                    onChange={(next) => field.handleChange([...next])}
                  />
                </div>
              )}
            </form.AppField>
            <form.AppField name="dropshipFloorPriceGross">
              {(field) => (
                <field.TextField
                  label="一件代发底价（含税运）"
                  description="勾选「一件代发」时必填；供应商商品资料保留双底价"
                />
              )}
            </form.AppField>
            <form.AppField name="bulkFloorPriceGross">
              {(field) => (
                <field.TextField
                  label="集采底价（含税）"
                  description="勾选「集采」时必填；登记供给时默认用作采购确认成本"
                />
              )}
            </form.AppField>
            <form.AppField name="minimumOrderQuantity">
              {(field) => <field.TextField label="供给起订量 *" />}
            </form.AppField>
            {!fixedSku?.hasPoolEntry ? (
              <form.AppField name="salesVisiblePrice">
                {(field) => (
                  <field.TextField
                    label="销售可见价 *"
                    description={`加入 ${fixedSku?.skuCode ?? ""} 的公司商品池价格；不等于采购成本`}
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
                label="保存供给"
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
  salesVisiblePrice: z.string(),
  poolPriceAction: z.enum(["KEEP_EXISTING", "SET_PRICE"]),
  inputTaxRate: z.string().trim().min(1, "请填写税率"),
  minimumOrderQuantity: z.string().trim().min(1, "请填写起订量"),
  supplyMode: z
    .array(z.enum(["DROPSHIP", "BULK"]))
    .min(1, "请至少选择一种供给方式"),
  supplyRegionText: z.string().trim().min(1, "请填写可供区域"),
  validFrom: z.string().min(1, "请选择生效日期"),
}).superRefine((value, context) => {
  if (value.poolPriceAction !== "SET_PRICE") return
  if (!/^\d+(?:\.\d{1,4})?$/.test(value.salesVisiblePrice.trim())) {
    context.addIssue({
      code: "custom",
      path: ["salesVisiblePrice"],
      message: "请输入正确销售可见价，最多 4 位小数",
    })
  }
})

export function PromoteSupplierProductDialog({
  item,
  open,
  onOpenChange,
  preferredProductId,
}: {
  item?: SupplierCatalogItemView
  open: boolean
  onOpenChange: (open: boolean) => void
  preferredProductId?: string
}) {
  const skuQuery = useCompanySkuOptionsQuery()
  const promoteMutation = usePromoteSupplierProductMutation()
  const [result, setResult] = React.useState<SupplierCatalogWriteResult | null>(null)
  const sourceRevision = item?.supplierProduct.incomingRevision ?? item?.supplierProduct.currentRevision
  const form = useAppForm({
    defaultValues: {
      targetSkuId: "",
      confirmedCostGross:
        sourceRevision?.bulkFloorPriceGross ??
        sourceRevision?.dropshipFloorPriceGross ??
        "",
      salesVisiblePrice: "",
      poolPriceAction: "SET_PRICE" as "KEEP_EXISTING" | "SET_PRICE",
      inputTaxRate: "0.13",
      minimumOrderQuantity:
        item?.offering?.proposedDefaults?.minimumOrderQuantity ??
        sourceRevision?.bulkMinimumOrderQuantity ??
        "1",
      supplyMode: [
        ...(item?.offering?.proposedDefaults?.supplyMode ?? ["BULK"]),
      ],
      supplyRegionText: "全国",
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
        salesVisiblePrice:
          value.poolPriceAction === "SET_PRICE"
            ? value.salesVisiblePrice.trim()
            : undefined,
        poolPriceAction: value.poolPriceAction,
        expectedSourceRevisionNo: sourceRevision?.revisionNo ?? 0,
        expectedPoolEntryRevisionId: sku.poolEntry?.poolEntryRevisionId,
        idempotencyKey: idempotencyKey("promote-supplier-product"),
      })
      setResult(response)
    },
  })

  React.useEffect(() => {
    if (!open) {
      setResult(null)
      return
    }
    if (!item || !skuQuery.data?.length) return
    const preferred = preferredProductId
      ? skuQuery.data.find((sku) => sku.productId === preferredProductId)
      : undefined
    const candidate =
      preferred ??
      skuQuery.data.find((sku) => sku.skuId === item.skuCandidates[0]?.skuId) ??
      skuQuery.data[0]
    if (!candidate) return
    const nextSource =
      item.supplierProduct.incomingRevision ?? item.supplierProduct.currentRevision
    form.reset({
      targetSkuId: candidate.skuId,
      confirmedCostGross:
        nextSource.bulkFloorPriceGross ??
        nextSource.dropshipFloorPriceGross ??
        "",
      salesVisiblePrice: candidate.poolEntry?.salesVisiblePrice ?? "",
      poolPriceAction: candidate.poolEntry ? "KEEP_EXISTING" : "SET_PRICE",
      inputTaxRate: "0.13",
      minimumOrderQuantity:
        item.offering?.proposedDefaults?.minimumOrderQuantity ??
        nextSource.bulkMinimumOrderQuantity ??
        "1",
      supplyMode: [
        ...(item.offering?.proposedDefaults?.supplyMode ?? ["BULK"]),
      ],
      supplyRegionText:
        item.offering?.proposedDefaults?.supplyRegion.join("、") || "全国",
      validFrom: "2026-08-02",
    })
  }, [form, item, open, preferredProductId, skuQuery.data])

  const skuOptions = (skuQuery.data ?? []).map((sku) => {
    const signals = sourceRevision
      ? companySkuMatchSignals(sku, sourceRevision)
      : []
    return {
      value: sku.skuId,
      label: `${sku.skuCode} · ${sku.skuName} · ${sku.specification}${signals.length ? ` · ${signals.join("/")}` : ""}`,
    }
  })

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
            <AlertTitle>
              {result.poolEntryChange === "UNCHANGED"
                ? "第二供应商供给已登记"
                : "已加入公司商品池"}
            </AlertTitle>
            <AlertDescription>
              {result.poolEntryChange === "UNCHANGED"
                ? `已沿用原商品池价格和版本，当前有效供应商 ${result.activeSupplierCount ?? "—"} 家。`
                : `业务记录 ${result.reference} 已形成。`}
            </AlertDescription>
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
                  onValueChange={(value) => {
                    const nextId = value ?? ""
                    field.handleChange(nextId)
                    const selected = skuQuery.data?.find((sku) => sku.skuId === nextId)
                    form.setFieldValue(
                      "poolPriceAction",
                      selected?.poolEntry ? "KEEP_EXISTING" : "SET_PRICE"
                    )
                    form.setFieldValue(
                      "salesVisiblePrice",
                      selected?.poolEntry?.salesVisiblePrice ?? ""
                    )
                  }}
                  options={skuOptions}
                  placeholder="选择已有公司 SKU"
                  className="w-full"
                />
              </div>
            )}
          </form.AppField>
          <form.Subscribe
            selector={(state) => ({
              targetSkuId: state.values.targetSkuId,
              poolPriceAction: state.values.poolPriceAction,
            })}
          >
            {({ targetSkuId, poolPriceAction }) => {
              const selected = skuQuery.data?.find((sku) => sku.skuId === targetSkuId)
              const signals =
                selected && sourceRevision
                  ? companySkuMatchSignals(selected, sourceRevision)
                  : []
              return (
                <div className="space-y-3">
                  <p className="text-xs text-muted-foreground">
                    匹配依据：{signals.length ? signals.join("、") : "暂无强匹配信号，请人工核对品牌、规格和包装单位；不能仅按名称合并。"}
                  </p>
                  {selected?.poolEntry ? (
                    <Alert>
                      <AlertTitle>
                        该公司 SKU 已有 {selected.activeSupplierCount ?? 0} 家有效供应商
                      </AlertTitle>
                      <AlertDescription>
                        当前销售可见价 ¥{selected.poolEntry.salesVisiblePrice}。默认只增加本供应商的映射和供给，商品池条目保持唯一。
                      </AlertDescription>
                    </Alert>
                  ) : (
                    <Alert>
                      <AlertTitle>该公司 SKU 尚未进入商品池</AlertTitle>
                      <AlertDescription>
                        本次必须同时设置首次销售可见价，成功后形成唯一商品池条目。
                      </AlertDescription>
                    </Alert>
                  )}
                  {selected?.poolEntry ? (
                    <form.AppField name="poolPriceAction">
                      {(field) => (
                        <div className="space-y-1.5">
                          <Label>商品池价格处理 *</Label>
                          <OptionCombobox
                            value={field.state.value}
                            onValueChange={(value) =>
                              field.handleChange(
                                (value ?? "KEEP_EXISTING") as "KEEP_EXISTING" | "SET_PRICE"
                              )
                            }
                            options={[
                              { value: "KEEP_EXISTING", label: "沿用现有价格（推荐）" },
                              { value: "SET_PRICE", label: "同时修改销售可见价" },
                            ]}
                            allowClear={false}
                            className="w-full"
                          />
                        </div>
                      )}
                    </form.AppField>
                  ) : null}
                  {poolPriceAction === "SET_PRICE" ? (
                    <form.AppField name="salesVisiblePrice">
                      {(field) => (
                        <field.TextField
                          label="销售可见价 *"
                          description="只写公司商品池，不等于任何供应商成本"
                        />
                      )}
                    </form.AppField>
                  ) : null}
                </div>
              )
            }}
          </form.Subscribe>
          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField name="confirmedCostGross">
              {(field) => (
                <field.TextField
                  label="采购确认含税成本 *"
                  description="采购私密字段；销售查询和导出不返回原值"
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
                  <Label>供给方式（可多选）*</Label>
                  <SupplyModeToggle
                    value={field.state.value}
                    onChange={(next) => field.handleChange([...next])}
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
