"use client"

import * as React from "react"
import { SaveIcon } from "lucide-react"

import {
    DetailWorkspaceSaveDock,
    DiscardConfirmDialog,
    detailWorkspaceStyles,
} from "@/components/business"
import { useAppForm, applyApiFormError, FormError } from "@/components/form"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { FieldGroup } from "@/components/ui/field"
import { Label } from "@/components/ui/label"
import { LoadingButton } from "@/components/ui/loading-button"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { BankAccountRowsSection } from "@/features/customers/components/customer-form-bank-section"
import {
    CustomerFormActionBar,
    CustomerFormResultPanel,
} from "@/features/customers/components/customer-form-feedback"
import {
    AddressRowsSection,
    ContactRowsSection,
    FormSection,
} from "@/features/customers/components/customer-form-sections"
import {
    buildDefaults,
    buildFormSubmission,
    countCustomerFormChanges,
    describeCustomerFormChanges,
    newIdempotencyKey,
    type FormValues,
} from "@/features/customers/components/customer-form-values"
import {
    useCreateCustomerMutation,
    useQueryCustomerIdempotencyMutation,
    useSaveCustomerDetailsMutation,
} from "@/features/customers/hooks/queries"
import {
    createSchema,
    editDraftSchema,
    editSchema,
} from "@/features/customers/lib/customer-form-schemas"
import type {
    CreateCustomerInput,
    CustomerCenterView,
    CustomerMutationResult,
    SaveCustomerDetailsInput,
} from "@/features/customers/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import { hasPermission } from "@/lib/permissions"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { useSelector } from "@tanstack/react-form"
import { CustomerConflictDialog } from "./customer-conflict-dialog"
import {
    CustomerSaveReasonDialog,
    QUICK_CHANGE_REASONS,
} from "./customer-save-reason-dialog"

/**
 * 客户资料表单：创建（对话框内）与编辑（详情页原地编辑）共用同一套字段、
 * 校验、敏感值处理、幂等提交与结果状态；外层只决定容器。
 */
export function CustomerForm({
    mode,
    grouped = false,
    customer,
    onCancel,
    onSucceeded,
    onDirtyChange,
    shortcutBlocked = false,
    readonlyContactsFallback,
    readonlyAddressesFallback,
    readonlyBanksFallback,
}: {
    mode: "create" | "edit"
    /** 页面内编辑按详情工作面分区展示；对话框内用紧凑布局。 */
    grouped?: boolean
    /** mode="edit" 必传。 */
    customer?: CustomerCenterView
    onCancel: () => void
    /** 成功回调；revisionNo 供页面展示反馈。 */
    onSucceeded: (customerId: string, revisionNo?: number) => void
    /** 表单是否含未保存输入（详情页用于拦截分区切换与跨页跳转）。 */
    onDirtyChange?: (isDirty: boolean) => void
    shortcutBlocked?: boolean
    readonlyContactsFallback?: React.ReactNode
    readonlyAddressesFallback?: React.ReactNode
    readonlyBanksFallback?: React.ReactNode
}) {
    const formRef = React.useRef<HTMLFormElement>(null)
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
    const [saveReasonOpen, setSaveReasonOpen] = React.useState(false)
    const [reasonDraft, setReasonDraft] = React.useState("")
    const [reasonError, setReasonError] = React.useState<string | null>(null)
    const pendingReasonRef = React.useRef<string | null>(null)

    const [baseline, setBaseline] = React.useState(customer)
    const [conflictDraft, setConflictDraft] = React.useState<FormValues | null>(
        null,
    )
    const defaults = React.useMemo(
        () => buildDefaults(mode, baseline),
        [baseline, mode],
    )

    const validationSchema =
        mode === "create"
            ? createSchema
            : grouped
              ? editDraftSchema
              : editSchema

    const form = useAppForm({
        defaultValues: defaults,
        validators: { onChange: validationSchema },
        onSubmit: async ({ value }) => {
            const effectiveValue: FormValues =
                grouped && mode === "edit" && pendingReasonRef.current != null
                    ? { ...value, changeReason: pendingReasonRef.current }
                    : value
            const input = buildFormSubmission(mode, effectiveValue, baseline, {
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

            pendingReasonRef.current = null
            setSaveReasonOpen(false)
            setResult(response)
            if (response.outcome !== "succeeded" && response.error) {
                applyApiFormError(form, response.error, {
                    mobile: "phone",
                    bank_branch_name: "branchName",
                    default_payment_term_id: "defaultPaymentTerm",
                })
            }
            if (response.outcome === "conflict") {
                setConflictDraft(structuredClone(effectiveValue))
                setConflictOpen(true)
            }
            if (response.outcome === "succeeded") {
                setReasonDraft("")
                setReasonError(null)
                setIdempotencyKey(
                    newIdempotencyKey(mode === "create" ? "create" : "revise"),
                )
                form.reset(
                    mode === "create"
                        ? buildDefaults("create", undefined)
                        : { ...effectiveValue, changeReason: "" },
                )
                onSucceeded(response.customerId, response.revisionNo)
            }
        },
    })

    const [changeCount, changedLabels, isDirty] = useSelector(
        form.store,
        (state) => {
            if (!grouped) return [0, [] as string[], state.isDirty] as const
            return [
                countCustomerFormChanges(defaults, state.values),
                describeCustomerFormChanges(defaults, state.values),
                state.isDirty,
            ] as const
        },
    )
    const dirty = grouped ? changeCount > 0 : isDirty
    const changeDescription =
        changedLabels.length > 0
            ? changedLabels.slice(0, 3).join("、") +
              (changedLabels.length > 3 ? " 等" : "")
            : undefined
    const [discardOpen, setDiscardOpen] = React.useState(false)

    // 详情页最新资料刷新且当前无未保存修改时，自动同步最新基线，避免冻结旧快照。
    const customerVersionKey = customer
        ? `${customer.customerId}:${customer.lockVersion}:${customer.partyLockVersion}:${customer.currentRevision.revisionId}`
        : ""
    const baselineVersionKey = baseline
        ? `${baseline.customerId}:${baseline.lockVersion}:${baseline.partyLockVersion}:${baseline.currentRevision.revisionId}`
        : ""
    React.useEffect(() => {
        if (
            mode !== "edit" ||
            !customer ||
            dirty ||
            customerVersionKey === baselineVersionKey
        ) {
            return
        }
        setBaseline(customer)
        form.reset(buildDefaults("edit", customer))
        setResult(null)
    }, [baselineVersionKey, customer, customerVersionKey, dirty, form, mode])

    React.useEffect(() => {
        onDirtyChange?.(dirty)
    }, [dirty, onDirtyChange])

    const isSubmitting =
        mode === "create" ? createMutation.isPending : saveMutation.isPending
    const isPending = isSubmitting || queryIdempotency.isPending

    useWorkspaceDirtyState({ dirty, busy: isPending })

    const resetSession = React.useCallback(() => {
        setResult(null)
        setConflictOpen(false)
        setSaveReasonOpen(false)
        setReasonDraft("")
        setReasonError(null)
        pendingReasonRef.current = null
        setIdempotencyKey(
            newIdempotencyKey(mode === "create" ? "create" : "revise"),
        )
        const latestBaseline =
            mode === "edit" ? (customer ?? baseline) : undefined
        if (mode === "edit" && latestBaseline) {
            setBaseline(latestBaseline)
        }
        form.reset(buildDefaults(mode, latestBaseline))
    }, [baseline, customer, form, mode])

    const focusFirstInvalidField = React.useCallback(() => {
        requestAnimationFrame(() => {
            const container = formRef.current
            if (!container) return
            const invalidTarget =
                container.querySelector<HTMLElement>('[aria-invalid="true"]') ??
                container.querySelector<HTMLElement>(
                    '[data-invalid="true"] input, [data-invalid="true"] textarea, [data-invalid="true"] button',
                )
            if (invalidTarget) {
                invalidTarget.scrollIntoView({
                    behavior: "smooth",
                    block: "center",
                })
                invalidTarget.focus({ preventScroll: true })
            }
        })
    }, [])

    const handleRequestGroupedSave = React.useCallback(async () => {
        if (
            !dirty ||
            isPending ||
            form.state.isSubmitting ||
            shortcutBlocked ||
            conflictOpen ||
            discardOpen ||
            saveReasonOpen
        ) {
            return
        }
        await form.validateAllFields("submit")
        const parsed = editDraftSchema.safeParse(form.state.values)
        if (!parsed.success) {
            focusFirstInvalidField()
            return
        }
        const inlineReason = form.state.values.changeReason.trim()
        if (inlineReason.length >= 2) {
            pendingReasonRef.current = inlineReason
            await form.handleSubmit()
            return
        }
        setReasonDraft(form.state.values.changeReason)
        setReasonError(null)
        setSaveReasonOpen(true)
    }, [
        conflictOpen,
        dirty,
        discardOpen,
        focusFirstInvalidField,
        form,
        isPending,
        saveReasonOpen,
        shortcutBlocked,
    ])

    const handleConfirmSaveWithReason = React.useCallback(() => {
        const trimmed = reasonDraft.trim()
        if (trimmed.length < 2) {
            setReasonError("请填写修改原因（至少 2 个字）")
            return
        }
        setReasonError(null)
        pendingReasonRef.current = trimmed
        form.setFieldValue("changeReason", trimmed)
        void form.handleSubmit()
    }, [form, reasonDraft])

    const submitLabel =
        mode === "create"
            ? createMutation.isPending
                ? "提交中…"
                : "创建客户"
            : saveMutation.isPending
              ? "保存中…"
              : "保存更新"

    return (
        <form
            ref={formRef}
            noValidate
            className={grouped ? "min-w-0" : "flex flex-col gap-4"}
            onSubmit={(e) => {
                e.preventDefault()
                e.stopPropagation()
                if (grouped && mode === "edit") {
                    void handleRequestGroupedSave()
                    return
                }
                void form.handleSubmit().then(() => {
                    if (!form.state.isValid) {
                        focusFirstInvalidField()
                    }
                })
            }}
        >
            {grouped ? (
                <FormSection
                    grouped={grouped}
                    title="客户基本资料"
                    description="修改仅对后续新业务生效，不影响已创建的合同和销售单"
                >
                    <FieldGroup
                        data-slot="detail-form-grid"
                        className="grid gap-x-5 gap-y-4 sm:grid-cols-12"
                    >
                        <form.AppField
                            name="legalName"
                            children={(field) => (
                                <field.TextField
                                    id="customers-form-legal-name"
                                    label="法定名称"
                                    required
                                    className="sm:col-span-8"
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
                                    className="sm:col-span-4"
                                />
                            )}
                        />
                        <form.AppField
                            name="unifiedCreditCode"
                            children={(field) => (
                                <field.TextField
                                    id="customers-form-credit-code"
                                    label="统一社会信用代码"
                                    inputClassName="num"
                                    placeholder="可选，18 位字母或数字"
                                    className="sm:col-span-5"
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
                                    className="sm:col-span-4"
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
                                    allowClear={false}
                                    options={[
                                        { value: "active", label: "启用" },
                                        { value: "disabled", label: "停用" },
                                    ]}
                                    className="sm:col-span-3"
                                />
                            )}
                        />
                    </FieldGroup>
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
                                inputClassName="num"
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
                                placeholder="开单时按需选择（选填）"
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
                                className="text-xs text-muted-foreground"
                            >
                                同一企业已登记为供应商等主体时，沿用已有名称、简称、信用代码、联系人、地址和银行账户，只新增客户档案及付款条件；尚未建档时按本次填写内容创建。
                            </p>
                        </div>
                    )}
                />
            ) : null}

            {canWriteContacts ? (
                <ContactRowsSection
                    form={form}
                    mode={mode}
                    grouped={grouped}
                    existingContacts={baseline?.contacts}
                />
            ) : (
                (readonlyContactsFallback ?? null)
            )}

            {canWriteAddresses ? (
                <AddressRowsSection
                    form={form}
                    mode={mode}
                    grouped={grouped}
                    existingAddresses={baseline?.addresses}
                />
            ) : (
                (readonlyAddressesFallback ?? null)
            )}

            {canWriteBanks ? (
                <BankAccountRowsSection
                    form={form}
                    mode={mode}
                    grouped={grouped}
                    existingBanks={baseline?.bankAccounts}
                />
            ) : (
                (readonlyBanksFallback ?? null)
            )}

            {grouped && mode === "edit" ? (
                <FormSection grouped={grouped} title="变更说明">
                    <FieldGroup
                        data-slot="detail-form-grid"
                        className="space-y-2.5"
                    >
                        <form.AppField
                            name="changeReason"
                            children={(field) => (
                                <field.TextareaField
                                    id="customers-form-change-reason"
                                    label="修改原因"
                                    required
                                    rows={2}
                                    placeholder="例如：补录统一社会信用代码、更新默认联系人或付款条件"
                                />
                            )}
                        />
                        <div className="flex flex-wrap items-center gap-1.5">
                            <span className="text-xs text-muted-foreground">
                                快捷填入：
                            </span>
                            {QUICK_CHANGE_REASONS.map((preset) => (
                                <Button
                                    key={preset}
                                    id={`customers-form-reason-preset-${toAutomationIdSegment(preset)}`}
                                    type="button"
                                    size="xs"
                                    variant="ghost"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                    disabled={isPending}
                                    onClick={() =>
                                        form.setFieldValue(
                                            "changeReason",
                                            preset,
                                        )
                                    }
                                >
                                    {preset}
                                </Button>
                            ))}
                        </div>
                    </FieldGroup>
                </FormSection>
            ) : null}

            <div className={grouped ? "max-w-4xl" : undefined}>
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
            </div>

            {grouped && mode === "edit" ? (
                <form.AppForm>
                    <FormError className="max-w-4xl pb-3" />
                    <DetailWorkspaceSaveDock
                        open={dirty}
                        changeCount={changeCount}
                        summary={changedLabels.join("；")}
                        description={changeDescription}
                        saving={isPending}
                        enableSaveShortcut={
                            !shortcutBlocked &&
                            !conflictOpen &&
                            !discardOpen &&
                            !saveReasonOpen
                        }
                        onSave={() => void handleRequestGroupedSave()}
                        actions={
                            <>
                                {dirty ? (
                                    <Button
                                        id="customers-form-cancel"
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        className={
                                            detailWorkspaceStyles.quietButton
                                        }
                                        disabled={isPending}
                                        onClick={() => setDiscardOpen(true)}
                                    >
                                        取消修改
                                    </Button>
                                ) : null}
                                <LoadingButton
                                    id="customers-form-submit"
                                    type="button"
                                    size="sm"
                                    className="px-3.5"
                                    aria-keyshortcuts="Meta+S Control+S"
                                    disabled={!dirty || isPending}
                                    loading={isSubmitting}
                                    onClick={() =>
                                        void handleRequestGroupedSave()
                                    }
                                >
                                    <SaveIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    {submitLabel}
                                </LoadingButton>
                            </>
                        }
                    />
                </form.AppForm>
            ) : (
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
            )}

            {mode === "edit" && result?.outcome === "conflict" ? (
                <div className={grouped ? "max-w-4xl pt-2" : undefined}>
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
                </div>
            ) : null}

            {grouped && mode === "edit" ? (
                <CustomerSaveReasonDialog
                    open={saveReasonOpen}
                    onOpenChange={setSaveReasonOpen}
                    changeCount={changeCount}
                    changedLabels={changedLabels}
                    reason={reasonDraft}
                    onReasonChange={(next) => {
                        setReasonDraft(next)
                        if (reasonError) setReasonError(null)
                    }}
                    reasonError={reasonError}
                    pending={isPending}
                    onConfirm={handleConfirmSaveWithReason}
                />
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
                title="放弃未保存的修改？"
                description="将还原为最近一次保存的客户资料并退出编辑。"
                confirmLabel="放弃并还原"
                onConfirm={() => {
                    setDiscardOpen(false)
                    resetSession()
                    onCancel()
                }}
            />
        </form>
    )
}
