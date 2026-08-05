"use client"

import * as React from "react"
import { PlusIcon } from "lucide-react"
import { z } from "zod"

import {
  ConflictResolutionDialog,
  DiscardConfirmDialog,
  DocumentSection,
  FormalActionResult,
  OptionCombobox,
  OwnerCombobox,
} from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { useSelector } from "@tanstack/react-form"
import {
  DEMO_OWNER_OPTIONS,
  PAYMENT_TERM_OPTIONS,
  paymentTermLabel,
} from "@/lib/business-options"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
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
import { Label } from "@/components/ui/label"
import { getW03SensitiveReveal } from "@/features/customers/session"
import {
  useCreateCustomerMutation,
  useQueryCustomerIdempotencyMutation,
  useSaveCustomerDetailsMutation,
} from "@/features/customers/queries"
import type {
  CustomerCenterView,
  CustomerMutationResult,
} from "@/features/customers/types"

const contactRowSchema = z.object({
  name: z.string().trim().min(1, "请填写联系人姓名"),
  title: z.string(),
  phone: z.string(),
  email: z.string(),
  isDefault: z.boolean(),
})

const addressRowSchema = z.object({
  addressType: z.string().trim().min(1, "请选择地址类型"),
  address: z.string().trim().min(1, "请填写地址"),
  isDefault: z.boolean(),
})

const bankAccountRowSchema = z.object({
  accountName: z.string().trim().min(1, "请填写户名"),
  bankName: z.string().trim().min(1, "请填写银行 / 支行"),
  accountNumber: z.string().trim().min(1, "请填写账号"),
  isDefault: z.boolean(),
})

const createSchema = z.object({
  legalName: z.string().trim().min(2, "请填写法定名称"),
  shortName: z.string(),
  unifiedCreditCode: z.string(),
  ownerUserId: z.string().min(1, "请选择负责销售"),
  defaultPaymentTerm: z.string(),
  changeReason: z.string(),
  contacts: z.array(contactRowSchema),
  addresses: z.array(addressRowSchema),
  bankAccounts: z.array(bankAccountRowSchema),
})

const editSchema = z.object({
  legalName: z.string().trim().min(2, "请填写法定名称"),
  shortName: z.string(),
  unifiedCreditCode: z.string(),
  ownerUserId: z.string(),
  defaultPaymentTerm: z.string(),
  changeReason: z.string().trim().min(2, "请填写修订原因"),
  contacts: z.array(contactRowSchema),
  addresses: z.array(addressRowSchema),
  bankAccounts: z.array(bankAccountRowSchema),
})

type ContactRow = {
  name: string
  title: string
  phone: string
  email: string
  isDefault: boolean
}

type AddressRow = {
  addressType: string
  address: string
  isDefault: boolean
}

type BankAccountRow = {
  accountName: string
  bankName: string
  accountNumber: string
  isDefault: boolean
}

type FormValues = {
  legalName: string
  shortName: string
  unifiedCreditCode: string
  ownerUserId: string
  defaultPaymentTerm: string
  changeReason: string
  contacts: ContactRow[]
  addresses: AddressRow[]
  bankAccounts: BankAccountRow[]
}

const ADDRESS_TYPE_OPTIONS = [
  { value: "履约地址", label: "履约地址" },
  { value: "注册地址", label: "注册地址" },
  { value: "开票地址", label: "开票地址" },
  { value: "办公地址", label: "办公地址" },
] as const

function newIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

function editableValue(token: string | undefined, masked: string): string {
  if (!token) return masked === "—" ? "" : masked
  return getW03SensitiveReveal(token) ?? masked
}

function buildDefaults(
  mode: "create" | "edit",
  customer: CustomerCenterView | undefined
): FormValues {
  if (mode === "create") {
    return {
      legalName: "",
      shortName: "",
      unifiedCreditCode: "",
      ownerUserId: "",
      defaultPaymentTerm: "POSTPAY_NET30",
      changeReason: "",
      contacts: [],
      addresses: [],
      bankAccounts: [],
    }
  }
  return {
    legalName: customer!.currentRevision.legalName,
    shortName: customer!.currentRevision.shortName ?? "",
    unifiedCreditCode: customer!.currentRevision.unifiedCreditCode ?? "",
    ownerUserId: "",
    defaultPaymentTerm: customer!.currentRevision.defaultPaymentTerm ?? "",
    changeReason: "",
    contacts: customer!.contacts.map((c) => ({
      name: c.name,
      title: c.title ?? "",
      phone: editableValue(c.phoneRevealToken, c.phoneMasked),
      email: c.email ?? "",
      isDefault: c.isDefault,
    })),
    addresses: customer!.addresses.map((a) => ({
      addressType: a.addressType,
      address: editableValue(a.addressRevealToken, a.addressMasked),
      isDefault: a.isDefault,
    })),
    bankAccounts: customer!.bankAccounts.map((b) => ({
      accountName: b.accountName,
      bankName: b.bankName,
      accountNumber: editableValue(b.accountRevealToken, b.accountMasked),
      isDefault: b.isDefault,
    })),
  }
}

function FormSection({
  grouped,
  title,
  description,
  action,
  children,
}: {
  grouped: boolean
  title: string
  description?: string
  action?: React.ReactNode
  children: React.ReactNode
}) {
  if (!grouped) {
    return (
      <div className="space-y-2">
        <div className="flex items-center justify-between gap-2">
          <div>
            <p className="text-sm font-medium text-foreground">{title}</p>
            {description ? (
              <p className="text-xs text-muted-foreground">{description}</p>
            ) : null}
          </div>
          {action}
        </div>
        {children}
      </div>
    )
  }
  return (
    <DocumentSection title={title} description={description} action={action}>
      {children}
    </DocumentSection>
  )
}

/**
 * 客户资料表单：创建（对话框内）与编辑（页面内）共用同一套字段、
 * 校验、敏感值处理、幂等提交与结果状态；外层只决定容器。
 */
export function CustomerForm({
  mode,
  grouped = false,
  customer,
  onCancel,
  onSucceeded,
  onDirtyChange,
}: {
  mode: "create" | "edit"
  /** 页面内编辑按分区展示（DocumentSection）；对话框内用紧凑布局。 */
  grouped?: boolean
  /** mode="edit" 必传。 */
  customer?: CustomerCenterView
  onCancel: () => void
  /** 成功回调；revisionNo 供页面展示「已保存 · 新版本 vN」反馈。 */
  onSucceeded: (customerId: string, revisionNo?: number) => void
  /** 表单是否含未保存输入（对话框容器用于拦截 X / Esc / 遮罩关闭）。 */
  onDirtyChange?: (isDirty: boolean) => void
}) {
  const createMutation = useCreateCustomerMutation()
  const saveMutation = useSaveCustomerDetailsMutation()
  const queryIdempotency = useQueryCustomerIdempotencyMutation()
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey(mode === "create" ? "create" : "revise")
  )
  const [simulate, setSimulate] = React.useState<"ok" | "conflict" | "unknown">(
    "ok"
  )
  const [result, setResult] = React.useState<CustomerMutationResult | null>(
    null
  )
  const [conflictOpen, setConflictOpen] = React.useState(false)
  const [maskedBlocked, setMaskedBlocked] = React.useState(false)

  const defaults = React.useMemo<FormValues>(
    () => buildDefaults(mode, customer),
    [customer, mode]
  )

  const form = useAppForm({
    defaultValues: defaults,
    validators: { onChange: mode === "create" ? createSchema : editSchema },
    onSubmit: async ({ value }) => {
      // 防脱敏值写回：无揭示令牌时预填的是打码文本，保存前必须已揭示。
      const maskedContacts = value.contacts.filter((row) =>
        row.phone.includes("*")
      )
      const maskedAddresses = value.addresses.filter((row) =>
        row.address.includes("*")
      )
      const maskedAccounts = value.bankAccounts.filter((row) =>
        row.accountNumber.includes("*")
      )
      if (
        maskedContacts.length > 0 ||
        maskedAddresses.length > 0 ||
        maskedAccounts.length > 0
      ) {
        setMaskedBlocked(true)
        return
      }
      setMaskedBlocked(false)
      const contacts = value.contacts.map((row) => ({
        name: row.name.trim(),
        title: row.title.trim() || undefined,
        phone: row.phone.trim() || undefined,
        email: row.email.trim() || undefined,
        isDefault: row.isDefault,
      }))
      const addresses = value.addresses.map((row) => ({
        addressType: row.addressType.trim(),
        address: row.address.trim(),
        isDefault: row.isDefault,
      }))
      const bankAccounts = value.bankAccounts.map((row) => ({
        accountName: row.accountName.trim(),
        bankName: row.bankName.trim(),
        accountNumber: row.accountNumber.trim(),
        isDefault: row.isDefault,
      }))

      const response =
        mode === "create"
          ? await createMutation.mutateAsync({
              legalName: value.legalName.trim(),
              shortName: value.shortName.trim() || undefined,
              unifiedCreditCode: value.unifiedCreditCode.trim() || undefined,
              defaultPaymentTerm: value.defaultPaymentTerm.trim()
                ? paymentTermLabel(value.defaultPaymentTerm.trim())
                : undefined,
              ownerUserId: value.ownerUserId,
              ownerName:
                DEMO_OWNER_OPTIONS.find(
                  (o) => o.userId === value.ownerUserId
                )?.displayName ?? value.ownerUserId,
              contacts,
              addresses,
              bankAccounts,
              idempotencyKey,
              simulate,
            })
          : await saveMutation.mutateAsync({
              customerId: customer!.customerId,
              expectedLockVersion: customer!.lockVersion,
              baseRevisionId: customer!.currentRevision.revisionId,
              legalName: value.legalName.trim(),
              shortName: value.shortName.trim() || undefined,
              unifiedCreditCode: value.unifiedCreditCode.trim() || undefined,
              changeReason: value.changeReason.trim(),
              contacts,
              addresses,
              bankAccounts,
              idempotencyKey,
              simulate,
            })

      setResult(response)
      if (response.outcome === "conflict") {
        setConflictOpen(true)
      }
      if (response.outcome === "succeeded") {
        form.reset()
        onSucceeded(response.customerId, response.revisionNo)
      }
    },
  })

  const dirty = useSelector(form.store, (state) => state.isDirty)
  const [discardOpen, setDiscardOpen] = React.useState(false)

  React.useEffect(() => {
    onDirtyChange?.(dirty)
  }, [dirty, onDirtyChange])

  React.useEffect(() => {
    if (!dirty) return
    const onBeforeUnload = (e: BeforeUnloadEvent) => {
      e.preventDefault()
      e.returnValue = "当前输入尚未提交，刷新后将丢失。"
    }
    window.addEventListener("beforeunload", onBeforeUnload)
    return () => window.removeEventListener("beforeunload", onBeforeUnload)
  }, [dirty])

  const resetSession = () => {
    setResult(null)
    setConflictOpen(false)
    setIdempotencyKey(newIdempotencyKey(mode === "create" ? "create" : "revise"))
    form.reset()
  }

  const isPending =
    (mode === "create" ? createMutation.isPending : saveMutation.isPending) ||
    queryIdempotency.isPending
  const submitLabel =
    mode === "create"
      ? createMutation.isPending
        ? "提交中…"
        : "创建客户"
      : saveMutation.isPending
        ? "保存中…"
        : "保存修订"

  return (
    <form
      className={grouped ? "space-y-4" : "flex flex-col gap-4"}
      onSubmit={(e) => {
        e.preventDefault()
        void form.handleSubmit()
      }}
    >
      {grouped ? (
        <FormSection
          grouped={grouped}
          title="主体身份与客户角色"
          description="保存后生成新基础资料版本，历史单据记录不变"
        >
          <div className="grid gap-4 sm:grid-cols-2">
            <form.AppField
              name="legalName"
              children={(field) => <field.TextField label="法定名称" />}
            />
            <form.AppField
              name="shortName"
              children={(field) => <field.TextField label="客户简称" />}
            />
            <form.AppField
              name="unifiedCreditCode"
              children={(field) => (
                <field.TextField label="统一社会信用代码" />
              )}
            />
            <div className="sm:col-span-2">
              <form.AppField
                name="changeReason"
                children={(field) => (
                  <field.TextareaField
                    label="修订原因"
                    placeholder="必填，写入修订时间线"
                  />
                )}
              />
            </div>
          </div>
        </FormSection>
      ) : (
        <div className="grid gap-4 sm:grid-cols-2">
          <form.AppField
            name="legalName"
            children={(field) => (
              <field.TextField label="法定名称" placeholder="企业全称" />
            )}
          />
          <form.AppField
            name="ownerUserId"
            children={(field) => {
              const isInvalid =
                field.state.meta.isTouched && !field.state.meta.isValid
              const errors = toFieldErrors(field.state.meta.errors)
              return (
                <Field data-invalid={isInvalid || undefined}>
                  <FieldLabel htmlFor="customer-form-owner">
                    负责销售
                  </FieldLabel>
                  <OwnerCombobox
                    value={field.state.value || undefined}
                    onValueChange={(id) => field.handleChange(id ?? "")}
                    owners={DEMO_OWNER_OPTIONS}
                    placeholder="搜索负责人"
                  />
                  {isInvalid ? <FieldError errors={errors} /> : null}
                </Field>
              )
            }}
          />
          <form.AppField
            name="shortName"
            children={(field) => (
              <field.TextField label="客户简称" placeholder="可选" />
            )}
          />
          <form.AppField
            name="unifiedCreditCode"
            children={(field) => (
              <field.TextField
                label="统一社会信用代码"
                placeholder="可选；不作自动合并依据"
              />
            )}
          />
          <form.AppField
            name="defaultPaymentTerm"
            children={(field) => (
              <field.SelectField
                label="默认付款条件"
                options={PAYMENT_TERM_OPTIONS}
                placeholder="录单提示"
              />
            )}
          />
        </div>
      )}

      <FormSection
        grouped={grouped}
        title="联系人"
        description="可多条；手机在详情页按权限打码展示"
        action={
          <form.AppField name="contacts">
            {(field) => (
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() =>
                  field.pushValue({
                    name: "",
                    title: "",
                    phone: "",
                    email: "",
                    isDefault: field.state.value.length === 0,
                  })
                }
              >
                <PlusIcon aria-hidden="true" />
                添加联系人
              </Button>
            )}
          </form.AppField>
        }
      >
        <form.AppField name="contacts">
          {(field) =>
            field.state.value.length === 0 ? (
              <p className="text-xs text-muted-foreground">
                {mode === "create"
                  ? "暂不填写；创建后可在客户详情「联系与地址」维护。"
                  : "暂无联系人"}
              </p>
            ) : (
              field.state.value.map((_row, index) => (
                <div
                  key={`contact-${index}`}
                  className="space-y-2 rounded-lg border border-border p-3"
                >
                  <div className="grid gap-2 sm:grid-cols-2">
                    <form.AppField name={`contacts[${index}].name`}>
                      {(nested) => <nested.TextField label="姓名" />}
                    </form.AppField>
                    <form.AppField name={`contacts[${index}].title`}>
                      {(nested) => <nested.TextField label="职务" />}
                    </form.AppField>
                    <form.AppField name={`contacts[${index}].phone`}>
                      {(nested) => (
                        <nested.TextField
                          label="手机"
                          placeholder="11 位手机号"
                        />
                      )}
                    </form.AppField>
                    <form.AppField name={`contacts[${index}].email`}>
                      {(nested) => (
                        <nested.TextField label="邮箱" placeholder="可选" />
                      )}
                    </form.AppField>
                  </div>
                  <div className="flex items-center justify-between gap-2">
                    <form.AppField name={`contacts[${index}].isDefault`}>
                      {(nested) => (
                        <label className="flex items-center gap-2 text-sm">
                          <Checkbox
                            checked={nested.state.value}
                            onCheckedChange={(checked) =>
                              nested.handleChange(checked === true)
                            }
                          />
                          默认联系人
                        </label>
                      )}
                    </form.AppField>
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      onClick={() => field.removeValue(index)}
                    >
                      移除
                    </Button>
                  </div>
                </div>
              ))
            )
          }
        </form.AppField>
      </FormSection>

      <FormSection
        grouped={grouped}
        title="地址"
        description="履约地址在详情页按权限打码展示"
        action={
          <form.AppField name="addresses">
            {(field) => (
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() =>
                  field.pushValue({
                    addressType: "履约地址",
                    address: "",
                    isDefault: field.state.value.length === 0,
                  })
                }
              >
                <PlusIcon aria-hidden="true" />
                添加地址
              </Button>
            )}
          </form.AppField>
        }
      >
        <form.AppField name="addresses">
          {(field) =>
            field.state.value.length === 0 ? (
              <p className="text-xs text-muted-foreground">
                {mode === "create"
                  ? "暂不填写；创建后可在客户详情「联系与地址」维护。"
                  : "暂无地址"}
              </p>
            ) : (
              field.state.value.map((_row, index) => (
                <div
                  key={`address-${index}`}
                  className="space-y-2 rounded-lg border border-border p-3"
                >
                  <div className="grid gap-2 sm:grid-cols-2">
                    <form.AppField name={`addresses[${index}].addressType`}>
                      {(nested) => (
                        <nested.SelectField
                          label="地址类型"
                          options={ADDRESS_TYPE_OPTIONS}
                        />
                      )}
                    </form.AppField>
                    <form.AppField name={`addresses[${index}].address`}>
                      {(nested) => (
                        <nested.TextField
                          label="地址"
                          placeholder="省市区 + 详细地址"
                        />
                      )}
                    </form.AppField>
                  </div>
                  <div className="flex items-center justify-between gap-2">
                    <form.AppField name={`addresses[${index}].isDefault`}>
                      {(nested) => (
                        <label className="flex items-center gap-2 text-sm">
                          <Checkbox
                            checked={nested.state.value}
                            onCheckedChange={(checked) =>
                              nested.handleChange(checked === true)
                            }
                          />
                          默认地址
                        </label>
                      )}
                    </form.AppField>
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      onClick={() => field.removeValue(index)}
                    >
                      移除
                    </Button>
                  </div>
                </div>
              ))
            )
          }
        </form.AppField>
      </FormSection>

      <FormSection
        grouped={grouped}
        title="银行账户"
        description="账号默认只显示末四位；完整显示需授权，操作会留记录"
        action={
          <form.AppField name="bankAccounts">
            {(field) => (
              <Button
                type="button"
                size="sm"
                variant="outline"
                onClick={() =>
                  field.pushValue({
                    accountName: "",
                    bankName: "",
                    accountNumber: "",
                    isDefault: field.state.value.length === 0,
                  })
                }
              >
                <PlusIcon aria-hidden="true" />
                添加账户
              </Button>
            )}
          </form.AppField>
        }
      >
        <form.AppField name="bankAccounts">
          {(field) =>
            field.state.value.length === 0 ? (
              <p className="text-xs text-muted-foreground">
                {mode === "create"
                  ? "暂不填写；创建后可由授权财务维护。"
                  : "暂无银行账户"}
              </p>
            ) : (
              field.state.value.map((_row, index) => (
                <div
                  key={`bank-${index}`}
                  className="space-y-2 rounded-lg border border-border p-3"
                >
                  <div className="grid gap-2 sm:grid-cols-2">
                    <form.AppField name={`bankAccounts[${index}].accountName`}>
                      {(nested) => <nested.TextField label="户名" />}
                    </form.AppField>
                    <form.AppField name={`bankAccounts[${index}].bankName`}>
                      {(nested) => <nested.TextField label="银行 / 支行" />}
                    </form.AppField>
                    <form.AppField name={`bankAccounts[${index}].accountNumber`}>
                      {(nested) => <nested.TextField label="账号" />}
                    </form.AppField>
                  </div>
                  <div className="flex items-center justify-between gap-2">
                    <form.AppField name={`bankAccounts[${index}].isDefault`}>
                      {(nested) => (
                        <label className="flex items-center gap-2 text-sm">
                          <Checkbox
                            checked={nested.state.value}
                            onCheckedChange={(checked) =>
                              nested.handleChange(checked === true)
                            }
                          />
                          默认账户
                        </label>
                      )}
                    </form.AppField>
                    <Button
                      type="button"
                      size="sm"
                      variant="ghost"
                      onClick={() => field.removeValue(index)}
                    >
                      移除
                    </Button>
                  </div>
                </div>
              ))
            )
          }
        </form.AppField>
      </FormSection>

      {maskedBlocked ? (
        <div className="space-y-2">
          <Alert variant="warning" role="alert">
            <AlertTitle>有敏感信息尚未揭示，暂不能保存</AlertTitle>
            <AlertDescription>
              手机号、地址或银行账号仍以打码文本显示。请先点击「显示」揭示并确认为真实内容后再保存，避免把打码文本写入资料。
            </AlertDescription>
          </Alert>
        </div>
      ) : null}

      <details className="rounded-lg border border-dashed border-border px-3 py-2 text-xs text-muted-foreground">
        <summary className="cursor-pointer select-none">
          演示模式（模拟提交结果）
        </summary>
        <div className="mt-2 space-y-2">
          <Label htmlFor="customer-form-simulate">演示结果</Label>
          <OptionCombobox
            id="customer-form-simulate"
            value={simulate}
            onValueChange={(v) =>
              setSimulate((v ?? "ok") as "ok" | "conflict" | "unknown")
            }
            options={[
              { value: "ok", label: "正常成功" },
              {
                value: "conflict",
                label:
                  mode === "create"
                    ? "重复候选冲突（保留输入）"
                    : "数据已更新（保留输入）",
              },
              { value: "unknown", label: "结果不确定（保留输入）" },
            ]}
            allowClear={false}
            aria-label="演示结果"
            placeholder="请选择演示结果"
          />
        </div>
      </details>

      {result?.outcome === "succeeded" ? (
        <FormalActionResult
          status="succeeded"
          title={mode === "create" ? "客户已创建" : "客户资料已保存"}
          description={
            mode === "create"
              ? `客户号 ${result.customerNo} · 基础资料版本 v${result.revisionNo}`
              : `客户号 ${result.customerNo} · 新版本 v${result.revisionNo} · 历史单据记录不变`
          }
          reference={result.reference}
          facts={
            mode === "create"
              ? [
                  { label: "客户号", value: result.customerNo },
                  { label: "版本", value: `v${result.revisionNo}` },
                  { label: "时间", value: result.occurredAt },
                ]
              : [
                  { label: "客户号", value: result.customerNo },
                  {
                    label: "新版本",
                    value: `v${result.revisionNo} · 数据版本 ${result.lockVersion}`,
                  },
                  { label: "时间", value: result.occurredAt },
                ]
          }
        />
      ) : null}

      {result?.outcome === "unknown" ? (
        <FormalActionResult
          status="unknown"
          title={mode === "create" ? "创建结果不确定" : "保存结果不确定"}
          description={result.message}
          reference={result.idempotencyKey}
          referenceLabel="原任务号"
          actions={
            <Button
              type="button"
              size="sm"
              variant="outline"
              disabled={queryIdempotency.isPending}
              onClick={async () => {
                const final = await queryIdempotency.mutateAsync(
                  result.idempotencyKey
                )
                if (final) setResult(final)
              }}
            >
              查询最终结果
            </Button>
          }
        />
      ) : null}

      <div className="flex flex-col-reverse gap-2 sm:flex-row sm:justify-end">
        <Button
          type="button"
          variant="outline"
          onClick={() => {
            if (result?.outcome === "succeeded") {
              resetSession()
              onCancel()
              return
            }
            if (dirty) {
              setDiscardOpen(true)
              return
            }
            onCancel()
          }}
        >
          {result?.outcome === "succeeded" ? "关闭" : "取消"}
        </Button>
        {result?.outcome !== "succeeded" ? (
          <form.AppForm>
            <form.SubmitButton label={submitLabel} disabled={isPending} />
          </form.AppForm>
        ) : (
          <Button
            type="button"
            onClick={() => {
              resetSession()
              onCancel()
            }}
          >
            完成
          </Button>
        )}
      </div>

      {result?.outcome === "conflict" ? (
        <ConflictResolutionDialog
          open={conflictOpen}
          onOpenChange={setConflictOpen}
          title={mode === "create" ? "存在重复候选（演示）" : undefined}
          description={result.message}
          currentVersion={
            mode === "create"
              ? "既有主体候选"
              : `v${result.serverRevisionNo} · 数据版本 ${result.serverLockVersion}`
          }
          localBaseline={mode === "create" ? "本次输入" : `v${customer!.currentRevision.revisionNo} · 数据版本 ${customer!.lockVersion}`}
          actor={result.actor}
          changedAt={result.changedAt}
          diff={
            mode === "create" ? (
              <p className="text-sm">
                法定名称：{result.serverLegalName || "（候选）"}。系统不会自动合并主体。
              </p>
            ) : (
              <ul className="list-inside list-disc space-y-1 text-sm">
                <li>系统现有法定名称：{result.serverLegalName}</li>
                <li>系统现有简称：{result.serverShortName ?? "—"}</li>
                <li>
                  系统现有信用代码：{result.serverUnifiedCreditCode ?? "—"}
                </li>
                <li>你输入的内容仍保留在表单中，未写入业务记录。</li>
              </ul>
            )
          }
          onReload={() => {
            setConflictOpen(false)
            setIdempotencyKey(
              newIdempotencyKey(mode === "create" ? "create" : "revise")
            )
            setSimulate("ok")
            setResult(null)
          }}
          onSaveCopy={() => setConflictOpen(false)}
          onCompare={() => setConflictOpen(false)}
        />
      ) : null}

      <DiscardConfirmDialog
        open={discardOpen}
        onOpenChange={setDiscardOpen}
        onConfirm={() => {
          setDiscardOpen(false)
          resetSession()
          onCancel()
        }}
      />
    </form>
  )
}
