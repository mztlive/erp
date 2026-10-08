"use client"

import { useEffect, useRef, useState } from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { ChevronRightIcon } from "lucide-react"
import { useAppForm, toFieldErrors } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
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
import { getErrorMessage } from "@/lib/api/errors"
import { hasPermission } from "@/lib/permissions"
import type { ContractImportTask, ConfirmContractImport } from "../api/upload"
import type { ImportIdentityMatches } from "../api/import-identities"
import {
    useImportIdentityMatches,
    useImportCustomer,
} from "../hooks/import-identity-queries"

const FIELDS = {
    contract_no: "合同编号",
    customer_name: "对方签约名称",
    customer_credit_code: "对方统一社会信用代码",
    company_name: "我方签约名称",
    company_credit_code: "我方统一社会信用代码",
    settlement_name: "结算主体",
    settlement_credit_code: "结算主体统一社会信用代码",
    payment_terms: "付款条件",
    invoice_type: "开票要求",
    tax_point: "税率",
    signed_at: "签订日期",
    valid_from: "生效日期",
    valid_to: "有效期止",
    business_scope: "业务范围",
} as const
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
    busy: boolean
    disabled: boolean
    expectedCustomerId?: string
    onConfirm: (command: ConfirmContractImport) => Promise<unknown>
    onSubmitStateChange: (state: ContractImportReviewSubmitState) => void
}

export type ContractImportReviewSubmitState = {
    taskId: string
    canSubmit: boolean
    isSubmitting: boolean
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
        createCustomer: false,
    }))
    const required = (label: string) =>
        z.string().trim().min(1, `请填写${label}`)
    const form = useAppForm({
        defaultValues: defaults,
        validators: {
            onSubmit: z
                .object({
                    contract_no: required("合同编号"),
                    customer_name: required("对方签约名称"),
                    customer_credit_code: z.string(),
                    company_name: required("我方签约名称"),
                    company_credit_code: z.string(),
                    payment_terms: required("付款条件"),
                    invoice_type: required("开票要求"),
                    tax_point: required("税率"),
                    signed_at: required("签订日期"),
                    valid_from: required("生效日期"),
                    valid_to: required("有效期止"),
                    business_scope: required("业务范围"),
                    customerId: z.string(),
                    companyId: z.string().min(1, "请选择系统中的我方签约主体"),
                    createCustomer: z.boolean(),
                })
                .superRefine((value, ctx) => {
                    if (
                        !value.customerId &&
                        !(
                            value.createCustomer &&
                            canCreate &&
                            !expectedCustomerId
                        )
                    )
                        ctx.addIssue({
                            code: "custom",
                            path: ["customerId"],
                            message: "请选择系统客户，或确认匹配并建立客户档案",
                        })
                    if (
                        value.customerId &&
                        customer.data &&
                        (customer.data.statusTone !== "success" ||
                            value.customer_name.trim() !==
                                customer.data.legalName.trim() ||
                            value.customer_credit_code.trim().toUpperCase() !==
                                customer.data.creditCode.trim().toUpperCase())
                    )
                        ctx.addIssue({
                            code: "custom",
                            path: ["customerId"],
                            message:
                                "签约名称或信用代码与所选客户不一致，请展开主体信息核对后修正。",
                        })
                    if (
                        value.companyId &&
                        company.data &&
                        (company.data.status !== "active" ||
                            value.company_name.trim() !==
                                company.data.legal_name.trim() ||
                            value.company_credit_code.trim().toUpperCase() !==
                                (company.data.unified_credit_code ?? "")
                                    .trim()
                                    .toUpperCase())
                    )
                        ctx.addIssue({
                            code: "custom",
                            path: ["companyId"],
                            message:
                                "签约名称或信用代码与所选我方主体不一致，请展开主体信息核对后修正。",
                        })
                }),
        },
        onSubmit: async ({ value }) => {
            if (busy || disabled || customer.isFetching || company.isFetching)
                return
            const {
                customerId: _customerId,
                companyId: _companyId,
                createCustomer,
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
                ...(createCustomer && !value.customerId
                    ? { create_customer: true }
                    : {}),
            })
        },
    })
    const customerId = useStore(form.store, (state) => state.values.customerId)
    const companyId = useStore(form.store, (state) => state.values.companyId)
    const customer = useImportCustomer(customerId)
    const company = useCompanyQuery(companyId || undefined)
    const canSubmit = useStore(form.store, (state) => state.canSubmit)
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
            isSubmitting,
        })
    }, [
        canSubmit,
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
    const locked = busy || disabled
    return (
        <form
            id="contract-import-review-form"
            className="space-y-5"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit().catch(() => undefined)
            }}
        >
            {task.draft?.warnings.length ? (
                <Collapsible>
                    <CollapsibleTrigger
                        id="contract-import-warnings-toggle"
                        className="group flex items-center gap-1 text-sm text-muted-foreground"
                    >
                        <ChevronRightIcon className="size-4 group-data-panel-open:rotate-90" />
                        查看需要核对的识别信息
                    </CollapsibleTrigger>
                    <CollapsibleContent>
                        <ul className="mt-2 list-disc space-y-1 pl-5 text-xs text-muted-foreground">
                            {task.draft.warnings.map((warning) => (
                                <li key={warning}>
                                    {Object.entries(FIELDS).reduce(
                                        (text, [key, label]) =>
                                            text.replaceAll(key, label),
                                        warning,
                                    )}
                                </li>
                            ))}
                        </ul>
                    </CollapsibleContent>
                </Collapsible>
            ) : null}
            <section className="space-y-3" aria-label="签约双方">
                <h3 className="text-sm font-medium">签约双方</h3>
                <div className="grid gap-4 sm:grid-cols-2">
                    <form.AppField name="customerId">
                        {(field) => (
                            <Field>
                                <FieldLabel htmlFor="contract-import-customer">
                                    客户（对方签约主体）
                                </FieldLabel>
                                <CustomerSearchCombobox
                                    id="contract-import-customer"
                                    value={field.state.value || undefined}
                                    selectedItem={matches.customer}
                                    disabled={
                                        locked || Boolean(expectedCustomerId)
                                    }
                                    placeholder="搜索并选择系统客户"
                                    onValueChange={(id) => {
                                        chosenCustomer.current = id ?? ""
                                        field.handleChange(id ?? "")
                                        field.handleBlur()
                                        form.setFieldValue(
                                            "createCustomer",
                                            false,
                                        )
                                    }}
                                />
                                <FieldDescription>
                                    {field.state.value &&
                                    field.state.value !== defaults.customerId
                                        ? "已手动选择，请与合同原文核对"
                                        : matches.customerMessage}
                                </FieldDescription>
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            </Field>
                        )}
                    </form.AppField>
                    <form.AppField name="companyId">
                        {(field) => (
                            <Field>
                                <FieldLabel htmlFor="contract-import-company">
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
                                </FieldDescription>
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            </Field>
                        )}
                    </form.AppField>
                </div>
                <p className="text-xs text-muted-foreground">
                    结算主体默认采用我方签约主体，可在销售单中另行调整。
                </p>
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
                <form.Subscribe selector={(state) => state.values.customerId}>
                    {(id) =>
                        !id && !expectedCustomerId ? (
                            <form.AppField name="createCustomer">
                                {(field) => (
                                    <Field className="rounded-lg border border-border bg-muted/30 p-3">
                                        <div className="flex items-start gap-2">
                                            <Checkbox
                                                id="contract-import-create-customer"
                                                checked={field.state.value}
                                                disabled={locked || !canCreate}
                                                onCheckedChange={(checked) =>
                                                    field.handleChange(checked)
                                                }
                                            />
                                            <FieldLabel
                                                htmlFor="contract-import-create-customer"
                                                className="leading-relaxed"
                                            >
                                                确认按下方对方名称和信用代码匹配或建立客户档案
                                            </FieldLabel>
                                        </div>
                                        <FieldDescription>
                                            {canCreate
                                                ? "确认归档时复用已有企业；尚无客户身份时将新建客户。名称或信用代码冲突需先修正。"
                                                : "你没有创建客户的权限，请选择已有客户或由有权限的同事先建档。"}
                                        </FieldDescription>
                                        <div className="grid gap-3 sm:grid-cols-2">
                                            <form.AppField name="customer_name">
                                                {(name) => (
                                                    <name.TextField
                                                        id="contract-import-new-customer-name"
                                                        label="对方签约名称"
                                                        required
                                                        disabled={locked}
                                                    />
                                                )}
                                            </form.AppField>
                                            <form.AppField name="customer_credit_code">
                                                {(code) => (
                                                    <code.TextField
                                                        id="contract-import-new-customer-code"
                                                        label="统一社会信用代码"
                                                        disabled={locked}
                                                    />
                                                )}
                                            </form.AppField>
                                        </div>
                                    </Field>
                                )}
                            </form.AppField>
                        ) : null
                    }
                </form.Subscribe>
            </section>
            <section className="space-y-3" aria-label="合同与结算条款">
                <h3 className="text-sm font-medium">合同与结算条款</h3>
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
            </section>
            <section className="space-y-3" aria-label="日期与业务范围">
                <h3 className="text-sm font-medium">日期与业务范围</h3>
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
                            <field.TextField
                                id="contract-import-edit-valid-to"
                                label="有效期止"
                                placeholder="YYYY-MM-DD 或长期"
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
                            label="业务范围"
                            required
                            disabled={locked}
                            maxLength={4096}
                        />
                    )}
                </form.AppField>
            </section>
            <Collapsible>
                <CollapsibleTrigger
                    id="contract-import-identity-details"
                    className="group flex items-center gap-1 text-sm text-muted-foreground"
                >
                    <ChevronRightIcon className="size-4 group-data-panel-open:rotate-90" />
                    核对签约名称与信用代码
                </CollapsibleTrigger>
                <CollapsibleContent className="mt-3 space-y-3">
                    <p className="text-xs text-muted-foreground">
                        原文对方：{task.draft?.fields.customer_name || "未识别"}{" "}
                        ·{" "}
                        {task.draft?.fields.customer_credit_code ||
                            "未识别信用代码"}
                        <br />
                        原文我方：{task.draft?.fields.company_name ||
                            "未识别"}{" "}
                        ·{" "}
                        {task.draft?.fields.company_credit_code ||
                            "未识别信用代码"}
                    </p>
                    {customer.data ? (
                        <p className="text-xs text-muted-foreground">
                            所选客户：{customer.data.legalName} ·{" "}
                            {customer.data.creditCode || "未登记信用代码"}
                        </p>
                    ) : null}
                    {company.data ? (
                        <p className="text-xs text-muted-foreground">
                            所选我方主体：{company.data.legal_name} ·{" "}
                            {company.data.unified_credit_code ||
                                "未登记信用代码"}
                        </p>
                    ) : null}
                    <form.Subscribe
                        selector={(state) => state.values.customerId}
                    >
                        {(id) =>
                            id ? (
                                <div className="grid gap-3 sm:grid-cols-2">
                                    <form.AppField name="customer_name">
                                        {(field) => (
                                            <field.TextField
                                                id="contract-import-edit-customer-name"
                                                label="对方签约名称"
                                                required
                                                disabled={locked}
                                            />
                                        )}
                                    </form.AppField>
                                    <form.AppField name="customer_credit_code">
                                        {(field) => (
                                            <field.TextField
                                                id="contract-import-edit-customer-credit-code"
                                                label="对方信用代码"
                                                disabled={locked}
                                            />
                                        )}
                                    </form.AppField>
                                </div>
                            ) : null
                        }
                    </form.Subscribe>
                    <div className="grid gap-3 sm:grid-cols-2">
                        <form.AppField name="company_name">
                            {(field) => (
                                <field.TextField
                                    id="contract-import-edit-company-name"
                                    label="我方签约名称"
                                    required
                                    disabled={locked}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="company_credit_code">
                            {(field) => (
                                <field.TextField
                                    id="contract-import-edit-company-credit-code"
                                    label="我方信用代码"
                                    disabled={locked}
                                />
                            )}
                        </form.AppField>
                    </div>
                </CollapsibleContent>
            </Collapsible>
        </form>
    )
}
