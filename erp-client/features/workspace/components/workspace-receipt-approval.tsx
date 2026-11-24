"use client"

import { useEffect, useImperativeHandle, useRef, useState } from "react"
import type { RefObject } from "react"
import {
    ChevronRightIcon,
    CircleQuestionMarkIcon,
    PaperclipIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    MoneyValue,
    RateValue,
    SensitiveValue,
    WorkspaceFactGrid,
    WorkspaceFactGroup,
    WorkspaceFactItem,
    detailWorkspaceStyles,
} from "@/components/business"
import { useUnifiedTaskWorkbenchRail } from "@/components/business/unified-task-workbench"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Spinner } from "@/components/ui/spinner"
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
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import { ApprovalMaterialFiles } from "@/features/approval-workflow/components/approval-material-files"
import { displayActorName } from "@/features/approval-workflow/display"
import { useApprovalMaterials } from "@/features/approval-workflow/hooks/use-approval-materials"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compareDecimal,
    divideFixed,
    minFixed,
    multiplyFixed,
    normalizeFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { hasPermission } from "@/lib/permissions"
import { cn } from "@/lib/utils"
import { dispatchWorkspaceHotkeyAction } from "@/lib/workspace-hotkeys"
import type { WorkspaceWorkItem } from "../types"
import { WorkspaceApprovalResubmissionBanner } from "./workspace-approval-resubmission-banner"
import {
    APPROVAL_TABLE_DEFAULT_PAGE_SIZE,
    ApprovalLineFilterBar,
    ApprovalLinePagination,
} from "./workspace-approval-table-controls"

type ReceiptSource = ApprovalMaterials["display"]["source"]
type ReceiptField = {
    label: string
    value: string
    numeric?: boolean
}
type ReceiptFields = ReadonlyMap<string, ReceiptField>
type ReceiptAllocationLine = {
    title: string
    amount: string | undefined
    dueLabel?: string | null
}

const MONEY_SCALE = { maxScale: 2, outputScale: 2 }
const DIFF_SUFFIX = "（前→后）"
const BANK_ACCOUNT_LABELS = new Map([
    ["收款账号", "receiving"],
    ["收款账户", "receiving"],
    ["付款账号", "paying"],
    ["付款账户", "paying"],
    ["银行账号", "bank"],
    ["银行账户", "bank"],
])
const SUMMARY_LABELS = new Set([
    "回款金额",
    "到账日",
    "到账日期",
    "银行流水",
    "待核销",
    "待核销余额",
    "往来主体",
    "提交人",
    "单号",
    "回款单号",
    "提交版本",
    "重新提交说明",
    "对比基准",
    "重提差异",
])

const EMPTY_VALUES = new Set([
    "—",
    "无",
    "未记录",
    "未提供",
    "未填写",
    "未记录名称",
])

function readField(fields: ReceiptFields, label: string) {
    const value = fields.get(label)?.value.trim()
    return value && !EMPTY_VALUES.has(value) ? value : undefined
}

function receiptMoney(value: string | null | undefined, allowNegative = false) {
    if (!value) return undefined
    const match =
        /^(-?)[¥￥]?(-?)(\d{1,3}(?:,\d{3})+|\d+)(\.\d+)?(?:\s*元)?$/.exec(
            value.trim(),
        )
    if (!match || (match[1] && match[2])) return undefined

    try {
        const decimal = `${match[1] || match[2]}${match[3].replaceAll(",", "")}${match[4] ?? ""}`
        return normalizeFixed(decimal, { ...MONEY_SCALE, allowNegative })
    } catch {
        return undefined
    }
}

function maskBankText(raw: string) {
    const value = raw.trim()
    if (/^[^*]{0,4}\*{2,}[^*]{0,4}$/.test(value)) return value
    if (value.length > 8) return `${value.slice(0, 4)}****${value.slice(-4)}`
    if (value.length > 4) return `****${value.slice(-4)}`
    return "****"
}

function readBankAccounts(fields: ReceiptFields) {
    const accounts = new Map<string, { label: string; value: string }>()
    for (const [label, kind] of BANK_ACCOUNT_LABELS) {
        const value = readField(fields, label)
        if (value && !accounts.has(kind)) accounts.set(kind, { label, value })
    }
    return accounts
}

function getAllocationTotals(
    amount: string | undefined,
    fields: ReceiptFields,
    lines: readonly ReceiptAllocationLine[],
    moreCount: number,
) {
    // 历史摘要的「待核销」可能是笔数。只接受金额；截断摘要不能用于推算余额。
    const recordedBalance =
        receiptMoney(readField(fields, "待核销余额"), true) ??
        receiptMoney(readField(fields, "待核销"), true)
    const lineAmounts = lines.flatMap((line) => line.amount ?? [])
    const lineTotal =
        moreCount === 0 &&
        lines.length > 0 &&
        lineAmounts.length === lines.length
            ? sumFixed(lineAmounts, MONEY_SCALE)
            : undefined
    const totalFromBalance =
        amount && recordedBalance
            ? recordedBalance.startsWith("-")
                ? sumFixed([amount, recordedBalance.slice(1)], MONEY_SCALE)
                : subtractFixed(amount, recordedBalance, MONEY_SCALE)
            : undefined
    const total =
        totalFromBalance && compareDecimal(totalFromBalance, "0", 2) >= 0
            ? totalFromBalance
            : lineTotal
    const balance =
        recordedBalance ??
        (amount && total
            ? subtractFixed(amount, total, MONEY_SCALE)
            : undefined)
    const percent =
        amount && total && compareDecimal(amount, "0", 2) > 0
            ? multiplyFixed(
                  divideFixed(total, amount, {
                      numeratorMaxScale: 2,
                      denominatorMaxScale: 2,
                      outputScale: 4,
                  }),
                  "100",
                  { leftMaxScale: 4, rightMaxScale: 0, outputScale: 2 },
              )
            : undefined

    const balanceSign =
        balance === undefined ? undefined : compareDecimal(balance, "0", 2)
    return { total, balance, balanceSign, percent }
}

type ReceiptRisk = {
    id: string
    label: string
    phrase: string
    locate: "arrival" | "materials" | "allocation" | "accounts"
    severe?: boolean
}

function getReceiptRisks(
    fields: ReceiptFields,
    attachmentCount: number | undefined,
    balanceSign: number | undefined,
    invalidLineCount: number,
) {
    const risks: ReceiptRisk[] = []
    if (!readField(fields, "银行流水")) {
        risks.push({
            id: "bank-reference",
            label: "缺少银行流水",
            phrase: "请补充银行流水号，并核对到账记录。",
            locate: "arrival",
            severe: true,
        })
    }
    if (attachmentCount === 0) {
        risks.push({
            id: "voucher",
            label: "缺少到账凭证",
            phrase: "请补充银行回单或其他到账凭证。",
            locate: "materials",
            severe: true,
        })
    }
    if (balanceSign === 1 || balanceSign === -1) {
        const overAllocated = balanceSign < 0
        risks.push({
            id: "allocation-balance",
            label: overAllocated ? "拟核销金额超过回款" : "存在待核销余额",
            phrase: overAllocated
                ? "拟核销金额超过本次回款，请重新核对分配金额。"
                : "本次回款仍有待核销余额，请补充分配或说明原因。",
            locate: "allocation",
            severe: overAllocated,
        })
    }
    if (readBankAccounts(fields).size === 0) {
        risks.push({
            id: "bank-account",
            label: "缺少银行账户",
            phrase: "请补充收付款账户，并与到账凭证核对。",
            locate: "accounts",
        })
    }
    if (invalidLineCount > 0) {
        risks.push({
            id: "allocation-line",
            label: `${invalidLineCount} 笔核销金额待核对`,
            phrase: "请核对拟核销明细，补齐关联销售单的分配金额。",
            locate: "allocation",
        })
    }

    return risks
}

/** 审批操作栏使用同一份冻结提交资料生成上下文短语。 */
export function buildReceiptDynamicRejectPhrases(
    source: ReceiptSource | null | undefined,
    attachmentCount?: number,
): string[] {
    if (!source) return []

    const fields = new Map(
        source.extra_sections.map((field) => [field.label, field]),
    )
    const lines = source.lines.map((line) => ({
        title: line.title,
        amount: receiptMoney(line.quantity),
    }))
    const amount =
        receiptMoney(readField(fields, "回款金额")) ??
        receiptMoney(source.amount_label)
    const { balanceSign } = getAllocationTotals(
        amount,
        fields,
        lines,
        source.more_count,
    )
    return getReceiptRisks(
        fields,
        attachmentCount,
        balanceSign,
        lines.filter((line) => line.amount === undefined).length,
    )
        .map((risk) => risk.phrase)
        .slice(0, 6)
}

export type ReceiptRiskItem = Readonly<ReceiptRisk>

/** 供审批风险提示与驳回行动指挥卡共享同一份回款前置风险体检结论。 */
export function inspectReceiptRisks(
    source: ReceiptSource | null | undefined,
    attachmentCount?: number,
): ReceiptRiskItem[] {
    if (!source) return []
    const fields = new Map(
        source.extra_sections.map((field) => [field.label, field]),
    )
    const lines = source.lines.map((line) => ({
        title: line.title,
        amount: receiptMoney(line.quantity),
    }))
    const amount =
        receiptMoney(readField(fields, "回款金额")) ??
        receiptMoney(source.amount_label)
    const { balanceSign } = getAllocationTotals(
        amount,
        fields,
        lines,
        source.more_count,
    )
    return getReceiptRisks(
        fields,
        attachmentCount,
        balanceSign,
        lines.filter((line) => line.amount === undefined).length,
    )
}

export interface WorkspaceReceiptApprovalHandle {
    /** 冻结摘要只提供行标题。同名分配定位到首条。 */
    locateLine: (title: string) => void
    locateRisk: (riskId: string) => void
}

/** 回款审批只读作业面，展示本次提交资料或工作台绑定的提交摘要。 */
export function WorkspaceReceiptApproval({
    item,
    instanceId,
    onSelectFile,
    onOpenMaterials,
    activeFileId,
    hideEmbeddedMaterials = false,
    hideTopBanners = false,
    approvalHandleRef,
}: {
    item: WorkspaceWorkItem
    instanceId: string
    onSelectFile?: (fileId: string) => void
    onOpenMaterials?: () => void
    activeFileId?: string
    hideEmbeddedMaterials?: boolean
    hideTopBanners?: boolean
    approvalHandleRef?: RefObject<WorkspaceReceiptApprovalHandle | null>
}) {
    const profile = useAccountProfileQuery()
    const canRevealBankAccount = hasPermission(
        profile.data?.permissions,
        "party_bank_account:reveal",
    )
    const materials = useApprovalMaterials(instanceId, true, item)
    const rail = useUnifiedTaskWorkbenchRail()
    const workItemSegment = toAutomationIdSegment(item.workItemId)
    const prefix = `workspace-receipt-approval-${workItemSegment}`
    const source = materials.data?.display.source
    const attachments = materials.data?.attachments ?? []
    const sections = (source?.extra_sections ?? item.summarySections ?? []).map(
        (field) => {
            const objectId =
                "object_id" in field ? field.object_id : field.objectId
            return {
                ...field,
                value:
                    objectId?.trim() === field.value.trim()
                        ? "未记录名称"
                        : field.value,
            }
        },
    )
    const fields = new Map(sections.map((field) => [field.label, field]))
    const lines = source
        ? source.lines.map((line) => ({
              title: line.title,
              amount: receiptMoney(line.quantity),
              dueLabel: line.due_label,
          }))
        : (item.briefLines ?? []).map((line) => ({
              title: line.title,
              amount: receiptMoney(line.quantity),
              dueLabel: line.dueLabel,
          }))
    const moreCount = source?.more_count ?? item.briefMoreCount ?? 0
    const amount =
        receiptMoney(readField(fields, "回款金额")) ??
        receiptMoney(source?.amount_label) ??
        receiptMoney(item.amountSummary?.value)
    const totals = getAllocationTotals(amount, fields, lines, moreCount)
    const bankReference = readField(fields, "银行流水")
    const bankAccounts = readBankAccounts(fields)
    const arrivalDate =
        readField(fields, "到账日") ?? readField(fields, "到账日期")
    const counterparty =
        readField(fields, "往来主体") ??
        materials.data?.display.counterparty_label ??
        item.counterpartyName
    const submitter = displayActorName(
        source?.submitter_name ?? readField(fields, "提交人"),
    )
    const redactBankText = (value: string) => {
        for (const [label] of BANK_ACCOUNT_LABELS) {
            const account = readField(fields, label)
            if (account)
                value = value.replaceAll(account, maskBankText(account))
        }
        return value
    }
    const extraFields = [...fields.values()].filter(
        (field) =>
            !SUMMARY_LABELS.has(field.label) &&
            !BANK_ACCOUNT_LABELS.has(field.label) &&
            !field.label.endsWith(DIFF_SUFFIX) &&
            field.value.trim(),
    )
    const roundNo = item.approval?.currentRoundNo ?? 1
    const resubmissionReason = readField(fields, "重新提交说明")
    const diffSummary = readField(fields, "重提差异")
    const previousReason =
        item.approval?.status !== "REJECTED"
            ? item.approval?.lastRejectReason?.trim()
            : undefined
    const diffs =
        roundNo >= 2
            ? sections.flatMap((section) => {
                  if (!section.label.endsWith(DIFF_SUFFIX)) return []
                  const arrowIndex = section.value.indexOf(" → ")
                  if (arrowIndex < 0) return []
                  const before = section.value.slice(0, arrowIndex).trim()
                  const after = section.value.slice(arrowIndex + 3).trim()
                  if (!before || !after) return []
                  const label = section.label.slice(0, -DIFF_SUFFIX.length)
                  const mask =
                      BANK_ACCOUNT_LABELS.has(label) || label === "银行流水"
                          ? maskBankText
                          : redactBankText
                  return [
                      {
                          label,
                          before: mask(before),
                          after: mask(after),
                          numeric: section.numeric,
                      },
                  ]
              })
            : []

    const [search, setSearch] = useState("")
    const [riskOnly, setRiskOnly] = useState(false)
    const [page, setPage] = useState(1)
    const [pageSize, setPageSize] = useState(APPROVAL_TABLE_DEFAULT_PAGE_SIZE)
    const [locateTarget, setLocateTarget] = useState<
        | { kind: "line"; lineIndex: number }
        | { kind: "footer" }
        | { kind: "section"; id: string }
        | null
    >(null)
    const locatedRow = useRef<HTMLTableRowElement>(null)
    const needle = search.trim().toLowerCase()
    const hasFilters = Boolean(needle) || riskOnly
    const invalidLineCount = lines.filter(
        (line) => line.amount === undefined,
    ).length
    const risks = source
        ? getReceiptRisks(
              fields,
              attachments.length,
              totals.balanceSign,
              invalidLineCount,
          )
        : []
    const severeRisk = risks.some((risk) => risk.severe)
    const filteredLines = lines
        .map((line, index) => ({ line, index }))
        .filter(
            ({ line }) =>
                (!riskOnly || line.amount === undefined) &&
                (!needle ||
                    `${line.title} ${line.dueLabel ?? ""}`
                        .toLowerCase()
                        .includes(needle)),
        )
    const activePage = Math.min(
        page,
        Math.max(1, Math.ceil(filteredLines.length / pageSize)),
    )
    const visibleLines = filteredLines.slice(
        (activePage - 1) * pageSize,
        activePage * pageSize,
    )
    const allocationSectionId = `${prefix}-allocations`
    const materialSectionId = `${prefix}-materials`
    const arrivalSectionId = `${prefix}-arrival`
    const accountsSectionId = `${prefix}-accounts`
    const hasAllocationTable = lines.length > 0 || totals.total !== undefined
    const clearFilters = () => {
        setSearch("")
        setRiskOnly(false)
        setPage(1)
    }

    const locateAllocation = (lineIndex?: number) => {
        clearFilters()
        if (lineIndex !== undefined) {
            setPage(Math.floor(lineIndex / pageSize) + 1)
            setLocateTarget({ kind: "line", lineIndex })
            return
        }
        setLocateTarget(
            hasAllocationTable
                ? { kind: "footer" }
                : { kind: "section", id: allocationSectionId },
        )
    }

    useEffect(() => {
        if (!locateTarget) return
        const reducedMotion = window.matchMedia?.(
            "(prefers-reduced-motion: reduce)",
        ).matches
        const frame = window.requestAnimationFrame(() => {
            const target =
                locateTarget.kind === "section"
                    ? document.getElementById(locateTarget.id)
                    : (locatedRow.current ??
                      document.getElementById(allocationSectionId))
            target?.focus({ preventScroll: true })
            target?.scrollIntoView({
                behavior: reducedMotion ? "instant" : "smooth",
                block: "center",
            })
        })
        const timeout = window.setTimeout(() => setLocateTarget(null), 2200)
        return () => {
            window.cancelAnimationFrame(frame)
            window.clearTimeout(timeout)
        }
    }, [allocationSectionId, locateTarget])

    const openMaterials = () => {
        if (onOpenMaterials) {
            onOpenMaterials()
            return
        }
        if (!hideEmbeddedMaterials) {
            setLocateTarget({ kind: "section", id: materialSectionId })
            return
        }

        const file =
            attachments.find((entry) => entry.file_asset_id === activeFileId) ??
            attachments[0]
        if (file && onSelectFile) {
            onSelectFile(file.file_asset_id)
            return
        }
        const tab = document.getElementById(
            `workspace-approval-rail-${workItemSegment}-tab-materials`,
        )
        if (!rail?.collapsed && tab?.getAttribute("aria-selected") === "true") {
            tab.focus()
            return
        }
        dispatchWorkspaceHotkeyAction("materials")
    }

    const locateRisk = (risk: ReceiptRisk) => {
        switch (risk.locate) {
            case "materials":
                openMaterials()
                break
            case "allocation": {
                const invalidIndex = lines.findIndex(
                    (line) => line.amount === undefined,
                )
                locateAllocation(
                    risk.id === "allocation-line" && invalidIndex >= 0
                        ? invalidIndex
                        : undefined,
                )
                break
            }
            case "arrival":
                setLocateTarget({ kind: "section", id: arrivalSectionId })
                break
            case "accounts":
                setLocateTarget({ kind: "section", id: accountsSectionId })
                break
        }
    }

    useImperativeHandle(approvalHandleRef, () => ({
        locateLine: (title) => {
            const index = lines.findIndex((line) => line.title === title)
            if (index >= 0) locateAllocation(index)
        },
        locateRisk: (riskId) => {
            const target = risks.find((entry) => entry.id === riskId)
            if (target) locateRisk(target)
        },
    }))

    const renderSensitiveBankField = (
        kind: string,
        label: string,
        value: string,
    ) => {
        const masked = maskBankText(value)
        return (
            <SensitiveValue
                key={`${instanceId}-${kind}-${canRevealBankAccount}`}
                id={`${prefix}-${kind}`}
                label={label}
                maskedValue={masked}
                onReveal={
                    canRevealBankAccount && !value.includes("*")
                        ? async () => value
                        : undefined
                }
                className="min-w-0 [&_code]:max-w-full [&_code]:select-all [&_code]:wrap-anywhere [&_code]:bg-transparent [&_code]:p-0 [&_code]:text-body-compact [&_code]:font-medium"
            />
        )
    }

    const diffBaselineRaw = readField(fields, "对比基准")
    const arrivalHighlighted =
        locateTarget?.kind === "section" && locateTarget.id === arrivalSectionId
    const accountsHighlighted =
        locateTarget?.kind === "section" &&
        locateTarget.id === accountsSectionId
    const allocationSectionHighlighted =
        locateTarget?.kind === "section" &&
        locateTarget.id === allocationSectionId
    const materialSectionHighlighted =
        locateTarget?.kind === "section" &&
        locateTarget.id === materialSectionId

    return (
        <div
            data-slot="workspace-receipt-approval"
            className="@container/receipt-approval flex min-w-0 flex-col"
        >
            {!hideTopBanners && (roundNo >= 2 || risks.length > 0) ? (
                <div className="space-y-2 pt-3">
                    {roundNo >= 2 ? (
                        <WorkspaceApprovalResubmissionBanner
                            roundNo={roundNo}
                            resubmissionReason={
                                resubmissionReason &&
                                redactBankText(resubmissionReason)
                            }
                            diffBaseline={
                                diffBaselineRaw
                                    ? redactBankText(diffBaselineRaw)
                                    : undefined
                            }
                            diffSummary={
                                diffSummary
                                    ? redactBankText(diffSummary)
                                    : resubmissionReason ||
                                        diffs.length ||
                                        previousReason
                                      ? undefined
                                      : "本次提交未附变更说明"
                            }
                            diffs={diffs}
                            previousRejection={
                                previousReason
                                    ? {
                                          reason: redactBankText(
                                              previousReason,
                                          ),
                                          actor: item.approval
                                              ?.lastRejectorLabel,
                                      }
                                    : undefined
                            }
                        />
                    ) : null}

                    {risks.length > 0 ? (
                        <div
                            role="alert"
                            aria-label="回款审批风险"
                            data-slot="receipt-approval-risk-strip"
                            className={cn(
                                "flex flex-wrap items-center gap-x-3 gap-y-1 rounded-md border-l-2 px-3 py-1.5 text-xs",
                                severeRisk
                                    ? "border-l-destructive bg-destructive-soft/65 text-destructive-soft-foreground"
                                    : "border-l-warning bg-warning-soft/75 text-warning-soft-foreground",
                            )}
                        >
                            <span className="inline-flex items-center gap-1.5 font-medium">
                                <TriangleAlertIcon
                                    aria-hidden="true"
                                    className="size-3.5 shrink-0"
                                />
                                <span className="num">
                                    发现 {risks.length} 处待核对
                                </span>
                            </span>
                            <ul className="flex flex-wrap gap-x-1.5 gap-y-0.5">
                                {risks.map((risk) => (
                                    <li key={risk.id}>
                                        <Button
                                            id={`${prefix}-locate-${toAutomationIdSegment(risk.id)}`}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className={cn(
                                                "h-control-xs gap-0.5 px-1.5 text-xs font-medium shadow-none",
                                                severeRisk
                                                    ? "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                                    : "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
                                            )}
                                            onClick={() => locateRisk(risk)}
                                        >
                                            <span className="num">
                                                {risk.label}
                                            </span>
                                            <ChevronRightIcon aria-hidden="true" />
                                        </Button>
                                    </li>
                                ))}
                            </ul>
                        </div>
                    ) : null}
                </div>
            ) : null}

            <TooltipProvider>
                <div
                    role="region"
                    aria-label="回款审批关键决策指标"
                    className="mt-3 grid grid-cols-1 gap-x-6 gap-y-3.5 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/receipt-approval:grid-cols-2"
                >
                    <div className="min-w-0">
                        <ReceiptKpiLabel
                            label="本次回款金额"
                            basis="本次登记"
                            tooltipId={`${prefix}-receipt-amount-help`}
                            tooltip="取本次提交登记的到账金额。审批全部通过并入账后，计入已收金额。"
                        />
                        <div className="num mt-1 wrap-anywhere text-value-section font-semibold tracking-tight text-foreground">
                            {amount ? (
                                <MoneyValue value={amount} display="exact" />
                            ) : (
                                <span className="text-body-compact font-medium text-muted-foreground">
                                    未记录
                                </span>
                            )}
                        </div>
                    </div>

                    <div className="min-w-0">
                        <ReceiptKpiLabel
                            label="待核销余额"
                            basis="回款 − 拟核销"
                            tooltipId={`${prefix}-allocation-balance-help`}
                            tooltip="核销覆盖率 = 本次拟核销合计 ÷ 本次回款金额。摘要不完整时以已记录的整单金额为准。"
                        />
                        <div
                            className={cn(
                                "num mt-1 wrap-anywhere text-value-compact font-semibold tracking-tight",
                                totals.balanceSign === 0 &&
                                    "text-success-soft-foreground",
                                totals.balanceSign === 1 &&
                                    "text-warning-soft-foreground",
                                totals.balanceSign === -1 &&
                                    "text-destructive-soft-foreground",
                            )}
                        >
                            {totals.balance !== undefined ? (
                                <span>
                                    {totals.balanceSign === -1
                                        ? "超配 "
                                        : totals.balanceSign === 1
                                          ? "余 "
                                          : "已配平 "}
                                    <MoneyValue
                                        value={
                                            totals.balanceSign === -1
                                                ? totals.balance.slice(1)
                                                : totals.balance
                                        }
                                        display="exact"
                                        inheritColor
                                    />
                                </span>
                            ) : (
                                <span className="text-body-compact font-medium text-muted-foreground">
                                    待核对
                                </span>
                            )}
                        </div>
                        <ReceiptCoverage totals={totals} />
                    </div>
                </div>
            </TooltipProvider>

            <WorkspaceFactGroup
                title="到账与账户信息"
                className="mt-4 border-b border-border/60 pb-4"
                actions={
                    attachments.length > 0 ? (
                        <Button
                            id={`${prefix}-open-materials`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className={cn(
                                detailWorkspaceStyles.inlineLinkButton,
                                "h-control-xs px-1.5 text-xs",
                            )}
                            onClick={openMaterials}
                        >
                            <PaperclipIcon aria-hidden="true" />
                            <span className="num">
                                查看凭证（{attachments.length}）
                            </span>
                            <ChevronRightIcon aria-hidden="true" />
                        </Button>
                    ) : undefined
                }
            >
                <WorkspaceFactGrid columns={3} ariaLabel="到账与账户信息">
                    <WorkspaceFactItem
                        id={arrivalSectionId}
                        label="到账日期"
                        value={arrivalDate ?? "未记录"}
                        numeric
                        selectable={Boolean(arrivalDate)}
                        tone={arrivalDate ? "default" : "muted"}
                        highlighted={arrivalHighlighted}
                    />
                    <WorkspaceFactItem
                        label="银行流水"
                        numeric
                        tone={bankReference ? "default" : "warning"}
                        highlighted={arrivalHighlighted}
                    >
                        {bankReference
                            ? renderSensitiveBankField(
                                  "bank-reference",
                                  "银行流水",
                                  bankReference,
                              )
                            : "未记录"}
                    </WorkspaceFactItem>
                    {bankAccounts.size > 0 ? (
                        [...bankAccounts].map(([kind, account], index) => (
                            <WorkspaceFactItem
                                key={kind}
                                id={index === 0 ? accountsSectionId : undefined}
                                label={account.label}
                                numeric
                                highlighted={accountsHighlighted}
                            >
                                {renderSensitiveBankField(
                                    `bank-account-${kind}`,
                                    account.label,
                                    account.value,
                                )}
                            </WorkspaceFactItem>
                        ))
                    ) : (
                        <WorkspaceFactItem
                            id={accountsSectionId}
                            label="收付款账户"
                            value="未记录"
                            tone="warning"
                            highlighted={accountsHighlighted}
                        />
                    )}
                    <WorkspaceFactItem
                        label="往来主体"
                        value={redactBankText(counterparty ?? "未记录")}
                        tone={counterparty ? "default" : "muted"}
                    />
                    <WorkspaceFactItem
                        label="提交人"
                        value={submitter || "未记录姓名"}
                        tone={submitter ? "default" : "muted"}
                    />
                    {extraFields.map((field) => {
                        const redacted = redactBankText(field.value)
                        const isLong =
                            redacted.length > 30 ||
                            redacted.includes("\n") ||
                            /(?:说明|原因|备注|事由)/.test(field.label)
                        return (
                            <WorkspaceFactItem
                                key={field.label}
                                label={field.label}
                                value={redacted}
                                numeric={field.numeric}
                                selectable={field.numeric}
                                span={isLong ? "full" : 1}
                            />
                        )
                    })}
                </WorkspaceFactGrid>
            </WorkspaceFactGroup>

            <DetailWorkspaceSection
                id={allocationSectionId}
                tabIndex={-1}
                divider={false}
                title="本次拟核销明细"
                subtitle={
                    lines.length + moreCount > 0 ? (
                        <span className="num">
                            共 {lines.length + moreCount} 笔
                        </span>
                    ) : undefined
                }
                ariaLabel="本次拟核销明细"
                className={cn(
                    "scroll-mt-16 rounded-md pt-4 pb-5 transition-colors",
                    allocationSectionHighlighted &&
                        "bg-info-soft/25 ring-1 ring-info-border motion-safe:animate-pulse",
                )}
            >
                <ApprovalLineFilterBar
                    idPrefix={`${prefix}-lines`}
                    totalCount={lines.length}
                    filteredCount={filteredLines.length}
                    riskCount={invalidLineCount}
                    hasHighRisk={false}
                    riskOnly={riskOnly}
                    onRiskOnlyChange={(next) => {
                        setRiskOnly(next)
                        setPage(1)
                    }}
                    search={search}
                    onSearchChange={(next) => {
                        setSearch(next)
                        setPage(1)
                    }}
                    onClearFilters={clearFilters}
                    searchPlaceholder="搜索关联销售单"
                    ariaLabel="核销明细筛选"
                />
                {hasAllocationTable ? (
                    <Table
                        data-density="comfortable"
                        aria-label="本次拟核销明细"
                    >
                        <TableHeader className="bg-surface-sunken [&_th]:bg-surface-sunken [&_th:first-child]:rounded-l-md [&_th:last-child]:rounded-r-md [&_tr]:border-b-0">
                            <TableRow>
                                <TableHead scope="col">关联销售单</TableHead>
                                <TableHead scope="col" data-align="end">
                                    本次拟核销金额
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody className="[&_tr]:border-border/60 [&_tr:last-child]:border-0">
                            {visibleLines.map(({ line, index }) => {
                                const isLocated =
                                    locateTarget?.kind === "line" &&
                                    locateTarget.lineIndex === index
                                return (
                                    <TableRow
                                        key={`${index}:${line.title}`}
                                        id={`${prefix}-row-${index + 1}`}
                                        tabIndex={-1}
                                        ref={isLocated ? locatedRow : undefined}
                                        className={cn(
                                            isLocated &&
                                                "bg-info-soft/25 ring-1 ring-info-border ring-inset motion-safe:animate-pulse",
                                        )}
                                    >
                                        <TableCell className="whitespace-normal wrap-anywhere">
                                            <span className="num select-all">
                                                {line.title}
                                            </span>
                                            {line.dueLabel ? (
                                                <p className="num mt-1 text-xs text-muted-foreground">
                                                    {line.dueLabel}
                                                </p>
                                            ) : null}
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            className="num font-semibold"
                                        >
                                            {line.amount !== undefined ? (
                                                <MoneyValue
                                                    value={line.amount}
                                                    display="exact"
                                                />
                                            ) : (
                                                <span className="text-xs text-warning-soft-foreground">
                                                    金额待核对
                                                </span>
                                            )}
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                            {visibleLines.length === 0 ? (
                                <TableRow>
                                    <TableCell
                                        colSpan={2}
                                        className="whitespace-normal text-xs text-muted-foreground"
                                    >
                                        {hasFilters
                                            ? "没有符合条件的核销明细。"
                                            : "暂无可查看的核销分配明细。"}
                                    </TableCell>
                                </TableRow>
                            ) : null}
                        </TableBody>
                        <TableFooter className="border-t-0 bg-surface-sunken [&_td:first-child]:rounded-l-md [&_td:last-child]:rounded-r-md">
                            <TableRow
                                tabIndex={-1}
                                ref={
                                    locateTarget?.kind === "footer"
                                        ? locatedRow
                                        : undefined
                                }
                                className={cn(
                                    "hover:bg-transparent",
                                    locateTarget?.kind === "footer" &&
                                        "bg-info-soft/25 ring-1 ring-info-border ring-inset motion-safe:animate-pulse",
                                )}
                            >
                                <TableCell className="text-xs whitespace-normal">
                                    整单拟核销合计
                                    {filteredLines.length !== lines.length ||
                                    moreCount > 0 ? (
                                        <span className="num ml-2 font-normal text-muted-foreground">
                                            （当前显示 {filteredLines.length}{" "}
                                            项，合计按整单）
                                        </span>
                                    ) : null}
                                </TableCell>
                                <TableCell
                                    data-align="end"
                                    className="num font-semibold"
                                >
                                    {totals.total !== undefined ? (
                                        <MoneyValue
                                            value={totals.total}
                                            display="exact"
                                        />
                                    ) : (
                                        <span className="text-xs text-muted-foreground">
                                            金额待核对
                                        </span>
                                    )}
                                </TableCell>
                            </TableRow>
                        </TableFooter>
                    </Table>
                ) : (
                    <p className="text-xs text-muted-foreground">
                        {materials.isPending
                            ? "正在读取本次提交的核销明细…"
                            : "暂无可查看的核销分配明细，请联系提交人核对。"}
                    </p>
                )}
                <ApprovalLinePagination
                    idPrefix={`${prefix}-lines`}
                    filteredCount={filteredLines.length}
                    page={activePage}
                    pageSize={pageSize}
                    onPageChange={setPage}
                    onPageSizeChange={(next) => {
                        setPageSize(next)
                        setPage(1)
                    }}
                    ariaLabel="核销明细分页"
                />
                {moreCount > 0 ? (
                    <Alert variant="warning">
                        <AlertDescription>
                            <span className="num">
                                摘要包含 {lines.length} 笔，另有 {moreCount}{" "}
                                笔未包含在摘要中。请联系提交人核对完整分配明细。
                            </span>
                        </AlertDescription>
                    </Alert>
                ) : null}
            </DetailWorkspaceSection>

            {!hideEmbeddedMaterials || materials.isError ? (
                <DetailWorkspaceSection
                    id={materialSectionId}
                    tabIndex={-1}
                    title={hideEmbeddedMaterials ? undefined : "到账凭证及附件"}
                    subtitle={
                        !hideEmbeddedMaterials && attachments.length > 0 ? (
                            <span className="num">
                                共 {attachments.length} 个文件
                            </span>
                        ) : undefined
                    }
                    ariaLabel="本次提交凭证"
                    className={cn(
                        "scroll-mt-16 rounded-md transition-colors",
                        materialSectionHighlighted &&
                            "bg-info-soft/25 ring-1 ring-info-border motion-safe:animate-pulse",
                    )}
                >
                    {materials.isPending ? (
                        <p
                            role="status"
                            className="flex items-center gap-2 text-xs text-muted-foreground"
                        >
                            <Spinner />
                            正在读取本次提交的凭证…
                        </p>
                    ) : materials.isError ? (
                        <BusinessFailureState
                            id={`${prefix}-materials-retry`}
                            title="提交资料读取失败"
                            error={materials.error}
                            onRetry={() => void materials.refetch()}
                        />
                    ) : (
                        <ApprovalMaterialFiles
                            instanceId={instanceId}
                            attachments={attachments}
                            idPrefix={`${prefix}-file`}
                            emptyMessage="本次提交未附到账凭证，请联系提交人核对。"
                            onSelectFile={onSelectFile}
                            activeFileId={activeFileId}
                        />
                    )}
                </DetailWorkspaceSection>
            ) : null}
        </div>
    )
}

function ReceiptKpiLabel({
    label,
    basis,
    tooltipId,
    tooltip,
}: {
    label: string
    basis: string
    tooltipId: string
    tooltip: string
}) {
    return (
        <div className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
            <span>{label}</span>
            <span className="text-tiny text-muted-foreground/80">{basis}</span>
            <Tooltip>
                <TooltipTrigger
                    render={
                        <button
                            id={tooltipId}
                            type="button"
                            aria-label={`${label}口径说明`}
                            className="inline-flex size-4 cursor-help items-center justify-center rounded-sm text-muted-foreground/80 transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-primary"
                        />
                    }
                >
                    <CircleQuestionMarkIcon
                        aria-hidden="true"
                        className="size-3"
                    />
                </TooltipTrigger>
                <TooltipContent className="block">{tooltip}</TooltipContent>
            </Tooltip>
        </div>
    )
}

function ReceiptCoverage({
    totals,
}: {
    totals: ReturnType<typeof getAllocationTotals>
}) {
    const { balanceSign, percent, total } = totals
    const barWidth =
        percent !== undefined
            ? balanceSign === -1
                ? "100%"
                : `${minFixed(percent, "100", { maxScale: 2, outputScale: 2 })}%`
            : undefined
    const state =
        balanceSign === -1
            ? {
                  label: "超额分配",
                  bar: "bg-destructive",
                  text: "text-destructive-soft-foreground",
              }
            : balanceSign === 0
              ? {
                    label: "已全额分配",
                    bar: "bg-success",
                    text: "text-success-soft-foreground",
                }
              : total === "0.00"
                ? {
                      label: "尚未分配",
                      bar: "bg-warning",
                      text: "text-warning-soft-foreground",
                  }
                : {
                      label: "部分分配",
                      bar: "bg-warning",
                      text: "text-warning-soft-foreground",
                  }

    return (
        <div className="mt-1.5 space-y-1">
            <div
                aria-hidden="true"
                className="h-1.5 overflow-hidden rounded-full bg-muted"
            >
                {barWidth !== undefined ? (
                    <div
                        className={cn(
                            "h-full max-w-full rounded-full",
                            state.bar,
                        )}
                        style={{ width: barWidth }}
                    />
                ) : null}
            </div>
            <p
                className={cn(
                    "flex flex-wrap items-center gap-x-1.5 text-tiny",
                    percent !== undefined
                        ? state.text
                        : "text-muted-foreground",
                )}
            >
                {percent !== undefined ? (
                    <>
                        <span>{state.label}</span>
                        <span>
                            覆盖{" "}
                            <RateValue
                                value={percent}
                                precision={2}
                                className="text-inherit"
                            />
                        </span>
                    </>
                ) : (
                    "覆盖率待核对"
                )}
            </p>
        </div>
    )
}
