"use client"

import * as React from "react"

import {
    ConflictResolutionDialog,
    DiscardConfirmDialog,
} from "@/components/business"
import { useAppForm } from "@/components/form"
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
    /** 表单是否含未保存输入（对话框容器用于拦截 X / Esc / 遮罩关闭）。 */
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

    const defaults = React.useMemo(
        () => buildDefaults(mode, customer),
        [customer, mode],
    )

    const form = useAppForm({
        defaultValues: defaults,
        validators: { onChange: mode === "create" ? createSchema : editSchema },
        onSubmit: async ({ value }) => {
            const input = buildFormSubmission(mode, value, customer, {
                canWriteContacts,
                canWriteAddresses,
                canWriteBanks,
                idempotencyKey,
            })

            const response =
                mode === "create"
                    ? await createMutation.mutateAsync(input as CreateCustomerInput)
                    : await saveMutation.mutateAsync(input as SaveCustomerDetailsInput)

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

    useCustomerFormDirtyGuard(dirty)

    const resetSession = () => {
        setResult(null)
        setConflictOpen(false)
        setIdempotencyKey(
            newIdempotencyKey(mode === "create" ? "create" : "revise"),
        )
        form.reset()
    }

    const isPending =
        (mode === "create"
            ? createMutation.isPending
            : saveMutation.isPending) || queryIdempotency.isPending
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
                            children={(field) => (
                                <field.TextField label="法定名称" required />
                            )}
                        />
                        <form.AppField
                            name="shortName"
                            children={(field) => (
                                <field.TextField label="客户简称" />
                            )}
                        />
                        <form.AppField
                            name="unifiedCreditCode"
                            children={(field) => (
                                <field.TextField
                                    label="统一社会信用代码"
                                    required
                                    placeholder="18 位字母或数字"
                                />
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
                                label="客户简称"
                                placeholder="可选"
                            />
                        )}
                    />
                    <form.AppField
                        name="unifiedCreditCode"
                        children={(field) => (
                            <field.TextField
                                label="统一社会信用代码"
                                required
                                placeholder="18 位字母或数字"
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
                                allowClear={false}
                            />
                        )}
                    />
                </div>
            )}

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
                submitLabel={submitLabel}
                dirty={dirty}
                onCancel={onCancel}
                onDiscardRequest={() => setDiscardOpen(true)}
                onResetSession={resetSession}
            />

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
                    localBaseline={
                        mode === "create"
                            ? "本次输入"
                            : `v${customer!.currentRevision.revisionNo} · 数据版本 ${customer!.lockVersion}`
                    }
                    actor={result.actor}
                    changedAt={result.changedAt}
                    diff={
                        mode === "create" ? (
                            <p className="text-sm">
                                法定名称：{result.serverLegalName || "（候选）"}
                                。系统不会自动合并主体。
                            </p>
                        ) : (
                            <ul className="list-inside list-disc space-y-1 text-sm">
                                <li>
                                    系统现有法定名称：{result.serverLegalName}
                                </li>
                                <li>
                                    系统现有简称：
                                    {result.serverShortName ?? "—"}
                                </li>
                                <li>
                                    系统现有信用代码：
                                    {result.serverUnifiedCreditCode ?? "—"}
                                </li>
                                <li>
                                    你输入的内容仍保留在表单中，未写入业务记录。
                                </li>
                            </ul>
                        )
                    }
                    onReload={() => {
                        setConflictOpen(false)
                        setIdempotencyKey(
                            newIdempotencyKey(
                                mode === "create" ? "create" : "revise",
                            ),
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
