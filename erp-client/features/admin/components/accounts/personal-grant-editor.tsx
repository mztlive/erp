"use client"
import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { OptionCombobox } from "@/components/business/option-combobox"
import { ScopeTargetPicker } from "@/features/organization/components/scope-target-picker"
import type { OrgUnit } from "@/features/organization/types"
import { actionLabel } from "@/lib/permission-catalog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    personGrantKey,
    type PersonGrantEditorInput,
    type PersonScopeBusiness,
    type PersonScopeGrant,
    type PersonScopeTerm,
} from "../../api/person-data-scopes"
import { scopeDimensionLabel } from "./personal-scope-summary"

export function grantEditorDefaults(
    business: PersonScopeBusiness,
    grant?: PersonScopeGrant,
): PersonGrantEditorInput {
    const terms = grant?.terms ?? []
    const department = terms.find(
        (term) => term.target_dimension === "internal_org",
    )
    return {
        key: grant?.key ?? null,
        actions: grant?.actions ?? business.actions,
        dimension: terms[0]?.target_dimension ?? business.dimensions[0],
        mode: terms.some((term) => term.scope_type === "company")
            ? "company"
            : department?.scope_type === "self_owned"
              ? "self"
              : department?.target_mode === "own_org"
                ? "own_org"
                : "explicit",
        org_ids: department?.scope_targets ?? [],
        warehouse_ids:
            terms.find((term) => term.target_dimension === "warehouse")
                ?.scope_targets ?? [],
        settlement_ids:
            terms.find((term) => term.target_dimension === "settlement_party")
                ?.scope_targets ?? [],
        include_descendants: department?.include_descendants ?? false,
    }
}

/** 仅编辑可无损回填的标准范围，旧协作等条件仍作为独立授权保留。 */
export function canEditGrant(
    grant: PersonScopeGrant,
    business: PersonScopeBusiness,
) {
    if (grant.terms.length === 1 && grant.terms[0].scope_type === "company")
        return true
    const dimensions = grant.terms.map((term) => term.target_dimension)
    return (
        grant.terms.every(
            (term) =>
                (term.scope_type === "organization" &&
                    ["explicit", "own_org"].includes(term.target_mode ?? "")) ||
                (!business.default_self &&
                    term.scope_type === "self_owned" &&
                    term.target_dimension === "internal_org"),
        ) &&
        new Set(dimensions).size === dimensions.length &&
        (business.resource === "approval_instance"
            ? dimensions.length === 1
            : business.dimensions.length === dimensions.length &&
              business.dimensions.every((dimension) =>
                  dimensions.includes(dimension),
              ))
    )
}

/** 每条授权内部保持交集；不同授权在保存和摘要中合并为并集。 */
function editorTerms(
    value: PersonGrantEditorInput,
    business: PersonScopeBusiness,
): PersonScopeTerm[] {
    const dimensions =
        business.resource === "approval_instance"
            ? [value.dimension]
            : business.dimensions
    if (value.mode === "company")
        return [
            {
                scope_type: "company",
                target_dimension: dimensions[0],
                target_mode: null,
                include_descendants: null,
                scope_targets: [],
            },
        ]
    return dimensions.map((dimension) => ({
        scope_type:
            dimension === "internal_org" && value.mode === "self"
                ? "self_owned"
                : "organization",
        target_dimension: dimension,
        target_mode:
            dimension === "internal_org" && value.mode === "self"
                ? null
                : dimension === "internal_org" && value.mode === "own_org"
                  ? "own_org"
                  : "explicit",
        include_descendants:
            dimension === "internal_org" && value.mode !== "self"
                ? value.include_descendants
                : null,
        scope_targets:
            dimension === "internal_org"
                ? value.mode !== "explicit"
                    ? []
                    : value.org_ids
                : dimension === "warehouse"
                  ? value.warehouse_ids
                  : value.settlement_ids,
    }))
}

export function PersonalGrantEditor({
    initial,
    business,
    units,
    name,
    onChange,
    onApply,
    onCancel,
}: {
    initial: PersonGrantEditorInput
    business: PersonScopeBusiness
    units: OrgUnit[]
    name: string
    onChange: (value: PersonGrantEditorInput) => void
    onApply: (grant: PersonScopeGrant) => void
    onCancel: () => void
}) {
    const schema = z
        .object({
            key: z.string().nullable(),
            actions: z.array(z.string()).min(1, "请选择此授权适用的操作"),
            dimension: z.enum([
                "internal_org",
                "warehouse",
                "settlement_party",
            ]),
            mode: z.enum(["self", "own_org", "explicit", "company"]),
            org_ids: z.array(z.string()),
            warehouse_ids: z.array(z.string()),
            settlement_ids: z.array(z.string()),
            include_descendants: z.boolean(),
        })
        .superRefine((value, context) => {
            if (
                value.actions.some(
                    (action) => !business.actions.includes(action),
                )
            )
                context.addIssue({
                    code: "custom",
                    message: "操作权限已变化，请刷新后核对",
                    path: ["actions"],
                })
            const dimensions =
                business.resource === "approval_instance"
                    ? [value.dimension]
                    : business.dimensions
            if (!business.dimensions.includes(value.dimension))
                context.addIssue({
                    code: "custom",
                    message: "请选择适用于当前业务的范围",
                    path: ["dimension"],
                })
            if (value.mode === "company") return
            if (
                ["self", "own_org"].includes(value.mode) &&
                !dimensions.includes("internal_org")
            )
                context.addIssue({
                    code: "custom",
                    message: "当前业务不能按所属部门授权",
                    path: ["mode"],
                })
            for (const dimension of dimensions) {
                const key =
                    dimension === "internal_org"
                        ? "org_ids"
                        : dimension === "warehouse"
                          ? "warehouse_ids"
                          : "settlement_ids"
                if (
                    (dimension !== "internal_org" ||
                        value.mode === "explicit") &&
                    !value[key].length
                )
                    context.addIssue({
                        code: "custom",
                        message: `请选择${scopeDimensionLabel(dimension)}`,
                        path: [key],
                    })
            }
        })
    const form = useAppForm({
        defaultValues: initial,
        validators: { onSubmit: schema },
        onSubmit: ({ value }) => {
            const terms = editorTerms(value, business)
            onApply({
                key: personGrantKey(terms),
                terms,
                actions: value.actions,
            })
        },
    })
    const value = useStore(form.store, (state) => state.values)
    React.useEffect(() => {
        onChange(value)
    }, [value, onChange])
    const dimensions =
        business.resource === "approval_instance"
            ? [value.dimension]
            : business.dimensions
    const validation = schema.safeParse(value)
    return (
        <section
            className="space-y-4 rounded-lg border border-primary/30 bg-muted/20 p-4"
            aria-label="编辑追加授权"
        >
            <h4 className="font-medium">
                {initial.key ? "修改追加授权" : "添加授权范围"}
            </h4>
            {business.resource === "approval_instance" && (
                <form.Field name="dimension">
                    {(field) => (
                        <div className="space-y-2">
                            <label htmlFor="person-scope-dimension">
                                按什么限定审批数据？
                            </label>
                            <OptionCombobox
                                id="person-scope-dimension"
                                aria-label="按什么限定审批数据？"
                                value={field.state.value}
                                allowClear={false}
                                options={business.dimensions.map(
                                    (dimension) => ({
                                        value: dimension,
                                        label: scopeDimensionLabel(dimension),
                                    }),
                                )}
                                onValueChange={(dimension) => {
                                    if (!dimension) return
                                    field.handleChange(
                                        dimension as PersonGrantEditorInput["dimension"],
                                    )
                                    form.setFieldValue("mode", "explicit")
                                }}
                            />
                        </div>
                    )}
                </form.Field>
            )}
            <form.Field name="mode">
                {(field) => (
                    <fieldset className="space-y-2">
                        <legend className="mb-2 font-medium">
                            追加哪些数据？
                        </legend>
                        {(
                            [
                                ...(dimensions.includes("internal_org") &&
                                !business.default_self
                                    ? [
                                          [
                                              "self",
                                              business.resource.endsWith(
                                                  "_person",
                                              )
                                                  ? "本人"
                                                  : "本人负责的业务",
                                          ],
                                      ]
                                    : []),
                                ...(dimensions.includes("internal_org")
                                    ? [["own_org", `${name}所属部门的数据`]]
                                    : []),
                                [
                                    "explicit",
                                    `指定${dimensions.map(scopeDimensionLabel).join("及")}`,
                                ],
                                ["company", "公司范围"],
                            ] as [PersonGrantEditorInput["mode"], string][]
                        ).map(([mode, label]) => (
                            <label
                                key={mode}
                                htmlFor={`person-scope-mode-${mode}`}
                                className="flex items-center gap-2"
                            >
                                <input
                                    id={`person-scope-mode-${mode}`}
                                    name="person-scope-mode"
                                    type="radio"
                                    checked={field.state.value === mode}
                                    onChange={() => field.handleChange(mode)}
                                />
                                {label}
                            </label>
                        ))}
                    </fieldset>
                )}
            </form.Field>
            {value.mode !== "company" &&
                dimensions.map((dimension) => {
                    if (
                        dimension === "internal_org" &&
                        value.mode !== "explicit"
                    )
                        return null
                    const key =
                        dimension === "internal_org"
                            ? "org_ids"
                            : dimension === "warehouse"
                              ? "warehouse_ids"
                              : "settlement_ids"
                    return (
                        <form.Field key={key} name={key}>
                            {(field) => (
                                <div className="space-y-2">
                                    <p>{scopeDimensionLabel(dimension)}</p>
                                    <ScopeTargetPicker
                                        id={`person-scope-target-${dimension}`}
                                        dimension={dimension}
                                        value={field.state.value}
                                        onChange={field.handleChange}
                                        units={units}
                                        noScopeLabel={`你当前登录的账号没有${scopeDimensionLabel(dimension)}目录的查看范围，暂时无法选择。请联系有权限的管理员完成配置；这不是${name}的权限提示。`}
                                    />
                                </div>
                            )}
                        </form.Field>
                    )
                })}
            {!["company", "self"].includes(value.mode) &&
                dimensions.includes("internal_org") && (
                    <form.Field name="include_descendants">
                        {(field) => (
                            <label
                                htmlFor="person-scope-descendants"
                                className="flex items-center gap-2"
                            >
                                <Checkbox
                                    id="person-scope-descendants"
                                    checked={field.state.value}
                                    onCheckedChange={(checked) =>
                                        field.handleChange(checked === true)
                                    }
                                />
                                包含下级部门
                            </label>
                        )}
                    </form.Field>
                )}
            {value.mode !== "company" && dimensions.length > 1 && (
                <p className="text-xs text-muted-foreground">
                    此条授权要求数据同时符合以上条件；其他组合请另加一条授权。
                </p>
            )}
            <form.Field name="actions">
                {(field) => (
                    <fieldset className="space-y-2">
                        <legend className="mb-2 font-medium">适用操作</legend>
                        <div className="flex flex-wrap gap-4">
                            {business.actions.map((action) => (
                                <label
                                    key={action}
                                    htmlFor={`person-scope-action-${toAutomationIdSegment(action)}`}
                                    className="flex items-center gap-2"
                                >
                                    <Checkbox
                                        id={`person-scope-action-${toAutomationIdSegment(action)}`}
                                        checked={field.state.value.includes(
                                            action,
                                        )}
                                        onCheckedChange={(checked) =>
                                            field.handleChange(
                                                checked
                                                    ? [
                                                          ...field.state.value,
                                                          action,
                                                      ]
                                                    : field.state.value.filter(
                                                          (item) =>
                                                              item !== action,
                                                      ),
                                            )
                                        }
                                    />
                                    {actionLabel(action)}
                                </label>
                            ))}
                        </div>
                    </fieldset>
                )}
            </form.Field>
            {!validation.success && (
                <p role="status" className="text-xs text-muted-foreground">
                    {validation.error.issues[0]?.message}
                </p>
            )}
            <div className="flex justify-end gap-2">
                <Button
                    id="person-scope-entry-cancel"
                    type="button"
                    variant="ghost"
                    size="sm"
                    onClick={onCancel}
                >
                    取消此条编辑
                </Button>
                <Button
                    id="person-scope-entry-apply"
                    type="button"
                    size="sm"
                    disabled={!validation.success}
                    onClick={() => void form.handleSubmit()}
                >
                    加入待保存列表
                </Button>
            </div>
        </section>
    )
}
