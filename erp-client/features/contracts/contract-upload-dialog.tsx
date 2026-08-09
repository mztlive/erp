"use client"

import * as React from "react"
import { z } from "zod"

import {
  CustomerCombobox,
  DiscardConfirmDialog,
  SettlementPartyCombobox,
} from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { useSelector } from "@tanstack/react-form"
import {
  PAYMENT_TERM_OPTIONS,
  paymentTermLabel,
} from "@/lib/business-options"
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
  Field,
  FieldError,
  FieldLabel,
} from "@/components/ui/field"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { CircleAlertIcon } from "lucide-react"
import { contractPdfError } from "@/features/contracts/pdf"
import { useUploadContractPdfMutation } from "@/features/contracts/queries"
import type { UploadContractPdfResult } from "@/features/contracts/types"
import {
  useCustomerCenterQuery,
  useCustomerDirectoryQuery,
} from "@/features/customers/queries"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { usePartyOptionsQuery } from "@/hooks/use-options"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage } from "@/lib/api/errors"

const uploadSchema = z
  .object({
    pdfFile: z.custom<File | null>(),
    contractNo: z.string().trim().min(1, "请填写合同编号"),
    customerId: z.string().trim().min(1, "请选择客户"),
    customerName: z.string().trim().min(2, "请选择客户"),
    settlementPartyId: z.string().trim().min(1, "请选择结算主体"),
    settlementPartyName: z.string().trim().min(2, "请选择结算主体"),
    paymentTerms: z.string().trim().min(1, "请选择付款条件"),
    signedAt: z.string().min(1, "请填写签订日期"),
    validFrom: z.string().min(1, "请填写有效期起"),
    validTo: z.string().min(1, "请填写有效期止"),
  })
  .superRefine((value, context) => {
    const fileError = contractPdfError(value.pdfFile)
    if (fileError) {
      context.addIssue({ code: "custom", path: ["pdfFile"], message: fileError })
    }
    if (value.validFrom && value.validTo && value.validTo < value.validFrom) {
      context.addIssue({
        code: "custom",
        path: ["validTo"],
        message: "有效期止不能早于有效期起",
      })
    }
  })

function uploadErrorMessage(error: unknown): string {
  const message = getErrorMessage(error, "上传失败，请使用原任务号重试。")
  if (message === "CONTRACT_NO_EXISTS") {
    return "该合同编号已存在，请打开已有合同核对；重复编号不能新建合同。"
  }
  if (message === "CONTRACT_VALIDITY_INVALID") {
    return "有效期止不能早于有效期起。"
  }
  return message
}

export type ContractUploadDialogProps = {
  open: boolean
  onOpenChange: (open: boolean) => void
  /** 预选客户（客户中心 / 建单页带入） */
  initialCustomerId?: string
  /** 归档成功后回调；父层负责刷新列表、选中合同或展示结果 */
  onSuccess?: (result: UploadContractPdfResult) => void
}

export function ContractUploadDialog({
  open,
  onOpenChange,
  initialCustomerId = "",
  onSuccess,
}: ContractUploadDialogProps) {
  const customerQuery = useCustomerCenterQuery(initialCustomerId)
  const accountProfile = useAccountProfileQuery()
  const canReadAllCustomers = hasPermission(
    accountProfile.data?.permissions,
    "customer_scope:detail"
  )
  const customerDirectoryQuery = useCustomerDirectoryQuery({
    scope: canReadAllCustomers ? "all_authorized" : "assigned",
    status: "active",
    page: 1,
    pageSize: 100,
  })
  const uploadMutation = useUploadContractPdfMutation()
  const { data: partyOptions } = usePartyOptionsQuery()
  const seededCustomerRef = React.useRef(false)
  const [discardOpen, setDiscardOpen] = React.useState(false)

  const customerComboboxItems = React.useMemo(
    () =>
      (customerDirectoryQuery.data?.items ?? []).map((c) => ({
        id: c.id,
        customerNo: c.customerNo,
        legalName: c.legalName,
        shortName: c.shortName,
        statusLabel: c.statusLabel.label,
        statusTone: c.statusLabel.tone,
        ownerName: c.ownerName,
      })),
    [customerDirectoryQuery.data?.items]
  )

  const form = useAppForm({
    defaultValues: {
      pdfFile: null as File | null,
      contractNo: "",
      customerId: initialCustomerId,
      customerName: "",
      settlementPartyId: "",
      settlementPartyName: "",
      paymentTerms: "CONTRACT",
      signedAt: "",
      validFrom: "",
      validTo: "",
    },
    validators: { onChange: uploadSchema },
    onSubmit: async ({ value }) => {
      if (!value.pdfFile) return
      const result = await uploadMutation.mutateAsync({
        pdfFile: value.pdfFile,
        contractNo: value.contractNo.trim(),
        customerId:
          value.customerId.trim() || initialCustomerId || undefined,
        customerName: value.customerName.trim(),
        settlementPartyName: value.settlementPartyName.trim(),
        paymentTerms:
          paymentTermLabel(value.paymentTerms) || value.paymentTerms.trim(),
        signedAt: value.signedAt,
        validFrom: value.validFrom,
        validTo: value.validTo,
        idempotencyKey: `upload-${Date.now().toString(36)}`,
      })
      form.reset()
      uploadMutation.reset()
      onOpenChange(false)
      onSuccess?.(result)
    },
  })

  const dirty = useSelector(form.store, (state) => state.isDirty)

  React.useEffect(() => {
    if (!dirty) return
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      e.preventDefault()
      e.returnValue = "当前输入尚未提交，刷新后将丢失。"
    }
    window.addEventListener("beforeunload", onBeforeUnload)
    return () => window.removeEventListener("beforeunload", onBeforeUnload)
  }, [dirty])

  React.useEffect(() => {
    if (!open) {
      seededCustomerRef.current = false
      uploadMutation.reset()
      return
    }
    form.reset()
    // 日期默认今天 / 明年同日，避免硬编码日期随时间失真。
    const today = new Date()
    const pad = (n: number) => String(n).padStart(2, "0")
    const todayText = `${today.getFullYear()}-${pad(today.getMonth() + 1)}-${pad(today.getDate())}`
    const nextYear = new Date(today)
    nextYear.setFullYear(today.getFullYear() + 1)
    const nextYearText = `${nextYear.getFullYear()}-${pad(nextYear.getMonth() + 1)}-${pad(nextYear.getDate())}`
    form.setFieldValue("signedAt", todayText)
    form.setFieldValue("validFrom", todayText)
    form.setFieldValue("validTo", nextYearText)
    if (initialCustomerId) {
      form.setFieldValue("customerId", initialCustomerId)
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps -- only seed when dialog opens
  }, [open, initialCustomerId])

  React.useEffect(() => {
    if (!open) return
    const customer = customerQuery.data
    if (!customer || seededCustomerRef.current) return
    seededCustomerRef.current = true
    form.setFieldValue("customerId", customer.customerId)
    form.setFieldValue("customerName", customer.currentRevision.legalName)
  }, [customerQuery.data, form, open])

  return (
    <>
      <Dialog
        open={open}
        onOpenChange={(next) => {
          if (!next && dirty) {
            setDiscardOpen(true)
            return
          }
          if (!next) {
            form.reset()
            uploadMutation.reset()
          }
          onOpenChange(next)
        }}
      >
        <DialogContent className="flex max-h-[calc(100dvh-2rem)] w-full flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl">
          <DialogHeader className="px-6 pt-6">
            <DialogTitle>上传合同 PDF</DialogTitle>
            <DialogDescription>
              系统不新建或编辑合同正文；上传已签署电子档并补充检索信息后，形成可引用的合同版本。
            </DialogDescription>
          </DialogHeader>
          <form
            className="flex min-h-0 flex-1 flex-col"
            onSubmit={(event) => {
              event.preventDefault()
              void form.handleSubmit()
            }}
          >
            <div className="min-h-0 flex-1 space-y-4 overflow-y-auto px-6 py-4">
              {uploadMutation.isError ? (
                <Alert variant="destructive">
                  <CircleAlertIcon aria-hidden="true" />
                  <AlertTitle>合同 PDF 未归档</AlertTitle>
                  <AlertDescription>
                    {uploadErrorMessage(uploadMutation.error)}
                  </AlertDescription>
                </Alert>
              ) : null}

              <div className="grid gap-4 md:grid-cols-2">
                <div className="min-w-0">
                  <form.AppField
                    name="pdfFile"
                    children={(field) => (
                      <field.PdfUploadField label="合同电子档" />
                    )}
                  />
                </div>

                <div className="grid min-w-0 content-start gap-3">
                  <form.AppField
                    name="contractNo"
                    children={(field) => <field.TextField label="合同编号" />}
                  />
                  <form.AppField
                    name="customerId"
                    children={(field) => {
                      const isInvalid =
                        field.state.meta.isTouched && !field.state.meta.isValid
                      const errors = toFieldErrors(field.state.meta.errors)
                      return (
                        <Field data-invalid={isInvalid || undefined}>
                          <FieldLabel htmlFor="upload-customerId">客户</FieldLabel>
                          <CustomerCombobox
                            value={field.state.value || undefined}
                            onValueChange={(id) => {
                              const next = id ?? ""
                              field.handleChange(next)
                              const customer = customerComboboxItems.find(
                                (c) => c.id === next
                              )
                              form.setFieldValue(
                                "customerName",
                                customer?.legalName ?? ""
                              )
                            }}
                            customers={customerComboboxItems}
                            loading={customerDirectoryQuery.isPending}
                            placeholder="搜索客户编号或名称"
                          />
                          {isInvalid ? <FieldError errors={errors} /> : null}
                        </Field>
                      )
                    }}
                  />
                  <form.AppField
                    name="settlementPartyId"
                    children={(field) => {
                      const isInvalid =
                        field.state.meta.isTouched && !field.state.meta.isValid
                      const errors = toFieldErrors(field.state.meta.errors)
                      return (
                        <Field data-invalid={isInvalid || undefined}>
                          <FieldLabel htmlFor="upload-settlementPartyId">
                            结算主体
                          </FieldLabel>
                          <SettlementPartyCombobox
                            value={field.state.value || undefined}
                            onValueChange={(id) => {
                              const next = id ?? ""
                              field.handleChange(next)
                              form.setFieldValue(
                                "settlementPartyName",
                                partyOptions?.find(
                                  (p) => p.partyId === next
                                )?.displayName ?? ""
                              )
                            }}
                            parties={[...(partyOptions ?? [])]}
                            placeholder="搜索结算主体"
                          />
                          {isInvalid ? <FieldError errors={errors} /> : null}
                        </Field>
                      )
                    }}
                  />
                  <form.AppField
                    name="paymentTerms"
                    children={(field) => (
                      <field.SelectField
                        label="付款条件"
                        options={PAYMENT_TERM_OPTIONS}
                        description="用于销售单快速带出；完整条款以 PDF 为准。"
                      />
                    )}
                  />
                </div>
              </div>

              <div className="grid gap-3 sm:grid-cols-3">
                <form.AppField
                  name="signedAt"
                  children={(field) => <field.DateField label="签订日期" />}
                />
                <form.AppField
                  name="validFrom"
                  children={(field) => <field.DateField label="有效期起" />}
                />
                <form.AppField
                  name="validTo"
                  children={(field) => <field.DateField label="有效期止" />}
                />
              </div>
            </div>
            <DialogFooter className="shrink-0 border-t px-6 py-4">
              <DialogClose render={<Button type="button" variant="outline" />}>
                取消
              </DialogClose>
              <form.AppForm>
                <form.SubmitButton
                  label={uploadMutation.isPending ? "上传中…" : "上传并归档"}
                />
              </form.AppForm>
            </DialogFooter>
          </form>
        </DialogContent>
      </Dialog>

      <DiscardConfirmDialog
        open={discardOpen}
        onOpenChange={setDiscardOpen}
        onConfirm={() => {
          setDiscardOpen(false)
          form.reset()
          uploadMutation.reset()
          onOpenChange(false)
        }}
      />
    </>
  )
}
