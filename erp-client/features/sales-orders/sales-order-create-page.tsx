"use client"

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { CircleAlertIcon, FilePlus2Icon, PlusIcon } from "lucide-react"
import { z } from "zod"

import {
  DocumentSection,
  EditableLineItemTable,
  MoneyValue,
  PageHeader,
  StickyTotalBar,
  type EditableLineItemColumn,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
} from "@/components/ui/card"
import { useContractCenterQuery, useContractsForNewSalesOrderQuery } from "@/features/contracts/queries"
import { contractPdfError } from "@/features/contracts/pdf"
import { useCustomerCenterQuery } from "@/features/customers/queries"
import { useCreateSalesOrderMutation } from "@/features/sales-orders/queries"
import type {
  CreateSalesOrderInput,
  SalesOrderCreateIntent,
  SalesOrderContractSource,
  SalesOrderDraftLineInput,
  SalesOrderNature,
} from "@/features/sales-orders/types"

const positiveDecimal = (label: string) =>
  z
    .string()
    .trim()
    .regex(/^\d+(?:\.\d{1,2})?$/, `${label}最多保留两位小数`)
    .refine((value) => Number(value) > 0, `${label}必须大于 0`)

const draftLineSchema = z.object({
  rowKey: z.string().min(1),
  name: z.string().trim().min(1, "请输入销售项目"),
  sku: z.string(),
  quantity: positiveDecimal("数量"),
  unit: z.string().trim().min(1, "请输入单位"),
  unitPriceGross: positiveDecimal("含税单价"),
  fulfillmentMode: z.string(),
  dueDate: z.string(),
  faceValue: z.string(),
  giftRate: z.string(),
  cardForm: z.string(),
})

const createSalesOrderSchema = z
  .object({
    contractSource: z.enum(["existing", "upload_pdf"]),
    contractId: z.string(),
    requestedContractRevisionId: z.string(),
    contractRevisionLabel: z.string(),
    contractPdf: z.custom<File | null>(),
    uploadedContractNo: z.string(),
    uploadedSignedAt: z.string(),
    uploadedValidFrom: z.string(),
    uploadedValidTo: z.string(),
    customerName: z.string(),
    settlementEntity: z.string(),
    nature: z.enum(["physical_service", "card_voucher"]),
    ownerName: z.string().trim().min(1, "请输入负责销售"),
    welfareScene: z.string().trim().min(1, "请输入福利场景"),
    paymentTerms: z.string().trim().min(1, "请输入付款条件"),
    fulfillmentDeadline: z.string().min(1, "请选择履约期限"),
    taxRatePercent: positiveDecimal("税率").refine(
      (value) => Number(value) <= 100,
      "税率不能超过 100%"
    ),
    remark: z.string(),
    lineItems: z.array(draftLineSchema).min(1, "至少需要一条销售明细"),
  })
  .superRefine((value, context) => {
    if (value.contractSource === "existing") {
      if (!value.contractId) {
        context.addIssue({
          code: "custom",
          path: ["contractId"],
          message: "请选择已有有效合同",
        })
      }
      if (!value.requestedContractRevisionId || !value.contractRevisionLabel) {
        context.addIssue({
          code: "custom",
          path: ["contractId"],
          message: "合同版本尚未加载完成",
        })
      }
    } else {
      const fileError = contractPdfError(value.contractPdf)
      if (fileError) {
        context.addIssue({
          code: "custom",
          path: ["contractPdf"],
          message: fileError,
        })
      }
      if (!value.uploadedContractNo.trim()) {
        context.addIssue({
          code: "custom",
          path: ["uploadedContractNo"],
          message: "请填写合同编号",
        })
      }
      if (!value.uploadedSignedAt) {
        context.addIssue({
          code: "custom",
          path: ["uploadedSignedAt"],
          message: "请选择签订日期",
        })
      }
      if (!value.uploadedValidFrom || !value.uploadedValidTo) {
        context.addIssue({
          code: "custom",
          path: ["uploadedValidFrom"],
          message: "请填写合同有效期",
        })
      } else if (value.uploadedValidTo < value.uploadedValidFrom) {
        context.addIssue({
          code: "custom",
          path: ["uploadedValidTo"],
          message: "有效期止不能早于有效期起",
        })
      }
    }
    if (!value.customerName.trim()) {
      context.addIssue({
        code: "custom",
        path: ["customerName"],
        message:
          value.contractSource === "existing"
            ? "客户尚未加载完成"
            : "请填写客户名称",
      })
    }
    if (!value.settlementEntity.trim()) {
      context.addIssue({
        code: "custom",
        path: ["settlementEntity"],
        message:
          value.contractSource === "existing"
            ? "结算主体尚未加载完成"
            : "请填写结算主体",
      })
    }
    if (value.nature === "card_voucher" && value.lineItems.length !== 1) {
      context.addIssue({
        code: "custom",
        path: ["lineItems"],
        message: "卡券销售单必须恰好只有一条明细",
      })
    }
    value.lineItems.forEach((line, index) => {
      if (value.nature === "card_voucher") {
        if (!line.faceValue.trim() || Number(line.faceValue) <= 0) {
          context.addIssue({
            code: "custom",
            path: ["lineItems", index, "faceValue"],
            message: "请输入大于 0 的卡券面值",
          })
        }
        if (!line.cardForm.trim()) {
          context.addIssue({
            code: "custom",
            path: ["lineItems", index, "cardForm"],
            message: "请选择卡形态",
          })
        }
      } else {
        if (!line.fulfillmentMode.trim()) {
          context.addIssue({
            code: "custom",
            path: ["lineItems", index, "fulfillmentMode"],
            message: "请选择履约方式",
          })
        }
        if (!line.dueDate) {
          context.addIssue({
            code: "custom",
            path: ["lineItems", index, "dueDate"],
            message: "请选择明细交付日期",
          })
        }
      }
    })
  })

type CreateSalesOrderFormValues = z.input<typeof createSalesOrderSchema>

const NATURE_OPTIONS = [
  { value: "physical_service", label: "实物与服务" },
  { value: "card_voucher", label: "卡券" },
] as const

const CONTRACT_SOURCE_OPTIONS = [
  { value: "existing", label: "选择已有合同" },
  { value: "upload_pdf", label: "同步上传合同 PDF" },
] as const

const FULFILLMENT_OPTIONS = [
  { value: "公司仓发", label: "公司仓发" },
  { value: "供应商直发", label: "供应商直发" },
  { value: "电子交付", label: "电子交付" },
  { value: "现场服务", label: "现场服务" },
] as const

const CARD_FORM_OPTIONS = [
  { value: "电子卡", label: "电子卡" },
  { value: "实体卡", label: "实体卡" },
] as const

let draftLineSequence = 0

function createEmptyLine(nature: SalesOrderNature): SalesOrderDraftLineInput {
  draftLineSequence += 1
  return {
    rowKey: `draft-line-${draftLineSequence}`,
    name: "",
    sku: "",
    quantity: "1",
    unit: nature === "card_voucher" ? "张" : "件",
    unitPriceGross: "0.00",
    fulfillmentMode: nature === "physical_service" ? "公司仓发" : "",
    dueDate: "",
    faceValue: "",
    giftRate: "0.00",
    cardForm: nature === "card_voucher" ? "电子卡" : "",
  }
}

function calculateTotals(
  lineItems: readonly SalesOrderDraftLineInput[],
  taxRatePercent: string
) {
  const gross = lineItems.reduce((sum, line) => {
    const quantity = Number(line.quantity)
    const unitPrice = Number(line.unitPriceGross)
    return sum +
      (Number.isFinite(quantity) && Number.isFinite(unitPrice)
        ? quantity * unitPrice
        : 0)
  }, 0)
  const taxRate = Number(taxRatePercent)
  const net = gross / (1 + (Number.isFinite(taxRate) ? taxRate : 0) / 100)
  return {
    gross: gross.toFixed(2),
    net: net.toFixed(2),
    tax: (gross - net).toFixed(2),
  }
}

function errorMessage(error: unknown): string {
  if (!(error instanceof Error)) return "创建失败，请重试。"
  const messages: Record<string, string> = {
    CONTRACT_NOT_SELECTABLE: "所选合同已不可用于新建销售单，请刷新后重选。",
    CONTRACT_REVISION_NOT_FOUND: "所选合同修订不存在，请刷新合同后重试。",
    CONTRACT_REVISION_NOT_CURRENT: "新销售单只能引用合同当前有效修订。",
    CONTRACT_NO_EXISTS: "该合同编号已存在，请改为选择已有合同。",
    CONTRACT_VALIDITY_INVALID: "合同有效期填写有误，请检查后重试。",
    LINE_ITEM_REQUIRED: "至少需要一条销售明细。",
    LINE_ITEM_INVALID: "销售明细不完整，请检查项目、数量、单位和价格。",
    VOUCHER_REQUIRES_EXACTLY_ONE_LINE: "卡券销售单必须恰好一条明细。",
  }
  return messages[error.message] ?? error.message
}

export function SalesOrderCreatePage({
  initialCustomerId = "",
  initialContractId = "",
  initialContractRevisionId = "",
  initialNature = "physical_service",
}: {
  initialCustomerId?: string
  initialContractId?: string
  initialContractRevisionId?: string
  initialNature?: SalesOrderNature
}) {
  const router = useRouter()
  const contractsQuery = useContractsForNewSalesOrderQuery()
  const customerQuery = useCustomerCenterQuery(initialCustomerId)
  const createMutation = useCreateSalesOrderMutation()
  const [selectedContractId, setSelectedContractId] =
    React.useState(initialContractId)
  const preferredRevisionRef = React.useRef(initialContractRevisionId)
  const submitIntentRef = React.useRef<SalesOrderCreateIntent>("SAVE_DRAFT")
  const contractQuery = useContractCenterQuery(selectedContractId)

  const form = useAppForm({
    defaultValues: {
      contractSource: "existing" as SalesOrderContractSource,
      contractId: initialContractId,
      requestedContractRevisionId: initialContractRevisionId,
      contractRevisionLabel: "",
      contractPdf: null as File | null,
      uploadedContractNo: "",
      uploadedSignedAt: "2026-08-02",
      uploadedValidFrom: "2026-08-02",
      uploadedValidTo: "2027-08-01",
      customerName: "",
      settlementEntity: "",
      nature: initialNature,
      ownerName: "",
      welfareScene: "",
      paymentTerms: "",
      fulfillmentDeadline: "",
      taxRatePercent: initialNature === "card_voucher" ? "6.00" : "13.00",
      remark: "",
      lineItems: [createEmptyLine(initialNature)],
    } satisfies CreateSalesOrderFormValues,
    validators: {
      onSubmit: createSalesOrderSchema,
    },
    onSubmit: async ({ value }) => {
      if (value.contractSource === "upload_pdf" && !value.contractPdf) return
      const command: CreateSalesOrderInput = {
        contract:
          value.contractSource === "existing"
            ? {
                source: "existing",
                contractId: value.contractId,
                requestedContractRevisionId: value.requestedContractRevisionId,
              }
            : {
                source: "upload_pdf",
                pdfFile: value.contractPdf!,
                contractNo: value.uploadedContractNo.trim(),
                customerId: initialCustomerId || undefined,
                customerName: value.customerName.trim(),
                settlementPartyName: value.settlementEntity.trim(),
                signedAt: value.uploadedSignedAt,
                validFrom: value.uploadedValidFrom,
                validTo: value.uploadedValidTo,
              },
        nature: value.nature,
        ownerName: value.ownerName,
        welfareScene: value.welfareScene,
        paymentTerms: value.paymentTerms,
        fulfillmentDeadline: value.fulfillmentDeadline,
        taxRatePercent: value.taxRatePercent,
        remark: value.remark,
        lineItems: value.lineItems,
        intent: submitIntentRef.current,
        idempotencyKey:
          typeof crypto !== "undefined" && "randomUUID" in crypto
            ? crypto.randomUUID()
            : `so-create-${Date.now()}`,
      }
      const result = await createMutation.mutateAsync(command)
      router.push(`/sales/orders/${result.salesOrderId}`)
    },
  })

  const contractOptions = React.useMemo(
    () =>
      (contractsQuery.data ?? [])
        .filter(
          (contract) =>
            !initialCustomerId || contract.customer.customerId === initialCustomerId
        )
        .map((contract) => ({
          value: contract.contractId,
          label: `${contract.contractNo} · ${contract.customer.displayName} · v${contract.revisionNo}`,
        })),
    [contractsQuery.data, initialCustomerId]
  )

  React.useEffect(() => {
    const contract = contractQuery.data
    if (!contract) return
    const preferredRevision = preferredRevisionRef.current
      ? contract.revisionTimeline.find(
          (revision) =>
            revision.revisionId === preferredRevisionRef.current && revision.isCurrent
        )
      : undefined
    const revision =
      preferredRevision ??
      contract.revisionTimeline.find((candidate) => candidate.isCurrent)
    form.setFieldValue(
      "requestedContractRevisionId",
      revision?.revisionId ?? contract.currentRevision.revisionId
    )
    form.setFieldValue(
      "contractRevisionLabel",
      `${contract.contractNo}@v${revision?.revisionNo ?? contract.currentRevision.revisionNo}`
    )
    form.setFieldValue("customerName", contract.customer.displayName)
    form.setFieldValue(
      "settlementEntity",
      contract.currentRevision.settlementParty.displayName
    )
    form.setFieldValue("ownerName", contract.ownerLabel.split(" · ")[0] ?? "")
    form.setFieldValue(
      "paymentTerms",
      contract.currentRevision.paymentTermSnapshot.label
    )
  }, [contractQuery.data, form])

  const handleContractChange = React.useCallback(
    (contractId: string) => {
      preferredRevisionRef.current = ""
      setSelectedContractId(contractId)
      form.setFieldValue("requestedContractRevisionId", "")
      form.setFieldValue("contractRevisionLabel", "")
      form.setFieldValue("customerName", "")
      form.setFieldValue("settlementEntity", "")
    },
    [form]
  )

  const handleContractSourceChange = React.useCallback(
    (source: string) => {
      if (source === "upload_pdf") {
        preferredRevisionRef.current = ""
        setSelectedContractId("")
        form.setFieldValue("contractId", "")
        form.setFieldValue("requestedContractRevisionId", "")
        form.setFieldValue("contractRevisionLabel", "")
        const customerName =
          customerQuery.data?.currentRevision.legalName ?? ""
        form.setFieldValue("customerName", customerName)
        form.setFieldValue("settlementEntity", customerName)
        form.setFieldValue("paymentTerms", "")
      } else {
        form.setFieldValue("contractPdf", null)
      }
    },
    [customerQuery.data, form]
  )

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 pb-6 md:p-4">
      <PageHeader
        title="新建销售单"
        description="卡券与实物/服务共用统一销售单；创建后业务性质不可修改。"
        breadcrumbs={[
          { id: "sales", label: "销售", href: "/sales/orders" },
          { id: "orders", label: "销售单", href: "/sales/orders" },
          { id: "create", label: "新建", current: true },
        ]}
        status={{ label: "未创建", tone: "neutral" }}
        actions={
          <Button
            type="button"
            variant="outline"
            render={<Link href="/sales/orders" />}
          >
            取消并返回
          </Button>
        }
      />

      {contractsQuery.isError ? (
        <Alert variant="destructive">
          <CircleAlertIcon aria-hidden="true" />
          <AlertTitle>有效合同加载失败</AlertTitle>
          <AlertDescription>
            暂时不能选择已有合同；仍可切换为同步上传合同 PDF。
          </AlertDescription>
        </Alert>
      ) : null}

      {createMutation.isError ? (
        <Alert variant="destructive">
          <CircleAlertIcon aria-hidden="true" />
          <AlertTitle>销售单未创建</AlertTitle>
          <AlertDescription>{errorMessage(createMutation.error)}</AlertDescription>
        </Alert>
      ) : null}

      <form
        className="space-y-3"
        onSubmit={(event) => {
          event.preventDefault()
          event.stopPropagation()
          void form.handleSubmit()
        }}
      >
        <Card>
          <CardHeader className="border-b border-border">
            <CardTitle>销售单草稿</CardTitle>
            <CardDescription>
              合同、客户和结算主体按精确修订保存快照；金额由提交接口重新计算。
            </CardDescription>
          </CardHeader>
          <CardContent>
            <DocumentSection
              title="客户与合同"
              description="选择已有有效合同，或随本单同步上传一份签署合同 PDF。"
            >
              <form.AppField name="contractSource">
                {(field) => (
                  <field.SelectField
                    label="合同来源"
                    options={CONTRACT_SOURCE_OPTIONS}
                    onValueChange={handleContractSourceChange}
                    description="两种方式互斥；上传成功后合同档案与销售单一次完成关联。"
                  />
                )}
              </form.AppField>
              <form.Subscribe selector={(state) => state.values.contractSource}>
                {(contractSource) =>
                  contractSource === "existing" ? (
                    <div className="mt-4 grid gap-4 lg:grid-cols-2">
                      <form.AppField name="contractId">
                        {(field) => (
                          <field.SelectField
                            label="已有有效合同"
                            placeholder={
                              contractsQuery.isPending
                                ? "正在加载有效合同…"
                                : contractOptions.length > 0
                                  ? "请选择合同"
                                  : "当前客户暂无可用合同"
                            }
                            options={contractOptions}
                            disabled={
                              contractsQuery.isPending || contractsQuery.isError
                            }
                            onValueChange={handleContractChange}
                          />
                        )}
                      </form.AppField>
                      <form.AppField name="contractRevisionLabel">
                        {(field) => (
                          <field.TextField
                            label="合同精确版本"
                            disabled
                            description={
                              contractQuery.data
                                ? `${contractQuery.data.contractNo} · 当前版本由合同中心校验`
                                : "选择合同后自动带出"
                            }
                          />
                        )}
                      </form.AppField>
                      <form.AppField name="customerName">
                        {(field) => <field.TextField label="客户" disabled />}
                      </form.AppField>
                      <form.AppField name="settlementEntity">
                        {(field) => <field.TextField label="结算主体" disabled />}
                      </form.AppField>
                    </div>
                  ) : (
                    <div className="mt-4 grid gap-4 lg:grid-cols-2">
                      <div className="lg:col-span-2">
                        <form.AppField name="contractPdf">
                          {(field) => (
                            <field.PdfUploadField label="合同电子档" />
                          )}
                        </form.AppField>
                      </div>
                      <form.AppField name="uploadedContractNo">
                        {(field) => <field.TextField label="合同编号" />}
                      </form.AppField>
                      <form.AppField name="uploadedSignedAt">
                        {(field) => (
                          <field.TextField label="签订日期" type="date" />
                        )}
                      </form.AppField>
                      <form.AppField name="customerName">
                        {(field) => <field.TextField label="客户" />}
                      </form.AppField>
                      <form.AppField name="settlementEntity">
                        {(field) => <field.TextField label="结算主体" />}
                      </form.AppField>
                      <form.AppField name="uploadedValidFrom">
                        {(field) => (
                          <field.TextField label="合同有效期起" type="date" />
                        )}
                      </form.AppField>
                      <form.AppField name="uploadedValidTo">
                        {(field) => (
                          <field.TextField label="合同有效期止" type="date" />
                        )}
                      </form.AppField>
                    </div>
                  )
                }
              </form.Subscribe>
            </DocumentSection>

            <DocumentSection
              title="商业约定"
              description="业务性质创建后锁定；正式单的后续商业变化必须走销售变更单。"
            >
              <div className="grid gap-4 lg:grid-cols-3">
                <form.AppField name="nature">
                  {(field) => (
                    <field.SelectField
                      label="业务性质"
                      options={NATURE_OPTIONS}
                      description="切换业务性质会重置当前明细。"
                      onValueChange={(value) => {
                        const nature = value as SalesOrderNature
                        form.setFieldValue(
                          "taxRatePercent",
                          nature === "card_voucher" ? "6.00" : "13.00"
                        )
                        form.setFieldValue("lineItems", [createEmptyLine(nature)])
                      }}
                    />
                  )}
                </form.AppField>
                <form.AppField name="ownerName">
                  {(field) => <field.TextField label="负责销售" />}
                </form.AppField>
                <form.AppField name="welfareScene">
                  {(field) => (
                    <field.TextField
                      label="福利场景"
                      placeholder="如年节礼包、慰问品、消费金"
                    />
                  )}
                </form.AppField>
                <form.AppField name="paymentTerms">
                  {(field) => (
                    <field.TextField
                      label="付款条件"
                      description="默认带出合同约定，可补充本单执行口径。"
                    />
                  )}
                </form.AppField>
                <form.AppField name="fulfillmentDeadline">
                  {(field) => (
                    <field.TextField label="全单履约期限" type="date" />
                  )}
                </form.AppField>
                <form.AppField name="taxRatePercent">
                  {(field) => (
                    <field.TextField
                      label="税率（%）"
                      type="number"
                      inputClassName="num"
                      description="页面仅预估，提交后由服务端重算。"
                    />
                  )}
                </form.AppField>
              </div>
            </DocumentSection>

            <DocumentSection
              title="销售内容"
              description="非卡券可增加多条明细；卡券销售版本必须恰好一条卡券明细。"
              action={<Badge variant="outline">统一销售单明细</Badge>}
            >
              <form.Subscribe selector={(state) => state.values}>
                {(values) => {
                  const nature = values.nature
                  const columns: EditableLineItemColumn<SalesOrderDraftLineInput>[] = [
                    {
                      id: "item",
                      header: "销售项目",
                      renderValue: ({ item }) => item.name,
                      renderEditor: ({ rowIndex }) => (
                        <div className="grid min-w-48 gap-2">
                          <form.AppField name={`lineItems[${rowIndex}].name`}>
                            {(field) => (
                              <field.TextField
                                label="销售项目"
                                hideLabel
                                placeholder={
                                  nature === "card_voucher" ? "卡券类目" : "商品或服务"
                                }
                              />
                            )}
                          </form.AppField>
                          <form.AppField name={`lineItems[${rowIndex}].sku`}>
                            {(field) => (
                              <field.TextField label="SKU / 类目编码" hideLabel placeholder="SKU / 类目编码" />
                            )}
                          </form.AppField>
                        </div>
                      ),
                    },
                    {
                      id: "quantity",
                      header: "数量 / 单位",
                      numeric: true,
                      renderValue: ({ item }) => `${item.quantity} ${item.unit}`,
                      renderEditor: ({ rowIndex }) => (
                        <div className="grid min-w-28 gap-2">
                          <form.AppField name={`lineItems[${rowIndex}].quantity`}>
                            {(field) => (
                              <field.TextField label="数量" hideLabel type="number" inputClassName="num" />
                            )}
                          </form.AppField>
                          <form.AppField name={`lineItems[${rowIndex}].unit`}>
                            {(field) => (
                              <field.TextField label="单位" hideLabel placeholder="单位" />
                            )}
                          </form.AppField>
                        </div>
                      ),
                    },
                    {
                      id: "unitPrice",
                      header: "含税单价",
                      numeric: true,
                      align: "end",
                      renderValue: ({ item }) => item.unitPriceGross,
                      renderEditor: ({ rowIndex }) => (
                        <form.AppField name={`lineItems[${rowIndex}].unitPriceGross`}>
                          {(field) => (
                            <field.TextField
                              label="含税单价"
                              hideLabel
                              type="number"
                              inputClassName="num min-w-28 text-right"
                            />
                          )}
                        </form.AppField>
                      ),
                    },
                    nature === "card_voucher"
                      ? {
                          id: "voucher",
                          header: "卡券条件",
                          renderValue: ({ item }) => item.cardForm,
                          renderEditor: ({ rowIndex }) => (
                            <div className="grid min-w-40 gap-2">
                              <form.AppField name={`lineItems[${rowIndex}].faceValue`}>
                                {(field) => (
                                  <field.TextField
                                    label="面值"
                                    hideLabel
                                    type="number"
                                    placeholder="面值"
                                    inputClassName="num"
                                  />
                                )}
                              </form.AppField>
                              <form.AppField name={`lineItems[${rowIndex}].giftRate`}>
                                {(field) => (
                                  <field.TextField
                                    label="配赠率（%）"
                                    hideLabel
                                    type="number"
                                    placeholder="配赠率（%）"
                                    inputClassName="num"
                                  />
                                )}
                              </form.AppField>
                              <form.AppField name={`lineItems[${rowIndex}].cardForm`}>
                                {(field) => (
                                  <field.SelectField
                                    label="卡形态"
                                    hideLabel
                                    options={CARD_FORM_OPTIONS}
                                  />
                                )}
                              </form.AppField>
                            </div>
                          ),
                        }
                      : {
                          id: "fulfillment",
                          header: "履约约定",
                          renderValue: ({ item }) => item.fulfillmentMode,
                          renderEditor: ({ rowIndex }) => (
                            <div className="grid min-w-40 gap-2">
                              <form.AppField name={`lineItems[${rowIndex}].fulfillmentMode`}>
                                {(field) => (
                                  <field.SelectField
                                    label="履约方式"
                                    hideLabel
                                    options={FULFILLMENT_OPTIONS}
                                  />
                                )}
                              </form.AppField>
                              <form.AppField name={`lineItems[${rowIndex}].dueDate`}>
                                {(field) => (
                                  <field.TextField label="交付日期" hideLabel type="date" />
                                )}
                              </form.AppField>
                            </div>
                          ),
                        },
                    {
                      id: "amount",
                      header: "含税小计",
                      numeric: true,
                      align: "end",
                      renderValue: ({ item }) => (
                        <MoneyValue
                          value={(
                            Number(item.quantity || 0) * Number(item.unitPriceGross || 0)
                          ).toFixed(2)}
                          taxBasis="gross"
                        />
                      ),
                    },
                  ]
                  const totals = calculateTotals(values.lineItems, values.taxRatePercent)

                  return (
                    <>
                      <EditableLineItemTable
                        items={values.lineItems}
                        columns={columns}
                        getRowId={(item) => item.rowKey}
                        caption="销售单创建明细"
                        emptyContent="至少需要一条销售明细。"
                        addLabel="新增销售明细"
                        addDisabledReason={
                          nature === "card_voucher"
                            ? "卡券销售单每个版本必须恰好一条明细"
                            : undefined
                        }
                        onAddItem={
                          nature === "physical_service"
                            ? () => form.pushFieldValue("lineItems", createEmptyLine(nature))
                            : undefined
                        }
                        onRemoveItem={(_item, _rowId, rowIndex) => {
                          void form.removeFieldValue("lineItems", rowIndex)
                        }}
                        getRemoveDisabledReason={() =>
                          values.lineItems.length === 1
                            ? "销售单至少保留一条明细"
                            : nature === "card_voucher"
                              ? "卡券销售单必须保留唯一明细"
                              : undefined
                        }
                      />

                      <div className="mt-5 grid gap-4 lg:grid-cols-2">
                        <form.AppField name="remark">
                          {(field) => (
                            <field.TextareaField
                              label="内部说明"
                              placeholder="补充客户确认、交付或内部协同说明"
                              rows={4}
                            />
                          )}
                        </form.AppField>
                        <Alert variant="info">
                          <FilePlus2Icon aria-hidden="true" />
                          <AlertTitle>
                            {nature === "card_voucher"
                              ? "提交后进入两级审批"
                              : "提交后进入采购二次确认"}
                          </AlertTitle>
                          <AlertDescription>
                            {nature === "card_voucher"
                              ? "依次由销售领导和运营审批；运营通过后才生效并形成应收。"
                              : "提交会冻结当前内容并创建采购二次确认任务，不会在前端乐观标记生效。"}
                          </AlertDescription>
                        </Alert>
                      </div>

                      <StickyTotalBar
                        className="mt-5"
                        items={[
                          {
                            id: "gross",
                            label: "含税金额",
                            value: <MoneyValue value={totals.gross} taxBasis="gross" />,
                          },
                          {
                            id: "net",
                            label: "不含税金额",
                            value: <MoneyValue value={totals.net} taxBasis="net" />,
                          },
                          {
                            id: "tax",
                            label: "税额",
                            value: <MoneyValue value={totals.tax} />,
                          },
                        ]}
                        note={`按税率 ${values.taxRatePercent || "0"}% 预估；正式金额由提交接口重算。`}
                        actions={
                          <form.AppForm>
                            <Button
                              type="button"
                              variant="outline"
                              render={<Link href="/sales/orders" />}
                            >
                              取消
                            </Button>
                            <form.SubmitButton
                              variant="outline"
                              label="保存草稿"
                              pendingLabel="正在创建…"
                              onClick={() => {
                                submitIntentRef.current = "SAVE_DRAFT"
                              }}
                            />
                            <form.SubmitButton
                              label="提交正式流程"
                              pendingLabel="正在提交…"
                              onClick={() => {
                                submitIntentRef.current = "SUBMIT"
                              }}
                            >
                              <PlusIcon data-icon="inline-start" aria-hidden="true" />
                              提交正式流程
                            </form.SubmitButton>
                          </form.AppForm>
                        }
                      />
                    </>
                  )
                }}
              </form.Subscribe>
            </DocumentSection>
          </CardContent>
        </Card>
      </form>
    </div>
  )
}
