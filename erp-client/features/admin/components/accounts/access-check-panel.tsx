"use client"

import * as React from "react"
import { SearchCheckIcon, XIcon } from "lucide-react"
import { z } from "zod"

import {
    BusinessFailureState,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { OptionCombobox } from "@/components/business/option-combobox"
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

const CHECK_LAYERS = [
    {
        step: "01",
        title: "功能操作权限",
        description: "核对已分配角色是否包含对应业务操作权限。",
    },
    {
        step: "02",
        title: "数据范围规则",
        description: "结合所属部门与个人追加范围，确认可访问边界。",
    },
    {
        step: "03",
        title: "单据归属与状态",
        description: "指定具体销售单时，进一步核对单据归属与当前状态。",
    },
] as const

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
            subtitle={
                canListOrders
                    ? "模拟核对此人能否执行所选操作；未选单据时检查权限配置，选择销售单后可核对具体单据权限。"
                    : "模拟核对此人能否执行所选业务操作，快速定位权限或数据范围阻断原因。"
            }
            width="form"
        >
            <form
                data-slot="detail-form-grid"
                className="grid gap-x-4 gap-y-4 sm:grid-cols-12"
                onSubmit={(event) => {
                    event.preventDefault()
                    void form.handleSubmit()
                }}
            >
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
                                                    row.resource === resource,
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
                    <form.Subscribe selector={(state) => state.values.resource}>
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
                                                    ?.actions.map((action) => ({
                                                        value: action,
                                                        label: actionLabel(
                                                            action,
                                                        ),
                                                    })) ?? []
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
                                            <p className="text-xs text-muted-foreground">
                                                此处列出的是你有权查看的销售单；选中后将核对此人对该单据的实际访问权限。
                                            </p>
                                        </Field>
                                    )}
                                </form.AppField>
                            </div>
                        ) : null
                    }
                </form.Subscribe>
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
            ) : (
                <div className="space-y-3 border-t border-border/60 pt-5">
                    <div className="flex items-center gap-2 text-xs font-medium text-muted-foreground">
                        <SearchCheckIcon
                            className="size-3.5"
                            aria-hidden="true"
                        />
                        <span>检查链路说明</span>
                    </div>
                    <ol className="grid gap-4 sm:grid-cols-3">
                        {CHECK_LAYERS.map((layer) => (
                            <li
                                key={layer.step}
                                className="space-y-1 border-l border-border/80 pl-3"
                            >
                                <p className="flex items-center gap-1.5 font-medium text-foreground">
                                    <span className="num text-xs text-primary">
                                        {layer.step}
                                    </span>
                                    <span>{layer.title}</span>
                                </p>
                                <p className="text-xs text-muted-foreground">
                                    {layer.description}
                                </p>
                            </li>
                        ))}
                    </ol>
                </div>
            )}
        </DetailWorkspaceSection>
    )
}
