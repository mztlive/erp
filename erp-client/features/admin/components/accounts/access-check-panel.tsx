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
                由服务端检查当前账号、角色与实际访问政策。销售单查看和修改可选择具体单据；来源继承与任务指派需要对应业务上下文，未核对具体对象时不会判定可处理。结果不代替业务执行时校验。检查他人权限需要公司范围的组织读取及账号、角色、数据范围读取权限。
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
                                            销售单（可选，仅列出你可查看的单据）
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
                                        <Button
                                            id="account-check-order-clear"
                                            type="button"
                                            variant="ghost"
                                            onClick={() => {
                                                field.handleChange("")
                                                setInput(null)
                                            }}
                                        >
                                            清除单据，仅检查配置
                                        </Button>
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
                <div role="status" className="space-y-3">
                    {query.data.steps.map((step, index) => (
                        <div
                            key={`${step.layer}-${index}`}
                            className="border-l-2 pl-3"
                        >
                            <p className="text-sm font-medium">
                                {step.layer} ·{" "}
                                {step.status === "passed"
                                    ? "已通过"
                                    : step.status === "blocked"
                                      ? "未通过"
                                      : "需要继续核对"}
                            </p>
                            <p className="text-sm text-muted-foreground">
                                {step.message}
                            </p>
                        </div>
                    ))}
                    <p className="text-xs text-muted-foreground">
                        配置或业务数据变化后，请重新检查。
                    </p>
                </div>
            ) : null}
        </section>
    )
}
