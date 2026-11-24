"use client"

import {
    Fragment,
    useEffect,
    useMemo,
    useRef,
    useState,
    type ReactNode,
} from "react"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import {
    AlertTriangleIcon,
    ArrowUpRightIcon,
    CheckCircle2Icon,
    ChevronRightIcon,
    CircleAlertIcon,
    DownloadIcon,
    EyeIcon,
    PlusIcon,
    RefreshCwIcon,
    RotateCcwIcon,
    Trash2Icon,
} from "lucide-react"
import {
    MoneyValue,
    QuantityValue,
    RateValue,
    WorkspaceTaskFooter,
    detailWorkspaceStyles,
} from "@/components/business"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
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
import { positive, totalOf } from "../lib/draft"
import { fulfillmentLabel, paymentReviewBasis } from "../lib/review"
import type {
    DepositAccountOption,
    PaymentCandidate,
    PaymentExecutionMethod,
    PaymentRecipient,
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

type ReworkWorkspaceProps = {
    request: PaymentRequest
    idPrefix: string
    onSelectFile?: (fileId: string) => void
    onTaskCompleted?: () => void
    canAbandon?: boolean
    abandonAction?: ReactNode
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
    abandonAction,
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

    // 表单可编辑状态（直接承接原单据，支持零跳转原地返工）
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
    const [resubmissionReason, setResubmissionReason] = useState("")
    const [choosingAccount, setChoosingAccount] = useState(false)
    const [openSourceId, setOpenSourceId] = useState<string>()
    const [paymentHistoryOpen, setPaymentHistoryOpen] = useState(false)
    const [touchedSubmit, setTouchedSubmit] = useState(false)
    const [submitError, setSubmitError] = useState<string | null>(null)
    const [submitUnknown, setSubmitUnknown] = useState(false)
    const [refreshing, setRefreshing] = useState(false)

    const uploadedFilesRef = useRef(new Map<File, string>())
    const submitIdentityRef = useRef<RequestCommand | null>(null)
    const submittingRef = useRef(false)

    // 候选来源与供应商最新账户（含联行号与主数据版本）
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
        const list = candidatesQuery.data ?? []
        const map = new Map<string, PaymentRecipient>()
        for (const candidate of list) {
            if (candidate.supplier_id !== request.supplier_id) continue
            for (const account of candidate.bank_accounts) {
                if (!map.has(account.bank_account_id)) {
                    map.set(account.bank_account_id, account)
                }
            }
        }
        return Array.from(map.values())
    }, [candidatesQuery.data, request.supplier_id])

    const depositAccounts = useMemo<DepositAccountOption[]>(() => {
        const map = new Map<string, DepositAccountOption>()
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
    }, [matchingCandidates, request.deposit_account])

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

    const routingNumberMissing = Boolean(
        executionMethod === "bank_transfer" &&
        liveSelectedBankAccount &&
        !liveSelectedBankAccount.bank_routing_number?.trim(),
    )

    // 每行采购单的实时余额与校验状态
    const originalLineByPayableId = useMemo(
        () =>
            new Map(
                request.lines.map((line) => [line.payable_account_id, line]),
            ),
        [request.lines],
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
    }, [candidateByPayableId, lines, originalLineByPayableId, request])

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

    // 实时修改摘要（回答用户：对比上一轮，我到底改了什么）
    const changedItems = useMemo(() => {
        const changes: string[] = []
        if (executionMethod !== request.execution_method.type) {
            changes.push("付款方式")
        }
        if (executionMethod === "bank_transfer") {
            const origAccountId =
                request.payment_recipient?.bank_account_id ?? ""
            if (bankAccountId !== origAccountId) {
                changes.push("收款账户")
            } else if (bankAccountMasterUpdated) {
                changes.push("收款账户主数据已更新")
            }
        } else {
            const origDepositId =
                request.execution_method.type === "supplier_deposit"
                    ? request.execution_method.account_id
                    : ""
            if (depositAccountId !== origDepositId) {
                changes.push("预存款账户")
            }
        }
        if (
            normalizeDate(desiredPaymentDate) !== request.desired_payment_date
        ) {
            changes.push("希望付款日")
        }
        if (!sameLines(lines, request.lines)) {
            changes.push("采购金额/来源")
        }
        if (description.trim() !== request.description.trim()) {
            changes.push("申请说明")
        }
        if (
            !sameStringSet(keptAttachmentIds, request.attachment_ids) ||
            newFiles.length > 0
        ) {
            changes.push("申请附件")
        }
        return changes
    }, [
        bankAccountId,
        bankAccountMasterUpdated,
        depositAccountId,
        description,
        desiredPaymentDate,
        executionMethod,
        keptAttachmentIds,
        lines,
        newFiles.length,
        request.attachment_ids,
        request.description,
        request.desired_payment_date,
        request.execution_method,
        request.lines,
        request.payment_recipient?.bank_account_id,
    ])

    const hasFormChanges = useMemo(
        () => changedItems.some((label) => label !== "收款账户主数据已更新"),
        [changedItems],
    )

    // 表单校验与真实阻塞诊断
    const accountError = useMemo(() => {
        if (executionMethod === "bank_transfer") {
            if (!bankAccountId) return "请选择供应商的有效收款账户"
            if (
                supplierBankAccounts.length > 0 &&
                !supplierBankAccounts.some(
                    (account) => account.bank_account_id === bankAccountId,
                )
            ) {
                return "原收款账户已失效，请更换有效收款账户"
            }
            return undefined
        }
        if (!depositAccountId) return "请选择同付款主体、同供应商的预存款账户"
        return undefined
    }, [bankAccountId, depositAccountId, executionMethod, supplierBankAccounts])

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

    // 本申请自己上传的附件列表（可移除、可预览）
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

    // 打开供应商档案去补联行号/收款账户，不冲毁当前工作台页签
    const openSupplierProfile = () => {
        const href = `/master-data/suppliers/${encodeURIComponent(request.supplier_id)}`
        if (!navigateWorkspace(href)) {
            router.push(href)
        }
    }

    const refreshLiveData = async () => {
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
    }

    // 构建草稿保存载荷（含幂等文件上传缓存）
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

    const focusFirstInvalid = (includeResubmissionReason: boolean) => {
        if (accountError) {
            document
                .getElementById(`${idPrefix}-account-section`)
                ?.scrollIntoView({
                    behavior: "smooth",
                    block: "center",
                })
            return
        }
        if (dateError) {
            document.getElementById(`${idPrefix}-desired-date`)?.focus()
            return
        }
        if (lineErrors.length > 0) {
            const firstId = `${idPrefix}-line-amount-${toAutomationIdSegment(lineErrors[0].entry.payableAccountId)}`
            document.getElementById(firstId)?.focus()
            return
        }
        if (descriptionError) {
            document.getElementById(`${idPrefix}-description`)?.focus()
            return
        }
        if (includeResubmissionReason && resubmissionReasonError) {
            document.getElementById(`${idPrefix}-resubmission-reason`)?.focus()
        }
    }

    const handleSaveDraft = async () => {
        if (busy || submitUnknown || !canEdit || !hasFormChanges) return
        setTouchedSubmit(true)
        setSubmitError(null)
        if (hasSaveValidationError) {
            focusFirstInvalid(false)
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
        toast.add({
            title: "已重新提交财务审核",
            description: `${request.request_no} 已重新进入财务审核流程。`,
            type: "success",
        })
        onTaskCompleted?.()
    }

    const handleResubmit = async () => {
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

        setTouchedSubmit(true)
        setSubmitError(null)
        if (hasDraftValidationError || resubmissionReasonError) {
            focusFirstInvalid(true)
            return
        }

        submittingRef.current = true
        try {
            let expectedVersion = request.version
            if (hasFormChanges) {
                if (!canEdit) {
                    setSubmitError("当前账号无权修改付款申请内容")
                    return
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
            if (!unknown) {
                submitIdentityRef.current = null
                setSubmitError(
                    getErrorMessage(error, "重新提交失败，请核对后重试"),
                )
            }
        } finally {
            submittingRef.current = false
        }
    }

    // 监听外部刷新后若单据状态已不再可编辑，解除未知提交锁
    useEffect(() => {
        if (!submitUnknown) return
        if (request.status === "in_approval") {
            submitIdentityRef.current = null
            setSubmitUnknown(false)
            onTaskCompleted?.()
        }
    }, [onTaskCompleted, request.status, submitUnknown])

    const primaryLabel = busy
        ? hasFormChanges
            ? "正在保存并重提…"
            : "正在提交…"
        : hasFormChanges
          ? "保存并重新提交"
          : "重新提交审批"

    return (
        <div className="min-w-0 space-y-5 bg-card pt-3 pb-4">
            {/* 1. 系统硬阻塞与账户补全提示（无问题时零占位） */}
            {hasBalanceBlock && (
                <section
                    role="alert"
                    aria-label="提交阻塞提示"
                    className="flex items-start gap-2 rounded-r-md border-l-2 border-l-destructive bg-destructive-soft/50 px-3.5 py-2.5 text-xs text-destructive-soft-foreground"
                >
                    <AlertTriangleIcon
                        aria-hidden="true"
                        className="mt-0.5 size-3.5 shrink-0 text-destructive"
                    />
                    <div className="min-w-0 space-y-1">
                        <p className="font-semibold">
                            来源采购单可申请余额不足，当前无法直接重新提交
                        </p>
                        <p className="text-foreground/80">
                            {canAbandon
                                ? "可展开下方「付款核销与其他申请占用」核对余额去向；如该笔采购款已通过其他申请结算、无需继续办理，可点击底栏「放弃本次办理」关闭此待办，或调整申请金额后重提。"
                                : "可展开下方「付款核销与其他申请占用」核对余额去向；当前仍可修改说明、日期或附件并点击底栏「暂存修改」，如需重提请先调整为有余额的采购单或等待释放额度。"}
                        </p>
                    </div>
                </section>
            )}

            {routingNumberMissing && (
                <section
                    aria-label="联行号补全提示"
                    className="flex flex-wrap items-center justify-between gap-2 rounded-r-md border-l-2 border-l-warning bg-warning-soft/60 px-3.5 py-2 text-xs text-warning-soft-foreground"
                >
                    <div className="flex min-w-0 items-center gap-2">
                        <CircleAlertIcon
                            aria-hidden="true"
                            className="size-3.5 shrink-0"
                        />
                        <span>
                            当前收款账户尚未维护
                            <strong className="font-semibold">联行号</strong>
                            ，如需补充可前往供应商档案维护后点击「刷新账户」同步。
                        </span>
                    </div>
                    <Button
                        id={`${idPrefix}-banner-open-supplier`}
                        type="button"
                        variant="ghost"
                        size="xs"
                        className="h-control-xs text-xs font-medium text-warning-soft-foreground hover:bg-warning-soft"
                        onClick={openSupplierProfile}
                    >
                        去维护供应商账户
                        <ArrowUpRightIcon
                            data-icon="inline-end"
                            aria-hidden="true"
                        />
                    </Button>
                </section>
            )}

            {/* 2. 实时金额、余额与本次修改摘要带 */}
            <TooltipProvider>
                <dl className="grid min-w-0 grid-cols-1 gap-x-5 gap-y-3 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/payment-request:grid-cols-2 @[480px]/payment-request:[&>*:last-child]:col-span-2 @[760px]/payment-request:grid-cols-3 @[760px]/payment-request:[&>*:last-child]:col-span-1">
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
                                            {formatCurrencyFixed(
                                                request.total_amount,
                                                {
                                                    maxScale: 4,
                                                    minimumFractionDigits: 2,
                                                },
                                            )}
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

                    <div className="min-w-0 space-y-1">
                        <dt className="flex items-center gap-1.5 text-xs text-muted-foreground">
                            <span>本次修改摘要</span>
                            <span className="num text-tiny text-muted-foreground/80">
                                对比 V{request.submission_version}
                            </span>
                        </dt>
                        <dd className="min-w-0 space-y-1.5">
                            {changedItems.length > 0 ? (
                                <div className="flex flex-wrap gap-1.5 pt-0.5">
                                    {changedItems.map((label) => (
                                        <span
                                            key={label}
                                            className="inline-flex items-center rounded-md bg-info-soft px-2 py-0.5 text-xs font-medium text-info-foreground"
                                        >
                                            {label}
                                        </span>
                                    ))}
                                </div>
                            ) : (
                                <p className="pt-0.5 text-body-compact text-muted-foreground">
                                    暂未修改表单内容
                                </p>
                            )}
                            <p className="text-tiny text-muted-foreground">
                                {changedItems.length > 0
                                    ? "点击底栏提交将自动保存修改并重新发起审批"
                                    : "若仅需补充说明或已更新供应商资料，可直接填写重提说明"}
                            </p>
                        </dd>
                    </div>
                </dl>
            </TooltipProvider>

            <fieldset
                disabled={busy || submitUnknown || !canEdit}
                className="min-w-0 space-y-5 disabled:opacity-75"
            >
                <legend className="sr-only">修改付款申请内容</legend>

                {/* 3. 付款与收款账户（主修复区：含联行号显式核验与供应商档案快捷维护） */}
                <section
                    id={`${idPrefix}-account-section`}
                    aria-label="付款与收款账户"
                    className="min-w-0 space-y-3.5"
                >
                    <div className="flex flex-wrap items-baseline justify-between gap-2">
                        <h2 className="text-section-title font-semibold text-foreground">
                            付款与收款账户
                        </h2>
                        <p className="text-xs text-muted-foreground">
                            付款主体：{request.our_company_name} · 供应商：
                            {request.supplier_name} · 申请人：
                            {request.applicant_name}
                        </p>
                    </div>

                    <div className="grid grid-cols-1 gap-4 @[520px]/payment-request:grid-cols-2">
                        <Field className="gap-1.5">
                            <FieldLabel
                                htmlFor={`${idPrefix}-execution-method`}
                            >
                                付款方式
                                <span className="text-destructive">*</span>
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
                                    setChoosingAccount(false)
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
                                    className="w-full"
                                >
                                    <SelectValue>
                                        {executionMethod === "supplier_deposit"
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
                            <FieldLabel htmlFor={`${idPrefix}-desired-date`}>
                                希望付款日
                                <span className="text-destructive">*</span>
                            </FieldLabel>
                            <DatePicker
                                id={`${idPrefix}-desired-date`}
                                value={desiredPaymentDate || undefined}
                                onValueChange={(next) =>
                                    setDesiredPaymentDate(next ?? "")
                                }
                                placeholder="选择希望付款日"
                                clearable={false}
                                disabled={busy || submitUnknown || !canEdit}
                                className="w-full"
                            />
                            {touchedSubmit && dateError ? (
                                <FieldError errors={[{ message: dateError }]} />
                            ) : null}
                        </Field>
                    </div>

                    {executionMethod === "bank_transfer" ? (
                        <div className="space-y-2">
                            <div className="flex flex-wrap items-center justify-between gap-2">
                                <span className="text-xs font-medium text-muted-foreground">
                                    收款账户
                                    <span className="ml-0.5 text-destructive">
                                        *
                                    </span>
                                </span>
                                <div className="flex flex-wrap items-center gap-1.5">
                                    {supplierBankAccounts.length > 1 && (
                                        <Button
                                            id={`${idPrefix}-toggle-account-select`}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className={
                                                detailWorkspaceStyles.quietButton
                                            }
                                            onClick={() =>
                                                setChoosingAccount((v) => !v)
                                            }
                                        >
                                            {choosingAccount
                                                ? "收起账户列表"
                                                : "更换账户"}
                                        </Button>
                                    )}
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
                                </div>
                            </div>

                            {choosingAccount &&
                                supplierBankAccounts.length > 0 && (
                                    <Select
                                        value={bankAccountId}
                                        onValueChange={(next) => {
                                            if (!next) return
                                            setBankAccountId(next)
                                            setChoosingAccount(false)
                                        }}
                                    >
                                        <SelectTrigger
                                            id={`${idPrefix}-bank-account-select`}
                                            className="w-full"
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
                                                        {account.account_name} ·{" "}
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
                                )}

                            {displayedBankAccount ? (
                                <div
                                    className={cn(
                                        "rounded-lg border px-4 py-3",
                                        accountError
                                            ? "border-destructive/60 bg-destructive-soft/20"
                                            : "border-border/60 bg-card",
                                    )}
                                >
                                    {bankAccountMasterUpdated && (
                                        <div className="mb-2.5 flex items-center gap-1.5 text-xs font-medium text-success-foreground">
                                            <CheckCircle2Icon
                                                aria-hidden="true"
                                                className="size-3.5 shrink-0"
                                            />
                                            <span>
                                                已同步供应商最新收款账户主数据（提交时将按最新资料冻结）
                                            </span>
                                        </div>
                                    )}
                                    <dl className="grid grid-cols-1 gap-x-6 gap-y-2.5 @[480px]/payment-request:grid-cols-2">
                                        <div className="min-w-0 space-y-0.5">
                                            <dt className="text-xs text-muted-foreground">
                                                收款户名
                                            </dt>
                                            <dd className="text-body-compact font-medium text-foreground">
                                                {
                                                    displayedBankAccount.account_name
                                                }
                                            </dd>
                                        </div>
                                        <div className="min-w-0 space-y-0.5">
                                            <dt className="text-xs text-muted-foreground">
                                                开户行
                                            </dt>
                                            <dd className="text-body-compact font-medium text-foreground">
                                                {bankDisplayLabel(
                                                    displayedBankAccount,
                                                )}
                                            </dd>
                                        </div>
                                        <div className="min-w-0 space-y-0.5">
                                            <dt className="text-xs text-muted-foreground">
                                                收款账号
                                            </dt>
                                            <dd className="num text-body-compact font-medium text-foreground">
                                                {
                                                    displayedBankAccount.account_number_masked
                                                }
                                            </dd>
                                        </div>
                                        <div className="min-w-0 space-y-0.5">
                                            <dt className="text-xs text-muted-foreground">
                                                联行号
                                            </dt>
                                            <dd className="text-body-compact font-medium">
                                                {liveSelectedBankAccount ? (
                                                    liveSelectedBankAccount.bank_routing_number ? (
                                                        <span className="num select-all text-foreground">
                                                            {
                                                                liveSelectedBankAccount.bank_routing_number
                                                            }
                                                        </span>
                                                    ) : (
                                                        <span className="text-warning-soft-foreground">
                                                            未填写
                                                        </span>
                                                    )
                                                ) : candidatesQuery.isPending ? (
                                                    <span className="text-xs font-normal text-muted-foreground">
                                                        读取中…
                                                    </span>
                                                ) : (
                                                    <span className="text-xs font-normal text-muted-foreground">
                                                        请前往供应商档案核对联行号
                                                    </span>
                                                )}
                                            </dd>
                                        </div>
                                    </dl>
                                </div>
                            ) : (
                                <p
                                    role="alert"
                                    className="rounded-lg border border-destructive/50 bg-destructive-soft/30 px-3.5 py-2.5 text-xs text-destructive-soft-foreground"
                                >
                                    未选择有效收款账户。请先在供应商档案维护收款账户后点击「刷新账户」。
                                </p>
                            )}

                            {touchedSubmit && accountError ? (
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
                                (touchedSubmit && Boolean(accountError)) ||
                                undefined
                            }
                        >
                            <FieldLabel htmlFor={`${idPrefix}-deposit-account`}>
                                预存款账户
                                <span className="text-destructive">*</span>
                            </FieldLabel>
                            <Select
                                value={depositAccountId}
                                onValueChange={(next) => {
                                    if (next) setDepositAccountId(next)
                                }}
                            >
                                <SelectTrigger
                                    id={`${idPrefix}-deposit-account`}
                                    className="w-full"
                                >
                                    <SelectValue placeholder="选择同付款主体、同供应商预存款账户">
                                        {depositAccounts.find(
                                            (a) => a.id === depositAccountId,
                                        )?.label ?? "选择预存款账户"}
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
                </section>

                {/* 4. 来源采购与申请金额（支持原地改金额、增减来源采购单） */}
                <section
                    aria-label="来源采购与申请金额"
                    className="min-w-0 space-y-2.5 border-t border-border/60 pt-4"
                >
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        <div className="flex items-baseline gap-2">
                            <h2 className="text-section-title font-semibold text-foreground">
                                来源采购与申请金额
                            </h2>
                            <span className="num text-xs text-muted-foreground">
                                共 {resolvedLines.length} 项
                            </span>
                        </div>
                        {addableCandidates.length > 0 && canEdit && (
                            <Select
                                value=""
                                onValueChange={(payableId) => {
                                    if (!payableId) return
                                    const candidate =
                                        candidateByPayableId.get(payableId)
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
                                        (candidate: PaymentCandidate) => (
                                            <SelectItem
                                                key={
                                                    candidate.payable_account_id
                                                }
                                                id={`${idPrefix}-add-po-${toAutomationIdSegment(candidate.payable_account_id)}`}
                                                value={
                                                    candidate.payable_account_id
                                                }
                                            >
                                                {candidate.purchase_order_no} ·
                                                可申请{" "}
                                                {formatCurrencyFixed(
                                                    candidate.available_amount,
                                                    {
                                                        maxScale: 4,
                                                        minimumFractionDigits: 2,
                                                    },
                                                )}
                                            </SelectItem>
                                        ),
                                    )}
                                </SelectContent>
                            </Select>
                        )}
                    </div>

                    <Table className="w-full table-fixed text-body-compact">
                        <TableHeader className="bg-surface-sunken text-xs text-muted-foreground [&_tr]:border-b-0 [&_th]:bg-surface-sunken [&_th]:hover:bg-surface-sunken [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md">
                            <TableRow>
                                <TableHead
                                    scope="col"
                                    className="w-[44%] px-2.5 py-2 text-left font-medium"
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
                                {resolvedLines.length > 1 && (
                                    <TableHead
                                        scope="col"
                                        className="w-12 px-1 py-2 text-right font-medium"
                                    >
                                        <span className="sr-only">移除</span>
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
                                const showError =
                                    row.noBalance ||
                                    Boolean(touchedSubmit && row.amountError) ||
                                    Boolean(
                                        row.entry.amount.trim() &&
                                        row.amountError,
                                    )
                                const colSpan = resolvedLines.length > 1 ? 4 : 3
                                return (
                                    <Fragment key={row.entry.payableAccountId}>
                                        <TableRow
                                            className={cn(
                                                showError
                                                    ? "border-b-0"
                                                    : "border-border/60",
                                            )}
                                        >
                                            <TableHead
                                                scope="row"
                                                className="bg-transparent px-2.5 py-2.5 text-left font-normal whitespace-normal"
                                            >
                                                <Button
                                                    id={`${idPrefix}-line-toggle-${seg}`}
                                                    type="button"
                                                    variant="ghost"
                                                    size="xs"
                                                    className="h-auto max-w-full justify-start px-0 text-left whitespace-normal wrap-anywhere"
                                                    aria-expanded={isOpen}
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
                                                            "transition-transform",
                                                            isOpen &&
                                                                "rotate-90",
                                                        )}
                                                    />
                                                    <span className="num font-medium">
                                                        {row.purchaseOrderNo}
                                                    </span>
                                                </Button>
                                                <span className="num block text-tiny text-muted-foreground">
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
                                                        showError || undefined
                                                    }
                                                    className="num h-control-sm text-right"
                                                    onChange={(event) => {
                                                        const nextValue =
                                                            event.target.value
                                                        setLines((prev) =>
                                                            prev.map((item) =>
                                                                item.payableAccountId ===
                                                                row.entry
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
                                            </TableCell>
                                            {resolvedLines.length > 1 && (
                                                <TableCell className="px-1 py-2 text-right">
                                                    <Button
                                                        id={`${idPrefix}-line-remove-${seg}`}
                                                        type="button"
                                                        variant="ghost"
                                                        size="icon-xs"
                                                        aria-label={`移除采购单 ${row.purchaseOrderNo}`}
                                                        className="text-muted-foreground hover:text-destructive"
                                                        onClick={() =>
                                                            setLines((prev) =>
                                                                prev.filter(
                                                                    (item) =>
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

                                        {showError && row.amountError && (
                                            <TableRow className="border-b border-border/60 hover:bg-transparent">
                                                <TableCell
                                                    colSpan={colSpan}
                                                    className="pt-0 pb-2.5"
                                                >
                                                    <div className="rounded-r-md border-l-2 border-l-destructive bg-destructive-soft/50 px-2.5 py-1.5 text-xs text-destructive-soft-foreground">
                                                        {row.amountError}
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
                                {resolvedLines.length > 1 && <TableCell />}
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
                                setPaymentHistoryOpen((open) => !open)
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
                </section>

                {/* 5. 申请说明与附件 */}
                <section
                    aria-label="申请说明与附件"
                    className="min-w-0 space-y-4 border-t border-border/60 pt-4"
                >
                    <h2 className="text-section-title font-semibold text-foreground">
                        申请说明与附件
                    </h2>

                    <Field
                        className="gap-1.5"
                        data-invalid={
                            (touchedSubmit && Boolean(descriptionError)) ||
                            undefined
                        }
                    >
                        <div className="flex items-baseline justify-between gap-2">
                            <FieldLabel htmlFor={`${idPrefix}-description`}>
                                申请说明
                                <span className="text-destructive">*</span>
                            </FieldLabel>
                            <span className="num text-tiny text-muted-foreground">
                                {descriptionCharCount} / {DESCRIPTION_MAX_CHARS}
                            </span>
                        </div>
                        <Textarea
                            id={`${idPrefix}-description`}
                            rows={2}
                            value={description}
                            placeholder="说明本次付款安排及依据"
                            aria-invalid={
                                (touchedSubmit && Boolean(descriptionError)) ||
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
                            <ul className="divide-y divide-border/60 rounded-lg border border-border/60">
                                {keptAttachmentIds.map((fileId) => {
                                    const file = ownAttachmentMap.get(fileId)
                                    const seg = toAutomationIdSegment(fileId)
                                    return (
                                        <li
                                            key={fileId}
                                            className="flex flex-wrap items-center justify-between gap-2 px-3 py-2 text-body-compact"
                                        >
                                            <span className="min-w-0 flex-1 truncate">
                                                {file?.file_name ?? fileId}
                                            </span>
                                            <div className="flex shrink-0 items-center gap-1">
                                                {onSelectFile && (
                                                    <Button
                                                        id={`${idPrefix}-att-preview-${seg}`}
                                                        type="button"
                                                        variant="ghost"
                                                        size="xs"
                                                        onClick={() =>
                                                            onSelectFile(fileId)
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
                                                                .mutateAsync({
                                                                    requestId:
                                                                        request.id,
                                                                    attachment:
                                                                        file,
                                                                })
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
                                                                    (id) =>
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
                            selectedDescription={fileUploadText.pendingSave}
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
                                        map.set(selectedFileKey(file), file)
                                    }
                                    return Array.from(map.values())
                                })
                            }}
                        />
                    </div>
                </section>
            </fieldset>

            {/* 6. 重新提交说明与审批路线（直面审批人，提交前一目了然） */}
            {canSubmit && (
                <section
                    aria-label="重新提交说明与审批路线"
                    className="min-w-0 space-y-3.5 border-t border-border/60 pt-4"
                >
                    <h2 className="text-section-title font-semibold text-foreground">
                        回复审批人并重新提交
                    </h2>

                    {requiresResubmissionReason && (
                        <ReasonTextareaField
                            id={`${idPrefix}-resubmission-reason`}
                            name="resubmission_reason"
                            mode="utf8-bytes"
                            max={BPM_REASON_MAX_UTF8_BYTES}
                            label="重新提交说明"
                            placeholder="向下一轮审批人说明本次修改内容或重提依据（例如：已补充供应商联行号，请复核）"
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
                                touchedSubmit && resubmissionReasonError,
                            )}
                            onCtrlEnterSubmit={() => {
                                if (!busy && !submitUnknown) {
                                    void handleResubmit()
                                }
                            }}
                        />
                    )}

                    <SubmissionRouteConfirmation
                        documentType="purchase_payment_request"
                        definition={approval?.definition}
                    />
                </section>
            )}

            {submitError && (
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

            {/* 7. 底部吸底操作栏：原地闭环，与左下角「处理后自动下一项」完整联动 */}
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
                            onClick={() => void handleResubmit()}
                        >
                            {busy ? "正在核对结果…" : "核对提交结果"}
                        </Button>
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
                            {changedItems.length > 0
                                ? ` · 已修改 ${changedItems.length} 项`
                                : ""}
                            {hasBalanceBlock ? (
                                <span className="ml-2 text-destructive">
                                    · 采购余额不足，暂不可重提
                                </span>
                            ) : null}
                        </span>

                        {abandonAction}

                        {canEdit && (
                            <Button
                                id={`${idPrefix}-save-draft`}
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={
                                    busy ||
                                    !hasFormChanges ||
                                    hasSaveValidationError
                                }
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
                                disabled={busy || hasBalanceBlock}
                                onClick={() => void handleResubmit()}
                            >
                                {primaryLabel}
                            </Button>
                        )}
                    </div>
                )}
            </WorkspaceTaskFooter>
        </div>
    )
}
