"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { InquiryFeedbackTable } from "./inquiry-feedback-table"
import { feedbackIssues } from "../lib/feedback-editing"
import { InquiryFeedbackResults } from "./inquiry-feedback-results"
import { MoneyValue } from "@/components/business"
import { useAppForm } from "@/components/form"
import { OptionCombobox } from "@/components/business/option-combobox"
import { MultiOptionCombobox } from "@/components/business/multi-option-combobox"
import { Button } from "@/components/ui/button"
import { Label } from "@/components/ui/label"
import { Checkbox } from "@/components/ui/checkbox"
import { DatePicker } from "@/components/ui/date-picker"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { inquiryPath } from "../api"
import { useInquiryCommand } from "../hooks/use-inquiry-command"
import { inquiryDateInput, inquiryDateSeconds } from "../lib/format"
import {
    inquiryStorageKey,
    readInquiryLocalValue,
    writeInquiryLocalValue,
} from "../lib/local-draft"
import {
    INQUIRY_CONCLUSION_LABEL,
    type InquiryFeedbackData,
    type InquiryFee,
    type ProcurementInquiry,
} from "../types"

export type InquiryTask = {
    id: string
    version: number
    owner_user_id?: string
    responsibility_scope_ids?: string[]
}
const FEE_CATEGORIES = {
    logistics: "物流",
    printing: "印刷",
    warehousing: "仓储",
    delivery: "配送",
    platform_tech: "平台技术",
    offline_service: "线下服务",
    other: "其他",
}

function newFee(lineIds: string[]): InquiryFee {
    return {
        fee_id: crypto.randomUUID(),
        category: "other",
        name: "",
        amount_gross: "",
        tax_rate: "0",
        line_ids: lineIds,
        allocation_ids: [],
        included_in_goods_price: false,
        confirmation: "estimated",
        requires_erp_payment: true,
        valid_until: 0,
        valid_conditions: "",
        public_note: "",
    }
}

type FeedbackBaseline = { scope: string; work: string }
function feedbackBaseline(
    inquiry: ProcurementInquiry,
    task: InquiryTask,
    userId: string,
): FeedbackBaseline {
    return {
        scope: JSON.stringify({
            requirement: inquiry.requirement_version,
            lines: inquiry.draft.lines.map((line) => line.line_id).sort(),
            task: task.id,
            owner: task.owner_user_id,
            feeOwner: inquiry.shared_fee_owner_user_id,
        }),
        work: JSON.stringify(
            inquiry.feedback_work.find(
                (work) => !work.frozen && work.actor_id === userId,
            )?.data ?? null,
        ),
    }
}

export function InquiryFeedbackEditor({
    inquiry,
    task,
    userId,
    onSaved,
    onDirtyChange,
    footerActions,
    onInternalQuote,
}: {
    inquiry: ProcurementInquiry
    task: InquiryTask
    userId: string
    onSaved: () => void
    onDirtyChange: (dirty: boolean) => void
    footerActions?: React.ReactNode
    onInternalQuote?: (lineId: string) => void
}) {
    const key = inquiryStorageKey(userId, `${inquiry.id}:${task.id}:feedback`)
    const [initial, setInitial] = React.useState<{
        data: InquiryFeedbackData
        version: number
        task_version: number
        baseline?: FeedbackBaseline
    } | null>(null)
    React.useEffect(() => {
        const existing =
            inquiry.feedback_work.find(
                (work) => !work.frozen && work.actor_id === userId,
            )?.data ?? inquiry.feedbacks.at(-1)?.data
        setInitial(
            readInquiryLocalValue(key) ?? {
                version: inquiry.version,
                baseline: feedbackBaseline(inquiry, task, userId),
                task_version: task.version,
                data: {
                    lines: inquiry.draft.lines.map(
                        (line) =>
                            existing?.lines.find(
                                (feedback) => feedback.line_id === line.line_id,
                            ) ?? {
                                line_id: line.line_id,
                                conclusion: "pending",
                                available_quantity: "0",
                                prices: [],
                                public_note: "",
                            },
                    ),
                    fees: existing?.fees ?? [],
                },
            },
        )
        // 当前任务的后台刷新不覆盖报价输入。
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [key])
    return initial ? (
        <FeedbackEditorContent
            key={key}
            storageKey={key}
            initial={initial}
            inquiry={inquiry}
            task={task}
            userId={userId}
            onSaved={onSaved}
            onDirtyChange={onDirtyChange}
            footerActions={footerActions}
            onInternalQuote={onInternalQuote}
        />
    ) : (
        <p role="status">正在恢复采购输入…</p>
    )
}

function FeedbackEditorContent({
    storageKey,
    initial,
    inquiry,
    task,
    userId,
    onSaved,
    onDirtyChange,
    footerActions,
    onInternalQuote,
}: {
    storageKey: string
    initial: {
        data: InquiryFeedbackData
        version: number
        task_version: number
        baseline?: FeedbackBaseline
    }
    inquiry: ProcurementInquiry
    task: InquiryTask
    userId: string
    onSaved: () => void
    onDirtyChange: (dirty: boolean) => void
    footerActions?: React.ReactNode
    onInternalQuote?: (lineId: string) => void
}) {
    const command = useInquiryCommand(userId, inquiry.id)
    const [error, setError] = React.useState<string | null>(null)
    const [saved, setSaved] = React.useState(false)
    const [validationRequest, setValidationRequest] = React.useState(0)
    const [ownCommandRevision, setOwnCommandRevision] = React.useState(0)
    const baseline = React.useRef(
        initial.baseline ??
            (initial.version === inquiry.version
                ? feedbackBaseline(inquiry, task, userId)
                : undefined),
    )
    const [conflict, setConflict] = React.useState(false)
    const version = React.useRef(initial.version)
    const taskVersion = React.useRef(initial.task_version)
    const form = useAppForm({
        defaultValues: initial.data,
        listeners: {
            onChange: ({ formApi }) => {
                setSaved(false)
                onDirtyChange(true)
                try {
                    writeInquiryLocalValue(storageKey, {
                        data: formApi.state.values,
                        version: version.current,
                        baseline: baseline.current,
                        task_version: taskVersion.current,
                    })
                } catch {
                    setError(
                        "浏览器无法保留本地输入，请先保存处理进度后再离开。",
                    )
                }
            },
        },
        onSubmit: async ({ value }) => {
            setError(null)
            if (conflict) return
            try {
                const result = await command.run<{ version: number }>(
                    `${inquiryPath(true, inquiry.id)}/feedback`,
                    {
                        expected_version: version.current,
                        task_id: task.id,
                        task_version: taskVersion.current,
                        data: value,
                    },
                    "PUT",
                )
                version.current = result.version
                localStorage.removeItem(storageKey)
                setSaved(true)
                onDirtyChange(false)
                onSaved()
            } catch (failure) {
                setError(
                    getErrorMessage(
                        failure,
                        "保存采购反馈失败，请保留输入并重试。",
                    ),
                )
            }
        },
    })
    React.useEffect(() => {
        const current = feedbackBaseline(inquiry, task, userId)
        const same =
            baseline.current?.scope === current.scope &&
            baseline.current?.work === current.work
        const resolved = command.resolution
        if (resolved?.path === `${inquiryPath(true, inquiry.id)}/feedback`) {
            const result = resolved.result as { version?: number }
            if (
                result.version === inquiry.version &&
                resolved.body.task_id === task.id
            ) {
                baseline.current = current
                version.current = inquiry.version
            }
        }
        if (
            same ||
            (version.current === inquiry.version &&
                baseline.current?.work === current.work)
        ) {
            version.current = inquiry.version
            taskVersion.current = task.version
            baseline.current = current
            setConflict(false)
            if (readInquiryLocalValue(storageKey)) {
                try {
                    writeInquiryLocalValue(storageKey, {
                        data: form.state.values,
                        version: inquiry.version,
                        task_version: task.version,
                        baseline: current,
                    })
                } catch {
                    setError("浏览器无法保留当前采购输入，请先保存处理进度。")
                }
            }
        } else if (!command.isPending && !command.unresolved) {
            setConflict(true)
        }
    }, [
        inquiry,
        task,
        userId,
        command.resolution,
        command.isPending,
        command.unresolved,
        form,
        storageKey,
        ownCommandRevision,
    ])
    React.useEffect(() => {
        onDirtyChange(Boolean(readInquiryLocalValue(storageKey)))
        return () => onDirtyChange(false)
    }, [storageKey, onDirtyChange])
    React.useEffect(() => {
        const resolved = command.resolution
        if (resolved?.path === `${inquiryPath(true, inquiry.id)}/feedback`) {
            // 恢复原命令后，只有当前输入与实际提交内容一致才算已保存。
            const unchanged =
                JSON.stringify(form.state.values) ===
                JSON.stringify(resolved.body.data)
            onDirtyChange(!unchanged)
            setSaved(unchanged)
            setOwnCommandRevision((value) => value + 1)
        }
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [command.resolution])
    const values = useStore(form.store, (state) => state.values)
    const completeCount = values.lines.filter((line) => {
        const demand = inquiry.draft.lines.find(
            (item) => item.line_id === line.line_id,
        )
        return demand && feedbackIssues(line, demand).length === 0
    }).length
    const disabled =
        conflict || command.isPending || !command.ready || command.unresolved
    const currentWork = inquiry.feedback_work.find(
        (work) => !work.frozen && work.actor_id === userId,
    )?.data
    return (
        <form
            id="procurement-inquiry-feedback-form"
            className="space-y-4"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <fieldset disabled={command.isPending} className="contents">
                <p className="text-sm text-muted-foreground">
                    直接填写商品报价，展开明细维护有效期与配送数量。保存进度后，发布给销售。
                </p>
                {conflict && (
                    <Alert variant="warning">
                        <AlertDescription className="space-y-3">
                            <p>
                                已保存的采购反馈发生变化，当前输入已保留。请核对以下最新内容，再决定是否用当前输入继续编辑。
                            </p>
                            {currentWork?.lines.map((line) => (
                                <div key={line.line_id}>
                                    {
                                        inquiry.draft.lines.find(
                                            (item) =>
                                                item.line_id === line.line_id,
                                        )?.product_name
                                    }
                                    ：
                                    {INQUIRY_CONCLUSION_LABEL[line.conclusion]}
                                    ，可供 {line.available_quantity}；
                                    {line.prices.map((price) => (
                                        <span key={price.price_id}>
                                            供给价{" "}
                                            <MoneyValue
                                                value={price.unit_price_gross}
                                            />
                                            ，有效至{" "}
                                            {inquiryDateInput(
                                                price.valid_until,
                                            )}
                                            ；
                                        </span>
                                    ))}
                                </div>
                            ))}
                            {currentWork?.fees.map((fee) => (
                                <div key={fee.fee_id}>
                                    {fee.name}：
                                    <MoneyValue value={fee.amount_gross} />
                                </div>
                            ))}
                            {!currentWork && <p>当前尚无已保存反馈。</p>}
                            {currentWork && (
                                <details>
                                    <summary
                                        id="procurement-inquiry-feedback-conflict-details"
                                        className="cursor-pointer"
                                    >
                                        查看完整已保存内容
                                    </summary>
                                    <InquiryFeedbackResults
                                        idPrefix="procurement-inquiry-feedback-conflict"
                                        inquiry={{
                                            ...inquiry,
                                            feedback_round:
                                                inquiry.feedback_round + 1,
                                            feedbacks: [
                                                {
                                                    round:
                                                        inquiry.feedback_round +
                                                        1,
                                                    requirement_version:
                                                        inquiry.requirement_version,
                                                    published_at:
                                                        inquiry.feedback_work.find(
                                                            (work) =>
                                                                !work.frozen &&
                                                                work.actor_id ===
                                                                    userId,
                                                        )?.saved_at ?? 0,
                                                    published_by: userId,
                                                    data: currentWork,
                                                },
                                            ],
                                        }}
                                    />
                                </details>
                            )}
                            <Button
                                id="procurement-inquiry-feedback-rebase"
                                type="button"
                                variant="outline"
                                disabled={
                                    command.isPending ||
                                    command.unresolved ||
                                    (baseline.current?.scope != null &&
                                        baseline.current.scope !==
                                            feedbackBaseline(
                                                inquiry,
                                                task,
                                                userId,
                                            ).scope)
                                }
                                onClick={() => {
                                    const current = feedbackBaseline(
                                        inquiry,
                                        task,
                                        userId,
                                    )
                                    try {
                                        writeInquiryLocalValue(storageKey, {
                                            data: form.state.values,
                                            version: inquiry.version,
                                            task_version: task.version,
                                            baseline: current,
                                        })
                                        baseline.current = current
                                        version.current = inquiry.version
                                        taskVersion.current = task.version
                                        setConflict(false)
                                        setError(null)
                                        onDirtyChange(true)
                                    } catch {
                                        setError(
                                            "浏览器无法保留输入，请先恢复浏览器存储后重试。",
                                        )
                                    }
                                }}
                            >
                                已核对，保留当前输入继续编辑
                            </Button>
                        </AlertDescription>
                    </Alert>
                )}
                {error && (
                    <Alert variant="destructive">
                        <AlertDescription>{error}</AlertDescription>
                    </Alert>
                )}
                {command.unresolved && (
                    <Alert variant="warning">
                        <AlertDescription>
                            上次提交结果待核对。
                            <Button
                                id="procurement-inquiry-feedback-recover"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={() =>
                                    void command
                                        .recover()
                                        .then(() => onSaved())
                                        .catch((failure) =>
                                            setError(getErrorMessage(failure)),
                                        )
                                }
                            >
                                核对上次提交结果
                            </Button>
                        </AlertDescription>
                    </Alert>
                )}
                <form.Field
                    name="lines"
                    mode="array"
                    children={(field) => (
                        <InquiryFeedbackTable
                            lines={field.state.value}
                            demand={inquiry.draft.lines}
                            onChange={field.handleChange}
                            validationRequest={validationRequest}
                            onInternalQuote={onInternalQuote}
                            disabled={disabled}
                        />
                    )}
                />
                <details className="space-y-4 rounded-lg border p-4">
                    <summary
                        id="procurement-feedback-fees-toggle"
                        className="cursor-pointer text-sm font-medium"
                    >
                        额外成本（{values.fees.length} 项）
                    </summary>
                    <div className="flex items-center justify-between">
                        <h3 className="text-base font-medium">额外成本</h3>
                        <Button
                            id="procurement-inquiry-add-fee"
                            type="button"
                            variant="outline"
                            size="sm"
                            onClick={() =>
                                form.pushFieldValue(
                                    "fees",
                                    newFee(
                                        inquiry.draft.lines.map(
                                            (line) => line.line_id,
                                        ),
                                    ),
                                )
                            }
                        >
                            添加费用
                        </Button>
                    </div>
                    <form.Field
                        name="fees"
                        mode="array"
                        children={(feesField) => (
                            <div className="space-y-3">
                                {feesField.state.value.map((fee, index) => {
                                    const prefix = `procurement-inquiry-fee-${toAutomationIdSegment(fee.fee_id)}`
                                    return (
                                        <div
                                            key={fee.fee_id}
                                            className="space-y-3 rounded-lg border p-4"
                                        >
                                            <div className="grid gap-3 md:grid-cols-2">
                                                <form.Field
                                                    name={`fees[${index}].line_ids`}
                                                    children={(field) => (
                                                        <div className="space-y-2">
                                                            <Label
                                                                htmlFor={`${prefix}-lines`}
                                                            >
                                                                关联商品
                                                                {inquiry.shared_fee_owner_user_id ===
                                                                userId
                                                                    ? "（清空表示整单费用）"
                                                                    : ""}
                                                            </Label>
                                                            <MultiOptionCombobox
                                                                id={`${prefix}-lines`}
                                                                aria-label="费用关联商品"
                                                                value={
                                                                    field.state
                                                                        .value
                                                                }
                                                                options={inquiry.draft.lines.map(
                                                                    (line) => ({
                                                                        value: line.line_id,
                                                                        label: line.product_name,
                                                                    }),
                                                                )}
                                                                onValueChange={(
                                                                    values,
                                                                ) => {
                                                                    if (
                                                                        !values.length &&
                                                                        inquiry.shared_fee_owner_user_id !==
                                                                            userId
                                                                    )
                                                                        return
                                                                    field.handleChange(
                                                                        values,
                                                                    )
                                                                    form.setFieldValue(
                                                                        `fees[${index}].allocation_ids`,
                                                                        [],
                                                                    )
                                                                }}
                                                            />
                                                        </div>
                                                    )}
                                                />
                                                <form.Field
                                                    name={`fees[${index}].allocation_ids`}
                                                    children={(field) => (
                                                        <div className="space-y-2">
                                                            <Label
                                                                htmlFor={`${prefix}-addresses`}
                                                            >
                                                                关联地址（不选表示关联范围全部地址）
                                                            </Label>
                                                            <MultiOptionCombobox
                                                                id={`${prefix}-addresses`}
                                                                aria-label="费用关联地址"
                                                                value={
                                                                    field.state
                                                                        .value
                                                                }
                                                                options={inquiry.draft.lines
                                                                    .filter(
                                                                        (
                                                                            line,
                                                                        ) =>
                                                                            !fee
                                                                                .line_ids
                                                                                .length ||
                                                                            fee.line_ids.includes(
                                                                                line.line_id,
                                                                            ),
                                                                    )
                                                                    .flatMap(
                                                                        (
                                                                            line,
                                                                        ) =>
                                                                            line.allocations.map(
                                                                                (
                                                                                    allocation,
                                                                                ) => ({
                                                                                    value: allocation.allocation_id,
                                                                                    label: `${line.product_name} · ${allocation.recipient} · ${allocation.region_name} ${allocation.address}`,
                                                                                }),
                                                                            ),
                                                                    )}
                                                                onValueChange={
                                                                    field.handleChange
                                                                }
                                                            />
                                                        </div>
                                                    )}
                                                />
                                            </div>
                                            <div className="grid gap-3 md:grid-cols-4">
                                                <form.AppField
                                                    name={`fees[${index}].name`}
                                                    children={(field) => (
                                                        <field.TextField
                                                            id={`${prefix}-name`}
                                                            label="费用名称"
                                                        />
                                                    )}
                                                />
                                                <form.Field
                                                    name={`fees[${index}].category`}
                                                    children={(field) => (
                                                        <div className="space-y-2">
                                                            <Label
                                                                htmlFor={`${prefix}-category`}
                                                            >
                                                                费用类别
                                                            </Label>
                                                            <OptionCombobox
                                                                id={`${prefix}-category`}
                                                                aria-label="费用类别"
                                                                allowClear={
                                                                    false
                                                                }
                                                                value={
                                                                    field.state
                                                                        .value
                                                                }
                                                                onValueChange={(
                                                                    value,
                                                                ) =>
                                                                    field.handleChange(
                                                                        value ??
                                                                            "other",
                                                                    )
                                                                }
                                                                options={Object.entries(
                                                                    FEE_CATEGORIES,
                                                                ).map(
                                                                    ([
                                                                        value,
                                                                        label,
                                                                    ]) => ({
                                                                        value,
                                                                        label,
                                                                    }),
                                                                )}
                                                            />
                                                        </div>
                                                    )}
                                                />
                                                <form.AppField
                                                    name={`fees[${index}].amount_gross`}
                                                    children={(field) => (
                                                        <field.TextField
                                                            id={`${prefix}-gross`}
                                                            label="含税金额"
                                                            inputMode="decimal"
                                                            money
                                                        />
                                                    )}
                                                />
                                                <form.AppField
                                                    name={`fees[${index}].tax_rate`}
                                                    children={(field) => (
                                                        <field.TextField
                                                            id={`${prefix}-tax`}
                                                            label="税率（0.13 表示 13%）"
                                                            inputMode="decimal"
                                                        />
                                                    )}
                                                />
                                                <form.Field
                                                    name={`fees[${index}].confirmation`}
                                                    children={(field) => (
                                                        <div className="space-y-2">
                                                            <Label
                                                                htmlFor={`${prefix}-confirmation`}
                                                            >
                                                                费用确认
                                                            </Label>
                                                            <OptionCombobox
                                                                id={`${prefix}-confirmation`}
                                                                aria-label="费用确认"
                                                                allowClear={
                                                                    false
                                                                }
                                                                options={[
                                                                    {
                                                                        value: "estimated",
                                                                        label: "暂估",
                                                                    },
                                                                    {
                                                                        value: "confirmed",
                                                                        label: "已确认",
                                                                    },
                                                                ]}
                                                                value={
                                                                    field.state
                                                                        .value
                                                                }
                                                                onValueChange={(
                                                                    value,
                                                                ) =>
                                                                    field.handleChange(
                                                                        value as
                                                                            | "estimated"
                                                                            | "confirmed",
                                                                    )
                                                                }
                                                            />
                                                        </div>
                                                    )}
                                                />
                                                <form.Field
                                                    name={`fees[${index}].valid_until`}
                                                    children={(field) => (
                                                        <div className="space-y-2">
                                                            <Label
                                                                htmlFor={`${prefix}-valid`}
                                                            >
                                                                费用有效日期
                                                            </Label>
                                                            <DatePicker
                                                                id={`${prefix}-valid`}
                                                                value={inquiryDateInput(
                                                                    field.state
                                                                        .value,
                                                                )}
                                                                onValueChange={(
                                                                    value,
                                                                ) =>
                                                                    field.handleChange(
                                                                        inquiryDateSeconds(
                                                                            value ??
                                                                                "",
                                                                        ),
                                                                    )
                                                                }
                                                            />
                                                        </div>
                                                    )}
                                                />
                                                <form.Field
                                                    name={`fees[${index}].included_in_goods_price`}
                                                    children={(field) => (
                                                        <label
                                                            htmlFor={`${prefix}-included`}
                                                            className="flex items-center gap-2 text-sm"
                                                        >
                                                            <Checkbox
                                                                id={`${prefix}-included`}
                                                                checked={
                                                                    field.state
                                                                        .value
                                                                }
                                                                onCheckedChange={(
                                                                    value,
                                                                ) =>
                                                                    field.handleChange(
                                                                        value ===
                                                                            true,
                                                                    )
                                                                }
                                                            />
                                                            已含商品供给价
                                                        </label>
                                                    )}
                                                />
                                                <form.Field
                                                    name={`fees[${index}].requires_erp_payment`}
                                                    children={(field) => (
                                                        <label
                                                            htmlFor={`${prefix}-erp-payment`}
                                                            className="flex items-center gap-2 text-sm"
                                                        >
                                                            <Checkbox
                                                                id={`${prefix}-erp-payment`}
                                                                checked={
                                                                    field.state
                                                                        .value
                                                                }
                                                                onCheckedChange={(
                                                                    value,
                                                                ) =>
                                                                    field.handleChange(
                                                                        value ===
                                                                            true,
                                                                    )
                                                                }
                                                            />
                                                            需要 ERP 付款
                                                        </label>
                                                    )}
                                                />
                                            </div>
                                            <form.AppField
                                                name={`fees[${index}].valid_conditions`}
                                                children={(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-conditions`}
                                                        label="有效条件"
                                                    />
                                                )}
                                            />
                                            <form.AppField
                                                name={`fees[${index}].public_note`}
                                                children={(field) => (
                                                    <field.TextField
                                                        id={`${prefix}-note`}
                                                        label="对销售说明"
                                                    />
                                                )}
                                            />
                                            <div className="flex justify-end">
                                                <Button
                                                    id={`${prefix}-remove`}
                                                    type="button"
                                                    size="sm"
                                                    variant="ghost"
                                                    onClick={() =>
                                                        feesField.removeValue(
                                                            index,
                                                        )
                                                    }
                                                >
                                                    移除费用
                                                </Button>
                                            </div>
                                        </div>
                                    )
                                })}
                            </div>
                        )}
                    />
                </details>
                <div
                    className="sticky bottom-0 z-20 flex flex-wrap items-center justify-end gap-3 border-t bg-background py-4"
                    onClickCapture={(event) => {
                        if (
                            !(event.target instanceof Element) ||
                            !event.target.closest(
                                "#sales-inquiry-action-publish",
                            )
                        )
                            return
                        const first = values.lines.find((line) => {
                            const demand = inquiry.draft.lines.find(
                                (item) => item.line_id === line.line_id,
                            )
                            return (
                                demand &&
                                feedbackIssues(line, demand).length > 0
                            )
                        })
                        if (first) {
                            event.preventDefault()
                            event.stopPropagation()
                            setValidationRequest((value) => value + 1)
                            setError("请先完善标出的商品反馈，再保存并发布。")
                        }
                    }}
                >
                    <p className="mr-auto text-xs text-muted-foreground">
                        已完成 {completeCount} / {values.lines.length} 项 ·{" "}
                        {saved
                            ? "处理进度已保存，发布后销售可见"
                            : "修改后请保存处理进度"}
                    </p>
                    <Button
                        id="procurement-inquiry-save-feedback"
                        type="submit"
                        variant="outline"
                        disabled={disabled}
                    >
                        {command.isPending ? "正在保存…" : "保存处理进度"}
                    </Button>
                    {footerActions}
                </div>
            </fieldset>
        </form>
    )
}
