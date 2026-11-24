"use client"

import { useEffect, useMemo, useRef, useState, type ReactNode } from "react"
import Link from "next/link"
import { usePathname, useSearchParams } from "next/navigation"
import { useMutation } from "@tanstack/react-query"
import { useStore } from "@tanstack/react-form"
import { ArrowUpRightIcon, CircleHelpIcon } from "lucide-react"
import { z } from "zod"
import {
    BusinessStatusBadge,
    MoneyValue,
    SensitiveValue,
    WorkspaceFactGrid,
    WorkspaceFactItem,
    WorkspaceTaskFooter,
    useWorkspaceInspectorMode,
} from "@/components/business"
import {
    OptionCombobox,
    type ComboboxOption,
} from "@/components/business/option-combobox"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { FileUpload } from "@/components/ui/file-upload"
import { Input } from "@/components/ui/input"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Tooltip,
    TooltipContent,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { toast } from "@/components/ui/toast"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { uploadEvidenceFileAsset } from "@/features/file-assets/api/file-assets"
import { RequestDraftDialog } from "@/features/purchase-payment-requests/components/request-draft-dialog"
import {
    usePaymentCandidatesQuery,
    useSubmitRequestBatchMutation,
} from "@/features/purchase-payment-requests/hooks/queries"
import {
    draftDefaults,
    positive,
    requestAmountError,
} from "@/features/purchase-payment-requests/lib/draft"
import type {
    PaymentCandidate,
    RequestDraftInput,
} from "@/features/purchase-payment-requests/types"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { formatDate, unixSecondsToIsoOrEmpty } from "@/lib/datetime"
import { selectedFileKey } from "@/lib/file-selection"
import {
    clampZeroFixed,
    compareDecimal,
    divideFixed,
    minFixed,
    multiplyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import {
    FormalCommandKeyLedger,
    isCommandOutcomeUnknown,
    type FormalCommandIdentity,
} from "@/lib/formal-command"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { utf8ByteLength } from "@/lib/validation/reason-length"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { isWorkspaceFrameActive } from "@/lib/workspace-hotkeys"
import type { PurchasePaymentArrangement as PurchasePaymentArrangementView } from "../api/payment-arrangement"
import {
    usePaymentArrangement,
    useSchedulePaymentFollowUp,
} from "../hooks/use-payment-arrangement"
import {
    purchasePaymentRequestStatusLabel,
    purchasePaymentRequestStatusTone,
} from "../lib/purchase-payment-request-display"
import { PurchasePaymentFollowUp } from "./purchase-payment-follow-up"

const titles = {
    inactive: "尚未形成付款安排",
    available: "采购付款待安排",
    blocked: "采购付款暂时受阻",
    pending: "付款申请已提交",
    settled: "采购应付已结清",
}

const cents = { maxScale: 2, outputScale: 2 } as const
const maximumNoteCharacters = 2000
const maximumNoteBytes = maximumNoteCharacters * 4
const editorControlClass =
    "h-control-sm w-full min-w-0 rounded-md border border-transparent bg-surface-sunken px-2.5 text-body-compact shadow-none outline-none hover:bg-card focus-visible:bg-card focus-visible:ring-2 focus-visible:ring-ring aria-invalid:ring-2 aria-invalid:ring-destructive/50"
const sunkenInputClass =
    "border-transparent bg-surface-sunken shadow-none hover:bg-card focus-visible:bg-card"

const draftFields = z.object({
    amount: z.string(),
    date: z.string(),
    method: z.enum(["bank_transfer", "supplier_deposit"]),
    bank: z.string(),
    deposit: z.string(),
    description: z.string(),
})

type DraftFields = z.infer<typeof draftFields>
type IdempotencySlot = FormalCommandIdentity<RequestDraftInput>

const emptyDraft: DraftFields = {
    amount: "",
    date: "",
    method: "bank_transfer",
    bank: "",
    deposit: "",
    description: "",
}

function readDraft(key: string): DraftFields | undefined {
    try {
        const saved = draftFields.safeParse(
            JSON.parse(window.localStorage.getItem(key) ?? "null"),
        )
        return saved.success ? saved.data : undefined
    } catch {
        return undefined
    }
}

function paymentDraftSchema(
    candidate: PaymentCandidate | undefined,
    limit: string,
) {
    return draftFields
        .extend({ files: z.array(z.custom<File>()) })
        .superRefine((values, context) => {
            const issues = {
                amount: requestAmountError(values.amount, limit),
                date: values.date ? undefined : "请选择希望付款日",
                bank:
                    values.method === "bank_transfer" &&
                    !candidate?.bank_accounts.some(
                        (account) => account.bank_account_id === values.bank,
                    )
                        ? "请选择有效收款账户"
                        : undefined,
                deposit:
                    values.method === "supplier_deposit" &&
                    !candidate?.deposit_accounts?.some(
                        (account) => account.id === values.deposit,
                    )
                        ? "请选择有效预存款账户"
                        : undefined,
                description: !values.description.trim()
                    ? "请填写申请说明"
                    : Array.from(values.description.trim()).length >
                            maximumNoteCharacters ||
                        utf8ByteLength(values.description) > maximumNoteBytes
                      ? "申请说明最多 2000 字"
                      : undefined,
            }
            for (const [name, message] of Object.entries(issues)) {
                if (message)
                    context.addIssue({ code: "custom", path: [name], message })
            }
        })
}

type ArrangementProps = {
    purchaseOrderId: string
    purchaseOrderNo?: string
    costMasked?: boolean
    workItemId?: string
    onTaskCompleted?: (id: string) => void
    idPrefix?: string
    identityInHeader?: boolean
    taskBlocker?: string
    /** Keep the editor and pending command mounted when the workbench changes its layout. */
    renderSurface?: (body: ReactNode) => ReactNode
}

export function PurchasePaymentArrangement(props: ArrangementProps) {
    const idPrefix =
        props.idPrefix ??
        `purchase-payment-arrangement-${toAutomationIdSegment(props.workItemId ?? props.purchaseOrderId)}`

    return (
        <PaymentArrangementSurface
            key={`${props.purchaseOrderId}:${props.workItemId ?? ""}`}
            {...props}
            idPrefix={idPrefix}
        />
    )
}

function PaymentArrangementSurface({
    purchaseOrderId,
    purchaseOrderNo,
    costMasked = false,
    workItemId,
    onTaskCompleted,
    idPrefix,
    identityInHeader = false,
    taskBlocker,
    renderSurface = (body) => body,
}: ArrangementProps & { idPrefix: string }) {
    const query = usePaymentArrangement(purchaseOrderId)
    const profile = useAccountProfileQuery()
    const permissions = profile.data?.permissions
    const arrangement = query.data
    const sameTask = !workItemId || arrangement?.task_id === workItemId
    const canApply =
        hasPermission(permissions, "purchase_payment_request:create") &&
        hasPermission(permissions, "purchase_payment_request:submit")
    const canRequest =
        canApply &&
        sameTask &&
        !taskBlocker &&
        arrangement?.state === "available" &&
        !arrangement.payment_blocked
    const canStart = canRequest && !costMasked
    const canOpenMaskedRequest = canRequest && costMasked && !workItemId
    const candidates = usePaymentCandidatesQuery(canStart)
    const candidate = candidates.data?.find(
        (row) => row.purchase_order_id === purchaseOrderId,
    )
    const available = arrangement?.available_amount ?? "0"
    const limit = candidate
        ? minFixed(available, candidate.available_amount, cents)
        : available
    const balanceChanged = Boolean(
        canStart &&
        candidate &&
        compareDecimal(available, candidate.available_amount, 2) !== 0,
    )
    const unavailable =
        query.isError || (canStart && candidates.isError) || balanceChanged
    const validation = paymentDraftSchema(candidate, limit)
    const submit = useSubmitRequestBatchMutation()
    const upload = useMutation({ mutationFn: uploadEvidenceFileAsset })
    const ledger = useRef(new FormalCommandKeyLedger()).current
    const submitting = useRef(false)
    const uploaded = useRef(new Map<File, string>())
    const notified = useRef(false)
    const inspectorMode = useWorkspaceInspectorMode()
    const [busy, setBusy] = useState(false)
    const [unknown, setUnknown] = useState<IdempotencySlot>()
    const [error, setError] = useState<unknown>()
    const [showIssues, setShowIssues] = useState(false)
    const [followUpOpen, setFollowUpOpen] = useState(false)
    const [maskedRequestOpen, setMaskedRequestOpen] = useState(false)
    const [highlightedField, setHighlightedField] = useState<
        keyof DraftFields | null
    >(null)
    const highlightTimer = useRef<ReturnType<typeof setTimeout>>(null)
    useEffect(
        () => () => {
            if (highlightTimer.current) clearTimeout(highlightTimer.current)
        },
        [],
    )
    useWorkspaceDirtyState({ dirty: Boolean(unknown), busy })

    const pathname = usePathname()
    const searchParams = useSearchParams()
    const returnTo = useMemo(() => {
        const params = new URLSearchParams(searchParams.toString())
        params.delete("returnTo")
        return `${pathname}${params.size ? `?${params.toString()}` : ""}`
    }, [pathname, searchParams])
    const allRequestsParams = new URLSearchParams({
        purchase_order_id: purchaseOrderId,
    })
    if (purchaseOrderNo)
        allRequestsParams.set("purchase_order_label", purchaseOrderNo)

    const draftKey = profile.data
        ? `erp.purchase-payment-arrangement.v1:${encodeURIComponent(profile.data.userid)}:${encodeURIComponent(purchaseOrderId)}`
        : undefined
    const [initializedDraft, setInitializedDraft] = useState<string>()
    const draftIdentity =
        draftKey && candidate
            ? `${draftKey}:${candidate.payable_account_id}`
            : undefined
    const editable = Boolean(
        canStart && candidate && draftIdentity === initializedDraft,
    )

    async function runSubmission(prepare: () => Promise<IdempotencySlot>) {
        if (submitting.current) return
        submitting.current = true
        setBusy(true)
        setError(undefined)
        let command: IdempotencySlot | undefined
        try {
            command = await prepare()
            await submit.mutateAsync({
                idempotency_key: command.idempotencyKey,
                requests: [command.payload],
            })
        } catch (failure) {
            if (command && isCommandOutcomeUnknown(failure)) {
                ledger.settle("submit", "unknown")
                setUnknown(command)
            } else {
                ledger.settle("submit", "failed")
                setUnknown(undefined)
            }
            setError(failure)
            submitting.current = false
            setBusy(false)
            return
        }

        ledger.settle("submit", "succeeded")
        setUnknown(undefined)
        setInitializedDraft(undefined)
        if (draftKey) {
            try {
                window.localStorage.removeItem(draftKey)
            } catch {
                // 本地存储受限时，服务端提交仍已完成。
            }
        }
        uploaded.current.clear()
        form.reset()
        setShowIssues(false)
        toast.add({ title: "付款申请已提交", type: "success" })
        try {
            await query.refetch()
        } finally {
            submitting.current = false
            setBusy(false)
        }
    }

    const form = useAppForm({
        defaultValues: { ...emptyDraft, files: [] as File[] },
        validators: { onSubmit: validation },
        onSubmit: async ({ value }) => {
            if (!editable || !candidate || unknown || unavailable) return
            await runSubmission(async () => {
                const attachmentIds = await Promise.all(
                    value.files.map(async (file) => {
                        const saved = uploaded.current.get(file)
                        if (saved) return saved
                        const asset = await upload.mutateAsync(file)
                        uploaded.current.set(file, asset.id)
                        return asset.id
                    }),
                )
                const account = candidate.bank_accounts.find(
                    (row) => row.bank_account_id === value.bank,
                )
                const payload: RequestDraftInput = {
                    supplier_id: candidate.supplier_id,
                    execution_method:
                        value.method === "supplier_deposit"
                            ? {
                                  type: "supplier_deposit",
                                  account_id: value.deposit,
                              }
                            : { type: "bank_transfer" },
                    bank_account_id:
                        value.method === "bank_transfer"
                            ? account?.bank_account_id
                            : undefined,
                    bank_account_version:
                        value.method === "bank_transfer"
                            ? account?.version
                            : undefined,
                    desired_payment_date: value.date,
                    description: value.description.trim(),
                    attachment_ids: attachmentIds,
                    lines: [
                        {
                            payable_account_id: candidate.payable_account_id,
                            amount: value.amount.trim(),
                        },
                    ],
                }
                return ledger.acquire("submit", "ppr-batch", payload)
            })
        },
    })
    const values = useStore(form.store, (state) => state.values)
    const { files, ...localDraft } = values
    const serializedDraft = JSON.stringify(localDraft)
    useEffect(() => {
        if (
            !canStart ||
            !candidate ||
            !draftKey ||
            !draftIdentity ||
            initializedDraft === draftIdentity ||
            busy ||
            unknown
        )
            return
        const defaults = draftDefaults([candidate], purchaseOrderId)
        const group = defaults.groups[0]
        form.reset({
            ...emptyDraft,
            amount: defaults.amounts[candidate.payable_account_id] ?? "",
            date: group?.date ?? "",
            bank: group?.account ?? "",
            description: group?.description ?? "",
            ...readDraft(draftKey),
            files: [],
        })
        setInitializedDraft(draftIdentity)
    }, [
        busy,
        candidate,
        canStart,
        draftIdentity,
        draftKey,
        form,
        initializedDraft,
        purchaseOrderId,
        unknown,
    ])

    useEffect(() => {
        if (!editable || !draftKey || unknown) return
        try {
            window.localStorage.setItem(draftKey, serializedDraft)
        } catch {
            // 无法写入本地存储时仍可在当前页面继续填写。
        }
    }, [draftKey, editable, serializedDraft, unknown])

    const refetchArrangement = query.refetch
    const refetchCandidates = candidates.refetch
    useEffect(() => {
        if (!editable || !draftKey || busy || unknown) return
        const syncDraft = (event: StorageEvent) => {
            if (
                event.key !== draftKey ||
                event.storageArea !== window.localStorage
            )
                return
            const saved = readDraft(draftKey)
            if (saved) {
                form.reset({ ...saved, files: form.state.values.files })
                return
            }
            void refetchArrangement()
            void refetchCandidates()
        }
        window.addEventListener("storage", syncDraft)
        return () => window.removeEventListener("storage", syncDraft)
    }, [
        busy,
        draftKey,
        editable,
        form,
        refetchArrangement,
        refetchCandidates,
        unknown,
    ])

    useEffect(() => {
        if (
            !workItemId ||
            !onTaskCompleted ||
            notified.current ||
            busy ||
            unknown ||
            !arrangement ||
            sameTask ||
            query.isFetching ||
            query.isError ||
            (arrangement.state !== "pending" &&
                arrangement.state !== "settled" &&
                arrangement.state !== "inactive")
        )
            return
        notified.current = true
        onTaskCompleted(workItemId)
    }, [
        arrangement,
        busy,
        onTaskCompleted,
        query.isError,
        query.isFetching,
        sameTask,
        unknown,
        workItemId,
    ])

    const refresh = () => {
        setError(undefined)
        void query.refetch()
        if (canStart) void candidates.refetch()
    }
    if (!arrangement)
        return renderSurface(
            query.isError ? (
                <div
                    role="alert"
                    className="flex flex-wrap items-center gap-2 py-4 text-body-compact"
                >
                    {getErrorMessage(query.error, "付款安排读取失败")}
                    <Button
                        id={`${idPrefix}-retry`}
                        variant="link"
                        size="sm"
                        onClick={refresh}
                    >
                        重新读取
                    </Button>
                </div>
            ) : (
                <p role="status" className="py-4 text-xs text-muted-foreground">
                    正在读取付款安排…
                </p>
            ),
        )
    if (arrangement.state === "inactive" && !unknown && !maskedRequestOpen)
        return renderSurface(null)

    const plannedInput =
        unknown?.payload.lines[0]?.amount ?? (editable ? values.amount : "")
    const planned = positive(plannedInput) ? plannedInput.trim() : "0"
    const difference = subtractFixed(available, planned, cents)
    const balance = compareDecimal(difference, "0", 2)
    const excess = clampZeroFixed(
        subtractFixed(planned, available, cents),
        cents,
    )
    const covered = sumFixed(
        [arrangement.reserved_amount, arrangement.paid_amount, planned],
        cents,
    )
    const total = sumFixed(
        [arrangement.reserved_amount, arrangement.paid_amount, available],
        cents,
    )
    const coverage = positive(total)
        ? multiplyFixed(
              divideFixed(minFixed(covered, total, cents), total, {
                  numeratorMaxScale: 2,
                  denominatorMaxScale: 2,
                  outputScale: 4,
              }),
              "100",
              { leftMaxScale: 4, rightMaxScale: 0, outputScale: 2 },
          )
        : "0"
    const money = (value: string) =>
        costMasked ? (
            "•••"
        ) : (
            <MoneyValue
                value={value}
                display="exact"
                inheritColor
                className="max-w-full [&_[data-slot=money-value-number]]:wrap-anywhere [&_[data-slot=money-value-number]]:whitespace-normal"
            />
        )
    const balanceState = costMasked ? (
        "余额受限"
    ) : balance === 0 ? (
        "已配平"
    ) : (
        <>
            {balance > 0 ? "尚余" : "超出"}{" "}
            {money(balance > 0 ? difference : excess)}
        </>
    )
    const balanceTone =
        balance === 0
            ? "text-success-soft-foreground"
            : balance > 0
              ? "text-warning-soft-foreground"
              : "text-destructive"
    const coverageTone =
        balance < 0
            ? "bg-destructive"
            : balance === 0
              ? "bg-success"
              : "bg-warning"
    const prepaymentGap = arrangement.prepayment_gap
    const followUp =
        arrangement.follow_up_at == null
            ? null
            : formatDate(unixSecondsToIsoOrEmpty(arrangement.follow_up_at), {
                  timeZone: "Asia/Shanghai",
              })
    const canSchedule =
        canApply &&
        sameTask &&
        arrangement.can_schedule &&
        (arrangement.state === "available" || arrangement.state === "blocked")
    const orderHref = `/procurement/orders/${encodeURIComponent(purchaseOrderId)}`
    const canList = hasPermission(permissions, "purchase_payment_request:list")
    const canOpen = hasPermission(
        permissions,
        "purchase_payment_request:detail",
    )
    const canSeeAccount = hasPermission(
        permissions,
        "party_bank_account:reveal",
    )

    const result = validation.safeParse(values)
    const issues = editable && !result.success ? result.error.issues : []
    const issueFor = (name: keyof DraftFields) =>
        issues.find((issue) => issue.path[0] === name)?.message
    const controlId = (name: keyof DraftFields) => `${idPrefix}-${name}`
    const locate = (name: keyof DraftFields, highlight = false) => {
        if (highlight) {
            if (highlightTimer.current) clearTimeout(highlightTimer.current)
            setHighlightedField(name)
            highlightTimer.current = setTimeout(
                () => setHighlightedField(null),
                2200,
            )
        }
        const control = document.getElementById(controlId(name))
        const behavior = window.matchMedia("(prefers-reduced-motion: reduce)")
            .matches
            ? "auto"
            : "smooth"
        control?.scrollIntoView({ block: "center", behavior })
        control?.focus({ preventScroll: true })
    }
    const locateFirstIssue = () => {
        const issue = issues[0]?.path[0] as keyof DraftFields | undefined
        if (!issue) return
        setShowIssues(true)
        locate(issue, true)
    }
    const fill = () => {
        form.setFieldValue("amount", limit)
        locate("amount")
    }
    const amountAction = balance < 0 ? "调整至上限" : "填剩余金额"
    const blockedMessage = arrangement.payment_blocked
        ? "销售变更尚未处理，当前采购暂停申请和付款。"
        : taskBlocker ||
          (workItemId && !sameTask && arrangement.state === "available"
              ? "当前任务已变更，请刷新任务。"
              : undefined)
    const summary = (
        <dl
            aria-label="本次付款安排合计"
            className="flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-0.5 text-xs"
        >
            <div className="flex items-baseline gap-1">
                <dt className="text-muted-foreground">待安排</dt>
                <dd>{money(available)}</dd>
            </div>
            {(editable || unknown) && (
                <>
                    <div className="flex items-baseline gap-1">
                        <dt className="text-muted-foreground">本次</dt>
                        <dd>{money(planned)}</dd>
                    </div>
                    <div className={cn("font-medium", balanceTone)}>
                        <dt className="sr-only">配平状态</dt>
                        <dd>{balanceState}</dd>
                    </div>
                </>
            )}
        </dl>
    )
    const actions = (
        <>
            {issues.length > 0 && !unknown && !busy && (
                <Button
                    id={`${idPrefix}-locate`}
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={locateFirstIssue}
                >
                    <span className="num">{issues.length}</span> 项待完善 · 定位
                </Button>
            )}
            {canSchedule && !unknown && (
                <Button
                    id={`${idPrefix}-schedule`}
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={busy}
                    onClick={() => setFollowUpOpen(true)}
                >
                    {followUp ? "调整跟进日期" : "安排跟进"}
                </Button>
            )}
            {unknown ? (
                <Button
                    id={`${idPrefix}-recover`}
                    type="button"
                    size="sm"
                    variant="default"
                    disabled={busy}
                    onClick={() => void runSubmission(async () => unknown)}
                >
                    {busy ? "正在核对…" : "核对提交结果"}
                </Button>
            ) : editable ? (
                <Button
                    id={`${idPrefix}-apply`}
                    type="submit"
                    form={`${idPrefix}-form`}
                    size="sm"
                    variant="default"
                    disabled={busy || unavailable}
                >
                    {busy ? "正在提交…" : "申请采购付款"}
                </Button>
            ) : canOpenMaskedRequest ? (
                <Button
                    id={`${idPrefix}-apply`}
                    type="button"
                    size="sm"
                    variant="default"
                    disabled={query.isError}
                    onClick={() => setMaskedRequestOpen(true)}
                >
                    申请采购付款
                </Button>
            ) : null}
        </>
    )
    const fieldError = (name: keyof DraftFields) =>
        showIssues || name === "amount" ? issueFor(name) : undefined
    const fieldMessage = (name: keyof DraftFields) =>
        fieldError(name) ? (
            <p
                id={`${controlId(name)}-error`}
                className="text-xs text-destructive"
            >
                {fieldError(name)}
            </p>
        ) : null
    const fieldA11y = (name: keyof DraftFields) => ({
        "aria-invalid": Boolean(fieldError(name)) || undefined,
        "aria-describedby": fieldError(name)
            ? `${controlId(name)}-error`
            : undefined,
    })
    const bankAccount = candidate?.bank_accounts.find(
        (account) => account.bank_account_id === values.bank,
    )

    const depositOptions = (candidate?.deposit_accounts ?? []).map(
        (account) => ({
            value: account.id,
            label: account.label,
        }),
    )
    const bankOptions = (candidate?.bank_accounts ?? []).map(
        (account, index) => ({
            value: account.bank_account_id,
            label: `${index + 1}. ${account.account_name} · ${account.bank_name}${
                canSeeAccount && account.account_number_masked
                    ? ` · ${account.account_number_masked}`
                    : ""
            }`,
        }),
    )
    const fieldHighlightClass = (name: keyof DraftFields) =>
        highlightedField === name &&
        "rounded-md bg-info-soft/25 p-1.5 -m-1.5 ring-1 ring-info-border motion-safe:animate-pulse"
    const renderSelection = (
        name: "method" | "bank" | "deposit",
        label: string,
        options: ComboboxOption[],
        supplement?: ReactNode,
    ) => (
        <div
            className={cn(
                "min-w-0 space-y-1 transition-colors",
                fieldHighlightClass(name),
            )}
        >
            <label htmlFor={controlId(name)} className="text-xs font-medium">
                {label}
            </label>
            <OptionCombobox
                id={controlId(name)}
                aria-label={label}
                size="sm"
                required
                retainMissingSelection={false}
                disabled={busy || Boolean(unknown)}
                allowClear={name !== "method"}
                value={values[name] || null}
                options={options}
                inputClassName={sunkenInputClass}
                onValueChange={(next) => {
                    if (name === "method") {
                        if (
                            next === "bank_transfer" ||
                            next === "supplier_deposit"
                        )
                            form.setFieldValue(name, next)
                    } else {
                        form.setFieldValue(name, next ?? "")
                    }
                }}
                {...fieldA11y(name)}
            />
            {supplement}
            {fieldMessage(name)}
        </div>
    )

    return renderSurface(
        <section
            aria-label="采购付款安排"
            className="@container/payment-arrangement min-w-0 space-y-4 bg-card py-4"
        >
            {(unknown ||
                unavailable ||
                error ||
                blockedMessage ||
                (editable && balance !== 0)) && (
                <div
                    role={
                        unknown || unavailable || error || blockedMessage
                            ? "alert"
                            : "status"
                    }
                    className={cn(
                        "flex min-w-0 flex-wrap items-center justify-between gap-x-3 gap-y-1 border-l-2 px-3 py-2 text-xs",
                        blockedMessage || unavailable || balance < 0
                            ? "border-destructive bg-destructive-soft text-destructive-soft-foreground"
                            : "border-warning bg-warning-soft text-warning-soft-foreground",
                    )}
                >
                    <span className="min-w-0 wrap-anywhere">
                        {unknown
                            ? "提交结果待确认，请核对本次提交结果。"
                            : unavailable || error
                              ? balanceChanged
                                  ? "可申请余额已变化，请刷新后重新核对金额。"
                                  : getErrorMessage(
                                        error ??
                                            query.error ??
                                            candidates.error,
                                        "读取或提交失败，请刷新后重试。",
                                    )
                              : blockedMessage || balanceState}
                    </span>
                    {!unknown && (unavailable || error) ? (
                        <Button
                            id={`${idPrefix}-refresh`}
                            type="button"
                            variant="link"
                            size="xs"
                            disabled={busy}
                            onClick={refresh}
                        >
                            刷新最新数据
                        </Button>
                    ) : !unknown && arrangement.payment_blocked ? (
                        <Button
                            id={`${idPrefix}-resolve-source`}
                            type="button"
                            variant="link"
                            size="xs"
                            render={<Link href={orderHref} />}
                        >
                            查看采购单
                            <ArrowUpRightIcon aria-hidden="true" />
                        </Button>
                    ) : !unknown &&
                      !blockedMessage &&
                      editable &&
                      balance !== 0 ? (
                        <Button
                            id={`${idPrefix}-balance`}
                            type="button"
                            variant="link"
                            size="xs"
                            disabled={busy}
                            onClick={fill}
                        >
                            {balance > 0 ? "填剩余金额" : "调整至上限"}
                        </Button>
                    ) : null}
                </div>
            )}

            <dl className="grid grid-cols-1 gap-x-5 gap-y-3 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/payment-arrangement:grid-cols-2 @[840px]/payment-arrangement:grid-cols-4">
                <div className="min-w-0 space-y-1">
                    <dt className="flex items-center gap-1 text-xs text-muted-foreground">
                        待安排付款额
                        <Tooltip>
                            <TooltipTrigger
                                id={`${idPrefix}-formula`}
                                aria-label="待安排付款额计算口径"
                                render={
                                    <Button
                                        type="button"
                                        size="icon-xs"
                                        variant="ghost"
                                    />
                                }
                            >
                                <CircleHelpIcon aria-hidden="true" />
                            </TooltipTrigger>
                            <TooltipContent>
                                待安排付款额 = 剩余应付 −
                                有效申请占用。有效已付已从应付余额扣除。
                            </TooltipContent>
                        </Tooltip>
                    </dt>
                    <dd className="num text-value-section font-semibold tracking-tight">
                        {money(available)}
                    </dd>
                </div>
                <div className="min-w-0 space-y-1">
                    <dt className="text-xs text-muted-foreground">
                        申请占用 / 有效已付
                    </dt>
                    <dd className="flex flex-wrap items-baseline gap-x-2 text-value-compact font-semibold">
                        {money(arrangement.reserved_amount)}
                        <span
                            aria-hidden="true"
                            className="text-xs font-normal text-muted-foreground"
                        >
                            /
                        </span>
                        {money(arrangement.paid_amount)}
                    </dd>
                    {!costMasked && (
                        <dd className="space-y-0.5">
                            <div
                                aria-hidden="true"
                                className="h-1.5 overflow-hidden rounded-full bg-border"
                            >
                                <div
                                    className={cn(
                                        "h-full rounded-full",
                                        coverageTone,
                                    )}
                                    style={{ width: `${coverage}%` }}
                                />
                            </div>
                            <p className="text-tiny text-muted-foreground">
                                含本次预计覆盖{" "}
                                <span className="num">{coverage}%</span>
                            </p>
                        </dd>
                    )}
                </div>
                <div className="min-w-0 space-y-1">
                    <dt className="text-xs text-muted-foreground">
                        本次安排金额
                    </dt>
                    <dd className="num text-value-compact font-semibold">
                        {money(planned)}
                    </dd>
                    <dd
                        aria-live="polite"
                        className={cn(
                            "text-xs font-medium",
                            !costMasked && balanceTone,
                        )}
                    >
                        {balanceState}
                    </dd>
                </div>
                <div className="min-w-0 space-y-1">
                    <dt className="text-xs text-muted-foreground">
                        {identityInHeader ? "先款门禁" : "付款条件"}
                    </dt>
                    {!identityInHeader && (
                        <dd className="wrap-anywhere text-body-compact font-semibold">
                            {arrangement.payment_term_label || "待核对"}
                        </dd>
                    )}
                    <dd
                        className={cn(
                            identityInHeader
                                ? "text-body-compact font-semibold"
                                : "text-xs",
                            prepaymentGap && positive(prepaymentGap)
                                ? "text-warning-soft-foreground"
                                : identityInHeader
                                  ? "text-foreground"
                                  : "text-muted-foreground",
                        )}
                    >
                        {prepaymentGap == null ? (
                            "无先款门禁"
                        ) : positive(prepaymentGap) ? (
                            <>履约前还需实付 {money(prepaymentGap)}</>
                        ) : (
                            "先款条件已满足"
                        )}
                    </dd>
                </div>
            </dl>

            {Boolean(
                (!identityInHeader && purchaseOrderNo) ||
                (candidate?.our_company_name && canStart) ||
                followUp ||
                arrangement.state !== "available",
            ) && (
                <WorkspaceFactGrid columns={3} ariaLabel="付款安排核对信息">
                    {!identityInHeader && purchaseOrderNo ? (
                        <WorkspaceFactItem
                            label="采购单"
                            value={purchaseOrderNo}
                            numeric
                            selectable
                        />
                    ) : null}
                    {candidate?.our_company_name && canStart ? (
                        <WorkspaceFactItem
                            label="付款主体"
                            value={candidate.our_company_name}
                        />
                    ) : null}
                    {followUp ? (
                        <WorkspaceFactItem
                            label="下次跟进"
                            value={followUp}
                            numeric
                        />
                    ) : null}
                    {arrangement.state !== "available" ? (
                        <WorkspaceFactItem
                            label="安排状态"
                            value={titles[arrangement.state]}
                        />
                    ) : null}
                </WorkspaceFactGrid>
            )}

            {canStart && !candidate && !candidates.isError && (
                <p role="status" className="text-xs text-muted-foreground">
                    {candidates.isPending
                        ? "正在读取付款申请资料…"
                        : "当前采购单暂无可申请的应付，请刷新核对。"}
                    {!candidates.isPending && (
                        <Button
                            id={`${idPrefix}-refresh-candidate`}
                            variant="link"
                            size="xs"
                            onClick={refresh}
                        >
                            刷新最新数据
                        </Button>
                    )}
                </p>
            )}
            {editable && candidate && (
                <form
                    id={`${idPrefix}-form`}
                    noValidate
                    onSubmit={(event) => {
                        event.preventDefault()
                        if (busy || unknown || unavailable) return
                        if (issues.length) {
                            locateFirstIssue()
                            return
                        }
                        void form.handleSubmit()
                    }}
                >
                    <fieldset
                        disabled={busy || Boolean(unknown)}
                        className="min-w-0 space-y-3 border-t border-border/60 pt-3 disabled:opacity-70"
                    >
                        <legend className="sr-only">本次付款安排</legend>
                        <div className="grid min-w-0 gap-x-4 gap-y-3 @[480px]/payment-arrangement:grid-cols-2">
                            <form.Field name="amount">
                                {(field) => (
                                    <div
                                        className={cn(
                                            "min-w-0 space-y-1 transition-colors",
                                            fieldHighlightClass("amount"),
                                        )}
                                    >
                                        <label
                                            htmlFor={controlId("amount")}
                                            className="text-xs font-medium"
                                        >
                                            本次申请金额
                                        </label>
                                        <div className="flex min-w-0 gap-1.5">
                                            <Input
                                                id={controlId("amount")}
                                                inputMode="decimal"
                                                value={field.state.value}
                                                onChange={(event) =>
                                                    field.handleChange(
                                                        event.target.value,
                                                    )
                                                }
                                                onBlur={field.handleBlur}
                                                onKeyDown={(event) => {
                                                    if (
                                                        event.key === "Enter" &&
                                                        (event.ctrlKey ||
                                                            event.metaKey) &&
                                                        !event.nativeEvent
                                                            .isComposing &&
                                                        isWorkspaceFrameActive()
                                                    ) {
                                                        event.preventDefault()
                                                        event.currentTarget.form?.requestSubmit()
                                                    }
                                                }}
                                                className={cn(
                                                    editorControlClass,
                                                    "num",
                                                )}
                                                {...fieldA11y("amount")}
                                            />
                                            {(balance !== 0 ||
                                                issueFor("amount")) && (
                                                <Button
                                                    id={`${idPrefix}-fill`}
                                                    type="button"
                                                    size="xs"
                                                    variant="ghost"
                                                    disabled={unavailable}
                                                    onClick={fill}
                                                >
                                                    {amountAction}
                                                </Button>
                                            )}
                                        </div>
                                        {fieldMessage("amount")}
                                    </div>
                                )}
                            </form.Field>
                            <form.AppField name="date">
                                {(field) => (
                                    <div
                                        className={cn(
                                            "min-w-0 space-y-1 transition-colors",
                                            fieldHighlightClass("date"),
                                        )}
                                    >
                                        <field.DateField
                                            id={controlId("date")}
                                            label="希望付款日"
                                            required
                                            size="sm"
                                            disabled={busy || Boolean(unknown)}
                                            className="gap-1"
                                            inputClassName={sunkenInputClass}
                                        />
                                        {fieldMessage("date")}
                                    </div>
                                )}
                            </form.AppField>
                            {(values.method === "supplier_deposit" ||
                                depositOptions.length > 0) &&
                                renderSelection("method", "付款方式", [
                                    {
                                        value: "bank_transfer",
                                        label: "银行转账",
                                    },
                                    {
                                        value: "supplier_deposit",
                                        label: "供应商预存款",
                                        disabled: !depositOptions.length,
                                    },
                                ])}
                            {values.method === "supplier_deposit"
                                ? renderSelection(
                                      "deposit",
                                      "预存款账户",
                                      depositOptions,
                                  )
                                : renderSelection(
                                      "bank",
                                      "收款账户",
                                      bankOptions,
                                      canSeeAccount &&
                                          bankAccount?.account_number_masked ? (
                                          <SensitiveValue
                                              id={`${idPrefix}-account-number`}
                                              label="收款账号"
                                              maskedValue={
                                                  bankAccount.account_number_masked
                                              }
                                              className="num text-xs text-muted-foreground"
                                          />
                                      ) : null,
                                  )}
                        </div>
                        <form.Field name="description">
                            {(field) => (
                                <div
                                    className={cn(
                                        "transition-colors",
                                        fieldHighlightClass("description"),
                                    )}
                                >
                                    <ReasonTextareaField
                                        id={controlId("description")}
                                        mode="utf8-bytes"
                                        max={maximumNoteBytes}
                                        label="付款说明"
                                        required
                                        rows={2}
                                        textareaClassName={sunkenInputClass}
                                        value={field.state.value}
                                        onChange={field.handleChange}
                                        onBlur={field.handleBlur}
                                        onCtrlEnterSubmit={() => {
                                            if (!isWorkspaceFrameActive())
                                                return
                                            const input =
                                                document.getElementById(
                                                    controlId("description"),
                                                )
                                            if (
                                                input instanceof
                                                HTMLTextAreaElement
                                            )
                                                input.form?.requestSubmit()
                                        }}
                                        error={fieldError("description")}
                                    />
                                </div>
                            )}
                        </form.Field>
                        <FileUpload
                            idPrefix={`${idPrefix}-attachments`}
                            inputId={`${idPrefix}-attachment-input`}
                            selectId={`${idPrefix}-attachment-select`}
                            density="button"
                            label="添加附件"
                            selectLabel="添加附件"
                            description="选填 · PDF / 图片，每份不超过 5 MB"
                            multiple
                            accept="application/pdf,image/jpeg,image/png,image/webp"
                            maxFileSize={5 * 1024 * 1024}
                            selectedFiles={files}
                            disabled={busy || Boolean(unknown)}
                            onFilesSelected={(selected) => {
                                const merged = new Map(
                                    [...files, ...selected].map((file) => [
                                        selectedFileKey(file),
                                        file,
                                    ]),
                                )
                                form.setFieldValue("files", [
                                    ...merged.values(),
                                ])
                            }}
                            onFileRemove={(file) =>
                                form.setFieldValue(
                                    "files",
                                    files.filter(
                                        (item) =>
                                            selectedFileKey(item) !==
                                            selectedFileKey(file),
                                    ),
                                )
                            }
                        />
                    </fieldset>
                </form>
            )}

            {inspectorMode && canSchedule && followUpOpen && (
                <InlinePaymentFollowUp
                    key={`${arrangement.task_id}:${arrangement.task_version}`}
                    idPrefix={`${idPrefix}-inline-follow-up`}
                    purchaseOrderId={purchaseOrderId}
                    arrangement={arrangement}
                    onCancel={() => setFollowUpOpen(false)}
                    onSaved={() => setFollowUpOpen(false)}
                />
            )}

            <section
                aria-label="审核与付款进度"
                className="min-w-0 border-t border-border/60 pt-3"
            >
                <div className="flex flex-wrap items-center justify-between gap-x-3 gap-y-1">
                    <h2 className="text-section-title font-semibold">
                        审核与付款进度
                    </h2>
                    {canList && arrangement.requests.length > 0 && (
                        <Button
                            id={`${idPrefix}-requests`}
                            variant="link"
                            size="xs"
                            render={
                                <Link
                                    href={`/procurement/payment-requests?${allRequestsParams.toString()}`}
                                />
                            }
                        >
                            全部付款申请（
                            <span className="num">
                                {arrangement.requests.length}
                            </span>
                            ）
                        </Button>
                    )}
                </div>
                {arrangement.requests.length ? (
                    <ul className="divide-y divide-border/60">
                        {arrangement.requests.map((request) => (
                            <li
                                key={request.request_id}
                                className="flex min-w-0 flex-wrap items-center gap-x-4 gap-y-1 py-2 text-xs"
                            >
                                {canOpen ? (
                                    <Link
                                        id={`${idPrefix}-request-${toAutomationIdSegment(request.request_id)}`}
                                        href={`/procurement/payment-requests/${encodeURIComponent(request.request_id)}?returnTo=${encodeURIComponent(returnTo)}`}
                                        className="num font-medium wrap-anywhere text-primary underline-offset-4 hover:underline"
                                    >
                                        {request.request_no}
                                    </Link>
                                ) : (
                                    <span className="num wrap-anywhere">
                                        {request.request_no}
                                    </span>
                                )}
                                <BusinessStatusBadge
                                    context="list"
                                    label={
                                        purchasePaymentRequestStatusLabel[
                                            request.status
                                        ]
                                    }
                                    tone={
                                        purchasePaymentRequestStatusTone[
                                            request.status
                                        ]
                                    }
                                />
                                <span>
                                    本单申请 {money(request.requested_amount)}
                                </span>
                                <span>
                                    本单已付 {money(request.paid_amount)}
                                </span>
                                <span>
                                    希望付款日{" "}
                                    <span className="num">
                                        {request.desired_payment_date}
                                    </span>
                                </span>
                            </li>
                        ))}
                    </ul>
                ) : (
                    <p className="py-2 text-xs text-muted-foreground">
                        暂无付款申请
                    </p>
                )}
            </section>

            <WorkspaceTaskFooter
                inline={!workItemId}
                summary={summary}
                fallback={
                    <div
                        className={cn(
                            "flex w-full min-w-0 flex-wrap items-center justify-between gap-x-3 gap-y-2",
                            !workItemId &&
                                "min-h-12 border-t border-border/60 pt-2",
                        )}
                    >
                        {summary}
                        <div className="ml-auto flex min-w-0 flex-wrap items-center justify-end gap-2">
                            {actions}
                        </div>
                    </div>
                }
            >
                {actions}
            </WorkspaceTaskFooter>
            {maskedRequestOpen && canApply && (
                <RequestDraftDialog
                    open
                    onOpenChange={setMaskedRequestOpen}
                    fixedPurchaseOrderId={purchaseOrderId}
                    onSubmitted={() => void query.refetch()}
                />
            )}
            {!inspectorMode && canSchedule && followUpOpen && (
                <PurchasePaymentFollowUp
                    open
                    onOpenChange={setFollowUpOpen}
                    purchaseOrderId={purchaseOrderId}
                    arrangement={arrangement}
                    onSaved={() => setFollowUpOpen(false)}
                />
            )}
        </section>,
    )
}

function InlinePaymentFollowUp({
    idPrefix,
    purchaseOrderId,
    arrangement,
    onCancel,
    onSaved,
}: {
    idPrefix: string
    purchaseOrderId: string
    arrangement: PurchasePaymentArrangementView
    onCancel: () => void
    onSaved: () => void
}) {
    const mutation = useSchedulePaymentFollowUp(purchaseOrderId)
    const command = useRef<{ payload: string; key: string } | null>(null)
    const form = useAppForm({
        defaultValues: { date: "" },
        validators: {
            onSubmit: z.object({ date: z.string().min(1, "请选择跟进日期") }),
        },
        onSubmit: async ({ value }) => {
            if (!arrangement.task_id || arrangement.task_version == null) return
            const payload = JSON.stringify([
                arrangement.task_id,
                arrangement.task_version,
                value.date,
            ])
            if (command.current?.payload !== payload)
                command.current = { payload, key: crypto.randomUUID() }
            try {
                await mutation.mutateAsync({
                    task_id: arrangement.task_id,
                    expected_version: arrangement.task_version,
                    follow_up_date: value.date,
                    idempotency_key: command.current.key,
                })
                onSaved()
            } catch {
                /* 留在表单中展示错误，重试沿用原操作号。 */
            }
        },
    })

    return (
        <section
            aria-label="安排下次跟进"
            className="space-y-3 rounded-lg bg-surface-sunken px-4 py-3"
        >
            <div className="space-y-0.5">
                <h3 className="text-body-compact font-semibold">
                    安排下次跟进
                </h3>
                <p className="text-xs text-muted-foreground">
                    设置下次跟进采购付款安排的日期，到该日期重新提醒处理；此日期不代表已对账或已获准付款。
                </p>
            </div>
            <form
                className="space-y-3"
                onSubmit={(event) => {
                    event.preventDefault()
                    void form.handleSubmit()
                }}
            >
                <form.AppField name="date">
                    {(field) => (
                        <field.DateField
                            id={`${idPrefix}-date`}
                            label="下次跟进日期"
                            size="sm"
                            required
                            disabled={mutation.isPending}
                        />
                    )}
                </form.AppField>
                {mutation.isError && (
                    <p role="alert" className="text-xs text-destructive">
                        {getErrorMessage(
                            mutation.error,
                            "跟进安排保存失败，请重试",
                        )}
                    </p>
                )}
                <div className="flex flex-wrap justify-end gap-2">
                    <Button
                        id={`${idPrefix}-cancel`}
                        type="button"
                        variant="ghost"
                        size="sm"
                        disabled={mutation.isPending}
                        onClick={onCancel}
                    >
                        取消
                    </Button>
                    <LoadingButton
                        id={`${idPrefix}-save`}
                        type="submit"
                        variant="outline"
                        size="sm"
                        loading={mutation.isPending}
                        disabled={mutation.isPending}
                    >
                        {mutation.isPending ? "保存中…" : "保存跟进日期"}
                    </LoadingButton>
                </div>
            </form>
        </section>
    )
}
