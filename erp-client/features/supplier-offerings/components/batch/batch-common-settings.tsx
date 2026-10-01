"use client"
import * as React from "react"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Alert, AlertDescription } from "@/components/ui/alert"
import { OptionCombobox } from "@/components/business/option-combobox"
import type { BatchEditor } from "../../hooks/use-batch-supply-editor"
import { AVAILABILITY_STATUS_LABELS } from "../../types"
import type { TextFieldKey } from "../../lib/batch-supply"

const availabilityOptions = [
    { value: "keep", label: "保持每行状态" },
    ...Object.entries(AVAILABILITY_STATUS_LABELS).map(([value, label]) => ({
        value,
        label,
    })),
]

export function BatchCommonSettings({ editor }: { editor: BatchEditor }) {
    const [confirmOverwrite, setConfirmOverwrite] = React.useState(false)
    const fields: { key: TextFieldKey; label: string }[] =
        editor.mode === "availability"
            ? [{ key: "availableQuantity", label: "可供数量" }]
            : [
                  { key: "inputTaxPercentage", label: "税率 %" },
                  { key: "minimumQuantity", label: "集采起订量" },
                  { key: "supplyRegionText", label: "可供区域" },
                  { key: "validFrom", label: "生效日期" },
                  { key: "validTo", label: "失效日期" },
                  { key: "dropshipPrice", label: "代发含税价" },
                  { key: "bulkPrice", label: "集采含税价" },
              ]
    return (
        <section
            className="space-y-3 rounded-xl border bg-muted/20 p-4"
            aria-label="公共设置"
        >
            <div className="flex flex-wrap items-baseline gap-2">
                <h3 className="font-semibold">公共设置</h3>
                <p className="text-xs text-muted-foreground">
                    仅应用到勾选的可编辑行；默认只补空白。单行仍可修改。
                </p>
            </div>
            <div className="grid grid-cols-2 gap-3 lg:grid-cols-4">
                {fields.map(({ key, label }) => (
                    <editor.form.AppField
                        key={key}
                        name={`common.${key}`}
                        listeners={{
                            onChange: ({ value }) => {
                                if (key === "availableQuantity") {
                                    editor.form.setFieldValue(
                                        "applyQuantity",
                                        true,
                                    )
                                    editor.form.setFieldValue(
                                        "common.quantityMode",
                                        value ? "provided" : "unknown",
                                    )
                                }
                            },
                        }}
                    >
                        {(field) => (
                            <field.TextField
                                id={`batch-supply-common-${toAutomationIdSegment(key)}`}
                                label={label}
                                type={
                                    key === "validFrom" || key === "validTo"
                                        ? "date"
                                        : "text"
                                }
                            />
                        )}
                    </editor.form.AppField>
                ))}
                {editor.mode === "availability" && (
                    <>
                        <editor.form.AppField name="common.availabilityStatus">
                            {(field) => (
                                <label
                                    className="space-y-1 text-xs"
                                    htmlFor="batch-supply-common-status"
                                >
                                    可供状态
                                    <OptionCombobox
                                        id="batch-supply-common-status"
                                        aria-label="公共可供状态"
                                        options={availabilityOptions}
                                        allowClear={false}
                                        disabled={editor.busy}
                                        value={
                                            editor.form.state.values
                                                .applyAvailabilityStatus
                                                ? field.state.value
                                                : "keep"
                                        }
                                        onValueChange={(value) => {
                                            const availabilityStatus =
                                                value === "keep" ? null : value
                                            editor.form.setFieldValue(
                                                "applyAvailabilityStatus",
                                                Boolean(availabilityStatus),
                                            )
                                            if (availabilityStatus)
                                                field.handleChange(
                                                    availabilityStatus as typeof field.state.value,
                                                )
                                        }}
                                    />
                                </label>
                            )}
                        </editor.form.AppField>
                        <editor.form.AppField name="common.quantityMode">
                            {(field) => (
                                <label
                                    className="flex items-center gap-2 text-sm"
                                    htmlFor="batch-supply-common-unknown"
                                >
                                    <Checkbox
                                        id="batch-supply-common-unknown"
                                        aria-label="数量未提供（覆盖时清空数量）"
                                        nativeButton
                                        render={
                                            <button
                                                type="button"
                                                aria-label="数量未提供（覆盖时清空数量）"
                                            />
                                        }
                                        disabled={editor.busy}
                                        checked={
                                            editor.form.state.values
                                                .applyQuantity &&
                                            field.state.value === "unknown"
                                        }
                                        onCheckedChange={(checked) => {
                                            editor.form.setFieldValue(
                                                "applyQuantity",
                                                checked,
                                            )
                                            if (checked)
                                                editor.form.setFieldValue(
                                                    "common.availableQuantity",
                                                    "",
                                                )
                                            field.handleChange(
                                                checked
                                                    ? "unknown"
                                                    : "provided",
                                            )
                                        }}
                                    />
                                    数量未提供（覆盖时清空数量）
                                </label>
                            )}
                        </editor.form.AppField>
                    </>
                )}
            </div>
            <div className="flex flex-wrap items-center gap-2">
                <Button
                    id="batch-supply-fill-empty"
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={editor.busy}
                    onClick={() => editor.applyCommon(false)}
                >
                    仅补齐空白
                </Button>
                <Button
                    id="batch-supply-overwrite"
                    type="button"
                    size="sm"
                    variant="ghost"
                    disabled={editor.busy}
                    onClick={() => setConfirmOverwrite(true)}
                >
                    覆盖勾选行…
                </Button>
                {editor.mode !== "availability" && (
                    <Button
                        id="batch-supply-same-prices"
                        type="button"
                        size="sm"
                        variant="ghost"
                        disabled={editor.busy}
                        onClick={editor.samePrice}
                    >
                        勾选行两种价格相同
                    </Button>
                )}
            </div>
            {confirmOverwrite && (
                <Alert variant="warning" role="alert">
                    <AlertDescription>
                        将用已填写的公共设置替换勾选行现有值
                        {editor.mode === "availability"
                            ? "，包括状态和数量"
                            : ""}
                        。确认后请重新校验。
                    </AlertDescription>
                    <div className="mt-3 flex flex-wrap gap-2">
                        <Button
                            id="batch-supply-overwrite-confirm"
                            type="button"
                            size="sm"
                            onClick={() => {
                                editor.applyCommon(true)
                                setConfirmOverwrite(false)
                            }}
                        >
                            确认覆盖
                        </Button>
                        <Button
                            id="batch-supply-overwrite-cancel"
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => setConfirmOverwrite(false)}
                        >
                            取消
                        </Button>
                    </div>
                </Alert>
            )}
        </section>
    )
}
