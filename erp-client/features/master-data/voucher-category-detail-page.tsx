"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { useSelector } from "@tanstack/react-form"
import { z } from "zod"

import {
  BrandCombobox,
  CategoryCombobox,
  DiscardConfirmDialog,
  DocumentSection,
  FormalActionResult,
  PageHeader,
  PageScaffold,
} from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { masterDataCopy } from "@/features/master-data/copy"
import { defaultImmediateEffectiveFrom } from "@/features/master-data/resource-fields"
import { toBrandComboboxItems, toCategoryComboboxItems } from "@/features/master-data/category-tree-model"
import { useCreateMasterDataMutation, useMasterDataListQuery } from "@/features/master-data/queries"
import { PRODUCT_KIND_LABELS } from "@/features/master-data/types"
import type { MasterDataMutationResult, VoucherCategoryFields } from "@/features/master-data/types"
import { useUnitOptionsQuery } from "@/hooks/use-options"

/**
 * 卡券类目创建页：业务上一个卡券类目即一个 VOUCHER 类型的 SKU，
 * 原子创建 Product + SKU + 卡券类目扩展修订，不再要求预先建好 SKU 再来关联。
 * 卡券类目暂无更新/停用接口，本页只承担创建；查看走通用对象中心（object-chrome）。
 */
function newIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

const createSchema = z
  .object({
    voucherNo: z.string().trim().min(1, "请填写卡券类目编号"),
    name: z.string().trim().min(2, "请填写卡券类目名称"),
    description: z.string().trim().min(1, "请填写卡券类目描述"),
    specification: z.string(),
    categoryMode: z.enum(["existing", "new"]),
    categoryId: z.string(),
    newCategoryCode: z.string(),
    newCategoryName: z.string(),
    newCategoryParentId: z.string(),
    brandId: z.string().min(1, "请选择品牌"),
    baseUnitId: z.string().min(1, "请选择基础单位"),
    barcode: z.string(),
    salesVisiblePriceGross: z.string(),
    marketPrice: z.string(),
    effectiveFrom: z.string().min(1, "请选择生效日期"),
  })
  .refine(
    (value) =>
      value.categoryMode === "existing"
        ? value.categoryId.trim().length > 0
        : value.newCategoryCode.trim().length > 0 &&
          value.newCategoryName.trim().length > 0,
    {
      message: "请选择已有分类，或完整填写新建分类的代码与名称",
      path: ["categoryId"],
    }
  )

type VoucherCategoryFormValues = {
  voucherNo: string
  name: string
  description: string
  specification: string
  categoryMode: "existing" | "new"
  categoryId: string
  newCategoryCode: string
  newCategoryName: string
  newCategoryParentId: string
  brandId: string
  baseUnitId: string
  barcode: string
  salesVisiblePriceGross: string
  marketPrice: string
  effectiveFrom: string
}

const CATEGORY_MODE_OPTIONS = [
  { value: "existing", label: "选择已有分类" },
  { value: "new", label: "新建分类" },
] as const

function defaultFormValues(): VoucherCategoryFormValues {
  return {
    voucherNo: "",
    name: "",
    description: "",
    specification: "",
    categoryMode: "existing",
    categoryId: "",
    newCategoryCode: "",
    newCategoryName: "",
    newCategoryParentId: "",
    brandId: "",
    baseUnitId: "",
    barcode: "",
    salesVisiblePriceGross: "",
    marketPrice: "",
    effectiveFrom: defaultImmediateEffectiveFrom(),
  }
}

export function VoucherCategoryDetailPage() {
  const router = useRouter()
  const createMutation = useCreateMasterDataMutation()
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey("create-voucher-category")
  )
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null
  )
  const [discardOpen, setDiscardOpen] = React.useState(false)

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
  const unitOptionsQuery = useUnitOptionsQuery()

  const voucherCategoryRows = React.useMemo(
    () =>
      (categoryListQuery.data?.rows ?? []).filter(
        (row) => row.productKind === PRODUCT_KIND_LABELS.VOUCHER
      ),
    [categoryListQuery.data?.rows]
  )
  const categoryOptions = React.useMemo(
    () => toCategoryComboboxItems(voucherCategoryRows),
    [voucherCategoryRows]
  )
  const brandOptions = React.useMemo(
    () => toBrandComboboxItems(brandListQuery.data?.rows ?? []),
    [brandListQuery.data?.rows]
  )

  const form = useAppForm({
    defaultValues: defaultFormValues(),
    validators: { onChange: createSchema },
    onSubmit: async ({ value }) => {
      const unit = unitOptionsQuery.data?.find(
        (item) => item.id === value.baseUnitId
      )
      const fields: VoucherCategoryFields = {
        voucherNo: value.voucherNo.trim(),
        description: value.description.trim(),
        specification: value.specification.trim() || undefined,
        categoryId: value.categoryMode === "existing" ? value.categoryId : "",
        category:
          value.categoryMode === "existing"
            ? (categoryOptions.find((c) => c.categoryId === value.categoryId)
                ?.categoryName ?? "")
            : "",
        newCategoryCode:
          value.categoryMode === "new"
            ? value.newCategoryCode.trim()
            : undefined,
        newCategoryName:
          value.categoryMode === "new"
            ? value.newCategoryName.trim()
            : undefined,
        newCategoryParentId:
          value.categoryMode === "new"
            ? value.newCategoryParentId || undefined
            : undefined,
        brandId: value.brandId,
        brand:
          brandOptions.find((b) => b.brandId === value.brandId)?.brandName ??
          "",
        baseUnitId: value.baseUnitId,
        baseUnitCode: unit?.code ?? "",
        baseUnit: unit?.label ?? "",
        barcode: value.barcode.trim() || undefined,
        salesVisiblePriceGross: value.salesVisiblePriceGross.trim() || undefined,
        marketPrice: value.marketPrice.trim() || undefined,
      }
      const response = await createMutation.mutateAsync({
        resource: "voucher-categories",
        name: value.name.trim(),
        effectiveFrom: value.effectiveFrom,
        changeReason: "新建",
        fields,
        idempotencyKey,
      })
      setResult(response)
      if (response.outcome === "succeeded") {
        router.replace(
          `/master-data/voucher-categories/${response.stableId}?section=overview`
        )
      } else {
        // 非成功结果允许重新提交；换新幂等键，避免复用上一次失败请求的键。
        setIdempotencyKey(newIdempotencyKey("create-voucher-category"))
      }
    },
  })

  const dirty = useSelector(form.store, (state) => state.isDirty)
  const categoryMode = useSelector(
    form.store,
    (state) => state.values.categoryMode
  )

  const navigateAway = (href: string) => {
    if (dirty && result?.outcome !== "succeeded") {
      setDiscardOpen(true)
      return
    }
    router.push(href)
  }

  return (
    <PageScaffold>
      <PageHeader
        title="新建卡券类目"
        description="业务上一个卡券类目即一个卡券 SKU：编号只需填一次，分类可现选现建"
        breadcrumbs={[
          { id: "root", label: "基础资料", href: "/master-data" },
          {
            id: "resource",
            label: "卡券类目",
            href: "/master-data/voucher-categories",
          },
          { id: "new", label: "新建", current: true },
        ]}
        actions={
          <Button
            type="button"
            size="sm"
            variant="outline"
            onClick={() => navigateAway("/master-data/voucher-categories")}
          >
            返回列表
          </Button>
        }
      />

      <form
        className="space-y-4"
        onSubmit={(e) => {
          e.preventDefault()
          void form.handleSubmit()
        }}
      >
        <DocumentSection
          title="卡券类目身份"
          description="编号同时作为商品编号与 SKU 编号，创建后不可修改"
        >
          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField
              name="voucherNo"
              children={(field) => (
                <field.TextField label="卡券类目编号" placeholder="全局唯一" />
              )}
            />
            <form.AppField
              name="name"
              children={(field) => <field.TextField label="卡券类目名称" />}
            />
            <div className="sm:col-span-2">
              <form.AppField
                name="description"
                children={(field) => (
                  <field.TextareaField label={masterDataCopy.fDescription} />
                )}
              />
            </div>
            <div className="sm:col-span-2">
              <form.AppField
                name="specification"
                children={(field) => (
                  <field.TextareaField label="规格或服务内容" placeholder="可选" />
                )}
              />
            </div>
            <form.AppField
              name="effectiveFrom"
              children={(field) => <field.DateField label="生效开始日" />}
            />
          </div>
        </DocumentSection>

        <DocumentSection
          title="分类"
          description="引用已有的卡券类目分类，或在此顺带新建"
        >
          <div className="space-y-4">
            <form.AppField
              name="categoryMode"
              children={(field) => (
                <field.SelectField
                  label="分类来源"
                  options={CATEGORY_MODE_OPTIONS}
                  allowClear={false}
                  onValueChange={(next) => {
                    if (next === "existing") {
                      form.setFieldValue("newCategoryCode", "")
                      form.setFieldValue("newCategoryName", "")
                      form.setFieldValue("newCategoryParentId", "")
                    } else {
                      form.setFieldValue("categoryId", "")
                    }
                  }}
                />
              )}
            />
            {categoryMode === "existing" ? (
              <form.AppField
                name="categoryId"
                children={(field) => {
                  const isInvalid =
                    field.state.meta.isTouched && !field.state.meta.isValid
                  return (
                    <Field data-invalid={isInvalid || undefined}>
                      <FieldLabel htmlFor="voucher-category-id">
                        {masterDataCopy.fCategory}
                      </FieldLabel>
                      <CategoryCombobox
                        categories={categoryOptions}
                        value={field.state.value || undefined}
                        onValueChange={(id) => field.handleChange(id ?? "")}
                        loading={categoryListQuery.isPending}
                        placeholder="请选择卡券类目分类"
                        emptyLabel="暂无可用的 VOUCHER 类型分类，请切换到「新建分类」"
                        className="w-full"
                      />
                      {isInvalid ? (
                        <FieldError errors={toFieldErrors(field.state.meta.errors)} />
                      ) : null}
                    </Field>
                  )
                }}
              />
            ) : (
              <div className="grid gap-4 sm:grid-cols-2">
                <form.AppField
                  name="newCategoryCode"
                  children={(field) => (
                    <field.TextField label="新分类代码" placeholder="唯一" />
                  )}
                />
                <form.AppField
                  name="newCategoryName"
                  children={(field) => <field.TextField label="新分类名称" />}
                />
                <div className="sm:col-span-2">
                  <form.AppField
                    name="newCategoryParentId"
                    children={(field) => (
                      <Field>
                        <FieldLabel htmlFor="voucher-new-category-parent">
                          上级分类
                        </FieldLabel>
                        <CategoryCombobox
                          categories={categoryOptions}
                          value={field.state.value || undefined}
                          onValueChange={(id) => field.handleChange(id ?? "")}
                          loading={categoryListQuery.isPending}
                          placeholder="无上级（可选）"
                          emptyLabel="暂无可选上级分类"
                          className="w-full"
                        />
                      </Field>
                    )}
                  />
                </div>
              </div>
            )}
          </div>
        </DocumentSection>

        <DocumentSection title="品牌与 SKU 属性">
          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField
              name="brandId"
              children={(field) => {
                const isInvalid =
                  field.state.meta.isTouched && !field.state.meta.isValid
                return (
                  <Field data-invalid={isInvalid || undefined}>
                    <FieldLabel htmlFor="voucher-brand-id">
                      {masterDataCopy.fBrand}
                    </FieldLabel>
                    <BrandCombobox
                      brands={brandOptions}
                      value={field.state.value || undefined}
                      onValueChange={(id) => field.handleChange(id ?? "")}
                      loading={brandListQuery.isPending}
                      placeholder="请选择品牌"
                      emptyLabel="暂无可用品牌，请先在品牌中维护"
                      className="w-full"
                    />
                    {isInvalid ? (
                      <FieldError errors={toFieldErrors(field.state.meta.errors)} />
                    ) : null}
                  </Field>
                )
              }}
            />
            <form.AppField
              name="baseUnitId"
              children={(field) => (
                <field.SelectField
                  label={masterDataCopy.fBaseUnit}
                  options={(unitOptionsQuery.data ?? []).map((unit) => ({
                    value: unit.id,
                    label: `${unit.label} · ${unit.code}`,
                  }))}
                  allowClear={false}
                  placeholder="请选择基础单位"
                />
              )}
            />
            <form.AppField
              name="barcode"
              children={(field) => (
                <field.TextField label="条码" placeholder="可选" />
              )}
            />
            <form.AppField
              name="salesVisiblePriceGross"
              children={(field) => (
                <field.TextField label="销售可见含税价" placeholder="可选" />
              )}
            />
            <form.AppField
              name="marketPrice"
              children={(field) => (
                <field.TextField label="市场参考价" placeholder="可选" />
              )}
            />
          </div>
        </DocumentSection>

        {result?.outcome === "succeeded" ? (
          <FormalActionResult
            status="succeeded"
            title={masterDataCopy.createSuccessTitle}
            description={masterDataCopy.createSuccessDesc}
            reference={result.reference}
            facts={[
              { label: masterDataCopy.resultNo, value: result.stableNo },
              { label: masterDataCopy.resultVersion, value: `v${result.revisionNo}` },
            ]}
          />
        ) : null}
        {result?.outcome === "blocked" ? (
          <FormalActionResult
            status="blocked"
            title="暂不能创建"
            description={result.message}
          />
        ) : null}

        <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
          <Button
            type="button"
            variant="outline"
            onClick={() => navigateAway("/master-data/voucher-categories")}
          >
            取消
          </Button>
          <form.AppForm>
            <form.SubmitButton
              label={masterDataCopy.createSubmit}
              disabled={createMutation.isPending}
            />
          </form.AppForm>
        </div>
      </form>

      <DiscardConfirmDialog
        open={discardOpen}
        onOpenChange={setDiscardOpen}
        onConfirm={() => {
          setDiscardOpen(false)
          router.push("/master-data/voucher-categories")
        }}
      />
    </PageScaffold>
  )
}
