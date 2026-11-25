"use client"

import { useEffect, useEffectEvent, useMemo, useRef, useState } from "react"
import { useSelector } from "@tanstack/react-form"
import { ArrowLeftIcon, ArrowRightIcon, ChevronDownIcon } from "lucide-react"
import {
    FormalActionResult,
    MoneyValue,
    QuantityValue,
    WorkspaceTaskFooter,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { DatePicker } from "@/components/ui/date-picker"
import {
    Dialog,
    DialogBody,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Input } from "@/components/ui/input"
import { LoadingButton } from "@/components/ui/loading-button"
import { useResubmissionReason } from "@/features/approval-workflow/hooks/use-resubmission-reason"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compactFixed,
    compareDecimal,
    divideFixed,
    multiplyFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"
import { useWorkspaceDirtyState } from "@/lib/workspace-form-guard"
import { isWorkspaceFrameActive } from "@/lib/workspace-hotkeys"
import type { PurchaseDeliveryLineOption } from "../api/purchase-delivery-options"
import {
    useAcquireDraftTokenMutation,
    usePurchaseDeliveryOptionsQuery,
    usePurchaseOrderCenterQuery,
    useSavePurchaseOrderDraftMutation,
    useSubmitPurchaseOrderMutation,
} from "../hooks/queries"
import { usePurchaseOrderDetailCommandState } from "../hooks/use-purchase-order-detail-command-state"
import { usePurchaseOrderDetailEditGuard } from "../hooks/use-purchase-order-detail-edit-guard"
import { usePurchaseOrderDetailPermissions } from "../hooks/use-purchase-order-detail-permissions"
import {
    afterDueDates,
    deliveryAddressKey,
    deliveryRowExceedsAvailable,
    editableDeliveryRows,
    expectedDateAfterDue,
    positiveDeliveryRows,
    removedDeliveryRows,
} from "../lib/delivery-adjustment"
import {
    draftDeliveryProblem,
    draftLinePayload,
    type DraftLineEdit,
    type DraftLineEdits,
} from "../lib/draft-line-payload"
import type {
    PurchaseOrderCenterView,
    SubmitPurchaseOrderPayload,
} from "../types"
import { PurchaseDeliveryAllocationEditor } from "./purchase-delivery-allocation-editor"
import { PurchaseOrderApprovalArea } from "./purchase-order-approval-area"
import {
    buildPurchaseSourceImpact,
    PurchaseSourceDelta,
} from "./purchase-source-update-context"

type DraftAction = "save-draft" | "submit"
type DraftLineForm = Omit<DraftLineEdit, "inputTaxRate"> & {
    taxPercent: string
}
type DraftFields = Record<string, DraftLineForm>
type PurchaseLine = PurchaseOrderCenterView["currentContent"]["lines"][number]

const MONEY_SCALE = { maxScale: 2, outputScale: 2 }

function taxPercent(rate: string) {
    return compactFixed(
        multiplyFixed(compactFixed(rate), "100", {
            leftMaxScale: 6,
            rightMaxScale: 0,
            outputScale: 4,
        }),
    )
}

function initialDraftFields(order: PurchaseOrderCenterView): DraftFields {
    return Object.fromEntries(
        order.currentContent.lines.map((line) => [
            line.lineId,
            {
                quantity: line.quantity,
                unitCostGross: line.unitCostGross,
                taxPercent: taxPercent(line.inputTaxRate),
                expectedDeliveryDate: line.expectedDeliveryDate,
            },
        ]),
    )
}

function inputTaxRate(percent: string) {
    try {
        return compactFixed(
            divideFixed(compactFixed(percent.trim()), "100", {
                numeratorMaxScale: 4,
                denominatorMaxScale: 0,
                outputScale: 6,
            }),
        )
    } catch {
        return ""
    }
}

function toDraftEdits(fields: DraftFields): DraftLineEdits {
    return Object.fromEntries(
        Object.entries(fields).map(([id, { taxPercent, ...line }]) => [
            id,
            {
                ...line,
                quantity:
                    line.quantity == null
                        ? undefined
                        : compactFixed(line.quantity.trim()),
                unitCostGross:
                    line.unitCostGross == null
                        ? undefined
                        : compactFixed(line.unitCostGross.trim()),
                inputTaxRate: inputTaxRate(taxPercent),
            },
        ]),
    )
}

type DraftLineIssue = {
    lineId: string | null
    field: "quantity" | "cost" | "tax" | "date" | "delivery"
    message: string
}

const sunkenDraftInputClass =
    "border-transparent bg-surface-sunken shadow-none hover:bg-card focus-visible:bg-card"

function collectDraftLineIssues(
    order: PurchaseOrderCenterView,
    edits: DraftLineEdits,
    options: ReadonlyMap<string, PurchaseDeliveryLineOption>,
): DraftLineIssue[] {
    const issues: DraftLineIssue[] = []
    for (const line of order.currentContent.lines) {
        const edit = edits[line.lineId]
        if (!edit) continue
        const item = line.lineType === "ITEM_SERVICE"
        const option = options.get(line.salesOrderLineId ?? "")
        const fail = (
            field: DraftLineIssue["field"],
            message: string,
        ): DraftLineIssue => ({
            lineId: line.lineId,
            field,
            message: `「${line.itemName}」${message}`,
        })
        let numericInvalid = false
        try {
            if (item) {
                const quantity = compareDecimal(edit.quantity ?? "", "0", 6)
                if (
                    quantity < 0 ||
                    (quantity === 0 && !order.sourceUpdate?.affected)
                ) {
                    issues.push(
                        fail(
                            "quantity",
                            order.sourceUpdate?.affected
                                ? "数量须为 0 或正数。"
                                : "数量须为正数。",
                        ),
                    )
                }
            }
        } catch {
            numericInvalid = true
            issues.push(
                fail(
                    "quantity",
                    "请填写有效的数量、含税单价与税率。数量最多 6 位小数，单价和百分比税率最多 4 位小数。",
                ),
            )
        }
        if (!numericInvalid) {
            try {
                if (compareDecimal(edit.unitCostGross ?? "", "0", 4) <= 0)
                    issues.push(fail("cost", "含税单价须为正数。"))
            } catch {
                numericInvalid = true
                issues.push(
                    fail(
                        "cost",
                        "请填写有效的数量、含税单价与税率。数量最多 6 位小数，单价和百分比税率最多 4 位小数。",
                    ),
                )
            }
        }
        if (!numericInvalid) {
            try {
                if (
                    compareDecimal(edit.inputTaxRate, "0", 6) < 0 ||
                    compareDecimal(edit.inputTaxRate, "1", 6) > 0
                )
                    issues.push(fail("tax", "税率须在 0% 到 100% 之间。"))
            } catch {
                issues.push(
                    fail(
                        "tax",
                        "请填写有效的数量、含税单价与税率。数量最多 6 位小数，单价和百分比税率最多 4 位小数。",
                    ),
                )
            }
        }
        if (
            edit.expectedDeliveryDate &&
            edit.expectedDeliveryDate !== line.expectedDeliveryDate &&
            option?.salesDueDate &&
            expectedDateAfterDue(edit.expectedDeliveryDate, option.salesDueDate)
        )
            issues.push(fail("date", "预计交付不能晚于销售履约期限。"))
        if (
            option?.requiresAddresses &&
            edit.deliveryAllocations?.some((row) =>
                deliveryRowExceedsAvailable(row, option),
            )
        )
            issues.push(fail("delivery", "收件分配数量超出地址可分配数量。"))
    }
    const deliveryProblem = draftDeliveryProblem(order, edits)
    if (deliveryProblem) {
        issues.push({
            lineId: null,
            field: "delivery",
            message: deliveryProblem,
        })
    }
    return issues
}

function validateLines(
    order: PurchaseOrderCenterView,
    edits: DraftLineEdits,
    options: ReadonlyMap<string, PurchaseDeliveryLineOption>,
) {
    return collectDraftLineIssues(order, edits, options)[0]?.message ?? null
}

/** 编辑、影响核对与结果恢复共用同一个弹窗。 */
export function PurchaseSourceUpdateDraftDialog({
    order,
    idPrefix = `purchase-source-update-draft-${toAutomationIdSegment(order.identity.purchaseOrderId)}`,
    onClose,
    onSubmitted,
    inline = false,
}: {
    order: PurchaseOrderCenterView
    idPrefix?: string
    onClose: () => void
    onSubmitted?: () => void
    inline?: boolean
}) {
    const purchaseOrderId = order.identity.purchaseOrderId
    const query = usePurchaseOrderCenterQuery(purchaseOrderId)
    const permissions = usePurchaseOrderDetailPermissions(order)
    const { commandLedger, result, setResult } =
        usePurchaseOrderDetailCommandState(purchaseOrderId)
    const {
        mutate: acquireToken,
        data: token,
        isPending: tokenPending,
        error: tokenError,
    } = useAcquireDraftTokenMutation()
    const saveMutation = useSavePurchaseOrderDraftMutation()
    const submitMutation = useSubmitPurchaseOrderMutation()
    const [baseline, setBaseline] = useState(order)
    const [step, setStep] = useState<"edit" | "confirm">(
        order.sourceUpdate?.awaitingSubmission && permissions.canSubmit
            ? "confirm"
            : "edit",
    )
    const [recovery, setRecovery] = useState<DraftAction | null>(null)
    const [busy, setBusy] = useState(false)
    const [refreshRequired, setRefreshRequired] = useState(false)
    const [highlightedLineId, setHighlightedLineId] = useState<string | null>(
        null,
    )
    const highlightTimer = useRef<ReturnType<typeof setTimeout>>(null)
    const running = useRef(false)
    const resultRef = useRef<HTMLDivElement>(null)
    const inlineRef = useRef<HTMLElement>(null)
    useEffect(
        () => () => {
            if (highlightTimer.current) clearTimeout(highlightTimer.current)
        },
        [],
    )
    const defaultValues = useMemo(
        () => ({ lines: initialDraftFields(baseline) }),
        [baseline],
    )
    const form = useAppForm({ defaultValues })
    const fields = useSelector(form.store, (state) => state.values.lines)
    const edits = useMemo(() => toDraftEdits(fields), [fields])
    const optionsQuery = usePurchaseDeliveryOptionsQuery(
        purchaseOrderId,
        permissions.canEdit,
    )
    const options = useMemo(
        () =>
            new Map(
                optionsQuery.data?.map((option) => [
                    option.salesOrderLineId,
                    option,
                ]),
            ),
        [optionsQuery.data],
    )
    const impact = useMemo(
        () => buildPurchaseSourceImpact(baseline),
        [baseline],
    )
    const lineIssues = useMemo(
        () => collectDraftLineIssues(baseline, edits, options),
        [baseline, edits, options],
    )
    const locateFirstIssue = () => {
        const first = lineIssues[0]
        if (!first) return
        setStep("edit")
        setResult({
            status: "rejected",
            title: "请核对采购明细",
            description: first.message,
        })
        if (!first.lineId) return
        if (highlightTimer.current) clearTimeout(highlightTimer.current)
        setHighlightedLineId(first.lineId)
        highlightTimer.current = setTimeout(
            () => setHighlightedLineId(null),
            2200,
        )
        const linePrefix = `${idPrefix}-line-${toAutomationIdSegment(first.lineId)}`
        const targetId =
            first.field === "delivery"
                ? `${linePrefix}-delivery-toggle`
                : `${linePrefix}-${first.field}`
        requestAnimationFrame(() => {
            const element =
                document.getElementById(targetId) ??
                document.getElementById(linePrefix)
            const behavior = window.matchMedia(
                "(prefers-reduced-motion: reduce)",
            ).matches
                ? "auto"
                : "smooth"
            element?.scrollIntoView({ block: "center", behavior })
            element?.focus({ preventScroll: true })
        })
    }
    const pending = busy || tokenPending
    const locked = busy || recovery != null
    const lockVersion = baseline.identity.lockVersion
    const newerVersion =
        order.identity.lockVersion > lockVersion ||
        (token?.lockVersion ?? lockVersion) > lockVersion
    const mustRefresh = refreshRequired || newerVersion
    const canEdit = permissions.canEdit && !mustRefresh
    const editReady =
        canEdit && !tokenPending && token?.lockVersion === lockVersion
    const resubmitting =
        baseline.identity.status === "REJECTED" ||
        baseline.approval?.instance?.status === "REJECTED"
    const rework = useResubmissionReason({
        open: step === "confirm",
        required: resubmitting,
        id: `${idPrefix}-resubmission-reason`,
        disabled: locked || !canEdit || !permissions.canSubmit,
    })

    useEffect(() => {
        if (permissions.canEdit) acquireToken(purchaseOrderId)
    }, [acquireToken, permissions.canEdit, purchaseOrderId])

    useEffect(() => {
        if (result && result.status !== "succeeded") {
            resultRef.current?.scrollIntoView({ block: "nearest" })
        }
    }, [result])

    // 保留结果待核对期间的原命令，即使单据刷新后已经进入审批。
    useWorkspaceDirtyState({ dirty: recovery != null, busy: locked })

    const acceptBaseline = (latest: PurchaseOrderCenterView) => {
        setBaseline(latest)
        form.reset({ lines: initialDraftFields(latest) })
        setStep("edit")
        setRefreshRequired(false)
        acquireToken(purchaseOrderId)
    }

    const reloadDraft = async () => {
        if (running.current || recovery) return
        running.current = true
        setBusy(true)
        try {
            const latest = await query.refetch()
            if (latest.isSuccess && latest.data) {
                acceptBaseline(latest.data)
                setResult(null)
            } else {
                setResult({
                    status: "blocked",
                    title: "草稿读取失败",
                    description: "请重新加载草稿。",
                })
            }
        } finally {
            running.current = false
            setBusy(false)
        }
    }

    function checkDraft() {
        const problem = validateLines(baseline, edits, options)
        if (problem) {
            setStep("edit")
            setResult({
                status: "rejected",
                title: "请核对采购明细",
                description: problem,
            })
        }
        return !problem
    }

    async function runCommand(action: DraftAction, reason?: string) {
        if (running.current || pending) return false
        const previous = commandLedger.peek<SubmitPurchaseOrderPayload>(action)
        if (
            !previous &&
            (!editReady ||
                (action === "submit" && !permissions.canSubmit) ||
                !checkDraft())
        )
            return false
        if (!previous && !token) return false

        running.current = true
        setBusy(true)
        try {
            const command =
                previous ??
                commandLedger.acquire<SubmitPurchaseOrderPayload>(
                    action,
                    `purchase:${purchaseOrderId}:${action === "save-draft" ? "save" : "submit"}`,
                    {
                        purchaseOrderId,
                        resubmissionReason: reason,
                        expectedLockVersion: lockVersion,
                        expectedDraftContentHash:
                            baseline.currentContent.subjectHash ??
                            `v${baseline.currentContent.version}`,
                        draftEditToken: token?.draftEditToken ?? "",
                        paymentTermCode: baseline.header.paymentTermCode,
                        lines: draftLinePayload(baseline, edits),
                    },
                )
            const payload = {
                ...command.payload,
                idempotencyKey: command.idempotencyKey,
            }
            const response =
                action === "save-draft"
                    ? await saveMutation.mutateAsync({
                          ...payload,
                          paymentTermLabel: baseline.header.paymentTermLabel,
                      })
                    : await submitMutation.mutateAsync(payload)
            commandLedger.settle(action, response.status)

            if (response.status === "unknown") {
                setRecovery(action)
                setResult({
                    status: "unknown",
                    title:
                        action === "submit"
                            ? "提交结果待核对"
                            : "保存结果待核对",
                    description: response.message,
                })
                return false
            }
            setRecovery(null)
            if (response.status === "failed") {
                if (
                    response.code === "CONFLICT" ||
                    response.code === "DATA_SCOPE_CHANGED"
                ) {
                    setRefreshRequired(true)
                    void query.refetch()
                }
                setResult({
                    status: "rejected",
                    title: action === "submit" ? "提交失败" : "保存失败",
                    description: response.message,
                })
                return false
            }
            if (action === "submit") {
                onSubmitted?.()
                onClose()
                return true
            }
            setResult({
                status: "succeeded",
                title: "草稿已保存",
                description: "核对影响后提交审批。",
            })
            const latest = await query.refetch()
            if (
                latest.isSuccess &&
                latest.data?.identity.lockVersion === response.data.lockVersion
            ) {
                acceptBaseline(latest.data)
            } else {
                setRefreshRequired(true)
            }
            return true
        } catch (error) {
            // 命令已登记时保留原请求用于核对，写入前的本地错误允许修复后重试。
            const recorded = Boolean(commandLedger.peek(action))
            if (recorded) setRecovery(action)
            setResult({
                status: recorded ? "unknown" : "blocked",
                title: recorded ? "处理结果待核对" : "暂时无法操作",
                description: getErrorMessage(error, "请重试本次操作。"),
            })
            return false
        } finally {
            running.current = false
            setBusy(false)
        }
    }

    const guard = usePurchaseOrderDetailEditGuard({
        mode: "edit",
        order: baseline,
        lineEdits: edits,
        busy: locked,
        onSave: () => runCommand("save-draft"),
    })
    const requestClose = () => {
        if (!locked) guard.requestLeave(onClose)
    }
    const goToConfirm = () => {
        if (!editReady || locked || !permissions.canSubmit || !checkDraft())
            return
        guard.setLeaveGuardOpen(false)
        setResult(null)
        setStep("confirm")
    }
    const submit = () => {
        if (rework.valid) void runCommand("submit", rework.reason)
    }
    const onDraftKeyDown = (event: KeyboardEvent) => {
        if (
            (!event.metaKey && !event.ctrlKey) ||
            event.repeat ||
            event.isComposing ||
            !isWorkspaceFrameActive()
        )
            return
        if (event.key === "Enter") {
            event.preventDefault()
            if (recovery) void runCommand(recovery)
            else if (step === "edit") goToConfirm()
            else submit()
        } else if (event.key.toLowerCase() === "s" && step === "edit") {
            event.preventDefault()
            if (!locked) void runCommand("save-draft")
        }
    }
    const onInlineKeyDown = useEffectEvent(onDraftKeyDown)
    useEffect(() => {
        const section = inlineRef.current
        if (!inline || !section) return
        section.focus()
        const listener = (event: KeyboardEvent) => onInlineKeyDown(event)
        section.addEventListener("keydown", listener)
        return () => section.removeEventListener("keydown", listener)
    }, [inline])
    const description =
        step === "edit"
            ? "按来源销售变更调整采购明细。"
            : "核对明细变化与审批路线后提交。"
    const body = (
        <>
            {result ? (
                <div ref={resultRef}>
                    <FormalActionResult {...result} />
                </div>
            ) : null}
            {mustRefresh && !recovery ? (
                <Alert variant="warning">
                    <AlertTitle>采购草稿需要重新加载</AlertTitle>
                    <AlertDescription>
                        重新加载会以最新草稿替换当前输入，请先核对未保存的修改。
                    </AlertDescription>
                </Alert>
            ) : null}
            {guard.leaveGuardOpen && !recovery ? (
                <Alert variant="warning">
                    <AlertTitle>有未保存的修改</AlertTitle>
                    <AlertDescription className="space-y-2">
                        <p>离开前可保存草稿。</p>
                        <div className="flex flex-wrap gap-2">
                            <Button
                                id={`${idPrefix}-keep-editing`}
                                size="sm"
                                variant="ghost"
                                onClick={() => guard.setLeaveGuardOpen(false)}
                            >
                                继续编辑
                            </Button>
                            <Button
                                id={`${idPrefix}-discard`}
                                size="sm"
                                variant="outline"
                                disabled={locked}
                                onClick={guard.discardAndLeave}
                            >
                                放弃修改并离开
                            </Button>
                            <LoadingButton
                                id={`${idPrefix}-save-and-close`}
                                size="sm"
                                variant="outline"
                                disabled={!editReady || locked}
                                loading={busy}
                                onClick={() => void guard.saveAndLeave()}
                            >
                                保存并离开
                            </LoadingButton>
                        </div>
                    </AlertDescription>
                </Alert>
            ) : null}

            {step === "edit" ? (
                <>
                    {tokenError && !mustRefresh ? (
                        <Alert variant="warning">
                            <AlertTitle>暂时无法编辑采购单</AlertTitle>
                            <AlertDescription>
                                {getErrorMessage(
                                    tokenError,
                                    "请重试读取采购草稿。",
                                )}
                            </AlertDescription>
                        </Alert>
                    ) : tokenPending ? (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            正在读取采购草稿…
                        </p>
                    ) : null}
                    {optionsQuery.isError ? (
                        <p
                            role="status"
                            className="text-xs text-muted-foreground"
                        >
                            销售履约期限与收件资料读取失败，当前只能调整数量、含税单价和税率。
                        </p>
                    ) : null}
                    <fieldset
                        disabled={!editReady || locked}
                        className="min-w-0 divide-y divide-border/60"
                    >
                        <legend className="mb-1 text-section-title font-semibold">
                            采购明细
                        </legend>
                        {baseline.currentContent.lines.map((line) => {
                            const value = fields[line.lineId]
                            if (!value) return null
                            return (
                                <DraftLineFields
                                    key={line.lineId}
                                    id={`${idPrefix}-line-${toAutomationIdSegment(line.lineId)}`}
                                    line={line}
                                    value={value}
                                    option={options.get(
                                        line.salesOrderLineId ?? "",
                                    )}
                                    targetQuantity={
                                        impact.byLine.get(line.lineId)
                                            ?.targetQuantity
                                    }
                                    highlighted={
                                        highlightedLineId === line.lineId
                                    }
                                    reasons={impact.byLine
                                        .get(line.lineId)
                                        ?.reasons.map(
                                            (reason) => reason.description,
                                        )}
                                    onPatch={(patch) =>
                                        form.setFieldValue(
                                            "lines",
                                            (current) => ({
                                                ...current,
                                                [line.lineId]: {
                                                    ...current[line.lineId],
                                                    ...patch,
                                                },
                                            }),
                                        )
                                    }
                                />
                            )
                        })}
                    </fieldset>
                </>
            ) : (
                <>
                    <DraftConfirmation order={baseline} edits={edits} />
                    <PurchaseOrderApprovalArea
                        phase="confirm"
                        approval={baseline.approval}
                    />
                    {rework.field}
                </>
            )}
        </>
    )
    const footer = (
        <>
            {recovery ? (
                <LoadingButton
                    id={`${idPrefix}-recover`}
                    variant="default"
                    loading={pending}
                    onClick={() => void runCommand(recovery)}
                >
                    {recovery === "submit" ? "核对提交结果" : "核对保存结果"}
                </LoadingButton>
            ) : mustRefresh ? (
                <>
                    <Button
                        id={`${idPrefix}-close-conflict`}
                        variant="outline"
                        disabled={locked}
                        onClick={requestClose}
                    >
                        关闭
                    </Button>
                    <LoadingButton
                        id={`${idPrefix}-reload`}
                        variant="default"
                        loading={pending}
                        onClick={() => void reloadDraft()}
                    >
                        重新加载草稿
                    </LoadingButton>
                </>
            ) : step === "edit" ? (
                <>
                    {!tokenError && lineIssues.length > 0 ? (
                        <Button
                            id={`${idPrefix}-locate-issue`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={locateFirstIssue}
                        >
                            <span className="num">{lineIssues.length}</span>{" "}
                            项待完善 · 定位
                        </Button>
                    ) : null}
                    <Button
                        id={`${idPrefix}-cancel`}
                        variant="ghost"
                        disabled={locked}
                        onClick={requestClose}
                    >
                        取消
                    </Button>
                    {tokenError ? (
                        <LoadingButton
                            id={`${idPrefix}-retry-token`}
                            variant="default"
                            loading={pending}
                            onClick={() => acquireToken(purchaseOrderId)}
                        >
                            重试读取
                        </LoadingButton>
                    ) : (
                        <>
                            <LoadingButton
                                id={`${idPrefix}-save`}
                                variant={
                                    permissions.canSubmit
                                        ? "outline"
                                        : "default"
                                }
                                loading={saveMutation.isPending}
                                disabled={!editReady || locked}
                                onClick={() => void runCommand("save-draft")}
                            >
                                保存草稿
                            </LoadingButton>
                            {permissions.canSubmit ? (
                                <Button
                                    id={`${idPrefix}-review`}
                                    variant="default"
                                    disabled={!editReady || locked}
                                    onClick={goToConfirm}
                                >
                                    核对影响
                                    <ArrowRightIcon
                                        aria-hidden="true"
                                        data-icon="inline-end"
                                    />
                                </Button>
                            ) : null}
                        </>
                    )}
                </>
            ) : (
                <>
                    <Button
                        id={`${idPrefix}-back`}
                        variant="outline"
                        disabled={locked}
                        onClick={() => setStep("edit")}
                    >
                        <ArrowLeftIcon
                            aria-hidden="true"
                            data-icon="inline-start"
                        />
                        返回编辑
                    </Button>
                    <LoadingButton
                        id={`${idPrefix}-submit`}
                        variant="default"
                        loading={submitMutation.isPending}
                        disabled={
                            !editReady ||
                            locked ||
                            !permissions.canSubmit ||
                            !rework.valid
                        }
                        onClick={submit}
                    >
                        确认提交审批
                    </LoadingButton>
                </>
            )}
        </>
    )

    if (inline) {
        const summary = (
            <dl
                aria-live="polite"
                className="mr-auto flex flex-wrap items-baseline gap-x-3 gap-y-1 text-xs"
            >
                <div className="flex items-baseline gap-1">
                    <dt className="text-muted-foreground">受影响</dt>
                    <dd className="num font-medium">{impact.byLine.size} 行</dd>
                </div>
                <div className="flex items-baseline gap-1">
                    <dt className="text-muted-foreground">处理状态</dt>
                    <dd>
                        {recovery
                            ? "结果待核对"
                            : step === "edit"
                              ? "编辑草稿"
                              : "核对影响"}
                    </dd>
                </div>
            </dl>
        )
        return (
            <section
                ref={inlineRef}
                id={`${idPrefix}-editor`}
                aria-labelledby={`${idPrefix}-title`}
                tabIndex={-1}
                className="min-w-0 space-y-4 bg-card py-4 @container/source-update"
            >
                <header>
                    <h2
                        id={`${idPrefix}-title`}
                        className="text-section-title font-semibold"
                    >
                        修改采购原单
                    </h2>
                    <p className="text-xs text-muted-foreground">
                        {description}
                    </p>
                </header>
                {body}
                <WorkspaceTaskFooter
                    summary={summary}
                    fallback={
                        <div className="flex w-full min-w-0 flex-wrap items-center justify-between gap-x-3 gap-y-2">
                            {summary}
                            <div className="ml-auto flex min-w-0 flex-wrap items-center justify-end gap-2">
                                {footer}
                            </div>
                        </div>
                    }
                >
                    {footer}
                </WorkspaceTaskFooter>
            </section>
        )
    }

    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open) requestClose()
            }}
        >
            <DialogContent
                size="2xl"
                scroll="body"
                closeButtonId={`${idPrefix}-close`}
                showCloseButton={!locked}
                onKeyDown={(event) => onDraftKeyDown(event.nativeEvent)}
                className="bg-card"
            >
                <DialogHeader>
                    <DialogTitle>修改采购原单</DialogTitle>
                    <DialogDescription>{description}</DialogDescription>
                </DialogHeader>
                <DialogBody className="min-w-0 space-y-4 @container/source-update">
                    {body}
                </DialogBody>
                <DialogFooter className="flex-wrap">{footer}</DialogFooter>
            </DialogContent>
        </Dialog>
    )
}

function DraftLineFields({
    id,
    line,
    value,
    option,
    targetQuantity,
    highlighted = false,
    reasons,
    onPatch,
}: {
    id: string
    line: PurchaseLine
    value: DraftLineForm
    option?: PurchaseDeliveryLineOption
    targetQuantity?: string
    highlighted?: boolean
    reasons?: string[]
    onPatch: (patch: Partial<DraftLineForm>) => void
}) {
    const original = line.deliveryAllocations ?? []
    const deliveryRows = option?.requiresAddresses
        ? (value.deliveryAllocations ?? editableDeliveryRows(option, original))
        : undefined
    const updateQuantity = (quantity: string) => {
        const assigned = positiveDeliveryRows(original)
        const target =
            assigned.length === 1
                ? deliveryRows?.find(
                      (row) =>
                          deliveryAddressKey(row) ===
                          deliveryAddressKey(assigned[0]),
                  )
                : deliveryRows?.length === 1
                  ? deliveryRows[0]
                  : undefined
        if (
            target &&
            deliveryRows &&
            !positiveDeliveryRows(deliveryRows.filter((row) => row !== target))
                .length
        ) {
            onPatch({
                quantity,
                deliveryAllocations: deliveryRows.map((row) =>
                    row === target ? { ...row, quantity } : row,
                ),
            })
        } else {
            onPatch({ quantity })
        }
    }
    const isFee = line.lineType === "LOGISTICS_FEE"
    const canFillTargetQuantity =
        !isFee &&
        targetQuantity != null &&
        value.quantity?.trim() !== targetQuantity
    const dueDateLate = Boolean(
        !isFee &&
        option?.salesDueDate &&
        value.expectedDeliveryDate &&
        expectedDateAfterDue(value.expectedDeliveryDate, option.salesDueDate),
    )

    return (
        <article
            id={id}
            tabIndex={-1}
            className={cn(
                "min-w-0 space-y-2 py-3 transition-colors",
                highlighted &&
                    "rounded-md bg-info-soft/25 px-2 ring-1 ring-info-border motion-safe:animate-pulse",
            )}
        >
            <div className="min-w-0 wrap-anywhere">
                <h3 className="text-body-compact font-medium">
                    {line.itemName}
                </h3>
                {reasons?.length ? (
                    <ul className="num border-l-2 border-l-warning pl-2 text-xs text-warning-soft-foreground">
                        {[...new Set(reasons)].map((reason) => (
                            <li key={reason}>{reason}</li>
                        ))}
                    </ul>
                ) : null}
            </div>
            <div className="grid grid-cols-2 gap-x-3 gap-y-2 @[560px]/source-update:grid-cols-[repeat(3,minmax(0,1fr))_minmax(0,1.5fr)]">
                {!isFee ? (
                    <DraftNumberField
                        id={`${id}-quantity`}
                        label={
                            line.unitName ? `数量（${line.unitName}）` : "数量"
                        }
                        value={value.quantity ?? ""}
                        onChange={updateQuantity}
                        quickAction={
                            canFillTargetQuantity ? (
                                <Button
                                    id={`${id}-fill-target-qty`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className="h-5 px-1.5 text-xs text-primary"
                                    onClick={() =>
                                        updateQuantity(targetQuantity)
                                    }
                                >
                                    填目标量{" "}
                                    <span className="num">
                                        {targetQuantity}
                                    </span>
                                </Button>
                            ) : null
                        }
                    />
                ) : null}
                <DraftNumberField
                    id={`${id}-cost`}
                    label={isFee ? "含税费用" : "含税单价"}
                    value={value.unitCostGross ?? ""}
                    onChange={(unitCostGross) => onPatch({ unitCostGross })}
                />
                <DraftNumberField
                    id={`${id}-tax`}
                    label="税率（%）"
                    value={value.taxPercent}
                    onChange={(taxPercent) => onPatch({ taxPercent })}
                />
                {!isFee ? (
                    <div className="min-w-0 space-y-1 text-xs text-muted-foreground">
                        {option ? (
                            <>
                                <div className="flex flex-wrap items-center justify-between gap-1">
                                    <label htmlFor={`${id}-date`}>
                                        预计交付
                                    </label>
                                    {dueDateLate && option.salesDueDate ? (
                                        <Button
                                            id={`${id}-fill-due-date`}
                                            type="button"
                                            variant="ghost"
                                            size="xs"
                                            className="h-5 px-1.5 text-xs text-primary"
                                            onClick={() =>
                                                onPatch({
                                                    expectedDeliveryDate:
                                                        option.salesDueDate,
                                                })
                                            }
                                        >
                                            调至销售期限
                                        </Button>
                                    ) : null}
                                </div>
                                <DatePicker
                                    id={`${id}-date`}
                                    size="sm"
                                    className={cn(
                                        "num w-full min-w-0",
                                        sunkenDraftInputClass,
                                    )}
                                    clearable={false}
                                    value={value.expectedDeliveryDate ?? ""}
                                    disabledDates={
                                        option.salesDueDate
                                            ? afterDueDates(option.salesDueDate)
                                            : undefined
                                    }
                                    onValueChange={(expectedDeliveryDate) =>
                                        onPatch({ expectedDeliveryDate })
                                    }
                                />
                                {option.salesDueDate ? (
                                    <p className="num">
                                        销售期限 {option.salesDueDate}
                                    </p>
                                ) : null}
                            </>
                        ) : (
                            <>
                                <p>预计交付</p>
                                <p className="num text-body-compact">
                                    {line.expectedDeliveryDate ?? "—"}
                                </p>
                            </>
                        )}
                    </div>
                ) : null}
            </div>
            {option && deliveryRows ? (
                <Collapsible>
                    <CollapsibleTrigger
                        id={`${id}-delivery-toggle`}
                        className="group flex cursor-pointer items-center gap-1 text-xs text-muted-foreground hover:text-foreground"
                    >
                        <ChevronDownIcon
                            aria-hidden="true"
                            className="size-3 transition-transform group-data-panel-open:rotate-180"
                        />
                        收件分配{" "}
                        <span className="num">{deliveryRows.length} 处</span>
                    </CollapsibleTrigger>
                    <CollapsibleContent className="min-w-0 pt-2">
                        <PurchaseDeliveryAllocationEditor
                            id={`${id}-delivery`}
                            option={option}
                            rows={deliveryRows}
                            removed={removedDeliveryRows(option, original)}
                            quantity={value.quantity ?? ""}
                            unit={line.unitName}
                            onChange={(deliveryAllocations) =>
                                onPatch({ deliveryAllocations })
                            }
                        />
                    </CollapsibleContent>
                </Collapsible>
            ) : null}
        </article>
    )
}

function DraftNumberField({
    id,
    label,
    value,
    onChange,
    quickAction,
}: {
    id: string
    label: string
    value: string
    onChange: (value: string) => void
    quickAction?: React.ReactNode
}) {
    return (
        <div className="min-w-0 space-y-1">
            <div className="flex flex-wrap items-center justify-between gap-1">
                <label htmlFor={id} className="text-xs text-muted-foreground">
                    {label}
                </label>
                {quickAction}
            </div>
            <Input
                id={id}
                size="sm"
                inputMode="decimal"
                className={cn(
                    "num w-full min-w-0 text-right",
                    sunkenDraftInputClass,
                )}
                value={value}
                onChange={(event) => onChange(event.target.value)}
            />
        </div>
    )
}

function DraftConfirmation({
    order,
    edits,
}: {
    order: PurchaseOrderCenterView
    edits: DraftLineEdits
}) {
    const { lines, totals, costMasked } = order.currentContent
    const payload = draftLinePayload(order, edits)
    const comparisons = lines.map((line, index) => {
        const patch = payload[index]
        const quantityDelta =
            line.quantity != null && patch.quantity != null
                ? subtractFixed(patch.quantity, compactFixed(line.quantity), {
                      maxScale: 6,
                      outputScale: 6,
                  })
                : "0"
        const priceChanged =
            compareDecimal(
                patch.unitCostGross ?? "0",
                compactFixed(line.unitCostGross),
                4,
            ) !== 0
        const taxChanged =
            compareDecimal(
                patch.inputTaxRate,
                compactFixed(line.inputTaxRate),
                6,
            ) !== 0
        const edited =
            compareDecimal(quantityDelta, "0", 6) !== 0 ||
            priceChanged ||
            taxChanged ||
            patch.expectedDeliveryDate != null ||
            patch.deliveryAllocations != null
        const gross =
            line.lineType === "ITEM_SERVICE"
                ? multiplyFixed(
                      patch.quantity ?? "0",
                      patch.unitCostGross ?? "0",
                      {
                          leftMaxScale: 6,
                          rightMaxScale: 4,
                          outputScale: 2,
                      },
                  )
                : (patch.unitCostGross ?? line.grossAmount)
        return {
            line,
            patch,
            edited,
            priceChanged,
            taxChanged,
            quantityDelta,
            gross: sumFixed([compactFixed(gross)], {
                maxScale: 4,
                outputScale: 2,
            }),
        }
    })
    const changed = comparisons.filter(({ edited }) => edited)
    const amount = sumFixed(
        comparisons.map(({ gross }) => gross),
        MONEY_SCALE,
    )

    return (
        <section aria-label="影响核对" className="min-w-0 space-y-3">
            <dl className="flex flex-wrap items-baseline gap-x-6 gap-y-1.5 rounded-lg bg-surface-sunken px-4 py-3 text-xs">
                <div>
                    <dt className="text-muted-foreground">调整明细</dt>
                    <dd>
                        <QuantityValue
                            value={String(changed.length)}
                            unit="行"
                        />
                    </dd>
                </div>
                {!costMasked ? (
                    <div>
                        <dt className="text-muted-foreground">预计含税金额</dt>
                        <dd className="flex flex-wrap items-baseline gap-x-2">
                            <MoneyValue value={totals.gross} display="exact" />
                            <span aria-hidden="true">→</span>
                            <MoneyValue value={amount} display="exact" />
                            <PurchaseSourceDelta
                                value={subtractFixed(
                                    amount,
                                    compactFixed(totals.gross),
                                    MONEY_SCALE,
                                )}
                                money
                            />
                        </dd>
                    </div>
                ) : null}
            </dl>
            {changed.length ? (
                <ul className="divide-y divide-border/60 text-body-compact">
                    {changed.map(
                        ({
                            line,
                            patch,
                            quantityDelta,
                            gross,
                            priceChanged,
                            taxChanged,
                        }) => (
                            <li
                                key={line.lineId}
                                className="flex flex-wrap items-baseline gap-x-4 gap-y-1 py-2 wrap-anywhere"
                            >
                                <span className="min-w-0 font-medium">
                                    {line.itemName}
                                </span>
                                {compareDecimal(quantityDelta, "0", 6) !== 0 ? (
                                    <PurchaseSourceDelta
                                        value={quantityDelta}
                                        unit={line.unitName}
                                    />
                                ) : null}
                                {!costMasked &&
                                compareDecimal(
                                    gross,
                                    compactFixed(line.grossAmount),
                                    2,
                                ) !== 0 ? (
                                    <PurchaseSourceDelta
                                        value={subtractFixed(
                                            gross,
                                            compactFixed(line.grossAmount),
                                            MONEY_SCALE,
                                        )}
                                        money
                                    />
                                ) : null}
                                {priceChanged && !costMasked ? (
                                    <span className="text-xs text-muted-foreground">
                                        {line.lineType === "LOGISTICS_FEE"
                                            ? "含税费用 "
                                            : "含税单价 "}
                                        <MoneyValue
                                            value={line.unitCostGross}
                                            display="exact"
                                        />
                                        {" → "}
                                        <MoneyValue
                                            value={patch.unitCostGross}
                                            display="exact"
                                        />
                                    </span>
                                ) : null}
                                {taxChanged ? (
                                    <span className="num text-xs text-muted-foreground">
                                        税率 {taxPercent(line.inputTaxRate)}% →{" "}
                                        {taxPercent(patch.inputTaxRate)}%
                                    </span>
                                ) : null}
                                {patch.expectedDeliveryDate ? (
                                    <span className="num text-xs text-muted-foreground">
                                        预计交付{" "}
                                        {line.expectedDeliveryDate ?? "—"} →{" "}
                                        {patch.expectedDeliveryDate}
                                    </span>
                                ) : null}
                                {patch.deliveryAllocations ? (
                                    <span className="num text-xs text-muted-foreground">
                                        收件分配调整为{" "}
                                        {patch.deliveryAllocations.length} 处
                                    </span>
                                ) : null}
                            </li>
                        ),
                    )}
                </ul>
            ) : (
                <p className="text-xs text-muted-foreground">
                    按已保存的明细提交审批。
                </p>
            )}
            <p className="text-xs text-muted-foreground">
                提交后完成依据变更待办，采购单在审批期间不可修改。
            </p>
        </section>
    )
}
