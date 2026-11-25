"use client"

import { useRef, useState, type ReactNode } from "react"
import { useQuery } from "@tanstack/react-query"
import { revalidateLogic, useStore } from "@tanstack/react-form"
import { z } from "zod"
import {
    CircleAlertIcon,
    CircleHelpIcon,
    FileDownIcon,
    InfoIcon,
    PlusIcon,
    Trash2Icon,
} from "lucide-react"

import {
    DetailFormGrid,
    DetailHint,
    DetailInlineTable,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceDirtyBadge,
    DetailWorkspaceMeta,
    DetailWorkspaceSection,
    DiscardConfirmDialog,
    FormalActionConfirmDialog,
    MoneyValue,
    detailWorkspaceStyles,
    type DetailWorkspaceBackLink,
} from "@/components/business"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { useAppForm, withForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { LoadingButton } from "@/components/ui/loading-button"
import { RequiredMark } from "@/components/ui/required-mark"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import {
    ReworkRejectionEcho,
    rejectionContextFromApprovalView,
    type ReworkRejectionContext,
} from "@/features/approval-workflow/components/rework-rejection-echo"
import { SubmissionRouteConfirmation } from "@/features/approval-workflow/components/submission-route-confirmation"
import { useResubmissionReason } from "@/features/approval-workflow/hooks/use-resubmission-reason"
import {
    mapDocumentApprovalViewDto,
    type DocumentApprovalView,
} from "@/features/approval-workflow/types"
import { APPROVAL_INSTANCE_STATUS_META } from "@/lib/task-status-dictionary"
import { loadReceivables } from "@/features/customer-receivables/api/loaders"
import { SalesOrderSearchCombobox } from "@/features/entity-selectors/components/sales-order-search-combobox"
import { getErrorCode, getErrorMessage } from "@/lib/api/errors"
import { formatCurrencyFixed } from "@/lib/fixed-decimal"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { describeMoney } from "@/lib/money-display"

import {
    requestStatusLabels,
    type InvoiceRequest,
    type InvoiceRequestLine,
    type SubmitRequest,
    type UpdateRequest,
} from "../api"
import {
    useInvoiceRequestAmounts,
    useInvoiceRequestCommands,
    useInvoiceRequestPermissions,
    useInvoiceRequestSaveState,
} from "../hooks/queries"
import {
    INVOICE_CONTENT_LIMIT,
    INVOICE_QUANTITY_SCALE,
    INVOICE_REQUEST_LINE_LIMIT,
    INVOICE_UNIT_PRICE_SCALE,
    canonicalInvoiceDecimal,
    invoiceExceedsAvailable,
    invoiceLineAmount,
    invoiceLinesTotal,
    invoiceOverAmount,
    isInvoiceAmountText,
    isPositiveInvoiceDecimal,
} from "../line-amounts"
import { InvoiceRequestFormFooter } from "./request-form-footer"
import {
    InvoiceRequestDialogContext,
    InvoiceRequestFormSidebar,
    type InvoiceArchiveSnapshot,
} from "./request-form-sidebar"
import { SalesOrderLinesImportDialog } from "./sales-order-lines-import-dialog"

type LineInput = InvoiceRequestLine & { rowKey: string }

const zeroLineMessage = "请提高数量或含税单价，使本行金额至少为 0.01 元。"
const lineLimitMessage = `最多填写 ${INVOICE_REQUEST_LINE_LIMIT} 行；如需增加项目，请先删除一行或另发起一笔申请。`

const lineSchema = z.object({
    rowKey: z.string(),
    content: z
        .string()
        .trim()
        .min(1, "请输入开票内容")
        .max(INVOICE_CONTENT_LIMIT, `开票内容最多 ${INVOICE_CONTENT_LIMIT} 字`),
    quantity: z
        .string()
        .refine(
            (value) => isPositiveInvoiceDecimal(value, INVOICE_QUANTITY_SCALE),
            `请输入大于零的数量，最多 ${INVOICE_QUANTITY_SCALE} 位小数`,
        ),
    unit_price: z
        .string()
        .refine(
            (value) =>
                isPositiveInvoiceDecimal(value, INVOICE_UNIT_PRICE_SCALE),
            `请输入大于零的单价，最多 ${INVOICE_UNIT_PRICE_SCALE} 位小数`,
        ),
})

const schema = z
    .object({
        receivableAccountId: z.string(),
        salesOrderId: z.string().min(1, "请选择销售单"),
        lines: z
            .array(lineSchema)
            .min(1, "请至少添加一行开票明细")
            .max(INVOICE_REQUEST_LINE_LIMIT, lineLimitMessage),
        reason: z.string().trim().min(1, "请输入申请事由").max(1000),
    })
    .superRefine(({ lines }, context) => {
        lines.forEach((line, index) => {
            if (invoiceLineAmount(line) === "0.00") {
                context.addIssue({
                    code: "custom",
                    path: ["lines", index, "unit_price"],
                    message: zeroLineMessage,
                })
            }
        })
    })

const formDefaults: z.input<typeof schema> = {
    receivableAccountId: "",
    salesOrderId: "",
    lines: [],
    reason: "",
}

function initialLines(request?: InvoiceRequest): LineInput[] {
    const lines = request?.data.lines?.length
        ? request.data.lines
        : [
              {
                  content: request?.data.invoice_content ?? "",
                  quantity: "",
                  unit_price: "",
              },
          ]
    let nextKey = 0
    return lines.map((line) => ({ ...line, rowKey: `item-${++nextKey}` }))
}

function isBlankLine(line: InvoiceRequestLine): boolean {
    return (
        !line.content.trim() && !line.quantity.trim() && !line.unit_price.trim()
    )
}

export type InvoiceRequestWorkspaceChromeConfig = {
    title?: string
    statusLabel?: string
    meta?: ReactNode
    back?: DetailWorkspaceBackLink
}

const INVOICE_REQUEST_RESUBMISSION_PHRASES = [
    "已按审批意见调整开票明细与申请金额，请复核",
    "已核对可开票额度及申请事由，请复核",
    "已按审批意见修改完善，请复核",
] as const

/**
 * 开票申请表单：
 * - `workspace`：独立新建/修改全页，采用纯白通栏左作业区 + 右参考侧栏 + 吸底核对栏；
 * - `dialog`：销售单内弹窗态，采用紧凑无套娃单栏；
 * - `inline`：嵌入式回退形态。
 * 保留未知结果载荷，锁定编辑后使用同一幂等命令核对结果。
 */
export function InvoiceRequestForm({
    salesOrderId,
    accountId,
    existing: original,
    presentation = "inline",
    workspaceChrome,
    rejectionContext,
    onDone,
    onCancel,
}: {
    salesOrderId?: string
    accountId?: string
    existing?: InvoiceRequest
    presentation?: "workspace" | "inline" | "dialog"
    workspaceChrome?: InvoiceRequestWorkspaceChromeConfig
    rejectionContext?: ReworkRejectionContext
    onDone: (request: InvoiceRequest, action?: "save" | "submit") => void
    onCancel: () => void
}) {
    const permissions = useInvoiceRequestPermissions()
    const commands = useInvoiceRequestCommands()
    const [adopted, setAdopted] = useState<Pick<
        InvoiceRequest,
        "id" | "version" | "status"
    > | null>(null)
    const existing =
        original && adopted?.id === original.id
            ? { ...original, ...adopted }
            : original
    const saveState = useInvoiceRequestSaveState(existing?.id)
    const [checked, setChecked] = useState<Pick<
        InvoiceRequest,
        "id" | "version" | "status"
    > | null>(null)

    const [defaultLines] = useState<LineInput[]>(() => initialLines(existing))
    const nextLineKey = useRef(defaultLines.length)
    const createRowKey = () => {
        nextLineKey.current += 1
        return `item-${nextLineKey.current}`
    }

    const pending = useRef<SubmitRequest | null>(null)
    const pendingSave = useRef<UpdateRequest | null>(null)
    const intent = useRef<"submit" | "save">("submit")
    const [saveUncertain, setSaveUncertain] = useState(false)
    const [uncertain, setUncertain] = useState(false)
    const [committedUnreadable, setCommittedUnreadable] = useState(false)
    const [error, setError] = useState<unknown>(null)
    const [pickedOrder, setPickedOrder] = useState<{
        id: string
        documentNumber?: string
    } | null>(null)
    const [importOpen, setImportOpen] = useState(false)
    const [pendingDiscardAction, setPendingDiscardAction] = useState<
        (() => void) | null
    >(null)
    const [confirmSubmitInput, setConfirmSubmitInput] =
        useState<SubmitRequest | null>(null)

    const rework = useResubmissionReason({
        open: true,
        required: existing?.status === "rejected",
        id: "invoice-request-resubmission-reason",
        disabled:
            commands.submit.isPending ||
            commands.update.isPending ||
            uncertain ||
            saveUncertain,
        quickPhrases: INVOICE_REQUEST_RESUBMISSION_PHRASES,
    })

    const form = useAppForm({
        defaultValues: {
            receivableAccountId:
                existing?.receivable_account_id ?? accountId ?? "",
            salesOrderId: existing?.sales_order_id ?? salesOrderId ?? "",
            lines: defaultLines,
            reason: existing?.data.reason ?? "",
        },
        validationLogic: revalidateLogic(),
        validators: { onDynamic: schema },
        onSubmit: async ({ value }) => {
            if (
                commands.update.isPending ||
                commands.submit.isPending ||
                uncertain ||
                saveUncertain
            )
                return
            const lines = value.lines.map(
                ({ content, quantity, unit_price }) => ({
                    content: content.trim(),
                    quantity: canonicalInvoiceDecimal(quantity),
                    unit_price: canonicalInvoiceDecimal(unit_price),
                }),
            )
            const total = invoiceLinesTotal(lines)
            if (!total || !isPositiveInvoiceDecimal(total, 2)) return
            const data = {
                amount: total,
                invoice_content: lines[0]?.content ?? "",
                reason: value.reason.trim(),
                seller_name: amounts.data?.seller_name,
                seller_tax_number: amounts.data?.seller_tax_number,
                lines,
            }
            if (intent.current === "save" && existing) {
                const input = pendingSave.current ?? {
                    id: existing.id,
                    expected_version: existing.version,
                    data: {
                        ...data,
                        invoice_title:
                            amounts.data?.invoice_title ??
                            existing.data.invoice_title,
                        tax_number:
                            amounts.data?.tax_number ??
                            existing.data.tax_number,
                    },
                }
                pendingSave.current = input
                await save(input)
                return
            }
            if (
                !rework.valid ||
                !resolvedAccountId ||
                !amounts.data?.can_submit ||
                amounts.isError ||
                amounts.isFetching ||
                invoiceExceedsAvailable(total, amounts.data.available_amount)
            )
                return
            const input = pending.current ?? {
                resubmission_reason: rework.reason,
                receivable_account_id: resolvedAccountId,
                request_id: existing?.id,
                expected_version: existing?.version,
                data: {
                    ...data,
                    invoice_title: amounts.data.invoice_title,
                    tax_number: amounts.data.tax_number,
                },
                idempotency_key: crypto.randomUUID(),
            }
            setConfirmSubmitInput(input)
        },
    })

    const selectedOrder = useStore(form.store, (s) => s.values.salesOrderId)
    const selectedAccount = useStore(
        form.store,
        (s) => s.values.receivableAccountId,
    )
    const formLines = useStore(form.store, (s) => s.values.lines)
    const isFormDirty = useStore(form.store, (s) => !s.isDefaultValue)
    const linesTotal = invoiceLinesTotal(formLines)

    const accounts = useQuery({
        queryKey: ["invoice-requests", "source", selectedOrder],
        queryFn: () =>
            loadReceivables({
                view: "receivable",
                page: 1,
                pageSize: 100,
                salesOrderId: selectedOrder,
            }),
        enabled: Boolean(selectedOrder) && !accountId && !existing,
    })

    const resolvedAccountId =
        existing?.receivable_account_id ??
        accountId ??
        (accounts.data?.items.find((account) => account.id === selectedAccount)
            ?.id ||
            (accounts.data?.items.length === 1
                ? accounts.data.items[0]?.id
                : undefined))
    const amounts = useInvoiceRequestAmounts(resolvedAccountId)

    const locked =
        commands.submit.isPending ||
        commands.update.isPending ||
        uncertain ||
        saveUncertain
    const isDirty = isFormDirty || Boolean(rework.reason?.trim())
    useWorkspaceDirtyState({
        dirty: isDirty,
        busy: locked,
    })
    function requestLeave(action: () => void) {
        if (locked) return
        if (isDirty) {
            setPendingDiscardAction(() => action)
            return
        }
        action()
    }
    const latest = checked?.id === existing?.id ? checked : null
    const versionAdvanced = Boolean(
        latest &&
        pendingSave.current &&
        latest.version > pendingSave.current.expected_version,
    )
    const latestEditable =
        latest?.status === "draft" || latest?.status === "rejected"

    const archive: InvoiceArchiveSnapshot = {
        invoiceTitle:
            pending.current?.data.invoice_title ??
            amounts.data?.invoice_title ??
            existing?.data.invoice_title ??
            "",
        taxNumber:
            pending.current?.data.tax_number ??
            amounts.data?.tax_number ??
            existing?.data.tax_number ??
            "",
        sellerName:
            pending.current?.data.seller_name ??
            pendingSave.current?.data.seller_name ??
            (amounts.data
                ? amounts.data.seller_name
                : existing?.data.seller_name) ??
            "",
        sellerTaxNumber:
            pending.current?.data.seller_tax_number ??
            pendingSave.current?.data.seller_tax_number ??
            (amounts.data
                ? amounts.data.seller_tax_number
                : existing?.data.seller_tax_number) ??
            "",
    }

    const exceedsAvailable = invoiceExceedsAvailable(
        linesTotal,
        amounts.data?.available_amount,
    )
    const overAmount = invoiceOverAmount(
        linesTotal,
        amounts.data?.available_amount,
    )
    const canImportSalesLines = Boolean(
        permissions.canReadSalesOrder && selectedOrder,
    )
    const hasMeaningfulExistingLines = formLines.some(
        (line) => !isBlankLine(line),
    )
    const showLegacyAmountHint = Boolean(
        existing &&
        !existing.data.lines?.length &&
        isInvoiceAmountText(existing.data.amount) &&
        !linesTotal,
    )
    const showOrderSelector = !salesOrderId && !existing
    const showAccountSelector =
        !accountId && !existing && (accounts.data?.items.length ?? 0) > 1
    const showSourceSection = showOrderSelector || showAccountSelector

    const canSubmit =
        !locked &&
        rework.valid &&
        !amounts.isError &&
        !amounts.isFetching &&
        Boolean(amounts.data?.can_submit) &&
        !exceedsAvailable

    const { statusMessage, isErrorStatus } = (() => {
        if (uncertain) {
            return {
                statusMessage: "提交结果尚未确认，请核对本次提交结果",
                isErrorStatus: true,
            }
        }
        if (saveUncertain) {
            return {
                statusMessage: "保存结果尚未确认，请读取最新版本核对",
                isErrorStatus: true,
            }
        }
        if (!selectedOrder && !existing) {
            return {
                statusMessage: "请先选择已生效的销售单",
                isErrorStatus: false,
            }
        }
        if (accounts.isError || amounts.isError) {
            return {
                statusMessage: "销售应收读取失败，请重新读取",
                isErrorStatus: true,
            }
        }
        if (accounts.isFetching || amounts.isFetching) {
            return {
                statusMessage: "正在核对本单可申请额度…",
                isErrorStatus: false,
            }
        }
        if (!resolvedAccountId) {
            return {
                statusMessage:
                    (accounts.data?.items.length ?? 0) > 1
                        ? "请选择本次开票的结算主体"
                        : "该销售单暂无可开票应收，请确认是否已生效",
                isErrorStatus: false,
            }
        }
        if (amounts.data && !amounts.data.can_submit) {
            return {
                statusMessage:
                    amounts.data.unavailable_reason || "当前暂不可提交开票申请",
                isErrorStatus: true,
            }
        }
        if (exceedsAvailable && overAmount) {
            return {
                statusMessage: `超出可申请额度 ${formatCurrencyFixed(overAmount, { maxScale: 2, minimumFractionDigits: 2 })}，请调减明细`,
                isErrorStatus: true,
            }
        }
        if (!rework.valid) {
            return {
                statusMessage: "请填写重新提交说明",
                isErrorStatus: false,
            }
        }
        if (!linesTotal) {
            return {
                statusMessage: "请完善明细数量与含税单价，系统将自动合计金额",
                isErrorStatus: false,
            }
        }
        return {
            statusMessage: `共 ${formLines.length} 行明细，核对无误后可提交审批`,
            isErrorStatus: false,
        }
    })()

    async function checkSave() {
        setChecked(null)
        const result = await saveState.refetch()
        if (result.isError) {
            setError(result.error)
            return
        }
        if (result.data) setChecked(result.data)
    }

    function adoptLatest() {
        if (!latest || !versionAdvanced || !latestEditable) return
        setAdopted(latest)
        pendingSave.current = null
        setSaveUncertain(false)
        setError(null)
        setChecked(null)
    }

    async function save(input: UpdateRequest) {
        setError(null)
        setChecked(null)
        try {
            const result = await commands.update.mutateAsync(input)
            pendingSave.current = null
            setSaveUncertain(false)
            onDone(result, "save")
        } catch (err) {
            setError(err)
            const unknown = classifyFormalCommandError(err) === "unknown"
            setSaveUncertain((previous) => previous || unknown)
            if (!unknown && !saveUncertain) pendingSave.current = null
        }
    }

    /** 成功时关闭表单；未知结果保持同一请求，明确失败才允许修改。 */
    async function send(input: SubmitRequest) {
        setError(null)
        try {
            const result = await commands.submit.mutateAsync(input)
            pending.current = null
            setUncertain(false)
            setCommittedUnreadable(false)
            onDone(result, "submit")
        } catch (err) {
            setError(err)
            const committed =
                getErrorCode(err) === "COMMITTED_RESULT_UNAVAILABLE"
            const unknown =
                committed || classifyFormalCommandError(err) === "unknown"
            setCommittedUnreadable((previous) => previous || committed)
            setUncertain((previous) => previous || unknown)
            if (!unknown && !uncertain) pending.current = null
        }
    }

    function handleSaveAction() {
        if (saveUncertain && pendingSave.current) {
            void save(pendingSave.current)
            return
        }
        intent.current = "save"
        void form.handleSubmit()
    }

    function handleImportSalesLines(
        imported: InvoiceRequestLine[],
        mode: "replace" | "append",
    ) {
        const nextImported: LineInput[] = imported.map((line) => ({
            ...line,
            rowKey: createRowKey(),
        }))
        if (mode === "replace") {
            form.setFieldValue("lines", nextImported)
            return
        }
        const current = form.getFieldValue("lines")
        const kept = current.filter((line) => !isBlankLine(line))
        form.setFieldValue(
            "lines",
            [...kept, ...nextImported].slice(0, INVOICE_REQUEST_LINE_LIMIT),
        )
    }

    function retryContextQueries() {
        if (accounts.isError) void accounts.refetch()
        if (amounts.isError) void amounts.refetch()
    }

    const existingApproval: DocumentApprovalView | undefined = (() => {
        const source = existing?.approval
        if (
            !source ||
            (source.definition && !Array.isArray(source.definition.nodes)) ||
            (source.recent_history != null &&
                !Array.isArray(source.recent_history))
        ) {
            return undefined
        }
        return mapDocumentApprovalViewDto({
            ...source,
            recent_history: source.recent_history ?? [],
            allowed_actions: source.allowed_actions ?? [],
        })
    })()

    const resolvedRejection =
        rejectionContext ?? rejectionContextFromApprovalView(existingApproval)

    const alertsNode = (
        <>
            {presentation === "workspace" ? (
                <ReworkRejectionEcho rejection={resolvedRejection} />
            ) : null}
            {amounts.data?.unavailable_reason ? (
                <Alert variant="warning">
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertTitle>当前暂不可提交开票申请</AlertTitle>
                    <AlertDescription>
                        {amounts.data.unavailable_reason}
                    </AlertDescription>
                </Alert>
            ) : null}
            {exceedsAvailable && overAmount && amounts.data ? (
                <Alert variant="destructive">
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertTitle>本次申请金额超出可申请额度</AlertTitle>
                    <AlertDescription>
                        明细合计已超过本单可申请额度{" "}
                        <span className="num font-medium">
                            {formatCurrencyFixed(overAmount, {
                                maxScale: 2,
                                minimumFractionDigits: 2,
                            })}
                        </span>
                        ，请调减明细数量或含税单价后再提交。
                    </AlertDescription>
                </Alert>
            ) : null}
            {error ? (
                <Alert variant="destructive">
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertDescription className="text-body-compact">
                        {saveUncertain
                            ? "保存结果尚未确认，请核对本次保存结果。"
                            : committedUnreadable
                              ? "已提交，结果暂不可读取。请稍后核对本次提交结果。"
                              : uncertain
                                ? "提交结果尚未确认，请核对本次提交结果，勿重复新建申请。"
                                : getErrorMessage(
                                      error,
                                      "提交失败，请检查后重试",
                                  )}
                    </AlertDescription>
                </Alert>
            ) : null}
            {saveUncertain ? (
                <Alert variant="warning">
                    <CircleAlertIcon aria-hidden="true" />
                    <AlertTitle>保存结果待核对</AlertTitle>
                    <AlertDescription className="space-y-3">
                        <p>刚才的保存结果尚未确认，已保留您当前填写的内容。</p>
                        <div className="flex flex-wrap items-center gap-2">
                            <LoadingButton
                                id="invoice-request-check-save"
                                type="button"
                                variant="outline"
                                size="sm"
                                loading={saveState.isFetching}
                                disabled={
                                    commands.update.isPending ||
                                    saveState.isFetching
                                }
                                onClick={() => void checkSave()}
                            >
                                读取最新版本
                            </LoadingButton>
                            {versionAdvanced && latestEditable ? (
                                <Button
                                    id="invoice-request-adopt-latest"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    disabled={
                                        saveState.isFetching ||
                                        commands.update.isPending
                                    }
                                    onClick={adoptLatest}
                                >
                                    采用最新版本并保留输入
                                </Button>
                            ) : null}
                            {versionAdvanced && !latestEditable ? (
                                <Button
                                    id="invoice-request-exit-save-recovery"
                                    type="button"
                                    variant="outline"
                                    size="sm"
                                    disabled={
                                        saveState.isFetching ||
                                        commands.update.isPending
                                    }
                                    onClick={onCancel}
                                >
                                    退出编辑
                                </Button>
                            ) : null}
                        </div>
                        {saveState.isError ? (
                            <p role="alert" className="text-destructive">
                                {getErrorMessage(
                                    saveState.error,
                                    "读取最新版本失败，请重试",
                                )}
                            </p>
                        ) : null}
                        {latest ? (
                            <p>
                                {versionAdvanced
                                    ? `该申请单已被更新（当前状态：${requestStatusLabels[latest.status]}），请加载最新内容后再修改。`
                                    : "刚才的保存尚未生效，可点击重新保存继续提交。"}
                            </p>
                        ) : null}
                        {versionAdvanced && !latestEditable ? (
                            <p>
                                当前状态不能修改。可保留此页查看本地输入，或退出编辑；退出不表示原保存成功。
                            </p>
                        ) : null}
                    </AlertDescription>
                </Alert>
            ) : null}
        </>
    )

    const sourceFields = (
        <DetailFormGrid>
            {showOrderSelector ? (
                <div className="sm:col-span-6">
                    <form.AppField name="salesOrderId">
                        {(field) => {
                            const isInvalid =
                                field.state.meta.isTouched &&
                                !field.state.meta.isValid
                            return (
                                <Field data-invalid={isInvalid || undefined}>
                                    <FieldLabel htmlFor="invoice-request-sales-order">
                                        销售单
                                        <RequiredMark />
                                    </FieldLabel>
                                    <SalesOrderSearchCombobox
                                        id="invoice-request-sales-order"
                                        className="w-full"
                                        value={field.state.value}
                                        onValueChange={(id) => {
                                            field.handleChange(id ?? "")
                                            form.setFieldValue(
                                                "receivableAccountId",
                                                "",
                                            )
                                            if (!id) setPickedOrder(null)
                                        }}
                                        onItemChange={(item) => {
                                            setPickedOrder(
                                                item
                                                    ? {
                                                          id: item.id,
                                                          documentNumber:
                                                              item.documentNumber,
                                                      }
                                                    : null,
                                            )
                                        }}
                                        disabled={locked}
                                    />
                                    {isInvalid ? (
                                        <FieldError
                                            id="invoice-request-sales-order-error"
                                            errors={field.state.meta.errors}
                                        />
                                    ) : null}
                                </Field>
                            )
                        }}
                    </form.AppField>
                </div>
            ) : null}

            {showAccountSelector ? (
                <div className="sm:col-span-6">
                    <form.AppField name="receivableAccountId">
                        {(field) => (
                            <field.SelectField
                                onRefresh={() =>
                                    accounts.refetch({ throwOnError: true })
                                }
                                id="invoice-request-account"
                                label="开票结算主体"
                                placeholder="选择本次开票对应的结算主体"
                                required
                                disabled={locked}
                                options={(accounts.data?.items ?? []).map(
                                    (account) => ({
                                        value: account.id,
                                        label: `第 ${account.account_seq} 笔 · ${account.counterparty_party_name ?? account.customer_name} · 未开票 ${account.open_invoiceable_total} 元`,
                                    }),
                                )}
                            />
                        )}
                    </form.AppField>
                </div>
            ) : null}

            {showOrderSelector &&
            selectedOrder &&
            !accounts.isPending &&
            !accounts.isError &&
            (accounts.data?.items.length ?? 0) === 0 ? (
                <div className="sm:col-span-12">
                    <Alert variant="warning">
                        <CircleAlertIcon aria-hidden="true" />
                        <AlertDescription>
                            该销售单暂无可开票的应收记录，请核对销售单是否已生效。
                        </AlertDescription>
                    </Alert>
                </div>
            ) : null}
        </DetailFormGrid>
    )

    const reasonFields = (
        <fieldset disabled={locked}>
            <DetailFormGrid layout="stack" className="space-y-4">
                <form.AppField name="reason">
                    {(field) => (
                        <field.TextareaField
                            id="invoice-request-reason"
                            label="申请事由"
                            placeholder="请输入本次开票背景、合同节点或财务开票注意事项"
                            rows={3}
                            maxLength={1000}
                            required
                        />
                    )}
                </form.AppField>
                {rework.field}
            </DetailFormGrid>
        </fieldset>
    )

    const importDialog = canImportSalesLines ? (
        <SalesOrderLinesImportDialog
            open={importOpen}
            onOpenChange={setImportOpen}
            salesOrderId={selectedOrder}
            availableAmount={amounts.data?.available_amount}
            existingLineCount={formLines.length}
            hasMeaningfulExistingLines={hasMeaningfulExistingLines}
            onImport={handleImportSalesLines}
        />
    ) : null

    const confirmOrderNo =
        existing?.sales_order_no ||
        (pickedOrder?.id === selectedOrder
            ? pickedOrder.documentNumber
            : undefined)

    const submitConfirmDialog = (
        <FormalActionConfirmDialog
            idPrefix="invoice-request-submit-confirm"
            layout="compact"
            open={confirmSubmitInput !== null}
            onOpenChange={(open) => {
                if (!open && !commands.submit.isPending) {
                    setConfirmSubmitInput(null)
                }
            }}
            pending={commands.submit.isPending}
            title={
                existing?.status === "rejected"
                    ? "重新提交销项开票申请"
                    : "提交销项开票申请"
            }
            description="请核对本次开票金额与审批路线，确认后进入审批流程。"
            actionLabel="提交审批"
            confirmLabel={uncertain ? "核对提交结果" : "确认提交审批"}
            cancelLabel="返回修改"
            actionVariant="default"
            fromStatus={{
                label: existing ? requestStatusLabels[existing.status] : "新建",
                tone:
                    existing?.status === "rejected" ? "destructive" : "neutral",
            }}
            toStatus={{
                label: APPROVAL_INSTANCE_STATUS_META.RUNNING.label,
                tone: APPROVAL_INSTANCE_STATUS_META.RUNNING.tone,
            }}
            summary={
                confirmSubmitInput
                    ? [
                          ...(confirmOrderNo
                              ? [
                                    <span key="order">
                                        来源销售单：
                                        <span className="num font-medium text-foreground">
                                            {confirmOrderNo}
                                        </span>
                                    </span>,
                                ]
                              : []),
                          ...(confirmSubmitInput.data.invoice_title
                              ? [
                                    <span key="title">
                                        开票抬头：
                                        {confirmSubmitInput.data.invoice_title}
                                    </span>,
                                ]
                              : []),
                          <span
                              key="amount"
                              className="inline-flex items-baseline gap-1"
                          >
                              申请金额：
                              <MoneyValue
                                  value={confirmSubmitInput.data.amount}
                                  display="exact"
                              />
                          </span>,
                          <span key="lines" className="num">
                              开票明细：
                              {confirmSubmitInput.data.lines?.length ?? 0} 行
                          </span>,
                      ]
                    : []
            }
            effects={[
                "提交后占用本笔应收可申请开票额度，审批期间不可修改。",
                "审批通过后由财务按本申请登记开具销项发票。",
            ]}
            formContent={
                <SubmissionRouteConfirmation
                    documentType="sales_invoice_request"
                    definition={existingApproval?.definition}
                    fallbackDescription={
                        existingApproval
                            ? undefined
                            : "提交后将按当前已发布的销项开票申请审批流程自动绑定审批人并启动审批；任一节点驳回将退回提交人修改重提。"
                    }
                />
            }
            onConfirm={async () => {
                if (!confirmSubmitInput) return
                const payload = pending.current ?? confirmSubmitInput
                pending.current = payload
                await send(payload)
                setConfirmSubmitInput(null)
            }}
            onConfirmError={() => undefined}
        />
    )

    const discardDialog = (
        <DiscardConfirmDialog
            idPrefix="invoice-request-form-discard"
            open={pendingDiscardAction !== null}
            onOpenChange={(open) => {
                if (!open) setPendingDiscardAction(null)
            }}
            onConfirm={() => {
                const action = pendingDiscardAction
                setPendingDiscardAction(null)
                action?.()
            }}
        />
    )

    if (presentation === "workspace") {
        const orderNo =
            existing?.sales_order_no ||
            (pickedOrder?.id === selectedOrder
                ? pickedOrder.documentNumber
                : undefined)
        const guardedBack =
            workspaceChrome?.back &&
            "onClick" in workspaceChrome.back &&
            workspaceChrome.back.onClick
                ? {
                      ...workspaceChrome.back,
                      onClick: () =>
                          requestLeave(
                              workspaceChrome.back!.onClick as () => void,
                          ),
                  }
                : workspaceChrome?.back
        const chromeMeta = (
            <DetailWorkspaceMeta
                items={[
                    existing?.request_no ? (
                        <span key="req-no" className="num">
                            申请单号 {existing.request_no}
                        </span>
                    ) : null,
                    orderNo ? (
                        <span key="order-no" className="num">
                            销售单 {orderNo}
                        </span>
                    ) : null,
                    workspaceChrome?.meta ??
                        (existing
                            ? "修改开票明细后保存或重新提交审批"
                            : "关联已生效销售单并录入开票明细，审批通过后由财务开票"),
                ]}
            />
        )

        return (
            <>
                <form
                    id="invoice-request-form"
                    className="flex min-w-0 flex-1 flex-col"
                    onSubmit={(event) => {
                        event.preventDefault()
                        intent.current = "submit"
                        void form.handleSubmit()
                    }}
                >
                    <DetailWorkspaceChrome
                        back={guardedBack}
                        title={
                            workspaceChrome?.title ??
                            (existing ? "修改开票申请" : "新建开票申请")
                        }
                        primaryStatus={{
                            label:
                                workspaceChrome?.statusLabel ??
                                (existing ? "修改中" : "新建"),
                            tone: "neutral",
                        }}
                        titleExtra={
                            isDirty ? <DetailWorkspaceDirtyBadge /> : undefined
                        }
                        meta={chromeMeta}
                    />

                    <DetailWorkspaceBody
                        alerts={alertsNode}
                        sidebarLabel="开票额度与档案"
                        sidebar={
                            <InvoiceRequestFormSidebar
                                hasOrder={Boolean(selectedOrder)}
                                hasAccount={Boolean(resolvedAccountId)}
                                isLoading={
                                    accounts.isFetching || amounts.isFetching
                                }
                                isError={accounts.isError || amounts.isError}
                                onRetry={retryContextQueries}
                                amounts={amounts.data}
                                archive={archive}
                                linesTotal={linesTotal}
                            />
                        }
                    >
                        {showSourceSection ? (
                            <DetailWorkspaceSection
                                title="关联业务单据"
                                width="form"
                            >
                                {sourceFields}
                            </DetailWorkspaceSection>
                        ) : null}

                        <InvoiceLinesSection
                            form={form}
                            locked={locked}
                            presentation="workspace"
                            canImportSalesLines={canImportSalesLines}
                            onOpenImport={() => setImportOpen(true)}
                            createRowKey={createRowKey}
                            availableAmount={amounts.data?.available_amount}
                            legacyAmountHint={
                                showLegacyAmountHint
                                    ? existing?.data.amount
                                    : undefined
                            }
                        />

                        <DetailWorkspaceSection title="申请说明" width="form">
                            {reasonFields}
                        </DetailWorkspaceSection>
                    </DetailWorkspaceBody>

                    <InvoiceRequestFormFooter
                        availableAmount={amounts.data?.available_amount}
                        linesTotal={linesTotal}
                        statusMessage={statusMessage}
                        isErrorStatus={isErrorStatus}
                        locked={locked}
                        isEditing={Boolean(existing)}
                        uncertain={uncertain}
                        saveUncertain={saveUncertain}
                        versionAdvanced={versionAdvanced}
                        submitPending={commands.submit.isPending}
                        savePending={commands.update.isPending}
                        saveStateFetching={saveState.isFetching}
                        canSubmit={canSubmit}
                        onCancel={() => requestLeave(onCancel)}
                        onSave={handleSaveAction}
                        onResolveUncertain={() =>
                            pending.current && void send(pending.current)
                        }
                    />
                </form>
                {importDialog}
                {submitConfirmDialog}
                {discardDialog}
            </>
        )
    }

    const compactBodyContent = (
        <>
            <ReworkRejectionEcho rejection={resolvedRejection} />
            {showSourceSection ? sourceFields : null}

            <InvoiceRequestDialogContext
                isLoading={accounts.isFetching || amounts.isFetching}
                isError={accounts.isError || amounts.isError}
                onRetry={retryContextQueries}
                amounts={amounts.data}
                archive={archive}
            />

            <div className="space-y-2.5 empty:hidden">{alertsNode}</div>

            <InvoiceLinesSection
                form={form}
                locked={locked}
                presentation={presentation}
                canImportSalesLines={canImportSalesLines}
                onOpenImport={() => setImportOpen(true)}
                createRowKey={createRowKey}
                availableAmount={amounts.data?.available_amount}
                legacyAmountHint={
                    showLegacyAmountHint ? existing?.data.amount : undefined
                }
            />

            {reasonFields}
        </>
    )

    const compactFooterContent = (
        <>
            <div className="flex min-w-0 flex-wrap items-baseline gap-x-4 gap-y-1">
                <div className="flex items-baseline gap-1.5">
                    <span className="text-xs text-muted-foreground">
                        本次申请
                    </span>
                    <span id="invoice-request-amount">
                        {linesTotal ? (
                            exceedsAvailable ? (
                                <span className="num font-semibold text-destructive">
                                    {formatCurrencyFixed(linesTotal, {
                                        maxScale: 2,
                                        minimumFractionDigits: 2,
                                    })}
                                </span>
                            ) : (
                                <MoneyValue
                                    value={linesTotal}
                                    display="exact"
                                />
                            )
                        ) : (
                            <span className="num text-muted-foreground">—</span>
                        )}
                    </span>
                </div>
                {statusMessage ? (
                    <span
                        role="status"
                        className={
                            isErrorStatus || exceedsAvailable
                                ? "text-xs text-destructive"
                                : "text-xs text-muted-foreground"
                        }
                    >
                        {statusMessage}
                    </span>
                ) : null}
            </div>

            <div className="flex shrink-0 flex-wrap items-center justify-end gap-2">
                <Button
                    id="invoice-request-form-cancel"
                    type="button"
                    variant="outline"
                    disabled={locked}
                    onClick={() => requestLeave(onCancel)}
                >
                    取消
                </Button>
                {existing && !uncertain ? (
                    <LoadingButton
                        id="invoice-request-save"
                        type="button"
                        variant="outline"
                        loading={commands.update.isPending}
                        disabled={
                            commands.update.isPending ||
                            commands.submit.isPending ||
                            saveState.isFetching ||
                            (saveUncertain && versionAdvanced)
                        }
                        onClick={handleSaveAction}
                    >
                        {saveUncertain ? "重新保存" : "保存修改"}
                    </LoadingButton>
                ) : null}
                {uncertain ? (
                    <LoadingButton
                        id="invoice-request-resolve"
                        type="button"
                        loading={commands.submit.isPending}
                        disabled={commands.submit.isPending}
                        onClick={() =>
                            pending.current && void send(pending.current)
                        }
                    >
                        核对提交结果
                    </LoadingButton>
                ) : (
                    <LoadingButton
                        id="invoice-request-submit"
                        type="submit"
                        loading={commands.submit.isPending}
                        disabled={!canSubmit}
                    >
                        提交审批
                    </LoadingButton>
                )}
            </div>
        </>
    )

    const compactForm = (
        <form
            id="invoice-request-form"
            className={
                presentation === "dialog"
                    ? "flex min-h-0 flex-1 flex-col overflow-hidden"
                    : "flex min-h-0 flex-col gap-5"
            }
            onSubmit={(event) => {
                event.preventDefault()
                intent.current = "submit"
                void form.handleSubmit()
            }}
        >
            {presentation === "dialog" ? (
                <DialogBody className="space-y-5">
                    {compactBodyContent}
                </DialogBody>
            ) : (
                <div className="space-y-5">{compactBodyContent}</div>
            )}

            {presentation === "dialog" ? (
                <DialogFooter className="flex-row flex-wrap items-center justify-between gap-3 sm:justify-between">
                    {compactFooterContent}
                </DialogFooter>
            ) : (
                <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-t border-border/60 pt-3">
                    {compactFooterContent}
                </div>
            )}
        </form>
    )

    if (presentation === "inline") {
        return (
            <>
                {compactForm}
                {importDialog}
                {submitConfirmDialog}
                {discardDialog}
            </>
        )
    }

    return (
        <>
            <Dialog
                open
                onOpenChange={(open) => {
                    if (!open && !locked) requestLeave(onCancel)
                }}
            >
                <DialogContent
                    id="invoice-request-dialog"
                    closeButtonId="invoice-request-dialog-close"
                    showCloseButton={!locked}
                    size="xl"
                    scroll="body"
                >
                    <DialogHeader>
                        <DialogTitle>
                            {existing ? "修改开票申请" : "申请开票"}
                        </DialogTitle>
                        <DialogDescription>
                            {existing
                                ? "保存修改保留当前状态；重新提交需说明依据，审批通过后由财务开票。"
                                : "录入开票明细，本次申请金额将自动合计；审批通过后由财务开票。"}
                        </DialogDescription>
                    </DialogHeader>
                    {compactForm}
                </DialogContent>
            </Dialog>
            {importDialog}
            {submitConfirmDialog}
            {discardDialog}
        </>
    )
}

const InvoiceLinesSection = withForm({
    defaultValues: formDefaults,
    props: {
        locked: false,
        presentation: "inline" as "workspace" | "inline" | "dialog",
        canImportSalesLines: false,
        onOpenImport: () => {},
        createRowKey: (() => "item-0") as () => string,
        availableAmount: undefined as string | null | undefined,
        legacyAmountHint: undefined as string | undefined,
    },
    render: function InvoiceLinesSection({
        form,
        locked,
        presentation,
        canImportSalesLines,
        onOpenImport,
        createRowKey,
        availableAmount,
        legacyAmountHint,
    }) {
        return (
            <form.AppField name="lines" mode="array">
                {(array) => {
                    const atLimit =
                        array.state.value.length >= INVOICE_REQUEST_LINE_LIMIT
                    const addRow = () => {
                        if (locked || atLimit) return
                        void array.pushValue({
                            rowKey: createRowKey(),
                            content: "",
                            quantity: "",
                            unit_price: "",
                        })
                    }

                    const badge = (
                        <div className="inline-flex items-center gap-1">
                            <span className="num rounded-md bg-muted px-2 py-0.5 text-xs text-muted-foreground">
                                {array.state.value.length} /{" "}
                                {INVOICE_REQUEST_LINE_LIMIT} 行
                            </span>
                            <DetailHint
                                id="invoice-request-lines-hint"
                                label="开票明细计算规则"
                                icon={
                                    <CircleHelpIcon
                                        className="size-3.5"
                                        aria-hidden="true"
                                    />
                                }
                            >
                                每行按数量 ×
                                含税单价舍入到分（四舍六入五成双）。本次申请金额将由明细自动合计，且不可超过本单可申请额度。
                            </DetailHint>
                        </div>
                    )

                    const actions = (
                        <>
                            {canImportSalesLines ? (
                                <Button
                                    id="invoice-request-import-sales-lines"
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    className={
                                        detailWorkspaceStyles.subtleButton
                                    }
                                    disabled={locked}
                                    onClick={onOpenImport}
                                >
                                    <FileDownIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                    从销售单带入
                                </Button>
                            ) : null}
                            <Button
                                id="invoice-request-add-line"
                                type="button"
                                variant="ghost"
                                size="sm"
                                className={detailWorkspaceStyles.subtleButton}
                                disabled={locked || atLimit}
                                onClick={addRow}
                            >
                                <PlusIcon
                                    data-icon="inline-start"
                                    aria-hidden="true"
                                />
                                添加明细
                            </Button>
                        </>
                    )

                    const tableBody = (
                        <fieldset
                            disabled={locked}
                            className="min-w-0 space-y-3"
                        >
                            {legacyAmountHint ? (
                                <Alert variant="info">
                                    <InfoIcon aria-hidden="true" />
                                    <AlertDescription>
                                        原申请金额为{" "}
                                        <MoneyValue
                                            value={legacyAmountHint}
                                            display="exact"
                                        />
                                        ，请补充开票明细的数量与含税单价；本次申请金额将按明细合计自动更新。
                                    </AlertDescription>
                                </Alert>
                            ) : null}

                            {atLimit ? (
                                <FieldDescription>
                                    {lineLimitMessage}
                                </FieldDescription>
                            ) : null}

                            <DetailInlineTable className="overflow-x-auto">
                                <Table className="min-w-[600px]">
                                    <TableHeader>
                                        <TableRow>
                                            <TableHead className="w-9">
                                                #
                                            </TableHead>
                                            <TableHead>
                                                开票内容
                                                <RequiredMark />
                                            </TableHead>
                                            <TableHead className="w-28 text-right">
                                                数量
                                                <RequiredMark />
                                            </TableHead>
                                            <TableHead className="w-36 text-right">
                                                含税单价（元）
                                                <RequiredMark />
                                            </TableHead>
                                            <TableHead className="w-32 text-right">
                                                行金额
                                            </TableHead>
                                            <TableHead className="w-10">
                                                <span className="sr-only">
                                                    删除
                                                </span>
                                            </TableHead>
                                        </TableRow>
                                    </TableHeader>
                                    <TableBody>
                                        {array.state.value.map(
                                            (line, index) => {
                                                const prefix = `invoice-request-line-${line.rowKey}`
                                                return (
                                                    <TableRow
                                                        key={line.rowKey}
                                                        className="align-top"
                                                    >
                                                        <TableCell className="num pt-3 text-muted-foreground">
                                                            {index + 1}
                                                        </TableCell>
                                                        <TableCell className="min-w-52">
                                                            <form.AppField
                                                                name={`lines[${index}].content`}
                                                            >
                                                                {(field) => (
                                                                    <field.TextareaField
                                                                        id={`${prefix}-content`}
                                                                        label="开票内容"
                                                                        hideLabel
                                                                        rows={1}
                                                                        maxLength={
                                                                            INVOICE_CONTENT_LIMIT
                                                                        }
                                                                        textareaClassName="min-h-control py-1.5"
                                                                        placeholder="如：货物名称、规格或服务项目"
                                                                        required
                                                                    />
                                                                )}
                                                            </form.AppField>
                                                        </TableCell>
                                                        <TableCell>
                                                            <form.AppField
                                                                name={`lines[${index}].quantity`}
                                                            >
                                                                {(field) => (
                                                                    <field.TextField
                                                                        id={`${prefix}-quantity`}
                                                                        label="数量"
                                                                        hideLabel
                                                                        inputMode="decimal"
                                                                        inputClassName="num text-right"
                                                                        placeholder="0"
                                                                        required
                                                                    />
                                                                )}
                                                            </form.AppField>
                                                        </TableCell>
                                                        <TableCell>
                                                            <form.AppField
                                                                name={`lines[${index}].unit_price`}
                                                            >
                                                                {(field) => (
                                                                    <field.TextField
                                                                        id={`${prefix}-unit-price`}
                                                                        label="含税单价（元）"
                                                                        hideLabel
                                                                        inputMode="decimal"
                                                                        inputClassName="num text-right"
                                                                        placeholder="0.00"
                                                                        required
                                                                    />
                                                                )}
                                                            </form.AppField>
                                                        </TableCell>
                                                        <TableCell className="pt-3 text-right">
                                                            <form.Subscribe
                                                                selector={(
                                                                    state,
                                                                ) => {
                                                                    const value =
                                                                        state
                                                                            .values
                                                                            .lines[
                                                                            index
                                                                        ]
                                                                    return value
                                                                        ? invoiceLineAmount(
                                                                              value,
                                                                          )
                                                                        : null
                                                                }}
                                                            >
                                                                {(
                                                                    lineAmount,
                                                                ) =>
                                                                    lineAmount ===
                                                                    null ? (
                                                                        <span className="num text-muted-foreground">
                                                                            —
                                                                        </span>
                                                                    ) : (
                                                                        <MoneyValue
                                                                            value={
                                                                                lineAmount
                                                                            }
                                                                            display="exact"
                                                                        />
                                                                    )
                                                                }
                                                            </form.Subscribe>
                                                        </TableCell>
                                                        <TableCell className="pt-2 text-right">
                                                            <Button
                                                                id={`${prefix}-remove`}
                                                                type="button"
                                                                size="icon-sm"
                                                                variant="ghost"
                                                                className="text-muted-foreground hover:bg-destructive-soft/75 hover:text-destructive-soft-foreground"
                                                                aria-label={`删除第 ${index + 1} 行`}
                                                                disabled={
                                                                    locked ||
                                                                    array.state
                                                                        .value
                                                                        .length <=
                                                                        1
                                                                }
                                                                onClick={() =>
                                                                    void array.removeValue(
                                                                        index,
                                                                    )
                                                                }
                                                            >
                                                                <Trash2Icon
                                                                    className="size-3.5"
                                                                    aria-hidden="true"
                                                                />
                                                            </Button>
                                                        </TableCell>
                                                    </TableRow>
                                                )
                                            },
                                        )}
                                        {!atLimit ? (
                                            <TableRow className="border-b-0 hover:bg-transparent">
                                                <TableCell
                                                    colSpan={6}
                                                    className="py-1.5"
                                                >
                                                    <Button
                                                        id="invoice-request-add-line-bottom"
                                                        type="button"
                                                        variant="ghost"
                                                        size="xs"
                                                        className="text-muted-foreground hover:text-foreground"
                                                        disabled={locked}
                                                        onClick={addRow}
                                                    >
                                                        <PlusIcon
                                                            data-icon="inline-start"
                                                            aria-hidden="true"
                                                        />
                                                        添加明细行
                                                    </Button>
                                                </TableCell>
                                            </TableRow>
                                        ) : null}
                                    </TableBody>
                                    <TableFooter>
                                        <TableRow>
                                            <TableCell colSpan={4}>
                                                <form.Subscribe
                                                    selector={(state) =>
                                                        invoiceLinesTotal(
                                                            state.values.lines,
                                                        )
                                                    }
                                                >
                                                    {(total) => {
                                                        const readout = total
                                                            ? describeMoney(
                                                                  total,
                                                              )?.readout
                                                            : null
                                                        return (
                                                            <div className="flex flex-wrap items-baseline gap-x-3 gap-y-0.5">
                                                                <span className="font-medium text-foreground">
                                                                    明细合计（本次申请金额）
                                                                </span>
                                                                {readout ? (
                                                                    <span className="text-xs font-normal text-muted-foreground">
                                                                        {
                                                                            readout
                                                                        }
                                                                    </span>
                                                                ) : null}
                                                            </div>
                                                        )
                                                    }}
                                                </form.Subscribe>
                                            </TableCell>
                                            <TableCell className="text-right font-semibold">
                                                <form.Subscribe
                                                    selector={(state) =>
                                                        invoiceLinesTotal(
                                                            state.values.lines,
                                                        )
                                                    }
                                                >
                                                    {(total) => {
                                                        if (total === null) {
                                                            return (
                                                                <span className="num text-muted-foreground">
                                                                    —
                                                                </span>
                                                            )
                                                        }
                                                        if (
                                                            invoiceExceedsAvailable(
                                                                total,
                                                                availableAmount,
                                                            )
                                                        ) {
                                                            return (
                                                                <span className="num font-semibold text-destructive">
                                                                    {formatCurrencyFixed(
                                                                        total,
                                                                        {
                                                                            maxScale: 2,
                                                                            minimumFractionDigits: 2,
                                                                        },
                                                                    )}
                                                                </span>
                                                            )
                                                        }
                                                        return (
                                                            <MoneyValue
                                                                value={total}
                                                                display="exact"
                                                            />
                                                        )
                                                    }}
                                                </form.Subscribe>
                                            </TableCell>
                                            <TableCell />
                                        </TableRow>
                                    </TableFooter>
                                </Table>
                            </DetailInlineTable>
                        </fieldset>
                    )

                    if (presentation === "workspace") {
                        return (
                            <DetailWorkspaceSection
                                title="开票明细"
                                headingId="invoice-request-lines-heading"
                                width="full"
                                badge={badge}
                                actions={actions}
                            >
                                {tableBody}
                            </DetailWorkspaceSection>
                        )
                    }

                    return (
                        <section
                            aria-labelledby="invoice-request-lines-heading"
                            className="flex min-w-0 flex-col gap-3"
                        >
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <div className="flex flex-wrap items-center gap-2">
                                    <h3
                                        id="invoice-request-lines-heading"
                                        className="font-semibold text-foreground"
                                    >
                                        开票明细
                                    </h3>
                                    {badge}
                                </div>
                                <div className="flex shrink-0 flex-wrap items-center gap-1.5">
                                    {actions}
                                </div>
                            </div>
                            {tableBody}
                        </section>
                    )
                }}
            </form.AppField>
        )
    },
})
