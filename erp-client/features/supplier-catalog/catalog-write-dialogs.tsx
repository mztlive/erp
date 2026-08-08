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
  useCreateSupplierCatalogItemMutation,
  useImportSupplierCatalogExcelMutation,
} from "@/features/supplier-catalog/queries"
import {
  uploadCatalogExcel,
  uploadCatalogImage,
} from "@/features/supplier-catalog/api"
import {
  parseSupplierCatalogExcel,
  type SupplierCatalogExcelImportResult,
  type SupplierCatalogExcelPreview,
} from "@/features/supplier-catalog/excel-import"
import type { ProductKind } from "@/features/master-data/types"
import type {
  SupplierCatalogSourceType,
  SupplierCatalogWriteResult,
} from "@/features/supplier-catalog/types"
import { useMasterDataListQuery } from "@/features/master-data/queries"

export { PromoteSupplierProductDialog } from "@/features/supplier-catalog/pool-entry-dialog"

/**
 * W14 固定 SKU 的「添加供应商并登记成本」上下文。
 * 公司侧资料（名称/规格/分类/品牌/条码/媒体）在登记供给时反向复用
 * 为供应商商品的基础快照，对话框只补录供应商独有差异。
 */
type FixedSku = Readonly<{
  skuId: string
  skuCode: string
  skuName: string
  productKind: ProductKind
  specification: string
  baseUnit: string
  category?: string
  brand?: string
  barcode?: string
  description?: string
  carouselImages?: readonly string[]
  detailImages?: readonly string[]
  mainImage?: string
  /** 轮播图 fileName → 已登记文件资产 id（随媒体快照复用）。 */
  carouselFileAssetIds?: Readonly<Record<string, string>>
  detailFileAssetIds?: Readonly<Record<string, string>>
  /** 轮播图 fileName → 可访问 URL。 */
  carouselPreviewUrls?: Readonly<Record<string, string>>
  detailPreviewUrls?: Readonly<Record<string, string>>
  /** SKU 主图已登记文件资产。 */
  mainImageAssetId?: string
  /** SKU 主图可访问 URL。 */
  mainImagePreviewUrl?: string
}>

const money = z
  .string()
  .trim()
  .regex(/^\d+(?:\.\d{1,4})?$/, "请输入正确金额，最多 4 位小数")

/** 金额字段允许为空；不再按供给方式联动必填。 */
function todayIso(): string {
  return new Date().toISOString().slice(0, 10)
}

function buildIntakeSchema() {
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
    inputTaxRate: z.string(),
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

function errorMessage(error: unknown, fallback: string) {
  return error && typeof error === "object" && "message" in error
    ? String(error.message)
    : fallback
}

export function SupplierCatalogIntakeDialog({
  open,
  onOpenChange,
  sourceType,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  sourceType: Exclude<SupplierCatalogSourceType, "API">
}) {
  const supplierQuery = useMasterDataListQuery({
    resource: "suppliers",
    lifecycleStatus: "enabled",
    revisionTiming: "current",
  })
  const createMutation = useCreateSupplierCatalogItemMutation()
  const excelImportMutation = useImportSupplierCatalogExcelMutation()
  const [result, setResult] = React.useState<SupplierCatalogWriteResult | null>(
    null
  )
  const [excelPreview, setExcelPreview] =
    React.useState<SupplierCatalogExcelPreview | null>(null)
  const [excelFileAssetId, setExcelFileAssetId] = React.useState("")
  const [excelSourceReference, setExcelSourceReference] = React.useState("")
  const [excelCommandKey, setExcelCommandKey] = React.useState("")
  const [excelResult, setExcelResult] =
    React.useState<SupplierCatalogExcelImportResult | null>(null)
  const [excelError, setExcelError] = React.useState<string | null>(null)
  const [excelPreparing, setExcelPreparing] = React.useState(false)
  const [uploadedSourceMedia, setUploadedSourceMedia] = React.useState<
    Record<string, { fileAssetId: string; url: string }>
  >({})
  const [sourceMediaUploading, setSourceMediaUploading] = React.useState(false)
  const [sourceMediaError, setSourceMediaError] = React.useState<string | null>(
    null
  )
  const form = useAppForm({
    defaultValues: {
      supplierId: "",
      sourceReference: "",
      supplierSpuCode: "",
      supplierSkuCode: "",
      name: "",
      description: "",
      specification: "",
      category: "",
      brand: "",
      sourceBaseUnit: "",
      barcode: "",
      attributeText: "",
      carouselImages: [] as string[],
      detailImages: [] as string[],
      skuMainImage: "",
      dropshipFloorPriceGross: "",
      bulkFloorPriceGross: "",
      bulkMinimumOrderQuantity: "1",
      minimumOrderQuantity: "1",
      inputTaxRate: "",
    },
    validators: {
      onSubmit: buildIntakeSchema(),
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
            fileAssetId:
              uploadedSourceMedia[`SPU_CAROUSEL:${fileName}`]?.fileAssetId,
            sourceUrl: uploadedSourceMedia[`SPU_CAROUSEL:${fileName}`]?.url,
            archiveStatus: "ARCHIVED" as const,
          })),
          ...value.detailImages.map((fileName, index) => ({
            usage: "SPU_DETAIL" as const,
            fileName,
            sortOrder: index,
            fileAssetId:
              uploadedSourceMedia[`SPU_DETAIL:${fileName}`]?.fileAssetId,
            sourceUrl: uploadedSourceMedia[`SPU_DETAIL:${fileName}`]?.url,
            archiveStatus: "ARCHIVED" as const,
          })),
          ...(value.skuMainImage
            ? [{
                usage: "SKU_MAIN" as const,
                fileName: value.skuMainImage,
                sortOrder: 0,
                fileAssetId:
                  uploadedSourceMedia[`SKU_MAIN:${value.skuMainImage}`]
                    ?.fileAssetId,
                sourceUrl:
                  uploadedSourceMedia[`SKU_MAIN:${value.skuMainImage}`]?.url,
                archiveStatus: "ARCHIVED" as const,
              }]
            : []),
        ],
        dropshipFloorPriceGross: value.dropshipFloorPriceGross.trim() || undefined,
        bulkFloorPriceGross: value.bulkFloorPriceGross.trim() || undefined,
        bulkMinimumOrderQuantity: value.bulkMinimumOrderQuantity.trim(),
        inputTaxRate: value.inputTaxRate.trim(),
        sourceReference: value.sourceReference.trim() || undefined,
        minimumOrderQuantity: value.minimumOrderQuantity.trim(),
        validFrom: todayIso(),
        idempotencyKey: idempotencyKey("supplier-catalog-intake"),
      })
      setResult(response)
    },
  })

  const prepareExcel = async (file: File) => {
    setExcelPreparing(true)
    setExcelError(null)
    setExcelPreview(null)
    setExcelFileAssetId("")
    setExcelSourceReference("")
    setExcelResult(null)
    try {
      const preview = await parseSupplierCatalogExcel(file)
      const asset = await uploadCatalogExcel(file)
      setExcelPreview(preview)
      setExcelFileAssetId(asset.fileAssetId)
      setExcelSourceReference(asset.fileAssetId)
      setExcelCommandKey(idempotencyKey("supplier-catalog-excel"))
    } catch (error) {
      setExcelError(errorMessage(error, "Excel 解析或文件登记失败"))
    } finally {
      setExcelPreparing(false)
    }
  }

  const uploadSourceFiles = async (
    usage: "SKU_MAIN" | "SPU_CAROUSEL" | "SPU_DETAIL",
    files: File[]
  ) => {
    setSourceMediaUploading(true)
    setSourceMediaError(null)
    try {
      const uploaded = await Promise.all(
        files.map(async (file) => ({
          file,
          asset: await uploadCatalogImage(file),
        }))
      )
      setUploadedSourceMedia((current) => {
        const next = { ...current }
        for (const { file, asset } of uploaded) {
          next[`${usage}:${file.name}`] = asset
        }
        return next
      })
      return uploaded.map(({ file }) => file.name)
    } catch (error) {
      setSourceMediaError(errorMessage(error, "来源图片上传失败"))
      return null
    } finally {
      setSourceMediaUploading(false)
    }
  }

  const submitExcel = async () => {
    const supplierId = form.state.values.supplierId
    if (!supplierId) {
      setExcelError("请选择供应商")
      return
    }
    if (!excelPreview || !excelFileAssetId || !excelSourceReference) {
      setExcelError("请先选择并完成预检的 Excel 文件")
      return
    }
    setExcelError(null)
    try {
      const response = await excelImportMutation.mutateAsync({
        supplierId,
        fileAssetId: excelFileAssetId,
        sourceReference: excelSourceReference,
        preview: excelPreview,
        idempotencyKey:
          excelCommandKey || idempotencyKey("supplier-catalog-excel"),
      })
      setExcelResult(response)
    } catch (error) {
      setExcelError(errorMessage(error, "Excel 批次导入失败"))
    }
  }

  React.useEffect(() => {
    if (!open) {
      setResult(null)
      setExcelPreview(null)
      setExcelFileAssetId("")
      setExcelSourceReference("")
      setExcelCommandKey("")
      setExcelResult(null)
      setExcelError(null)
      setExcelPreparing(false)
      setUploadedSourceMedia({})
      setSourceMediaUploading(false)
      setSourceMediaError(null)
    }
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
            {sourceType === "EXCEL"
              ? "按 Excel 模板录入供应商商品"
              : "手工录入供应商商品"}
          </DialogTitle>
          <DialogDescription>
            三种来源共用同一套供应商商品资料；采购成本只对采购授权角色可见，销售只看到公司商品池价格。
          </DialogDescription>
        </DialogHeader>

        {excelResult ? (
          <Alert>
            <AlertTitle>Excel 批次已导入</AlertTitle>
            <AlertDescription>
              业务记录 {excelResult.reference}：成功 {excelResult.importedCount} 行，错误清单 {excelResult.rejectedCount} 行。
            </AlertDescription>
          </Alert>
        ) : result ? (
          <Alert>
            <AlertTitle>供应商商品已保存</AlertTitle>
            <AlertDescription>业务记录 {result.reference} 已形成。</AlertDescription>
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="mt-2"
              render={
                <Link
                  href={`/procurement/supplier-catalog/${result.supplierProductId}`}
                />
              }
            >
              查看详情
            </Button>
          </Alert>
        ) : null}

        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault()
            if (sourceType === "EXCEL") {
              void submitExcel()
            } else {
              void form.handleSubmit()
            }
          }}
        >
          {sourceType === "EXCEL" ? (
            <FileUpload
              accept=".xlsx,.csv"
              multiple={false}
              disabled={excelPreparing || excelImportMutation.isPending}
              label="供应商商品表（模板）"
              description={
                excelPreparing
                  ? "正在解析并登记原始文件…"
                  : excelPreview
                    ? `${excelPreview.fileName} · ${excelPreview.totalRows} 行已预检`
                    : "支持 xlsx、csv；必填列：供应商 SPU 编码、供应商 SKU 编码、商品名称、规格"
              }
              onFilesSelected={(files) => {
                if (files[0]) void prepareExcel(files[0])
              }}
            />
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
            {sourceType !== "EXCEL" ? (
              <>
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
                  description="供应商商品资料保留双底价"
                />
              )}
            </form.AppField>
            <form.AppField name="bulkFloorPriceGross">
              {(field) => (
                <field.TextField
                  label="集采底价（含税）"
                  description={
                    "供应商商品资料保留双底价"
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
                  label="最小起订量 *"
                />
              )}
            </form.AppField>
            <form.AppField name="inputTaxRate">
              {(field) => (
                <field.TextField
                  label="进项税率"
                  description="无可靠来源时可留空；提交时会要求补充来源，建议先向供应商确认"
                />
              )}
            </form.AppField>
              </>
            ) : null}
          </div>

          {sourceType === "EXCEL" && excelPreview ? (
            <div className="space-y-3 rounded-lg border p-4">
              <div>
                <p className="font-medium">批次预检</p>
                <p className="text-sm text-muted-foreground">
                  共 {excelPreview.totalRows} 行；可导入{" "}
                  {excelPreview.products.reduce(
                    (count, product) => count + product.skus.length,
                    0
                  )}{" "}
                  行 / {excelPreview.products.length} 个 SPU；错误{" "}
                  {excelPreview.rejectedRows.length} 行。确认后合法行与错误清单一起形成正式批次。
                </p>
              </div>
              {excelPreview.rejectedRows.length > 0 ? (
                <div className="max-h-40 overflow-y-auto rounded-md bg-muted p-3 text-sm">
                  {excelPreview.rejectedRows.slice(0, 20).map((row) => (
                    <p key={row.rowNo}>
                      第 {row.rowNo} 行{row.supplierSkuCode ? ` · ${row.supplierSkuCode}` : ""}：{row.errorText}
                    </p>
                  ))}
                  {excelPreview.rejectedRows.length > 20 ? (
                    <p>另有 {excelPreview.rejectedRows.length - 20} 条错误随批次保存。</p>
                  ) : null}
                </div>
              ) : null}
            </div>
          ) : null}

          {excelError ? (
            <Alert variant="destructive">
              <AlertTitle>无法导入</AlertTitle>
              <AlertDescription>{excelError}</AlertDescription>
            </Alert>
          ) : null}

          {sourceType !== "EXCEL" ? (
            <div className="grid gap-4 sm:grid-cols-3">
            <form.AppField name="skuMainImage">
              {(field) => (
                <FileUpload
                  accept="image/jpeg,image/png,image/webp"
                  multiple={false}
                  disabled={sourceMediaUploading}
                  label="来源 SKU 主图"
                  description={field.state.value || "可空；首次建品时若仍缺主图，必须补齐后才能保存"}
                  onFilesSelected={(files) => {
                    void uploadSourceFiles("SKU_MAIN", files.slice(0, 1)).then(
                      (names) => {
                        if (names) field.handleChange(names[0] ?? "")
                      }
                    )
                  }}
                />
              )}
            </form.AppField>
            <form.AppField name="carouselImages">
              {(field) => (
                <FileUpload
                  accept="image/jpeg,image/png,image/webp"
                  multiple
                  disabled={sourceMediaUploading}
                  label="来源轮播图"
                  description={field.state.value.length ? `${field.state.value.length} 张` : "可上传多张"}
                  onFilesSelected={(files) => {
                    void uploadSourceFiles("SPU_CAROUSEL", files).then(
                      (names) => {
                        if (names) field.handleChange(names)
                      }
                    )
                  }}
                />
              )}
            </form.AppField>
            <form.AppField name="detailImages">
              {(field) => (
                <FileUpload
                  accept="image/jpeg,image/png,image/webp"
                  multiple
                  disabled={sourceMediaUploading}
                  label="来源详情图"
                  description={field.state.value.length ? `${field.state.value.length} 张` : "可上传多张"}
                  onFilesSelected={(files) => {
                    void uploadSourceFiles("SPU_DETAIL", files).then((names) => {
                      if (names) field.handleChange(names)
                    })
                  }}
                />
              )}
            </form.AppField>
            </div>
          ) : null}

          {sourceMediaError ? (
            <Alert variant="destructive">
              <AlertTitle>图片未保存</AlertTitle>
              <AlertDescription>{sourceMediaError}</AlertDescription>
            </Alert>
          ) : null}

          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              关闭
            </DialogClose>
            {sourceType === "EXCEL" ? (
              <Button
                type="submit"
                disabled={
                  excelPreparing ||
                  excelImportMutation.isPending ||
                  !excelPreview ||
                  !excelFileAssetId ||
                  Boolean(excelResult)
                }
              >
                {excelImportMutation.isPending ? "正在导入…" : "确认导入批次"}
              </Button>
            ) : (
              <form.AppForm>
                <form.SubmitButton
                  label="保存到供应商商品库"
                  disabled={
                    createMutation.isPending ||
                    sourceMediaUploading ||
                    Boolean(result)
                  }
                />
              </form.AppForm>
            )}
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

function buildRegisterSupplySchema() {
  return z.object({
    supplierId: z.string().min(1, "请选择供应商"),
    supplierSkuCode: z.string().trim().min(1, "请填写供应商 SKU 编码"),
    dropshipFloorPriceGross: money,
    bulkFloorPriceGross: money,
    minimumOrderQuantity: z
      .string()
      .trim()
      .regex(/^\d+(?:\.\d{1,6})?$/, "请输入正确起订量"),
    inputTaxRate: z
      .string()
      .trim()
      .regex(/^\d{1,3}$/, "请输入 0–100 的整数税率")
      .refine((value) => Number(value) <= 100, "税率不能超过 100"),
    supplyRegionText: z.string().trim().min(1, "请填写可供区域"),
  })
}

/**
 * W14/W21 固定公司 SKU 的「添加供应商并登记成本」最小对话框。
 * 只补录供应商侧独有差异：供应商、供应商 SKU 编码与
 * 双价供给（一件代发含税运 / 集采含税）、供给起订量、税率和区域。
 * 名称/商品类型/规格/分类/品牌/单位/条码/媒体从公司 SKU 正向复用为
 * 供应商商品快照；本入口不修改公司 SKU 销售可见价。
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
  const [result, setResult] = React.useState<SupplierCatalogWriteResult | null>(
    null
  )
  const form = useAppForm({
    defaultValues: {
      supplierId: "",
      supplierSkuCode: "",
      dropshipFloorPriceGross: "",
      bulkFloorPriceGross: "",
      minimumOrderQuantity: "1",
      inputTaxRate: "",
      supplyRegionText: "",
    },
    validators: {
      onSubmit: buildRegisterSupplySchema(),
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
          fileAssetId: fixedSku.carouselFileAssetIds?.[fileName],
          sourceUrl: fixedSku.carouselPreviewUrls?.[fileName],
          archiveStatus: "ARCHIVED" as const,
        })),
        ...(fixedSku.detailImages ?? []).map((fileName, index) => ({
          usage: "SPU_DETAIL" as const,
          fileName,
          sortOrder: index,
          fileAssetId: fixedSku.detailFileAssetIds?.[fileName],
          sourceUrl: fixedSku.detailPreviewUrls?.[fileName],
          archiveStatus: "ARCHIVED" as const,
        })),
        ...(fixedSku.mainImage
          ? [{
              usage: "SKU_MAIN" as const,
              fileName: fixedSku.mainImage,
              sortOrder: 0,
              fileAssetId: fixedSku.mainImageAssetId,
              sourceUrl: fixedSku.mainImagePreviewUrl,
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
        sourceProductKind: fixedSku.productKind,
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
        inputTaxRate: (Number(value.inputTaxRate.trim()) / 100).toFixed(2),
        supplyRegion: splitValues(value.supplyRegionText),
        targetSkuId: fixedSku.skuId,
        targetSkuCode: fixedSku.skuCode,
        targetSkuName: fixedSku.skuName,
        targetSpecification: fixedSku.specification,
        baseUnit: fixedSku.baseUnit,
        minimumOrderQuantity: value.minimumOrderQuantity.trim(),
        validFrom: "",
        idempotencyKey: idempotencyKey("register-supply-for-sku"),
      })
      setResult(response)
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
            固定当前公司 SKU；名称、规格、分类、品牌、单位、条码和图文从公司资料复用为供应商商品基础，只补录供应商 SKU、双底价（一件代发含税运 / 集采含税）和起订量。
          </DialogDescription>
        </DialogHeader>

        {result ? (
          <Alert>
            <AlertTitle>供应商供给已登记</AlertTitle>
            <AlertDescription>
              业务记录 {result.reference} 已形成；供应商商品、映射和供给已在同一次保存中完成。
            </AlertDescription>
            <Button
              type="button"
              size="sm"
              variant="outline"
              className="mt-2"
              render={
                <Link
                  href={`/procurement/supplier-catalog/${result.supplierProductId}`}
                />
              }
            >
              查看详情
            </Button>
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
            <p className="mt-1 text-xs text-muted-foreground">
              本次只新增供应商映射和供给，不形成公司 SKU 价格修订。
            </p>
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
            <form.AppField name="dropshipFloorPriceGross">
              {(field) => (
                <field.TextField
                  label="一件代发供给价（含税运）*"
                  description="同时保存为供应商目录底价和首版正式供给价"
                />
              )}
            </form.AppField>
            <form.AppField name="bulkFloorPriceGross">
              {(field) => (
                <field.TextField
                  label="集采供给价（含税）*"
                  description="同时保存为供应商目录底价和首版正式供给价"
                />
              )}
            </form.AppField>
            <form.AppField name="minimumOrderQuantity">
              {(field) => (
                <field.TextField
                  label="集采起订量 *"
                  description="登记供给时同时作为该供应商的供给起订量"
                />
              )}
            </form.AppField>
            <form.AppField name="inputTaxRate">
              {(field) => (
                <field.TextField
                  label="进项税率（%）*"
                  description="填写 0–100 的整数，例如 13"
                />
              )}
            </form.AppField>
            <form.AppField name="supplyRegionText">
              {(field) => (
                <field.TextField
                  label="可供区域 *"
                  description="多个区域使用逗号分隔；无可靠来源时必须由采购确认"
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

export type { FixedSku }
