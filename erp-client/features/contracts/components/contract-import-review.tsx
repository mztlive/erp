"use client"

import { useEffect, useRef, useState } from "react"
import { useStore } from "@tanstack/react-form"
import { useAppForm, toFieldErrors } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { Spinner } from "@/components/ui/spinner"
import { CustomerSearchCombobox } from "@/features/entity-selectors"
import { CompanySearchCombobox } from "@/features/companies/company-search-combobox"
import { useCompanyQuery } from "@/features/companies/queries"
import { useAccountProfileQuery } from "@/features/auth/queries"
import {
    importNewCustomerCreditCodeSchema,
    importReviewSchema,
    importReviewFieldStep,
    type ImportReviewStep,
} from "../lib/import-review"
import { ContractImportReviewWarnings } from "./contract-import-review-warnings"
import { ContractImportCustomerChoice } from "./contract-import-customer-choice"
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import type { ContractImportTask, ConfirmContractImport } from "../api/upload"
import type { ImportIdentityMatches } from "../api/import-identities"
import {
    useImportIdentityMatches,
    useImportCustomer,
} from "../hooks/import-identity-queries"

const PAYMENT = [
    "先款 100%",
    "先款 50%",
    "先款 30%",
    "货到 15 天",
    "货到 30 天",
]
const INVOICE = ["增值税专用发票", "增值税普通发票", "不开发票"]
const options = (values: string[]) =>
    values.map((value) => ({ value, label: value }))
type Props = {
    task: ContractImportTask
    step: ImportReviewStep
    onStepChange: (step: ImportReviewStep) => void
    busy: boolean
    disabled: boolean
    expectedCustomerId?: string
    onConfirm: (command: ConfirmContractImport) => Promise<unknown>
    onSubmitStateChange: (state: ContractImportReviewSubmitState) => void
}

export type ContractImportReviewSubmitState = {
    taskId: string
    canSubmit: boolean
    validSteps: boolean[]
    isSubmitting: boolean
    stepHints?: string[]
}

export function ContractImportReview(props: Props) {
    const matches = useImportIdentityMatches(
        props.task,
        props.expectedCustomerId,
    )
    if (matches.isPending)
        return (
            <p
                className="flex items-center gap-2 py-4 text-sm text-muted-foreground"
                role="status"
            >
                <Spinner />
                正在匹配客户与我方签约主体…
            </p>
        )
    if (matches.isError && !matches.data)
        return (
            <Alert variant="destructive">
                <AlertTitle>主体匹配未完成</AlertTitle>
                <AlertDescription>
                    {getErrorMessage(matches.error, "请重新匹配后核对合同")}
                </AlertDescription>
                <Button
                    id="contract-import-identity-retry"
                    type="button"
                    variant="outline"
                    size="sm"
                    onClick={() => void matches.refetch()}
                    disabled={matches.isFetching}
                >
                    重新匹配
                </Button>
            </Alert>
        )
    return (
        <div className="space-y-4">
            {matches.isError ? (
                <Alert variant="destructive">
                    <AlertTitle>主体匹配未完成</AlertTitle>
                    <AlertDescription>
                        {getErrorMessage(matches.error, "请重新匹配后核对合同")}
                        已填写的信息已保留，重新匹配成功后可以继续归档。
                    </AlertDescription>
                    <Button
                        id="contract-import-identity-retry"
                        type="button"
                        variant="outline"
                        size="sm"
                        onClick={() => void matches.refetch()}
                        disabled={matches.isFetching}
                    >
                        重新匹配
                    </Button>
                </Alert>
            ) : matches.isFetching ? (
                <p
                    className="flex items-center gap-2 text-sm text-muted-foreground"
                    role="status"
                >
                    <Spinner />
                    正在重新匹配签约双方，请稍候…
                </p>
            ) : null}
            <ContractImportReviewForm
                {...props}
                disabled={
                    props.disabled || matches.isError || matches.isFetching
                }
                matches={matches.data}
            />
        </div>
    )
}

function ContractImportReviewForm({
    task,
    step,
    onStepChange,
    busy,
    disabled,
    expectedCustomerId,
    matches,
    onConfirm,
    onSubmitStateChange,
}: Props & {
    matches: ImportIdentityMatches
}) {
    const profile = useAccountProfileQuery()
    const canCreate = hasPermission(
        profile.data?.permissions,
        "customer:create",
    )
    const [defaults] = useState(() => ({
        contract_no: task.draft?.fields.contract_no ?? "",
        customer_name:
            task.draft?.fields.customer_name ||
            matches.customer?.legalName ||
            "",
        customer_credit_code:
            task.draft?.fields.customer_credit_code ||
            matches.customer?.creditCode ||
            "",
        company_name:
            task.draft?.fields.company_name ||
            matches.company?.legal_name ||
            "",
        company_credit_code:
            task.draft?.fields.company_credit_code ||
            matches.company?.unified_credit_code ||
            "",
        payment_terms: task.draft?.fields.payment_terms ?? "",
        invoice_type: task.draft?.fields.invoice_type ?? "",
        tax_point: (task.draft?.fields.tax_point ?? "").replace(/[%％]$/, ""),
        signed_at: task.draft?.fields.signed_at ?? "",
        valid_from: task.draft?.fields.valid_from ?? "",
        valid_to: task.draft?.fields.valid_to ?? "",
        business_scope: task.draft?.fields.business_scope ?? "",
        customerId: matches.customer?.id ?? expectedCustomerId ?? "",
        companyId: matches.company?.id ?? "",
        customerMode: (!expectedCustomerId && !matches.hasAvailableCustomers
            ? "create"
            : "existing") as "existing" | "create",
    }))
    const form = useAppForm({
        defaultValues: defaults,
        validators: {
            onSubmit: ({ value }) => {
                const result = importReviewSchema({
                    canCreate,
                    expectedCustomerId,
                    customer: customer.data,
                    company: company.data,
                }).safeParse(value)
                if (result.success) return undefined
                onStepChange(
                    importReviewFieldStep(
                        String(result.error.issues[0]?.path[0] ?? ""),
                    ),
                )
                return {
                    fields: Object.fromEntries(
                        result.error.issues.map((issue) => [
                            issue.path[0],
                            issue.message,
                        ]),
                    ),
                }
            },
        },
        onSubmit: async ({ value }) => {
            if (busy || disabled || customer.isFetching || company.isFetching)
                return
            const {
                customerId: _customerId,
                companyId: _companyId,
                customerMode: _customerMode,
                ...fields
            } = value
            await onConfirm({
                version: task.version,
                fields: Object.fromEntries(
                    Object.entries(fields).map(([key, text]) => [
                        key,
                        text.trim() || null,
                    ]),
                ),
                ...(value.customerMode === "create" && !value.customerId
                    ? { create_customer: true }
                    : {}),
            })
        },
    })
    const customerId = useStore(form.store, (state) => state.values.customerId)
    const companyId = useStore(form.store, (state) => state.values.companyId)
    const customer = useImportCustomer(customerId)
    const company = useCompanyQuery(companyId || undefined)
    const values = useStore(form.store, (state) => state.values)
    const createOnly =
        !expectedCustomerId && !matches.hasAvailableCustomers && !customerId
    useEffect(() => {
        if (createOnly && values.customerMode !== "create")
            form.setFieldValue("customerMode", "create")
    }, [createOnly, values.customerMode, form])
    const result = importReviewSchema({
        canCreate,
        expectedCustomerId,
        customer: customer.data,
        company: company.data,
    }).safeParse(values)
    const invalidSteps = new Set(
        result.success
            ? []
            : result.error.issues.map((issue) =>
                  importReviewFieldStep(String(issue.path[0] ?? "")),
              ),
    )
    const identitiesValid = !invalidSteps.has(0)
    const termsValid = !invalidSteps.has(1)
    const datesValid = !invalidSteps.has(2)
    const canSubmit = result.success
    const stepHints = [0, 1, 2].map((index) =>
        index === 0 && createOnly && !canCreate
            ? "当前没有可选客户，请联系有权限的同事建档或调整客户归属"
            : result.success
              ? ""
              : (result.error.issues.find(
                    (issue) =>
                        importReviewFieldStep(String(issue.path[0] ?? "")) ===
                        index,
                )?.message ?? ""),
    )
    const [identityHint, termsHint, datesHint] = stepHints
    const isSubmitting = useStore(form.store, (state) => state.isSubmitting)
    const lookupBlocked =
        customer.isError ||
        company.isError ||
        customer.isFetching ||
        company.isFetching
    useEffect(() => {
        onSubmitStateChange({
            taskId: task.id,
            canSubmit: canSubmit && !disabled && !lookupBlocked,
            validSteps: [
                identitiesValid && !disabled && !lookupBlocked,
                termsValid,
                datesValid,
            ],
            isSubmitting,
            stepHints: [identityHint, termsHint, datesHint],
        })
    }, [
        canSubmit,
        identityHint,
        termsHint,
        datesHint,
        identitiesValid,
        termsValid,
        datesValid,
        disabled,
        isSubmitting,
        lookupBlocked,
        onSubmitStateChange,
        task.id,
    ])
    useEffect(
        () => () => {
            onSubmitStateChange({
                taskId: task.id,
                canSubmit: false,
                validSteps: [false, false, false],
                isSubmitting: false,
            })
        },
        [onSubmitStateChange, task.id],
    )
    const chosenCustomer = useRef("")
    const chosenCompany = useRef("")
    useEffect(() => {
        if (!customer.data || chosenCustomer.current !== customer.data.id)
            return
        chosenCustomer.current = ""
        form.setFieldValue("customer_name", customer.data.legalName)
        form.setFieldValue("customer_credit_code", customer.data.creditCode)
    }, [customer.data, form])
    useEffect(() => {
        if (!company.data || chosenCompany.current !== company.data.id) return
        chosenCompany.current = ""
        form.setFieldValue("company_name", company.data.legal_name)
        form.setFieldValue(
            "company_credit_code",
            company.data.unified_credit_code ?? "",
        )
    }, [company.data, form])
    const customerDraft = useRef({
        name: defaults.customer_name,
        code: defaults.customer_credit_code,
    })
    const selectedCustomer = useRef({
        id: defaults.customerId,
        name: defaults.customer_name,
        code: defaults.customer_credit_code,
    })
    const changeCustomerMode = (mode: "existing" | "create") => {
        if (mode === values.customerMode || expectedCustomerId) return
        chosenCustomer.current = ""
        if (mode === "create") {
            selectedCustomer.current = {
                id: values.customerId,
                name: values.customer_name,
                code: values.customer_credit_code,
            }
            form.setFieldValue("customerId", "")
            form.setFieldValue("customer_name", customerDraft.current.name)
            form.setFieldValue(
                "customer_credit_code",
                customerDraft.current.code,
            )
        } else {
            customerDraft.current = {
                name: values.customer_name,
                code: values.customer_credit_code,
            }
            form.setFieldValue("customerId", selectedCustomer.current.id)
            form.setFieldValue("customer_name", selectedCustomer.current.name)
            form.setFieldValue(
                "customer_credit_code",
                selectedCustomer.current.code,
            )
        }
        form.setFieldValue("customerMode", mode)
    }
    const customerConflict = Boolean(
        customerId &&
        customer.data &&
        !result.success &&
        result.error.issues.some((issue) => issue.path[0] === "customerId"),
    )
    const companyConflict = Boolean(
        companyId &&
        company.data &&
        !result.success &&
        result.error.issues.some((issue) => issue.path[0] === "companyId"),
    )
    const warnings = task.draft?.warnings ?? []
    const locked = busy || disabled
    return (
        <form
            id="contract-import-review-form"
            className="space-y-5"
            onSubmit={(event) => {
                event.preventDefault()
                if (locked || isSubmitting || lookupBlocked) return
                if (step < 2) {
                    if (!invalidSteps.has(step))
                        onStepChange((step + 1) as ImportReviewStep)
                    return
                }
                void form.handleSubmit().catch(() => undefined)
            }}
        >
            <section
                hidden={step !== 0}
                className="space-y-4"
                aria-label="签约双方"
            >
                <div className="space-y-1">
                    <h3 className="text-base font-semibold">签约双方</h3>
                    <p className="text-sm text-muted-foreground">
                        核对客户与我方签约主体。
                    </p>
                </div>
                <ContractImportCustomerChoice
                    mode={values.customerMode}
                    recognizedName={task.draft?.fields.customer_name}
                    matched={Boolean(matches.customer)}
                    createOnly={createOnly}
                    candidates={matches.customerCandidates}
                    selectedCustomerId={customerId}
                    onSelectCustomer={(id) => {
                        chosenCustomer.current = id
                        form.setFieldValue("customerId", id)
                    }}
                    expectedCustomerId={expectedCustomerId}
                    disabled={locked || customer.isFetching}
                    canCreate={canCreate}
                    onModeChange={changeCustomerMode}
                >
                    {createOnly && !canCreate ? null : values.customerMode ===
                      "existing" ? (
                        <form.AppField name="customerId">
                            {(field) => (
                                <Field>
                                    <FieldLabel htmlFor="contract-import-customer">
                                        系统客户
                                    </FieldLabel>
                                    <CustomerSearchCombobox
                                        id="contract-import-customer"
                                        value={field.state.value || undefined}
                                        selectedItem={matches.customer}
                                        disabled={
                                            locked ||
                                            Boolean(expectedCustomerId)
                                        }
                                        placeholder="搜索并选择系统客户"
                                        onValueChange={(id) => {
                                            chosenCustomer.current = id ?? ""
                                            field.handleChange(id ?? "")
                                            field.handleBlur()
                                        }}
                                    />
                                    <FieldDescription>
                                        {field.state.value
                                            ? field.state.value !==
                                              defaults.customerId
                                                ? "已手动选择，请与合同原文核对"
                                                : matches.customerMessage
                                            : "请选择合同签约方对应的客户档案"}
                                    </FieldDescription>
                                    {customer.data ? (
                                        <p className="text-xs text-muted-foreground">
                                            统一社会信用代码：
                                            {customer.data.creditCode ||
                                                "未登记"}
                                        </p>
                                    ) : null}
                                    <FieldError
                                        errors={toFieldErrors(
                                            field.state.meta.errors,
                                        )}
                                    />
                                </Field>
                            )}
                        </form.AppField>
                    ) : (
                        <div className="space-y-4">
                            <div className="grid gap-4 sm:grid-cols-2">
                                <form.AppField name="customer_name">
                                    {(field) => (
                                        <field.TextField
                                            id="contract-import-new-customer-name"
                                            label="客户名称"
                                            required
                                            disabled={locked || !canCreate}
                                        />
                                    )}
                                </form.AppField>
                                <form.AppField
                                    name="customer_credit_code"
                                    validators={{
                                        onChange: ({ value, fieldApi }) =>
                                            fieldApi.form.state.values
                                                .customerMode === "create"
                                                ? importNewCustomerCreditCodeSchema.safeParse(
                                                      value,
                                                  ).error?.issues[0]?.message
                                                : undefined,
                                        onBlur: ({ value, fieldApi }) =>
                                            fieldApi.form.state.values
                                                .customerMode === "create"
                                                ? importNewCustomerCreditCodeSchema.safeParse(
                                                      value,
                                                  ).error?.issues[0]?.message
                                                : undefined,
                                    }}
                                >
                                    {(field) => (
                                        <field.TextField
                                            id="contract-import-new-customer-code"
                                            label="统一社会信用代码"
                                            required
                                            placeholder="请输入18位统一社会信用代码"
                                            description={
                                                task.draft?.fields
                                                    .customer_credit_code
                                                    ? "请与合同原文核对"
                                                    : "原文未识别，请补充。"
                                            }
                                            disabled={locked || !canCreate}
                                        />
                                    )}
                                </form.AppField>
                            </div>
                            <p className="text-xs leading-relaxed text-muted-foreground">
                                最终确认归档时优先复用已有档案，缺少时再创建。
                            </p>
                        </div>
                    )}
                    {customerConflict ? (
                        <Alert variant="warning">
                            <AlertDescription>
                                合同签约信息与所选客户档案不一致，请对照原文核对。
                                原文信用代码：
                                {task.draft?.fields.customer_credit_code ||
                                    "未识别"}
                                。
                            </AlertDescription>
                            <Button
                                id="contract-import-apply-customer-identity"
                                type="button"
                                variant="outline"
                                size="sm"
                                className="mt-2 w-fit"
                                disabled={
                                    locked ||
                                    customer.isFetching ||
                                    customer.data?.statusTone !== "success"
                                }
                                onClick={() => {
                                    if (!customer.data) return
                                    form.setFieldValue(
                                        "customer_name",
                                        customer.data.legalName,
                                    )
                                    form.setFieldValue(
                                        "customer_credit_code",
                                        customer.data.creditCode,
                                    )
                                }}
                            >
                                确认使用所选客户信息
                            </Button>
                        </Alert>
                    ) : null}
                    <ContractImportReviewWarnings
                        warnings={warnings}
                        scope="customer"
                    />
                </ContractImportCustomerChoice>
                <div className="rounded-lg border border-border p-4 space-y-3">
                    <form.AppField name="companyId">
                        {(field) => (
                            <Field>
                                <FieldLabel
                                    htmlFor="contract-import-company"
                                    className="font-semibold"
                                >
                                    我方签约主体
                                </FieldLabel>
                                <CompanySearchCombobox
                                    id="contract-import-company"
                                    value={field.state.value || undefined}
                                    disabled={locked}
                                    onValueChange={(id) => {
                                        chosenCompany.current = id ?? ""
                                        field.handleChange(id ?? "")
                                        field.handleBlur()
                                    }}
                                />
                                <FieldDescription>
                                    {field.state.value &&
                                    field.state.value !== defaults.companyId
                                        ? "已手动选择，请与合同原文核对"
                                        : matches.companyMessage}
                                    。结算主体可在销售单中另行调整。
                                </FieldDescription>
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            </Field>
                        )}
                    </form.AppField>
                    {company.data?.unified_credit_code ? (
                        <p className="text-xs text-muted-foreground">
                            统一社会信用代码：{company.data.unified_credit_code}
                        </p>
                    ) : null}
                    {companyConflict ? (
                        <Alert variant="warning">
                            <AlertDescription>
                                签约信息与所选我方主体不一致，请核对原文。
                                合同识别：
                                {task.draft?.fields.company_name ||
                                    "未识别名称"}{" "}
                                ·{" "}
                                {task.draft?.fields.company_credit_code ||
                                    "未识别信用代码"}
                            </AlertDescription>
                            <Button
                                id="contract-import-apply-company-identity"
                                type="button"
                                variant="outline"
                                size="sm"
                                className="mt-2 w-fit"
                                disabled={
                                    locked ||
                                    company.isFetching ||
                                    company.data?.status !== "active"
                                }
                                onClick={() => {
                                    if (!company.data) return
                                    form.setFieldValue(
                                        "company_name",
                                        company.data.legal_name,
                                    )
                                    form.setFieldValue(
                                        "company_credit_code",
                                        company.data.unified_credit_code ?? "",
                                    )
                                }}
                            >
                                确认使用所选我方主体信息
                            </Button>
                        </Alert>
                    ) : null}
                    <ContractImportReviewWarnings
                        warnings={warnings}
                        scope="company"
                    />
                </div>
                {customer.isError || company.isError ? (
                    <Alert variant="destructive">
                        <AlertDescription>
                            {getErrorMessage(
                                customer.error ?? company.error,
                                "主体信息加载失败，请重新选择",
                            )}
                        </AlertDescription>
                    </Alert>
                ) : null}
            </section>
            <section
                hidden={step !== 1}
                className="space-y-5"
                aria-label="合同与结算条款"
            >
                <div className="space-y-1">
                    <h3 className="text-base font-semibold">合同与结算条款</h3>
                    <p className="text-sm text-muted-foreground">
                        核对合同编号、付款与开票约定
                    </p>
                </div>
                <form.AppField name="contract_no">
                    {(field) => (
                        <field.TextField
                            id="contract-import-edit-contract-no"
                            label="合同编号"
                            required
                            disabled={locked}
                        />
                    )}
                </form.AppField>
                <div className="grid gap-3 sm:grid-cols-3">
                    <form.AppField name="payment_terms">
                        {(field) => (
                            <field.SelectField
                                id="contract-import-edit-payment-terms"
                                label="付款条件"
                                required
                                disabled={locked}
                                options={options(PAYMENT)}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="invoice_type">
                        {(field) => (
                            <field.SelectField
                                id="contract-import-edit-invoice-type"
                                label="开票要求"
                                required
                                disabled={locked}
                                options={options(INVOICE)}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="tax_point">
                        {(field) => (
                            <field.SelectField
                                id="contract-import-edit-tax-point"
                                label="税率"
                                required
                                disabled={locked}
                                options={["0", "1", "3", "6", "9", "13"].map(
                                    (value) => ({ value, label: `${value}%` }),
                                )}
                            />
                        )}
                    </form.AppField>
                </div>
                <ContractImportReviewWarnings
                    warnings={warnings}
                    scope="terms"
                />
            </section>
            <section
                hidden={step !== 2}
                className="space-y-5"
                aria-label="日期与业务范围"
            >
                <div className="space-y-1">
                    <h3 className="text-base font-semibold">日期与业务范围</h3>
                    <p className="text-sm text-muted-foreground">
                        核对有效期与业务内容，确认后归档
                    </p>
                </div>
                <div className="grid gap-3 sm:grid-cols-3">
                    <form.AppField name="signed_at">
                        {(field) => (
                            <field.DateField
                                id="contract-import-edit-signed-at"
                                label="签订日期"
                                required
                                disabled={locked}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="valid_from">
                        {(field) => (
                            <field.DateField
                                id="contract-import-edit-valid-from"
                                label="生效日期"
                                required
                                disabled={locked}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="valid_to">
                        {(field) => (
                            <field.DateField
                                id="contract-import-edit-valid-to"
                                label="有效期止"
                                placeholder="选择日期或长期"
                                allowLongTerm
                                required
                                disabled={locked}
                            />
                        )}
                    </form.AppField>
                </div>
                <form.AppField name="business_scope">
                    {(field) => (
                        <field.TextareaField
                            id="contract-import-edit-business-scope"
                            label="业务范围（选填）"
                            disabled={locked}
                            maxLength={4096}
                        />
                    )}
                </form.AppField>
                <ContractImportReviewWarnings
                    warnings={warnings}
                    scope="dates"
                />
                <ContractImportReviewWarnings
                    warnings={warnings}
                    scope="other"
                />
            </section>
        </form>
    )
}
