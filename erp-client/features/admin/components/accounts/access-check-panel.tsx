"use client"

import * as React from "react"
import { XIcon } from "lucide-react"
import { z } from "zod"

import {
    BusinessFailureState,
    DetailFormGrid,
    DetailWorkspaceSection,
    OptionCombobox,
    detailWorkspaceStyles,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Field, FieldLabel } from "@/components/ui/field"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { SalesOrderSearchCombobox } from "@/features/entity-selectors/components/sales-order-search-combobox"
import { registeredResources } from "@/features/organization/lib/scope-payload"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { hasPermission } from "@/lib/permissions"
import type { AccessCheckInput } from "../../api/access-check"
import { useAccessCheck } from "../../hooks/use-access-check"
import { AccessCheckResult } from "./access-check-result"

export function AccessCheckPanel({ accountId }: { accountId: string }) {
    const [input, setInput] = React.useState<AccessCheckInput | null>(null)
    const profile = useAccountProfileQuery()
    const canListOrders = hasPermission(
        profile.data?.permissions,
        "sales_order:list",
    )
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
                    canListOrders &&
                    value.resource === "sales_order" &&
                    ["detail", "update"].includes(value.action)
                        ? value.objectId || null
                        : null,
            }
            if (JSON.stringify(input) === JSON.stringify(next)) {
                await query.refetch()
            } else {
                setInput(next)
            }
        },
    })

    return (
        <DetailWorkspaceSection
            title="访问检查"
            headingId="account-access-check-title"
            width="form"
        >
            <form
                onSubmit={(event) => {
                    event.preventDefault()
                    void form.handleSubmit()
                }}
            >
                <DetailFormGrid className="gap-x-4">
                    <div className="sm:col-span-5">
                        <form.AppField name="resource">
                            {(field) => (
                                <Field>
                                    <FieldLabel htmlFor="account-check-resource">
                                        业务模块
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
                                                    (row) =>
                                                        row.resource ===
                                                        resource,
                                                )?.actions[0] ?? "",
                                            )
                                            form.setFieldValue("objectId", "")
                                            setInput(null)
                                        }}
                                    />
                                </Field>
                            )}
                        </form.AppField>
                    </div>

                    <div className="sm:col-span-4">
                        <form.Subscribe
                            selector={(state) => state.values.resource}
                        >
                            {(resource) => (
                                <form.AppField name="action">
                                    {(field) => (
                                        <Field>
                                            <FieldLabel htmlFor="account-check-action">
                                                检查操作
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
                                                        ?.actions.map(
                                                            (action) => ({
                                                                value: action,
                                                                label: actionLabel(
                                                                    action,
                                                                ),
                                                            }),
                                                        ) ?? []
                                                }
                                                placeholder="选择操作"
                                                onBlur={field.handleBlur}
                                                onValueChange={(action) => {
                                                    if (!action) return
                                                    field.handleChange(action)
                                                    form.setFieldValue(
                                                        "objectId",
                                                        "",
                                                    )
                                                    setInput(null)
                                                }}
                                            />
                                        </Field>
                                    )}
                                </form.AppField>
                            )}
                        </form.Subscribe>
                    </div>

                    <div className="flex items-end sm:col-span-3">
                        <form.AppForm>
                            <form.SubmitButton
                                loading={query.isFetching}
                                id="account-check-submit"
                                label="开始检查"
                                disabled={query.isFetching}
                                className="w-full"
                            />
                        </form.AppForm>
                    </div>

                    <form.Subscribe selector={(state) => state.values}>
                        {(value) =>
                            canListOrders &&
                            value.resource === "sales_order" &&
                            ["detail", "update"].includes(value.action) ? (
                                <div className="sm:col-span-12">
                                    <form.AppField name="objectId">
                                        {(field) => (
                                            <Field>
                                                <div className="flex flex-wrap items-center justify-between gap-2">
                                                    <FieldLabel htmlFor="account-check-order">
                                                        要检查的销售单（可选）
                                                    </FieldLabel>
                                                    {field.state.value ? (
                                                        <Button
                                                            id="account-check-order-clear"
                                                            type="button"
                                                            variant="ghost"
                                                            size="xs"
                                                            className={
                                                                detailWorkspaceStyles.quietButton
                                                            }
                                                            onClick={() => {
                                                                field.handleChange(
                                                                    "",
                                                                )
                                                                setInput(null)
                                                            }}
                                                        >
                                                            <XIcon
                                                                data-icon="inline-start"
                                                                aria-hidden="true"
                                                            />
                                                            清除选择，改查权限配置
                                                        </Button>
                                                    ) : null}
                                                </div>
                                                <SalesOrderSearchCombobox
                                                    id="account-check-order"
                                                    value={
                                                        field.state.value ||
                                                        undefined
                                                    }
                                                    onValueChange={(next) => {
                                                        field.handleChange(
                                                            next ?? "",
                                                        )
                                                        setInput(null)
                                                    }}
                                                />
                                            </Field>
                                        )}
                                    </form.AppField>
                                </div>
                            ) : null
                        }
                    </form.Subscribe>
                </DetailFormGrid>
            </form>

            {query.isFetching ? (
                <div
                    role="status"
                    className="space-y-3 border-t border-border/60 pt-5"
                    aria-busy="true"
                >
                    <p className="text-xs text-muted-foreground">
                        正在逐层核对操作权限与数据范围…
                    </p>
                    <div className="h-16 animate-pulse rounded-lg bg-muted/70" />
                    <div className="h-24 animate-pulse rounded-lg bg-muted/50" />
                </div>
            ) : query.isError ? (
                <div className="border-t border-border/60 pt-5">
                    <BusinessFailureState
                        error={query.error}
                        onRetry={() => void query.refetch()}
                        id="account-check-retry"
                    />
                </div>
            ) : input && query.data ? (
                <AccessCheckResult input={input} result={query.data} />
            ) : null}
        </DetailWorkspaceSection>
    )
}
