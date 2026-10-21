"use client"

import * as React from "react"

import { DiscardConfirmDialog } from "@/components/business"
import { useAppForm, applyApiFormError } from "@/components/form"
import { useSelector } from "@tanstack/react-form"
import { PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import {
    useCreateCustomerMutation,
    useQueryCustomerIdempotencyMutation,
    useSaveCustomerDetailsMutation,
} from "@/features/customers/hooks/queries"
import { useAccountProfileQuery } from "@/features/auth/queries"
import type {
    CreateCustomerInput,
    CustomerCenterView,
    CustomerMutationResult,
    SaveCustomerDetailsInput,
} from "@/features/customers/types"
import { hasPermission } from "@/lib/permissions"
import {
    createSchema,
    editSchema,
} from "@/features/customers/lib/customer-form-schemas"
import {
    buildDefaults,
    buildFormSubmission,
    newIdempotencyKey,
    type FormValues,
} from "@/features/customers/components/customer-form-values"
import {
    AddressRowsSection,
    ContactRowsSection,
    FormSection,
} from "@/features/customers/components/customer-form-sections"
import { BankAccountRowsSection } from "@/features/customers/components/customer-form-bank-section"
import {
    CustomerFormActionBar,
    CustomerFormResultPanel,
} from "@/features/customers/components/customer-form-feedback"
import { CustomerConflictDialog } from "./customer-conflict-dialog"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { Label } from "@/components/ui/label"
import { useCustomerFormDirtyGuard } from "@/features/customers/hooks/use-customer-form-dirty-guard"

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
    /** 表单是否含未保存输入（详情页用于拦截分区切换）。 */
    onDirtyChange?: (isDirty: boolean) => void
}) {
    const createMutation = useCreateCustomerMutation()
    const saveMutation = useSaveCustomerDetailsMutation()
    const queryIdempotency = useQueryCustomerIdempotencyMutation()
    const accountProfile = useAccountProfileQuery()
    const canWriteContacts =
        hasPermission(
            accountProfile.data?.permissions,
            mode === "create" ? "party_contact:create" : "party_contact:update",
        ) &&
        (mode === "create" ||
            hasPermission(
                accountProfile.data?.permissions,
                "party_contact:detail",
            ))
    const canWriteAddresses =
        hasPermission(
            accountProfile.data?.permissions,
            mode === "create" ? "party_address:create" : "party_address:update",
        ) &&
        (mode === "create" ||
            hasPermission(
                accountProfile.data?.permissions,
                "party_address:detail",
            ))
    const bankWritePermission =
        mode === "create"
            ? "party_bank_account:create"
            : "party_bank_account:update"
    const canWriteBanks =
        hasPermission(accountProfile.data?.permissions, bankWritePermission) &&
        (mode === "create" ||
            hasPermission(
                accountProfile.data?.permissions,
                "party_bank_account:detail",
            ))
    const [idempotencyKey, setIdempotencyKey] = React.useState(() =>
        newIdempotencyKey(mode === "create" ? "create" : "revise"),
    )
    const [result, setResult] = React.useState<CustomerMutationResult | null>(
        null,
    )
    const [conflictOpen, setConflictOpen] = React.useState(false)

    const [baseline, setBaseline] = React.useState(customer)
    const [conflictDraft, setConflictDraft] = React.useState<FormValues | null>(
        null,
    )
    const defaults = React.useMemo(
        () => buildDefaults(mode, baseline),
        [baseline, mode],
    )

    const form = useAppForm({
        defaultValues: defaults,
        validators: { onChange: mode === "create" ? createSchema : editSchema },
        onSubmit: async ({ value }) => {
            const input = buildFormSubmission(mode, value, baseline, {
                canWriteContacts,
                canWriteAddresses,
                canWriteBanks,
                idempotencyKey,
            })

            const response =
                mode === "create"
                    ? await createMutation.mutateAsync(
                          input as CreateCustomerInput,
                      )
                    : await saveMutation.mutateAsync(
                          input as SaveCustomerDetailsInput,
                      )

            setResult(response)
            if (response.outcome !== "succeeded" && response.error) {
                applyApiFormError(form, response.error, {
                    mobile: "phone",
                    bank_branch_name: "branchName",
                    default_payment_term_id: "defaultPaymentTerm",
                })
            }
            if (response.outcome === "conflict") {
                setConflictDraft(structuredClone(value))
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

    useCustomerFormDirtyGuard(dirty)

    const resetSession = () => {
        setResult(null)
        setConflictOpen(false)
        setIdempotencyKey(
            newIdempotencyKey(mode === "create" ? "create" : "revise"),
        )
        form.reset()
    }

    const isSubmitting =
        mode === "create" ? createMutation.isPending : saveMutation.isPending
    const isPending = isSubmitting || queryIdempotency.isPending
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
            noValidate
            className={grouped ? "space-y-4" : "flex flex-col gap-4"}
            onSubmit={(e) => {
                e.preventDefault()
                e.stopPropagation()
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
                            children={(field) => (
                                <field.TextField
                                    id="customers-form-legal-name"
                                    label="法定名称"
                                    required
                                />
                            )}
                        />
                        <form.AppField
                            name="shortName"
                            children={(field) => (
                                <field.TextField
                                    id="customers-form-short-name"
                                    label="客户简称"
                                />
                            )}
                        />
                        <form.AppField
                            name="unifiedCreditCode"
                            children={(field) => (
                                <field.TextField
                                    id="customers-form-credit-code"
                                    label="统一社会信用代码"
                                    placeholder="可选，18 位字母或数字"
                                />
                            )}
                        />
                        <form.AppField
                            name="defaultPaymentTerm"
                            children={(field) => (
                                <field.SelectField
                                    id="customers-form-payment-term"
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
                                    id="customers-form-status"
                                    label="客户状态"
                                    required
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
                                        id="customers-form-change-reason"
                                        label="修订原因"
                                        required
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
                            <field.TextField
                                id="customers-form-legal-name"
                                label="法定名称"
                                required
                                placeholder="企业全称"
                            />
                        )}
                    />
                    <form.AppField
                        name="shortName"
                        children={(field) => (
                            <field.TextField
                                id="customers-form-short-name"
                                label="客户简称"
                                placeholder="可选"
                            />
                        )}
                    />
                    <form.AppField
                        name="unifiedCreditCode"
                        children={(field) => (
                            <field.TextField
                                id="customers-form-credit-code"
                                label="统一社会信用代码"
                                placeholder="可选，18 位字母或数字"
                            />
                        )}
                    />
                    <form.AppField
                        name="defaultPaymentTerm"
                        children={(field) => (
                            <field.SelectField
                                id="customers-form-payment-term"
                                label="默认付款条件"
                                options={PAYMENT_TERM_OPTIONS}
                                placeholder="录单提示"
                                allowClear={false}
                            />
                        )}
                    />
                </div>
            )}

            {mode === "create" ? (
                <form.AppField
                    name="reuseExistingParty"
                    children={(field) => (
                        <div className="space-y-2 rounded-lg border p-3">
                            <div className="flex items-center gap-2">
                                <NativeCheckbox
                                    id="customers-form-reuse-party"
                                    checked={field.state.value}
                                    onCheckedChange={field.handleChange}
                                    onBlur={field.handleBlur}
                                    disabled={isPending}
                                    aria-describedby="customers-form-reuse-party-help"
                                />
                                <Label htmlFor="customers-form-reuse-party">
                                    复用已有企业资料
                                </Label>
                            </div>
                            <p
                                id="customers-form-reuse-party-help"
                                className="text-sm text-muted-foreground"
                            >
                                同一企业已登记为供应商等主体时，沿用已有名称、简称、信用代码、联系人、地址和银行账户，只新增客户档案及付款条件；尚未建档时按本次填写内容创建。
                            </p>
                        </div>
                    )}
                />
            ) : null}

            {canWriteContacts ? (
                <ContactRowsSection form={form} mode={mode} grouped={grouped} />
            ) : null}

            {canWriteAddresses ? (
                <AddressRowsSection form={form} mode={mode} grouped={grouped} />
            ) : null}

            {canWriteBanks ? (
                <BankAccountRowsSection
                    form={form}
                    mode={mode}
                    grouped={grouped}
                />
            ) : null}

            <CustomerFormResultPanel
                result={result}
                mode={mode}
                isQueryingIdempotency={queryIdempotency.isPending}
                onQueryFinalResult={(key) => {
                    void queryIdempotency.mutateAsync(key).then((final) => {
                        if (final) setResult(final)
                    })
                }}
            />

            <CustomerFormActionBar
                form={form}
                result={result}
                isPending={isPending}
                isSubmitting={isSubmitting}
                submitLabel={submitLabel}
                dirty={dirty}
                onCancel={onCancel}
                onDiscardRequest={
                    mode === "edit" ? () => setDiscardOpen(true) : undefined
                }
                onResetSession={resetSession}
            />

            {mode === "edit" && result?.outcome === "conflict" ? (
                <Button
                    id="customers-form-review-conflict"
                    type="button"
                    variant="outline"
                    onClick={() => {
                        setConflictDraft(structuredClone(form.state.values))
                        setConflictOpen(true)
                    }}
                >
                    核对最新资料
                </Button>
            ) : null}
            {mode === "edit" && baseline && conflictDraft ? (
                <CustomerConflictDialog
                    open={conflictOpen}
                    onOpenChange={setConflictOpen}
                    baseline={baseline}
                    draft={conflictDraft}
                    onResolved={(latest, values) => {
                        setBaseline(latest)
                        setIdempotencyKey(newIdempotencyKey("revise"))
                        form.reset(buildDefaults("edit", latest))
                        for (const key of Object.keys(
                            values,
                        ) as (keyof FormValues)[]) {
                            form.setFieldValue(key, values[key])
                        }
                        setResult(null)
                        setConflictOpen(false)
                    }}
                />
            ) : null}

            <DiscardConfirmDialog
                id="customers-form-discard"
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
