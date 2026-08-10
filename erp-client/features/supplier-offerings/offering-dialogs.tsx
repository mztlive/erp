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
import {
  FieldGroup,
  FieldLegend,
  FieldSeparator,
  FieldSet,
} from "@/components/ui/field"
import { Label } from "@/components/ui/label"
import { toast } from "@/components/ui/toast"
import {
  CompanySkuSearchCombobox,
  SupplierSearchCombobox,
} from "@/features/entity-selectors"
import {
  useCreateSupplierOfferingMutation,
  useReviseSupplierOfferingMutation,
  useUpdateOfferingAvailabilityMutation,
} from "@/features/supplier-offerings/queries"
import type {
  AvailabilityStatus,
  FixedSku,
  OfferingStatus,
  SupplierOfferingView,
} from "@/features/supplier-offerings/types"
import {
  AVAILABILITY_STATUS_LABELS,
  OFFERING_STATUS_LABELS,
} from "@/features/supplier-offerings/types"

const decimal = z
  .string()
  .trim()
  .regex(/^\d+(?:\.\d{1,4})?$/, "请输入非负数，最多 4 位小数")

const quantity = z
  .string()
  .trim()
  .regex(/^\d+(?:\.\d{1,6})?$/, "请输入非负数，最多 6 位小数")

const taxPercentage = z
  .string()
  .trim()
  .regex(/^\d+(?:\.\d{1,4})?$/, "请输入 0–100 的税率")
  .refine((value) => Number(value) <= 100, "税率不能超过 100%")

const termsSchema = {
  dropshipPrice: decimal,
  bulkPrice: decimal,
  minimumQuantity: quantity.refine((value) => Number(value) > 0, "起订量必须大于 0"),
  inputTaxPercentage: taxPercentage,
  supplyRegionText: z.string().trim().min(1, "请填写可供区域"),
  validFrom: z.string().trim().min(1, "请选择生效日期"),
  validTo: z.string(),
  dropshipExpress: z.string(),
  freightAmount: z.union([z.literal(""), decimal]),
  serviceFeeAmount: z.union([z.literal(""), decimal]),
}

const createSchema = z.object({
  skuId: z.string().min(1, "请选择公司 SKU"),
  supplierId: z.string().min(1, "请选择供应商"),
  supplierProductCode: z.string(),
  supplierSkuCode: z.string().trim().min(1, "请填写供应商 SKU 编码"),
  ...termsSchema,
  availabilityStatus: z.enum(["AVAILABLE", "UNAVAILABLE", "STOPPED", "STALE"]),
  availableQuantity: z.union([z.literal(""), quantity]),
  changeReason: z.string().trim().min(1, "请填写登记原因"),
})

const reviseSchema = z.object({
  ...termsSchema,
  status: z.enum(["ACTIVE", "PAUSED", "STOPPED"]),
  changeReason: z.string().trim().min(1, "请填写变更原因"),
})

const availabilitySchema = z.object({
  availabilityStatus: z.enum(["AVAILABLE", "UNAVAILABLE", "STOPPED", "STALE"]),
  availableQuantity: z.union([z.literal(""), quantity]),
  changeReason: z.string().trim().min(1, "请填写变更原因"),
})

function idempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 10)}`
}

function splitValues(value: string): readonly string[] {
  return value
    .split(/[，,、]/)
    .map((item) => item.trim())
    .filter(Boolean)
}

function rateFromPercentage(value: string): string {
  return (Number(value) / 100).toFixed(6)
}

function percentageFromRate(value?: string | null): string {
  if (!value) return ""
  return String(Number(value) * 100)
}

function errorMessage(error: unknown, fallback: string): string {
  return error && typeof error === "object" && "message" in error
    ? String(error.message)
    : fallback
}

export function RegisterSupplyForSkuDialog({
  open,
  onOpenChange,
  fixedSku,
}: {
  open: boolean
  onOpenChange: (open: boolean) => void
  fixedSku?: FixedSku
}) {
  const mutation = useCreateSupplierOfferingMutation()
  const [submitError, setSubmitError] = React.useState<string | null>(null)
  const form = useAppForm({
    defaultValues: {
      skuId: fixedSku?.skuId ?? "",
      supplierId: "",
      supplierProductCode: "",
      supplierSkuCode: "",
      dropshipPrice: "",
      bulkPrice: "",
      minimumQuantity: "1",
      inputTaxPercentage: "",
      supplyRegionText: "",
      validFrom: "",
      validTo: "",
      dropshipExpress: "",
      freightAmount: "",
      serviceFeeAmount: "",
      availabilityStatus: "AVAILABLE" as AvailabilityStatus,
      availableQuantity: "",
      changeReason: "新增供应商供给",
    },
    validators: { onSubmit: createSchema },
    onSubmit: async ({ value }) => {
      setSubmitError(null)
      try {
        await mutation.mutateAsync({
          sku_id: fixedSku?.skuId ?? value.skuId,
          supplier_id: value.supplierId,
          supplier_product_code: value.supplierProductCode.trim() || null,
          supplier_sku_code: value.supplierSkuCode.trim(),
          source_type: "MANUAL",
          terms: {
            dropship_supply_price_gross: value.dropshipPrice.trim(),
            bulk_supply_price_gross: value.bulkPrice.trim(),
            input_tax_rate: rateFromPercentage(value.inputTaxPercentage),
            bulk_minimum_order_quantity: value.minimumQuantity.trim(),
            supply_region: splitValues(value.supplyRegionText),
            product_capabilities: [],
            valid_from: value.validFrom,
            valid_to: value.validTo || null,
            dropship_express: value.dropshipExpress.trim() || null,
            freight_amount: value.freightAmount.trim() || null,
            service_fee_amount: value.serviceFeeAmount.trim() || null,
          },
          availability_status: value.availabilityStatus,
          available_quantity: value.availableQuantity.trim() || null,
          change_reason: value.changeReason.trim(),
          idempotency_key: idempotencyKey("create-supplier-offering"),
        })
        toast.add({
          title: "供给已添加",
          description:
            "公司 SKU 与供应商之间的供给关系、首版条款和初始可供状态已同时生效。",
          type: "success",
          timeout: 4000,
        })
        onOpenChange(false)
      } catch (error) {
        setSubmitError(errorMessage(error, "供给登记失败，请稍后重试"))
      }
    },
  })

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[88vh] w-[calc(100vw-2rem)] flex-col gap-4 overflow-hidden p-5 sm:max-h-[76vh] sm:max-w-6xl">
        <DialogHeader className="shrink-0">
          <DialogTitle>添加供给</DialogTitle>
          <DialogDescription>
            供给直接连接公司 SKU 与供应商；供应商订货编码、商业条款和当前可供情况在此维护。
          </DialogDescription>
        </DialogHeader>

        {fixedSku ? (
          <div className="rounded-lg border bg-muted/40 px-3 py-2 text-sm">
            <div className="font-medium">{fixedSku.skuName}</div>
            <div className="mt-1 text-muted-foreground">
              {fixedSku.skuCode} · {fixedSku.specification} · {fixedSku.baseUnit}
            </div>
          </div>
        ) : null}

        {submitError ? (
          <Alert variant="destructive">
            <AlertTitle>保存失败</AlertTitle>
            <AlertDescription>{submitError}</AlertDescription>
          </Alert>
        ) : null}

        <form
          className="flex min-h-0 flex-1 flex-col gap-4 overflow-hidden"
          onSubmit={(event) => {
            event.preventDefault()
            void form.handleSubmit()
          }}
        >
          <div className="min-h-0 flex-1 overflow-y-auto overscroll-contain pr-1">
            <FieldGroup className="gap-4">
              <FieldSet className="gap-4 rounded-lg border bg-muted/20 p-4">
                <FieldLegend variant="label">基础信息</FieldLegend>
                <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
                  {!fixedSku ? (
                    <form.AppField name="skuId">
                      {(field) => (
                        <div className="space-y-1.5 sm:col-span-2">
                          <Label>公司 SKU *</Label>
                          <CompanySkuSearchCombobox
                            value={field.state.value || undefined}
                            onValueChange={(value) =>
                              field.handleChange(value ?? "")
                            }
                            placeholder="选择公司 SKU"
                            className="w-full"
                          />
                        </div>
                      )}
                    </form.AppField>
                  ) : null}
                  <form.AppField name="supplierId">
                    {(field) => (
                      <div className="space-y-1.5 sm:col-span-2">
                        <Label>供应商 *</Label>
                        <SupplierSearchCombobox
                          value={field.state.value || undefined}
                          onValueChange={(value) =>
                            field.handleChange(value ?? "")
                          }
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
                        description="用于下单、对账和履约快照"
                      />
                    )}
                  </form.AppField>
                  <form.AppField name="supplierProductCode">
                    {(field) => <field.TextField label="供应商商品编码" />}
                  </form.AppField>
                </div>
              </FieldSet>

              <div className="grid gap-4 lg:grid-cols-2">
                <FieldSet className="gap-4 rounded-lg border p-4">
                  <FieldLegend variant="label">价格与条款</FieldLegend>
                  <div className="grid gap-3 sm:grid-cols-2">
                    <form.AppField name="dropshipPrice">
                      {(field) => (
                        <field.TextField label="一件代发供给价（含税）*" />
                      )}
                    </form.AppField>
                    <form.AppField name="bulkPrice">
                      {(field) => (
                        <field.TextField label="集采供给价（含税）*" />
                      )}
                    </form.AppField>
                    <form.AppField name="minimumQuantity">
                      {(field) => <field.TextField label="集采起订量 *" />}
                    </form.AppField>
                    <form.AppField name="inputTaxPercentage">
                      {(field) => (
                        <field.TextField
                          label="进项税率（%）*"
                          description="例如 13 表示 13%"
                        />
                      )}
                    </form.AppField>
                  </div>
                </FieldSet>

                <FieldSet className="gap-4 rounded-lg border p-4">
                  <FieldLegend variant="label">供应范围与时效</FieldLegend>
                  <div className="grid gap-3 sm:grid-cols-2">
                    <form.AppField name="supplyRegionText">
                      {(field) => (
                        <div className="sm:col-span-2">
                          <field.TextField
                            label="可供区域 *"
                            description="多个区域使用逗号分隔"
                          />
                        </div>
                      )}
                    </form.AppField>
                    <form.AppField name="validFrom">
                      {(field) => <field.DateField label="生效日期 *" />}
                    </form.AppField>
                    <form.AppField name="validTo">
                      {(field) => <field.DateField label="失效日期" />}
                    </form.AppField>
                  </div>
                </FieldSet>
              </div>

              <div className="grid gap-4 lg:grid-cols-2">
                <FieldSet className="gap-4 rounded-lg border p-4">
                  <FieldLegend variant="label">物流与费用</FieldLegend>
                  <div className="grid gap-3 sm:grid-cols-2">
                    <form.AppField name="dropshipExpress">
                      {(field) => (
                        <field.TextField label="一件代发快递说明" />
                      )}
                    </form.AppField>
                    <form.AppField name="freightAmount">
                      {(field) => <field.TextField label="运费" />}
                    </form.AppField>
                    <form.AppField name="serviceFeeAmount">
                      {(field) => <field.TextField label="服务费" />}
                    </form.AppField>
                  </div>
                </FieldSet>

                <FieldSet className="gap-4 rounded-lg border p-4">
                  <FieldLegend variant="label">
                    可供状态与登记说明
                  </FieldLegend>
                  <div className="grid gap-3 sm:grid-cols-2">
                    <form.AppField name="availabilityStatus">
                      {(field) => (
                        <div className="space-y-1.5">
                          <Label>初始可供状态 *</Label>
                          <OptionCombobox
                            value={field.state.value}
                            onValueChange={(value) =>
                              field.handleChange(
                                (value ?? "AVAILABLE") as AvailabilityStatus,
                              )
                            }
                            options={Object.entries(
                              AVAILABILITY_STATUS_LABELS,
                            ).map(([value, label]) => ({ value, label }))}
                            className="w-full"
                          />
                        </div>
                      )}
                    </form.AppField>
                    <form.AppField name="availableQuantity">
                      {(field) => (
                        <field.TextField
                          label="当前可供数量"
                          description="留空表示供应商未提供数量上限"
                        />
                      )}
                    </form.AppField>
                    <form.AppField name="changeReason">
                      {(field) => (
                        <div className="sm:col-span-2">
                          <field.TextField label="登记原因 *" />
                        </div>
                      )}
                    </form.AppField>
                  </div>
                </FieldSet>
              </div>
            </FieldGroup>
          </div>

          <DialogFooter className="shrink-0 border-t pt-4">
            <DialogClose render={<Button type="button" variant="outline" />}>
              关闭
            </DialogClose>
            <form.AppForm>
              <form.SubmitButton
                label="保存供给"
                disabled={mutation.isPending}
              />
            </form.AppForm>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export function ReviseOfferingDialog({
  offering,
  onOpenChange,
}: {
  offering: SupplierOfferingView
  onOpenChange: (open: boolean) => void
}) {
  const mutation = useReviseSupplierOfferingMutation()
  const [submitError, setSubmitError] = React.useState<string | null>(null)
  const form = useAppForm({
    defaultValues: {
      dropshipPrice: offering.dropship_supply_price_gross ?? "",
      bulkPrice: offering.bulk_supply_price_gross ?? "",
      minimumQuantity: offering.bulk_minimum_order_quantity ?? "1",
      inputTaxPercentage: percentageFromRate(offering.input_tax_rate),
      supplyRegionText: offering.supply_region.join("，"),
      validFrom: offering.valid_from ?? "",
      validTo: offering.valid_to ?? "",
      dropshipExpress: offering.dropship_express ?? "",
      freightAmount: offering.freight_amount ?? "",
      serviceFeeAmount: offering.service_fee_amount ?? "",
      status: offering.status,
      changeReason: "调整供给条款",
    },
    validators: { onSubmit: reviseSchema },
    onSubmit: async ({ value }) => {
      setSubmitError(null)
      try {
        await mutation.mutateAsync({
          offeringId: offering.id,
          expected_revision_no: offering.current_revision_no ?? 1,
          terms: {
            dropship_supply_price_gross: value.dropshipPrice.trim(),
            bulk_supply_price_gross: value.bulkPrice.trim(),
            input_tax_rate: rateFromPercentage(value.inputTaxPercentage),
            bulk_minimum_order_quantity: value.minimumQuantity.trim(),
            supply_region: splitValues(value.supplyRegionText),
            product_capabilities: offering.product_capabilities,
            valid_from: value.validFrom,
            valid_to: value.validTo || null,
            dropship_express: value.dropshipExpress.trim() || null,
            freight_amount: value.freightAmount.trim() || null,
            service_fee_amount: value.serviceFeeAmount.trim() || null,
          },
          status: value.status,
          change_reason: value.changeReason.trim(),
          idempotency_key: idempotencyKey("revise-supplier-offering"),
        })
        onOpenChange(false)
      } catch (error) {
        setSubmitError(errorMessage(error, "供给条款保存失败"))
      }
    },
  })

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="max-h-[90vh] overflow-y-auto sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>修订供给条款</DialogTitle>
          <DialogDescription>
            {offering.sku_name ?? offering.sku_no ?? offering.sku_id} · {offering.supplier_name ?? offering.supplier_sku_code}
          </DialogDescription>
        </DialogHeader>
        {submitError ? (
          <Alert variant="destructive">
            <AlertTitle>保存失败</AlertTitle>
            <AlertDescription>{submitError}</AlertDescription>
          </Alert>
        ) : null}
        <form
          className="space-y-6"
          onSubmit={(event) => {
            event.preventDefault()
            void form.handleSubmit()
          }}
        >
          <FieldGroup>
            <FieldSet>
              <FieldLegend variant="label">价格与条款</FieldLegend>
              <div className="grid gap-4 sm:grid-cols-2">
                <form.AppField name="dropshipPrice">
                  {(field) => <field.TextField label="一件代发供给价（含税）*" />}
                </form.AppField>
                <form.AppField name="bulkPrice">
                  {(field) => <field.TextField label="集采供给价（含税）*" />}
                </form.AppField>
                <form.AppField name="minimumQuantity">
                  {(field) => <field.TextField label="集采起订量 *" />}
                </form.AppField>
                <form.AppField name="inputTaxPercentage">
                  {(field) => (
                    <field.TextField
                      label="进项税率（%）*"
                      description="例如 13 表示 13%"
                    />
                  )}
                </form.AppField>
              </div>
            </FieldSet>

            <FieldSeparator />

            <FieldSet>
              <FieldLegend variant="label">供应范围与时效</FieldLegend>
              <div className="grid gap-4 sm:grid-cols-2">
                <form.AppField name="supplyRegionText">
                  {(field) => (
                    <div className="sm:col-span-2">
                      <field.TextField
                        label="可供区域 *"
                        description="多个区域使用逗号分隔"
                      />
                    </div>
                  )}
                </form.AppField>
                <form.AppField name="validFrom">
                  {(field) => <field.DateField label="生效日期 *" />}
                </form.AppField>
                <form.AppField name="validTo">
                  {(field) => <field.DateField label="失效日期" />}
                </form.AppField>
              </div>
            </FieldSet>

            <FieldSeparator />

            <FieldSet>
              <FieldLegend variant="label">物流与费用</FieldLegend>
              <div className="grid gap-4 sm:grid-cols-2">
                <form.AppField name="dropshipExpress">
                  {(field) => <field.TextField label="一件代发快递说明" />}
                </form.AppField>
                <form.AppField name="freightAmount">
                  {(field) => <field.TextField label="运费" />}
                </form.AppField>
                <form.AppField name="serviceFeeAmount">
                  {(field) => <field.TextField label="服务费" />}
                </form.AppField>
              </div>
            </FieldSet>

            <FieldSeparator />

            <FieldSet>
              <FieldLegend variant="label">状态与说明</FieldLegend>
              <div className="grid gap-4 sm:grid-cols-2">
                <form.AppField name="status">
                  {(field) => (
                    <div className="space-y-1.5">
                      <Label>供给关系状态 *</Label>
                      <OptionCombobox
                        value={field.state.value}
                        onValueChange={(value) =>
                          field.handleChange((value ?? "ACTIVE") as OfferingStatus)
                        }
                        options={Object.entries(OFFERING_STATUS_LABELS).map(
                          ([value, label]) => ({ value, label })
                        )}
                        className="w-full"
                      />
                    </div>
                  )}
                </form.AppField>
                <form.AppField name="changeReason">
                  {(field) => <field.TextField label="变更原因 *" />}
                </form.AppField>
              </div>
            </FieldSet>
          </FieldGroup>
          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              取消
            </DialogClose>
            <form.AppForm>
              <form.SubmitButton label="保存新版本" disabled={mutation.isPending} />
            </form.AppForm>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export function UpdateAvailabilityDialog({
  offering,
  onOpenChange,
}: {
  offering: SupplierOfferingView
  onOpenChange: (open: boolean) => void
}) {
  const mutation = useUpdateOfferingAvailabilityMutation()
  const [submitError, setSubmitError] = React.useState<string | null>(null)
  const form = useAppForm({
    defaultValues: {
      availabilityStatus: offering.availability_status ?? "UNAVAILABLE",
      availableQuantity: offering.available_quantity ?? "",
      changeReason: "更新当前可供情况",
    },
    validators: { onSubmit: availabilitySchema },
    onSubmit: async ({ value }) => {
      setSubmitError(null)
      try {
        await mutation.mutateAsync({
          offeringId: offering.id,
          expected_version: offering.availability_version ?? undefined,
          availability_status: value.availabilityStatus,
          available_quantity: value.availableQuantity.trim() || null,
          change_reason: value.changeReason.trim(),
          idempotency_key: idempotencyKey("update-offering-availability"),
        })
        onOpenChange(false)
      } catch (error) {
        setSubmitError(errorMessage(error, "可供情况保存失败"))
      }
    },
  })

  return (
    <Dialog open onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-lg">
        <DialogHeader>
          <DialogTitle>更新当前可供情况</DialogTitle>
          <DialogDescription>
            该信息独立于商业条款版本，可由人工或供应商接口高频更新。
          </DialogDescription>
        </DialogHeader>
        {submitError ? (
          <Alert variant="destructive">
            <AlertTitle>保存失败</AlertTitle>
            <AlertDescription>{submitError}</AlertDescription>
          </Alert>
        ) : null}
        <form
          className="space-y-4"
          onSubmit={(event) => {
            event.preventDefault()
            void form.handleSubmit()
          }}
        >
          <form.AppField name="availabilityStatus">
            {(field) => (
              <div className="space-y-1.5">
                <Label>可供状态 *</Label>
                <OptionCombobox
                  value={field.state.value}
                  onValueChange={(value) =>
                    field.handleChange((value ?? "UNAVAILABLE") as AvailabilityStatus)
                  }
                  options={Object.entries(AVAILABILITY_STATUS_LABELS).map(
                    ([value, label]) => ({ value, label })
                  )}
                  className="w-full"
                />
              </div>
            )}
          </form.AppField>
          <form.AppField name="availableQuantity">
            {(field) => (
              <field.TextField
                label="当前可供数量"
                description="留空表示供应商未提供数量上限"
              />
            )}
          </form.AppField>
          <form.AppField name="changeReason">
            {(field) => <field.TextField label="变更原因 *" />}
          </form.AppField>
          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              取消
            </DialogClose>
            <form.AppForm>
              <form.SubmitButton label="保存可供情况" disabled={mutation.isPending} />
            </form.AppForm>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}

export type { FixedSku }
