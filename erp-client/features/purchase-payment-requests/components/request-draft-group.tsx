import { useRef } from "react"
import { ChevronDownIcon, PaperclipIcon, SplitIcon } from "lucide-react"
import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { Field } from "@/components/ui/field"
import { formatCurrencyFixed } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DraftForm } from "../hooks/use-draft-form"
import { requestAmountError, totalOf, type DraftGroup } from "../lib/draft"
import type { PaymentCandidate } from "../types"

export function RequestDraftGroup({
    form,
    group,
    index,
    candidates,
    editing,
    open,
    onOpenChange,
}: {
    form: DraftForm
    group: DraftGroup
    index: number
    candidates: PaymentCandidate[]
    editing: boolean
    open: boolean
    onOpenChange: (open: boolean) => void
}) {
    const fileInput = useRef<HTMLInputElement>(null)
    const rows = group.ids
        .map((id) => candidates.find((item) => item.payable_account_id === id)!)
        .filter(Boolean)
    const first = rows[0]
    if (!first) return null
    const id = `ppr-group-${toAutomationIdSegment(group.key)}`
    const fieldId = (suffix: string) =>
        index === 0 ? `ppr-create-${suffix}` : `${id}-${suffix}`
    return (
        <Collapsible
            open={open}
            onOpenChange={onOpenChange}
            className="overflow-hidden rounded-lg border"
            aria-label={`申请 ${index + 1} ${first.supplier_name}`}
        >
            <CollapsibleTrigger
                id={`${id}-trigger`}
                className="flex w-full items-center gap-3 p-4 text-left outline-none hover:bg-muted/40 focus-visible:ring-2 focus-visible:ring-ring"
            >
                <span className="flex size-7 shrink-0 items-center justify-center rounded-full bg-muted text-xs text-muted-foreground">
                    {index + 1}
                </span>
                <span className="min-w-0 flex-1">
                    <span className="block font-medium">
                        {first.supplier_name}
                    </span>
                    <span className="mt-1 block text-xs text-muted-foreground">
                        {rows.length} 张采购单 · {first.our_company_name}
                    </span>
                </span>
                <form.Subscribe selector={(state) => state.values.amounts}>
                    {(amounts) => (
                        <span className="shrink-0 text-sm">
                            <MoneyValue
                                value={totalOf(
                                    group.ids.map((key) => amounts[key] ?? ""),
                                )}
                            />
                        </span>
                    )}
                </form.Subscribe>
                <ChevronDownIcon
                    className={`size-4 shrink-0 text-muted-foreground transition-transform ${open ? "rotate-180" : ""}`}
                />
            </CollapsibleTrigger>
            <CollapsibleContent
                keepMounted
                className="border-t data-[closed]:hidden"
            >
                <div className="divide-y px-4">
                    {rows.map((candidate) => (
                        <div
                            key={candidate.payable_account_id}
                            className="grid gap-3 py-3 sm:grid-cols-[minmax(0,1fr)_12rem]"
                        >
                            <div className="min-w-0">
                                <p className="font-medium">
                                    {candidate.purchase_summary ||
                                        candidate.purchase_order_no}
                                </p>
                                {candidate.purchase_summary && (
                                    <p className="num mt-1 break-all text-xs text-muted-foreground">
                                        {candidate.purchase_order_no}
                                    </p>
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
                                {!editing && rows.length > 1 && (
                                    <Button
                                        id={`ppr-split-${toAutomationIdSegment(candidate.payable_account_id)}`}
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        className="mt-1 h-7 px-0 text-xs text-muted-foreground"
                                        onClick={() => {
                                            const current =
                                                form.state.values.groups
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
                                                    key: `${group.key}:${candidate.payable_account_id}`,
                                                    ids: [
                                                        candidate.payable_account_id,
                                                    ],
                                                    files: [],
                                                },
                                            ])
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
                <div className="space-y-3 border-t p-4">
                    <div className="grid gap-4 sm:grid-cols-[minmax(0,1fr)_19rem]">
                        <form.AppField name={`groups[${index}].account`}>
                            {(field) => (
                                <field.SelectField
                                    id={fieldId("account")}
                                    label="收款账户"
                                    className="gap-2 sm:grid sm:grid-cols-[6rem_minmax(0,1fr)] [&>[data-slot=field-error]]:col-span-2 sm:items-center"
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
                                    className="gap-2 sm:grid sm:grid-cols-[6rem_minmax(0,1fr)] [&>[data-slot=field-error]]:col-span-2 sm:items-center"
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
                                placeholder="说明本组采购单的付款安排及依据"
                                className="gap-2 sm:grid sm:grid-cols-[6rem_minmax(0,1fr)] [&>[data-slot=field-error]]:col-span-2 sm:items-start"
                                textareaClassName="min-h-9"
                                rows={1}
                            />
                        )}
                    </form.AppField>
                    <form.Field name={`groups[${index}].files`}>
                        {(field) => (
                            <Field>
                                <div className="flex flex-wrap items-center gap-3">
                                    <Button
                                        id={`${id}-attachments-trigger`}
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        onClick={() =>
                                            fileInput.current?.click()
                                        }
                                    >
                                        <PaperclipIcon className="size-4" />
                                        添加附件
                                    </Button>
                                    <span className="text-xs text-muted-foreground">
                                        选填 · PDF / 图片，每份不超过 5 MB
                                    </span>
                                </div>
                                <Input
                                    id={fieldId("attachments")}
                                    ref={fileInput}
                                    className="sr-only"
                                    tabIndex={-1}
                                    aria-label="选择本组附件"
                                    type="file"
                                    multiple
                                    accept="application/pdf,image/jpeg,image/png,image/webp"
                                    onChange={(event) =>
                                        field.handleChange(
                                            Array.from(
                                                event.target.files ?? [],
                                            ),
                                        )
                                    }
                                />
                                {!!field.state.value.length && (
                                    <p className="text-xs text-muted-foreground">
                                        已选：
                                        {field.state.value
                                            .map((file) => file.name)
                                            .join("、")}
                                    </p>
                                )}
                            </Field>
                        )}
                    </form.Field>
                </div>
            </CollapsibleContent>
        </Collapsible>
    )
}
