"use client"

import * as React from "react"
import { useAppForm } from "@/components/form"
import { MoneyValue } from "@/components/business"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { getErrorMessage } from "@/lib/api/errors"
import { compareDecimal } from "@/lib/fixed-decimal"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    SaveSupplierExecutionSetup,
    SupplierExecutionSetup,
} from "../api/execution-setup"

const METHOD_LABEL = {
    COURIER: "快递配送",
    LOCAL_DELIVERY: "同城配送",
    PICKUP: "到店自提",
} as const

function coordinateValid(value: string, maximum: string) {
    try {
        return (
            compareDecimal(value.trim(), `-${maximum}`, 12) >= 0 &&
            compareDecimal(value.trim(), maximum, 12) <= 0
        )
    } catch {
        return false
    }
}

export function SupplierExecutionSetupForm({
    setup,
    pending,
    error,
    onSubmit,
    onCancel,
    orderId,
    addressSummary,
}: {
    setup: SupplierExecutionSetup
    pending: boolean
    error: unknown
    onSubmit: (request: SaveSupplierExecutionSetup) => Promise<void>
    onCancel: () => void
    orderId: string
    addressSummary: string
}) {
    const prefix = `supplier-execution-setup-${toAutomationIdSegment(orderId)}`
    const identity = React.useRef<{ payload: string; key: string } | null>(null)
    const form = useAppForm({
        defaultValues: {
            cityName: setup.geocode?.city_name ?? "",
            district: setup.geocode?.district ?? "",
            latitude: setup.geocode?.latitude ?? "",
            longitude: setup.geocode?.longitude ?? "",
            coordinateConfirmed: false,
            recipientPhone: "",
            choiceIndex: setup.selected_choice as number | null,
        },
        validators: {
            onSubmit: ({ value }) => {
                if (
                    setup.recipient_phone_required &&
                    !/^[+\d][\d\s()-]{5,31}$/.test(value.recipientPhone.trim())
                )
                    return "请填写收货仓的真实收件联系电话，不能使用客户电话替代"
                const hasInput = [
                    value.cityName,
                    value.district,
                    value.latitude,
                    value.longitude,
                ].some((part) => Boolean(part.trim()))
                const changed =
                    value.cityName !== (setup.geocode?.city_name ?? "") ||
                    value.district !== (setup.geocode?.district ?? "") ||
                    value.latitude !== (setup.geocode?.latitude ?? "") ||
                    value.longitude !== (setup.geocode?.longitude ?? "")
                if (
                    setup.geocode_editable &&
                    hasInput &&
                    (!value.cityName.trim() ||
                        !value.district.trim() ||
                        !coordinateValid(value.latitude, "90") ||
                        !coordinateValid(value.longitude, "180"))
                )
                    return "请完整填写城市、区县和有效的百度坐标"
                if (
                    setup.geocode_editable &&
                    changed &&
                    !value.coordinateConfirmed
                )
                    return "请核对坐标确为收件地址的百度 BD-09 坐标，并勾选确认"
                if (setup.geocode_editable && !setup.geocode && !hasInput)
                    return "请先补充经过核验的收件地区和百度坐标"
                if (
                    setup.choice_editable &&
                    !changed &&
                    setup.choices.length > 0 &&
                    value.choiceIndex === null
                )
                    return "请选择供应商返回的配送方案"
                if (!changed && value.choiceIndex === setup.selected_choice)
                    return "资料未变化，请修改后提交或关闭窗口"
                return undefined
            },
        },
        onSubmit: async ({ value }) => {
            const geocodeChanged =
                value.cityName !== (setup.geocode?.city_name ?? "") ||
                value.district !== (setup.geocode?.district ?? "") ||
                value.latitude !== (setup.geocode?.latitude ?? "") ||
                value.longitude !== (setup.geocode?.longitude ?? "")
            const request = {
                expected_version: setup.execution_version,
                ...(setup.recipient_phone_required
                    ? { recipient_phone: value.recipientPhone.trim() }
                    : {}),
                ...(setup.geocode_editable && geocodeChanged
                    ? {
                          geocode: {
                              city_name: value.cityName.trim(),
                              district: value.district.trim(),
                              latitude: value.latitude.trim(),
                              longitude: value.longitude.trim(),
                              coordinate_system: "BD09" as const,
                          },
                      }
                    : {}),
                ...(setup.choice_editable &&
                !geocodeChanged &&
                value.choiceIndex !== null
                    ? { choice_index: value.choiceIndex }
                    : {}),
            }
            const payload = JSON.stringify(request)
            if (identity.current?.payload !== payload)
                identity.current = { payload, key: crypto.randomUUID() }
            await onSubmit({
                ...request,
                idempotency_key: identity.current.key,
            })
        },
    })
    return (
        <form
            id={`${prefix}-form`}
            className="space-y-4"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <Alert variant="info">
                <AlertDescription>
                    收件人和详细地址使用已批准采购资料。仅补充该地址对应的城市、区县及真实百度
                    BD-09
                    坐标，不接受其他坐标直接替用。保存后由后台继续核验并取得可选配送方案。
                </AlertDescription>
            </Alert>
            <div className="rounded-md border p-3 text-sm">
                <p className="font-medium">本单收件资料</p>
                <p className="mt-1 text-muted-foreground">
                    {addressSummary ||
                        "收件摘要暂不可查，请先核对原采购单资料。"}
                </p>
                <p className="mt-1 text-xs text-muted-foreground">
                    完整地址按原采购单和既有地址查看权限核对。
                </p>
            </div>
            {setup.recipient_phone_required ? (
                <form.Field name="recipientPhone">
                    {(field) => (
                        <div className="space-y-1.5">
                            <Label htmlFor={`${prefix}-recipient-phone`}>
                                收货仓收件联系电话
                            </Label>
                            <Input
                                id={`${prefix}-recipient-phone`}
                                type="tel"
                                autoComplete="tel"
                                value={field.state.value}
                                onChange={(event) =>
                                    field.handleChange(event.target.value)
                                }
                                onBlur={field.handleBlur}
                                disabled={pending || !setup.geocode_editable}
                                placeholder="填写本单收货仓的真实联系电话"
                            />
                            <p className="text-xs text-muted-foreground">
                                收货目标为采购单指定仓库。首次补齐其缺失联系电话，不替换为客户电话，也不修改已有冻结电话。
                            </p>
                        </div>
                    )}
                </form.Field>
            ) : null}
            <div className="grid gap-3 sm:grid-cols-2">
                {(
                    [
                        {
                            key: "cityName",
                            label: "城市名称",
                            placeholder: "例如：上海市",
                        },
                        {
                            key: "district",
                            label: "区县名称",
                            placeholder: "例如：徐汇区",
                        },
                        {
                            key: "latitude",
                            label: "百度纬度",
                            placeholder: "请填写已核验的纬度",
                        },
                        {
                            key: "longitude",
                            label: "百度经度",
                            placeholder: "请填写已核验的经度",
                        },
                    ] as const
                ).map(({ key, label, placeholder }) => (
                    <form.Field key={key} name={key}>
                        {(field) => (
                            <div className="space-y-1.5">
                                <Label htmlFor={`${prefix}-${key}`}>
                                    {label}
                                </Label>
                                <Input
                                    id={`${prefix}-${key}`}
                                    value={field.state.value}
                                    onChange={(event) => {
                                        field.handleChange(event.target.value)
                                        form.setFieldValue(
                                            "coordinateConfirmed",
                                            false,
                                        )
                                    }}
                                    onBlur={field.handleBlur}
                                    inputMode={
                                        key === "latitude" ||
                                        key === "longitude"
                                            ? "decimal"
                                            : "text"
                                    }
                                    placeholder={placeholder}
                                    disabled={
                                        pending || !setup.geocode_editable
                                    }
                                />
                            </div>
                        )}
                    </form.Field>
                ))}
            </div>
            {setup.geocode_editable ? (
                <form.Field name="coordinateConfirmed">
                    {(field) => (
                        <div className="flex items-start gap-2">
                            <Checkbox
                                id={`${prefix}-coordinate-confirmed`}
                                checked={field.state.value}
                                onCheckedChange={(checked) =>
                                    field.handleChange(checked === true)
                                }
                                disabled={pending}
                            />
                            <Label
                                htmlFor={`${prefix}-coordinate-confirmed`}
                                className="text-sm font-normal"
                            >
                                已核对坐标来自可信百度 BD-09
                                数据，且对应本单收件地址
                            </Label>
                        </div>
                    )}
                </form.Field>
            ) : (
                <p className="text-xs text-muted-foreground">
                    供应商已确认地址资料，当前不能更改坐标；请从实际返回的配送方案中选择。
                </p>
            )}
            {setup.choices.length > 0 ? (
                <fieldset className="space-y-3">
                    <legend className="mb-2 font-medium">
                        供应商可用配送方案
                    </legend>
                    <form.Field name="choiceIndex">
                        {(field) => (
                            <RadioGroup
                                id={`${prefix}-choices`}
                                className="space-y-2"
                                value={
                                    field.state.value === null
                                        ? null
                                        : String(field.state.value)
                                }
                                onValueChange={(value) => {
                                    const choice = setup.choices.find(
                                        (item) => String(item.index) === value,
                                    )
                                    field.handleChange(choice?.index ?? null)
                                }}
                                disabled={pending || !setup.choice_editable}
                            >
                                {setup.choices.map((choice) => (
                                    <Label
                                        key={choice.index}
                                        htmlFor={`${prefix}-choice-${choice.index}`}
                                        className="flex cursor-pointer items-start gap-3 rounded-md border p-3 font-normal"
                                    >
                                        <RadioGroupItem
                                            id={`${prefix}-choice-${choice.index}`}
                                            value={String(choice.index)}
                                            disabled={
                                                pending ||
                                                !setup.choice_editable
                                            }
                                        />
                                        <span className="space-y-1">
                                            <span className="block">
                                                {METHOD_LABEL[choice.method]} ·{" "}
                                                {choice.date ??
                                                    "日期由供应商确认"}
                                                {choice.slot
                                                    ? ` · ${choice.slot}`
                                                    : ""}
                                            </span>
                                            <span className="block text-xs text-muted-foreground">
                                                配送费用{" "}
                                                <MoneyValue
                                                    value={choice.shipping_fee}
                                                    display="exact"
                                                />
                                                {choice.store_id
                                                    ? " · 指定门店方案"
                                                    : ""}
                                            </span>
                                        </span>
                                    </Label>
                                ))}
                            </RadioGroup>
                        )}
                    </form.Field>
                </fieldset>
            ) : (
                <p className="text-sm text-muted-foreground">
                    配送方案尚未取得。保存地址资料后，刷新执行状态查看实际返回结果。
                </p>
            )}
            <form.Subscribe selector={(state) => state.errors}>
                {(errors) =>
                    errors.length ? (
                        <p role="alert" className="text-sm text-destructive">
                            {errors.map(String).join("；")}
                        </p>
                    ) : null
                }
            </form.Subscribe>
            {error ? (
                <Alert variant="destructive">
                    <AlertDescription>
                        {getErrorMessage(error)}。请刷新执行资料并重新核对。
                    </AlertDescription>
                </Alert>
            ) : null}
            <div className="flex justify-end gap-2">
                <Button
                    id={`${prefix}-cancel`}
                    type="button"
                    variant="outline"
                    disabled={pending}
                    onClick={onCancel}
                >
                    取消
                </Button>
                <Button id={`${prefix}-save`} type="submit" disabled={pending}>
                    {pending ? "保存中…" : "保存配送资料"}
                </Button>
            </div>
        </form>
    )
}
