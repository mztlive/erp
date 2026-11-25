"use client"

import * as React from "react"
import {
    ChevronUpIcon,
    MessageSquarePlusIcon,
    MessageSquareTextIcon,
    PlusIcon,
    ReceiptTextIcon,
    Trash2Icon,
} from "lucide-react"
import { useStore } from "@tanstack/react-form"
import { MoneyValue, detailWorkspaceStyles } from "@/components/business"
import { toFieldErrors, withForm } from "@/components/form"
import { Alert, AlertDescription } from "@/components/ui/alert"
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
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RequiredMark } from "@/components/ui/required-mark"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { Textarea } from "@/components/ui/textarea"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { describeMoney } from "@/lib/money-display"
import { cn } from "@/lib/utils"
import { feedbackFeeIssues } from "../lib/feedback-fee-validation"
import type { FeedbackSummary } from "../lib/feedback-summary"
import type { InquiryFeedbackData, InquiryFee } from "../types"
import { InquiryTaxRateInput } from "./inquiry-tax-rate-input"

const COMMON_FEE_PRESETS: readonly { key: string; label: string }[] = [
    { key: "freight", label: "运费" },
    { key: "packaging", label: "包装费" },
    { key: "sampling", label: "打样费" },
    { key: "printing", label: "印刷费" },
    { key: "handling", label: "装卸费" },
]

export function createExtraCostFee(name = ""): InquiryFee {
    return {
        fee_id: crypto.randomUUID(),
        name,
        amount_gross: "",
        tax_rate: "0",
        line_ids: [],
        public_note: "",
    }
}

const DEFAULT_FEEDBACK_VALUES: InquiryFeedbackData = {
    lines: [],
    fees: [],
}

function focusElementById(id: string) {
    requestAnimationFrame(() => {
        const element = document.getElementById(id)
        element?.focus()
        element?.scrollIntoView({ block: "nearest" })
    })
}

export const InquiryExtraCostsDialog = withForm({
    defaultValues: DEFAULT_FEEDBACK_VALUES,
    props: {
        open: false,
        onOpenChange: (_open: boolean) => {},
        triggerRef: undefined as
            | React.RefObject<HTMLButtonElement | null>
            | undefined,
        feeFocusRef: undefined as React.RefObject<string | null> | undefined,
        summary: null as FeedbackSummary | null,
        error: null as string | null,
        conflict: false,
        commandUnresolved: false,
        disabled: false,
        saving: false,
        onSave: () => {},
        onClearFeeError: () => {},
    },
    render: function ExtraCostsDialog({
        form,
        open,
        onOpenChange,
        triggerRef,
        feeFocusRef,
        summary,
        error,
        conflict,
        commandUnresolved,
        disabled,
        saving,
        onSave,
        onClearFeeError,
    }) {
        const fees = useStore(form.store, (state) => state.values.fees)
        const fieldMeta = useStore(form.store, (state) => state.fieldMeta)
        const [noteOverrides, setNoteOverrides] = React.useState<
            Record<string, boolean>
        >({})

        const existingNames = React.useMemo(
            () => new Set(fees.map((fee) => fee.name.trim()).filter(Boolean)),
            [fees],
        )
        const remainingPresets = React.useMemo(
            () =>
                COMMON_FEE_PRESETS.filter(
                    (preset) => !existingNames.has(preset.label),
                ),
            [existingNames],
        )
        const touchedIncompleteRowCount = React.useMemo(() => {
            const invalidRows = new Set<number>()
            fees.forEach((_, index) => {
                for (const key of [
                    "name",
                    "amount_gross",
                    "tax_rate",
                ] as const) {
                    const meta = fieldMeta[`fees[${index}].${key}`]
                    if (meta?.isTouched && meta.errors.length > 0) {
                        invalidRows.add(index)
                        break
                    }
                }
            })
            return invalidRows.size
        }, [fees, fieldMeta])

        const handleOpenChange = (next: boolean) => {
            if (!next) {
                setNoteOverrides({})
                if (feeFocusRef) feeFocusRef.current = null
            }
            onOpenChange(next)
        }

        const addFee = (presetName?: string) => {
            const fee = createExtraCostFee(presetName ?? "")
            form.pushFieldValue("fees", fee)
            const seg = toAutomationIdSegment(fee.fee_id)
            focusElementById(
                `procurement-inquiry-fee-${seg}-${presetName ? "gross" : "name"}`,
            )
        }

        const toggleNote = (feeId: string, currentlyExpanded: boolean) => {
            const next = !currentlyExpanded
            setNoteOverrides((current) => ({ ...current, [feeId]: next }))
            if (next) {
                focusElementById(
                    `procurement-inquiry-fee-${toAutomationIdSegment(feeId)}-note`,
                )
            }
        }

        return (
            <Dialog open={open} onOpenChange={handleOpenChange}>
                <DialogContent
                    id="procurement-feedback-fees-dialog"
                    size="xl"
                    scroll="body"
                    closeButtonId="procurement-feedback-fees-close"
                    finalFocus={triggerRef}
                    initialFocus={(openType) => {
                        const targetId = feeFocusRef?.current
                        if (targetId) {
                            return (
                                document.getElementById(targetId) ??
                                document.getElementById(
                                    "procurement-feedback-fees-dialog",
                                ) ??
                                true
                            )
                        }
                        if (openType === "keyboard") return true
                        return (
                            document.getElementById(
                                "procurement-feedback-fees-dialog",
                            ) ?? undefined
                        )
                    }}
                >
                    <div className="shrink-0 border-b border-border/70">
                        <DialogHeader className="gap-1 border-b-0 pb-3">
                            <div className="flex flex-wrap items-center gap-2">
                                <DialogTitle>登记额外成本</DialogTitle>
                                {fees.length > 0 && (
                                    <span className="num rounded-md bg-muted px-1.5 py-0.5 text-xs font-medium text-muted-foreground">
                                        {fees.length} 项
                                    </span>
                                )}
                            </div>
                            <DialogDescription>
                                录入运费、包装、打样等整单级费用；销售在报价结果中可见，按含税金额计入整单含税总计。
                            </DialogDescription>
                        </DialogHeader>

                        <div className="px-overlay-padding pb-3.5">
                            <div className="flex flex-wrap items-center justify-between gap-x-4 gap-y-1.5 rounded-lg bg-surface-sunken/80 px-3.5 py-2 text-xs">
                                <div className="flex flex-wrap items-center gap-x-2.5 gap-y-1">
                                    <span className="text-muted-foreground">
                                        额外成本合计
                                    </span>
                                    {summary?.feesTotal != null ? (
                                        <span className="text-body-compact font-semibold">
                                            <MoneyValue
                                                value={summary.feesTotal}
                                                display="exact"
                                                showHint={false}
                                            />
                                        </span>
                                    ) : (
                                        <span className="num text-body-compact font-medium text-muted-foreground">
                                            {fees.length === 0
                                                ? "¥0.00"
                                                : "待填写"}
                                        </span>
                                    )}
                                    {touchedIncompleteRowCount > 0 && (
                                        <span
                                            role="status"
                                            className="inline-flex items-center gap-1 rounded-full bg-warning-soft px-2 py-0.5 text-tiny font-medium text-warning-soft-foreground"
                                        >
                                            <span
                                                aria-hidden="true"
                                                className="size-1.5 shrink-0 rounded-full bg-warning"
                                            />
                                            <span className="num">
                                                {touchedIncompleteRowCount}
                                            </span>{" "}
                                            项待完善
                                        </span>
                                    )}
                                </div>

                                <div className="num flex flex-wrap items-center gap-x-2.5 gap-y-1 text-muted-foreground">
                                    <span>
                                        商品含税小计{" "}
                                        {summary?.goodsTotal != null ? (
                                            <MoneyValue
                                                value={summary.goodsTotal}
                                                display="exact"
                                                showHint={false}
                                            />
                                        ) : (
                                            <span className="text-muted-foreground/70">
                                                —
                                            </span>
                                        )}
                                    </span>
                                    {summary?.goodsTotal != null &&
                                        summary.grandTotal != null && (
                                            <>
                                                <span
                                                    aria-hidden="true"
                                                    className="select-none text-muted-foreground/40"
                                                >
                                                    ·
                                                </span>
                                                <span>
                                                    整单含税总计{" "}
                                                    <MoneyValue
                                                        value={
                                                            summary.grandTotal
                                                        }
                                                        display="exact"
                                                        showHint={false}
                                                    />
                                                </span>
                                            </>
                                        )}
                                </div>
                            </div>
                        </div>
                    </div>

                    <DialogBody className="space-y-4">
                        {error && (
                            <Alert variant="destructive">
                                <AlertDescription>{error}</AlertDescription>
                            </Alert>
                        )}
                        {(conflict || commandUnresolved) && (
                            <Alert variant="warning">
                                <AlertDescription>
                                    请先关闭弹窗，在采购反馈中核对
                                    {conflict
                                        ? "已保存内容的变化"
                                        : "上次提交结果"}
                                    ，再继续保存。
                                </AlertDescription>
                            </Alert>
                        )}

                        <fieldset disabled={disabled} className="min-w-0">
                            {fees.length === 0 ? (
                                <div className="flex flex-col items-center rounded-lg border border-dashed border-border/80 bg-surface-sunken/35 px-6 py-8 text-center">
                                    <div className="flex size-10 items-center justify-center rounded-full border border-border/60 bg-card text-muted-foreground shadow-2xs">
                                        <ReceiptTextIcon
                                            aria-hidden="true"
                                            className="size-4.5"
                                        />
                                    </div>
                                    <p className="mt-3 text-body-compact font-medium text-foreground">
                                        暂未登记额外成本
                                    </p>
                                    <p className="mt-1 max-w-md text-xs text-muted-foreground">
                                        如有未计入商品单价的整单费用，可点选下方常用费用快速录入，或添加自定义费用。
                                    </p>
                                    <div className="mt-5 flex flex-wrap items-center justify-center gap-2">
                                        {COMMON_FEE_PRESETS.map((preset) => (
                                            <Button
                                                key={preset.key}
                                                id={`procurement-inquiry-add-fee-preset-${preset.key}`}
                                                type="button"
                                                size="sm"
                                                variant="outline"
                                                className="font-normal"
                                                onClick={() =>
                                                    addFee(preset.label)
                                                }
                                            >
                                                <PlusIcon
                                                    aria-hidden="true"
                                                    data-icon="inline-start"
                                                    className="size-3.5 text-muted-foreground"
                                                />
                                                {preset.label}
                                            </Button>
                                        ))}
                                    </div>
                                    <Button
                                        id="procurement-inquiry-add-fee"
                                        type="button"
                                        size="sm"
                                        variant="ghost"
                                        className={cn(
                                            detailWorkspaceStyles.inlineLinkButton,
                                            "mt-3",
                                        )}
                                        onClick={() => addFee()}
                                    >
                                        <PlusIcon
                                            aria-hidden="true"
                                            data-icon="inline-start"
                                        />
                                        添加自定义费用
                                    </Button>
                                </div>
                            ) : (
                                <form.Field
                                    name="fees"
                                    mode="array"
                                    children={(feesField) => (
                                        <div className="overflow-hidden rounded-lg border border-border/75 bg-card">
                                            <Table className="min-w-136 table-fixed [--table-cell-inline:0.75rem]">
                                                <colgroup>
                                                    <col className="w-auto" />
                                                    <col className="w-40" />
                                                    <col className="w-28" />
                                                    <col className="w-20" />
                                                </colgroup>
                                                <TableHeader>
                                                    <TableRow className="hover:bg-transparent">
                                                        <TableHead>
                                                            费用名称
                                                            <RequiredMark />
                                                        </TableHead>
                                                        <TableHead
                                                            data-align="end"
                                                            className="text-right"
                                                        >
                                                            含税金额（元）
                                                            <RequiredMark />
                                                        </TableHead>
                                                        <TableHead
                                                            data-align="end"
                                                            className="text-right"
                                                        >
                                                            税率
                                                            <RequiredMark />
                                                        </TableHead>
                                                        <TableHead
                                                            data-align="end"
                                                            className="text-right"
                                                        >
                                                            操作
                                                        </TableHead>
                                                    </TableRow>
                                                </TableHeader>
                                                <TableBody>
                                                    {feesField.state.value.map(
                                                        (fee, index) => {
                                                            const seg =
                                                                toAutomationIdSegment(
                                                                    fee.fee_id,
                                                                )
                                                            const prefix = `procurement-inquiry-fee-${seg}`
                                                            const hasNote =
                                                                fee.public_note.trim()
                                                                    .length > 0
                                                            const noteExpanded =
                                                                noteOverrides[
                                                                    fee.fee_id
                                                                ] ?? hasNote
                                                            const showSubRow =
                                                                noteExpanded ||
                                                                hasNote
                                                            const nextFee =
                                                                feesField.state
                                                                    .value[
                                                                    index + 1
                                                                ]
                                                            const rowHasError =
                                                                (
                                                                    [
                                                                        "name",
                                                                        "amount_gross",
                                                                        "tax_rate",
                                                                    ] as const
                                                                ).some(
                                                                    (key) => {
                                                                        const meta =
                                                                            fieldMeta[
                                                                                `fees[${index}].${key}`
                                                                            ]
                                                                        return Boolean(
                                                                            meta?.isTouched &&
                                                                            meta
                                                                                .errors
                                                                                .length >
                                                                                0,
                                                                        )
                                                                    },
                                                                )

                                                            return (
                                                                <React.Fragment
                                                                    key={
                                                                        fee.fee_id
                                                                    }
                                                                >
                                                                    <TableRow
                                                                        className={cn(
                                                                            "has-aria-expanded:bg-transparent [&>td]:align-top",
                                                                            showSubRow
                                                                                ? "border-b-0 [&>td]:pt-2.5 [&>td]:pb-1.5"
                                                                                : "[&>td]:py-2.5",
                                                                            rowHasError &&
                                                                                "bg-warning-soft/20 hover:bg-warning-soft/30 has-aria-expanded:bg-warning-soft/20",
                                                                        )}
                                                                    >
                                                                        <TableCell className="whitespace-normal">
                                                                            <form.Field
                                                                                name={`fees[${index}].name`}
                                                                                children={(
                                                                                    field,
                                                                                ) => {
                                                                                    const errors =
                                                                                        toFieldErrors(
                                                                                            field
                                                                                                .state
                                                                                                .meta
                                                                                                .errors,
                                                                                        )
                                                                                    const invalid =
                                                                                        field
                                                                                            .state
                                                                                            .meta
                                                                                            .isTouched &&
                                                                                        errors.length >
                                                                                            0
                                                                                    const errorId = `${prefix}-name-error`
                                                                                    return (
                                                                                        <div className="space-y-1">
                                                                                            <Input
                                                                                                id={`${prefix}-name`}
                                                                                                size="sm"
                                                                                                aria-label={`第 ${index + 1} 项费用名称`}
                                                                                                aria-invalid={
                                                                                                    invalid ||
                                                                                                    undefined
                                                                                                }
                                                                                                aria-describedby={
                                                                                                    invalid
                                                                                                        ? errorId
                                                                                                        : undefined
                                                                                                }
                                                                                                placeholder="例如：运费、包装费、打样费"
                                                                                                value={
                                                                                                    field
                                                                                                        .state
                                                                                                        .value
                                                                                                }
                                                                                                onBlur={
                                                                                                    field.handleBlur
                                                                                                }
                                                                                                onChange={(
                                                                                                    event,
                                                                                                ) =>
                                                                                                    field.handleChange(
                                                                                                        event
                                                                                                            .target
                                                                                                            .value,
                                                                                                    )
                                                                                                }
                                                                                                onKeyDown={(
                                                                                                    event,
                                                                                                ) => {
                                                                                                    if (
                                                                                                        event.key ===
                                                                                                            "Enter" &&
                                                                                                        !event
                                                                                                            .nativeEvent
                                                                                                            .isComposing
                                                                                                    ) {
                                                                                                        event.preventDefault()
                                                                                                        document
                                                                                                            .getElementById(
                                                                                                                `${prefix}-gross`,
                                                                                                            )
                                                                                                            ?.focus()
                                                                                                    }
                                                                                                }}
                                                                                            />
                                                                                            {invalid && (
                                                                                                <p
                                                                                                    id={
                                                                                                        errorId
                                                                                                    }
                                                                                                    role="alert"
                                                                                                    className="text-xs text-destructive"
                                                                                                >
                                                                                                    {
                                                                                                        errors[0]
                                                                                                            ?.message
                                                                                                    }
                                                                                                </p>
                                                                                            )}
                                                                                        </div>
                                                                                    )
                                                                                }}
                                                                            />
                                                                        </TableCell>

                                                                        <TableCell
                                                                            data-align="end"
                                                                            className="whitespace-normal text-right"
                                                                        >
                                                                            <form.Field
                                                                                name={`fees[${index}].amount_gross`}
                                                                                children={(
                                                                                    field,
                                                                                ) => {
                                                                                    const errors =
                                                                                        toFieldErrors(
                                                                                            field
                                                                                                .state
                                                                                                .meta
                                                                                                .errors,
                                                                                        )
                                                                                    const invalid =
                                                                                        field
                                                                                            .state
                                                                                            .meta
                                                                                            .isTouched &&
                                                                                        errors.length >
                                                                                            0
                                                                                    const moneyInfo =
                                                                                        describeMoney(
                                                                                            field
                                                                                                .state
                                                                                                .value,
                                                                                        )
                                                                                    const showReadout =
                                                                                        !invalid &&
                                                                                        moneyInfo !=
                                                                                            null &&
                                                                                        moneyInfo.magnitude !==
                                                                                            "base"
                                                                                    const errorId = `${prefix}-gross-error`
                                                                                    const hintId = `${prefix}-gross-money-hint`
                                                                                    const describedBy =
                                                                                        [
                                                                                            invalid
                                                                                                ? errorId
                                                                                                : undefined,
                                                                                            showReadout
                                                                                                ? hintId
                                                                                                : undefined,
                                                                                        ]
                                                                                            .filter(
                                                                                                Boolean,
                                                                                            )
                                                                                            .join(
                                                                                                " ",
                                                                                            )
                                                                                    return (
                                                                                        <div className="space-y-1">
                                                                                            <div className="relative min-w-0">
                                                                                                <span
                                                                                                    aria-hidden="true"
                                                                                                    className="pointer-events-none absolute inset-y-0 left-2.5 flex items-center text-xs text-muted-foreground"
                                                                                                >
                                                                                                    ¥
                                                                                                </span>
                                                                                                <Input
                                                                                                    id={`${prefix}-gross`}
                                                                                                    size="sm"
                                                                                                    type="text"
                                                                                                    inputMode="decimal"
                                                                                                    className="num pl-6 text-right"
                                                                                                    aria-label={`第 ${index + 1} 项含税金额（元）`}
                                                                                                    aria-invalid={
                                                                                                        invalid ||
                                                                                                        undefined
                                                                                                    }
                                                                                                    aria-describedby={
                                                                                                        describedBy ||
                                                                                                        undefined
                                                                                                    }
                                                                                                    placeholder="0.00"
                                                                                                    value={
                                                                                                        field
                                                                                                            .state
                                                                                                            .value
                                                                                                    }
                                                                                                    onBlur={
                                                                                                        field.handleBlur
                                                                                                    }
                                                                                                    onChange={(
                                                                                                        event,
                                                                                                    ) =>
                                                                                                        field.handleChange(
                                                                                                            event
                                                                                                                .target
                                                                                                                .value,
                                                                                                        )
                                                                                                    }
                                                                                                    onKeyDown={(
                                                                                                        event,
                                                                                                    ) => {
                                                                                                        if (
                                                                                                            event.key ===
                                                                                                                "Enter" &&
                                                                                                            !event
                                                                                                                .nativeEvent
                                                                                                                .isComposing
                                                                                                        ) {
                                                                                                            event.preventDefault()
                                                                                                            document
                                                                                                                .getElementById(
                                                                                                                    `${prefix}-tax`,
                                                                                                                )
                                                                                                                ?.focus()
                                                                                                        }
                                                                                                    }}
                                                                                                />
                                                                                            </div>
                                                                                            {invalid ? (
                                                                                                <p
                                                                                                    id={
                                                                                                        errorId
                                                                                                    }
                                                                                                    role="alert"
                                                                                                    className="text-right text-xs text-destructive"
                                                                                                >
                                                                                                    {
                                                                                                        errors[0]
                                                                                                            ?.message
                                                                                                    }
                                                                                                </p>
                                                                                            ) : showReadout ? (
                                                                                                <p
                                                                                                    id={
                                                                                                        hintId
                                                                                                    }
                                                                                                    className="text-right text-xs text-muted-foreground"
                                                                                                >
                                                                                                    {
                                                                                                        moneyInfo.readout
                                                                                                    }
                                                                                                </p>
                                                                                            ) : null}
                                                                                        </div>
                                                                                    )
                                                                                }}
                                                                            />
                                                                        </TableCell>

                                                                        <TableCell
                                                                            data-align="end"
                                                                            className="whitespace-normal text-right"
                                                                        >
                                                                            <form.Field
                                                                                name={`fees[${index}].tax_rate`}
                                                                                children={(
                                                                                    field,
                                                                                ) => {
                                                                                    const errors =
                                                                                        toFieldErrors(
                                                                                            field
                                                                                                .state
                                                                                                .meta
                                                                                                .errors,
                                                                                        )
                                                                                    const invalid =
                                                                                        field
                                                                                            .state
                                                                                            .meta
                                                                                            .isTouched &&
                                                                                        errors.length >
                                                                                            0
                                                                                    const errorId = `${prefix}-tax-error`
                                                                                    return (
                                                                                        <div className="space-y-1">
                                                                                            <InquiryTaxRateInput
                                                                                                id={`${prefix}-tax`}
                                                                                                className="h-control-sm num text-right"
                                                                                                aria-label={`第 ${index + 1} 项税率（%）`}
                                                                                                aria-invalid={
                                                                                                    invalid ||
                                                                                                    undefined
                                                                                                }
                                                                                                aria-describedby={
                                                                                                    invalid
                                                                                                        ? errorId
                                                                                                        : undefined
                                                                                                }
                                                                                                placeholder="0"
                                                                                                title="无税额填 0，例如 6 表示 6%"
                                                                                                value={
                                                                                                    field
                                                                                                        .state
                                                                                                        .value
                                                                                                }
                                                                                                onFocus={(
                                                                                                    event,
                                                                                                ) =>
                                                                                                    event.currentTarget.select()
                                                                                                }
                                                                                                onBlur={
                                                                                                    field.handleBlur
                                                                                                }
                                                                                                onValueChange={(
                                                                                                    rate,
                                                                                                ) =>
                                                                                                    field.handleChange(
                                                                                                        rate,
                                                                                                    )
                                                                                                }
                                                                                                onKeyDown={(
                                                                                                    event,
                                                                                                ) => {
                                                                                                    if (
                                                                                                        event.key ===
                                                                                                            "Enter" &&
                                                                                                        !event
                                                                                                            .nativeEvent
                                                                                                            .isComposing
                                                                                                    ) {
                                                                                                        event.preventDefault()
                                                                                                        if (
                                                                                                            nextFee
                                                                                                        ) {
                                                                                                            document
                                                                                                                .getElementById(
                                                                                                                    `procurement-inquiry-fee-${toAutomationIdSegment(nextFee.fee_id)}-name`,
                                                                                                                )
                                                                                                                ?.focus()
                                                                                                        }
                                                                                                    }
                                                                                                }}
                                                                                            />
                                                                                            {invalid && (
                                                                                                <p
                                                                                                    id={
                                                                                                        errorId
                                                                                                    }
                                                                                                    role="alert"
                                                                                                    className="text-right text-xs text-destructive"
                                                                                                >
                                                                                                    {
                                                                                                        errors[0]
                                                                                                            ?.message
                                                                                                    }
                                                                                                </p>
                                                                                            )}
                                                                                        </div>
                                                                                    )
                                                                                }}
                                                                            />
                                                                        </TableCell>

                                                                        <TableCell
                                                                            data-align="end"
                                                                            className="text-right"
                                                                        >
                                                                            <div className="flex items-center justify-end gap-1">
                                                                                <Button
                                                                                    id={`${prefix}-note-toggle`}
                                                                                    type="button"
                                                                                    size="icon-sm"
                                                                                    variant="ghost"
                                                                                    aria-expanded={
                                                                                        noteExpanded
                                                                                    }
                                                                                    aria-label={
                                                                                        noteExpanded
                                                                                            ? `收起第 ${index + 1} 项备注`
                                                                                            : hasNote
                                                                                              ? `展开第 ${index + 1} 项备注`
                                                                                              : `为第 ${index + 1} 项添加备注`
                                                                                    }
                                                                                    title={
                                                                                        noteExpanded
                                                                                            ? "收起备注"
                                                                                            : hasNote
                                                                                              ? "编辑备注（销售可见）"
                                                                                              : "添加备注（销售可见）"
                                                                                    }
                                                                                    className={cn(
                                                                                        hasNote
                                                                                            ? "bg-sidebar-accent/55 text-primary hover:bg-sidebar-accent hover:text-primary"
                                                                                            : noteExpanded
                                                                                              ? "bg-muted/70 text-foreground"
                                                                                              : "text-muted-foreground hover:text-foreground",
                                                                                    )}
                                                                                    onClick={() =>
                                                                                        toggleNote(
                                                                                            fee.fee_id,
                                                                                            noteExpanded,
                                                                                        )
                                                                                    }
                                                                                >
                                                                                    {hasNote ||
                                                                                    noteExpanded ? (
                                                                                        <MessageSquareTextIcon aria-hidden="true" />
                                                                                    ) : (
                                                                                        <MessageSquarePlusIcon aria-hidden="true" />
                                                                                    )}
                                                                                </Button>
                                                                                <Button
                                                                                    id={`${prefix}-remove`}
                                                                                    type="button"
                                                                                    size="icon-sm"
                                                                                    variant="ghost"
                                                                                    aria-label={`移除第 ${index + 1} 项费用`}
                                                                                    title="移除此项费用"
                                                                                    className={
                                                                                        detailWorkspaceStyles.quietDangerButton
                                                                                    }
                                                                                    onClick={() => {
                                                                                        const remaining =
                                                                                            feesField.state.value.filter(
                                                                                                (
                                                                                                    _,
                                                                                                    itemIndex,
                                                                                                ) =>
                                                                                                    itemIndex !==
                                                                                                    index,
                                                                                            )
                                                                                        feesField.removeValue(
                                                                                            index,
                                                                                        )
                                                                                        setNoteOverrides(
                                                                                            (
                                                                                                current,
                                                                                            ) => {
                                                                                                if (
                                                                                                    !(
                                                                                                        fee.fee_id in
                                                                                                        current
                                                                                                    )
                                                                                                )
                                                                                                    return current
                                                                                                const next =
                                                                                                    {
                                                                                                        ...current,
                                                                                                    }
                                                                                                delete next[
                                                                                                    fee
                                                                                                        .fee_id
                                                                                                ]
                                                                                                return next
                                                                                            },
                                                                                        )
                                                                                        if (
                                                                                            feedbackFeeIssues(
                                                                                                remaining,
                                                                                            )
                                                                                                .length ===
                                                                                            0
                                                                                        ) {
                                                                                            onClearFeeError()
                                                                                        }
                                                                                        if (
                                                                                            remaining.length ===
                                                                                            0
                                                                                        ) {
                                                                                            focusElementById(
                                                                                                "procurement-feedback-fees-dialog",
                                                                                            )
                                                                                        }
                                                                                    }}
                                                                                >
                                                                                    <Trash2Icon aria-hidden="true" />
                                                                                </Button>
                                                                            </div>
                                                                        </TableCell>
                                                                    </TableRow>

                                                                    {showSubRow && (
                                                                        <TableRow
                                                                            className={cn(
                                                                                "hover:bg-transparent",
                                                                                rowHasError
                                                                                    ? "bg-warning-soft/20"
                                                                                    : "bg-surface-sunken/35",
                                                                            )}
                                                                        >
                                                                            <TableCell
                                                                                colSpan={
                                                                                    4
                                                                                }
                                                                                className="whitespace-normal pt-1 pb-2.5"
                                                                            >
                                                                                {noteExpanded ? (
                                                                                    <form.Field
                                                                                        name={`fees[${index}].public_note`}
                                                                                        children={(
                                                                                            noteField,
                                                                                        ) => (
                                                                                            <div className="flex items-start gap-2">
                                                                                                <Label
                                                                                                    htmlFor={`${prefix}-note`}
                                                                                                    className="shrink-0 pt-1.5 text-xs font-normal text-muted-foreground"
                                                                                                >
                                                                                                    备注
                                                                                                </Label>
                                                                                                <Textarea
                                                                                                    id={`${prefix}-note`}
                                                                                                    rows={
                                                                                                        1
                                                                                                    }
                                                                                                    className="min-h-control-sm flex-1 bg-card py-1.5"
                                                                                                    placeholder="补充费用说明（销售可见，选填）"
                                                                                                    value={
                                                                                                        noteField
                                                                                                            .state
                                                                                                            .value
                                                                                                    }
                                                                                                    onBlur={
                                                                                                        noteField.handleBlur
                                                                                                    }
                                                                                                    onChange={(
                                                                                                        event,
                                                                                                    ) =>
                                                                                                        noteField.handleChange(
                                                                                                            event
                                                                                                                .target
                                                                                                                .value,
                                                                                                        )
                                                                                                    }
                                                                                                />
                                                                                                <Button
                                                                                                    id={`${prefix}-note-collapse`}
                                                                                                    type="button"
                                                                                                    size="xs"
                                                                                                    variant="ghost"
                                                                                                    className="h-control-sm shrink-0 text-xs text-muted-foreground hover:text-foreground"
                                                                                                    onClick={() =>
                                                                                                        setNoteOverrides(
                                                                                                            (
                                                                                                                current,
                                                                                                            ) => ({
                                                                                                                ...current,
                                                                                                                [fee.fee_id]: false,
                                                                                                            }),
                                                                                                        )
                                                                                                    }
                                                                                                >
                                                                                                    <ChevronUpIcon
                                                                                                        aria-hidden="true"
                                                                                                        data-icon="inline-start"
                                                                                                    />
                                                                                                    收起
                                                                                                </Button>
                                                                                            </div>
                                                                                        )}
                                                                                    />
                                                                                ) : (
                                                                                    <button
                                                                                        id={`${prefix}-note-preview`}
                                                                                        type="button"
                                                                                        className="flex max-w-full cursor-pointer items-center gap-1.5 rounded-sm text-left text-xs text-muted-foreground transition-colors hover:text-foreground"
                                                                                        onClick={() =>
                                                                                            toggleNote(
                                                                                                fee.fee_id,
                                                                                                false,
                                                                                            )
                                                                                        }
                                                                                    >
                                                                                        <span className="shrink-0">
                                                                                            备注：
                                                                                        </span>
                                                                                        <span className="truncate">
                                                                                            {fee.public_note.trim()}
                                                                                        </span>
                                                                                    </button>
                                                                                )}
                                                                            </TableCell>
                                                                        </TableRow>
                                                                    )}
                                                                </React.Fragment>
                                                            )
                                                        },
                                                    )}
                                                </TableBody>
                                            </Table>

                                            <div className="flex flex-wrap items-center justify-between gap-2 border-t border-border/70 bg-surface-sunken/40 px-3 py-2">
                                                <Button
                                                    id="procurement-inquiry-add-fee"
                                                    type="button"
                                                    size="xs"
                                                    variant="ghost"
                                                    className={cn(
                                                        detailWorkspaceStyles.inlineLinkButton,
                                                        "h-control-sm px-2",
                                                    )}
                                                    onClick={() => addFee()}
                                                >
                                                    <PlusIcon
                                                        aria-hidden="true"
                                                        data-icon="inline-start"
                                                    />
                                                    添加费用
                                                </Button>

                                                {remainingPresets.length >
                                                    0 && (
                                                    <div className="flex flex-wrap items-center gap-1.5">
                                                        <span className="text-xs text-muted-foreground">
                                                            快速添加
                                                        </span>
                                                        {remainingPresets.map(
                                                            (preset) => (
                                                                <button
                                                                    key={
                                                                        preset.key
                                                                    }
                                                                    id={`procurement-inquiry-add-fee-preset-${preset.key}`}
                                                                    type="button"
                                                                    className="inline-flex h-6 cursor-pointer items-center gap-0.5 rounded-md border border-border/75 bg-card px-2 text-xs text-foreground/80 transition-colors hover:border-primary/45 hover:bg-sidebar-accent/50 hover:text-primary disabled:pointer-events-none disabled:opacity-50"
                                                                    onClick={() =>
                                                                        addFee(
                                                                            preset.label,
                                                                        )
                                                                    }
                                                                >
                                                                    <PlusIcon
                                                                        aria-hidden="true"
                                                                        className="size-3 text-muted-foreground"
                                                                    />
                                                                    {
                                                                        preset.label
                                                                    }
                                                                </button>
                                                            ),
                                                        )}
                                                    </div>
                                                )}
                                            </div>
                                        </div>
                                    )}
                                />
                            )}
                        </fieldset>
                    </DialogBody>

                    <DialogFooter className="flex-wrap items-center gap-3">
                        <p className="mr-auto text-xs text-muted-foreground">
                            关闭保留当前输入，保存将连同商品反馈一并提交。
                        </p>
                        <Button
                            id="procurement-feedback-fees-dismiss"
                            type="button"
                            variant="outline"
                            onClick={() => handleOpenChange(false)}
                        >
                            关闭
                        </Button>
                        <Button
                            id="procurement-feedback-fees-save"
                            type="button"
                            disabled={disabled}
                            onClick={onSave}
                        >
                            {saving ? "正在保存…" : "保存处理进度"}
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        )
    },
})
