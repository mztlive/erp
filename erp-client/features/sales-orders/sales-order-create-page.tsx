"use client"

import * as React from "react"
import { useRouter } from "next/navigation"
import { CircleAlertIcon, CircleCheckIcon, PlusIcon } from "lucide-react"
import { z } from "zod"

import {
  ContractCombobox,
  DiscardConfirmDialog,
  EditableLineItemTable,
  MoneyValue,
  OwnerCombobox,
  PageHeader,
  ProductCombobox,
  StickyTotalBar,
  ValidationSummary,
  type EditableLineItemColumn,
} from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { useSelector } from "@tanstack/react-form"
import type { StandardSchemaV1Issue } from "@tanstack/react-form"
import {
  PAYMENT_TERM_OPTIONS,
  paymentTermLabel,
} from "@/lib/business-options"
import {
  compareDecimal,
  multiplyFixed,
  splitGrossByPercentRate,
  sumFixed,
} from "@/lib/fixed-decimal"
import {
  Alert,
  AlertDescription,
  AlertTitle,
} from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  Field,
  FieldError,
  FieldLabel,
} from "@/components/ui/field"
import { ContractUploadDialog } from "@/features/contracts/contract-upload-dialog"
import {
  useContractCenterQuery,
  useContractsForNewSalesOrderQuery,
} from "@/features/contracts/queries"
import type { UploadContractPdfResult } from "@/features/contracts/types"
import { useMasterDataListQuery } from "@/features/master-data/queries"
import { useCreateSalesOrderMutation } from "@/features/sales-orders/queries"
import { useOwnerOptionsQuery } from "@/hooks/use-options"
import type {
  CreateSalesOrderInput,
  SalesOrderCreateIntent,
  SalesOrderDraftLineInput,
  SalesOrderNature,
} from "@/features/sales-orders/types"

const decimalInput = (
  label: string,
  maxScale: number,
  options: { positive?: boolean } = {}
) =>
  z
    .string()
    .trim()
    .regex(
      new RegExp(`^\\d+(?:\\.\\d{1,${maxScale}})?$`),
      `${label}最多保留 ${maxScale} 位小数`
    )
    .refine(
      (value) => !options.positive || /[1-9]/.test(value),
      `${label}必须大于 0`
    )

function decimalAtMost(value: string, maximum: string, maxScale: number) {
  try {
    return compareDecimal(value, maximum, maxScale) <= 0
  } catch {
    return false
  }
}

const draftLineSchema = z.object({
  rowKey: z.string().min(1),
  name: z.string().trim().min(1, "请输入销售项目"),
  sku: z.string(),
  quantity: decimalInput("数量", 6, { positive: true }),
  unit: z.string().trim().min(1, "请输入单位"),
  unitPriceGross: decimalInput("含税单价", 4, { positive: true }),
  fulfillmentMode: z.string(),
  dueDate: z.string(),
  faceValue: z.string(),
  giftRate: z.string(),
  cardForm: z.string(),
})

/** 草稿只要求「已选合同 + 至少一行明细」，明细内容允许不完整。 */
const draftRowSchema = z.object({
  rowKey: z.string().min(1),
  name: z.string(),
  sku: z.string(),
  quantity: z.string(),
  unit: z.string(),
  unitPriceGross: z.string(),
  fulfillmentMode: z.string(),
  dueDate: z.string(),
  faceValue: z.string(),
  giftRate: z.string(),
  cardForm: z.string(),
})

const createSalesOrderSchema = z
  .object({
    contractId: z.string(),
    requestedContractRevisionId: z.string(),
    contractRevisionLabel: z.string(),
    customerId: z.string(),
    customerName: z.string(),
    settlementPartyId: z.string(),
    settlementEntity: z.string(),
    nature: z.enum(["physical_service", "card_voucher"]),
    ownerUserId: z.string(),
    ownerName: z.string().trim().min(1, "请选择负责销售"),
    welfareScene: z.string().trim().min(1, "请输入福利场景"),
    paymentTerms: z.string().trim().min(1, "请选择付款条件"),
    fulfillmentDeadline: z.string().min(1, "请选择履约期限"),
    taxRatePercent: decimalInput("税率", 6).refine(
      (value) => decimalAtMost(value, "100", 6),
      "税率不能超过 100%"
    ),
    remark: z.string(),
    lineItems: z.array(draftLineSchema).min(1, "至少需要一条销售明细"),
  })
  .superRefine((value, context) => {
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
        message: "正在同步合同信息，请稍后再提交",
      })
    }
    if (!value.customerName.trim()) {
      context.addIssue({
        code: "custom",
        path: ["customerName"],
        message: "正在同步客户信息，请稍后再提交",
      })
    }
    if (!value.settlementEntity.trim()) {
      context.addIssue({
        code: "custom",
        path: ["settlementEntity"],
        message: "正在同步结算主体信息，请稍后再提交",
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
        if (!/^\d+(?:\.\d{1,2})?$/.test(line.faceValue) || !/[1-9]/.test(line.faceValue)) {
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

/** 保存草稿：宽松校验（合同已选 + 至少一行明细），提交才走全量 schema。 */
const draftSalesOrderSchema = z
  .object({
    contractId: z.string(),
    requestedContractRevisionId: z.string(),
    contractRevisionLabel: z.string(),
    customerId: z.string(),
    customerName: z.string(),
    settlementPartyId: z.string(),
    settlementEntity: z.string(),
    nature: z.enum(["physical_service", "card_voucher"]),
    ownerUserId: z.string(),
    ownerName: z.string(),
    welfareScene: z.string(),
    paymentTerms: z.string(),
    fulfillmentDeadline: z.string(),
    taxRatePercent: z.string(),
    remark: z.string(),
    lineItems: z.array(draftRowSchema).min(1, "至少需要一条销售明细"),
  })
  .superRefine((value, context) => {
    if (!value.contractId) {
      context.addIssue({
        code: "custom",
        path: ["contractId"],
        message: "请选择已有有效合同",
      })
    }
  })

function validateSalesOrderForm(
  value: CreateSalesOrderFormValues,
  intent: SalesOrderCreateIntent
): StandardSchemaV1Issue[] | undefined {
  const schema =
    intent === "SAVE_DRAFT" ? draftSalesOrderSchema : createSalesOrderSchema
  const result = schema.safeParse(value)
  if (result.success) return undefined
  return result.error.issues as unknown as StandardSchemaV1Issue[]
}

const NATURE_OPTIONS = [
  { value: "physical_service", label: "实物与服务" },
  { value: "card_voucher", label: "卡券" },
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
    /** 非卡券单位随 SKU 基础单位带出；卡券固定为张。建单页不可改。 */
    unit: nature === "card_voucher" ? "张" : "",
    unitPriceGross: "0.00",
    /**
     * 建单页不提供仓发/直发选择；履约方式由后续采购二次确认写入正式结论。
     * 提交仍带占位值以满足契约，服务端以确认结果为准。
     */
    fulfillmentMode: nature === "physical_service" ? "公司仓发" : "",
    dueDate: "",
    faceValue: "",
    giftRate: "0.00",
    cardForm: nature === "card_voucher" ? "电子卡" : "",
  }
}

/** 明细行是否已有实质内容（用于切换业务性质前的防丢失确认）。 */
function hasMeaningfulLines(
  lineItems: readonly SalesOrderDraftLineInput[]
): boolean {
  return lineItems.some(
    (line) =>
      line.name.trim() !== "" ||
      line.sku.trim() !== "" ||
      line.quantity !== "1" ||
      line.unitPriceGross !== "0.00" ||
      line.faceValue !== "" ||
      line.giftRate !== "0.00" ||
      line.dueDate !== ""
  )
}

function calculateTotals(
  lineItems: readonly SalesOrderDraftLineInput[],
  taxRatePercent: string
) {
  try {
    const lines = lineItems.map((line) => {
      const gross = multiplyFixed(line.quantity || "0", line.unitPriceGross || "0", {
        leftMaxScale: 6,
        rightMaxScale: 4,
        outputScale: 2,
      })
      return splitGrossByPercentRate(gross, taxRatePercent || "0")
    })
    return {
      gross: sumFixed(lines.map((line) => line.gross), { maxScale: 2, outputScale: 2 }),
      net: sumFixed(lines.map((line) => line.net), { maxScale: 2, outputScale: 2 }),
      tax: sumFixed(lines.map((line) => line.tax), { maxScale: 2, outputScale: 2 }),
    }
  } catch {
    return { gross: "0.00", net: "0.00", tax: "0.00" }
  }
}

function errorMessage(error: unknown): string {
  if (!(error instanceof Error)) return "创建失败，请重试。"
  const messages: Record<string, string> = {
    CONTRACT_NOT_SELECTABLE: "所选合同已不可用于新建销售单，请刷新后重选。",
    CONTRACT_REVISION_NOT_FOUND: "所选合同修订不存在，请刷新合同后重试。",
    CONTRACT_REVISION_NOT_CURRENT: "新销售单只能引用合同当前有效修订。",
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
  const productsQuery = useMasterDataListQuery({
    resource: "products",
    lifecycleStatus: "enabled",
  })
  const createMutation = useCreateSalesOrderMutation()
  const { data: ownerOptions } = useOwnerOptionsQuery()
  const [selectedContractId, setSelectedContractId] =
    React.useState(initialContractId)
  const [uploadOpen, setUploadOpen] = React.useState(false)
  const preferredRevisionRef = React.useRef(initialContractRevisionId)
  const submitIntentRef = React.useRef<SalesOrderCreateIntent>("SAVE_DRAFT")
  const contractQuery = useContractCenterQuery(selectedContractId)

  const productComboboxItems = React.useMemo(
    () =>
      (productsQuery.data?.rows ?? []).map((p) => {
        const baseUnit =
          p.keyFacts.find((f) => f.label === "基础单位")?.value ?? ""
        return {
          productId: p.stableId,
          sku: p.stableNo,
          name: p.name,
          statusLabel: p.lifecycleStatusLabel,
          statusTone: p.lifecycleTone,
          baseUnit,
          description: baseUnit ? `单位 ${baseUnit}` : undefined,
        }
      }),
    [productsQuery.data?.rows]
  )

  /**
   * 必须稳定：createEmptyLine 每次生成新 rowKey。
   * useAppForm 在 layout effect 里会 deep-compare defaultValues，
   * 未 touch 时若每次渲染都变，会 setState → 重渲染 → 死循环
   *（Maximum update depth exceeded @ Field）。
   */
  const defaultValues = React.useMemo(
    () =>
      ({
        contractId: initialContractId,
        requestedContractRevisionId: initialContractRevisionId,
        contractRevisionLabel: "",
        customerId: initialCustomerId,
        customerName: "",
        settlementPartyId: "",
        settlementEntity: "",
        nature: initialNature,
        ownerUserId: "",
        ownerName: "",
        welfareScene: "",
        paymentTerms: "",
        fulfillmentDeadline: "",
        taxRatePercent: initialNature === "card_voucher" ? "6.00" : "13.00",
        remark: "",
        lineItems: [createEmptyLine(initialNature)],
      }) satisfies CreateSalesOrderFormValues,
    [
      initialContractId,
      initialContractRevisionId,
      initialCustomerId,
      initialNature,
    ]
  )

  const form = useAppForm({
    defaultValues,
    validators: {
      onSubmit: ({ value }) =>
        validateSalesOrderForm(value, submitIntentRef.current),
    },
    onSubmit: async ({ value }) => {
      const command: CreateSalesOrderInput = {
        contract: {
          contractId: value.contractId,
          requestedContractRevisionId: value.requestedContractRevisionId,
        },
        nature: value.nature,
        ownerName: value.ownerName,
        welfareScene: value.welfareScene,
        paymentTerms: paymentTermLabel(value.paymentTerms) || value.paymentTerms,
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
      if (submitIntentRef.current === "SAVE_DRAFT") {
        setDraftSaved({
          documentNumber: result.documentNumber,
          savedAt: new Date(),
        })
        return
      }
      form.reset()
      router.push(`/sales/orders/${result.salesOrderId}`)
    },
  })

  const dirty = useSelector(form.store, (state) => state.isDirty)
  const [discardOpen, setDiscardOpen] = React.useState(false)
  const [draftSaved, setDraftSaved] = React.useState<{
    documentNumber: string
    savedAt: Date
  } | null>(null)
  const [pendingNature, setPendingNature] =
    React.useState<SalesOrderNature | null>(null)

  React.useEffect(() => {
    if (!dirty) return
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      e.preventDefault()
      e.returnValue = "当前输入尚未提交，刷新后将丢失。"
    }
    window.addEventListener("beforeunload", onBeforeUnload)
    return () => window.removeEventListener("beforeunload", onBeforeUnload)
  }, [dirty])

  const contractComboboxItems = React.useMemo(
    () =>
      (contractsQuery.data ?? [])
        .filter(
          (contract) =>
            !initialCustomerId ||
            contract.customer.customerId === initialCustomerId
        )
        .map((c) => ({
          contractId: c.contractId,
          contractNo: c.contractNo,
          customerName: c.customer.displayName,
          statusLabel: c.statusLabel,
          statusTone: c.statusTone,
          revisionNo: c.revisionNo,
          validTo: c.validTo,
          settlementPartyName: c.settlementParty.displayName,
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
    form.setFieldValue("customerId", contract.customer.id)
    form.setFieldValue("customerName", contract.customer.displayName)
    form.setFieldValue(
      "settlementPartyId",
      contract.currentRevision.settlementParty.id
    )
    form.setFieldValue(
      "settlementEntity",
      contract.currentRevision.settlementParty.displayName
    )
    const ownerLabel = contract.ownerLabel.split(" · ")[0] ?? ""
    const ownerMatch = (ownerOptions ?? []).find(
      (o) => o.displayName === ownerLabel
    )
    form.setFieldValue("ownerUserId", ownerMatch?.userId ?? "")
    form.setFieldValue("ownerName", ownerLabel)
    const termLabel = contract.currentRevision.paymentTermSnapshot.label
    const termMatch = PAYMENT_TERM_OPTIONS.find(
      (o) => o.label === termLabel || o.value === termLabel
    )
    form.setFieldValue("paymentTerms", termMatch?.value ?? "CONTRACT")
  }, [contractQuery.data, form, ownerOptions])

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

  const handleUploadSuccess = React.useCallback(
    async (result: UploadContractPdfResult) => {
      await contractsQuery.refetch()
      preferredRevisionRef.current = result.revisionId
      setSelectedContractId(result.contractId)
      form.setFieldValue("contractId", result.contractId)
      form.setFieldValue("requestedContractRevisionId", "")
      form.setFieldValue("contractRevisionLabel", "")
      form.setFieldValue("customerName", "")
      form.setFieldValue("settlementEntity", "")
    },
    [contractsQuery, form]
  )

  const applyNature = React.useCallback(
    (nature: SalesOrderNature) => {
      form.setFieldValue(
        "taxRatePercent",
        nature === "card_voucher" ? "6.00" : "13.00"
      )
      form.setFieldValue("lineItems", [createEmptyLine(nature)])
      setDraftSaved(null)
    },
    [form]
  )

  /** 明细表根路径校验错误（如卡券仅一条）在明细区汇总展示。 */
  const lineItemIssues = useSelector(form.store, (state) => {
    return toFieldErrors(state.fieldMeta.lineItems?.errors ?? [])
      .filter((error) => Boolean(error?.message))
      .map((error, index) => ({
        id: `line-items-${index}`,
        label: "销售明细",
        message: error!.message!,
        targetId: "sales-line-items-section",
      }))
  })

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 pb-8 md:gap-5 md:p-5">
      <PageHeader
        density="compact"
        title="新建销售单"
        description="创建后业务性质不可修改；金额以提交后系统计算为准。"
        breadcrumbs={[
          { id: "sales", label: "销售", href: "/sales/orders" },
          { id: "orders", label: "销售单", href: "/sales/orders" },
          { id: "create", label: "新建", current: true },
        ]}
        status={{ label: "未创建", tone: "neutral" }}
      />

      {contractsQuery.isError ? (
        <Alert variant="destructive">
          <CircleAlertIcon aria-hidden="true" />
          <AlertTitle>有效合同加载失败</AlertTitle>
          <AlertDescription>
            暂时不能选择合同。可点击加号上传新合同，或刷新页面后重试。
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

      {draftSaved ? (
        <Alert variant="success">
          <CircleCheckIcon aria-hidden="true" />
          <AlertTitle>草稿已保存</AlertTitle>
          <AlertDescription>
            销售单 {draftSaved.documentNumber} 已保存为草稿（
            {draftSaved.savedAt.toLocaleTimeString("zh-CN")}）。当前内容仍保留在本页，可继续完善后提交；草稿也会出现在销售单列表中。
          </AlertDescription>
        </Alert>
      ) : null}

      <form
        onSubmit={(event) => {
          event.preventDefault()
          event.stopPropagation()
          void form.handleSubmit()
        }}
      >
        <div className="grid items-start gap-4 xl:grid-cols-[minmax(0,1fr)_17.5rem] xl:gap-5">
          <div className="min-w-0 overflow-hidden rounded-xl border border-border bg-card">
            <section className="border-b border-border p-4 md:p-5 lg:p-6">
              <div className="mb-4 flex items-center justify-between gap-2">
                <h2 className="font-heading text-sm font-semibold">单据头</h2>
                <span className="text-xs text-muted-foreground">
                  合同与商业约定
                </span>
              </div>

              <div className="space-y-5">
                <form.Subscribe
                  selector={(state) => ({
                    contractRevisionLabel: state.values.contractRevisionLabel,
                    customerName: state.values.customerName,
                    settlementEntity: state.values.settlementEntity,
                  })}
                >
                  {({
                    contractRevisionLabel,
                    customerName,
                    settlementEntity,
                  }) => (
                    <div className="space-y-3">
                      <form.AppField name="contractId">
                        {(field) => {
                          const isInvalid =
                            field.state.meta.isTouched && !field.state.meta.isValid
                          const errors = toFieldErrors(field.state.meta.errors)
                          return (
                            <Field data-invalid={isInvalid || undefined}>
                              <FieldLabel htmlFor="contractId">
                                有效合同
                              </FieldLabel>
                              <div className="flex items-start gap-2">
                                <div className="min-w-0 flex-1">
                                  <ContractCombobox
                                    value={field.state.value || undefined}
                                    onValueChange={(id) => {
                                      const next = id ?? ""
                                      field.handleChange(next)
                                      handleContractChange(next)
                                    }}
                                    contracts={contractComboboxItems}
                                    disabled={
                                      contractsQuery.isPending ||
                                      contractsQuery.isError
                                    }
                                    loading={contractsQuery.isPending}
                                    placeholder={
                                      contractsQuery.isPending
                                        ? "正在加载有效合同…"
                                        : contractComboboxItems.length > 0
                                          ? "搜索合同编号或客户"
                                          : "暂无可用合同，请点加号上传"
                                    }
                                    emptyLabel={
                                      contractComboboxItems.length > 0
                                        ? "没有符合条件的合同"
                                        : "暂无可用合同，请点加号上传"
                                    }
                                  />
                                </div>
                                <Button
                                  type="button"
                                  variant="outline"
                                  size="icon"
                                  className="shrink-0"
                                  aria-label="上传合同 PDF"
                                  title="上传合同 PDF"
                                  onClick={() => setUploadOpen(true)}
                                >
                                  <PlusIcon aria-hidden="true" />
                                </Button>
                              </div>
                              {isInvalid ? (
                                <FieldError errors={errors} />
                              ) : null}
                            </Field>
                          )
                        }}
                      </form.AppField>
                      {contractRevisionLabel ||
                      customerName ||
                      settlementEntity ? (
                        <div className="flex flex-wrap items-center gap-2 rounded-lg border border-dashed border-border bg-muted/40 px-3 py-2.5 text-xs">
                          {contractRevisionLabel ? (
                            <Badge variant="outline" className="font-normal">
                              {contractRevisionLabel}
                            </Badge>
                          ) : null}
                          {customerName ? (
                            <span className="text-muted-foreground">
                              客户{" "}
                              <span className="text-foreground">
                                {customerName}
                              </span>
                            </span>
                          ) : null}
                          {settlementEntity ? (
                            <span className="text-muted-foreground">
                              · 结算{" "}
                              <span className="text-foreground">
                                {settlementEntity}
                              </span>
                            </span>
                          ) : null}
                          {contractQuery.isFetching ? (
                            <span className="text-muted-foreground">
                              加载中…
                            </span>
                          ) : null}
                        </div>
                      ) : (
                        <p className="text-xs leading-relaxed text-muted-foreground">
                          选择合同后自动带出版本、客户与结算主体；无合同时点加号上传 PDF。
                        </p>
                      )}
                    </div>
                  )}
                </form.Subscribe>

                <div className="grid gap-x-4 gap-y-4 sm:grid-cols-2 lg:grid-cols-3">
                  <form.AppField name="nature">
                    {(field) => (
                      <field.SelectField
                        label="业务性质"
                        options={NATURE_OPTIONS}
                        onValueChange={(value) => {
                          const nature = value as SalesOrderNature
                          if (nature === field.state.value) return
                          const lines = form.state.values.lineItems
                          if (hasMeaningfulLines(lines)) {
                            setPendingNature(nature)
                            return
                          }
                          applyNature(nature)
                        }}
                      />
                    )}
                  </form.AppField>
                  <form.AppField
                    name="ownerUserId"
                    validators={{
                      onBlur: z.string().min(1, "请选择负责销售"),
                    }}
                  >
                    {(field) => {
                      const isInvalid =
                        field.state.meta.isTouched && !field.state.meta.isValid
                      const errors = toFieldErrors(field.state.meta.errors)
                      return (
                        <Field data-invalid={isInvalid || undefined}>
                          <FieldLabel htmlFor="ownerUserId">负责销售</FieldLabel>
                          <OwnerCombobox
                            value={field.state.value || undefined}
                            onValueChange={(id) => {
                              const next = id ?? ""
                              field.handleChange(next)
                              form.setFieldValue(
                                "ownerName",
                                ownerOptions?.find(
                                  (o) => o.userId === next
                                )?.displayName ?? ""
                              )
                            }}
                            owners={ownerOptions ?? []}
                            placeholder="搜索负责人"
                          />
                          {isInvalid ? <FieldError errors={errors} /> : null}
                        </Field>
                      )
                    }}
                  </form.AppField>
                  <form.AppField
                    name="welfareScene"
                    validators={{
                      onBlur: z.string().trim().min(1, "请输入福利场景"),
                    }}
                  >
                    {(field) => (
                      <field.TextField
                        label="福利场景"
                        placeholder="年节礼包、慰问品…"
                      />
                    )}
                  </form.AppField>
                  <form.AppField
                    name="paymentTerms"
                    validators={{
                      onBlur: z.string().min(1, "请选择付款条件"),
                    }}
                  >
                    {(field) => (
                      <field.SelectField
                        label="付款条件"
                        options={PAYMENT_TERM_OPTIONS}
                      />
                    )}
                  </form.AppField>
                  <form.AppField
                    name="fulfillmentDeadline"
                    validators={{
                      onBlur: z.string().min(1, "请选择履约期限"),
                    }}
                  >
                    {(field) => (
                      <field.DateField label="履约期限" />
                    )}
                  </form.AppField>
                  <form.AppField
                    name="taxRatePercent"
                    validators={{
                      onBlur: decimalInput("税率", 6).refine(
                        (value) => decimalAtMost(value, "100", 6),
                        "税率不能超过 100%"
                      ),
                    }}
                  >
                    {(field) => (
                      <field.TextField
                        label="税率（%）"
                        type="number"
                        inputClassName="num"
                      />
                    )}
                  </form.AppField>
                </div>
              </div>
            </section>

            <section
              id="sales-line-items-section"
              className="border-b border-border p-4 md:p-5 lg:p-6"
            >
              <div className="mb-4 flex items-center justify-between gap-2">
                <h2 className="font-heading text-sm font-semibold">销售明细</h2>
                <form.Subscribe selector={(state) => state.values.nature}>
                  {(nature) => (
                    <Badge variant="outline" className="font-normal">
                      {nature === "card_voucher"
                        ? "卡券 · 仅一条"
                        : "实物/服务 · 可多行"}
                    </Badge>
                  )}
                </form.Subscribe>
              </div>

              <form.Subscribe selector={(state) => state.values}>
                {(values) => {
                  const nature = values.nature
                  const columns: EditableLineItemColumn<SalesOrderDraftLineInput>[] =
                    [
                      {
                        id: "item",
                        header: "销售项目",
                        renderValue: ({ item }) => item.name,
                        renderEditor: ({ rowIndex }) =>
                          nature === "card_voucher" ? (
                            <div className="flex min-w-52 items-start gap-2">
                              <div className="min-w-0 flex-1">
                                <form.AppField
                                  name={`lineItems[${rowIndex}].name`}
                                >
                                  {(field) => (
                                    <field.TextField
                                      label="销售项目"
                                      hideLabel
                                      placeholder="卡券类目"
                                    />
                                  )}
                                </form.AppField>
                              </div>
                              <div className="w-24 shrink-0">
                                <form.AppField
                                  name={`lineItems[${rowIndex}].sku`}
                                >
                                  {(field) => (
                                    <field.TextField
                                      label="类目编码"
                                      hideLabel
                                      placeholder="编码"
                                    />
                                  )}
                                </form.AppField>
                              </div>
                            </div>
                          ) : (
                            <div className="min-w-48">
                              <form.AppField
                                name={`lineItems[${rowIndex}].sku`}
                              >
                                {(field) => {
                                  const productId =
                                    productComboboxItems.find(
                                      (p) => p.sku === field.state.value
                                    )?.productId ??
                                    productComboboxItems.find(
                                      (p) =>
                                        p.name ===
                                        values.lineItems[rowIndex]?.name
                                    )?.productId
                                  return (
                                    <ProductCombobox
                                      value={productId}
                                      onValueChange={(id) => {
                                        const product =
                                          productComboboxItems.find(
                                            (p) => p.productId === id
                                          )
                                        field.handleChange(product?.sku ?? "")
                                        form.setFieldValue(
                                          `lineItems[${rowIndex}].name`,
                                          product?.name ?? ""
                                        )
                                        form.setFieldValue(
                                          `lineItems[${rowIndex}].unit`,
                                          product?.baseUnit ?? ""
                                        )
                                      }}
                                      products={productComboboxItems}
                                      loading={productsQuery.isPending}
                                      placeholder="搜索 SKU 或商品名称"
                                    />
                                  )
                                }}
                              </form.AppField>
                            </div>
                          ),
                      },
                      {
                        id: "quantity",
                        header: "数量 / 单位",
                        numeric: true,
                        renderValue: ({ item }) =>
                          `${item.quantity} ${item.unit}`,
                        renderEditor: ({ rowIndex }) => {
                          const line = values.lineItems[rowIndex]
                          const unitLocked =
                            nature === "card_voucher" ||
                            Boolean(line?.sku?.trim())
                          return (
                            <div className="flex min-w-32 items-center gap-2">
                              <div className="w-20 shrink-0">
                                <form.AppField
                                  name={`lineItems[${rowIndex}].quantity`}
                                >
                                  {(field) => (
                                    <field.TextField
                                      label="数量"
                                      hideLabel
                                      type="number"
                                      inputClassName="num"
                                    />
                                  )}
                                </form.AppField>
                              </div>
                              <form.AppField
                                name={`lineItems[${rowIndex}].unit`}
                              >
                                {(field) => (
                                  <span
                                    className="min-w-8 shrink-0 text-sm text-muted-foreground"
                                    title={
                                      unitLocked
                                        ? nature === "card_voucher"
                                          ? "卡券单位固定为张"
                                          : "单位随 SKU 基础单位带出，不可改"
                                        : "选择 SKU 后带出基础单位"
                                    }
                                  >
                                    {field.state.value || "—"}
                                  </span>
                                )}
                              </form.AppField>
                            </div>
                          )
                        },
                      },
                      {
                        id: "unitPrice",
                        header: "含税单价",
                        numeric: true,
                        align: "end",
                        renderValue: ({ item }) => item.unitPriceGross,
                        renderEditor: ({ rowIndex }) => (
                          <form.AppField
                            name={`lineItems[${rowIndex}].unitPriceGross`}
                          >
                            {(field) => (
                              <field.TextField
                                label="含税单价"
                                hideLabel
                                type="number"
                                inputClassName="num min-w-24 text-right"
                              />
                            )}
                          </form.AppField>
                        ),
                      },
                      nature === "card_voucher"
                        ? {
                            id: "voucher",
                            header: "卡券条件",
                            renderValue: ({ item }) =>
                              [item.faceValue, item.giftRate, item.cardForm]
                                .filter(Boolean)
                                .join(" · "),
                            renderEditor: ({ rowIndex }) => (
                              <div className="flex min-w-56 items-start gap-2">
                                <div className="w-20 shrink-0">
                                  <form.AppField
                                    name={`lineItems[${rowIndex}].faceValue`}
                                  >
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
                                </div>
                                <div className="w-20 shrink-0">
                                  <form.AppField
                                    name={`lineItems[${rowIndex}].giftRate`}
                                  >
                                    {(field) => (
                                      <field.TextField
                                        label="配赠率（%）"
                                        hideLabel
                                        type="number"
                                        placeholder="配赠%"
                                        inputClassName="num"
                                      />
                                    )}
                                  </form.AppField>
                                </div>
                                <div className="min-w-20 flex-1">
                                  <form.AppField
                                    name={`lineItems[${rowIndex}].cardForm`}
                                  >
                                    {(field) => (
                                      <field.SelectField
                                        label="卡形态"
                                        hideLabel
                                        options={CARD_FORM_OPTIONS}
                                      />
                                    )}
                                  </form.AppField>
                                </div>
                              </div>
                            ),
                          }
                        : {
                            id: "fulfillment",
                            header: "交付日期",
                            renderValue: ({ item }) => item.dueDate || "—",
                            renderEditor: ({ rowIndex }) => (
                              <div className="min-w-32">
                                <form.AppField
                                  name={`lineItems[${rowIndex}].dueDate`}
                                >
                                  {(field) => (
                                    <field.DateField
                                      label="交付日期"
                                      hideLabel
                                    />
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
                            value={
                              calculateTotals([item], values.taxRatePercent)
                                .gross
                            }
                          />
                        ),
                      },
                    ]

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
                            ? () =>
                                form.pushFieldValue(
                                  "lineItems",
                                  createEmptyLine(nature)
                                )
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

                      {lineItemIssues.length > 0 ? (
                        <ValidationSummary
                          className="mt-4"
                          issues={lineItemIssues}
                          title={`明细共 ${lineItemIssues.length} 项待处理`}
                        />
                      ) : null}

                      <div className="mt-5">
                        <form.AppField name="remark">
                          {(field) => (
                            <field.TextareaField
                              label="内部说明"
                              placeholder="补充客户确认、交付或内部协同说明（可选）"
                              rows={3}
                            />
                          )}
                        </form.AppField>
                      </div>
                    </>
                  )
                }}
              </form.Subscribe>
            </section>

            <form.Subscribe selector={(state) => state.values}>
              {(values) => {
                const totals = calculateTotals(
                  values.lineItems,
                  values.taxRatePercent
                )
                const flowNote =
                  values.nature === "card_voucher"
                    ? "提交后进入销售领导 → 运营两级审批，运营通过后生效并形成应收。"
                    : "提交后内容锁定并进入采购二次确认；生效以确认通过为准。"
                return (
                  <StickyTotalBar
                    className="rounded-none border-0 border-t border-border px-4 py-4 shadow-none md:px-5 md:py-4"
                    items={[
                      {
                        id: "gross",
                        label: "含税金额",
                        value: (
                          <MoneyValue
                            value={totals.gross}
                            taxBasis="gross"
                          />
                        ),
                      },
                      {
                        id: "net",
                        label: "不含税金额",
                        value: (
                          <MoneyValue value={totals.net} taxBasis="net" />
                        ),
                      },
                      {
                        id: "tax",
                        label: "税额",
                        value: <MoneyValue value={totals.tax} />,
                      },
                    ]}
                    note={
                      <>
                        税率 {values.taxRatePercent || "0"}% 预估。{flowNote}
                      </>
                    }
                    actions={
                      <form.AppForm>
                        <Button
                          type="button"
                          variant="outline"
                          onClick={() => {
                            if (dirty) {
                              setDiscardOpen(true)
                              return
                            }
                            router.push("/sales/orders")
                          }}
                        >
                          取消
                        </Button>
                        <form.SubmitButton
                          variant="outline"
                          label="保存草稿"
                          pendingLabel="正在保存草稿…"
                          onClick={() => {
                            submitIntentRef.current = "SAVE_DRAFT"
                          }}
                        />
                        <form.SubmitButton
                          label="提交"
                          pendingLabel="正在提交…"
                          onClick={() => {
                            submitIntentRef.current = "SUBMIT"
                          }}
                        >
                          <PlusIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                          />
                          提交
                        </form.SubmitButton>
                      </form.AppForm>
                    }
                  />
                )
              }}
            </form.Subscribe>
          </div>

          <aside className="hidden xl:block">
            <form.Subscribe selector={(state) => state.values}>
              {(values) => {
                const totals = calculateTotals(
                  values.lineItems,
                  values.taxRatePercent
                )
                const natureLabel =
                  values.nature === "card_voucher" ? "卡券" : "实物/服务"
                const nextStep =
                  values.nature === "card_voucher"
                    ? "提交后进入销售领导 → 运营两级审批"
                    : "提交后进入采购二次确认"
                return (
                  <div className="sticky top-14 space-y-4 rounded-xl border border-border bg-card p-4">
                    <div>
                      <h2 className="font-heading text-sm font-semibold">
                        本单摘要
                      </h2>
                      <p className="mt-1 text-xs text-muted-foreground">
                        随填写实时更新
                      </p>
                    </div>
                    <dl className="space-y-2.5 text-xs">
                      <div className="flex justify-between gap-2">
                        <dt className="text-muted-foreground">合同</dt>
                        <dd className="max-w-[10rem] truncate text-right font-medium">
                          {values.contractRevisionLabel || "未选择"}
                        </dd>
                      </div>
                      <div className="flex justify-between gap-2">
                        <dt className="text-muted-foreground">客户</dt>
                        <dd className="max-w-[10rem] truncate text-right font-medium">
                          {values.customerName || "—"}
                        </dd>
                      </div>
                      <div className="flex justify-between gap-2">
                        <dt className="text-muted-foreground">结算</dt>
                        <dd className="max-w-[10rem] truncate text-right font-medium">
                          {values.settlementEntity || "—"}
                        </dd>
                      </div>
                      <div className="flex justify-between gap-2">
                        <dt className="text-muted-foreground">业务性质</dt>
                        <dd className="font-medium">{natureLabel}</dd>
                      </div>
                      <div className="flex justify-between gap-2">
                        <dt className="text-muted-foreground">明细行</dt>
                        <dd className="font-medium">
                          {values.lineItems.length} 行
                        </dd>
                      </div>
                      <div className="border-t border-border pt-3">
                        <div className="flex justify-between gap-2">
                          <dt className="text-muted-foreground">含税预估</dt>
                          <dd className="num font-semibold">
                            <MoneyValue
                              value={totals.gross}
                              taxBasis="gross"
                            />
                          </dd>
                        </div>
                        <div className="mt-2 flex justify-between gap-2">
                          <dt className="text-muted-foreground">税额</dt>
                          <dd className="num">
                            <MoneyValue value={totals.tax} />
                          </dd>
                        </div>
                      </div>
                    </dl>
                    <p className="rounded-md bg-muted/50 px-2.5 py-2 text-xs leading-relaxed text-muted-foreground">
                      {nextStep}
                    </p>
                  </div>
                )
              }}
            </form.Subscribe>
          </aside>
        </div>
      </form>

      <ContractUploadDialog
        open={uploadOpen}
        onOpenChange={setUploadOpen}
        initialCustomerId={initialCustomerId}
        onSuccess={(result) => {
          void handleUploadSuccess(result)
        }}
      />

      <DiscardConfirmDialog
        open={discardOpen}
        onOpenChange={setDiscardOpen}
        onConfirm={() => {
          setDiscardOpen(false)
          router.push("/sales/orders")
        }}
      />

      <DiscardConfirmDialog
        open={pendingNature != null}
        onOpenChange={(open) => {
          if (!open) setPendingNature(null)
        }}
        title="切换业务性质？"
        description="业务性质切换后，已填写的销售明细会被清空并重新开始，无法撤销。"
        confirmLabel="清空明细并切换"
        cancelLabel="取消"
        onConfirm={() => {
          if (pendingNature) applyNature(pendingNature)
          setPendingNature(null)
        }}
      />
    </div>
  )
}
