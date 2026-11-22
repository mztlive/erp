"use client"

import { useEffect, useState } from "react"
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
    useArrangement,
    useSaveArrangement,
} from "@/features/delivery-arrangements/hooks/queries"
import type {
    AddressAllocation,
    ArrangementView,
} from "@/features/delivery-arrangements/api"
import {
    compareDecimal,
    compactFixed,
    subtractFixed,
    sumFixed,
} from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { DeliveryDestination } from "../../lib/delivery-packing"

export type AddressAction =
    | { mode: "add" }
    | { mode: "edit" | "delete"; destination: DeliveryDestination }
const precision = { maxScale: 6, outputScale: 6 } as const

/** 以服务端当前安排匹配收件身份；不从展示地址反向拆分地区。 */
function matches(row: AddressAllocation, action: AddressAction) {
    return (
        action.mode !== "add" &&
        row.recipient === action.destination.recipient &&
        row.phone === action.destination.phone &&
        `${row.region_name}${row.address}` === action.destination.address
    )
}

/** 每次打开读取完整版本，只替换选中地址；其他地址原样提交。 */
export function FulfillmentAddressDialog({
    operationId,
    action,
    hasUnsavedChanges,
    onSaved,
    onClose,
}: {
    operationId: string
    action: AddressAction
    hasUnsavedChanges: boolean
    onSaved: () => Promise<void>
    onClose: () => void
}) {
    const query = useArrangement("deliveries", operationId, true)
    const [snapshot, setSnapshot] = useState<ArrangementView | null>(null)
    const [busy, setBusy] = useState(false)
    useEffect(() => {
        if (!snapshot && query.isSuccess && !query.isFetching)
            setSnapshot(query.data)
    }, [snapshot, query.isSuccess, query.isFetching, query.data])
    const prefix = `fulfillment-address-${action.mode}-${toAutomationIdSegment(operationId)}`
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !busy) onClose()
            }}
        >
            <DialogContent
                closeButtonId={`${prefix}-close`}
                showCloseButton={!busy}
                className="max-h-[90dvh] overflow-y-auto sm:max-w-2xl"
            >
                <DialogHeader>
                    <DialogTitle>
                        {action.mode === "add"
                            ? "新增收货地址"
                            : action.mode === "edit"
                              ? "调整当前地址"
                              : "删除收货地址"}
                    </DialogTitle>
                    <DialogDescription>
                        {action.mode === "delete"
                            ? "删除后，未发商品回到待补地址；已发货记录不变。"
                            : action.mode === "edit"
                              ? "修改当前收件信息，仅影响尚未发货的包裹；已发货记录保持不变。"
                              : "填写收件信息和分配数量，保存后更新当前任务。"}
                    </DialogDescription>
                </DialogHeader>
                {snapshot ? (
                    <AddressEditor
                        data={snapshot}
                        {...{
                            operationId,
                            action,
                            hasUnsavedChanges,
                            onSaved,
                            onClose,
                            prefix,
                        }}
                        onBusyChange={setBusy}
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
                        正在读取地址…
                    </p>
                )}
            </DialogContent>
        </Dialog>
    )
}

function AddressEditor({
    data,
    operationId,
    action,
    hasUnsavedChanges,
    onSaved,
    onClose,
    onBusyChange,
    prefix,
}: {
    data: ArrangementView
    operationId: string
    action: AddressAction
    hasUnsavedChanges: boolean
    onSaved: () => Promise<void>
    onClose: () => void
    onBusyChange: (busy: boolean) => void
    prefix: string
}) {
    const mutation = useSaveArrangement("deliveries", operationId)
    const selected = data.lines.flatMap((line) =>
        line.allocations.filter((row) => matches(row, action)),
    )
    const original = selected[0]
    const unavailable =
        data.blocked_reason ||
        (action.mode !== "add" &&
        (!original ||
            selected.some((row) => row.region_code !== original.region_code))
            ? "当前地址已变化，请关闭后刷新任务。"
            : undefined)
    const editable = action.mode === "add" ? data.lines : []
    const requiresReset = data.registered_packages > 0 || hasUnsavedChanges
    const hasAddresses = data.lines.some((line) => line.allocations.length)
    const [committed, setCommitted] = useState(false)
    const [refreshError, setRefreshError] = useState<string | null>(null)
    useEffect(() => {
        onBusyChange(mutation.isPending)
        return () => onBusyChange(false)
    }, [mutation.isPending, onBusyChange])
    const maximum = (id: string) => {
        const line = data.lines.find((row) => row.line_id === id)!
        return subtractFixed(
            line.quantity,
            sumFixed(
                line.allocations
                    .filter((row) => !matches(row, action))
                    .map((row) => row.quantity),
                precision,
            ),
            precision,
        )
    }
    const refresh = async () => {
        try {
            await onSaved()
            onClose()
        } catch (error) {
            setRefreshError(
                error instanceof Error ? error.message : "任务刷新失败",
            )
        }
    }
    const form = useAppForm({
        defaultValues: {
            recipient: original?.recipient ?? "",
            phone: original?.phone ?? "",
            region_code: original?.region_code ?? "",
            address: original?.address ?? "",
            quantities: editable.map((line) => ({
                lineId: line.line_id,
                quantity: compactFixed(maximum(line.line_id)),
            })),
            reason: "",
            reset_packages: false,
        },
        validators: {
            onSubmit: ({ value }) => {
                if (unavailable) return unavailable
                if (hasAddresses && !value.reason.trim())
                    return "请填写调整原因"
                if (requiresReset && !value.reset_packages)
                    return "请确认重新核对受影响包裹"
                if (action.mode === "delete") return undefined
                if (
                    !value.recipient.trim() ||
                    !value.phone.trim() ||
                    !value.region_code ||
                    !value.address.trim()
                )
                    return "请填写完整收件信息，地区须选择到区县"
                if (action.mode === "edit") return undefined
                try {
                    for (const row of value.quantities) {
                        if (
                            compareDecimal(row.quantity, "0", 6) < 0 ||
                            compareDecimal(
                                row.quantity,
                                maximum(row.lineId),
                                6,
                            ) > 0
                        )
                            return "分配数量须在 0 与可分配数量之间"
                    }
                    if (
                        !value.quantities.some(
                            (row) => compareDecimal(row.quantity, "0", 6) > 0,
                        )
                    )
                        return "请至少分配一种商品；移除当前地址请使用删除操作"
                } catch {
                    return "请填写有效数量，最多 6 位小数"
                }
                return undefined
            },
        },
        onSubmit: async ({ value }) => {
            if (committed || unavailable) return
            try {
                await mutation.mutateAsync({
                    expected_version: data.version,
                    reason: value.reason.trim(),
                    reset_packages: value.reset_packages,
                    lines: data.lines.map((line) => {
                        // 改址只替换收件字段；商品、每条分配数量及其他地址原样保留。
                        if (action.mode === "edit") {
                            return {
                                line_id: line.line_id,
                                allocations: line.allocations.map((row) =>
                                    matches(row, action)
                                        ? {
                                              ...row,
                                              recipient: value.recipient.trim(),
                                              phone: value.phone.trim(),
                                              region_code: value.region_code,
                                              region_name: "",
                                              address: value.address.trim(),
                                          }
                                        : row,
                                ),
                            }
                        }
                        const allocations = line.allocations.filter(
                            (row) => !matches(row, action),
                        )
                        const amount =
                            value.quantities.find(
                                (row) => row.lineId === line.line_id,
                            )?.quantity ?? "0"
                        if (
                            action.mode === "add" &&
                            compareDecimal(amount, "0", 6) > 0
                        )
                            allocations.push({
                                recipient: value.recipient.trim(),
                                phone: value.phone.trim(),
                                region_code: value.region_code,
                                region_name: original?.region_name ?? "",
                                address: value.address.trim(),
                                quantity: compactFixed(amount),
                            })
                        return { line_id: line.line_id, allocations }
                    }),
                })
                setCommitted(true)
                await refresh()
            } catch {
                /* 失败保留输入，错误由 mutation 展示。 */
            }
        },
    })
    const locked = mutation.isPending || committed || !!unavailable
    return (
        <form
            id={`${prefix}-form`}
            className="space-y-4"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            {unavailable && (
                <p role="alert" className="text-body-compact text-destructive">
                    {unavailable}
                </p>
            )}
            {data.source_changed && (
                <p
                    role="status"
                    className="text-body-compact text-warning-soft-foreground"
                >
                    {action.mode === "edit"
                        ? "销售内容已变化，请核对最新收件信息。"
                        : "销售内容已变化，请重新核对分配数量。"}
                </p>
            )}
            {action.mode === "delete" ? (
                <p className="text-body-compact leading-6">
                    {action.destination.recipient} · {action.destination.phone}
                    <br />
                    {action.destination.address}
                </p>
            ) : (
                <>
                    <div className="grid gap-4 sm:grid-cols-2">
                        {(["recipient", "phone"] as const).map((name) => (
                            <form.Field key={name} name={name}>
                                {(field) => (
                                    <div className="space-y-2">
                                        <Label htmlFor={`${prefix}-${name}`}>
                                            {name === "recipient"
                                                ? "收货人"
                                                : "联系电话"}
                                        </Label>
                                        <Input
                                            id={`${prefix}-${name}`}
                                            value={field.state.value}
                                            disabled={locked}
                                            onChange={(event) =>
                                                field.handleChange(
                                                    event.target.value,
                                                )
                                            }
                                        />
                                    </div>
                                )}
                            </form.Field>
                        ))}
                    </div>
                    <form.Field name="region_code">
                        {(field) => (
                            <div className="space-y-2">
                                <Label htmlFor={`${prefix}-region-province`}>
                                    地区
                                </Label>
                                <RegionCascadeSelect
                                    idPrefix={`${prefix}-region`}
                                    leafOnly
                                    value={field.state.value}
                                    disabled={locked}
                                    onValueChange={field.handleChange}
                                />
                            </div>
                        )}
                    </form.Field>
                    <form.Field name="address">
                        {(field) => (
                            <div className="space-y-2">
                                <Label htmlFor={`${prefix}-address`}>
                                    详细地址
                                </Label>
                                <Input
                                    id={`${prefix}-address`}
                                    value={field.state.value}
                                    disabled={locked}
                                    onChange={(event) =>
                                        field.handleChange(event.target.value)
                                    }
                                />
                            </div>
                        )}
                    </form.Field>
                    {action.mode === "add" && (
                        <div className="max-h-64 space-y-3 overflow-y-auto overscroll-contain">
                            {editable.map((line, index) => (
                                <form.Field
                                    key={line.line_id}
                                    name={`quantities[${index}].quantity`}
                                >
                                    {(field) => (
                                        <div className="flex items-center gap-3">
                                            <Label
                                                htmlFor={`${prefix}-${toAutomationIdSegment(line.line_id)}-quantity`}
                                                className="min-w-0 flex-1"
                                            >
                                                {line.item_name}
                                            </Label>
                                            <span className="text-xs text-muted-foreground">
                                                可分配{" "}
                                                {compactFixed(
                                                    maximum(line.line_id),
                                                )}{" "}
                                                {line.unit}
                                            </span>
                                            <Input
                                                id={`${prefix}-${toAutomationIdSegment(line.line_id)}-quantity`}
                                                className="w-24"
                                                inputMode="decimal"
                                                value={field.state.value}
                                                disabled={locked}
                                                onChange={(event) =>
                                                    field.handleChange(
                                                        event.target.value,
                                                    )
                                                }
                                            />
                                        </div>
                                    )}
                                </form.Field>
                            ))}
                        </div>
                    )}
                    {action.mode === "add" &&
                        editable.every(
                            (line) =>
                                compareDecimal(maximum(line.line_id), "0", 6) <=
                                0,
                        ) && (
                            <p className="text-xs text-muted-foreground">
                                当前商品已全部分配，暂无可分配到新地址的商品。
                            </p>
                        )}
                </>
            )}
            <form.Field name="reason">
                {(field) => (
                    <div className="space-y-2">
                        <Label htmlFor={`${prefix}-reason`}>
                            调整原因{hasAddresses ? "" : "（选填）"}
                        </Label>
                        <Textarea
                            id={`${prefix}-reason`}
                            maxLength={500}
                            value={field.state.value}
                            disabled={locked}
                            onChange={(event) =>
                                field.handleChange(event.target.value)
                            }
                        />
                    </div>
                )}
            </form.Field>
            {requiresReset && (
                <form.Field name="reset_packages">
                    {(field) => (
                        <Label
                            htmlFor={`${prefix}-reset`}
                            className="items-start text-xs font-normal leading-5"
                        >
                            <Checkbox
                                id={`${prefix}-reset`}
                                checked={field.state.value}
                                disabled={locked}
                                onCheckedChange={field.handleChange}
                            />
                            我已确认：受影响的包裹需重新登记。
                            {hasUnsavedChanges
                                ? "本页尚未保存的录入需重新填写。"
                                : ""}
                        </Label>
                    )}
                </form.Field>
            )}
            <FormValidationSummary form={form} />
            {mutation.isError && (
                <p role="alert" className="text-body-compact text-destructive">
                    {mutation.error.message}。输入已保留。
                </p>
            )}
            {refreshError && (
                <div role="alert">
                    <p className="text-body-compact">
                        地址已保存，任务刷新失败：{refreshError}
                    </p>
                    <Button
                        id={`${prefix}-refresh`}
                        type="button"
                        onClick={() => void refresh()}
                    >
                        重新载入任务
                    </Button>
                </div>
            )}
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
                    variant={
                        action.mode === "delete" ? "destructive" : "default"
                    }
                    disabled={locked}
                >
                    {mutation.isPending
                        ? "保存中…"
                        : action.mode === "delete"
                          ? "确认删除"
                          : "保存地址"}
                </Button>
            </DialogFooter>
        </form>
    )
}
