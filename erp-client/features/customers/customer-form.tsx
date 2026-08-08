"use client"

import * as React from "react"
import { PlusIcon } from "lucide-react"
import { z } from "zod"

import {
  ConflictResolutionDialog,
  DiscardConfirmDialog,
  DocumentSection,
  FormalActionResult,
  OwnerCombobox,
} from "@/components/business"
import { toFieldErrors, useAppForm } from "@/components/form"
import { useSelector } from "@tanstack/react-form"
import { PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import {
  useCreateCustomerMutation,
  useQueryCustomerIdempotencyMutation,
  useSaveCustomerDetailsMutation,
} from "@/features/customers/queries"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type {
  CustomerCenterView,
  CustomerMutationResult,
} from "@/features/customers/types"
import { useOwnerOptionsQuery } from "@/hooks/use-options"
import { hasPermission } from "@/lib/permissions"

const contactRowSchema = z
  .object({
    existingId: z.string().optional(),
    name: z.string().trim().min(1, "请填写联系人姓名"),
    title: z.string(),
    phone: z.string(),
    telephone: z.string(),
    email: z.string(),
    isDefault: z.boolean(),
  })
  .superRefine((value, context) => {
    if (!value.existingId && !value.phone.trim()) {
      context.addIssue({
        code: "custom",
        path: ["phone"],
        message: "请填写手机号",
      })
    }
  })

const addressRowSchema = z
  .object({
    existingId: z.string().optional(),
    addressType: z.string().trim().min(1, "请选择地址类型"),
    contactName: z.string(),
    address: z.string(),
    isDefault: z.boolean(),
  })
  .superRefine((value, context) => {
    if (!value.existingId && !value.address.trim()) {
      context.addIssue({
        code: "custom",
        path: ["address"],
        message: "请填写地址",
      })
    }
  })

const bankAccountRowSchema = z
  .object({
    existingId: z.string().optional(),
    accountName: z.string().trim().min(1, "请填写户名"),
    bankName: z.string().trim().min(1, "请填写银行名称"),
    branchName: z.string(),
    accountNumber: z.string(),
    isDefault: z.boolean(),
  })
  .superRefine((value, context) => {
    if (!value.existingId && !value.accountNumber.trim()) {
      context.addIssue({
        code: "custom",
        path: ["accountNumber"],
        message: "请填写银行账号",
      })
    }
  })

const createSchema = z.object({
  legalName: z.string().trim().min(2, "请填写法定名称"),
  shortName: z.string(),
  unifiedCreditCode: z.string(),
  ownerUserId: z.string().min(1, "请选择负责销售"),
  defaultPaymentTerm: z.string(),
  status: z.enum(["active", "disabled"]),
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
  status: z.enum(["active", "disabled"]),
  changeReason: z.string().trim().min(2, "请填写修订原因"),
  contacts: z.array(contactRowSchema),
  addresses: z.array(addressRowSchema),
  bankAccounts: z.array(bankAccountRowSchema),
})

type ContactRow = {
  existingId?: string
  name: string
  title: string
  phone: string
  telephone: string
  email: string
  isDefault: boolean
}

type AddressRow = {
  existingId?: string
  addressType: string
  contactName: string
  address: string
  isDefault: boolean
}

type BankAccountRow = {
  existingId?: string
  accountName: string
  bankName: string
  branchName: string
  accountNumber: string
  isDefault: boolean
}

type FormValues = {
  legalName: string
  shortName: string
  unifiedCreditCode: string
  ownerUserId: string
  defaultPaymentTerm: string
  status: "active" | "disabled"
  changeReason: string
  contacts: ContactRow[]
  addresses: AddressRow[]
  bankAccounts: BankAccountRow[]
}

const ADDRESS_TYPE_OPTIONS = [
  { value: "履约地址", label: "履约地址" },
  { value: "注册地址", label: "注册地址" },
  { value: "经营地址", label: "经营地址" },
] as const

function newIdempotencyKey(prefix: string): string {
  return `${prefix}-${Date.now().toString(36)}-${Math.random().toString(36).slice(2, 8)}`
}

/**
 * 编辑态敏感字段：后端不回传明文/reveal token，预填留空，避免把掩码写回。
 * 有 token 时也仅作占位（reveal 接口未落地）。
 */
function editableValue(token: string | undefined, masked: string): string {
  if (token) return ""
  if (!masked || masked === "—" || masked.includes("*")) return ""
  return masked
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
      status: "active",
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
    status: customer!.status,
    changeReason: "",
    contacts: customer!.contacts.map((c) => ({
      existingId: c.id,
      name: c.name,
      title: c.title ?? "",
      phone: editableValue(c.phoneRevealToken, c.phoneMasked),
      telephone: c.telephone ?? "",
      email: c.email ?? "",
      isDefault: c.isDefault,
    })),
    addresses: customer!.addresses.map((a) => ({
      existingId: a.id,
      addressType: a.addressType,
      contactName: a.contactName ?? "",
      address: editableValue(a.addressRevealToken, a.addressMasked),
      isDefault: a.isDefault,
    })),
    bankAccounts: customer!.bankAccounts.map((b) => ({
      existingId: b.id,
      accountName: b.accountName,
      bankName: b.bankName,
      branchName: b.branchName ?? "",
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
  const { data: ownerOptions } = useOwnerOptionsQuery()
  const accountProfile = useAccountProfileQuery()
  const canWriteContacts =
    hasPermission(
      accountProfile.data?.permissions,
      mode === "create" ? "party_contact:create" : "party_contact:update"
    ) &&
    (mode === "create" ||
      hasPermission(accountProfile.data?.permissions, "party_contact:detail"))
  const canWriteAddresses =
    hasPermission(
      accountProfile.data?.permissions,
      mode === "create" ? "party_address:create" : "party_address:update"
    ) &&
    (mode === "create" ||
      hasPermission(accountProfile.data?.permissions, "party_address:detail"))
  const bankWritePermission =
    mode === "create" ? "party_bank_account:create" : "party_bank_account:update"
  const canWriteBanks =
    hasPermission(accountProfile.data?.permissions, bankWritePermission) &&
    (mode === "create" ||
      hasPermission(
        accountProfile.data?.permissions,
        "party_bank_account:detail"
      ))
  const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
    newIdempotencyKey(mode === "create" ? "create" : "revise")
  )
  const [result, setResult] = React.useState<CustomerMutationResult | null>(
    null
  )
  const [conflictOpen, setConflictOpen] = React.useState(false)

  const defaults = React.useMemo<FormValues>(
    () => buildDefaults(mode, customer),
    [customer, mode]
  )

  const form = useAppForm({
    defaultValues: defaults,
    validators: { onChange: mode === "create" ? createSchema : editSchema },
    onSubmit: async ({ value }) => {
      const contacts = value.contacts.map((row) => ({
        existingId: row.existingId,
        name: row.name.trim(),
        title: row.title.trim() || undefined,
        phone:
          row.existingId && row.phone.includes("*")
            ? undefined
            : row.phone.trim() || undefined,
        telephone: row.telephone.trim() || undefined,
        email: row.email.trim() || undefined,
        isDefault: row.isDefault,
      }))
      const addresses = value.addresses.map((row) => ({
        existingId: row.existingId,
        addressType: row.addressType.trim(),
        contactName: row.contactName.trim() || undefined,
        address:
          row.existingId && row.address.includes("*")
            ? undefined
            : row.address.trim() || undefined,
        isDefault: row.isDefault,
      }))
      const bankAccounts = value.bankAccounts.map((row) => ({
        existingId: row.existingId,
        accountName: row.accountName.trim(),
        bankName: row.bankName.trim(),
        branchName: row.branchName.trim() || undefined,
        accountNumber:
          row.existingId && row.accountNumber.includes("*")
            ? undefined
            : row.accountNumber.trim() || undefined,
        isDefault: row.isDefault,
      }))

      const response =
        mode === "create"
          ? await createMutation.mutateAsync({
              legalName: value.legalName.trim(),
              shortName: value.shortName.trim() || undefined,
              unifiedCreditCode: value.unifiedCreditCode.trim() || undefined,
              defaultPaymentTerm: value.defaultPaymentTerm.trim() || undefined,
              status: value.status,
              ownerUserId: value.ownerUserId,
              ownerName:
                ownerOptions?.find((o) => o.userId === value.ownerUserId)
                  ?.displayName ?? value.ownerUserId,
              contacts: canWriteContacts ? contacts : undefined,
              addresses: canWriteAddresses ? addresses : undefined,
              bankAccounts: canWriteBanks ? bankAccounts : undefined,
              idempotencyKey,
            })
          : await saveMutation.mutateAsync({
              customerId: customer!.customerId,
              expectedLockVersion: customer!.lockVersion,
              expectedPartyVersion: customer!.partyLockVersion,
              baseRevisionId: customer!.currentRevision.revisionId,
              legalName: value.legalName.trim(),
              shortName: value.shortName.trim(),
              unifiedCreditCode: value.unifiedCreditCode.trim(),
              defaultPaymentTerm: value.defaultPaymentTerm.trim(),
              status: value.status,
              changeReason: value.changeReason.trim(),
              contacts: canWriteContacts ? contacts : undefined,
              addresses: canWriteAddresses ? addresses : undefined,
              bankAccounts: canWriteBanks ? bankAccounts : undefined,
              idempotencyKey,
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
            <form.AppField
              name="defaultPaymentTerm"
              children={(field) => (
                <field.SelectField
                  label="默认付款条件"
                  options={PAYMENT_TERM_OPTIONS}
                  placeholder="请选择付款条件"
                />
              )}
            />
            <form.AppField
              name="status"
              children={(field) => (
                <field.SelectField
                  label="客户状态"
                  options={[
                    { value: "active", label: "启用" },
                    { value: "disabled", label: "停用" },
                  ]}
                />
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
                    owners={ownerOptions ?? []}
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

      {canWriteContacts ? (
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
                    telephone: "",
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
                    <form.AppField name={`contacts[${index}].telephone`}>
                      {(nested) => (
                        <nested.TextField label="固定电话" placeholder="可选" />
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
      ) : null}

      {canWriteAddresses ? (
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
                    contactName: "",
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
                    <form.AppField name={`addresses[${index}].contactName`}>
                      {(nested) => (
                        <nested.TextField label="地址联系人" placeholder="可选" />
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
      ) : null}

      {canWriteBanks ? (
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
                    branchName: "",
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
                      {(nested) => (
                        <nested.TextField
                          label="户名"
                          disabled={Boolean(_row.existingId)}
                        />
                      )}
                    </form.AppField>
                    <form.AppField name={`bankAccounts[${index}].bankName`}>
                      {(nested) => (
                        <nested.TextField
                          label="银行名称"
                          disabled={Boolean(_row.existingId)}
                        />
                      )}
                    </form.AppField>
                    <form.AppField name={`bankAccounts[${index}].branchName`}>
                      {(nested) => (
                        <nested.TextField
                          label="支行名称"
                          disabled={Boolean(_row.existingId)}
                        />
                      )}
                    </form.AppField>
                    <form.AppField name={`bankAccounts[${index}].accountNumber`}>
                      {(nested) => (
                        <nested.TextField
                          label="账号"
                          disabled={Boolean(_row.existingId)}
                        />
                      )}
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
                      {_row.existingId ? "结束账户" : "移除"}
                    </Button>
                  </div>
                </div>
              ))
            )
          }
        </form.AppField>
        </FormSection>
      ) : null}

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
          title={mode === "create" ? "存在重复候选" : undefined}
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
