import { ChevronDownIcon, SplitIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { FileUpload } from "@/components/ui/file-upload"
import { selectedFileKey } from "@/lib/file-selection"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Field } from "@/components/ui/field"
import { formatCurrencyFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DraftForm } from "../hooks/use-draft-form"
import { requestAmountError, type DraftGroup } from "../lib/draft"
import type { PaymentCandidate } from "../types"
import { RequestDraftOrderNumber } from "./request-draft-order-number"

export function RequestDraftGroup({
    form,
    group,
    index,
    candidates,
    editing,
    fixedOrder = false,
    open,
    onSplit,
    issues,
    onRefreshCandidates,
}: {
    form: DraftForm
    group: DraftGroup
    index: number
    candidates: PaymentCandidate[]
    editing: boolean
    fixedOrder?: boolean
    open: boolean
    onSplit: (key: string) => void
    issues: string[]
    onRefreshCandidates: () => Promise<unknown>
}) {
    const rows = group.ids
        .map((id) => candidates.find((item) => item.payable_account_id === id)!)
        .filter(Boolean)
    const first = rows[0]
    const id = `ppr-group-${toAutomationIdSegment(group.key)}`
    if (!first)
        return (
            <section
                id={`${id}-panel`}
                hidden={!open}
                aria-labelledby={`${id}-title`}
            >
                <h3
                    id={`${id}-title`}
                    tabIndex={-1}
                    className="font-medium outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                    申请 {index + 1} · 采购来源不可用
                </h3>
                <p className="mt-3 text-sm text-warning-soft-foreground">
                    {fixedOrder
                        ? "当前采购单已不可申请，请关闭弹窗后刷新详情。"
                        : "请返回上一步重新选择采购单。"}
                </p>
            </section>
        )
    const fieldId = (suffix: string) =>
        index === 0 ? `ppr-create-${suffix}` : `${id}-${suffix}`
    return (
        <section
            id={`${id}-panel`}
            hidden={!open}
            aria-labelledby={`${id}-title`}
            className="min-w-0"
        >
            <header className="space-y-2 border-b pb-5">
                <p className="text-xs text-muted-foreground">
                    {fixedOrder
                        ? "当前采购单付款申请"
                        : `申请 ${index + 1} · ${rows.length} 张采购单`}
                </p>
                <h3
                    id={`${id}-title`}
                    tabIndex={-1}
                    className="break-words text-lg font-semibold outline-none focus-visible:ring-2 focus-visible:ring-ring"
                >
                    {first.supplier_name}
                </h3>
                <p className="text-sm text-muted-foreground">
                    付款主体：{first.our_company_name}
                </p>
            </header>
            {issues.length > 0 && (
                <div className="mt-4 rounded-md bg-warning-soft px-3 py-2 text-xs leading-relaxed text-warning-soft-foreground">
                    <p className="font-medium">待补充</p>
                    <ul className="mt-1 list-inside list-disc">
                        {issues.map((message) => (
                            <li key={message}>{message}</li>
                        ))}
                    </ul>
                </div>
            )}
            <div>
                <div className="divide-y">
                    {rows.map((candidate) => (
                        <div
                            key={candidate.payable_account_id}
                            className="grid gap-4 py-5 sm:grid-cols-[minmax(0,1fr)_11rem]"
                        >
                            <div className="min-w-0">
                                <p className="font-medium">
                                    {candidate.purchase_summary ||
                                        candidate.purchase_order_no}
                                </p>
                                {candidate.purchase_summary && (
                                    <RequestDraftOrderNumber
                                        candidate={candidate}
                                    />
                                )}
                                <div className="mt-2 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs">
                                    <span>
                                        可申请{" "}
                                        <MoneyValue
                                            value={candidate.available_amount}
                                        />
                                    </span>
                                    <span className="text-muted-foreground">
                                        {candidate.payment_term_name}
                                    </span>
                                </div>
                                <Collapsible className="mt-1">
                                    <CollapsibleTrigger
                                        id={`ppr-basis-${toAutomationIdSegment(candidate.payable_account_id)}`}
                                        className="group inline-flex items-center gap-1 py-1 text-xs text-muted-foreground hover:text-foreground"
                                    >
                                        付款依据
                                        <ChevronDownIcon className="size-3 group-data-[panel-open]:rotate-180" />
                                    </CollapsibleTrigger>
                                    <CollapsibleContent>
                                        <p className="num mt-1 break-all text-xs text-muted-foreground">
                                            采购单号：
                                            {candidate.purchase_order_no}
                                        </p>
                                        <div className="mt-1 flex flex-wrap gap-x-4 gap-y-1 text-xs text-muted-foreground">
                                            <span>
                                                采购总额{" "}
                                                <MoneyValue
                                                    value={
                                                        candidate.total_amount
                                                    }
                                                />
                                            </span>
                                            <span>
                                                已付{" "}
                                                <MoneyValue
                                                    value={
                                                        candidate.paid_amount
                                                    }
                                                />
                                            </span>
                                            <span>
                                                申请中{" "}
                                                <MoneyValue
                                                    value={
                                                        candidate.reserved_amount
                                                    }
                                                />
                                            </span>
                                        </div>
                                        {candidate.prepayment_gap != null && (
                                            <p className="mt-2 text-xs text-muted-foreground">
                                                预付款尚差{" "}
                                                <MoneyValue
                                                    value={
                                                        candidate.prepayment_gap
                                                    }
                                                />
                                                ，请结合申请中金额核对。
                                            </p>
                                        )}
                                    </CollapsibleContent>
                                </Collapsible>
                                {!editing && !fixedOrder && rows.length > 1 && (
                                    <Button
                                        id={`ppr-split-${toAutomationIdSegment(candidate.payable_account_id)}`}
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        className="mt-1 h-7 px-0 text-xs text-muted-foreground"
                                        onClick={() => {
                                            const current =
                                                form.state.values.groups
                                            const splitKey = `${group.key}:${candidate.payable_account_id}`
                                            form.setFieldValue("groups", [
                                                ...current.map((item) =>
                                                    item.key === group.key
                                                        ? {
                                                              ...item,
                                                              ids: item.ids.filter(
                                                                  (key) =>
                                                                      key !==
                                                                      candidate.payable_account_id,
                                                              ),
                                                          }
                                                        : item,
                                                ),
                                                {
                                                    ...group,
                                                    key: splitKey,
                                                    ids: [
                                                        candidate.payable_account_id,
                                                    ],
                                                    files: [],
                                                },
                                            ])
                                            onSplit(splitKey)
                                        }}
                                    >
                                        <SplitIcon className="size-3" />
                                        单独申请 / 使用其他账户
                                    </Button>
                                )}
                            </div>
                            <form.AppField
                                name={`amounts.${candidate.payable_account_id}`}
                                validators={{
                                    onChange: ({ value }) =>
                                        requestAmountError(
                                            value,
                                            candidate.available_amount,
                                        ),
                                    onBlur: ({ value }) =>
                                        requestAmountError(
                                            value,
                                            candidate.available_amount,
                                        ),
                                }}
                            >
                                {(field) => (
                                    <field.TextField
                                        money
                                        id={`ppr-create-line-${toAutomationIdSegment(candidate.payable_account_id)}`}
                                        label="本次申请金额"
                                        required
                                        type="number"
                                        inputMode="decimal"
                                        min="0.01"
                                        max={candidate.available_amount}
                                        step="0.01"
                                        description={
                                            field.state.meta.isTouched &&
                                            !field.state.meta.isValid
                                                ? undefined
                                                : `最多 ${formatCurrencyFixed(candidate.available_amount, { maxScale: 2, minimumFractionDigits: 2 })}`
                                        }
                                        placeholder="填写本次金额"
                                    />
                                )}
                            </form.AppField>
                        </div>
                    ))}
                </div>
                <div className="space-y-5 border-t pt-5">
                    <div className="grid gap-4 xl:grid-cols-[minmax(0,1fr)_13rem]">
                        <form.AppField name={`groups[${index}].account`}>
                            {(field) => (
                                <field.SelectField
                                    onRefresh={onRefreshCandidates}
                                    id={fieldId("account")}
                                    label="收款账户"
                                    className="min-w-0 gap-2"
                                    required
                                    options={first.bank_accounts.map(
                                        (account) => ({
                                            value: account.bank_account_id,
                                            label: `${account.account_name === first.supplier_name ? "" : `${account.account_name} · `}${account.bank_name} · ${account.account_number_masked}`,
                                        }),
                                    )}
                                    emptyLabel="供应商暂无有效收款账户"
                                />
                            )}
                        </form.AppField>
                        <form.AppField name={`groups[${index}].date`}>
                            {(field) => (
                                <field.DateField
                                    id={fieldId("desired-payment-date")}
                                    label="希望付款日"
                                    className="min-w-0 gap-2"
                                    required
                                />
                            )}
                        </form.AppField>
                    </div>
                    {!first.bank_accounts.length && (
                        <p role="alert" className="text-xs text-destructive">
                            请先维护供应商的有效收款账户，再申请付款。
                        </p>
                    )}
                    <form.AppField name={`groups[${index}].description`}>
                        {(field) => (
                            <field.TextareaField
                                id={fieldId("description")}
                                label="申请说明"
                                required
                                placeholder={
                                    fixedOrder
                                        ? "说明本采购单的付款安排及依据"
                                        : "说明本组采购单的付款安排及依据"
                                }
                                className="gap-2"
                                textareaClassName="min-h-20"
                                rows={2}
                            />
                        )}
                    </form.AppField>
                    <form.Field name={`groups[${index}].files`}>
                        {(field) => (
                            <Field>
                                <FileUpload
                                    idPrefix={`${id}-attachments`}
                                    inputId={fieldId("attachments")}
                                    selectId={`${id}-attachments-trigger`}
                                    density="button"
                                    label="添加附件"
                                    selectLabel="添加附件"
                                    description="选填 · PDF / 图片，每份不超过 5 MB"
                                    multiple
                                    accept="application/pdf,image/jpeg,image/png,image/webp"
                                    maxFileSize={5 * 1024 * 1024}
                                    selectedFiles={field.state.value}
                                    onFileRemove={(file) =>
                                        field.handleChange(
                                            field.state.value.filter(
                                                (item) =>
                                                    selectedFileKey(item) !==
                                                    selectedFileKey(file),
                                            ),
                                        )
                                    }
                                    onFilesSelected={(files) => {
                                        const selected = new Map(
                                            field.state.value.map((file) => [
                                                selectedFileKey(file),
                                                file,
                                            ]),
                                        )
                                        for (const file of files)
                                            selected.set(
                                                selectedFileKey(file),
                                                file,
                                            )
                                        field.handleChange([
                                            ...selected.values(),
                                        ])
                                        field.handleBlur()
                                    }}
                                />
                            </Field>
                        )}
                    </form.Field>
                </div>
            </div>
        </section>
    )
}
