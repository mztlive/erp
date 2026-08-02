"use client"

/**
 * 供应商详情页 = 查看 + 编辑（同一页面）。
 * - /master-data/suppliers/new  新建
 * - /master-data/suppliers/:id  查看并直接改，保存即形成新版本
 * 不使用侧边 sheet，也没有单独的编辑弹窗。
 */

import * as React from "react"
import Link from "next/link"
import { useRouter } from "next/navigation"
import { BanIcon, CircleAlertIcon, SaveIcon } from "lucide-react"

import {
  BusinessFailureState,
  DocumentSection,
  FormalActionResult,
  OptionCombobox,
  PageHeader,
  RevisionTimeline,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { DatePicker } from "@/components/ui/date-picker"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { StatusBadge } from "@/components/ui/status-badge"
import { Textarea } from "@/components/ui/textarea"
import {
  MasterDataDisableDialog,
  MediaListField,
} from "@/features/master-data/master-data-action-dialog"
import { masterDataCopy } from "@/features/master-data/copy"
import { formatEffectiveRange } from "@/features/master-data/filter"
import {
  INVOICE_TYPE_OPTIONS,
  SETTLEMENT_MODE_OPTIONS,
  SUPPLIER_CAPABILITY_OPTIONS,
  SUPPLIER_RATING_OPTIONS,
  buildResourceFields,
  currentResourceFieldValues,
} from "@/features/master-data/resource-fields"
import {
  useCreateMasterDataMutation,
  useCreateRevisionMutation,
  useMasterDataCenterQuery,
} from "@/features/master-data/queries"
import type {
  MasterDataCenterView,
  MasterDataMutationResult,
} from "@/features/master-data/types"

function newIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

const CAPABILITY_SEPARATOR = "、"

function parseCapabilities(value: string): string[] {
  return value
    .split(/[、,，]/)
    .map((s) => s.trim())
    .filter(Boolean)
}

function CapabilityCheckboxGroup({
  value,
  onChange,
  disabled,
}: {
  value: string
  onChange: (next: string) => void
  disabled?: boolean
}) {
  const selected = parseCapabilities(value)
  const toggle = (option: string, checked: boolean) => {
    const next = checked
      ? [...selected, option]
      : selected.filter((item) => item !== option)
    onChange(next.join(CAPABILITY_SEPARATOR))
  }
  return (
    <div className="grid gap-2 sm:grid-cols-2">
      {SUPPLIER_CAPABILITY_OPTIONS.map((option) => (
        <label key={option} className="flex items-center gap-2 text-sm">
          <Checkbox
            checked={selected.includes(option)}
            disabled={disabled}
            onCheckedChange={(checked) => toggle(option, checked === true)}
          />
          {option}
        </label>
      ))}
    </div>
  )
}

type SupplierEditorFormValues = Readonly<{
  name: string
  company: string
  contactName: string
  contactPhone: string
  address: string
  capability: string
  settlement: string
  businessCategory: string
  signingEntity: string
  paymentEntity: string
  qualification: string
  contractNo: string
  contractValidFrom: string
  contractValidTo: string
  contractFile: string
  authorizationFile: string
  authorizationValidFrom: string
  authorizationValidTo: string
  foodLicense: string
  legalPersonIdCard: string
  taxNo: string
  bankName: string
  bankAccount: string
  invoiceType: string
  invoiceTaxRate: string
  initialScore: string
  supplierRating: string
  currentScore: string
  effectiveFrom: string
  effectiveTo: string
  changeReason: string
}>

function validateSupplierEditor(values: SupplierEditorFormValues): string | null {
  if (values.name.trim().length < 2) return "请填写供应商名称"
  if (values.company.trim().length < 1) return "请填写企业主体"
  if (!values.effectiveFrom) return "请选择生效开始日期"
  if (values.changeReason.trim().length < 2) {
    return "请填写本次保存的变更原因"
  }
  return null
}

function hydrateFromCenter(data: MasterDataCenterView): SupplierEditorFormValues {
  const fields = currentResourceFieldValues(data)
  return {
    name: data.name,
    company: fields.company ?? "",
    contactName: fields.contactName ?? "",
    contactPhone: fields.contactPhone ?? "",
    address: fields.address ?? "",
    capability: fields.capability ?? "",
    settlement: fields.settlement ?? "",
    businessCategory: fields.businessCategory ?? "",
    signingEntity: fields.signingEntity ?? "",
    paymentEntity: fields.paymentEntity ?? "",
    qualification: fields.qualification ?? "",
    contractNo: fields.contractNo ?? "",
    contractValidFrom: fields.contractValidFrom ?? "",
    contractValidTo: fields.contractValidTo ?? "",
    contractFile: fields.contractFile ?? "",
    authorizationFile: fields.authorizationFile ?? "",
    authorizationValidFrom: fields.authorizationValidFrom ?? "",
    authorizationValidTo: fields.authorizationValidTo ?? "",
    foodLicense: fields.foodLicense ?? "",
    legalPersonIdCard: fields.legalPersonIdCard ?? "",
    taxNo: fields.taxNo ?? "",
    bankName: fields.bankName ?? "",
    bankAccount: fields.bankAccount ?? "",
    invoiceType: fields.invoiceType ?? "",
    invoiceTaxRate: fields.invoiceTaxRate ?? "",
    initialScore: fields.initialScore ?? "",
    supplierRating: fields.supplierRating ?? "",
    currentScore: fields.currentScore ?? "",
    effectiveFrom: data.currentRevision.effectiveFrom,
    effectiveTo: data.currentRevision.effectiveTo ?? "",
    changeReason: "",
  }
}

function createDefaults(isCreate: boolean): SupplierEditorFormValues {
  return {
    name: "",
    company: "",
    contactName: "",
    contactPhone: "",
    address: "",
    capability: "",
    settlement: "",
    businessCategory: "",
    signingEntity: "",
    paymentEntity: "",
    qualification: "",
    contractNo: "",
    contractValidFrom: "",
    contractValidTo: "",
    contractFile: "",
    authorizationFile: "",
    authorizationValidFrom: "",
    authorizationValidTo: "",
    foodLicense: "",
    legalPersonIdCard: "",
    taxNo: "",
    bankName: "",
    bankAccount: "",
    invoiceType: "",
    invoiceTaxRate: "",
    initialScore: "",
    supplierRating: "",
    currentScore: "",
    effectiveFrom: "2026-08-01",
    effectiveTo: "",
    changeReason: isCreate ? "新建供应商" : "",
  }
}

export function SupplierDetailPage({ stableId }: { stableId: string }) {
  const router = useRouter()
  const isCreate = stableId === "new"
  const detailQuery = useMasterDataCenterQuery(
    "suppliers",
    isCreate ? "" : stableId
  )
  const createMutation = useCreateMasterDataMutation()
  const reviseMutation = useCreateRevisionMutation()

  const data = detailQuery.data
  const lockVersion = data?.lockVersion
  const revisionId = data?.currentRevision.revisionId
  const [formError, setFormError] = React.useState<string | null>(null)
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null
  )
  const [simulate, setSimulate] = React.useState<
    "ok" | "overlap" | "conflict"
  >("ok")
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey(isCreate ? "create-supplier" : "revise-supplier")
  )
  const [disableOpen, setDisableOpen] = React.useState(false)
  const hydratedKeyRef = React.useRef<string | null>(null)
  const initialFormValues = React.useMemo(
    () =>
      !isCreate && data
        ? hydrateFromCenter(data)
        : createDefaults(isCreate),
    [data, isCreate]
  )

  const form = useAppForm({
    defaultValues: initialFormValues,
    onSubmit: async ({ value }) => {
      setFormError(null)
      setResult(null)

      const validation = validateSupplierEditor(value)
      if (validation) {
        setFormError(validation)
        return
      }

      const fields = buildResourceFields("suppliers", value)

      if (!isCreate) {
        if (!data || !revisionId || lockVersion == null) return
        const response = await reviseMutation.mutateAsync({
          resource: "suppliers",
          stableId: data.stableId,
          baseRevisionId: revisionId,
          expectedLockVersion: lockVersion,
          name: value.name.trim(),
          effectiveFrom: value.effectiveFrom,
          effectiveTo: value.effectiveTo.trim() || undefined,
          changeReason: value.changeReason.trim(),
          fields,
          idempotencyKey,
          simulate,
        })
        setResult(response)
        if (response.outcome === "succeeded") {
          setIdempotencyKey(newIdempotencyKey("revise-supplier"))
          hydratedKeyRef.current = null
          await detailQuery.refetch()
        }
        return
      }

      const response = await createMutation.mutateAsync({
        resource: "suppliers",
        name: value.name.trim(),
        effectiveFrom: value.effectiveFrom,
        effectiveTo: value.effectiveTo.trim() || undefined,
        changeReason: value.changeReason.trim(),
        fields,
        idempotencyKey,
        simulate: simulate === "conflict" ? "ok" : simulate,
      })
      setResult(response)
      if (response.outcome === "succeeded") {
        router.replace(`/master-data/suppliers/${response.stableId}`)
      }
    },
  })

  React.useEffect(() => {
    if (isCreate || !data) return
    const key = `${data.stableId}:${data.lockVersion}:${data.currentRevision.revisionId}`
    if (hydratedKeyRef.current === key) return
    form.reset(hydrateFromCenter(data))
    hydratedKeyRef.current = key
  }, [data, form, isCreate])

  const listHref = "/master-data/suppliers"
  const pending = createMutation.isPending || reviseMutation.isPending
  const canRevise =
    isCreate || (data?.allowedActions.includes("CREATE_REVISION") ?? false)
  const canDisable = data?.allowedActions.includes("DISABLE") ?? false
  const reviseBlocker = data?.actionBlockers.find(
    (b) => b.action === "CREATE_REVISION"
  )
  const disableBlocker = data?.actionBlockers.find(
    (b) => b.action === "DISABLE"
  )

  if (!isCreate && detailQuery.isPending) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader
          title="供应商详情"
          description={masterDataCopy.centerLoading}
        />
        <div className="h-40 animate-pulse rounded-lg bg-muted" aria-busy />
      </div>
    )
  }

  if (!isCreate && (detailQuery.isError || !data)) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title="供应商详情" />
        <BusinessFailureState
          kind="system"
          description={
            detailQuery.isError
              ? masterDataCopy.centerLoadFail
              : masterDataCopy.centerMissingDesc
          }
          action={
            detailQuery.isError ? (
              <Button type="button" onClick={() => void detailQuery.refetch()}>
                重试
              </Button>
            ) : (
              <Button render={<Link href={listHref} />}>
                {masterDataCopy.actionBackList}
              </Button>
            )
          }
        />
      </div>
    )
  }

  const formId = "supplier-detail-form"
  const canEdit = isCreate || canRevise

  return (
    <form.Subscribe selector={(state) => state.values}>
      {(values) => {
        const title = isCreate
          ? masterDataCopy.supplierCreateTitle
          : values.name || data?.name || "供应商详情"
        const setFieldValue = (
          key:
            | "name"
            | "company"
            | "contactName"
            | "contactPhone"
            | "address"
            | "capability"
            | "settlement"
            | "businessCategory"
            | "signingEntity"
            | "paymentEntity"
            | "qualification"
            | "contractNo"
            | "contractValidFrom"
            | "contractValidTo"
            | "contractFile"
            | "authorizationFile"
            | "authorizationValidFrom"
            | "authorizationValidTo"
            | "foodLicense"
            | "legalPersonIdCard"
            | "taxNo"
            | "bankName"
            | "bankAccount"
            | "invoiceType"
            | "invoiceTaxRate"
            | "initialScore"
            | "supplierRating"
            | "currentScore"
            | "effectiveFrom"
            | "effectiveTo"
            | "changeReason",
          next: string
        ) => form.setFieldValue(key, next)
        const handleSubmit = (event?: React.FormEvent) => {
          event?.preventDefault()
          void form.handleSubmit()
        }
        return (
          <div className="mx-auto w-full max-w-shell">
            <form id={formId} onSubmit={handleSubmit}>
              <header className="sticky top-0 z-30 border-b border-border bg-background/95 px-4 py-3 shadow-sm backdrop-blur md:px-5">
                <div className="flex flex-col gap-3 lg:flex-row lg:items-center lg:justify-between">
                  <div className="min-w-0 space-y-1">
                    <div className="flex min-w-0 flex-wrap items-center gap-2">
                      <h1 className="truncate text-lg font-semibold tracking-tight">
                        {title}
                      </h1>
                      {!isCreate && data ? (
                        <StatusBadge
                          tone={data.lifecycleTone}
                          label={data.lifecycleStatusLabel}
                        />
                      ) : null}
                    </div>
                    {!isCreate && data ? (
                      <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
                        <span>
                          单号{" "}
                          <span className="num text-foreground">
                            {data.stableNo}
                          </span>
                        </span>
                        <span className="num rounded-md bg-muted px-1.5 py-0.5 text-[11px] text-foreground">
                          版本 {data.currentRevision.revisionNo}
                        </span>
                        <span className="num">
                          {formatEffectiveRange(
                            data.currentRevision.effectiveFrom,
                            data.currentRevision.effectiveTo
                          )}
                        </span>
                        <span className="inline-flex items-center gap-1.5">
                          <span>{masterDataCopy.centerVersionState}</span>
                          <StatusBadge
                            tone={
                              data.revisionTiming === "FUTURE"
                                ? "warning"
                                : "info"
                            }
                            label={data.revisionTimingLabel}
                          />
                        </span>
                      </div>
                    ) : (
                      <p className="text-sm text-muted-foreground">
                        {masterDataCopy.supplierCreateDesc}
                      </p>
                    )}
                  </div>

                  <div className="flex shrink-0 flex-wrap items-center gap-2">
                    {!isCreate && data ? (
                      <Button
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={!canDisable}
                        title={disableBlocker?.message}
                        onClick={() => setDisableOpen(true)}
                      >
                        <BanIcon data-icon="inline-start" aria-hidden />
                        {masterDataCopy.actionDisable}
                      </Button>
                    ) : null}
                    <Button
                      type="button"
                      size="sm"
                      variant="outline"
                      render={<Link href={listHref} />}
                    >
                      返回列表
                    </Button>
                    <Button
                      type="submit"
                      size="sm"
                      disabled={!canEdit || pending}
                    >
                      <SaveIcon data-icon="inline-start" aria-hidden />
                      {isCreate
                        ? masterDataCopy.createSubmit
                        : masterDataCopy.reviseSubmit}
                    </Button>
                  </div>
                </div>
              </header>

              <div className="space-y-4 p-4 md:p-5">
                {!isCreate && !canRevise && reviseBlocker ? (
                  <p className="text-xs text-muted-foreground">
                    {masterDataCopy.centerUpdateBlocked(reviseBlocker.message)}
                  </p>
                ) : null}

                {result?.outcome === "succeeded" ? (
                  <FormalActionResult
                    status="succeeded"
                    title={
                      isCreate
                        ? masterDataCopy.createSuccessTitle
                        : masterDataCopy.reviseSuccessTitle
                    }
                    description={
                      isCreate
                        ? masterDataCopy.createSuccessDesc
                        : masterDataCopy.reviseSuccessDesc
                    }
                    reference={result.reference}
                    facts={[
                      {
                        label: masterDataCopy.resultNo,
                        value: result.stableNo,
                      },
                      {
                        label: masterDataCopy.resultVersion,
                        value: `v${result.revisionNo}`,
                      },
                    ]}
                  />
                ) : null}

                {result?.outcome === "blocked" ? (
                  <FormalActionResult
                    status="blocked"
                    title={
                      isCreate
                        ? masterDataCopy.createBlockedTitle
                        : masterDataCopy.reviseBlockedTitle
                    }
                    description={result.message}
                    facts={
                      result.detail
                        ? [{ label: "说明", value: result.detail }]
                        : undefined
                    }
                  />
                ) : null}

                {result?.outcome === "conflict" ? (
                  <FormalActionResult
                    status="blocked"
                    title={masterDataCopy.reviseConflictTitle}
                    description={
                      result.message || masterDataCopy.reviseConflictHint
                    }
                  />
                ) : null}

                {formError ? (
                  <Alert variant="destructive">
                    <CircleAlertIcon aria-hidden />
                    <AlertTitle>填写不完整</AlertTitle>
                    <AlertDescription>{formError}</AlertDescription>
                  </Alert>
                ) : null}

                <section className="space-y-3 rounded-2xl border border-border bg-card p-5 shadow-sm">
                  <h2 className="px-1 text-base font-semibold">基本信息</h2>
                  <p className="px-1 text-xs text-muted-foreground">
                    名称与企业主体是必填项；保存即生成新版本。
                  </p>
                  <div className="grid gap-3 px-1 sm:grid-cols-2">
                    <div className="space-y-1.5 sm:col-span-2">
                      <Label htmlFor="supplier-name">名称 *</Label>
                      <Input
                        id="supplier-name"
                        value={values.name}
                        onChange={(e) => setFieldValue("name", e.target.value)}
                        placeholder="供应商名称"
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5 sm:col-span-2">
                      <Label htmlFor="supplier-company">
                        {masterDataCopy.fCompany} *
                      </Label>
                      <Input
                        id="supplier-company"
                        value={values.company}
                        onChange={(e) =>
                          setFieldValue("company", e.target.value)
                        }
                        placeholder="企业主体全称"
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label htmlFor="supplier-contact-name">
                        {masterDataCopy.fContactName}
                      </Label>
                      <Input
                        id="supplier-contact-name"
                        value={values.contactName}
                        onChange={(e) =>
                          setFieldValue("contactName", e.target.value)
                        }
                        placeholder="联系人姓名"
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label htmlFor="supplier-contact-phone">
                        {masterDataCopy.fContactPhone}
                      </Label>
                      <Input
                        id="supplier-contact-phone"
                        value={values.contactPhone}
                        onChange={(e) =>
                          setFieldValue("contactPhone", e.target.value)
                        }
                        placeholder="联系电话"
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5 sm:col-span-2">
                      <Label htmlFor="supplier-address">
                        {masterDataCopy.fAddress}
                      </Label>
                      <Input
                        id="supplier-address"
                        value={values.address}
                        onChange={(e) =>
                          setFieldValue("address", e.target.value)
                        }
                        placeholder="供应商注册或经营地址"
                        disabled={!canEdit}
                      />
                    </div>
                  </div>
                </section>

                <section className="space-y-3 rounded-2xl border border-border bg-card p-5 shadow-sm">
                  <h2 className="px-1 text-base font-semibold">商务与资质</h2>
                  <p className="px-1 text-xs text-muted-foreground">
                    能力为多选；结算方式用于采购选供应商时的校验与提示。
                  </p>
                  <div className="space-y-1.5 px-1">
                    <Label>{masterDataCopy.fCapability}</Label>
                    <CapabilityCheckboxGroup
                      value={values.capability}
                      onChange={(next) => setFieldValue("capability", next)}
                      disabled={!canEdit}
                    />
                  </div>
                  <div className="space-y-1.5 px-1">
                    <Label>{masterDataCopy.fSettlement}</Label>
                    <OptionCombobox
                      value={values.settlement || null}
                      onValueChange={(v) =>
                        setFieldValue("settlement", v ?? "")
                      }
                      options={SETTLEMENT_MODE_OPTIONS.map((o) => ({
                        value: o,
                        label: o,
                      }))}
                      allowClear
                      placeholder="请选择结算方式"
                      className="w-full"
                      disabled={!canEdit}
                    />
                  </div>
                  <div className="grid gap-3 px-1 sm:grid-cols-2">
                    <div className="space-y-1.5 sm:col-span-2">
                      <Label htmlFor="supplier-business-category">
                        {masterDataCopy.fBusinessCategory}
                      </Label>
                      <Input
                        id="supplier-business-category"
                        value={values.businessCategory}
                        onChange={(e) =>
                          setFieldValue("businessCategory", e.target.value)
                        }
                        placeholder="如：礼盒、茶叶、卡券"
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label htmlFor="supplier-signing-entity">
                        {masterDataCopy.fSigningEntity}
                      </Label>
                      <Input
                        id="supplier-signing-entity"
                        value={values.signingEntity}
                        onChange={(e) =>
                          setFieldValue("signingEntity", e.target.value)
                        }
                        placeholder="与我司签约的公司主体"
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label htmlFor="supplier-payment-entity">
                        {masterDataCopy.fPaymentEntity}
                      </Label>
                      <Input
                        id="supplier-payment-entity"
                        value={values.paymentEntity}
                        onChange={(e) =>
                          setFieldValue("paymentEntity", e.target.value)
                        }
                        placeholder="付款时的公司主体"
                        disabled={!canEdit}
                      />
                    </div>
                  </div>
                </section>

                <section className="space-y-3 rounded-2xl border border-border bg-card p-5 shadow-sm">
                  <h2 className="px-1 text-base font-semibold">资质附件</h2>
                  <p className="px-1 text-xs text-muted-foreground">
                    一家供应商可维护多份资质；失效后对应能力不得用于新的采购单。
                  </p>
                  <div className="space-y-1.5 px-1">
                    <MediaListField
                      label={masterDataCopy.fQualification}
                      hint={masterDataCopy.supplierQualificationHint}
                      value={values.qualification}
                      onChange={(next) =>
                        setFieldValue("qualification", next)
                      }
                      accept="image/jpeg,image/png,image/webp,application/pdf"
                    />
                  </div>
                </section>

                <section className="space-y-3 rounded-2xl border border-border bg-card p-5 shadow-sm">
                  <h2 className="px-1 text-base font-semibold">合同与授权</h2>
                  <p className="px-1 text-xs text-muted-foreground">
                    合同与授权书文件支持图片 / PDF；有效期到期后需重新维护。
                  </p>
                  <div className="grid gap-3 px-1 sm:grid-cols-2">
                    <div className="space-y-1.5">
                      <Label htmlFor="supplier-contract-no">
                        {masterDataCopy.fContractNo}
                      </Label>
                      <Input
                        id="supplier-contract-no"
                        value={values.contractNo}
                        onChange={(e) =>
                          setFieldValue("contractNo", e.target.value)
                        }
                        placeholder="合同编号"
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5 sm:col-span-2">
                      <Label htmlFor="supplier-contract-valid-from">
                        {masterDataCopy.fContractValidFrom}
                      </Label>
                      <DatePicker
                        value={values.contractValidFrom || undefined}
                        onValueChange={(next) =>
                          setFieldValue("contractValidFrom", next ?? "")
                        }
                        disabled={!canEdit}
                        className="w-full"
                      />
                    </div>
                    <div className="space-y-1.5 sm:col-span-2">
                      <Label htmlFor="supplier-contract-valid-to">
                        {masterDataCopy.fContractValidTo}
                      </Label>
                      <DatePicker
                        value={values.contractValidTo || undefined}
                        onValueChange={(next) =>
                          setFieldValue("contractValidTo", next ?? "")
                        }
                        disabled={!canEdit}
                        className="w-full"
                      />
                    </div>
                    <div className="space-y-1.5">
                      <MediaListField
                        label={masterDataCopy.fContractFile}
                        hint={masterDataCopy.supplierQualificationHint}
                        value={values.contractFile}
                        onChange={(next) =>
                          setFieldValue("contractFile", next)
                        }
                        accept="image/jpeg,image/png,image/webp,application/pdf"
                      />
                    </div>
                    <div className="space-y-1.5">
                      <MediaListField
                        label={masterDataCopy.fAuthorizationFile}
                        hint={masterDataCopy.supplierQualificationHint}
                        value={values.authorizationFile}
                        onChange={(next) =>
                          setFieldValue("authorizationFile", next)
                        }
                        accept="image/jpeg,image/png,image/webp,application/pdf"
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label htmlFor="supplier-auth-valid-from">
                        {masterDataCopy.fAuthorizationValidFrom}
                      </Label>
                      <DatePicker
                        value={values.authorizationValidFrom || undefined}
                        onValueChange={(next) =>
                          setFieldValue("authorizationValidFrom", next ?? "")
                        }
                        disabled={!canEdit}
                        className="w-full"
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label htmlFor="supplier-auth-valid-to">
                        {masterDataCopy.fAuthorizationValidTo}
                      </Label>
                      <DatePicker
                        value={values.authorizationValidTo || undefined}
                        onValueChange={(next) =>
                          setFieldValue("authorizationValidTo", next ?? "")
                        }
                        disabled={!canEdit}
                        className="w-full"
                      />
                    </div>
                    <div className="space-y-1.5">
                      <MediaListField
                        label={masterDataCopy.fFoodLicense}
                        hint={masterDataCopy.supplierQualificationHint}
                        value={values.foodLicense}
                        onChange={(next) =>
                          setFieldValue("foodLicense", next)
                        }
                        accept="image/jpeg,image/png,image/webp,application/pdf"
                      />
                    </div>
                    <div className="space-y-1.5">
                      <MediaListField
                        label={masterDataCopy.fLegalPersonIdCard}
                        hint={masterDataCopy.supplierQualificationHint}
                        value={values.legalPersonIdCard}
                        onChange={(next) =>
                          setFieldValue("legalPersonIdCard", next)
                        }
                        accept="image/jpeg,image/png,image/webp,application/pdf"
                      />
                    </div>
                  </div>
                </section>

                <section className="space-y-3 rounded-2xl border border-border bg-card p-5 shadow-sm">
                  <h2 className="px-1 text-base font-semibold">开票信息</h2>
                  <p className="px-1 text-xs text-muted-foreground">
                    税号与银行信息用于采购开票与付款；可随每次保存的版本修改。
                  </p>
                  <div className="grid gap-3 px-1 sm:grid-cols-2">
                    <div className="space-y-1.5">
                      <Label htmlFor="supplier-tax-no">
                        {masterDataCopy.fTaxNo}
                      </Label>
                      <Input
                        id="supplier-tax-no"
                        value={values.taxNo}
                        onChange={(e) => setFieldValue("taxNo", e.target.value)}
                        placeholder="纳税人识别号"
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label htmlFor="supplier-bank-name">
                        {masterDataCopy.fBankName}
                      </Label>
                      <Input
                        id="supplier-bank-name"
                        value={values.bankName}
                        onChange={(e) =>
                          setFieldValue("bankName", e.target.value)
                        }
                        placeholder="开户银行"
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5 sm:col-span-2">
                      <Label htmlFor="supplier-bank-account">
                        {masterDataCopy.fBankAccount}
                      </Label>
                      <Input
                        id="supplier-bank-account"
                        value={values.bankAccount}
                        onChange={(e) =>
                          setFieldValue("bankAccount", e.target.value)
                        }
                        placeholder="银行账号"
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>{masterDataCopy.fInvoiceType}</Label>
                      <OptionCombobox
                        value={values.invoiceType || null}
                        onValueChange={(v) =>
                          setFieldValue("invoiceType", v ?? "")
                        }
                        options={INVOICE_TYPE_OPTIONS.map((o) => ({
                          value: o,
                          label: o,
                        }))}
                        allowClear
                        placeholder="请选择发票类型"
                        className="w-full"
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label htmlFor="supplier-invoice-tax-rate">
                        {masterDataCopy.fInvoiceTaxRate}
                      </Label>
                      <Input
                        id="supplier-invoice-tax-rate"
                        value={values.invoiceTaxRate}
                        onChange={(e) =>
                          setFieldValue("invoiceTaxRate", e.target.value)
                        }
                        placeholder="如：13%"
                        disabled={!canEdit}
                      />
                    </div>
                  </div>
                </section>

                <section className="space-y-3 rounded-2xl border border-border bg-card p-5 shadow-sm">
                  <h2 className="px-1 text-base font-semibold">合作评估</h2>
                  <p className="px-1 text-xs text-muted-foreground">
                    期初评分在合作开始时记录；合作中评分随合作过程定期更新。
                  </p>
                  <div className="grid gap-3 px-1 sm:grid-cols-3">
                    <div className="space-y-1.5">
                      <Label htmlFor="supplier-initial-score">
                        {masterDataCopy.fInitialScore}
                      </Label>
                      <Input
                        id="supplier-initial-score"
                        value={values.initialScore}
                        onChange={(e) =>
                          setFieldValue("initialScore", e.target.value)
                        }
                        placeholder="如：85"
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label>{masterDataCopy.fSupplierRating}</Label>
                      <OptionCombobox
                        value={values.supplierRating || null}
                        onValueChange={(v) =>
                          setFieldValue("supplierRating", v ?? "")
                        }
                        options={SUPPLIER_RATING_OPTIONS.map((o) => ({
                          value: o,
                          label: o,
                        }))}
                        allowClear
                        placeholder="请选择评级"
                        className="w-full"
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label htmlFor="supplier-current-score">
                        {masterDataCopy.fCurrentScore}
                      </Label>
                      <Input
                        id="supplier-current-score"
                        value={values.currentScore}
                        onChange={(e) =>
                          setFieldValue("currentScore", e.target.value)
                        }
                        placeholder="如：88"
                        disabled={!canEdit}
                      />
                    </div>
                  </div>
                </section>

                <section className="space-y-3 rounded-2xl border border-border bg-card p-5 shadow-sm">
                  <h2 className="px-1 text-base font-semibold">生效与原因</h2>
                  <div className="grid gap-3 px-1 sm:grid-cols-2">
                    <div className="space-y-1.5">
                      <Label htmlFor="ef-from">
                        {masterDataCopy.fieldEffectiveFrom}
                      </Label>
                      <DatePicker
                        value={values.effectiveFrom || undefined}
                        onValueChange={(next) =>
                          setFieldValue("effectiveFrom", next ?? "")
                        }
                        disabled={!canEdit}
                        className="w-full"
                      />
                    </div>
                    <div className="space-y-1.5">
                      <Label htmlFor="ef-to">
                        {masterDataCopy.fieldEffectiveTo}
                      </Label>
                      <DatePicker
                        value={values.effectiveTo || undefined}
                        onValueChange={(next) =>
                          setFieldValue("effectiveTo", next ?? "")
                        }
                        disabled={!canEdit}
                        className="w-full"
                      />
                    </div>
                    <div className="space-y-1.5 sm:col-span-2">
                      <Label htmlFor="reason">
                        {masterDataCopy.fieldChangeReason}
                      </Label>
                      <Textarea
                        id="reason"
                        value={values.changeReason}
                        onChange={(e) =>
                          setFieldValue("changeReason", e.target.value)
                        }
                        rows={2}
                        placeholder={
                          isCreate
                            ? "新建原因"
                            : "说明本次修改内容，保存后形成新版本"
                        }
                        disabled={!canEdit}
                      />
                    </div>
                    <div className="space-y-1.5 sm:col-span-2">
                      <Label>{masterDataCopy.demoSimulateLabel}</Label>
                      <OptionCombobox
                        value={simulate}
                        onValueChange={(v) =>
                          setSimulate((v ?? "ok") as typeof simulate)
                        }
                        options={[
                          { value: "ok", label: masterDataCopy.demoOk },
                          {
                            value: "overlap",
                            label: masterDataCopy.demoOverlap,
                          },
                          ...(!isCreate
                            ? [
                                {
                                  value: "conflict" as const,
                                  label: masterDataCopy.demoConflict,
                                },
                              ]
                            : []),
                        ]}
                        allowClear={false}
                        className="w-full max-w-md"
                      />
                    </div>
                  </div>
                </section>

                {!isCreate && data ? (
                  <section
                    aria-label="历史与引用"
                    className="overflow-hidden rounded-2xl border border-border bg-card px-5 shadow-sm"
                  >
                    <DocumentSection
                      title={masterDataCopy.centerVersions}
                      description={masterDataCopy.centerVersionsDesc}
                    >
                      <RevisionTimeline
                        revisions={data.revisionTimeline.map((rev) => ({
                          id: rev.id,
                          version: rev.revisionNo,
                          source: "erp-change" as const,
                          actor: rev.actor,
                          effectiveAt: {
                            dateTime: rev.effectiveFrom,
                            label: formatEffectiveRange(
                              rev.effectiveFrom,
                              rev.effectiveTo
                            ),
                          },
                          reason: (
                            <div className="space-y-1">
                              <div>
                                {masterDataCopy.centerHistoryName}：
                                <strong>{rev.nameSnapshot}</strong>
                              </div>
                              <div>{rev.changeReason}</div>
                              <div className="flex flex-wrap gap-2">
                                <Badge variant="outline">
                                  {rev.timingLabel}
                                </Badge>
                                <Badge variant="secondary">
                                  {rev.lifecycleAtRevision === "ENABLED"
                                    ? "启用"
                                    : "停用"}
                                </Badge>
                              </div>
                            </div>
                          ),
                          isCurrent: rev.isCurrent,
                        }))}
                      />
                    </DocumentSection>

                    <DocumentSection
                      title={masterDataCopy.centerRelations}
                      description={masterDataCopy.centerRelationsDesc}
                    >
                      <p className="text-sm">
                        {masterDataCopy.centerUsageCount(
                          data.usageSummary.historicalReferenceCount
                        )}
                        {data.usageSummary.note}
                      </p>
                      <ul className="mt-3 space-y-2">
                        {data.selectorEligibility.map((s) => (
                          <li
                            key={s.context}
                            className="flex flex-wrap items-center gap-2 rounded-md bg-muted/40 px-2 py-1.5 text-sm"
                          >
                            <span>{s.contextLabel}</span>
                            <Badge
                              variant={
                                s.eligible ? "success" : "destructive"
                              }
                            >
                              {s.eligible
                                ? masterDataCopy.eligible
                                : masterDataCopy.ineligible}
                            </Badge>
                            {s.reason ? (
                              <span className="text-xs text-muted-foreground">
                                {s.reason}
                              </span>
                            ) : null}
                          </li>
                        ))}
                      </ul>
                    </DocumentSection>

                    <DocumentSection
                      title={masterDataCopy.centerAudit}
                      description={masterDataCopy.centerAuditDesc}
                    >
                      {data.auditEvents.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                          {masterDataCopy.centerNoAudit}
                        </p>
                      ) : (
                        <ul className="space-y-2 text-sm">
                          {data.auditEvents.map((ev) => (
                            <li
                              key={ev.id}
                              className="rounded-md border border-border px-3 py-2"
                            >
                              <div className="flex flex-wrap gap-2">
                                <span className="num text-xs text-muted-foreground">
                                  {ev.at.slice(0, 19).replace("T", " ")}
                                </span>
                                <span>{ev.actor}</span>
                                <Badge variant="outline">{ev.action}</Badge>
                              </div>
                              <div className="mt-1 text-muted-foreground">
                                {ev.detail}
                              </div>
                            </li>
                          ))}
                        </ul>
                      )}
                    </DocumentSection>
                  </section>
                ) : null}
              </div>
            </form>

            {!isCreate && data ? (
              <MasterDataDisableDialog
                open={disableOpen}
                onOpenChange={setDisableOpen}
                resource="suppliers"
                target={data}
              />
            ) : null}
          </div>
        )
      }}
    </form.Subscribe>
  )
}
