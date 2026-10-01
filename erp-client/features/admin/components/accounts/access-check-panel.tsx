"use client"
import * as React from "react"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { OptionCombobox } from "@/components/business/option-combobox"
import { BusinessFailureState } from "@/components/business"
import { Button } from "@/components/ui/button"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { registeredResources } from "@/features/organization/lib/scope-payload"
import { SalesOrderSearchCombobox } from "@/features/entity-selectors/components/sales-order-search-combobox"
import { Field, FieldLabel } from "@/components/ui/field"
import { useAccessCheck } from "../../hooks/use-access-check"
import type { AccessCheckInput } from "../../api/access-check"
import { AccessCheckResult } from "./access-check-result"

export function AccessCheckPanel({ accountId }: { accountId: string }) {
    const [input, setInput] = React.useState<AccessCheckInput | null>(null)
    const query = useAccessCheck(input)
    const resources = registeredResources()
    const form = useAppForm({
        defaultValues: {
            resource: "sales_order",
            action: "detail",
            objectId: "",
        },
        validators: {
            onChange: z.object({
                resource: z.string().min(1),
                action: z.string().min(1),
                objectId: z.string(),
            }),
        },
        onSubmit: async ({ value }) => {
            const next = {
                user_id: accountId,
                resource: value.resource,
                action: value.action,
                object_id:
                    value.resource === "sales_order" &&
                    ["detail", "update"].includes(value.action)
                        ? value.objectId || null
                        : null,
            }
            if (JSON.stringify(input) === JSON.stringify(next))
                await query.refetch()
            else setInput(next)
        },
    })
    return (
        <section
            className="space-y-4 text-sm"
            aria-labelledby="account-access-check-title"
        >
            <h2 id="account-access-check-title" className="font-semibold">
                检查访问权限
            </h2>
            <p className="text-sm text-muted-foreground">
                检查此人能否执行所选操作。未选择单据时，只检查权限配置；选择销售单后，可进一步确认此人能否查看或修改这张单据。
            </p>
            <form
                className="space-y-3"
                onSubmit={(event) => {
                    event.preventDefault()
                    void form.handleSubmit()
                }}
            >
                <form.AppField name="resource">
                    {(field) => (
                        <Field>
                            <FieldLabel htmlFor="account-check-resource">
                                业务
                            </FieldLabel>
                            <OptionCombobox
                                id="account-check-resource"
                                aria-label="业务"
                                value={field.state.value}
                                allowClear={false}
                                options={resources.map((row) => ({
                                    value: row.resource,
                                    label: resourceLabel(row.resource),
                                }))}
                                placeholder="选择业务"
                                onBlur={field.handleBlur}
                                onValueChange={(resource) => {
                                    if (!resource) return
                                    field.handleChange(resource)
                                    form.setFieldValue(
                                        "action",
                                        resources.find(
                                            (row) => row.resource === resource,
                                        )?.actions[0] ?? "",
                                    )
                                    form.setFieldValue("objectId", "")
                                    setInput(null)
                                }}
                            />
                        </Field>
                    )}
                </form.AppField>
                <form.Subscribe selector={(state) => state.values.resource}>
                    {(resource) => (
                        <form.AppField name="action">
                            {(field) => (
                                <Field>
                                    <FieldLabel htmlFor="account-check-action">
                                        操作
                                    </FieldLabel>
                                    <OptionCombobox
                                        id="account-check-action"
                                        aria-label="操作"
                                        value={field.state.value}
                                        allowClear={false}
                                        options={
                                            resources
                                                .find(
                                                    (row) =>
                                                        row.resource ===
                                                        resource,
                                                )
                                                ?.actions.map((action) => ({
                                                    value: action,
                                                    label: actionLabel(action),
                                                })) ?? []
                                        }
                                        placeholder="选择操作"
                                        onBlur={field.handleBlur}
                                        onValueChange={(action) => {
                                            if (!action) return
                                            field.handleChange(action)
                                            form.setFieldValue("objectId", "")
                                            setInput(null)
                                        }}
                                    />
                                </Field>
                            )}
                        </form.AppField>
                    )}
                </form.Subscribe>
                <form.Subscribe selector={(state) => state.values}>
                    {(value) =>
                        value.resource === "sales_order" &&
                        ["detail", "update"].includes(value.action) ? (
                            <form.AppField name="objectId">
                                {(field) => (
                                    <Field>
                                        <FieldLabel htmlFor="account-check-order">
                                            要检查的销售单（可选）
                                        </FieldLabel>
                                        <SalesOrderSearchCombobox
                                            id="account-check-order"
                                            value={
                                                field.state.value || undefined
                                            }
                                            onValueChange={(value) => {
                                                field.handleChange(value ?? "")
                                                setInput(null)
                                            }}
                                        />
                                        <p className="text-xs text-muted-foreground">
                                            这里列出的是你能查看的单据。选中后，检查的是此人对该单据的权限。
                                        </p>
                                        {field.state.value ? (
                                            <Button
                                                id="account-check-order-clear"
                                                type="button"
                                                variant="ghost"
                                                onClick={() => {
                                                    field.handleChange("")
                                                    setInput(null)
                                                }}
                                            >
                                                清除选择，改查权限配置
                                            </Button>
                                        ) : null}
                                    </Field>
                                )}
                            </form.AppField>
                        ) : null
                    }
                </form.Subscribe>
                <form.AppForm>
                    <form.SubmitButton
                        id="account-check-submit"
                        label="开始检查"
                        disabled={query.isFetching}
                    />
                </form.AppForm>
            </form>
            {query.isFetching ? (
                <p role="status">正在检查当前权限…</p>
            ) : query.isError ? (
                <BusinessFailureState
                    error={query.error}
                    onRetry={() => void query.refetch()}
                    id="account-check-retry"
                />
            ) : input && query.data ? (
                <AccessCheckResult input={input} result={query.data} />
            ) : null}
        </section>
    )
}
