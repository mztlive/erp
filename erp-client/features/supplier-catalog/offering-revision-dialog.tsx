"use client"

import * as React from "react"

import { OptionCombobox } from "@/components/business"
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
import { Label } from "@/components/ui/label"
import { useAppForm } from "@/components/form"
import {
  offeringDefaultsFromCurrentRevision,
  offeringDraftSchema,
  offeringRevisionPayload,
} from "@/features/supplier-catalog/offering-form-model"
import { useReviseSupplierOfferingMutation } from "@/features/supplier-catalog/queries"
import type { SupplierCatalogItemView } from "@/features/supplier-catalog/types"

function idempotencyKey(prefix: string) {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

/**
 * 改供给价：不依赖来源变化，随时可对已入池的供应商商品发起供给条件修订。
 * 与队列页"确认建议供给"是两个入口，共用 offering-form-model 里的字段与提交逻辑。
 */
export function OfferingRevisionDialog({
  item,
  open,
  onOpenChange,
}: {
  item?: SupplierCatalogItemView
  open: boolean
  onOpenChange: (open: boolean) => void
}) {
  const reviseMutation = useReviseSupplierOfferingMutation()
  const [submitError, setSubmitError] = React.useState<string | null>(null)
  const [succeeded, setSucceeded] = React.useState(false)
  const seededKeyRef = React.useRef<string>("")

  const offering = item?.offering
  const currentRevision = offering?.currentRevision

  const form = useAppForm({
    defaultValues: offeringDefaultsFromCurrentRevision(currentRevision),
    validators: { onChange: offeringDraftSchema },
    onSubmit: async ({ value }) => {
      if (!offering || !currentRevision) return
      setSubmitError(null)
      try {
        await reviseMutation.mutateAsync(
          offeringRevisionPayload(value, {
            offeringId: offering.stableId,
            expectedRevisionNo: currentRevision.revisionNo,
            availableQuantity: currentRevision.availableQuantity,
            idempotencyKey: idempotencyKey("revise-offering"),
            defaultChangeReason: "采购调整供货条件",
          })
        )
        setSucceeded(true)
      } catch (error) {
        setSubmitError(
          error instanceof Error ? error.message : "保存供货条件失败"
        )
      }
    },
  })

  React.useEffect(() => {
    if (!open) {
      seededKeyRef.current = ""
      setSucceeded(false)
      setSubmitError(null)
      return
    }
    if (!offering || !currentRevision) return
    const key = `${offering.stableId}:${currentRevision.revisionNo}`
    if (seededKeyRef.current === key) return
    seededKeyRef.current = key
    form.reset(offeringDefaultsFromCurrentRevision(currentRevision))
    setSucceeded(false)
    setSubmitError(null)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅按供给身份重置表单
  }, [open, offering, currentRevision])

  const productName = item?.supplierProduct.currentRevision.name ?? "供应商商品"

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="flex max-h-[90vh] flex-col sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>改供给价</DialogTitle>
          <DialogDescription>
            {productName} · 修改价格、税率、起订量、区域、有效期或供给状态后形成新修订，不改动商城销售价。
          </DialogDescription>
        </DialogHeader>

        <form
          className="min-h-0 flex-1 space-y-4 overflow-y-auto pr-1"
          onSubmit={(event) => {
            event.preventDefault()
            void form.handleSubmit()
          }}
        >
          {!offering || !currentRevision ? (
            <Alert variant="destructive">
              <AlertTitle>暂无供给条件</AlertTitle>
              <AlertDescription>
                该供应商商品尚未入池，没有可修改的供给条件；请先完成入池。
              </AlertDescription>
            </Alert>
          ) : succeeded ? (
            <Alert>
              <AlertTitle>供货条件已生效</AlertTitle>
              <AlertDescription>新修订已保存，立即生效。</AlertDescription>
            </Alert>
          ) : (
            <>
              <div className="grid gap-3 sm:grid-cols-2">
                <form.AppField name="dropshipSupplyPriceGross">
                  {(field) => <field.TextField label="一件代发供给价（含税运）" />}
                </form.AppField>
                <form.AppField name="bulkSupplyPriceGross">
                  {(field) => <field.TextField label="集采供给价（含税）" />}
                </form.AppField>
                <form.AppField name="inputTaxRate">
                  {(field) => (
                    <field.TextField
                      label="进项税率"
                      description="0 到 1 的十进制数，例如 0.13"
                    />
                  )}
                </form.AppField>
                <form.AppField name="freightAmount">
                  {(field) => <field.TextField label="运费" />}
                </form.AppField>
                <form.AppField name="serviceFeeAmount">
                  {(field) => <field.TextField label="服务费" />}
                </form.AppField>
                <form.AppField name="minimumOrderQuantity">
                  {(field) => <field.TextField label="最小起订量（基础单位）" />}
                </form.AppField>
                <form.AppField name="dropshipExpress">
                  {(field) => <field.TextField label="一件代发快递" />}
                </form.AppField>
                <form.AppField name="supplyRegionText">
                  {(field) => <field.TextField label="可供区域（逗号分隔）" />}
                </form.AppField>
                <form.AppField name="productCapabilitiesText">
                  {(field) => <field.TextField label="商品能力（逗号分隔）" />}
                </form.AppField>
                <form.AppField name="validFrom">
                  {(field) => <field.TextField label="生效日期" />}
                </form.AppField>
                <form.AppField name="validTo">
                  {(field) => <field.TextField label="失效日期（可空）" />}
                </form.AppField>
                <form.AppField name="status">
                  {(field) => (
                    <div className="space-y-1.5">
                      <Label>供给状态</Label>
                      <OptionCombobox
                        value={field.state.value}
                        onValueChange={(value) =>
                          field.handleChange(
                            (value ?? "ACTIVE") as "ACTIVE" | "PAUSED" | "STOPPED"
                          )
                        }
                        options={[
                          { value: "ACTIVE", label: "启用" },
                          { value: "PAUSED", label: "暂停" },
                          { value: "STOPPED", label: "停止" },
                        ]}
                        className="w-full"
                      />
                    </div>
                  )}
                </form.AppField>
              </div>
              <form.AppField name="note">
                {(field) => <field.TextareaField label="变更原因" rows={2} />}
              </form.AppField>

              {submitError ? (
                <Alert variant="destructive">
                  <AlertTitle>无法提交</AlertTitle>
                  <AlertDescription>{submitError}</AlertDescription>
                </Alert>
              ) : null}
            </>
          )}

          <DialogFooter>
            <DialogClose render={<Button type="button" variant="outline" />}>
              关闭
            </DialogClose>
            {offering && currentRevision && !succeeded ? (
              <form.AppForm>
                <form.SubmitButton
                  label="确认供货条件"
                  disabled={reviseMutation.isPending}
                />
              </form.AppForm>
            ) : null}
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  )
}
