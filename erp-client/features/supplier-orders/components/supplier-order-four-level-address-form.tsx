"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { FormValidationSummary, useAppForm } from "@/components/form"
import { OptionCombobox } from "@/components/business/option-combobox"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type {
    SaveSupplierExecutionSetup,
    SupplierAddressCandidate,
    SupplierExecutionSetup,
} from "../api/execution-setup"
import { useSupplierAddressCandidatesQuery } from "../hooks/use-execution-setup"

/** 按供应商返回候选确认四级地址，最终仅提交服务端签发的完整地址票据。 */
export function SupplierOrderFourLevelAddressForm({
    orderId,
    setup,
    addressSummary,
    pending,
    error,
    onSubmit,
    onCancel,
}: {
    orderId: string
    setup: SupplierExecutionSetup
    addressSummary: string
    pending: boolean
    error: unknown
    onSubmit: (request: SaveSupplierExecutionSetup) => Promise<void>
    onCancel: () => void
}) {
    const prefix = `supplier-four-level-address-${toAutomationIdSegment(orderId)}`
    const identity = React.useRef<{ payload: string; key: string } | null>(null)
    const form = useAppForm({
        defaultValues: {
            province: "",
            city: "",
            county: "",
            town: "",
            confirmed: false,
            phone: "",
        },
        validators: {
            onSubmit: ({ value }) => {
                if (
                    !value.province ||
                    !value.city ||
                    !value.county ||
                    !value.town
                )
                    return "请选择供应商返回的完整四级地址"
                if (!value.confirmed) return "请核对完整收货地址并确认所选区域"
                if (
                    setup.recipient_phone_required &&
                    !/^[+\d][\d\s()-]{5,31}$/.test(value.phone.trim())
                )
                    return "请补齐收货仓的真实收件联系电话"
                return undefined
            },
        },
        onSubmit: async ({ value }) => {
            const request = {
                expected_version: setup.execution_version,
                supplier_address_ticket: value.town,
                ...(setup.recipient_phone_required
                    ? { recipient_phone: value.phone.trim() }
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
    const values = useStore(form.store, (state) => state.values)
    const province = useSupplierAddressCandidatesQuery(
        orderId,
        setup.execution_version,
        "PROVINCE",
        undefined,
        true,
    )
    const city = useSupplierAddressCandidatesQuery(
        orderId,
        setup.execution_version,
        "CITY",
        values.province,
        Boolean(values.province),
    )
    const county = useSupplierAddressCandidatesQuery(
        orderId,
        setup.execution_version,
        "COUNTY",
        values.city,
        Boolean(values.city),
    )
    const town = useSupplierAddressCandidatesQuery(
        orderId,
        setup.execution_version,
        "TOWN",
        values.county,
        Boolean(values.county),
    )
    const queryError = [province, city, county, town].find(
        (query) => query.isError,
    )
    const fields = [
        {
            name: "province",
            label: "省 / 直辖市",
            query: province,
            enabled: true,
        },
        {
            name: "city",
            label: "市",
            query: city,
            enabled: Boolean(values.province),
        },
        {
            name: "county",
            label: "区 / 县",
            query: county,
            enabled: Boolean(values.city),
        },
        {
            name: "town",
            label: "街道 / 乡镇",
            query: town,
            enabled: Boolean(values.county),
        },
    ] as const
    const fieldNames = ["province", "city", "county", "town"] as const
    const candidateLabel = (
        candidates: readonly SupplierAddressCandidate[] | undefined,
        ticket: string,
    ) => candidates?.find((candidate) => candidate.ticket === ticket)?.name
    const selectedSummary = fields
        .map((field) =>
            candidateLabel(field.query.data?.candidates, values[field.name]),
        )
        .filter(Boolean)
        .join(" / ")
    return (
        <form
            id={`${prefix}-form`}
            className="space-y-5"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <Alert variant="info">
                <AlertDescription>
                    按已批准采购的收货地址选择京东四级区域。候选来自供应商，不使用本地地区编号或自由填写编码替代。
                </AlertDescription>
            </Alert>
            <div className="space-y-1 rounded-lg border p-3 text-body-compact">
                <p className="font-medium">本单收货地址</p>
                <p className="text-muted-foreground">
                    {addressSummary || "请按原采购单权限核对完整地址。"}
                </p>
                <p className="text-xs text-muted-foreground">
                    如需更改收货地址或数量，先办理原采购变更。
                </p>
            </div>
            <div className="grid gap-4 sm:grid-cols-2">
                {fields.map(({ name, label, query, enabled }, index) => (
                    <form.Field key={name} name={name}>
                        {(field) => (
                            <div className="space-y-1.5">
                                <Label htmlFor={`${prefix}-${name}`}>
                                    {label}
                                </Label>
                                <OptionCombobox
                                    id={`${prefix}-${name}`}
                                    value={field.state.value || null}
                                    onValueChange={(value) => {
                                        field.handleChange(value ?? "")
                                        for (const lower of fieldNames.slice(
                                            index + 1,
                                        ))
                                            form.setFieldValue(lower, "")
                                        form.setFieldValue("confirmed", false)
                                    }}
                                    options={(query.data?.candidates ?? []).map(
                                        (candidate) => ({
                                            value: candidate.ticket,
                                            label: candidate.name,
                                        }),
                                    )}
                                    placeholder={
                                        !enabled
                                            ? "请先选择上一级区域"
                                            : query.isFetching
                                              ? "正在读取供应商候选…"
                                              : "请选择"
                                    }
                                    disabled={
                                        pending ||
                                        !enabled ||
                                        query.isFetching ||
                                        query.isError
                                    }
                                />
                                {enabled &&
                                query.isSuccess &&
                                query.data.candidates.length === 0 ? (
                                    <p className="text-xs text-warning-soft-foreground">
                                        未取得有效候选，请核对供应商地址权限和上级区域。
                                    </p>
                                ) : null}
                            </div>
                        )}
                    </form.Field>
                ))}
            </div>
            {setup.recipient_phone_required ? (
                <form.Field name="phone">
                    {(field) => (
                        <div className="space-y-1.5">
                            <Label htmlFor={`${prefix}-phone`}>
                                收货仓收件联系电话
                            </Label>
                            <Input
                                id={`${prefix}-phone`}
                                type="tel"
                                value={field.state.value}
                                onChange={(event) =>
                                    field.handleChange(event.target.value)
                                }
                                disabled={pending}
                            />
                        </div>
                    )}
                </form.Field>
            ) : null}
            {values.town ? (
                <form.Field name="confirmed">
                    {(field) => (
                        <div className="flex items-start gap-2">
                            <Checkbox
                                id={`${prefix}-confirmed`}
                                checked={field.state.value}
                                onCheckedChange={(checked) =>
                                    field.handleChange(checked === true)
                                }
                                disabled={pending}
                            />
                            <Label
                                htmlFor={`${prefix}-confirmed`}
                                className="leading-6"
                            >
                                已核对完整收货地址，对应区域为：
                                {selectedSummary}
                            </Label>
                        </div>
                    )}
                </form.Field>
            ) : null}
            {queryError ? (
                <Alert variant="warning">
                    <AlertDescription>
                        <p>{getErrorMessage(queryError.error)}</p>
                        <Button
                            id={`${prefix}-retry-candidates`}
                            type="button"
                            variant="link"
                            size="sm"
                            disabled={queryError.isFetching}
                            onClick={() => void queryError.refetch()}
                        >
                            重新读取区域候选
                        </Button>
                    </AlertDescription>
                </Alert>
            ) : null}
            <FormValidationSummary form={form} />
            {error ? (
                <Alert variant="destructive">
                    <AlertDescription>
                        {getErrorMessage(error)}
                        。请重新读取资料并核对原保存结果。
                    </AlertDescription>
                </Alert>
            ) : null}
            <div className="flex flex-wrap justify-end gap-2 border-t pt-4">
                <Button
                    id={`${prefix}-cancel`}
                    type="button"
                    variant="outline"
                    disabled={pending}
                    onClick={onCancel}
                >
                    取消
                </Button>
                <form.AppForm>
                    <form.SubmitButton
                        id={`${prefix}-submit`}
                        label="确认地址并继续核验"
                        loading={pending}
                        disabled={
                            pending || !values.town || Boolean(queryError)
                        }
                    />
                </form.AppForm>
            </div>
        </form>
    )
}
