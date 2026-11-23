"use client"
import { type ComponentProps, useEffect, useRef, useState } from "react"
import { MapPinIcon, PlusIcon, Trash2Icon } from "lucide-react"
import { FormValidationSummary, useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Checkbox } from "@/components/ui/checkbox"
import { Textarea } from "@/components/ui/textarea"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
    DialogFooter,
} from "@/components/ui/dialog"
import { RegionCascadeSelect } from "@/features/regions"
import {
    compactFixed,
    compareDecimal,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useArrangement, useSaveArrangement } from "../hooks/queries"
import type {
    AddressAllocation,
    ArrangementScope,
    ArrangementView,
} from "../api"

type Props = {
    scope: ArrangementScope
    objectId: string
    disabled?: boolean
    label?: string
    variant?: ComponentProps<typeof Button>["variant"]
    className?: string
    hasUnsavedChanges?: boolean
    onSaved?: () => void | Promise<void>
    idPrefix?: string
    unitLabels?: Readonly<Record<string, string>>
}

/** 受控的收货安排编辑弹窗；打开时读取最新范围，取消不改变原安排。 */
export function ArrangementDialog({
    scope,
    objectId,
    open,
    onOpenChange,
    hasUnsavedChanges,
    onSaved,
    idPrefix,
    unitLabels,
}: {
    scope: ArrangementScope
    objectId: string
    open: boolean
    onOpenChange: (open: boolean) => void
    hasUnsavedChanges?: boolean
    onSaved?: () => void | Promise<void>
    idPrefix?: string
    unitLabels?: Readonly<Record<string, string>>
}) {
    const [busy, setBusy] = useState(false)
    const [editingData, setEditingData] = useState<ArrangementView | null>(null)
    const query = useArrangement(scope, objectId, open)
    useEffect(() => {
        if (!open) {
            setEditingData(null)
            return
        }
        if (!editingData && !query.isFetching && query.isSuccess)
            setEditingData(query.data)
    }, [open, editingData, query.isFetching, query.isSuccess, query.data])
    const prefix =
        idPrefix ??
        `delivery-arrangement-${scope}-${toAutomationIdSegment(objectId)}`
    return (
        <Dialog
            open={open}
            onOpenChange={(next) => {
                if (!busy) onOpenChange(next)
            }}
        >
            <DialogContent
                className="max-h-[90vh] overflow-y-auto sm:max-w-5xl"
                closeButtonId={`${prefix}-close`}
                showCloseButton={!busy}
            >
                <DialogHeader>
                    <DialogTitle>补充或调整收货安排</DialogTitle>
                    <DialogDescription>
                        未分配的数量可暂留待补，已有地址的商品可以先发货。已发包裹保持原地址；价格、运费变化另行办理改单。
                    </DialogDescription>
                </DialogHeader>
                {editingData ? (
                    <ArrangementEditor
                        key={prefix}
                        scope={scope}
                        objectId={objectId}
                        prefix={prefix}
                        hasUnsavedChanges={hasUnsavedChanges}
                        onSaved={onSaved}
                        unitLabels={unitLabels}
                        data={editingData}
                        onBusyChange={setBusy}
                        onClose={() => onOpenChange(false)}
                    />
                ) : query.isError ? (
                    <div role="alert" className="space-y-3">
                        <p>{query.error.message}</p>
                        <Button
                            id={`${prefix}-retry`}
                            onClick={() => void query.refetch()}
                        >
                            重新读取
                        </Button>
                    </div>
                ) : (
                    <p className="text-xs text-muted-foreground">
                        正在读取收货安排…
                    </p>
                )}
            </DialogContent>
        </Dialog>
    )
}

/** 销售详情与履约任务共用操作；打开时读取最新范围，取消不改变原安排。 */
export function ArrangementButton({
    scope,
    objectId,
    disabled,
    label = "收货安排",
    variant = "outline",
    className,
    hasUnsavedChanges,
    onSaved,
    idPrefix,
    unitLabels,
}: Props) {
    const [open, setOpen] = useState(false)
    const prefix =
        idPrefix ??
        `delivery-arrangement-${scope}-${toAutomationIdSegment(objectId)}`
    return (
        <>
            <Button
                id={`${prefix}-open`}
                type="button"
                variant={variant}
                size="sm"
                className={className}
                disabled={disabled}
                onClick={() => setOpen(true)}
            >
                <MapPinIcon data-icon="inline-start" />
                {label}
            </Button>
            <ArrangementDialog
                scope={scope}
                objectId={objectId}
                open={open}
                onOpenChange={setOpen}
                hasUnsavedChanges={hasUnsavedChanges}
                onSaved={onSaved}
                idPrefix={prefix}
                unitLabels={unitLabels}
            />
        </>
    )
}
type EditAllocation = AddressAllocation & { rowKey: string }
const emptyAddress = (key: string, quantity: string): EditAllocation => ({
    rowKey: key,
    recipient: "",
    phone: "",
    region_code: "",
    region_name: "",
    address: "",
    quantity,
})
function ArrangementEditor({
    scope,
    objectId,
    prefix,
    data: incomingData,
    onBusyChange,
    onClose,
    onSaved,
    hasUnsavedChanges,
    unitLabels,
}: Props & {
    prefix: string
    data: ArrangementView
    onClose: () => void
    onBusyChange: (busy: boolean) => void
}) {
    const [data] = useState(incomingData)
    const mutation = useSaveArrangement(scope, objectId)
    useEffect(() => {
        onBusyChange(mutation.isPending)
        return () => onBusyChange(false)
    }, [mutation.isPending, onBusyChange])
    const sequence = useRef(0)
    const hasAddresses = data.lines.some((line) => line.allocations.length)
    const requiresReset = data.registered_packages > 0 || hasUnsavedChanges
    const [saved, setSaved] = useState(false)
    const [refreshError, setRefreshError] = useState<string | null>(null)
    const form = useAppForm({
        defaultValues: {
            lines: data.lines.map((line) => ({
                line_id: line.line_id,
                allocations: line.allocations.map((row) => ({
                    ...row,
                    rowKey: `${line.line_id}-${row.recipient}-${row.phone}-${row.region_code}-${row.address}`,
                })),
            })),
            reason: "",
            reset_packages: false,
        },
        validators: {
            onSubmit: ({ value }) => {
                if (data.blocked_reason) return data.blocked_reason
                if (requiresReset && !value.reset_packages)
                    return "请确认重新核对受影响包裹后再保存"
                try {
                    for (const line of value.lines) {
                        const capacity = data.lines.find(
                            (row) => row.line_id === line.line_id,
                        )!
                        for (const row of line.allocations) {
                            if (
                                !row.recipient.trim() ||
                                !row.phone.trim() ||
                                !row.region_code ||
                                !row.address.trim()
                            )
                                return "请填写完整收件信息，地区须选择到区县"
                            if (compareDecimal(row.quantity, "0", 6) <= 0)
                                return "地址分配数量必须大于0"
                        }
                        if (
                            compareDecimal(
                                sumFixed(
                                    line.allocations.map((row) => row.quantity),
                                    { maxScale: 6, outputScale: 6 },
                                ),
                                capacity.quantity,
                                6,
                            ) > 0
                        )
                            return `${capacity.item_name}的地址分配数量超过商品数量`
                    }
                } catch {
                    return "请填写有效数量，最多6位小数"
                }
                return undefined
            },
        },
        onSubmit: async ({ value }) => {
            let committed = false
            try {
                await mutation.mutateAsync({
                    expected_version: data.version,
                    reason: value.reason,
                    reset_packages: value.reset_packages,
                    lines: value.lines.map((line) => ({
                        line_id: line.line_id,
                        allocations: line.allocations.map(
                            ({ rowKey: _key, ...row }) => row,
                        ),
                    })),
                })
                committed = true
                setSaved(true)
                await onSaved?.()
                onClose()
            } catch (error) {
                if (committed)
                    setRefreshError(
                        error instanceof Error ? error.message : "任务刷新失败",
                    )
            }
        },
    })
    const remainder = (index: number, rows: EditAllocation[]) => {
        try {
            return subtractFixed(
                data.lines[index].quantity,
                sumFixed(
                    rows.map((row) => row.quantity || "0"),
                    { maxScale: 6, outputScale: 6 },
                ),
                { maxScale: 6, outputScale: 6 },
            )
        } catch {
            return "0"
        }
    }
    return (
        <form
            id={`${prefix}-form`}
            className="space-y-5"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            {data.blocked_reason ? (
                <p role="alert" className="text-body-compact text-destructive">
                    {data.blocked_reason}
                </p>
            ) : null}
            {data.source_changed ? (
                <p role="status" className="text-body-compact text-warning">
                    销售内容已有更新，请重新核对本次收货安排。
                </p>
            ) : null}
            <form.Subscribe selector={(state) => state.values.lines}>
                {(lines) => (
                    <div className="space-y-4">
                        {lines.map((line, index) => {
                            const source = data.lines[index]
                            const displayUnit =
                                unitLabels?.[source.line_id] || source.unit
                            const linePrefix = `${prefix}-${toAutomationIdSegment(line.line_id)}`
                            return (
                                <section
                                    key={line.line_id}
                                    className="space-y-3 rounded-lg border p-4"
                                >
                                    <div className="flex flex-wrap items-center justify-between gap-3">
                                        <div>
                                            <p className="font-medium">
                                                {source.item_name}
                                            </p>
                                            <p className="text-xs text-muted-foreground">
                                                共{" "}
                                                {compactFixed(source.quantity)}{" "}
                                                {displayUnit} · 待补地址{" "}
                                                {compactFixed(
                                                    remainder(
                                                        index,
                                                        line.allocations,
                                                    ),
                                                )}{" "}
                                                {displayUnit}
                                            </p>
                                        </div>
                                        <Button
                                            id={`${linePrefix}-add`}
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={
                                                mutation.isPending ||
                                                line.allocations.length >= 100
                                            }
                                            onClick={() => {
                                                sequence.current += 1
                                                const left = remainder(
                                                    index,
                                                    line.allocations,
                                                )
                                                form.setFieldValue(
                                                    `lines[${index}].allocations`,
                                                    [
                                                        ...line.allocations,
                                                        emptyAddress(
                                                            `new-${sequence.current}`,
                                                            compareDecimal(
                                                                left,
                                                                "0",
                                                                6,
                                                            ) > 0
                                                                ? compactFixed(
                                                                      left,
                                                                  )
                                                                : "",
                                                        ),
                                                    ],
                                                )
                                            }}
                                        >
                                            <PlusIcon data-icon="inline-start" />
                                            添加收货地址
                                        </Button>
                                    </div>
                                    {source.shipped_addresses.length ? (
                                        <div className="rounded-md border p-3 text-body-compact">
                                            <p className="font-medium">
                                                已发部分（地址及数量不可减少）
                                            </p>
                                            {source.shipped_addresses.map(
                                                (row) => (
                                                    <p
                                                        key={row.key}
                                                        className="mt-1 text-muted-foreground"
                                                    >
                                                        {row.recipient} ·{" "}
                                                        {row.phone} ·{" "}
                                                        {row.address} ·{" "}
                                                        {compactFixed(
                                                            row.quantity,
                                                        )}{" "}
                                                        {displayUnit}
                                                    </p>
                                                ),
                                            )}
                                        </div>
                                    ) : null}
                                    {line.allocations.length === 0 ? (
                                        <p className="text-xs text-muted-foreground">
                                            待补收货地址；本项商品暂不发货。
                                        </p>
                                    ) : null}
                                    {line.allocations.map(
                                        (row, addressIndex) => {
                                            const rowPrefix = `${linePrefix}-${toAutomationIdSegment(row.rowKey)}`
                                            return (
                                                <div
                                                    key={row.rowKey}
                                                    className="grid gap-3 rounded-lg bg-muted/30 p-3 sm:grid-cols-2 lg:grid-cols-3"
                                                >
                                                    {(
                                                        [
                                                            [
                                                                "recipient",
                                                                "收货人",
                                                            ],
                                                            [
                                                                "phone",
                                                                "联系电话",
                                                            ],
                                                            [
                                                                "address",
                                                                "详细地址",
                                                            ],
                                                            [
                                                                "quantity",
                                                                "分配数量",
                                                            ],
                                                        ] as const
                                                    ).map(([name, title]) => (
                                                        <form.Field
                                                            key={name}
                                                            name={`lines[${index}].allocations[${addressIndex}].${name}`}
                                                        >
                                                            {(field) => (
                                                                <div className="space-y-1.5">
                                                                    <Label
                                                                        htmlFor={`${rowPrefix}-${name}`}
                                                                    >
                                                                        {title}
                                                                    </Label>
                                                                    <Input
                                                                        id={`${rowPrefix}-${name}`}
                                                                        value={
                                                                            field
                                                                                .state
                                                                                .value
                                                                        }
                                                                        onBlur={
                                                                            field.handleBlur
                                                                        }
                                                                        disabled={
                                                                            mutation.isPending
                                                                        }
                                                                        inputMode={
                                                                            name ===
                                                                            "quantity"
                                                                                ? "decimal"
                                                                                : undefined
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
                                                                    />
                                                                </div>
                                                            )}
                                                        </form.Field>
                                                    ))}
                                                    <form.Field
                                                        name={`lines[${index}].allocations[${addressIndex}].region_code`}
                                                    >
                                                        {(field) => (
                                                            <div className="space-y-1.5">
                                                                <Label
                                                                    htmlFor={`${rowPrefix}-region-province`}
                                                                >
                                                                    收货地区
                                                                </Label>
                                                                <RegionCascadeSelect
                                                                    idPrefix={`${rowPrefix}-region`}
                                                                    leafOnly
                                                                    value={
                                                                        field
                                                                            .state
                                                                            .value
                                                                    }
                                                                    onValueChange={
                                                                        field.handleChange
                                                                    }
                                                                    onBlur={
                                                                        field.handleBlur
                                                                    }
                                                                    disabled={
                                                                        mutation.isPending
                                                                    }
                                                                />
                                                            </div>
                                                        )}
                                                    </form.Field>
                                                    <div className="flex flex-wrap items-end gap-2">
                                                        <Button
                                                            id={`${rowPrefix}-apply-remaining`}
                                                            type="button"
                                                            variant="outline"
                                                            size="sm"
                                                            disabled={
                                                                mutation.isPending ||
                                                                !row.recipient ||
                                                                !row.phone ||
                                                                !row.region_code ||
                                                                !row.address
                                                            }
                                                            onClick={() => {
                                                                lines.forEach(
                                                                    (
                                                                        other,
                                                                        otherIndex,
                                                                    ) => {
                                                                        if (
                                                                            other.line_id ===
                                                                            line.line_id
                                                                        )
                                                                            return
                                                                        const left =
                                                                            remainder(
                                                                                otherIndex,
                                                                                other.allocations,
                                                                            )
                                                                        if (
                                                                            compareDecimal(
                                                                                left,
                                                                                "0",
                                                                                6,
                                                                            ) <=
                                                                            0
                                                                        )
                                                                            return
                                                                        sequence.current += 1
                                                                        form.setFieldValue(
                                                                            `lines[${otherIndex}].allocations`,
                                                                            [
                                                                                ...other.allocations,
                                                                                {
                                                                                    ...row,
                                                                                    quantity:
                                                                                        compactFixed(
                                                                                            left,
                                                                                        ),
                                                                                    rowKey: `new-${sequence.current}`,
                                                                                },
                                                                            ],
                                                                        )
                                                                    },
                                                                )
                                                            }}
                                                        >
                                                            其他待补商品也送到这里
                                                        </Button>
                                                        <Button
                                                            id={`${rowPrefix}-remove`}
                                                            type="button"
                                                            variant="ghost"
                                                            disabled={
                                                                mutation.isPending
                                                            }
                                                            onClick={() =>
                                                                form.setFieldValue(
                                                                    `lines[${index}].allocations`,
                                                                    line.allocations.filter(
                                                                        (
                                                                            entry,
                                                                        ) =>
                                                                            entry.rowKey !==
                                                                            row.rowKey,
                                                                    ),
                                                                )
                                                            }
                                                        >
                                                            <Trash2Icon data-icon="inline-start" />
                                                            移除地址
                                                        </Button>
                                                    </div>
                                                </div>
                                            )
                                        },
                                    )}
                                </section>
                            )
                        })}
                    </div>
                )}
            </form.Subscribe>
            {data.lines.length === 0 ? (
                <p>当前销售单没有可补充收货地址的商品。</p>
            ) : null}
            <form.Field name="reason">
                {(field) => (
                    <div className="space-y-2">
                        <Label htmlFor={`${prefix}-reason`}>
                            调整原因
                            {hasAddresses
                                ? "（修改已有地址时必填）"
                                : "（选填）"}
                        </Label>
                        <Textarea
                            id={`${prefix}-reason`}
                            value={field.state.value}
                            maxLength={500}
                            disabled={mutation.isPending}
                            onChange={(event) =>
                                field.handleChange(event.target.value)
                            }
                            onBlur={field.handleBlur}
                        />
                    </div>
                )}
            </form.Field>
            {requiresReset ? (
                <form.Field name="reset_packages">
                    {(field) => (
                        <div className="flex items-start gap-2 rounded-lg border border-warning/30 bg-warning-soft p-3">
                            <Checkbox
                                id={`${prefix}-reset-packages`}
                                checked={field.state.value}
                                onCheckedChange={(checked) =>
                                    field.handleChange(Boolean(checked))
                                }
                                disabled={mutation.isPending}
                            />
                            <Label
                                htmlFor={`${prefix}-reset-packages`}
                                className="block leading-relaxed"
                            >
                                我已确认：地址发生变化的包裹需要重新登记，原物流号保留供核对。
                                {hasUnsavedChanges
                                    ? "保存后会重新载入任务，本页尚未保存的录入需要重新填写。"
                                    : ""}
                            </Label>
                        </div>
                    )}
                </form.Field>
            ) : null}
            <FormValidationSummary form={form} />
            {mutation.isError ? (
                <p role="alert" className="text-body-compact text-destructive">
                    {mutation.error.message}
                    。已保留您刚才填写的内容；若单据已被他人更新，请关闭窗口后重新打开核对。
                </p>
            ) : null}
            {saved ? <p role="status">收货安排已保存。</p> : null}
            {refreshError ? (
                <div role="alert" className="space-y-2">
                    <p>收货安排已保存，任务刷新失败：{refreshError}</p>
                    <Button
                        id={`${prefix}-reload-task`}
                        type="button"
                        onClick={() => {
                            void Promise.resolve(onSaved?.())
                                .then(onClose)
                                .catch((error: unknown) =>
                                    setRefreshError(
                                        error instanceof Error
                                            ? error.message
                                            : "任务刷新失败",
                                    ),
                                )
                        }}
                    >
                        重新载入任务
                    </Button>
                </div>
            ) : null}
            <DialogFooter>
                <Button
                    id={`${prefix}-cancel`}
                    type="button"
                    variant="outline"
                    disabled={mutation.isPending}
                    onClick={onClose}
                >
                    取消
                </Button>
                <Button
                    id={`${prefix}-save`}
                    type="submit"
                    disabled={
                        mutation.isPending ||
                        saved ||
                        Boolean(data.blocked_reason) ||
                        !data.lines.length
                    }
                >
                    {mutation.isPending ? "正在保存…" : "保存收货安排"}
                </Button>
            </DialogFooter>
        </form>
    )
}
