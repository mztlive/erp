"use client"

import * as React from "react"
import { z } from "zod"
import { useAppForm, toFieldErrors } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Alert, AlertDescription } from "@/components/ui/alert"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog"
import { MultiOptionCombobox } from "@/components/business/multi-option-combobox"
import { Field, FieldLabel, FieldError } from "@/components/ui/field"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { hasPermission } from "@/lib/permissions"
import { getErrorMessage } from "@/lib/api/errors"
import {
    registeredResources,
    validateCreateDataScope,
} from "../lib/scope-payload"
import type {
    CreateDataScopeInput,
    OrganizationStateView,
    ScopeDimension,
} from "../types"
import { ScopeTargetPicker } from "./scope-target-picker"

const schema = z.object({
    subjectType: z.enum(["role", "user"]),
    subjectId: z.string().min(1, "请选择角色或人员"),
    resource: z.string().min(1, "请选择业务"),
    actions: z.array(z.string()).min(1, "请选择操作"),
    range: z.enum([
        "self_owned",
        "own_org",
        "managed_orgs",
        "explicit",
        "company",
        "collaborative",
    ]),
    dimension: z.enum(["internal_org", "warehouse", "settlement_party"]),
    descendants: z.boolean(),
    targets: z.array(z.string()),
})

export function DataScopeFormDialog({
    open,
    onOpenChange,
    roles,
    people,
    units,
    submitting,
    onSubmit,
    subject,
    embedded = false,
    initialResource = "",
    initialActions = [],
    permissions,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    roles: OrganizationStateView["roles"]
    people: OrganizationStateView["people"]
    units: OrganizationStateView["units"]
    submitting: boolean
    onSubmit: (input: CreateDataScopeInput) => Promise<void>
    subject?: { type: "role" | "user"; id: string; label: string }
    embedded?: boolean
    initialResource?: string
    initialActions?: string[]
    permissions?: readonly string[]
}) {
    const [error, setError] = React.useState<string | null>(null)
    const resources = registeredResources()
    const initialKey = initialActions.join("|")
    const defaults = React.useMemo(
        () =>
            ({
                subjectType: subject?.type ?? "role",
                subjectId: subject?.id ?? "",
                resource: initialResource,
                actions: initialKey ? initialKey.split("|") : [],
                range:
                    initialResource &&
                    registeredResources().find(
                        (item) => item.resource === initialResource,
                    )?.dimensions[0] !== "internal_org"
                        ? "explicit"
                        : "self_owned",
                dimension:
                    registeredResources().find(
                        (item) => item.resource === initialResource,
                    )?.dimensions[0] ?? "internal_org",
                descendants: false,
                targets: [],
            }) as z.infer<typeof schema>,
        [subject?.type, subject?.id, initialResource, initialKey],
    )
    const form = useAppForm({
        defaultValues: defaults,
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            const targeted = ["explicit", "own_org", "managed_orgs"].includes(
                value.range,
            )
            const input: CreateDataScopeInput = {
                subjectType: value.subjectType,
                subjectId: value.subjectId,
                resource: value.resource,
                actions: value.actions,
                scopeType: targeted
                    ? "organization"
                    : (value.range as
                          | "company"
                          | "self_owned"
                          | "collaborative"),
                targetDimension: value.dimension,
                targetMode: targeted
                    ? (value.range as "explicit" | "own_org" | "managed_orgs")
                    : null,
                includeDescendants:
                    targeted &&
                    value.dimension === "internal_org" &&
                    value.range !== "managed_orgs"
                        ? value.descendants
                        : null,
                scopeTargets: value.range === "explicit" ? value.targets : [],
            }
            const invalid = validateCreateDataScope(input)
            if (invalid) {
                setError(invalid)
                return
            }
            try {
                setError(null)
                await onSubmit(input)
                onOpenChange(false)
            } catch (failure) {
                setError(getErrorMessage(failure, "保存失败，请重试"))
            }
        },
    })
    React.useEffect(() => {
        if (open) {
            form.reset(defaults)
            setError(null)
        }
    }, [open, defaults, form])
    const content = (
        <>
            {embedded ? (
                <h3 className="font-medium">添加操作适用范围</h3>
            ) : (
                <DialogHeader>
                    <DialogTitle>添加操作适用范围</DialogTitle>
                    <DialogDescription>
                        选择业务、操作以及允许访问的数据。个人限制只能收窄已有授权。
                    </DialogDescription>
                </DialogHeader>
            )}
            <form
                className="space-y-4"
                onSubmit={(event) => {
                    event.preventDefault()
                    void form.handleSubmit()
                }}
            >
                {subject ? (
                    <p className="text-sm">
                        {subject.type === "role" ? "角色" : "个人限制"}：
                        {subject.label}
                    </p>
                ) : (
                    <>
                        <form.AppField name="subjectType">
                            {(field) => (
                                <field.SelectField
                                    id="organization-scope-subject-type"
                                    label="配置对象"
                                    allowClear={false}
                                    onValueChange={() =>
                                        form.setFieldValue("subjectId", "")
                                    }
                                    options={[
                                        { value: "role", label: "角色授权" },
                                        {
                                            value: "user",
                                            label: "个人范围限制（只能收窄）",
                                        },
                                    ]}
                                />
                            )}
                        </form.AppField>
                        <form.Subscribe
                            selector={(state) => state.values.subjectType}
                        >
                            {(type) => (
                                <form.AppField name="subjectId">
                                    {(field) => (
                                        <field.SelectField
                                            id="organization-scope-subject"
                                            label="角色或人员"
                                            options={
                                                type === "role"
                                                    ? roles.map((role) => ({
                                                          value: role.id,
                                                          label: role.name,
                                                      }))
                                                    : people.map((person) => ({
                                                          value: person.id,
                                                          label: `${person.label}（${person.account}）`,
                                                      }))
                                            }
                                        />
                                    )}
                                </form.AppField>
                            )}
                        </form.Subscribe>
                    </>
                )}
                <form.AppField name="resource">
                    {(field) => (
                        <Field>
                            <FieldLabel htmlFor="organization-scope-resource">
                                业务
                            </FieldLabel>
                            <select
                                id="organization-scope-resource"
                                className="h-10 w-full rounded-md border bg-background px-3"
                                value={field.state.value}
                                onChange={(event) => {
                                    const resource = event.target.value
                                    field.handleChange(resource)
                                    form.setFieldValue("actions", [])
                                    const dimension =
                                        resources.find(
                                            (item) =>
                                                item.resource === resource,
                                        )?.dimensions[0] ?? "internal_org"
                                    form.setFieldValue("dimension", dimension)
                                    form.setFieldValue(
                                        "range",
                                        dimension === "internal_org"
                                            ? "self_owned"
                                            : "explicit",
                                    )
                                    form.setFieldValue("targets", [])
                                }}
                            >
                                <option value="">请选择业务</option>
                                {resources
                                    .filter(
                                        (item) =>
                                            !permissions ||
                                            item.actions.some((action) =>
                                                hasPermission(
                                                    permissions,
                                                    `${item.resource}:${action}`,
                                                ),
                                            ),
                                    )
                                    .map((item) => (
                                        <option
                                            key={item.resource}
                                            value={item.resource}
                                        >
                                            {resourceLabel(item.resource)}
                                        </option>
                                    ))}
                            </select>
                            <FieldError
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        </Field>
                    )}
                </form.AppField>
                <form.Subscribe selector={(state) => state.values.resource}>
                    {(resource) => (
                        <form.AppField name="actions">
                            {(field) => (
                                <Field>
                                    <FieldLabel htmlFor="organization-scope-actions">
                                        允许的操作
                                    </FieldLabel>
                                    <MultiOptionCombobox
                                        id="organization-scope-actions"
                                        aria-label="允许的操作"
                                        value={field.state.value}
                                        onValueChange={field.handleChange}
                                        options={(
                                            resources.find(
                                                (item) =>
                                                    item.resource === resource,
                                            )?.actions ?? []
                                        )
                                            .filter(
                                                (action) =>
                                                    !permissions ||
                                                    hasPermission(
                                                        permissions,
                                                        `${resource}:${action}`,
                                                    ),
                                            )
                                            .map((action) => ({
                                                value: action,
                                                label: actionLabel(action),
                                            }))}
                                        placeholder="选择已授予的操作"
                                    />
                                    <FieldError
                                        errors={toFieldErrors(
                                            field.state.meta.errors,
                                        )}
                                    />
                                </Field>
                            )}
                        </form.AppField>
                    )}
                </form.Subscribe>
                <form.Subscribe selector={(state) => state.values.resource}>
                    {(resource) => {
                        const dimensions = resources.find(
                            (item) => item.resource === resource,
                        )?.dimensions ?? ["internal_org"]
                        return dimensions.length > 1 ? (
                            <form.AppField name="dimension">
                                {(field) => (
                                    <Field>
                                        <FieldLabel htmlFor="organization-scope-dimension">
                                            本条规则限制什么
                                        </FieldLabel>
                                        <select
                                            id="organization-scope-dimension"
                                            className="h-10 w-full rounded-md border bg-background px-3"
                                            value={field.state.value}
                                            onChange={(event) => {
                                                field.handleChange(
                                                    event.target
                                                        .value as ScopeDimension,
                                                )
                                                form.setFieldValue(
                                                    "range",
                                                    event.target.value ===
                                                        "internal_org"
                                                        ? "self_owned"
                                                        : "explicit",
                                                )
                                                form.setFieldValue(
                                                    "targets",
                                                    [],
                                                )
                                            }}
                                        >
                                            {dimensions.map((dimension) => (
                                                <option
                                                    key={dimension}
                                                    value={dimension}
                                                >
                                                    {dimension ===
                                                    "internal_org"
                                                        ? "负责人及部门"
                                                        : dimension ===
                                                            "warehouse"
                                                          ? "允许的仓库"
                                                          : "允许的结算主体"}
                                                </option>
                                            ))}
                                        </select>
                                        <p className="text-xs text-muted-foreground">
                                            本业务需要同时满足各项范围；可以分条配置，公司范围覆盖适用维度。
                                        </p>
                                    </Field>
                                )}
                            </form.AppField>
                        ) : null
                    }}
                </form.Subscribe>
                <form.Subscribe
                    selector={(state) => ({
                        dimension: state.values.dimension,
                        resource: state.values.resource,
                    })}
                >
                    {({ dimension, resource }) => (
                        <form.AppField name="range">
                            {(field) => (
                                <field.SelectField
                                    id="organization-scope-type"
                                    label="允许访问哪些数据"
                                    options={[
                                        ...(dimension === "internal_org"
                                            ? [
                                                  {
                                                      value: "self_owned",
                                                      label: "本人负责",
                                                  },
                                                  {
                                                      value: "own_org",
                                                      label: "本人所属部门",
                                                  },
                                                  {
                                                      value: "managed_orgs",
                                                      label: "本人管理的部门",
                                                  },
                                              ]
                                            : []),
                                        {
                                            value: "explicit",
                                            label:
                                                dimension === "internal_org"
                                                    ? "指定部门"
                                                    : dimension === "warehouse"
                                                      ? "指定仓库"
                                                      : "指定结算主体",
                                        },
                                        { value: "company", label: "公司范围" },
                                        ...(dimension === "internal_org" &&
                                        ![
                                            "customer",
                                            "contract",
                                            "sales_order",
                                            "sales_person",
                                            "procurement_person",
                                            "business_person",
                                            "person_query_qualification",
                                        ].includes(resource)
                                            ? [
                                                  {
                                                      value: "collaborative",
                                                      label: "符合业务规则的协作参与",
                                                  },
                                              ]
                                            : []),
                                    ]}
                                />
                            )}
                        </form.AppField>
                    )}
                </form.Subscribe>
                <form.Subscribe selector={(state) => state.values}>
                    {(value) => (
                        <>
                            {value.range === "explicit" && (
                                <form.AppField name="targets">
                                    {(field) => (
                                        <Field>
                                            <FieldLabel htmlFor="organization-scope-targets">
                                                允许的目标
                                            </FieldLabel>
                                            <ScopeTargetPicker
                                                dimension={value.dimension}
                                                units={units}
                                                value={field.state.value}
                                                onChange={field.handleChange}
                                            />
                                        </Field>
                                    )}
                                </form.AppField>
                            )}
                            {value.dimension === "internal_org" &&
                                ["explicit", "own_org"].includes(
                                    value.range,
                                ) && (
                                    <form.AppField name="descendants">
                                        {(field) => (
                                            <label
                                                className="flex items-center gap-2 text-sm"
                                                htmlFor="organization-scope-descendants"
                                            >
                                                <input
                                                    id="organization-scope-descendants"
                                                    type="checkbox"
                                                    checked={field.state.value}
                                                    onChange={(event) =>
                                                        field.handleChange(
                                                            event.target
                                                                .checked,
                                                        )
                                                    }
                                                />
                                                包含下级部门
                                            </label>
                                        )}
                                    </form.AppField>
                                )}
                            {value.range === "managed_orgs" && (
                                <p className="rounded-md bg-muted p-3 text-sm">
                                    每位使用该角色的人员还需设置管理部门；下级范围沿用对应管理关系。保存本规则不会自动建立管理关系。
                                </p>
                            )}
                            {value.range === "company" && (
                                <p className="text-sm text-amber-700">
                                    公司范围会扩大所选操作的适用数据。新增较窄规则不会覆盖已有的公司范围。
                                </p>
                            )}
                        </>
                    )}
                </form.Subscribe>
                {error && (
                    <Alert variant="destructive">
                        <AlertDescription>{error}</AlertDescription>
                    </Alert>
                )}
                <div className="flex justify-end gap-2">
                    <Button
                        id="organization-scope-cancel"
                        type="button"
                        variant="outline"
                        disabled={submitting}
                        onClick={() => onOpenChange(false)}
                    >
                        取消
                    </Button>
                    <form.AppForm>
                        <form.SubmitButton
                            id="organization-scope-submit"
                            label="保存范围"
                            disabled={submitting}
                        />
                    </form.AppForm>
                </div>
            </form>
        </>
    )
    return embedded ? (
        content
    ) : (
        <Dialog open={open} onOpenChange={onOpenChange}>
            <DialogContent
                className="max-h-[88vh] w-[calc(100vw-1.5rem)] overflow-x-hidden overflow-y-auto sm:max-w-xl"
                closeButtonId="organization-scope-close"
            >
                {content}
            </DialogContent>
        </Dialog>
    )
}
