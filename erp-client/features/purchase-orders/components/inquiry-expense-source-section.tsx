"use client"

import * as React from "react"
import { z } from "zod"

import {
    BusinessFailureState,
    FormalActionConfirmDialog,
    MoneyValue,
    RateValue,
} from "@/components/business"
import { useAppForm, toFieldErrors } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { SupplierSearchCombobox } from "@/features/entity-selectors/components/supplier-search-combobox"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    SUPPLIER_PAYMENT_TERM_OPTIONS,
    paymentTermLabel,
} from "@/lib/business-options"
import { compareDecimal, multiplyFixed } from "@/lib/fixed-decimal"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { formatDateTime } from "@/lib/datetime"
import { hasPermission } from "@/lib/permissions"
import type { InquiryExpenseCandidate } from "../api/expense-sources"
import type { CreateExpensePurchaseInput } from "../api/purchase-expenses"
import { useInquiryExpenseCandidatesQuery } from "../hooks/expense-source-queries"
import { useCreateExpensePurchaseMutation } from "../hooks/queries"
import {
    PURCHASE_EXPENSE_CATEGORY_LABEL,
    type PurchaseExpenseCategory,
} from "../types"
import type { ExpensePurchaseCreated } from "./expense-purchase-create-dialog"

function categoryCode(category: string): PurchaseExpenseCategory | undefined {
    const code = category === "warehousing" ? "storage" : category
    return Object.hasOwn(PURCHASE_EXPENSE_CATEGORY_LABEL, code)
        ? (code as PurchaseExpenseCategory)
        : undefined
}

function candidateReason(candidate: InquiryExpenseCandidate): string | null {
    return (
        candidate.unavailable_reason ??
        (!categoryCode(candidate.category)
            ? "费用类别暂不支持，请重新确认费用"
            : null)
    )
}

/** 在费用来源授权内办理已接受询价费用，不读取已经结束的询价任务。 */
export function InquiryExpenseSourceSection({
    salesOrderId,
    sourceLabel,
    onCreated,
}: {
    salesOrderId: string
    sourceLabel?: string
    onCreated?: (created: ExpensePurchaseCreated) => void
}) {
    const profile = useAccountProfileQuery()
    const allowed =
        hasPermission(profile.data?.permissions, "purchase_order:create") &&
        hasPermission(
            profile.data?.permissions,
            "procurement_inquiry:convert_expenses",
        )
    const query = useInquiryExpenseCandidatesQuery(salesOrderId, allowed)
    const mutation = useCreateExpensePurchaseMutation(salesOrderId)
    const [selected, setSelected] =
        React.useState<InquiryExpenseCandidate | null>(null)
    const [pending, setPending] =
        React.useState<CreateExpensePurchaseInput | null>(null)
    const [failure, setFailure] = React.useState<string | null>(null)
    const commandKey = `erp.inquiry-expense-purchase.v1:${encodeURIComponent(profile.data?.userid ?? "")}:${encodeURIComponent(salesOrderId)}:command`
    React.useEffect(() => {
        if (!profile.data?.userid) return
        try {
            const value = localStorage.getItem(commandKey)
            setPending(
                value
                    ? (JSON.parse(value) as CreateExpensePurchaseInput)
                    : null,
            )
        } catch {
            setFailure("无法恢复原费用操作，请检查本机存储后重试。")
        }
    }, [commandKey, profile.data?.userid])

    async function submit(input: CreateExpensePurchaseInput) {
        if (!allowed || !profile.data?.userid)
            throw new Error("当前账号不能转换询价费用，请重新核对权限。")
        setFailure(null)
        // 原载荷和操作身份保存成功后才允许发出创建命令。
        localStorage.setItem(commandKey, JSON.stringify(input))
        setPending(input)
        let created
        try {
            created = await mutation.mutateAsync(input)
        } catch (error) {
            if (classifyFormalCommandError(error) === "failed") {
                localStorage.removeItem(commandKey)
                setPending(null)
            }
            setFailure(
                getErrorMessage(error, "费用采购创建失败，请保留输入后重试。"),
            )
            throw error
        }
        try {
            localStorage.removeItem(commandKey)
        } catch {
            /* 服务端已确认；旧本机记录后续仍只能重放原结果。 */
        }
        setPending(null)
        setSelected(null)
        await query.refetch()
        onCreated?.({
            purchaseOrderId: created.purchase_order_id,
            purchaseNo: created.purchase_no,
        })
    }

    if (!allowed) return null
    return (
        <section
            className="space-y-3 rounded-lg border p-4"
            aria-label="已接受询价费用来源"
        >
            <div>
                <h3 className="font-medium">从已接受询价费用创建</h3>
                <p className="mt-1 text-sm text-muted-foreground">
                    已在询价中明确的费用须从这里选择，保留原费用和分摊来源。下方手工费用用于其他未关联询价的费用，不会按名称猜测来源。
                </p>
            </div>
            {pending ? (
                <Alert variant="warning">
                    <AlertTitle>上次费用采购结果待确认</AlertTitle>
                    <AlertDescription>
                        已保留原费用、金额和操作内容，请先核对原结果。
                    </AlertDescription>
                    <Button
                        id="expense-inquiry-source-recover"
                        type="button"
                        disabled={mutation.isPending}
                        onClick={() => {
                            void submit(pending).catch(() => undefined)
                        }}
                    >
                        使用原内容核对结果
                    </Button>
                </Alert>
            ) : null}
            {failure ? (
                <Alert variant="destructive">
                    <AlertDescription>{failure}</AlertDescription>
                </Alert>
            ) : null}
            {query.isError ? (
                <BusinessFailureState
                    id="expense-inquiry-source-retry"
                    title="询价费用来源加载失败"
                    error={query.error}
                    onRetry={() => void query.refetch()}
                />
            ) : query.isPending ? (
                <p role="status">正在核对本销售单的询价费用…</p>
            ) : query.data.items.length ? (
                <div className="overflow-x-auto">
                    <Table>
                        <TableHeader>
                            <TableRow>
                                <TableHead>询价及费用</TableHead>
                                <TableHead>可转金额</TableHead>
                                <TableHead>有效条件</TableHead>
                                <TableHead>办理</TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {query.data.items.map((candidate) => {
                                const reason = candidateReason(candidate)
                                return (
                                    <TableRow
                                        key={`${candidate.inquiry_id}:${candidate.allocation_id}`}
                                    >
                                        <TableCell>
                                            <p>{candidate.name}</p>
                                            <p className="text-xs text-muted-foreground">
                                                {candidate.inquiry_no} · 第{" "}
                                                {candidate.feedback_round} 轮
                                            </p>
                                        </TableCell>
                                        <TableCell>
                                            <MoneyValue
                                                value={
                                                    candidate.available_amount
                                                }
                                                display="exact"
                                            />
                                        </TableCell>
                                        <TableCell className="whitespace-normal">
                                            {reason ??
                                                (candidate.valid_conditions ||
                                                    "按本轮已确认费用条件办理")}
                                            <p className="mt-1 text-xs text-muted-foreground">
                                                有效至{" "}
                                                {formatDateTime(
                                                    new Date(
                                                        candidate.valid_until *
                                                            1000,
                                                    ).toISOString(),
                                                )}
                                            </p>
                                        </TableCell>
                                        <TableCell>
                                            <Button
                                                id={`expense-inquiry-source-select-${toAutomationIdSegment(`${candidate.inquiry_id}-${candidate.allocation_id}`)}`}
                                                type="button"
                                                variant="outline"
                                                size="sm"
                                                disabled={
                                                    Boolean(reason) ||
                                                    Boolean(pending) ||
                                                    mutation.isPending ||
                                                    query.isFetching
                                                }
                                                onClick={() =>
                                                    setSelected(candidate)
                                                }
                                            >
                                                选择并预览
                                            </Button>
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                        </TableBody>
                    </Table>
                </div>
            ) : (
                <p className="text-sm text-muted-foreground">
                    本销售单尚无已明确分摊的询价费用。请由销售先确认费用归属，采购再办理。
                </p>
            )}
            {selected ? (
                <InquiryExpenseSourceDialog
                    key={`${selected.inquiry_id}:${selected.allocation_id}`}
                    candidate={selected}
                    salesOrderId={salesOrderId}
                    sourceLabel={sourceLabel}
                    userId={profile.data!.userid}
                    pending={mutation.isPending}
                    unresolved={Boolean(pending)}
                    onClose={() => setSelected(null)}
                    onConfirm={submit}
                />
            ) : null}
        </section>
    )
}

const inputSchema = z.object({
    amount: z.string(),
    supplier_id: z.string(),
    supplier_name: z.string(),
    payment_term_code: z.string(),
})
type SourceFormValues = z.infer<typeof inputSchema>

function InquiryExpenseSourceDialog({
    candidate,
    salesOrderId,
    sourceLabel,
    userId,
    pending,
    unresolved,
    onClose,
    onConfirm,
}: {
    candidate: InquiryExpenseCandidate
    salesOrderId: string
    sourceLabel?: string
    userId: string
    pending: boolean
    unresolved: boolean
    onClose: () => void
    onConfirm: (input: CreateExpensePurchaseInput) => Promise<void>
}) {
    const draftKey = `erp.inquiry-expense-purchase.v1:${encodeURIComponent(userId)}:${encodeURIComponent(salesOrderId)}:${encodeURIComponent(candidate.allocation_id)}:draft`
    const [initial, setInitial] = React.useState<SourceFormValues | null>(null)
    React.useEffect(() => {
        const fallback = {
            amount: candidate.available_amount,
            supplier_id: candidate.payee_supplier_id ?? "",
            supplier_name: "",
            payment_term_code: candidate.payment_term_code ?? "",
        }
        try {
            const stored = localStorage.getItem(draftKey)
            const parsed = stored
                ? inputSchema.safeParse(JSON.parse(stored))
                : null
            setInitial(parsed?.success ? parsed.data : fallback)
        } catch {
            setInitial(fallback)
        }
        // 切换来源才恢复输入，背景刷新不重置已编辑字段。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [draftKey])
    return initial ? (
        <InquiryExpenseSourceForm
            candidate={candidate}
            sourceLabel={sourceLabel}
            initial={initial}
            draftKey={draftKey}
            pending={pending}
            unresolved={unresolved}
            onClose={onClose}
            onConfirm={onConfirm}
        />
    ) : null
}

function InquiryExpenseSourceForm({
    candidate,
    sourceLabel,
    initial,
    draftKey,
    pending,
    unresolved,
    onClose,
    onConfirm,
}: {
    candidate: InquiryExpenseCandidate
    sourceLabel?: string
    initial: SourceFormValues
    draftKey: string
    pending: boolean
    unresolved: boolean
    onClose: () => void
    onConfirm: (input: CreateExpensePurchaseInput) => Promise<void>
}) {
    const [preview, setPreview] = React.useState<{
        input: CreateExpensePurchaseInput
        supplier_name: string
    } | null>(null)
    const [storageFailure, setStorageFailure] = React.useState(false)
    const form = useAppForm({
        defaultValues: initial,
        listeners: {
            onChange: ({ formApi }) => {
                try {
                    localStorage.setItem(
                        draftKey,
                        JSON.stringify(formApi.state.values),
                    )
                    setStorageFailure(false)
                } catch {
                    setStorageFailure(true)
                }
            },
        },
        validators: {
            onSubmit: ({ value }) => {
                try {
                    if (
                        !/^(?:0|[1-9]\d*)(?:\.\d{1,2})?$/.test(value.amount) ||
                        compareDecimal(value.amount, "0", 2) <= 0
                    )
                        return {
                            fields: {
                                amount: "请填写大于 0 且最多两位小数的金额",
                            },
                        }
                    if (
                        compareDecimal(
                            value.amount,
                            candidate.available_amount,
                            2,
                        ) > 0
                    )
                        return {
                            fields: {
                                amount: "金额超过该笔询价费用的可转余额",
                            },
                        }
                } catch {
                    return { fields: { amount: "请填写有效的金额" } }
                }
                if (!value.supplier_id)
                    return {
                        fields: { supplier_id: "请选择本轮费用的收款供应商" },
                    }
                if (
                    candidate.payee_supplier_id &&
                    value.supplier_id !== candidate.payee_supplier_id
                )
                    return {
                        fields: {
                            supplier_id: "收款供应商须与本轮已确认费用依据一致",
                        },
                    }
                if (!value.payment_term_code)
                    return { fields: { payment_term_code: "请选择付款条件" } }
                if (
                    candidate.payment_term_code &&
                    value.payment_term_code !== candidate.payment_term_code
                )
                    return {
                        fields: {
                            payment_term_code:
                                "付款条件须与本轮已确认费用依据一致",
                        },
                    }
                return undefined
            },
        },
        onSubmit: ({ value }) => {
            if (
                storageFailure ||
                pending ||
                unresolved ||
                candidateReason(candidate)
            )
                return
            setPreview({
                supplier_name: value.supplier_name || "本轮已确认收款供应商",
                input: {
                    supplierId: value.supplier_id,
                    paymentTermCode: value.payment_term_code,
                    idempotencyKey: `inquiry-expense:${crypto.randomUUID()}`,
                    lines: [
                        {
                            category: categoryCode(candidate.category)!,
                            description: candidate.name,
                            grossAmount: value.amount,
                            inputTaxRate: candidate.tax_rate,
                            inquirySource: {
                                inquiry_id: candidate.inquiry_id,
                                feedback_round: candidate.feedback_round,
                                fee_id: candidate.fee_id,
                                allocation_id: candidate.allocation_id,
                            },
                        },
                    ],
                },
            })
        },
    })
    const taxPercent = multiplyFixed(candidate.tax_rate, "100", {
        leftMaxScale: 6,
        rightMaxScale: 0,
        outputScale: 2,
    })
    return (
        <>
            <Dialog
                open
                onOpenChange={(open) => {
                    if (!open && !pending) onClose()
                }}
            >
                <DialogContent
                    closeButtonId="expense-inquiry-create-close"
                    className="sm:max-w-2xl"
                >
                    <DialogHeader>
                        <DialogTitle>从询价费用创建采购</DialogTitle>
                        <DialogDescription>
                            {candidate.inquiry_no} · {candidate.name}
                            。费用来源固定保留，提交后进入采购审批。
                        </DialogDescription>
                    </DialogHeader>
                    <p className="text-sm">
                        归属销售单：{sourceLabel || "当前已选择销售单"}
                        ；可转金额：
                        <MoneyValue
                            value={candidate.available_amount}
                            display="exact"
                        />
                        ；税率：
                        <RateValue value={taxPercent} precision={2} />
                    </p>
                    <form
                        className="space-y-4"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <form.AppField name="amount">
                            {(field) => (
                                <field.TextField
                                    id="expense-inquiry-create-amount"
                                    label="本次转换含税金额"
                                    inputMode="decimal"
                                    disabled={pending || unresolved}
                                />
                            )}
                        </form.AppField>
                        <form.AppField name="supplier_id">
                            {(field) => (
                                <Field>
                                    <FieldLabel htmlFor="expense-inquiry-create-supplier">
                                        收款供应商
                                    </FieldLabel>
                                    <SupplierSearchCombobox
                                        id="expense-inquiry-create-supplier"
                                        value={field.state.value || undefined}
                                        disabled={pending || unresolved}
                                        onValueChange={(id) => {
                                            field.handleChange(id ?? "")
                                            field.handleBlur()
                                            form.setFieldValue(
                                                "supplier_name",
                                                "",
                                            )
                                        }}
                                        onItemChange={(item) =>
                                            form.setFieldValue(
                                                "supplier_name",
                                                item?.supplierName ?? "",
                                            )
                                        }
                                    />
                                    <FieldError
                                        errors={toFieldErrors(
                                            field.state.meta.errors,
                                        )}
                                    />
                                </Field>
                            )}
                        </form.AppField>
                        <form.AppField name="payment_term_code">
                            {(field) => (
                                <field.SelectField
                                    id="expense-inquiry-create-payment"
                                    label="付款条件"
                                    options={SUPPLIER_PAYMENT_TERM_OPTIONS}
                                    disabled={pending || unresolved}
                                />
                            )}
                        </form.AppField>
                        <p className="text-sm text-muted-foreground">
                            {candidate.valid_conditions ||
                                "按本轮确认的费用条件办理。"}{" "}
                            收款对象和付款条件不同的费用分别建单。
                        </p>
                        {storageFailure ? (
                            <Alert variant="destructive">
                                <AlertDescription>
                                    输入无法保存到本机，请检查浏览器存储后重试，当前内容仍保留。
                                </AlertDescription>
                            </Alert>
                        ) : null}
                        <DialogFooter>
                            <Button
                                id="expense-inquiry-create-cancel"
                                type="button"
                                variant="outline"
                                disabled={pending}
                                onClick={onClose}
                            >
                                返回
                            </Button>
                            <form.AppForm>
                                <form.SubmitButton
                                    id="expense-inquiry-create-preview"
                                    label="预览费用采购"
                                    disabled={
                                        pending || unresolved || storageFailure
                                    }
                                />
                            </form.AppForm>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
            <FormalActionConfirmDialog
                idPrefix="expense-inquiry-create-confirm"
                open={Boolean(preview)}
                onOpenChange={(open) => {
                    if (!open && !pending) setPreview(null)
                }}
                title="核对询价费用采购"
                actionLabel="创建费用采购"
                confirmLabel="创建并提交审批"
                showStatusChange={false}
                fromStatus={{ label: "已接受费用", tone: "info" }}
                toStatus={{ label: "待采购审批", tone: "success" }}
                pending={pending}
                summary={
                    preview
                        ? [
                              <span key="inquiry">
                                  询价：{candidate.inquiry_no} · 第{" "}
                                  {candidate.feedback_round} 轮
                              </span>,
                              <span key="fee">费用：{candidate.name}</span>,
                              <span key="sales">
                                  归属销售单：
                                  {sourceLabel || "当前已选择销售单"}
                              </span>,
                              <span key="amount">
                                  含税金额：
                                  <MoneyValue
                                      value={preview.input.lines[0].grossAmount}
                                      display="exact"
                                  />
                              </span>,
                              <span key="supplier">
                                  收款供应商：{preview.supplier_name}
                              </span>,
                              <span key="payment">
                                  付款条件：
                                  {paymentTermLabel(
                                      preview.input.paymentTermCode,
                                  )}
                              </span>,
                          ]
                        : []
                }
                effects={[
                    "原子核对销售来源、已接受费用、分摊和余额，创建一张费用采购并提交审批。",
                    "审批中、撤回或驳回仍保留费用占用；已办理金额不能直接释放。",
                ]}
                onConfirm={async () => {
                    if (!preview) return
                    await onConfirm(preview.input)
                    localStorage.removeItem(draftKey)
                    setPreview(null)
                }}
            />
        </>
    )
}
