"use client"

import {
    Fragment,
    useCallback,
    useEffect,
    useMemo,
    useRef,
    useState,
    type ReactNode,
} from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import {
    ArrowUpRightIcon,
    ChevronRightIcon,
    CircleAlertIcon,
    DownloadIcon,
    EyeIcon,
    PencilLineIcon,
    PlusIcon,
    RefreshCwIcon,
    RotateCcwIcon,
    Trash2Icon,
} from "lucide-react"
import {
    FormalActionConfirmDialog,
    MoneyValue,
    QuantityValue,
    RateValue,
    ReasonTextareaField,
    WorkspaceFactGrid,
    WorkspaceFactGroup,
    WorkspaceFactItem,
    WorkspaceReworkCommandCard,
    WorkspaceTaskFooter,
    detailWorkspaceStyles,
    type WorkspaceReworkTriageItem,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { DatePicker } from "@/components/ui/date-picker"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { FileUpload } from "@/components/ui/file-upload"
import { Input } from "@/components/ui/input"
import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import {
    Table,
    TableBody,
    TableCell,
    TableFooter,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import { toast } from "@/components/ui/toast"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { SubmissionRouteConfirmation } from "@/features/approval-workflow/components/submission-route-confirmation"
import { mapDocumentApprovalViewDto } from "@/features/approval-workflow/types"
import { uploadEvidenceFileAsset } from "@/features/file-assets/api/file-assets"
import { getErrorMessage } from "@/lib/api"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { selectedFileKey } from "@/lib/file-selection"
import {
    clampZeroFixed,
    compareDecimal,
    divideFixed,
    formatCurrencyFixed,
    minFixed,
    multiplyFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { isCommandOutcomeUnknown } from "@/lib/formal-command"
import { useRouter } from "@/lib/navigation"
import { queryKeyRoots } from "@/lib/query-key-roots"
import { fileUploadText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"
import {
    BPM_REASON_MAX_UTF8_BYTES,
    utf8ByteLength,
} from "@/lib/validation/reason-length"
import { navigateWorkspace } from "@/lib/workspace-tabs"
import {
    paymentRequestKeys,
    usePaymentCandidatesQuery,
    useRequestCommandMutation,
    useRequestDownloadMutation,
    useSaveRequestMutation,
} from "../hooks/queries"
import { requestStatusLabel, requestStatusTone } from "../lib/display"
import { positive, totalOf } from "../lib/draft"
import { fulfillmentLabel, paymentReviewBasis } from "../lib/review"
import type {
    DepositAccountOption,
    PaymentCandidate,
    PaymentExecutionMethod,
    PaymentRecipient,
    PaymentRejectionNoticeInfo,
    PaymentRequest,
    RequestCommand,
    RequestDraftInput,
    RequestLine,
} from "../types"
import { RequestReviewHistory } from "./request-review-history"

const moneyMath = { maxScale: 4, outputScale: 4, allowNegative: true } as const
const DESCRIPTION_MAX_CHARS = 2000

type ReworkLineState = {
    payableAccountId: string
    amount: string
}

type SectionKey = "account" | "lines" | "details"

type ReworkFieldDiff = {
    label: string
    before?: string
    after: string
    numeric?: boolean
}

type ReworkWorkspaceProps = {
    request: PaymentRequest
    idPrefix: string
    onSelectFile?: (fileId: string) => void
    onTaskCompleted?: () => void
    canAbandon?: boolean
    rejection?: PaymentRejectionNoticeInfo
    onRequestAbandon?: () => void
    renderAbandonAction?: (variant?: "outline" | "default") => ReactNode
}

function normalizeDate(value: string): string {
    return value.trim()
}

function initialLinesFromRequest(request: PaymentRequest): ReworkLineState[] {
    return request.lines.map((line) => ({
        payableAccountId: line.payable_account_id,
        amount: line.requested_amount,
    }))
}

function validateReworkLineAmount(
    value: string,
    available: string,
): string | undefined {
    if (!positive(value)) return "请填写大于零且最多两位小数的金额"
    if (compareDecimal(value, available, 4) > 0) {
        return `本次最多可申请 ${formatCurrencyFixed(available, {
            maxScale: 4,
            minimumFractionDigits: 2,
        })}`
    }
    return undefined
}

function sameLines(
    current: readonly ReworkLineState[],
    original: readonly RequestLine[],
): boolean {
    if (current.length !== original.length) return false
    return current.every((line, index) => {
        const orig = original[index]
        if (!orig || line.payableAccountId !== orig.payable_account_id) {
            return false
        }
        try {
            return (
                compareDecimal(line.amount.trim(), orig.requested_amount, 4) ===
                0
            )
        } catch {
            return line.amount.trim() === orig.requested_amount
        }
    })
}

function sameStringSet(a: readonly string[], b: readonly string[]): boolean {
    if (a.length !== b.length) return false
    const setB = new Set(b)
    return a.every((item) => setB.has(item))
}

function bankDisplayLabel(account: {
    bank_name?: string | null
    bank_branch_name?: string | null
}): string {
    const bankName = account.bank_name?.trim() || ""
    const branchName = account.bank_branch_name?.trim() || ""
    if (bankName && bankName !== "未记录") {
        if (branchName && !bankName.includes(branchName)) {
            return `${bankName} · ${branchName}`
        }
        return bankName
    }
    return branchName || "未记录"
}

function formatMoney(value: string): string {
    return formatCurrencyFixed(value, {
        maxScale: 4,
        minimumFractionDigits: 2,
    })
}

function EditedPill({ label = "已修改" }: { label?: string }) {
    return (
        <span className="inline-flex items-center rounded-md bg-info-soft px-1.5 py-0.5 text-tiny font-medium text-info-foreground">
            {label}
        </span>
    )
}

export function RequestReworkWorkspace(props: ReworkWorkspaceProps) {
    return (
        <RequestReworkWorkspaceInner
            key={`${props.request.id}:${props.request.version}`}
            {...props}
        />
    )
}

function RequestReworkWorkspaceInner({
    request,
    idPrefix,
    onSelectFile,
    onTaskCompleted,
    canAbandon = false,
    rejection,
    onRequestAbandon,
    renderAbandonAction,
}: ReworkWorkspaceProps) {
    const router = useRouter()
    const queryClient = useQueryClient()
    const candidatesQuery = usePaymentCandidatesQuery(true)
    const saveMutation = useSaveRequestMutation()
    const commandMutation = useRequestCommandMutation()
    const downloadMutation = useRequestDownloadMutation()
    const uploadMutation = useMutation({
        mutationFn: (files: File[]) =>
            Promise.all(files.map(uploadEvidenceFileAsset)),
    })

    const actions = useMemo(
        () => new Set(request.allowed_actions.map((a) => a.toUpperCase())),
        [request.allowed_actions],
    )
    const canEdit = actions.has("EDIT")
    const canSubmit = actions.has("SUBMIT")
    const approval = useMemo(
        () =>
            request.approval
                ? mapDocumentApprovalViewDto(request.approval)
                : undefined,
        [request.approval],
    )

    // 表单状态：默认以紧凑只读核验态呈现，按需原地展开对应分区修改。
    const [executionMethod, setExecutionMethod] = useState<
        "bank_transfer" | "supplier_deposit"
    >(request.execution_method.type)
    const [bankAccountId, setBankAccountId] = useState(
        request.payment_recipient?.bank_account_id ?? "",
    )
    const [depositAccountId, setDepositAccountId] = useState(
        request.execution_method.type === "supplier_deposit"
            ? request.execution_method.account_id
            : "",
    )
    const [desiredPaymentDate, setDesiredPaymentDate] = useState(
        request.desired_payment_date,
    )
    const [description, setDescription] = useState(request.description)
    const [lines, setLines] = useState<ReworkLineState[]>(() =>
        initialLinesFromRequest(request),
    )
    const [keptAttachmentIds, setKeptAttachmentIds] = useState<string[]>(() => [
        ...request.attachment_ids,
    ])
    const [newFiles, setNewFiles] = useState<File[]>([])
    const [editingSections, setEditingSections] = useState<
        Record<SectionKey, boolean>
    >({
        account: false,
        lines: false,
        details: false,
    })
    const [openSourceId, setOpenSourceId] = useState<string>()
    const [historyToggled, setHistoryToggled] = useState<boolean | null>(null)
    const [resubmitDialogOpen, setResubmitDialogOpen] = useState(false)
    const [resubmissionReason, setResubmissionReason] = useState("")
    const [touchedSubmit, setTouchedSubmit] = useState(false)
    const [submitError, setSubmitError] = useState<string | null>(null)
    const [submitUnknown, setSubmitUnknown] = useState(false)
    const [refreshing, setRefreshing] = useState(false)

    const uploadedFilesRef = useRef(new Map<File, string>())
    const submitIdentityRef = useRef<RequestCommand | null>(null)
    const submittingRef = useRef(false)
    const pendingMasterRefreshRef = useRef(false)

    // 候选来源与供应商实时账户：优先消费详情接口返回的实时供应商账户，不再依赖候选采购单余额。
    const matchingCandidates = useMemo(() => {
        const list = candidatesQuery.data ?? []
        return list.filter(
            (candidate) =>
                candidate.supplier_id === request.supplier_id &&
                candidate.our_company_id === request.our_company_id,
        )
    }, [candidatesQuery.data, request.our_company_id, request.supplier_id])

    const candidateByPayableId = useMemo(
        () =>
            new Map(
                matchingCandidates.map((candidate) => [
                    candidate.payable_account_id,
                    candidate,
                ]),
            ),
        [matchingCandidates],
    )

    const supplierBankAccounts = useMemo<PaymentRecipient[]>(() => {
        const map = new Map<string, PaymentRecipient>()
        for (const account of request.supplier_bank_accounts ?? []) {
            if (!map.has(account.bank_account_id)) {
                map.set(account.bank_account_id, account)
            }
        }
        for (const candidate of candidatesQuery.data ?? []) {
            if (candidate.supplier_id !== request.supplier_id) continue
            for (const account of candidate.bank_accounts) {
                if (!map.has(account.bank_account_id)) {
                    map.set(account.bank_account_id, account)
                }
            }
        }
        return Array.from(map.values())
    }, [
        candidatesQuery.data,
        request.supplier_bank_accounts,
        request.supplier_id,
    ])

    const hasLiveBankDirectory =
        request.supplier_bank_accounts !== undefined ||
        supplierBankAccounts.length > 0

    const depositAccounts = useMemo<DepositAccountOption[]>(() => {
        const map = new Map<string, DepositAccountOption>()
        for (const account of request.supplier_deposit_accounts ?? []) {
            if (!map.has(account.id)) map.set(account.id, account)
        }
        for (const candidate of matchingCandidates) {
            for (const account of candidate.deposit_accounts ?? []) {
                if (!map.has(account.id)) map.set(account.id, account)
            }
        }
        if (
            request.deposit_account &&
            !map.has(request.deposit_account.account_id)
        ) {
            const ref =
                request.deposit_account.identity.external_account_ref.slice(-4)
            map.set(request.deposit_account.account_id, {
                id: request.deposit_account.account_id,
                label: ref ? `人民币预存款 · ****${ref}` : "当前预存款账户",
            })
        }
        return Array.from(map.values())
    }, [
        matchingCandidates,
        request.deposit_account,
        request.supplier_deposit_accounts,
    ])

    const liveSelectedBankAccount = useMemo(
        () =>
            supplierBankAccounts.find(
                (account) => account.bank_account_id === bankAccountId,
            ) ?? null,
        [bankAccountId, supplierBankAccounts],
    )

    const displayedBankAccount = useMemo(() => {
        if (liveSelectedBankAccount) return liveSelectedBankAccount
        if (
            request.payment_recipient &&
            request.payment_recipient.bank_account_id === bankAccountId
        ) {
            return request.payment_recipient
        }
        return null
    }, [bankAccountId, liveSelectedBankAccount, request.payment_recipient])

    const bankAccountMasterUpdated = Boolean(
        executionMethod === "bank_transfer" &&
        liveSelectedBankAccount &&
        request.payment_recipient &&
        liveSelectedBankAccount.bank_account_id ===
            request.payment_recipient.bank_account_id &&
        liveSelectedBankAccount.version > request.payment_recipient.version,
    )

    const liveRoutingNumber =
        liveSelectedBankAccount?.bank_routing_number?.trim() || ""

    const routingNumberMissing = Boolean(
        executionMethod === "bank_transfer" &&
        liveSelectedBankAccount &&
        !liveRoutingNumber,
    )

    // 每行采购单的实时余额、校验与核销/占用证据
    const originalLineByPayableId = useMemo(
        () =>
            new Map(
                request.lines.map((line) => [line.payable_account_id, line]),
            ),
        [request.lines],
    )

    const reviewLineByPayableId = useMemo(
        () =>
            new Map(
                (request.review_lines ?? []).map((line) => [
                    line.payable_account_id,
                    line,
                ]),
            ),
        [request.review_lines],
    )

    const resolvedLines = useMemo(() => {
        return lines.map((entry) => {
            const candidate = candidateByPayableId.get(entry.payableAccountId)
            const origLine = originalLineByPayableId.get(entry.payableAccountId)
            const fallbackBasis = origLine
                ? paymentReviewBasis({ ...request, lines: [origLine] })
                : null
            const available =
                candidate?.available_amount ??
                fallbackBasis?.available ??
                "0.00"
            const total =
                candidate?.total_amount ??
                fallbackBasis?.total ??
                origLine?.total_amount ??
                "0.00"
            const settled =
                candidate?.paid_amount ??
                fallbackBasis?.paid ??
                origLine?.source_paid_amount ??
                "0.00"
            const purchaseOrderNo =
                candidate?.purchase_order_no ??
                origLine?.purchase_order_no ??
                entry.payableAccountId
            const purchaseVersion = origLine?.purchase_version
            const paymentTermName =
                candidate?.payment_term_name ??
                origLine?.payment_term_name ??
                ""
            const fulfillment = origLine?.current_fulfillment
            const noBalance = compareDecimal(available, "0", 4) <= 0
            const amountError = noBalance
                ? "当前可申请余额为 ¥0.00，无法继续申请此采购单"
                : validateReworkLineAmount(entry.amount, available)

            return {
                entry,
                candidate,
                origLine,
                reviewLine: reviewLineByPayableId.get(entry.payableAccountId),
                purchaseOrderNo,
                purchaseVersion,
                paymentTermName,
                fulfillment,
                available,
                total,
                settled,
                noBalance,
                amountError,
            }
        })
    }, [
        candidateByPayableId,
        lines,
        originalLineByPayableId,
        request,
        reviewLineByPayableId,
    ])

    const addableCandidates = useMemo(() => {
        const selected = new Set(lines.map((line) => line.payableAccountId))
        return matchingCandidates.filter(
            (candidate) => !selected.has(candidate.payable_account_id),
        )
    }, [lines, matchingCandidates])

    // 汇总金额、余额与进度
    const liveTotalAmount = useMemo(
        () => totalOf(lines.map((line) => line.amount)),
        [lines],
    )
    const liveAvailableTotal = useMemo(
        () =>
            sumFixed(
                resolvedLines.map((row) =>
                    clampZeroFixed(row.available, moneyMath),
                ),
                moneyMath,
            ),
        [resolvedLines],
    )
    const liveSourceTotal = useMemo(
        () =>
            sumFixed(
                resolvedLines.map((row) => row.total),
                moneyMath,
            ),
        [resolvedLines],
    )
    const liveSourceSettled = useMemo(
        () =>
            sumFixed(
                resolvedLines.map((row) => row.settled),
                moneyMath,
            ),
        [resolvedLines],
    )
    const paidRate = useMemo(() => {
        if (compareDecimal(liveSourceTotal, "0", 4) <= 0) return "0"
        return divideFixed(
            multiplyFixed(clampZeroFixed(liveSourceSettled, moneyMath), "100", {
                leftMaxScale: 4,
                rightMaxScale: 0,
                outputScale: 4,
            }),
            liveSourceTotal,
            {
                numeratorMaxScale: 4,
                denominatorMaxScale: 4,
                outputScale: 1,
            },
        )
    }, [liveSourceSettled, liveSourceTotal])

    const amountChanged = useMemo(() => {
        try {
            return (
                compareDecimal(liveTotalAmount, request.total_amount, 4) !== 0
            )
        } catch {
            return true
        }
    }, [liveTotalAmount, request.total_amount])

    // 余额去向摘要：余额不足时在行动指挥卡中直接亮明证据，无需用户翻找折叠面板
    const balanceEvidenceItems = useMemo(() => {
        const items: string[] = []
        for (const row of resolvedLines) {
            if (!row.noBalance && !row.amountError) continue
            const review = row.reviewLine
            if (!review) continue
            for (const settlement of review.settlements) {
                if (compareDecimal(settlement.effective_amount, "0", 4) <= 0) {
                    continue
                }
                const docLabel = settlement.document_no
                    ? ` ${settlement.document_no}`
                    : ""
                items.push(
                    `${row.purchaseOrderNo}：已通过${settlement.method_label}${docLabel} 结算 ${formatMoney(settlement.effective_amount)}`,
                )
            }
            for (const other of review.other_requests) {
                if (
                    ![
                        "in_approval",
                        "approved",
                        "partially_paid",
                        "paid",
                    ].includes(other.status)
                ) {
                    continue
                }
                items.push(
                    `${row.purchaseOrderNo}：存在其他申请 ${other.request_no}（${requestStatusLabel[other.status]}）${formatMoney(other.amount)}`,
                )
            }
        }
        return items
    }, [resolvedLines])

    const historyRecordCount = useMemo(() => {
        let count = 0
        for (const review of request.review_lines ?? []) {
            count += review.settlements.length + review.other_requests.length
        }
        return count
    }, [request.review_lines])

    // 表单校验与三态势分诊
    const accountError = useMemo(() => {
        if (executionMethod === "bank_transfer") {
            if (!bankAccountId) return "请选择供应商的有效收款账户"
            if (
                hasLiveBankDirectory &&
                !supplierBankAccounts.some(
                    (account) => account.bank_account_id === bankAccountId,
                )
            ) {
                return "原收款账户已停用或失效，请更换有效收款账户"
            }
            return undefined
        }
        if (!depositAccountId) return "请选择同付款主体、同供应商的预存款账户"
        return undefined
    }, [
        bankAccountId,
        depositAccountId,
        executionMethod,
        hasLiveBankDirectory,
        supplierBankAccounts,
    ])

    const dateError = !normalizeDate(desiredPaymentDate)
        ? "请选择希望付款日"
        : undefined
    const trimmedDescription = description.trim()
    const descriptionCharCount = Array.from(trimmedDescription).length
    const descriptionError = !trimmedDescription
        ? "请填写申请说明"
        : descriptionCharCount > DESCRIPTION_MAX_CHARS
          ? `申请说明最多 ${DESCRIPTION_MAX_CHARS} 字`
          : undefined

    const lineErrors = resolvedLines.filter((row) => Boolean(row.amountError))
    const hasBalanceBlock = resolvedLines.some(
        (row) =>
            row.noBalance ||
            (positive(row.entry.amount) &&
                compareDecimal(row.entry.amount, row.available, 4) > 0),
    )
    const isPremiseInvalidated =
        resolvedLines.length > 0 && resolvedLines.every((row) => row.noBalance)
    const hasAdjustableOverLimit =
        !isPremiseInvalidated &&
        resolvedLines.some(
            (row) =>
                !row.noBalance &&
                positive(row.entry.amount) &&
                compareDecimal(row.entry.amount, row.available, 4) > 0,
        )

    const triageState: "premise_invalidated" | "needs_fix" | "ready" =
        isPremiseInvalidated
            ? "premise_invalidated"
            : hasBalanceBlock || Boolean(accountError) || routingNumberMissing
              ? "needs_fix"
              : "ready"

    // 余额异常时默认展开付款核销与其他申请占用，确保真相直接可见
    const paymentHistoryOpen =
        historyToggled !== null
            ? historyToggled
            : hasBalanceBlock && historyRecordCount > 0

    // 分区改动检测 & 精准 Diff（供区块标记与重提确认弹窗使用）
    const accountSectionChanged = useMemo(() => {
        if (executionMethod !== request.execution_method.type) return true
        if (normalizeDate(desiredPaymentDate) !== request.desired_payment_date)
            return true
        if (executionMethod === "bank_transfer") {
            return (
                bankAccountId !==
                (request.payment_recipient?.bank_account_id ?? "")
            )
        }
        const origDepositId =
            request.execution_method.type === "supplier_deposit"
                ? request.execution_method.account_id
                : ""
        return depositAccountId !== origDepositId
    }, [
        bankAccountId,
        depositAccountId,
        desiredPaymentDate,
        executionMethod,
        request.desired_payment_date,
        request.execution_method,
        request.payment_recipient?.bank_account_id,
    ])

    const linesSectionChanged = useMemo(
        () => !sameLines(lines, request.lines),
        [lines, request.lines],
    )

    const detailsSectionChanged = useMemo(
        () =>
            description.trim() !== request.description.trim() ||
            !sameStringSet(keptAttachmentIds, request.attachment_ids) ||
            newFiles.length > 0,
        [
            description,
            keptAttachmentIds,
            newFiles.length,
            request.attachment_ids,
            request.description,
        ],
    )

    const resubmissionDiffs = useMemo<ReworkFieldDiff[]>(() => {
        const diffs: ReworkFieldDiff[] = []
        if (executionMethod !== request.execution_method.type) {
            diffs.push({
                label: "付款方式",
                before:
                    request.execution_method.type === "supplier_deposit"
                        ? "供应商预存款"
                        : "银行付款",
                after:
                    executionMethod === "supplier_deposit"
                        ? "供应商预存款"
                        : "银行付款",
            })
        }
        if (executionMethod === "bank_transfer") {
            const origAccountId =
                request.payment_recipient?.bank_account_id ?? ""
            if (bankAccountId !== origAccountId) {
                diffs.push({
                    label: "收款账户",
                    before:
                        request.payment_recipient?.account_number_masked ??
                        "未选择",
                    after:
                        displayedBankAccount?.account_number_masked ?? "已更换",
                    numeric: true,
                })
            } else if (bankAccountMasterUpdated) {
                diffs.push({
                    label: "收款账户主数据",
                    after: liveRoutingNumber
                        ? `已同步最新资料（联行号 ${liveRoutingNumber}）`
                        : "已同步供应商最新收款账户",
                })
            }
        } else {
            const origDepositId =
                request.execution_method.type === "supplier_deposit"
                    ? request.execution_method.account_id
                    : ""
            if (depositAccountId !== origDepositId) {
                diffs.push({
                    label: "预存款账户",
                    after:
                        depositAccounts.find((a) => a.id === depositAccountId)
                            ?.label ?? "已更换",
                })
            }
        }
        if (
            normalizeDate(desiredPaymentDate) !== request.desired_payment_date
        ) {
            diffs.push({
                label: "希望付款日",
                before: request.desired_payment_date,
                after: normalizeDate(desiredPaymentDate),
                numeric: true,
            })
        }
        if (amountChanged) {
            diffs.push({
                label: "申请金额",
                before: formatMoney(request.total_amount),
                after: formatMoney(liveTotalAmount),
                numeric: true,
            })
        } else if (linesSectionChanged) {
            diffs.push({
                label: "来源采购单",
                after: `已调整为 ${lines.length} 张采购单`,
            })
        }
        if (description.trim() !== request.description.trim()) {
            diffs.push({
                label: "申请说明",
                after: "已更新说明内容",
            })
        }
        const removedCount = request.attachment_ids.filter(
            (id) => !keptAttachmentIds.includes(id),
        ).length
        if (newFiles.length > 0 || removedCount > 0) {
            const parts: string[] = []
            if (newFiles.length > 0) parts.push(`新增 ${newFiles.length} 份`)
            if (removedCount > 0) parts.push(`移除 ${removedCount} 份`)
            diffs.push({
                label: "申请附件",
                after: parts.join("、"),
            })
        }
        return diffs
    }, [
        amountChanged,
        bankAccountId,
        bankAccountMasterUpdated,
        depositAccountId,
        depositAccounts,
        description,
        desiredPaymentDate,
        displayedBankAccount?.account_number_masked,
        executionMethod,
        keptAttachmentIds,
        lines.length,
        linesSectionChanged,
        liveRoutingNumber,
        liveTotalAmount,
        newFiles.length,
        request.attachment_ids,
        request.description,
        request.desired_payment_date,
        request.execution_method,
        request.payment_recipient?.account_number_masked,
        request.payment_recipient?.bank_account_id,
        request.total_amount,
    ])

    const hasFormChanges =
        accountSectionChanged || linesSectionChanged || detailsSectionChanged

    const changedFormSectionCount =
        (accountSectionChanged ? 1 : 0) +
        (linesSectionChanged ? 1 : 0) +
        (detailsSectionChanged ? 1 : 0)

    const requiresResubmissionReason = request.status === "rejected"
    const trimmedResubmissionReason = resubmissionReason.trim()
    const resubmissionReasonError = useMemo(() => {
        if (!requiresResubmissionReason) return undefined
        if (!trimmedResubmissionReason) {
            return "请填写重新提交说明，向审批人说明本次修改或重提依据"
        }
        if (
            utf8ByteLength(trimmedResubmissionReason) >
            BPM_REASON_MAX_UTF8_BYTES
        ) {
            return `重新提交说明不能超过 ${BPM_REASON_MAX_UTF8_BYTES} 字节`
        }
        return undefined
    }, [requiresResubmissionReason, trimmedResubmissionReason])

    const quickReplyPhrases = useMemo(() => {
        const phrases: string[] = []
        if (
            (bankAccountMasterUpdated && liveRoutingNumber) ||
            (rejection?.reason?.includes("联行号") && liveRoutingNumber)
        ) {
            phrases.push("已补充供应商联行号，请复核")
        }
        if (amountChanged || linesSectionChanged) {
            phrases.push("已按意见调整采购来源及申请金额，请复核")
        }
        if (newFiles.length > 0) {
            phrases.push("已补充相关证明附件，请复核")
        }
        if (accountSectionChanged && !phrases.length) {
            phrases.push("已更新付款账户及日期信息，请复核")
        }
        phrases.push("已按审批意见核对完善，请复核")
        return [...new Set(phrases)].slice(0, 3)
    }, [
        accountSectionChanged,
        amountChanged,
        bankAccountMasterUpdated,
        linesSectionChanged,
        liveRoutingNumber,
        newFiles.length,
        rejection?.reason,
    ])

    const invalidFormatLines = resolvedLines.filter(
        (row) => !positive(row.entry.amount),
    )
    const hasSaveValidationError = Boolean(
        accountError ||
        dateError ||
        descriptionError ||
        lines.length === 0 ||
        invalidFormatLines.length > 0,
    )
    const hasDraftValidationError = Boolean(
        hasSaveValidationError || lineErrors.length > 0,
    )

    const ownAttachmentMap = useMemo(() => {
        const ownIds = new Set(request.attachment_ids)
        return new Map(
            request.attachments
                .filter((file) => ownIds.has(file.file_asset_id))
                .map((file) => [file.file_asset_id, file]),
        )
    }, [request.attachment_ids, request.attachments])

    const removedAttachmentIds = useMemo(
        () =>
            request.attachment_ids.filter(
                (id) => !keptAttachmentIds.includes(id),
            ),
        [keptAttachmentIds, request.attachment_ids],
    )

    const busy =
        saveMutation.isPending ||
        commandMutation.isPending ||
        uploadMutation.isPending

    const refreshLiveData = useCallback(async () => {
        setRefreshing(true)
        try {
            await Promise.all([
                candidatesQuery.refetch(),
                queryClient.invalidateQueries({
                    queryKey: paymentRequestKeys.detail(request.id),
                }),
                queryClient.invalidateQueries({
                    queryKey: queryKeyRoots.workItems,
                }),
            ])
        } finally {
            setRefreshing(false)
        }
    }, [candidatesQuery, queryClient, request.id])

    // 用户跳转供应商档案补联行号后切回工作台页签时自动同步最新主数据
    useEffect(() => {
        const handleReturn = () => {
            if (
                document.visibilityState === "visible" &&
                pendingMasterRefreshRef.current
            ) {
                pendingMasterRefreshRef.current = false
                void refreshLiveData()
            }
        }
        window.addEventListener("focus", handleReturn)
        document.addEventListener("visibilitychange", handleReturn)
        return () => {
            window.removeEventListener("focus", handleReturn)
            document.removeEventListener("visibilitychange", handleReturn)
        }
    }, [refreshLiveData])

    const openSupplierProfile = () => {
        pendingMasterRefreshRef.current = true
        const href = `/master-data/suppliers/${encodeURIComponent(request.supplier_id)}`
        if (!navigateWorkspace(href)) {
            router.push(href)
        }
    }

    const openSectionForEdit = (
        section: SectionKey,
        scrollTargetId?: string,
    ) => {
        setEditingSections((prev) => ({ ...prev, [section]: true }))
        window.requestAnimationFrame(() => {
            const el = document.getElementById(
                scrollTargetId ?? `${idPrefix}-section-${section}`,
            )
            el?.scrollIntoView({ behavior: "smooth", block: "center" })
        })
    }

    const toggleSectionEdit = (section: SectionKey) => {
        setEditingSections((prev) => ({ ...prev, [section]: !prev[section] }))
    }

    const adjustOverLimitLinesToAvailable = () => {
        setLines((prev) =>
            prev.map((line) => {
                const row = resolvedLines.find(
                    (item) =>
                        item.entry.payableAccountId === line.payableAccountId,
                )
                if (!row || row.noBalance) return line
                if (
                    !positive(line.amount) ||
                    compareDecimal(line.amount, row.available, 4) > 0
                ) {
                    return { ...line, amount: row.available }
                }
                return line
            }),
        )
        openSectionForEdit("lines")
    }

    const buildDraftInput = async (): Promise<RequestDraftInput> => {
        const pendingFiles = newFiles.filter(
            (file) => !uploadedFilesRef.current.has(file),
        )
        if (pendingFiles.length > 0) {
            const assets = await uploadMutation.mutateAsync(pendingFiles)
            pendingFiles.forEach((file, index) => {
                uploadedFilesRef.current.set(file, assets[index].id)
            })
        }
        const execution: PaymentExecutionMethod =
            executionMethod === "supplier_deposit"
                ? { type: "supplier_deposit", account_id: depositAccountId }
                : { type: "bank_transfer" }
        return {
            supplier_id: request.supplier_id,
            bank_account_id:
                executionMethod === "bank_transfer" ? bankAccountId : undefined,
            bank_account_version:
                executionMethod === "bank_transfer"
                    ? (liveSelectedBankAccount?.version ??
                      request.payment_recipient?.version)
                    : undefined,
            execution_method: execution,
            desired_payment_date: normalizeDate(desiredPaymentDate),
            description: trimmedDescription,
            lines: lines.map((line) => ({
                payable_account_id: line.payableAccountId,
                amount: line.amount.trim(),
            })),
            attachment_ids: [
                ...keptAttachmentIds,
                ...newFiles.map((file) => uploadedFilesRef.current.get(file)!),
            ],
        }
    }

    const focusFirstInvalid = () => {
        if (accountError || dateError) {
            openSectionForEdit("account")
            return
        }
        if (lineErrors.length > 0 || lines.length === 0) {
            openSectionForEdit("lines")
            return
        }
        if (descriptionError) {
            openSectionForEdit("details")
        }
    }

    const handleSaveDraft = async () => {
        if (busy || submitUnknown || !canEdit || !hasFormChanges) return
        setTouchedSubmit(true)
        setSubmitError(null)
        if (hasSaveValidationError) {
            focusFirstInvalid()
            return
        }
        try {
            const input = await buildDraftInput()
            await saveMutation.mutateAsync({
                id: request.id,
                version: request.version,
                data: input,
            })
            setNewFiles([])
            setEditingSections({ account: false, lines: false, details: false })
            toast.add({
                title: "已暂存修改",
                description: "申请内容已更新，当前驳回待办仍为你保留。",
                type: "success",
            })
        } catch (error) {
            setSubmitError(getErrorMessage(error, "暂存修改失败，请核对后重试"))
        }
    }

    const executeSubmit = async (identity: RequestCommand) => {
        await commandMutation.mutateAsync({
            id: request.id,
            action: "submit",
            command: identity,
        })
        submitIdentityRef.current = null
        setSubmitUnknown(false)
        setSubmitError(null)
        setResubmitDialogOpen(false)
        toast.add({
            title: "已重新提交财务审核",
            description: `${request.request_no} 已重新进入财务审核流程。`,
            type: "success",
        })
        onTaskCompleted?.()
    }

    const handleOpenResubmitDialog = () => {
        if (busy || submitUnknown || !canSubmit) return
        setTouchedSubmit(true)
        setSubmitError(null)
        if (hasDraftValidationError) {
            focusFirstInvalid()
            return
        }
        setResubmitDialogOpen(true)
    }

    const handleConfirmResubmit = async () => {
        if (submittingRef.current || busy || !canSubmit) return
        if (submitUnknown && submitIdentityRef.current) {
            submittingRef.current = true
            setSubmitError(null)
            try {
                await executeSubmit(submitIdentityRef.current)
            } catch (error) {
                if (!isCommandOutcomeUnknown(error)) {
                    submitIdentityRef.current = null
                    setSubmitUnknown(false)
                    setSubmitError(
                        getErrorMessage(error, "重新提交未完成，请核对后重试"),
                    )
                }
            } finally {
                submittingRef.current = false
            }
            return
        }

        if (hasDraftValidationError || resubmissionReasonError) {
            setTouchedSubmit(true)
            return
        }

        submittingRef.current = true
        setSubmitError(null)
        try {
            let expectedVersion = request.version
            if (hasFormChanges) {
                if (!canEdit) {
                    throw new Error("当前账号无权修改付款申请内容")
                }
                const input = await buildDraftInput()
                const saved = await saveMutation.mutateAsync({
                    id: request.id,
                    version: request.version,
                    data: input,
                })
                expectedVersion = saved.version
                setNewFiles([])
            }

            const identity: RequestCommand = {
                expected_version: expectedVersion,
                resubmission_reason: requiresResubmissionReason
                    ? trimmedResubmissionReason
                    : undefined,
                idempotency_key: `ppr-submit:${crypto.randomUUID()}`,
            }
            submitIdentityRef.current = identity
            await executeSubmit(identity)
        } catch (error) {
            const unknown =
                Boolean(submitIdentityRef.current) &&
                isCommandOutcomeUnknown(error)
            setSubmitUnknown(unknown)
            if (unknown) {
                setResubmitDialogOpen(false)
            } else {
                submitIdentityRef.current = null
                setSubmitError(
                    getErrorMessage(error, "重新提交失败，请核对后重试"),
                )
            }
            throw error
        } finally {
            submittingRef.current = false
        }
    }

    useEffect(() => {
        if (!submitUnknown) return
        if (request.status === "in_approval") {
            submitIdentityRef.current = null
            setSubmitUnknown(false)
            onTaskCompleted?.()
        }
    }, [onTaskCompleted, request.status, submitUnknown])

    const primaryResubmitLabel = busy
        ? hasFormChanges
            ? "正在保存并重提…"
            : "正在提交…"
        : hasFormChanges
          ? "保存并重新提交"
          : "重新提交审批"

    const currentDepositLabel =
        depositAccounts.find((a) => a.id === depositAccountId)?.label ??
        "未选择预存款账户"

    const isAccountEditing =
        editingSections.account ||
        (Boolean(accountError) && !isPremiseInvalidated)

    const reworkTriageItems: WorkspaceReworkTriageItem[] = []
    if (triageState !== "premise_invalidated") {
        if (accountError) {
            reworkTriageItems.push({
                id: "account-error",
                tone: "destructive",
                text: accountError,
                action: canEdit
                    ? {
                          id: `${idPrefix}-triage-fix-account`,
                          label: "更换收款账户",
                          variant: "outline",
                          onClick: () => openSectionForEdit("account"),
                      }
                    : undefined,
            })
        }
        if (routingNumberMissing) {
            reworkTriageItems.push({
                id: "routing-number-missing",
                tone: "warning",
                text: "当前收款账户尚未填写联行号（在供应商档案补全后切回本页将自动同步）",
                action: {
                    id: `${idPrefix}-triage-open-supplier`,
                    label: "去维护供应商账户",
                    icon: "external",
                    variant: "outline",
                    onClick: openSupplierProfile,
                },
            })
        }
        if (hasBalanceBlock) {
            const balanceActions = [
                ...(hasAdjustableOverLimit && canEdit
                    ? [
                          {
                              id: `${idPrefix}-triage-clamp-amount`,
                              label: "一键调至可用额",
                              variant: "outline" as const,
                              onClick: adjustOverLimitLinesToAvailable,
                          },
                      ]
                    : []),
                ...(canEdit
                    ? [
                          {
                              id: `${idPrefix}-triage-fix-lines`,
                              label: "定位调整",
                              variant: "ghost" as const,
                              onClick: () => openSectionForEdit("lines"),
                          },
                      ]
                    : []),
            ]
            reworkTriageItems.push({
                id: "balance-block",
                tone: "destructive",
                text: (
                    <>
                        申请金额超出采购单当前可申请余额（可用{" "}
                        <MoneyValue
                            value={liveAvailableTotal}
                            display="exact"
                        />
                        ）
                    </>
                ),
                actions: balanceActions.length > 0 ? balanceActions : undefined,
            })
        }
        if (bankAccountMasterUpdated) {
            reworkTriageItems.push({
                id: "bank-account-master-updated",
                tone: "success",
                text: `已同步供应商最新收款账户主数据${liveRoutingNumber ? `（联行号 ${liveRoutingNumber}）` : ""}，重提时将自动按最新资料提交。`,
            })
        }
    }

    return (
        <div className="min-w-0 space-y-5 bg-card pt-3 pb-4">
            {/* 1. 统一行动指挥卡：合并「上轮驳回意见 + 前置可行性体检 + 唯一清晰行动指引」 */}
            <WorkspaceReworkCommandCard
                idPrefix={idPrefix}
                rejection={rejection ?? {}}
                stance={
                    triageState === "premise_invalidated"
                        ? "blocked"
                        : triageState
                }
                headline={
                    triageState === "premise_invalidated"
                        ? "当前无法继续重提：来源采购单可申请余额已为 ¥0.00"
                        : triageState === "ready"
                          ? "前置校验已通过，核对或按需修改后即可重新提交"
                          : "提交前需先核对或处理以下事项"
                }
                description={
                    triageState === "premise_invalidated"
                        ? canAbandon
                            ? "该采购单可能已通过其他付款或申请结清。若无需继续付款，建议核对下方记录后直接「放弃本次办理」关闭此待办。"
                            : "该采购单已无可用申请额度；如需改付同供应商其他未付采购单，请点击下方「更换采购单」。"
                        : triageState === "ready"
                          ? "收款账户与来源采购余额均符合重提条件；如需改动单据内容，可点击对应分区右上角「修改」。"
                          : undefined
                }
                items={reworkTriageItems}
                evidence={
                    triageState === "premise_invalidated"
                        ? balanceEvidenceItems.length > 0
                            ? balanceEvidenceItems.slice(0, 4)
                            : [
                                  <span
                                      key="source-balance-summary"
                                      className="text-muted-foreground"
                                  >
                                      来源采购总额{" "}
                                      <MoneyValue
                                          value={liveSourceTotal}
                                          display="exact"
                                          className="font-medium text-foreground"
                                      />{" "}
                                      · 已结算{" "}
                                      <RateValue
                                          value={paidRate}
                                          precision={1}
                                          className="font-medium text-foreground"
                                      />
                                      ，当前无可申请余额。
                                  </span>,
                              ]
                        : undefined
                }
                primaryAction={
                    triageState === "premise_invalidated" &&
                    canAbandon &&
                    onRequestAbandon
                        ? {
                              id: `${idPrefix}-card-abandon`,
                              label: "放弃本次办理",
                              variant: "default",
                              disabled: busy,
                              onClick: onRequestAbandon,
                          }
                        : undefined
                }
                secondaryActions={
                    triageState === "premise_invalidated" &&
                    addableCandidates.length > 0 &&
                    canEdit
                        ? [
                              {
                                  id: `${idPrefix}-card-switch-po`,
                                  label: `更换其他未付采购单（${addableCandidates.length}）`,
                                  variant: "outline",
                                  disabled: busy,
                                  onClick: () => openSectionForEdit("lines"),
                              },
                          ]
                        : undefined
                }
                onRefresh={() => void refreshLiveData()}
                refreshing={refreshing}
                refreshLabel={
                    triageState === "premise_invalidated"
                        ? "刷新余额"
                        : "刷新校验"
                }
                footnote={
                    triageState === "premise_invalidated" &&
                    routingNumberMissing
                        ? "另注：当前收款账户尚未填写联行号；若后续仍需更换其他采购单发起付款，可先在供应商档案补全联行号。"
                        : undefined
                }
            />

            {/* 2. 核心资金指标带（仅保留 2 格高价值金额与余额指标，消除低价值凑数格） */}
            <TooltipProvider>
                <dl className="grid min-w-0 grid-cols-1 gap-x-6 gap-y-3 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/payment-request:grid-cols-2">
                    <div className="min-w-0 space-y-1">
                        <dt className="flex items-center gap-1.5 text-xs text-muted-foreground">
                            <span>本次申请付款金额</span>
                            <span className="text-tiny text-muted-foreground/80">
                                含税
                            </span>
                        </dt>
                        <dd className="min-w-0 space-y-1">
                            <MoneyValue
                                value={liveTotalAmount}
                                size="section"
                                display="exact"
                            />
                            <p className="text-tiny text-muted-foreground">
                                {amountChanged ? (
                                    <>
                                        原提交{" "}
                                        <span className="num line-through">
                                            {formatMoney(request.total_amount)}
                                        </span>{" "}
                                        ·{" "}
                                    </>
                                ) : null}
                                <span className="num">{lines.length}</span>{" "}
                                张采购单
                            </p>
                        </dd>
                    </div>

                    <div className="min-w-0 space-y-1">
                        <dt className="flex items-center gap-1.5 text-xs text-muted-foreground">
                            <span>来源可申请余额</span>
                            <span className="text-tiny text-muted-foreground/80">
                                采购未结 − 占用
                            </span>
                            <Tooltip>
                                <TooltipTrigger
                                    id={`${idPrefix}-available-formula`}
                                    aria-label="来源可申请余额口径"
                                    render={
                                        <Button
                                            variant="ghost"
                                            size="icon-xs"
                                            type="button"
                                            className="size-4 rounded-full border border-border/80 text-tiny text-muted-foreground hover:bg-card hover:text-foreground"
                                        />
                                    }
                                >
                                    <span aria-hidden="true">?</span>
                                </TooltipTrigger>
                                <TooltipContent>
                                    已驳回申请不占用采购余额。当前展示所选采购单的最新可申请额度。
                                </TooltipContent>
                            </Tooltip>
                        </dt>
                        <dd className="min-w-0 space-y-1">
                            <MoneyValue
                                value={liveAvailableTotal}
                                display="exact"
                                className={cn(
                                    "num text-value-compact font-semibold",
                                    hasBalanceBlock && "text-destructive",
                                )}
                            />
                            <div
                                aria-hidden="true"
                                className="h-1.5 overflow-hidden rounded-full bg-muted"
                            >
                                <div
                                    className="h-full rounded-full bg-primary"
                                    style={{
                                        width: `${minFixed(paidRate, "100", { maxScale: 1, outputScale: 1 })}%`,
                                    }}
                                />
                            </div>
                            <p className="text-tiny text-muted-foreground">
                                采购总额{" "}
                                <MoneyValue
                                    value={liveSourceTotal}
                                    display="exact"
                                />{" "}
                                · 已结算{" "}
                                <RateValue
                                    value={paidRate}
                                    precision={1}
                                    className="text-muted-foreground"
                                />
                            </p>
                        </dd>
                    </div>
                </dl>
            </TooltipProvider>

            <fieldset
                disabled={busy || submitUnknown || !canEdit}
                className="min-w-0 space-y-5 disabled:opacity-75"
            >
                <legend className="sr-only">付款申请核验与按需修改</legend>

                {/* 3. 分区一：付款与收款账户（默认紧凑只读，按需点「修改」原地展开） */}
                <WorkspaceFactGroup
                    id={`${idPrefix}-section-account`}
                    title={
                        <span className="inline-flex flex-wrap items-center gap-2">
                            <span>付款与收款账户</span>
                            {accountSectionChanged && <EditedPill />}
                            {bankAccountMasterUpdated && (
                                <span className="inline-flex items-center rounded-md bg-success-soft px-1.5 py-0.5 text-tiny font-medium text-success-foreground">
                                    主数据已更新
                                </span>
                            )}
                        </span>
                    }
                    actions={
                        <>
                            {executionMethod === "bank_transfer" && (
                                <>
                                    <Button
                                        id={`${idPrefix}-open-supplier-bank`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        className={
                                            detailWorkspaceStyles.quietButton
                                        }
                                        onClick={openSupplierProfile}
                                    >
                                        维护供应商账户
                                        <ArrowUpRightIcon
                                            data-icon="inline-end"
                                            aria-hidden="true"
                                        />
                                    </Button>
                                    <Button
                                        id={`${idPrefix}-refresh-accounts`}
                                        type="button"
                                        variant="ghost"
                                        size="xs"
                                        disabled={
                                            refreshing ||
                                            candidatesQuery.isFetching
                                        }
                                        className={
                                            detailWorkspaceStyles.quietButton
                                        }
                                        onClick={() => void refreshLiveData()}
                                    >
                                        <RefreshCwIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                            className={cn(
                                                (refreshing ||
                                                    candidatesQuery.isFetching) &&
                                                    "animate-spin",
                                            )}
                                        />
                                        刷新账户
                                    </Button>
                                </>
                            )}
                            {canEdit && (
                                <Button
                                    id={`${idPrefix}-toggle-edit-account`}
                                    type="button"
                                    variant={
                                        isAccountEditing ? "secondary" : "ghost"
                                    }
                                    size="xs"
                                    className={cn(
                                        !isAccountEditing &&
                                            detailWorkspaceStyles.quietButton,
                                    )}
                                    onClick={() => toggleSectionEdit("account")}
                                >
                                    {!isAccountEditing && (
                                        <PencilLineIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                    )}
                                    {isAccountEditing ? "完成修改" : "修改"}
                                </Button>
                            )}
                        </>
                    }
                >
                    {!isAccountEditing ? (
                        <WorkspaceFactGrid
                            columns={3}
                            ariaLabel="付款与收款账户只读核验"
                        >
                            <WorkspaceFactItem
                                label="付款方式"
                                value={
                                    executionMethod === "supplier_deposit"
                                        ? "供应商预存款"
                                        : "银行付款"
                                }
                            />
                            <WorkspaceFactItem
                                label="希望付款日"
                                value={desiredPaymentDate}
                                numeric
                            />
                            <WorkspaceFactItem
                                label="付款主体"
                                value={request.our_company_name}
                            />
                            <WorkspaceFactItem
                                label="供应商"
                                value={request.supplier_name}
                            />
                            <WorkspaceFactItem
                                label="申请人"
                                value={request.applicant_name}
                            />
                            {executionMethod === "bank_transfer" ? (
                                <>
                                    <WorkspaceFactItem
                                        label="收款户名"
                                        value={
                                            displayedBankAccount?.account_name
                                        }
                                    />
                                    <WorkspaceFactItem
                                        label="开户行"
                                        value={
                                            displayedBankAccount
                                                ? bankDisplayLabel(
                                                      displayedBankAccount,
                                                  )
                                                : undefined
                                        }
                                    />
                                    <WorkspaceFactItem
                                        label="收款账号"
                                        value={
                                            displayedBankAccount?.account_number_masked
                                        }
                                        numeric
                                        selectable
                                    />
                                    <WorkspaceFactItem
                                        label="联行号"
                                        highlighted={routingNumberMissing}
                                        tone={
                                            routingNumberMissing
                                                ? "warning"
                                                : "default"
                                        }
                                    >
                                        {liveRoutingNumber ? (
                                            <span className="num select-all text-foreground">
                                                {liveRoutingNumber}
                                            </span>
                                        ) : liveSelectedBankAccount ? (
                                            <span className="inline-flex flex-wrap items-center gap-2">
                                                <span className="text-warning-soft-foreground">
                                                    未填写
                                                </span>
                                                <Button
                                                    id={`${idPrefix}-routing-fill`}
                                                    type="button"
                                                    variant="ghost"
                                                    size="xs"
                                                    className="h-5 px-1.5 text-xs text-warning-soft-foreground hover:bg-warning-soft"
                                                    onClick={
                                                        openSupplierProfile
                                                    }
                                                >
                                                    去补全
                                                    <ArrowUpRightIcon
                                                        data-icon="inline-end"
                                                        aria-hidden="true"
                                                    />
                                                </Button>
                                            </span>
                                        ) : accountError ? (
                                            <span className="text-destructive">
                                                原账户已失效
                                            </span>
                                        ) : (
                                            <span className="text-muted-foreground">
                                                —
                                            </span>
                                        )}
                                    </WorkspaceFactItem>
                                </>
                            ) : (
                                <WorkspaceFactItem
                                    label="预存款账户"
                                    value={currentDepositLabel}
                                />
                            )}
                        </WorkspaceFactGrid>
                    ) : (
                        <div className="space-y-3.5 rounded-lg border border-border/60 bg-surface-sunken/35 p-3.5">
                            <div className="grid grid-cols-1 gap-4 @[520px]/payment-request:grid-cols-2">
                                <Field className="gap-1.5">
                                    <FieldLabel
                                        htmlFor={`${idPrefix}-execution-method`}
                                    >
                                        付款方式
                                        <span className="text-destructive">
                                            *
                                        </span>
                                    </FieldLabel>
                                    <Select
                                        value={executionMethod}
                                        onValueChange={(next) => {
                                            if (
                                                next !== "bank_transfer" &&
                                                next !== "supplier_deposit"
                                            ) {
                                                return
                                            }
                                            setExecutionMethod(next)
                                            if (
                                                next === "bank_transfer" &&
                                                !bankAccountId &&
                                                supplierBankAccounts[0]
                                            ) {
                                                setBankAccountId(
                                                    supplierBankAccounts[0]
                                                        .bank_account_id,
                                                )
                                            }
                                            if (
                                                next === "supplier_deposit" &&
                                                !depositAccountId &&
                                                depositAccounts[0]
                                            ) {
                                                setDepositAccountId(
                                                    depositAccounts[0].id,
                                                )
                                            }
                                        }}
                                    >
                                        <SelectTrigger
                                            id={`${idPrefix}-execution-method`}
                                            className="w-full bg-card"
                                        >
                                            <SelectValue>
                                                {executionMethod ===
                                                "supplier_deposit"
                                                    ? "供应商预存款"
                                                    : "银行付款"}
                                            </SelectValue>
                                        </SelectTrigger>
                                        <SelectContent align="start">
                                            <SelectItem
                                                id={`${idPrefix}-method-bank`}
                                                value="bank_transfer"
                                            >
                                                银行付款
                                            </SelectItem>
                                            <SelectItem
                                                id={`${idPrefix}-method-deposit`}
                                                value="supplier_deposit"
                                            >
                                                供应商预存款
                                            </SelectItem>
                                        </SelectContent>
                                    </Select>
                                </Field>

                                <Field
                                    className="gap-1.5"
                                    data-invalid={
                                        (touchedSubmit && Boolean(dateError)) ||
                                        undefined
                                    }
                                >
                                    <FieldLabel
                                        htmlFor={`${idPrefix}-desired-date`}
                                    >
                                        希望付款日
                                        <span className="text-destructive">
                                            *
                                        </span>
                                    </FieldLabel>
                                    <DatePicker
                                        id={`${idPrefix}-desired-date`}
                                        value={desiredPaymentDate || undefined}
                                        onValueChange={(next) =>
                                            setDesiredPaymentDate(next ?? "")
                                        }
                                        placeholder="选择希望付款日"
                                        clearable={false}
                                        disabled={
                                            busy || submitUnknown || !canEdit
                                        }
                                        className="w-full bg-card"
                                    />
                                    {touchedSubmit && dateError ? (
                                        <FieldError
                                            errors={[{ message: dateError }]}
                                        />
                                    ) : null}
                                </Field>
                            </div>

                            {executionMethod === "bank_transfer" ? (
                                <div className="space-y-2.5">
                                    {supplierBankAccounts.length > 0 && (
                                        <Field className="gap-1.5">
                                            <FieldLabel
                                                htmlFor={`${idPrefix}-bank-account-select`}
                                            >
                                                收款账户
                                                <span className="text-destructive">
                                                    *
                                                </span>
                                            </FieldLabel>
                                            <Select
                                                value={bankAccountId}
                                                onValueChange={(next) => {
                                                    if (next)
                                                        setBankAccountId(next)
                                                }}
                                            >
                                                <SelectTrigger
                                                    id={`${idPrefix}-bank-account-select`}
                                                    className="w-full bg-card"
                                                >
                                                    <SelectValue placeholder="选择供应商收款账户">
                                                        {displayedBankAccount
                                                            ? `${displayedBankAccount.account_name} · ${bankDisplayLabel(displayedBankAccount)} · ${displayedBankAccount.account_number_masked}`
                                                            : "选择供应商收款账户"}
                                                    </SelectValue>
                                                </SelectTrigger>
                                                <SelectContent align="start">
                                                    {supplierBankAccounts.map(
                                                        (account) => (
                                                            <SelectItem
                                                                key={
                                                                    account.bank_account_id
                                                                }
                                                                id={`${idPrefix}-bank-option-${toAutomationIdSegment(account.bank_account_id)}`}
                                                                value={
                                                                    account.bank_account_id
                                                                }
                                                            >
                                                                {
                                                                    account.account_name
                                                                }{" "}
                                                                ·{" "}
                                                                {bankDisplayLabel(
                                                                    account,
                                                                )}{" "}
                                                                ·{" "}
                                                                {
                                                                    account.account_number_masked
                                                                }
                                                                {account.bank_routing_number
                                                                    ? ` · 联行号 ${account.bank_routing_number}`
                                                                    : " · 未填联行号"}
                                                            </SelectItem>
                                                        ),
                                                    )}
                                                </SelectContent>
                                            </Select>
                                        </Field>
                                    )}

                                    {displayedBankAccount ? (
                                        <div className="rounded-md border border-border/60 bg-card px-3.5 py-2.5">
                                            <WorkspaceFactGrid columns={2}>
                                                <WorkspaceFactItem
                                                    label="收款户名"
                                                    value={
                                                        displayedBankAccount.account_name
                                                    }
                                                />
                                                <WorkspaceFactItem
                                                    label="开户行"
                                                    value={bankDisplayLabel(
                                                        displayedBankAccount,
                                                    )}
                                                />
                                                <WorkspaceFactItem
                                                    label="收款账号"
                                                    value={
                                                        displayedBankAccount.account_number_masked
                                                    }
                                                    numeric
                                                />
                                                <WorkspaceFactItem
                                                    label="联行号"
                                                    tone={
                                                        routingNumberMissing
                                                            ? "warning"
                                                            : "default"
                                                    }
                                                >
                                                    {liveRoutingNumber ? (
                                                        <span className="num select-all">
                                                            {liveRoutingNumber}
                                                        </span>
                                                    ) : (
                                                        <span>未填写</span>
                                                    )}
                                                </WorkspaceFactItem>
                                            </WorkspaceFactGrid>
                                        </div>
                                    ) : (
                                        <p
                                            role="alert"
                                            className="rounded-md border border-destructive/50 bg-destructive-soft/30 px-3 py-2 text-xs text-destructive-soft-foreground"
                                        >
                                            未找到有效收款账户。请先在供应商档案维护收款账户后点击「刷新账户」。
                                        </p>
                                    )}

                                    {accountError ? (
                                        <p
                                            role="alert"
                                            className="text-xs text-destructive"
                                        >
                                            {accountError}
                                        </p>
                                    ) : null}
                                </div>
                            ) : (
                                <Field
                                    className="gap-1.5"
                                    data-invalid={
                                        (touchedSubmit &&
                                            Boolean(accountError)) ||
                                        undefined
                                    }
                                >
                                    <FieldLabel
                                        htmlFor={`${idPrefix}-deposit-account`}
                                    >
                                        预存款账户
                                        <span className="text-destructive">
                                            *
                                        </span>
                                    </FieldLabel>
                                    <Select
                                        value={depositAccountId}
                                        onValueChange={(next) => {
                                            if (next) setDepositAccountId(next)
                                        }}
                                    >
                                        <SelectTrigger
                                            id={`${idPrefix}-deposit-account`}
                                            className="w-full bg-card"
                                        >
                                            <SelectValue placeholder="选择同付款主体、同供应商预存款账户">
                                                {currentDepositLabel}
                                            </SelectValue>
                                        </SelectTrigger>
                                        <SelectContent align="start">
                                            {depositAccounts.map((account) => (
                                                <SelectItem
                                                    key={account.id}
                                                    id={`${idPrefix}-deposit-${toAutomationIdSegment(account.id)}`}
                                                    value={account.id}
                                                >
                                                    {account.label}
                                                </SelectItem>
                                            ))}
                                        </SelectContent>
                                    </Select>
                                    {touchedSubmit && accountError ? (
                                        <FieldError
                                            errors={[{ message: accountError }]}
                                        />
                                    ) : null}
                                </Field>
                            )}
                        </div>
                    )}
                </WorkspaceFactGroup>

                {/* 4. 分区二：来源采购单与申请金额（默认只读紧凑表格 + 异常时自动亮出核销/占用依据） */}
                <WorkspaceFactGroup
                    id={`${idPrefix}-section-lines`}
                    divider
                    title={
                        <span className="inline-flex flex-wrap items-center gap-2">
                            <span>来源采购单与申请金额</span>
                            <span className="num text-xs font-normal text-muted-foreground">
                                共 {resolvedLines.length} 项
                            </span>
                            {linesSectionChanged && <EditedPill />}
                        </span>
                    }
                    actions={
                        <>
                            {editingSections.lines &&
                                addableCandidates.length > 0 &&
                                canEdit && (
                                    <Select
                                        value=""
                                        onValueChange={(payableId) => {
                                            if (!payableId) return
                                            const candidate =
                                                candidateByPayableId.get(
                                                    payableId,
                                                )
                                            if (!candidate) return
                                            setLines((prev) => [
                                                ...prev,
                                                {
                                                    payableAccountId: payableId,
                                                    amount: candidate.available_amount,
                                                },
                                            ])
                                        }}
                                    >
                                        <SelectTrigger
                                            id={`${idPrefix}-add-po`}
                                            size="sm"
                                            className="w-auto min-w-36 text-xs"
                                        >
                                            <PlusIcon
                                                aria-hidden="true"
                                                className="size-3.5"
                                            />
                                            <SelectValue placeholder="添加同供应商采购单" />
                                        </SelectTrigger>
                                        <SelectContent align="end">
                                            {addableCandidates.map(
                                                (
                                                    candidate: PaymentCandidate,
                                                ) => (
                                                    <SelectItem
                                                        key={
                                                            candidate.payable_account_id
                                                        }
                                                        id={`${idPrefix}-add-po-${toAutomationIdSegment(candidate.payable_account_id)}`}
                                                        value={
                                                            candidate.payable_account_id
                                                        }
                                                    >
                                                        {
                                                            candidate.purchase_order_no
                                                        }{" "}
                                                        · 可申请{" "}
                                                        {formatMoney(
                                                            candidate.available_amount,
                                                        )}
                                                    </SelectItem>
                                                ),
                                            )}
                                        </SelectContent>
                                    </Select>
                                )}
                            {canEdit && (
                                <Button
                                    id={`${idPrefix}-toggle-edit-lines`}
                                    type="button"
                                    variant={
                                        editingSections.lines
                                            ? "secondary"
                                            : "ghost"
                                    }
                                    size="xs"
                                    className={cn(
                                        !editingSections.lines &&
                                            detailWorkspaceStyles.quietButton,
                                    )}
                                    onClick={() => toggleSectionEdit("lines")}
                                >
                                    {!editingSections.lines && (
                                        <PencilLineIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                    )}
                                    {editingSections.lines
                                        ? "完成修改"
                                        : isPremiseInvalidated &&
                                            addableCandidates.length > 0
                                          ? "更换采购单"
                                          : "修改金额 / 换单"}
                                </Button>
                            )}
                        </>
                    }
                >
                    <Table className="w-full table-fixed text-body-compact">
                        <TableHeader className="bg-surface-sunken text-xs text-muted-foreground [&_tr]:border-b-0 [&_th]:bg-surface-sunken [&_th]:hover:bg-surface-sunken [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md">
                            <TableRow>
                                <TableHead
                                    scope="col"
                                    className="w-[46%] px-2.5 py-2 text-left font-medium"
                                >
                                    采购单 / 履约
                                </TableHead>
                                <TableHead
                                    scope="col"
                                    className="px-2.5 py-2 text-right font-medium"
                                >
                                    可申请余额
                                </TableHead>
                                <TableHead
                                    scope="col"
                                    className="w-40 px-2.5 py-2 text-right font-medium"
                                >
                                    本次申请金额
                                </TableHead>
                                {editingSections.lines &&
                                    resolvedLines.length > 1 && (
                                        <TableHead
                                            scope="col"
                                            className="w-12 px-1 py-2 text-right font-medium"
                                        >
                                            <span className="sr-only">
                                                移除
                                            </span>
                                        </TableHead>
                                    )}
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {resolvedLines.map((row) => {
                                const seg = toAutomationIdSegment(
                                    row.entry.payableAccountId,
                                )
                                const isOpen =
                                    openSourceId === row.entry.payableAccountId
                                const showInlineError =
                                    Boolean(row.amountError) &&
                                    (editingSections.lines ||
                                        touchedSubmit ||
                                        !isPremiseInvalidated)
                                const colSpan =
                                    editingSections.lines &&
                                    resolvedLines.length > 1
                                        ? 4
                                        : 3

                                return (
                                    <Fragment key={row.entry.payableAccountId}>
                                        <TableRow
                                            className={cn(
                                                (showInlineError || isOpen) &&
                                                    "border-b-0",
                                            )}
                                        >
                                            <TableHead
                                                scope="row"
                                                className="px-2.5 py-2.5 text-left font-normal whitespace-normal"
                                            >
                                                <Button
                                                    id={`${idPrefix}-line-toggle-${seg}`}
                                                    type="button"
                                                    variant="ghost"
                                                    size="xs"
                                                    aria-expanded={isOpen}
                                                    disabled={!row.origLine}
                                                    className="-ml-1.5 h-auto max-w-full justify-start gap-1 px-1.5 py-0.5 text-left font-medium text-foreground"
                                                    onClick={() =>
                                                        setOpenSourceId(
                                                            isOpen
                                                                ? undefined
                                                                : row.entry
                                                                      .payableAccountId,
                                                        )
                                                    }
                                                >
                                                    <ChevronRightIcon
                                                        aria-hidden="true"
                                                        className={cn(
                                                            "size-3.5 shrink-0 text-muted-foreground transition-transform",
                                                            isOpen &&
                                                                "rotate-90",
                                                        )}
                                                    />
                                                    <span className="num truncate">
                                                        {row.purchaseOrderNo}
                                                    </span>
                                                </Button>
                                                <span className="num block pl-5 text-tiny text-muted-foreground">
                                                    {row.purchaseVersion != null
                                                        ? `V${row.purchaseVersion}`
                                                        : ""}
                                                    {row.fulfillment
                                                        ? `${row.purchaseVersion != null ? " · " : ""}${fulfillmentLabel[row.fulfillment] ?? "履约待核对"}`
                                                        : ""}
                                                    {row.paymentTermName
                                                        ? ` · ${row.paymentTermName}`
                                                        : ""}
                                                </span>
                                            </TableHead>
                                            <TableCell className="px-2.5 py-2.5 text-right whitespace-normal">
                                                <MoneyValue
                                                    display="exact"
                                                    value={row.available}
                                                    className={cn(
                                                        row.noBalance &&
                                                            "text-destructive",
                                                    )}
                                                />
                                            </TableCell>
                                            <TableCell className="px-2.5 py-2 text-right">
                                                {editingSections.lines ? (
                                                    <Input
                                                        id={`${idPrefix}-line-amount-${seg}`}
                                                        aria-label={`${row.purchaseOrderNo} 本次申请金额`}
                                                        type="number"
                                                        inputMode="decimal"
                                                        min="0.01"
                                                        step="0.01"
                                                        value={row.entry.amount}
                                                        disabled={
                                                            busy ||
                                                            submitUnknown ||
                                                            !canEdit
                                                        }
                                                        aria-invalid={
                                                            showInlineError ||
                                                            undefined
                                                        }
                                                        className="num h-control-sm text-right"
                                                        onChange={(event) => {
                                                            const nextValue =
                                                                event.target
                                                                    .value
                                                            setLines((prev) =>
                                                                prev.map(
                                                                    (item) =>
                                                                        item.payableAccountId ===
                                                                        row
                                                                            .entry
                                                                            .payableAccountId
                                                                            ? {
                                                                                  ...item,
                                                                                  amount: nextValue,
                                                                              }
                                                                            : item,
                                                                ),
                                                            )
                                                        }}
                                                    />
                                                ) : (
                                                    <MoneyValue
                                                        display="exact"
                                                        value={row.entry.amount}
                                                        className="font-medium text-foreground"
                                                    />
                                                )}
                                            </TableCell>
                                            {editingSections.lines &&
                                                resolvedLines.length > 1 && (
                                                    <TableCell className="px-1 py-2 text-right">
                                                        <Button
                                                            id={`${idPrefix}-line-remove-${seg}`}
                                                            type="button"
                                                            variant="ghost"
                                                            size="icon-xs"
                                                            aria-label={`移除采购单 ${row.purchaseOrderNo}`}
                                                            className="text-muted-foreground hover:text-destructive"
                                                            onClick={() =>
                                                                setLines(
                                                                    (prev) =>
                                                                        prev.filter(
                                                                            (
                                                                                item,
                                                                            ) =>
                                                                                item.payableAccountId !==
                                                                                row
                                                                                    .entry
                                                                                    .payableAccountId,
                                                                        ),
                                                                )
                                                            }
                                                        >
                                                            <Trash2Icon aria-hidden="true" />
                                                        </Button>
                                                    </TableCell>
                                                )}
                                        </TableRow>

                                        {showInlineError && row.amountError && (
                                            <TableRow className="border-b border-border/60 hover:bg-transparent">
                                                <TableCell
                                                    colSpan={colSpan}
                                                    className="pt-0 pb-2.5"
                                                >
                                                    <div className="flex flex-wrap items-center justify-between gap-2 rounded-r-md border-l-2 border-l-destructive bg-destructive-soft/45 px-2.5 py-1.5 text-xs text-destructive-soft-foreground">
                                                        <span>
                                                            {row.amountError}
                                                        </span>
                                                        {!row.noBalance &&
                                                            canEdit && (
                                                                <Button
                                                                    id={`${idPrefix}-line-clamp-${seg}`}
                                                                    type="button"
                                                                    variant="ghost"
                                                                    size="xs"
                                                                    className="h-5 px-1.5 text-xs text-destructive-soft-foreground hover:bg-destructive-soft"
                                                                    onClick={() => {
                                                                        setEditingSections(
                                                                            (
                                                                                prev,
                                                                            ) => ({
                                                                                ...prev,
                                                                                lines: true,
                                                                            }),
                                                                        )
                                                                        setLines(
                                                                            (
                                                                                prev,
                                                                            ) =>
                                                                                prev.map(
                                                                                    (
                                                                                        item,
                                                                                    ) =>
                                                                                        item.payableAccountId ===
                                                                                        row
                                                                                            .entry
                                                                                            .payableAccountId
                                                                                            ? {
                                                                                                  ...item,
                                                                                                  amount: row.available,
                                                                                              }
                                                                                            : item,
                                                                                ),
                                                                        )
                                                                    }}
                                                                >
                                                                    填满可用额{" "}
                                                                    {formatMoney(
                                                                        row.available,
                                                                    )}
                                                                </Button>
                                                            )}
                                                    </div>
                                                </TableCell>
                                            </TableRow>
                                        )}

                                        {isOpen && row.origLine && (
                                            <TableRow className="border-b border-border/60 hover:bg-transparent">
                                                <TableCell
                                                    colSpan={colSpan}
                                                    className="px-3 py-2 text-xs whitespace-normal"
                                                >
                                                    <p className="text-muted-foreground">
                                                        采购总额{" "}
                                                        <MoneyValue
                                                            value={row.total}
                                                            display="exact"
                                                        />{" "}
                                                        · 已结算{" "}
                                                        <MoneyValue
                                                            value={row.settled}
                                                            display="exact"
                                                        />
                                                    </p>
                                                    <ul className="mt-1.5 divide-y divide-border/60">
                                                        {row.origLine.purchase_items.map(
                                                            (item) => (
                                                                <li
                                                                    key={
                                                                        item.id
                                                                    }
                                                                    className="grid grid-cols-[minmax(0,1fr)_auto] gap-2 py-1"
                                                                >
                                                                    <span className="wrap-anywhere">
                                                                        {
                                                                            item.item_name
                                                                        }
                                                                        {item.spec && (
                                                                            <span className="ml-2 text-muted-foreground">
                                                                                {
                                                                                    item.spec
                                                                                }
                                                                            </span>
                                                                        )}
                                                                    </span>
                                                                    <span className="num inline-flex items-baseline gap-1 text-right">
                                                                        <QuantityValue
                                                                            value={
                                                                                item.quantity
                                                                            }
                                                                            unit={
                                                                                item.unit ??
                                                                                ""
                                                                            }
                                                                        />
                                                                        <span>
                                                                            ×
                                                                        </span>
                                                                        <MoneyValue
                                                                            display="exact"
                                                                            value={
                                                                                item.unit_price
                                                                            }
                                                                        />
                                                                        <span>
                                                                            ·
                                                                        </span>
                                                                        <MoneyValue
                                                                            display="exact"
                                                                            value={
                                                                                item.amount
                                                                            }
                                                                        />
                                                                    </span>
                                                                </li>
                                                            ),
                                                        )}
                                                    </ul>
                                                </TableCell>
                                            </TableRow>
                                        )}
                                    </Fragment>
                                )
                            })}
                        </TableBody>
                        <TableFooter className="border-0 bg-surface-sunken [&_th]:bg-surface-sunken [&_td:last-child]:rounded-r-md [&_th:first-child]:rounded-l-md">
                            <TableRow>
                                <TableHead
                                    scope="row"
                                    className="px-2.5 py-2 text-left text-xs font-medium"
                                >
                                    整单合计
                                </TableHead>
                                <TableCell className="px-2.5 py-2 text-right">
                                    <MoneyValue
                                        display="exact"
                                        value={liveAvailableTotal}
                                    />
                                </TableCell>
                                <TableCell className="px-2.5 py-2 text-right font-semibold">
                                    <MoneyValue
                                        display="exact"
                                        value={liveTotalAmount}
                                    />
                                </TableCell>
                                {editingSections.lines &&
                                    resolvedLines.length > 1 && <TableCell />}
                            </TableRow>
                        </TableFooter>
                    </Table>

                    <div>
                        <Button
                            id={`${idPrefix}-payment-history-toggle`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={detailWorkspaceStyles.quietButton}
                            aria-expanded={paymentHistoryOpen}
                            aria-controls={`${idPrefix}-payment-history`}
                            onClick={() =>
                                setHistoryToggled(!paymentHistoryOpen)
                            }
                        >
                            <ChevronRightIcon
                                aria-hidden="true"
                                className={cn(
                                    "transition-transform",
                                    paymentHistoryOpen && "rotate-90",
                                )}
                            />
                            付款核销与其他申请占用
                            {historyRecordCount > 0 && (
                                <span className="num text-muted-foreground">
                                    （{historyRecordCount} 条）
                                </span>
                            )}
                        </Button>
                        <div
                            id={`${idPrefix}-payment-history`}
                            hidden={!paymentHistoryOpen}
                        >
                            {paymentHistoryOpen && (
                                <RequestReviewHistory request={request} />
                            )}
                        </div>
                    </div>
                </WorkspaceFactGroup>

                {/* 5. 分区三：申请说明与附件（默认只读紧缩，按需点「修改」再展开大输入框与上传器） */}
                <WorkspaceFactGroup
                    id={`${idPrefix}-section-details`}
                    divider
                    title={
                        <span className="inline-flex flex-wrap items-center gap-2">
                            <span>申请说明与附件</span>
                            {detailsSectionChanged && <EditedPill />}
                        </span>
                    }
                    actions={
                        canEdit ? (
                            <Button
                                id={`${idPrefix}-toggle-edit-details`}
                                type="button"
                                variant={
                                    editingSections.details
                                        ? "secondary"
                                        : "ghost"
                                }
                                size="xs"
                                className={cn(
                                    !editingSections.details &&
                                        detailWorkspaceStyles.quietButton,
                                )}
                                onClick={() => toggleSectionEdit("details")}
                            >
                                {!editingSections.details && (
                                    <PencilLineIcon
                                        data-icon="inline-start"
                                        aria-hidden="true"
                                    />
                                )}
                                {editingSections.details ? "完成修改" : "修改"}
                            </Button>
                        ) : undefined
                    }
                >
                    {!editingSections.details ? (
                        <WorkspaceFactGrid
                            columns={2}
                            ariaLabel="申请说明与附件只读核验"
                        >
                            <WorkspaceFactItem
                                label="申请说明"
                                value={description}
                                span="full"
                            />
                            <WorkspaceFactItem
                                label={`本申请附件（${keptAttachmentIds.length + newFiles.length}）`}
                                span="full"
                            >
                                {keptAttachmentIds.length === 0 &&
                                newFiles.length === 0 ? (
                                    <span className="inline-flex items-center gap-2 font-normal text-muted-foreground">
                                        <span>暂无本申请附件</span>
                                        {canEdit && (
                                            <Button
                                                id={`${idPrefix}-quick-add-att`}
                                                type="button"
                                                variant="ghost"
                                                size="xs"
                                                className={
                                                    detailWorkspaceStyles.quietButton
                                                }
                                                onClick={() =>
                                                    openSectionForEdit(
                                                        "details",
                                                    )
                                                }
                                            >
                                                <PlusIcon
                                                    data-icon="inline-start"
                                                    aria-hidden="true"
                                                />
                                                补充附件
                                            </Button>
                                        )}
                                    </span>
                                ) : (
                                    <ul className="divide-y divide-border/60 rounded-lg border border-border/60 font-normal">
                                        {keptAttachmentIds.map((fileId) => {
                                            const file =
                                                ownAttachmentMap.get(fileId)
                                            const seg =
                                                toAutomationIdSegment(fileId)
                                            return (
                                                <li
                                                    key={fileId}
                                                    className="flex flex-wrap items-center justify-between gap-2 px-3 py-1.5 text-body-compact"
                                                >
                                                    <span className="min-w-0 flex-1 truncate">
                                                        {file?.file_name ??
                                                            fileId}
                                                    </span>
                                                    <div className="flex shrink-0 items-center gap-1">
                                                        {onSelectFile && (
                                                            <Button
                                                                id={`${idPrefix}-att-preview-${seg}`}
                                                                type="button"
                                                                variant="ghost"
                                                                size="xs"
                                                                onClick={() =>
                                                                    onSelectFile(
                                                                        fileId,
                                                                    )
                                                                }
                                                            >
                                                                <EyeIcon
                                                                    data-icon="inline-start"
                                                                    aria-hidden="true"
                                                                />
                                                                对照
                                                            </Button>
                                                        )}
                                                        {file && (
                                                            <Button
                                                                id={`${idPrefix}-att-download-${seg}`}
                                                                type="button"
                                                                variant="ghost"
                                                                size="xs"
                                                                disabled={
                                                                    downloadMutation.isPending
                                                                }
                                                                onClick={() =>
                                                                    void downloadMutation
                                                                        .mutateAsync(
                                                                            {
                                                                                requestId:
                                                                                    request.id,
                                                                                attachment:
                                                                                    file,
                                                                            },
                                                                        )
                                                                        .catch(
                                                                            () =>
                                                                                undefined,
                                                                        )
                                                                }
                                                            >
                                                                <DownloadIcon
                                                                    data-icon="inline-start"
                                                                    aria-hidden="true"
                                                                />
                                                                下载
                                                            </Button>
                                                        )}
                                                    </div>
                                                </li>
                                            )
                                        })}
                                        {newFiles.map((file) => (
                                            <li
                                                key={selectedFileKey(file)}
                                                className="flex flex-wrap items-center justify-between gap-2 bg-info-soft/25 px-3 py-1.5 text-body-compact"
                                            >
                                                <span className="min-w-0 flex-1 truncate">
                                                    {file.name}
                                                </span>
                                                <EditedPill label="待提交上传" />
                                            </li>
                                        ))}
                                    </ul>
                                )}
                            </WorkspaceFactItem>
                        </WorkspaceFactGrid>
                    ) : (
                        <div className="space-y-4 rounded-lg border border-border/60 bg-surface-sunken/35 p-3.5">
                            <Field
                                className="gap-1.5"
                                data-invalid={
                                    (touchedSubmit &&
                                        Boolean(descriptionError)) ||
                                    undefined
                                }
                            >
                                <div className="flex items-baseline justify-between gap-2">
                                    <FieldLabel
                                        htmlFor={`${idPrefix}-description`}
                                    >
                                        申请说明
                                        <span className="text-destructive">
                                            *
                                        </span>
                                    </FieldLabel>
                                    <span className="num text-tiny text-muted-foreground">
                                        {descriptionCharCount} /{" "}
                                        {DESCRIPTION_MAX_CHARS}
                                    </span>
                                </div>
                                <Textarea
                                    id={`${idPrefix}-description`}
                                    rows={2}
                                    value={description}
                                    placeholder="说明本次付款安排及依据"
                                    className="bg-card"
                                    aria-invalid={
                                        (touchedSubmit &&
                                            Boolean(descriptionError)) ||
                                        undefined
                                    }
                                    onChange={(event) =>
                                        setDescription(event.target.value)
                                    }
                                />
                                {touchedSubmit && descriptionError ? (
                                    <FieldError
                                        errors={[{ message: descriptionError }]}
                                    />
                                ) : null}
                            </Field>

                            <div className="space-y-2.5">
                                <span className="block text-xs font-medium text-muted-foreground">
                                    本申请附件
                                </span>

                                {keptAttachmentIds.length > 0 && (
                                    <ul className="divide-y divide-border/60 rounded-lg border border-border/60 bg-card">
                                        {keptAttachmentIds.map((fileId) => {
                                            const file =
                                                ownAttachmentMap.get(fileId)
                                            const seg =
                                                toAutomationIdSegment(fileId)
                                            return (
                                                <li
                                                    key={fileId}
                                                    className="flex flex-wrap items-center justify-between gap-2 px-3 py-2 text-body-compact"
                                                >
                                                    <span className="min-w-0 flex-1 truncate">
                                                        {file?.file_name ??
                                                            fileId}
                                                    </span>
                                                    <div className="flex shrink-0 items-center gap-1">
                                                        {onSelectFile && (
                                                            <Button
                                                                id={`${idPrefix}-att-edit-preview-${seg}`}
                                                                type="button"
                                                                variant="ghost"
                                                                size="xs"
                                                                onClick={() =>
                                                                    onSelectFile(
                                                                        fileId,
                                                                    )
                                                                }
                                                            >
                                                                <EyeIcon
                                                                    data-icon="inline-start"
                                                                    aria-hidden="true"
                                                                />
                                                                对照
                                                            </Button>
                                                        )}
                                                        <Button
                                                            id={`${idPrefix}-att-remove-${seg}`}
                                                            type="button"
                                                            variant="ghost"
                                                            size="xs"
                                                            className="text-muted-foreground hover:text-destructive"
                                                            onClick={() =>
                                                                setKeptAttachmentIds(
                                                                    (prev) =>
                                                                        prev.filter(
                                                                            (
                                                                                id,
                                                                            ) =>
                                                                                id !==
                                                                                fileId,
                                                                        ),
                                                                )
                                                            }
                                                        >
                                                            <Trash2Icon
                                                                data-icon="inline-start"
                                                                aria-hidden="true"
                                                            />
                                                            移除
                                                        </Button>
                                                    </div>
                                                </li>
                                            )
                                        })}
                                    </ul>
                                )}

                                {removedAttachmentIds.length > 0 && (
                                    <div className="flex flex-wrap items-center justify-between gap-2 rounded-md bg-surface-sunken px-3 py-1.5 text-xs text-muted-foreground">
                                        <span>
                                            已标记移除{" "}
                                            <span className="num">
                                                {removedAttachmentIds.length}
                                            </span>{" "}
                                            份原附件（保存后生效）
                                        </span>
                                        <Button
                                            id={`${idPrefix}-att-restore-all`}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            onClick={() =>
                                                setKeptAttachmentIds([
                                                    ...request.attachment_ids,
                                                ])
                                            }
                                        >
                                            <RotateCcwIcon
                                                data-icon="inline-start"
                                                aria-hidden="true"
                                            />
                                            恢复原附件
                                        </Button>
                                    </div>
                                )}

                                <FileUpload
                                    selectedDescription={
                                        fileUploadText.pendingSave
                                    }
                                    idPrefix={`${idPrefix}-new-attachments`}
                                    inputId={`${idPrefix}-new-attachments-input`}
                                    selectId={`${idPrefix}-new-attachments-trigger`}
                                    density="button"
                                    label="补充新附件"
                                    selectLabel="补充附件"
                                    description="选填 · PDF / 图片，每份不超过 5 MB；右侧「附件对照」可随时对照历史材料"
                                    multiple
                                    accept="application/pdf,image/jpeg,image/png,image/webp"
                                    maxFileSize={5 * 1024 * 1024}
                                    selectedFiles={newFiles}
                                    onFileRemove={(file) =>
                                        setNewFiles((prev) =>
                                            prev.filter(
                                                (item) =>
                                                    selectedFileKey(item) !==
                                                    selectedFileKey(file),
                                            ),
                                        )
                                    }
                                    onFilesSelected={(selectedList) => {
                                        setNewFiles((prev) => {
                                            const map = new Map(
                                                prev.map((file) => [
                                                    selectedFileKey(file),
                                                    file,
                                                ]),
                                            )
                                            for (const file of selectedList) {
                                                map.set(
                                                    selectedFileKey(file),
                                                    file,
                                                )
                                            }
                                            return Array.from(map.values())
                                        })
                                    }}
                                />
                            </div>
                        </div>
                    )}
                </WorkspaceFactGroup>
            </fieldset>

            {submitError && !resubmitDialogOpen && (
                <div
                    role="alert"
                    className="flex items-start gap-2 rounded-r-md border-l-2 border-l-destructive bg-destructive-soft/50 px-3.5 py-2.5 text-xs text-destructive-soft-foreground"
                >
                    <CircleAlertIcon
                        aria-hidden="true"
                        className="mt-0.5 size-3.5 shrink-0 text-destructive"
                    />
                    <p>{submitError}</p>
                </div>
            )}

            {/* 6. 重提确认收口弹窗（集中展示本轮 Diff、回复审批人、快捷短语与审批路线） */}
            <FormalActionConfirmDialog
                idPrefix={`${idPrefix}-resubmit-confirm`}
                open={resubmitDialogOpen}
                onOpenChange={(open) => {
                    if (!busy) setResubmitDialogOpen(open)
                }}
                pending={busy}
                title="重新提交付款申请审批"
                actionLabel="重新提交"
                confirmLabel={
                    hasFormChanges ? "保存并确认重提" : "确认重新提交"
                }
                cancelLabel="返回核对"
                fromStatus={{
                    label: requestStatusLabel[request.status],
                    tone: requestStatusTone[request.status],
                }}
                toStatus={{ label: "待审核", tone: "info" }}
                summary={[
                    <span key="amount">
                        申请总额{" "}
                        <MoneyValue value={liveTotalAmount} display="exact" />
                    </span>,
                    <span key="supplier">供应商：{request.supplier_name}</span>,
                    <span key="date">
                        希望付款日：
                        <span className="num">{desiredPaymentDate}</span>
                    </span>,
                ]}
                confirmDisabled={
                    busy ||
                    hasDraftValidationError ||
                    Boolean(resubmissionReasonError)
                }
                formContent={
                    <div className="space-y-3.5">
                        {rejection?.reason && (
                            <div className="rounded-r-md border-l-2 border-l-destructive bg-destructive-soft/35 px-3 py-2 text-xs">
                                <p className="text-muted-foreground">
                                    上轮驳回
                                    {rejection.rejector
                                        ? `（${rejection.rejector}）`
                                        : ""}
                                </p>
                                <p className="mt-0.5 font-medium text-foreground whitespace-pre-wrap wrap-anywhere">
                                    {rejection.reason}
                                </p>
                            </div>
                        )}

                        {routingNumberMissing && (
                            <div className="rounded-r-md border-l-2 border-l-warning bg-warning-soft/50 px-3 py-2 text-xs text-warning-soft-foreground">
                                提示：当前收款账户仍未填写联行号，确认是否仍需按现有账户资料提交。
                            </div>
                        )}

                        <div className="space-y-1.5">
                            <p className="text-xs font-medium text-muted-foreground">
                                对比 V{request.submission_version} 本次变更
                            </p>
                            {resubmissionDiffs.length > 0 ? (
                                <div className="flex flex-wrap gap-1.5">
                                    {resubmissionDiffs.map((diff) => (
                                        <span
                                            key={diff.label}
                                            className="inline-flex flex-wrap items-baseline gap-1 rounded-md bg-info-soft px-2 py-1 text-xs text-info-foreground"
                                        >
                                            <span className="font-medium">
                                                {diff.label}：
                                            </span>
                                            {diff.before && (
                                                <>
                                                    <span
                                                        className={cn(
                                                            "line-through opacity-75",
                                                            diff.numeric &&
                                                                "num",
                                                        )}
                                                    >
                                                        {diff.before}
                                                    </span>
                                                    <span aria-hidden="true">
                                                        →
                                                    </span>
                                                </>
                                            )}
                                            <span
                                                className={cn(
                                                    "font-semibold",
                                                    diff.numeric && "num",
                                                )}
                                            >
                                                {diff.after}
                                            </span>
                                        </span>
                                    ))}
                                </div>
                            ) : (
                                <p className="text-xs text-muted-foreground">
                                    表单字段未变更，将按当前单据及最新主数据重新提交审批。
                                </p>
                            )}
                        </div>

                        {requiresResubmissionReason && (
                            <div className="space-y-2">
                                <ReasonTextareaField
                                    id={`${idPrefix}-resubmission-reason`}
                                    name="resubmission_reason"
                                    mode="utf8-bytes"
                                    max={BPM_REASON_MAX_UTF8_BYTES}
                                    label="回复审批人（重新提交说明）"
                                    placeholder="说明本次修改或重提依据（例如：已补充供应商联行号，请复核）"
                                    rows={2}
                                    required
                                    value={resubmissionReason}
                                    onChange={setResubmissionReason}
                                    disabled={busy || submitUnknown}
                                    error={
                                        touchedSubmit
                                            ? resubmissionReasonError
                                            : undefined
                                    }
                                    invalid={Boolean(
                                        touchedSubmit &&
                                        resubmissionReasonError,
                                    )}
                                    onCtrlEnterSubmit={() => {
                                        if (
                                            !busy &&
                                            !submitUnknown &&
                                            !resubmissionReasonError
                                        ) {
                                            void handleConfirmResubmit().catch(
                                                () => undefined,
                                            )
                                        }
                                    }}
                                />
                                <div className="flex flex-wrap items-center gap-1.5">
                                    {quickReplyPhrases.map((phrase, index) => (
                                        <Button
                                            key={phrase}
                                            id={`${idPrefix}-quick-reply-${index}`}
                                            type="button"
                                            variant="outline"
                                            size="xs"
                                            disabled={busy}
                                            className="h-6 rounded-full px-2.5 text-tiny font-normal text-muted-foreground hover:text-foreground"
                                            onClick={() =>
                                                setResubmissionReason(phrase)
                                            }
                                        >
                                            {phrase}
                                        </Button>
                                    ))}
                                </div>
                            </div>
                        )}

                        <SubmissionRouteConfirmation
                            documentType="purchase_payment_request"
                            definition={approval?.definition}
                        />
                    </div>
                }
                effects={[
                    "重提后将从第一个审批节点重新审批，并锁定本次申请金额与最新收款账户。",
                ]}
                onConfirm={handleConfirmResubmit}
                onConfirmError={() => undefined}
            />

            {/* 7. 底部吸底操作栏：永不卡死的清晰双出口 */}
            <WorkspaceTaskFooter>
                {submitUnknown ? (
                    <div className="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-2">
                        <div
                            role="alert"
                            className="flex min-w-0 flex-[1_1_16rem] items-start gap-2 rounded-r-md border-l-2 border-l-warning bg-warning-soft/50 px-3 py-1.5 text-xs text-warning-soft-foreground"
                        >
                            <CircleAlertIcon
                                aria-hidden="true"
                                className="mt-0.5 size-3.5 shrink-0"
                            />
                            <p>
                                <strong className="font-semibold">
                                    重新提交结果待确认。
                                </strong>
                                点击核对提交结果，将沿用原提交凭证重试，不会重复发起审批。
                            </p>
                        </div>
                        <Button
                            id={`${idPrefix}-unknown-refresh`}
                            type="button"
                            variant="outline"
                            size="sm"
                            disabled={busy || refreshing}
                            onClick={() => void refreshLiveData()}
                        >
                            {refreshing ? "正在刷新…" : "刷新最新数据"}
                        </Button>
                        <Button
                            id={`${idPrefix}-unknown-recover`}
                            type="button"
                            variant="default"
                            size="sm"
                            disabled={busy}
                            onClick={() =>
                                void handleConfirmResubmit().catch(
                                    () => undefined,
                                )
                            }
                        >
                            {busy ? "正在核对结果…" : "核对提交结果"}
                        </Button>
                    </div>
                ) : isPremiseInvalidated ? (
                    <div className="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-2">
                        <span className="mr-auto hidden text-xs text-destructive @[640px]/workspace:inline">
                            来源采购单余额为 ¥0.00，建议核对后放弃本次办理
                        </span>

                        {addableCandidates.length > 0 && canEdit && (
                            <Button
                                id={`${idPrefix}-footer-switch-po`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={busy}
                                onClick={() => openSectionForEdit("lines")}
                            >
                                更换采购单
                            </Button>
                        )}

                        {canEdit && hasFormChanges && (
                            <Button
                                id={`${idPrefix}-save-draft`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={busy || hasSaveValidationError}
                                onClick={() => void handleSaveDraft()}
                            >
                                {saveMutation.isPending
                                    ? "正在暂存…"
                                    : "暂存修改"}
                            </Button>
                        )}

                        {renderAbandonAction?.("default")}
                    </div>
                ) : (
                    <div className="flex min-w-0 flex-1 flex-wrap items-center justify-end gap-2">
                        <span className="mr-auto hidden text-xs text-muted-foreground @[640px]/workspace:inline">
                            申请总额{" "}
                            <MoneyValue
                                value={liveTotalAmount}
                                display="exact"
                                className="font-medium text-foreground"
                            />
                            {changedFormSectionCount > 0
                                ? ` · 已修改 ${changedFormSectionCount} 个分区`
                                : ""}
                            {hasBalanceBlock ? (
                                <span className="ml-2 text-destructive">
                                    · 超出可用余额
                                </span>
                            ) : routingNumberMissing ? (
                                <span className="ml-2 text-warning-soft-foreground">
                                    · 尚未填写联行号
                                </span>
                            ) : null}
                        </span>

                        {renderAbandonAction?.("outline")}

                        {canEdit && hasFormChanges && (
                            <Button
                                id={`${idPrefix}-save-draft`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={busy || hasSaveValidationError}
                                onClick={() => void handleSaveDraft()}
                            >
                                {saveMutation.isPending
                                    ? "正在暂存…"
                                    : "暂存修改"}
                            </Button>
                        )}

                        {canSubmit && (
                            <Button
                                id={`${idPrefix}-resubmit`}
                                type="button"
                                variant="default"
                                size="sm"
                                disabled={
                                    busy ||
                                    hasBalanceBlock ||
                                    Boolean(accountError)
                                }
                                onClick={handleOpenResubmitDialog}
                            >
                                {primaryResubmitLabel}
                            </Button>
                        )}
                    </div>
                )}
            </WorkspaceTaskFooter>
        </div>
    )
}
