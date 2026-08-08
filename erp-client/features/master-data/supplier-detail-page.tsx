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
import {
  ArrowLeftIcon,
  BanIcon,
  CircleAlertIcon,
  SaveIcon,
} from "lucide-react"

import {
  BusinessFailureState,
  DiscardConfirmDialog,
  DocumentHeader,
  DocumentSection,
  FormalActionResult,
  OptionCombobox,
  PageHeader,
  PageScaffold,
  RevisionTimeline,
  SettlementPartyCombobox,
  surfaceInsetClassName,
  surfacePanelClassName,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { DatePicker } from "@/components/ui/date-picker"
import {
  Dialog,
  DialogClose,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Tabs, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { Textarea } from "@/components/ui/textarea"
import { toast } from "@/components/ui/toast"
import { uploadFileAssetImage } from "@/features/file-assets/api"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
  MasterDataDisableDialog,
  MediaListField,
} from "@/features/master-data/master-data-action-dialog"
import { masterDataCopy } from "@/features/master-data/copy"
import {
  INVOICE_TYPE_OPTIONS,
  SETTLEMENT_MODE_OPTIONS,
  SUPPLIER_CAPABILITY_OPTIONS,
  SUPPLIER_RATING_OPTIONS,
  buildResourceFields,
  currentResourceFieldValues,
  defaultImmediateEffectiveFrom,
} from "@/features/master-data/resource-fields"
import {
  revealMasterDataSensitive,
  useCreateMasterDataMutation,
  useCreateRevisionMutation,
  useMasterDataCenterQuery,
} from "@/features/master-data/queries"
import type {
  MasterDataCenterView,
  MasterDataMutationResult,
  SupplierFields,
} from "@/features/master-data/types"
import { formatDateTime } from "@/lib/datetime"
import { usePartyOptionsQuery } from "@/hooks/use-options"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"

function newIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

const CAPABILITY_SEPARATOR = "、"

/**
 * 敏感字段：
 * - 无 revealToken（新建 / 后端未提供门禁）→ 直接可编辑，避免卡死无法输入
 * - 有 revealToken → 默认打码，短时查看后可编辑（15 秒自动隐藏）
 */
function SensitiveEditableField({
  label,
  id,
  value,
  maskedValue,
  revealToken,
  onChange,
  disabled,
  canReveal = false,
  placeholder,
}: {
  label: string
  id: string
  value: string
  maskedValue?: string
  revealToken?: string
  onChange: (next: string) => void
  disabled?: boolean
  canReveal?: boolean
  placeholder?: string
}) {
  const [revealed, setRevealed] = React.useState(false)
  const [revealedValue, setRevealedValue] = React.useState<string | null>(null)
  const [revealError, setRevealError] = React.useState<string | null>(null)

  React.useEffect(() => {
    if (!revealed) return
    const timer = window.setTimeout(() => {
      setRevealed(false)
      setRevealedValue(null)
    }, 15000)
    return () => window.clearTimeout(timer)
  }, [revealed])

  // 新建或无揭示令牌时直接可编辑（创建场景必走此分支）
  if (!revealToken) {
    return (
      <div className="space-y-1.5">
        <Label htmlFor={id}>{label}</Label>
        <Input
          id={id}
          value={value}
          onChange={(e) => onChange(e.target.value)}
          disabled={disabled}
          placeholder={placeholder}
        />
      </div>
    )
  }

  const reveal = async () => {
    try {
      const plaintext = value || (await revealMasterDataSensitive(revealToken))
      setRevealedValue(plaintext)
      setRevealError(null)
      setRevealed(true)
    } catch (error) {
      setRevealError(error instanceof Error ? error.message : "无权查看")
    }
  }

  if (!revealed) {
    return (
      <div className="space-y-1.5">
        <Label htmlFor={id}>{label}</Label>
        <div className="flex flex-wrap items-center gap-2">
          <code className="num rounded-md bg-muted px-2 py-1.5 text-sm">
            {maskedValue || "****"}
          </code>
          <Button
            type="button"
            size="sm"
            variant="outline"
            disabled={!canReveal}
            onClick={() => void reveal()}
          >
            短时查看
          </Button>
        </div>
        {revealError ? (
          <p className="text-xs text-destructive" role="alert">
            {revealError}
          </p>
        ) : (
          <p className="text-xs text-muted-foreground">
            敏感信息已打码；查看后 15 秒自动隐藏。
          </p>
        )}
      </div>
    )
  }
  return (
    <div className="space-y-1.5">
      <Label htmlFor={id}>{label}</Label>
      <Input
        id={id}
        value={revealedValue ?? value}
        autoFocus
        onChange={(e) => {
          setRevealedValue(e.target.value)
          onChange(e.target.value)
        }}
        onBlur={() => {
          setRevealed(false)
          setRevealedValue(null)
        }}
        disabled={disabled}
        placeholder={placeholder}
      />
      <p className="text-xs text-muted-foreground">
        已显示明文；离开输入框后自动打码。
      </p>
    </div>
  )
}

/** 分区导航：真正的标签切换（每次只挂载一个分区），避免整页长滚动堆叠。 */
const SUPPLIER_SECTIONS: ReadonlyArray<{ id: string; label: string }> = [
  { id: "basic", label: "基本信息" },
  { id: "commercial", label: "商务合作" },
  { id: "contract", label: "合同资质" },
  { id: "invoice", label: "开票信息" },
  { id: "history", label: "历史引用" },
]

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
    <div className="grid grid-cols-2 gap-x-3 gap-y-1.5 sm:grid-cols-3 lg:grid-cols-5">
      {SUPPLIER_CAPABILITY_OPTIONS.map((option) => (
        <label
          key={option}
          className="flex items-center gap-2 text-sm leading-none"
        >
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

function FieldShell({
  className,
  children,
}: {
  className?: string
  children: React.ReactNode
}) {
  return (
    <div
      className={cn(
        "space-y-2 [&_[data-slot=label]]:text-[13px] [&_[data-slot=label]]:font-medium [&_[data-slot=label]]:text-foreground/80",
        className,
      )}
    >
      {children}
    </div>
  )
}

/** 分区内容：只在切到该标签时挂载，不再是卡片，直接铺在共享卡片的内容区里。 */
function SectionPanel({
  title,
  description,
  children,
}: {
  title: string
  description?: string
  children: React.ReactNode
}) {
  return (
    <section className="space-y-5">
      <div className="space-y-1 border-b border-border/60 pb-3">
        <h2 className="text-base font-semibold tracking-tight">{title}</h2>
        {description ? (
          <p className="max-w-3xl text-sm leading-5 text-muted-foreground">
            {description}
          </p>
        ) : null}
      </div>
      {children}
    </section>
  )
}

/** 合同资质页内的业务对象分组，避免把不同文件与有效期混排。 */
function CredentialGroup({
  title,
  description,
  children,
}: {
  title: string
  description: string
  children: React.ReactNode
}) {
  return (
    <section className={cn(surfaceInsetClassName, "overflow-hidden")}>
      <div className="border-b border-border/60 px-4 py-3">
        <h3 className="text-sm font-semibold text-foreground">{title}</h3>
        <p className="mt-1 text-xs leading-5 text-muted-foreground">
          {description}
        </p>
      </div>
      <div className="p-4">{children}</div>
    </section>
  )
}

type SupplierEditorFormValues = Readonly<{
  name: string
  company: string
  creditCode: string
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
  changeReason: string
}>

/** 保存前字段校验（不含变更原因；原因在右上角保存弹窗中填写）。 */
type SupplierValidationContext = Readonly<{
  hasStoredContactPhone?: boolean
  originalContactName?: string
}>

function validateSupplierEditorFields(
  values: SupplierEditorFormValues,
  context: SupplierValidationContext = {},
): string | null {
  if (values.name.trim().length < 2) return "请填写供应商名称"
  if (values.company.trim().length < 1) return "请填写企业主体"
  if (values.creditCode.trim() && !/^[0-9A-Z]{18}$/i.test(values.creditCode.trim())) {
    return "统一社会信用代码必须是 18 位字母或数字"
  }
  const hasContactName = Boolean(values.contactName.trim())
  const hasContactPhone = Boolean(values.contactPhone.trim())
  const preservesStoredContact =
    hasContactName &&
    !hasContactPhone &&
    context.hasStoredContactPhone === true &&
    values.contactName.trim() === context.originalContactName?.trim()
  if (hasContactName !== hasContactPhone && !preservesStoredContact) {
    return "联系人姓名和联系电话必须同时填写；修改联系人姓名前请先短时查看联系电话"
  }
  if (!values.signingEntity.trim()) return "请选择公司签约主体"
  if (!values.paymentEntity.trim()) return "请选择公司付款主体"
  for (const [label, score] of [
    ["合作期初评分", values.initialScore],
    ["合作中评分", values.currentScore],
  ] as const) {
    if (score.trim() && !/^(100|[1-9]?\d)$/.test(score.trim())) {
      return `${label}必须是 0–100 的整数`
    }
  }
  const taxRate = values.invoiceTaxRate.trim().replace(/%$/, "")
  if (taxRate) {
    const numeric = Number(taxRate)
    if (!Number.isFinite(numeric) || numeric < 0 || numeric >= 100) {
      return "发票税点必须在 0%（含）到 100%（不含）之间"
    }
  }
  if (
    values.contractValidFrom &&
    values.contractValidTo &&
    values.contractValidTo <= values.contractValidFrom
  ) {
    return "合同有效期止必须晚于有效期起"
  }
  if (
    values.authorizationValidFrom &&
    values.authorizationValidTo &&
    values.authorizationValidTo <= values.authorizationValidFrom
  ) {
    return "授权书有效期止必须晚于有效期起"
  }
  return null
}

function hydrateFromCenter(
  data: MasterDataCenterView,
): SupplierEditorFormValues {
  const fields = currentResourceFieldValues(data)
  return {
    name: data.name,
    company: fields.company ?? "",
    creditCode: fields.creditCode ?? "",
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
    changeReason: "",
  }
}

function createDefaults(isCreate: boolean): SupplierEditorFormValues {
  return {
    name: "",
    company: "",
    creditCode: "",
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
    changeReason: isCreate ? "新建供应商" : "",
  }
}

type SupplierFieldKey = keyof SupplierEditorFormValues

export function SupplierDetailPage({ stableId }: { stableId: string }) {
  const router = useRouter()
  const isCreate = stableId === "new"
  const detailQuery = useMasterDataCenterQuery(
    "suppliers",
    isCreate ? "" : stableId,
  )
  const createMutation = useCreateMasterDataMutation()
  const reviseMutation = useCreateRevisionMutation()
  const accountQuery = useAccountProfileQuery()
  const partyOptionsQuery = usePartyOptionsQuery()

  const data = detailQuery.data
  const lockVersion = data?.lockVersion
  const revisionId = data?.currentRevision.revisionId
  const [formError, setFormError] = React.useState<string | null>(null)
  const [result, setResult] = React.useState<MasterDataMutationResult | null>(
    null,
  )
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey(isCreate ? "create-supplier" : "revise-supplier"),
  )
  /** 已登记资质附件：字段 key → fileName → { assetId, url }（回显链接 + 再次保存不重复上传）。 */
  const mediaAssetMaps = React.useMemo(() => {
    const maps: Record<
      string,
      Record<string, { assetId: string; url: string }>
    > = {}
    for (const [key, entries] of Object.entries(data?.mediaAssets ?? {})) {
      const map: Record<string, { assetId: string; url: string }> = {}
      for (const entry of entries) {
        map[entry.fileName] = { assetId: entry.assetId, url: entry.url }
      }
      maps[key] = map
    }
    return maps
  }, [data])
  /** 本会话选择但尚未上传的资质文件；保存时按文件名上传并回填 asset id。 */
  const pendingFilesRef = React.useRef<Map<string, File>>(new Map())
  /** 保存失败后保留本会话已上传资产，重试时只复查后台扫描结果，不重复上传。 */
  const uploadedAssetMapsRef = React.useRef<
    Record<string, Record<string, { assetId: string; url: string }>>
  >({})
  /** 已实际编辑过的敏感字段；用于区分“保留打码值”和“明确清空”。 */
  const editedSensitiveRef = React.useRef(new Set<"contactPhone" | "address">())
  const rememberMediaFiles = React.useCallback((files: File[]) => {
    for (const file of files) {
      pendingFilesRef.current.set(file.name, file)
    }
  }, [])
  const mediaUrlsFor = React.useCallback(
    (fieldKey: string): Readonly<Record<string, string>> => {
      const entries = mediaAssetMaps[fieldKey] ?? {}
      return Object.fromEntries(
        Object.entries(entries).map(([name, info]) => [name, info.url]),
      )
    },
    [mediaAssetMaps],
  )
  /** 上传仍为本地待传的资质文件，返回 fileName → asset id 映射（按字段）。 */
  const resolvePendingMedia = React.useCallback(
    async (
      values: SupplierEditorFormValues,
    ): Promise<Record<string, Record<string, string>>> => {
      const mediaFields = [
        "qualification",
        "contractFile",
        "authorizationFile",
        "foodLicense",
        "legalPersonIdCard",
      ] as const
      const out: Record<string, Record<string, string>> = {}
      for (const key of mediaFields) {
        const names = (values[key] ?? "")
          .split(",")
          .map((s) => s.trim())
          .filter(Boolean)
        const existing = mediaAssetMaps[key] ?? {}
        const uploadedInSession = uploadedAssetMapsRef.current[key] ?? {}
        const map: Record<string, string> = {}
        for (const name of names) {
          const known = existing[name] ?? uploadedInSession[name]
          if (known?.assetId) {
            map[name] = known.assetId
            continue
          }
          const file = pendingFilesRef.current.get(name)
          if (!file) continue
          const sensitivityClass =
            key === "legalPersonIdCard" ? "highly_sensitive" : "sensitive"
          const uploaded = await uploadFileAssetImage(
            file,
            "attachment",
            sensitivityClass,
          )
          map[name] = uploaded.fileAssetId
          uploadedAssetMapsRef.current[key] = {
            ...uploadedAssetMapsRef.current[key],
            [name]: { assetId: uploaded.fileAssetId, url: uploaded.url },
          }
        }
        out[key] = map
      }
      return out
    },
    [mediaAssetMaps],
  )
  const [disableOpen, setDisableOpen] = React.useState(false)
  const [discardOpen, setDiscardOpen] = React.useState(false)
  const [saveReasonOpen, setSaveReasonOpen] = React.useState(false)
  const [reasonDraft, setReasonDraft] = React.useState("")
  const [reasonError, setReasonError] = React.useState<string | null>(null)
  const [pendingNav, setPendingNav] = React.useState<string | null>(null)
  const [activeSection, setActiveSection] = React.useState("basic")
  const errorRef = React.useRef<HTMLDivElement | null>(null)
  const hydratedKeyRef = React.useRef<string | null>(null)
  /** 弹窗确认的变更原因；保证 setFieldValue 与 handleSubmit 之间不丢值。 */
  const pendingChangeReasonRef = React.useRef<string | null>(null)

  const initialFormValues = React.useMemo(
    () =>
      !isCreate && data ? hydrateFromCenter(data) : createDefaults(isCreate),
    [data, isCreate],
  )

  const form = useAppForm({
    defaultValues: initialFormValues,
    onSubmit: async ({ value }) => {
      setFormError(null)
      setResult(null)

      const hasStoredContactPhone = data?.sensitiveFields.some(
        (field) => field.label === "联系电话" || field.label === "联系人",
      )
      const validation = validateSupplierEditorFields(value, {
        hasStoredContactPhone,
        originalContactName: initialFormValues.contactName,
      })
      if (validation) {
        setFormError(validation)
        return
      }
      const changeReason = (
        pendingChangeReasonRef.current ?? value.changeReason
      ).trim()
      pendingChangeReasonRef.current = null
      if (changeReason.length < 2) {
        setFormError("请填写本次保存的变更原因")
        return
      }

      let fields = buildResourceFields("suppliers", value)
      try {
        const assetMaps = await resolvePendingMedia(value)
        fields = {
          ...fields,
          clearContact:
            !isCreate &&
            !value.contactName.trim() &&
            !value.contactPhone.trim() &&
            (Boolean(initialFormValues.contactName.trim()) ||
              editedSensitiveRef.current.has("contactPhone")),
          clearAddress:
            !isCreate &&
            !value.address.trim() &&
            editedSensitiveRef.current.has("address"),
          clearTaxProfile:
            !isCreate &&
            Boolean(initialFormValues.taxNo.trim()) &&
            !value.taxNo.trim(),
          qualificationFileAssetIds: assetMaps.qualification,
          contractFileAssetIds: assetMaps.contractFile,
          authorizationFileAssetIds: assetMaps.authorizationFile,
          foodLicenseFileAssetIds: assetMaps.foodLicense,
          legalPersonIdCardFileAssetIds: assetMaps.legalPersonIdCard,
          qualificationCapabilityCodes:
            data?.supplierQualificationCapabilityCodes,
        } as SupplierFields
      } catch (error) {
        setFormError(
          error instanceof Error ? error.message : "资质文件上传失败",
        )
        return
      }

      if (!isCreate) {
        if (!data || !revisionId || lockVersion == null) return
        const response = await reviseMutation.mutateAsync({
          resource: "suppliers",
          stableId: data.stableId,
          baseRevisionId: revisionId,
          expectedLockVersion: lockVersion,
          expectedPartyVersion: data.partyLockVersion,
          name: value.name.trim(),
          effectiveFrom: defaultImmediateEffectiveFrom(),
          changeReason,
          fields,
          idempotencyKey,
        })
        if (response.outcome === "succeeded") {
          toast.add({
            title: masterDataCopy.reviseSuccessTitle,
            description: `${masterDataCopy.resultNo} ${response.stableNo} · v${response.revisionNo}`,
            type: "success",
            timeout: 4000,
          })
          setIdempotencyKey(newIdempotencyKey("revise-supplier"))
          hydratedKeyRef.current = null
          await detailQuery.refetch()
          return
        }
        setResult(response)
        return
      }

      const response = await createMutation.mutateAsync({
        resource: "suppliers",
        name: value.name.trim(),
        effectiveFrom: defaultImmediateEffectiveFrom(),
        changeReason,
        fields,
        idempotencyKey,
      })
      if (response.outcome === "succeeded") {
        toast.add({
          title: masterDataCopy.createSuccessTitle,
          description: `${masterDataCopy.resultNo} ${response.stableNo} · v${response.revisionNo}`,
          type: "success",
          timeout: 4000,
        })
        router.replace(`/master-data/suppliers/${response.stableId}`)
        return
      }
      setResult(response)
    },
  })

  React.useEffect(() => {
    if (isCreate || !data) return
    const key = `${data.stableId}:${data.lockVersion}:${data.currentRevision.revisionId}`
    if (hydratedKeyRef.current === key) return
    form.reset(hydrateFromCenter(data))
    editedSensitiveRef.current.clear()
    hydratedKeyRef.current = key
  }, [data, form, isCreate])

  // 离开确认：返回列表 / 侧栏 / 刷新都受未保存保护
  React.useEffect(() => {
    const onBeforeUnload = (event: BeforeUnloadEvent) => {
      if (form.state.isDirty) {
        event.preventDefault()
      }
    }
    window.addEventListener("beforeunload", onBeforeUnload)
    return () => window.removeEventListener("beforeunload", onBeforeUnload)
    // eslint-disable-next-line react-hooks/exhaustive-deps -- 仅挂载时注册一次
  }, [])

  // 校验错误出现时滚动到顶部错误条
  React.useEffect(() => {
    if (formError) {
      errorRef.current?.scrollIntoView({ block: "center", behavior: "smooth" })
    }
  }, [formError])

  // 成功面板不滞留：表单再次变脏后清掉「已保存」结果（禁止在 Subscribe 渲染里 setState）
  React.useEffect(() => {
    const subscription = form.store.subscribe(() => {
      if (form.store.state.isDirty) {
        setResult((prev) => (prev ? null : prev))
      }
    })
    return () => subscription.unsubscribe()
  }, [form.store])

  const navigateAway = React.useCallback(
    (href: string) => {
      if (form.state.isDirty) {
        setPendingNav(href)
        setDiscardOpen(true)
        return
      }
      router.push(href)
    },
    [form.state.isDirty, router],
  )

  /** 敏感字段映射：label → 打码展示 + 揭示令牌 */
  const sensitiveByLabel = React.useMemo(() => {
    const map = new Map<
      string,
      { maskedValue: string; revealToken?: string }
    >()
    for (const field of data?.sensitiveFields ?? []) {
      map.set(field.label, {
        maskedValue: field.maskedValue,
        revealToken: field.revealToken,
      })
    }
    return map
  }, [data?.sensitiveFields])

  const listHref = "/master-data/suppliers"
  const pending = createMutation.isPending || reviseMutation.isPending
  const granted = accountQuery.data?.permissions
  const canCreate = hasPermission(granted, "supplier:create")
  const hasUpdatePermission = hasPermission(granted, "supplier:update")
  const hasDeletePermission = hasPermission(granted, "supplier:delete")
  const canRevealSensitive = hasPermission(granted, "supplier_sensitive:reveal")
  const canRevise =
    !isCreate &&
    hasUpdatePermission &&
    (data?.allowedActions.includes("CREATE_REVISION") ?? false)
  const canDisable =
    hasDeletePermission && (data?.allowedActions.includes("DISABLE") ?? false)
  const reviseBlocker = data?.actionBlockers.find(
    (b) => b.action === "CREATE_REVISION",
  )
  const disableBlocker = data?.actionBlockers.find(
    (b) => b.action === "DISABLE",
  )

  if (!isCreate && detailQuery.isPending) {
    return (
      <PageScaffold density="compact">
        <PageHeader
          title="供应商详情"
          description={masterDataCopy.centerLoading}
        />
        <div className="h-40 animate-pulse rounded-lg bg-muted" aria-busy />
      </PageScaffold>
    )
  }

  if (!isCreate && (detailQuery.isError || !data)) {
    return (
      <PageScaffold density="compact">
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
      </PageScaffold>
    )
  }

  const formId = "supplier-detail-form"
  const canEdit = isCreate ? canCreate : canRevise

  return (
    <>
      <form.Subscribe selector={(state) => state.values}>
        {(values) => {
          const title = isCreate
            ? masterDataCopy.supplierCreateTitle
            : values.name || data?.name || "供应商详情"
          const setFieldValue = (key: SupplierFieldKey, next: string) =>
            form.setFieldValue(key, next)
          /** 右上角保存：先校验字段，再弹窗填写变更原因。 */
          const requestSave = (event?: React.FormEvent) => {
            event?.preventDefault()
            const validation = validateSupplierEditorFields(values, {
              hasStoredContactPhone: data?.sensitiveFields.some(
                (field) =>
                  field.label === "联系电话" || field.label === "联系人",
              ),
              originalContactName: initialFormValues.contactName,
            })
            if (validation) {
              setFormError(validation)
              return
            }
            setFormError(null)
            setReasonDraft(
              isCreate
                ? values.changeReason.trim() || "新建供应商"
                : values.changeReason,
            )
            setReasonError(null)
            setSaveReasonOpen(true)
          }
          const confirmSaveWithReason = () => {
            const reason = reasonDraft.trim()
            if (reason.length < 2) {
              setReasonError("请填写本次保存的变更原因")
              return
            }
            setReasonError(null)
            pendingChangeReasonRef.current = reason
            form.setFieldValue("changeReason", reason)
            setSaveReasonOpen(false)
            void form.handleSubmit()
          }

          const phoneSensitive =
            sensitiveByLabel.get("联系电话") ?? sensitiveByLabel.get("联系人")
          const addressSensitive = sensitiveByLabel.get("经营地址")
          const bankSensitive = sensitiveByLabel.get("银行账号")

          const summaryRows: Array<{ label: string; value: string }> = [
            {
              label: masterDataCopy.fContactName,
              value: values.contactName.trim() || "—",
            },
            {
              label: masterDataCopy.fSettlement,
              value: values.settlement || "—",
            },
            {
              label: masterDataCopy.fSupplierRating,
              value: values.supplierRating || "—",
            },
            {
              label: masterDataCopy.fCapability,
              value: values.capability || "—",
            },
          ]

          return (
            <PageScaffold density="compact">
              <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                  {
                    id: "master-data",
                    label: "基础资料",
                    href: "/master-data",
                  },
                  {
                    id: "suppliers",
                    label: "供应商",
                    href: listHref,
                  },
                  {
                    id: "detail",
                    label: isCreate ? "新建供应商" : data?.stableNo || title,
                    current: true,
                  },
                ]}
                actions={
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    onClick={() => navigateAway(listHref)}
                  >
                    <ArrowLeftIcon data-icon="inline-start" aria-hidden />
                    返回列表
                  </Button>
                }
              />

              <form id={formId} className="space-y-4" onSubmit={requestSave}>
                <DocumentHeader
                  density="compact"
                  title={title}
                  documentNumber={isCreate ? "待生成" : data?.stableNo || "—"}
                  primaryStatus={
                    !isCreate && data
                      ? {
                          label: data.lifecycleStatusLabel,
                          tone: data.lifecycleTone,
                        }
                      : { label: "待创建", tone: "neutral" }
                  }
                  version={
                    !isCreate && data
                      ? data.currentRevision.revisionNo
                      : undefined
                  }
                  meta={
                    <span className="inline-flex flex-wrap items-center gap-x-1.5 gap-y-0.5">
                      <span>
                        企业主体{" "}
                        <span className="font-medium text-foreground">
                          {values.company.trim() || "待填写"}
                        </span>
                      </span>
                      <span className="text-border" aria-hidden="true">
                        ·
                      </span>
                      <span>
                        联系人{" "}
                        <span className="font-medium text-foreground">
                          {values.contactName.trim() || "待填写"}
                        </span>
                      </span>
                    </span>
                  }
                  secondaryActions={
                    !isCreate && data ? (
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
                    ) : null
                  }
                  primaryAction={
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
                  }
                />

                <div className="space-y-3">
                  {!isCreate && !canRevise ? (
                    <Alert variant="info">
                      <AlertTitle>你只能查看</AlertTitle>
                      <AlertDescription>
                        {reviseBlocker
                          ? masterDataCopy.centerUpdateBlocked(
                              reviseBlocker.message,
                            )
                          : "当前账号没有维护供应商资料的权限；需要修改请联系有权限的同事。"}
                      </AlertDescription>
                    </Alert>
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
                    <div ref={errorRef}>
                      <Alert variant="destructive">
                        <CircleAlertIcon aria-hidden />
                        <AlertTitle>填写不完整</AlertTitle>
                        <AlertDescription>{formError}</AlertDescription>
                      </Alert>
                    </div>
                  ) : null}

                  {/* 关键信息条：切换标签时始终可见，避免来回跳转确认合作事实。 */}
                  <dl
                    className={cn(
                      surfaceInsetClassName,
                      "grid grid-cols-2 gap-x-6 gap-y-3 px-4 py-3 sm:grid-cols-4",
                    )}
                  >
                    {summaryRows.map((row) => (
                      <div key={row.label} className="min-w-0">
                        <dt className="text-tiny text-muted-foreground">
                          {row.label}
                        </dt>
                        <dd
                          className="mt-0.5 truncate text-sm font-medium text-foreground"
                          title={row.value}
                        >
                          {row.value}
                        </dd>
                      </div>
                    ))}
                  </dl>

                  <div className={cn(surfacePanelClassName, "overflow-hidden")}>
                    <Tabs
                      value={activeSection}
                      onValueChange={(value) => {
                        if (value) setActiveSection(value)
                      }}
                      className="gap-0"
                    >
                      <TabsList
                        variant="line"
                        aria-label="供应商编辑分区"
                        className="sticky top-0 z-10 h-auto w-full flex-nowrap justify-start gap-0 overflow-x-auto rounded-none border-b border-border/60 bg-card/95 px-4 py-0 backdrop-blur supports-backdrop-filter:bg-card/85"
                      >
                        {SUPPLIER_SECTIONS.filter(
                          (section) => !isCreate || section.id !== "history",
                        ).map((section) => (
                          <TabsTrigger
                            key={section.id}
                            value={section.id}
                            className="h-11 flex-none rounded-none px-4 text-sm after:inset-x-3 after:bottom-0 after:h-0.5 after:rounded-full after:bg-primary data-active:font-semibold"
                          >
                            {section.label}
                          </TabsTrigger>
                        ))}
                      </TabsList>
                    </Tabs>

                    <div className="p-4 md:p-5">
                      {activeSection === "basic" && (
                        <SectionPanel
                          title="基本信息"
                          description="名称与企业主体必填；联系方式便于采购对接。"
                        >
                          <div className="grid gap-4 sm:grid-cols-2">
                            <FieldShell>
                              <Label htmlFor="supplier-name">名称 *</Label>
                              <Input
                                id="supplier-name"
                                value={values.name}
                                onChange={(e) =>
                                  setFieldValue("name", e.target.value)
                                }
                                placeholder="供应商名称"
                                disabled={!canEdit}
                              />
                            </FieldShell>
                            <FieldShell>
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
                            </FieldShell>
                            <FieldShell>
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
                            </FieldShell>
                            <FieldShell>
                              <Label htmlFor="supplier-credit-code">
                                {masterDataCopy.fCreditCode}
                              </Label>
                              <Input
                                id="supplier-credit-code"
                                value={values.creditCode}
                                onChange={(event) =>
                                  setFieldValue("creditCode", event.target.value)
                                }
                                placeholder="18 位统一社会信用代码"
                                disabled={!canEdit}
                              />
                            </FieldShell>
                            <FieldShell>
                              <SensitiveEditableField
                                label={masterDataCopy.fContactPhone}
                                id="supplier-contact-phone"
                                value={values.contactPhone}
                                maskedValue={phoneSensitive?.maskedValue}
                                revealToken={phoneSensitive?.revealToken}
                                onChange={(next) => {
                                  editedSensitiveRef.current.add("contactPhone")
                                  setFieldValue("contactPhone", next)
                                }}
                                disabled={!canEdit}
                                canReveal={canRevealSensitive}
                                placeholder="手机号或固定电话"
                              />
                            </FieldShell>
                            <FieldShell className="sm:col-span-2">
                              <SensitiveEditableField
                                label={masterDataCopy.fAddress}
                                id="supplier-address"
                                value={values.address}
                                maskedValue={addressSensitive?.maskedValue}
                                revealToken={addressSensitive?.revealToken}
                                onChange={(next) => {
                                  editedSensitiveRef.current.add("address")
                                  setFieldValue("address", next)
                                }}
                                placeholder="注册或经营地址"
                                disabled={!canEdit}
                                canReveal={canRevealSensitive}
                              />
                            </FieldShell>
                          </div>
                        </SectionPanel>
                      )}

                      {activeSection === "commercial" && (
                        <SectionPanel
                          title="商务合作"
                          description="能力、结算与主体用于采购选用；评估分便于后续优选。"
                        >
                          <div className="space-y-4">
                            <FieldShell>
                              <Label>{masterDataCopy.fCapability}</Label>
                              <CapabilityCheckboxGroup
                                value={values.capability}
                                onChange={(next) =>
                                  setFieldValue("capability", next)
                                }
                                disabled={!canEdit}
                              />
                            </FieldShell>

                            <div className="grid gap-4 sm:grid-cols-2">
                              <FieldShell>
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
                              </FieldShell>
                              <FieldShell>
                                <Label htmlFor="supplier-business-category">
                                  {masterDataCopy.fBusinessCategory}
                                </Label>
                                <Input
                                  id="supplier-business-category"
                                  value={values.businessCategory}
                                  onChange={(e) =>
                                    setFieldValue(
                                      "businessCategory",
                                      e.target.value,
                                    )
                                  }
                                  placeholder="如：礼盒、茶叶、卡券"
                                  disabled={!canEdit}
                                />
                              </FieldShell>
                              <FieldShell>
                                <Label>
                                  {masterDataCopy.fSigningEntity}
                                </Label>
                                <SettlementPartyCombobox
                                  value={values.signingEntity || undefined}
                                  onValueChange={(value) =>
                                    setFieldValue("signingEntity", value ?? "")
                                  }
                                  parties={partyOptionsQuery.data ?? []}
                                  placeholder="选择与供应商签约的公司主体"
                                  disabled={!canEdit}
                                />
                              </FieldShell>
                              <FieldShell>
                                <Label>
                                  {masterDataCopy.fPaymentEntity}
                                </Label>
                                <SettlementPartyCombobox
                                  value={values.paymentEntity || undefined}
                                  onValueChange={(value) =>
                                    setFieldValue("paymentEntity", value ?? "")
                                  }
                                  parties={partyOptionsQuery.data ?? []}
                                  placeholder="选择向供应商付款的公司主体"
                                  disabled={!canEdit}
                                />
                              </FieldShell>
                            </div>

                            <div
                              className={cn(
                                surfaceInsetClassName,
                                "grid gap-4 p-4 sm:grid-cols-3",
                              )}
                            >
                              <FieldShell>
                                <Label htmlFor="supplier-initial-score">
                                  {masterDataCopy.fInitialScore}
                                </Label>
                                <Input
                                  id="supplier-initial-score"
                                  value={values.initialScore}
                                  onChange={(e) =>
                                    setFieldValue(
                                      "initialScore",
                                      e.target.value,
                                    )
                                  }
                                  placeholder="如：85"
                                  disabled={!canEdit}
                                />
                              </FieldShell>
                              <FieldShell>
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
                              </FieldShell>
                              <FieldShell>
                                <Label htmlFor="supplier-current-score">
                                  {masterDataCopy.fCurrentScore}
                                </Label>
                                <Input
                                  id="supplier-current-score"
                                  value={values.currentScore}
                                  onChange={(e) =>
                                    setFieldValue(
                                      "currentScore",
                                      e.target.value,
                                    )
                                  }
                                  placeholder="如：88"
                                  disabled={!canEdit}
                                />
                              </FieldShell>
                            </div>
                          </div>
                        </SectionPanel>
                      )}

                      {activeSection === "contract" && (
                        <SectionPanel
                          title="合同与资质"
                          description="合同、授权与证照集中维护；有效期到期后需重新上传。"
                        >
                          <div className="space-y-5">
                            <CredentialGroup
                              title="采购合同"
                              description="维护当前合作合同的编号、有效期与电子附件。"
                            >
                              <div className="grid gap-5 lg:grid-cols-2">
                                <div className="space-y-4">
                                  <FieldShell>
                                    <Label htmlFor="supplier-contract-no">
                                      {masterDataCopy.fContractNo}
                                    </Label>
                                    <Input
                                      id="supplier-contract-no"
                                      value={values.contractNo}
                                      onChange={(e) =>
                                        setFieldValue(
                                          "contractNo",
                                          e.target.value,
                                        )
                                      }
                                      placeholder="合同编号"
                                      disabled={!canEdit}
                                    />
                                  </FieldShell>
                                  <div className="grid gap-4 sm:grid-cols-2">
                                    <FieldShell>
                                      <Label>
                                        {masterDataCopy.fContractValidFrom}
                                      </Label>
                                      <DatePicker
                                        value={
                                          values.contractValidFrom || undefined
                                        }
                                        onValueChange={(next) =>
                                          setFieldValue(
                                            "contractValidFrom",
                                            next ?? "",
                                          )
                                        }
                                        disabled={!canEdit}
                                        className="w-full"
                                      />
                                    </FieldShell>
                                    <FieldShell>
                                      <Label>
                                        {masterDataCopy.fContractValidTo}
                                      </Label>
                                      <DatePicker
                                        value={
                                          values.contractValidTo || undefined
                                        }
                                        onValueChange={(next) =>
                                          setFieldValue(
                                            "contractValidTo",
                                            next ?? "",
                                          )
                                        }
                                        disabled={!canEdit}
                                        className="w-full"
                                      />
                                    </FieldShell>
                                  </div>
                                </div>
                                <div className="border-border/60 lg:border-l lg:pl-5">
                                  <MediaListField
                                    label={masterDataCopy.fContractFile}
                                    hint={
                                      masterDataCopy.supplierQualificationHint
                                    }
                                    value={values.contractFile}
                                    onChange={(next) =>
                                      setFieldValue("contractFile", next)
                                    }
                                    urlByFileName={mediaUrlsFor("contractFile")}
                                    onFilesSelected={rememberMediaFiles}
                                    disabled={!canEdit}
                                    accept="image/jpeg,image/png,image/webp,application/pdf"
                                  />
                                </div>
                              </div>
                            </CredentialGroup>

                            <CredentialGroup
                              title="品牌与经营授权"
                              description="授权书有效期与附件成组维护，便于到期前统一核验。"
                            >
                              <div className="grid gap-5 lg:grid-cols-2">
                                <div className="grid content-start gap-4 sm:grid-cols-2">
                                  <FieldShell>
                                    <Label>
                                      {masterDataCopy.fAuthorizationValidFrom}
                                    </Label>
                                    <DatePicker
                                      value={
                                        values.authorizationValidFrom ||
                                        undefined
                                      }
                                      onValueChange={(next) =>
                                        setFieldValue(
                                          "authorizationValidFrom",
                                          next ?? "",
                                        )
                                      }
                                      disabled={!canEdit}
                                      className="w-full"
                                    />
                                  </FieldShell>
                                  <FieldShell>
                                    <Label>
                                      {masterDataCopy.fAuthorizationValidTo}
                                    </Label>
                                    <DatePicker
                                      value={
                                        values.authorizationValidTo || undefined
                                      }
                                      onValueChange={(next) =>
                                        setFieldValue(
                                          "authorizationValidTo",
                                          next ?? "",
                                        )
                                      }
                                      disabled={!canEdit}
                                      className="w-full"
                                    />
                                  </FieldShell>
                                </div>
                                <div className="border-border/60 lg:border-l lg:pl-5">
                                  <MediaListField
                                    label={masterDataCopy.fAuthorizationFile}
                                    hint={
                                      masterDataCopy.supplierQualificationHint
                                    }
                                    value={values.authorizationFile}
                                    onChange={(next) =>
                                      setFieldValue("authorizationFile", next)
                                    }
                                    urlByFileName={mediaUrlsFor("authorizationFile")}
                                    onFilesSelected={rememberMediaFiles}
                                    disabled={!canEdit}
                                    accept="image/jpeg,image/png,image/webp,application/pdf"
                                  />
                                </div>
                              </div>
                            </CredentialGroup>

                            <CredentialGroup
                              title="企业经营资质"
                              description="按证照类型分别归档，缺少的材料可后续补充。"
                            >
                              <div className="grid gap-4 lg:grid-cols-3">
                                <div className="rounded-md border border-border/60 bg-background p-4">
                                  <MediaListField
                                    label={masterDataCopy.fQualification}
                                    hint={
                                      masterDataCopy.supplierQualificationHint
                                    }
                                    value={values.qualification}
                                    onChange={(next) =>
                                      setFieldValue("qualification", next)
                                    }
                                    urlByFileName={mediaUrlsFor("qualification")}
                                    onFilesSelected={rememberMediaFiles}
                                    disabled={!canEdit}
                                    accept="image/jpeg,image/png,image/webp,application/pdf"
                                  />
                                </div>
                                <div className="rounded-md border border-border/60 bg-background p-4">
                                  <MediaListField
                                    label={masterDataCopy.fFoodLicense}
                                    hint={
                                      masterDataCopy.supplierQualificationHint
                                    }
                                    value={values.foodLicense}
                                    onChange={(next) =>
                                      setFieldValue("foodLicense", next)
                                    }
                                    urlByFileName={mediaUrlsFor("foodLicense")}
                                    onFilesSelected={rememberMediaFiles}
                                    disabled={!canEdit}
                                    accept="image/jpeg,image/png,image/webp,application/pdf"
                                  />
                                </div>
                                <div className="rounded-md border border-border/60 bg-background p-4">
                                  <MediaListField
                                    label={masterDataCopy.fLegalPersonIdCard}
                                    hint={
                                      masterDataCopy.supplierQualificationHint
                                    }
                                    value={values.legalPersonIdCard}
                                    onChange={(next) =>
                                      setFieldValue("legalPersonIdCard", next)
                                    }
                                    urlByFileName={mediaUrlsFor("legalPersonIdCard")}
                                    onFilesSelected={rememberMediaFiles}
                                    disabled={!canEdit}
                                    accept="image/jpeg,image/png,image/webp,application/pdf"
                                  />
                                </div>
                              </div>
                            </CredentialGroup>
                          </div>
                        </SectionPanel>
                      )}

                      {activeSection === "invoice" && (
                        <SectionPanel
                          title="开票信息"
                          description="税号与银行信息用于采购开票与付款。"
                        >
                          <div className="grid gap-4 sm:grid-cols-2">
                            <FieldShell>
                              <Label htmlFor="supplier-tax-no">
                                {masterDataCopy.fTaxNo}
                              </Label>
                              <Input
                                id="supplier-tax-no"
                                value={values.taxNo}
                                onChange={(event) =>
                                  setFieldValue("taxNo", event.target.value)
                                }
                                disabled={!canEdit}
                                placeholder="纳税人识别号"
                              />
                            </FieldShell>
                            <FieldShell className="sm:col-span-2">
                              <Label>银行账户</Label>
                              <div className="rounded-md border bg-muted/30 px-3 py-2 text-sm">
                                <span>{values.bankName || "未维护"}</span>
                                <span className="mx-2 text-muted-foreground">·</span>
                                <code className="num">
                                  {bankSensitive?.maskedValue || "请从财务主体资料维护"}
                                </code>
                              </div>
                              <p className="text-xs text-muted-foreground">
                                银行账户属于财务敏感资料，本页只展示摘要，不在供应商资料命令中修改。
                              </p>
                            </FieldShell>
                            <FieldShell>
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
                            </FieldShell>
                            <FieldShell>
                              <Label htmlFor="supplier-invoice-tax-rate">
                                {masterDataCopy.fInvoiceTaxRate}
                              </Label>
                              <Input
                                id="supplier-invoice-tax-rate"
                                value={values.invoiceTaxRate}
                                onChange={(e) =>
                                  setFieldValue(
                                    "invoiceTaxRate",
                                    e.target.value,
                                  )
                                }
                                placeholder="如：13%"
                                disabled={!canEdit}
                              />
                            </FieldShell>
                          </div>
                        </SectionPanel>
                      )}

                      {activeSection === "history" && !isCreate && data && (
                        <div className="-mt-5">
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
                                  label: `创建于 ${rev.effectiveFrom}`,
                                },
                                reason: (
                                  <div className="space-y-1">
                                    <div>
                                      {masterDataCopy.centerHistoryName}：
                                      <strong>{rev.nameSnapshot}</strong>
                                    </div>
                                    <div>{rev.changeReason}</div>
                                    <div className="flex flex-wrap gap-2">
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
                                data.usageSummary.historicalReferenceCount,
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
                                        {formatDateTime(
                                          ev.at,
                                          "full",
                                          "passthrough",
                                        )}
                                      </span>
                                      <span>{ev.actor}</span>
                                      <Badge variant="outline">
                                        {ev.action}
                                      </Badge>
                                    </div>
                                    <div className="mt-1 text-muted-foreground">
                                      {ev.detail}
                                    </div>
                                  </li>
                                ))}
                              </ul>
                            )}
                          </DocumentSection>
                        </div>
                      )}
                    </div>
                  </div>
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

              <Dialog
                open={saveReasonOpen}
                onOpenChange={(open) => {
                  setSaveReasonOpen(open)
                  if (!open) setReasonError(null)
                }}
              >
                <DialogContent className="sm:max-w-md">
                  <DialogHeader>
                    <DialogTitle>
                      {isCreate ? "确认创建" : "确认保存"}
                    </DialogTitle>
                    <DialogDescription>
                      {isCreate
                        ? "创建后生成供应商档案；请填写创建说明。"
                        : "保存将生成新版本；变更原因必填。"}
                    </DialogDescription>
                  </DialogHeader>
                  <div className="space-y-1.5">
                    <Label htmlFor="supplier-save-reason">
                      {masterDataCopy.fieldChangeReason} *
                    </Label>
                    <Textarea
                      id="supplier-save-reason"
                      value={reasonDraft}
                      onChange={(e) => {
                        setReasonDraft(e.target.value)
                        if (reasonError) setReasonError(null)
                      }}
                      rows={3}
                      placeholder={
                        isCreate
                          ? "新建原因"
                          : "说明本次修改内容，保存后形成新版本"
                      }
                      autoFocus
                    />
                    {reasonError ? (
                      <p className="text-xs text-destructive" role="alert">
                        {reasonError}
                      </p>
                    ) : null}
                  </div>
                  <DialogFooter>
                    <DialogClose
                      render={<Button type="button" variant="outline" />}
                    >
                      取消
                    </DialogClose>
                    <Button
                      type="button"
                      disabled={pending}
                      onClick={confirmSaveWithReason}
                    >
                      <SaveIcon data-icon="inline-start" aria-hidden />
                      {isCreate
                        ? masterDataCopy.createSubmit
                        : masterDataCopy.reviseSubmit}
                    </Button>
                  </DialogFooter>
                </DialogContent>
              </Dialog>

              <DiscardConfirmDialog
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                title="放弃未保存的更改？"
                description="本次修改尚未保存，离开后将丢失。"
                confirmLabel="放弃更改"
                cancelLabel="继续编辑"
                onConfirm={() => {
                  setDiscardOpen(false)
                  if (pendingNav) {
                    setPendingNav(null)
                    router.push(pendingNav)
                  }
                }}
              />
            </PageScaffold>
          )
        }}
      </form.Subscribe>
    </>
  )
}
