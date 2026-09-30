"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { ScopeTargetPicker } from "@/features/organization/components/scope-target-picker"
import { useReplaceDataScopeMutation } from "@/features/organization/hooks/queries"
import { validateCreateDataScope } from "@/features/organization/lib/scope-payload"
import type {
    DataScopeRecord,
    OrgUnit,
    ScopeDimension,
} from "@/features/organization/types"
import { getErrorMessage } from "@/lib/api/errors"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import {
    initialScopeChoice,
    scopeChoiceInput,
} from "../../lib/role-scope-choice"

export function RoleScopeChoiceForm({
    roleId,
    resource,
    actions,
    rows,
    units,
    dimensions,
    policyVersion,
    disabled,
    onDirtyChange,
    onReload,
    onSaved,
}: {
    roleId: string
    resource: string
    actions: readonly string[]
    rows: readonly DataScopeRecord[]
    units: readonly OrgUnit[]
    dimensions: readonly ScopeDimension[]
    policyVersion: number
    disabled: boolean
    onDirtyChange: (dirty: boolean) => void
    onSaved: () => void
    onReload: () => Promise<void>
}) {
    const replace = useReplaceDataScopeMutation()
    const [error, setError] = React.useState<string | null>(null)
    const [defaults] = React.useState(() =>
        initialScopeChoice(rows, actions, dimensions[0]!),
    )
    const form = useAppForm({
        defaultValues: defaults,
        onSubmit: async ({ value }) => {
            if (disabled || !value.range) return
            const input = scopeChoiceInput(value, roleId, resource, actions)
            const invalid = validateCreateDataScope(input)
            if (invalid) {
                setError(invalid)
                return
            }
            try {
                setError(null)
                await replace.mutateAsync({ input, policyVersion })
                form.reset(value)
                onSaved()
            } catch (failure) {
                setError(getErrorMessage(failure, "范围未保存，请重试"))
            }
        },
    })
    const value = useStore(form.store, (state) => state.values)
    const dirty = useStore(form.store, (state) => !state.isDefaultValue)
    React.useEffect(() => {
        onDirtyChange(dirty || replace.isPending)
    }, [dirty, replace.isPending, onDirtyChange])
    React.useEffect(() => () => onDirtyChange(false), [onDirtyChange])
    const busy = disabled || replace.isPending
    const objectLabel = resourceLabel(resource)
    const options =
        value.dimension === "internal_org"
            ? [
                  { value: "self_owned", label: `自己负责的${objectLabel}` },
                  { value: "own_org", label: `自己所属部门的${objectLabel}` },
                  {
                      value: "managed_orgs",
                      label: `自己管理部门的${objectLabel}`,
                  },
                  { value: "explicit", label: `指定部门的${objectLabel}` },
                  { value: "company", label: `公司范围内的${objectLabel}` },
              ]
            : [
                  {
                      value: "explicit",
                      label:
                          value.dimension === "warehouse"
                              ? "指定仓库的数据"
                              : "指定结算主体的数据",
                  },
                  { value: "company", label: "公司范围内的数据" },
              ]
    return (
        <div className="space-y-3">
            <p className="text-xs leading-5 text-muted-foreground">
                适用于：{actions.map(actionLabel).join("、")}
            </p>
            {dimensions.length > 1 && (
                <label
                    className="flex items-center gap-2 text-sm"
                    htmlFor="role-scope-dimension"
                >
                    按什么划定范围
                    <select
                        id="role-scope-dimension"
                        className="h-control rounded-md border bg-background px-2"
                        value={value.dimension}
                        disabled={busy}
                        onChange={(event) => {
                            form.setFieldValue(
                                "dimension",
                                event.target.value as ScopeDimension,
                            )
                            form.setFieldValue("range", "")
                            form.setFieldValue("targets", [])
                        }}
                    >
                        {dimensions.map((dimension) => (
                            <option key={dimension} value={dimension}>
                                {dimension === "internal_org"
                                    ? "负责人及部门"
                                    : dimension === "warehouse"
                                      ? "仓库"
                                      : "结算主体"}
                            </option>
                        ))}
                    </select>
                </label>
            )}
            <fieldset disabled={busy} className="space-y-3">
                <legend className="sr-only">选择数据范围</legend>
                {options.map((option) => (
                    <label
                        key={option.value}
                        htmlFor={`role-scope-choice-${option.value}`}
                        className="flex cursor-pointer items-center gap-2 text-sm"
                    >
                        <input
                            id={`role-scope-choice-${option.value}`}
                            type="radio"
                            name="role-scope-choice"
                            className="size-4 accent-primary"
                            checked={value.range === option.value}
                            onChange={() => {
                                form.setFieldValue("range", option.value)
                                setError(null)
                            }}
                        />
                        {option.label}
                    </label>
                ))}
                {value.range === "explicit" && (
                    <ScopeTargetPicker
                        id="role-scope-targets"
                        disabled={busy}
                        dimension={value.dimension}
                        units={units.filter((unit) => unit.enabled)}
                        value={value.targets}
                        onChange={(targets) =>
                            form.setFieldValue("targets", targets)
                        }
                    />
                )}
                {value.dimension === "internal_org" &&
                    ["explicit", "own_org"].includes(value.range) && (
                        <label
                            htmlFor="role-scope-descendants"
                            className="flex items-center gap-2 text-sm"
                        >
                            <Checkbox
                                id="role-scope-descendants"
                                checked={value.descendants}
                                onCheckedChange={(checked) =>
                                    form.setFieldValue(
                                        "descendants",
                                        Boolean(checked),
                                    )
                                }
                            />
                            包含下级部门
                        </label>
                    )}
            </fieldset>
            {value.range === "self_owned" && (
                <p className="text-xs leading-5 text-muted-foreground">
                    “自己”指使用此岗位的每位人员，按业务负责人判断。
                </p>
            )}
            {value.range === "managed_orgs" && (
                <p className="text-xs leading-5 text-muted-foreground">
                    按每位人员已设置的管理部门执行；未设置管理部门时，不会获得部门范围。
                </p>
            )}
            {value.range === "company" && (
                <p className="text-xs leading-5 text-amber-700">
                    此岗位的所选操作将扩大至公司范围，人员的个人限制仍然生效。
                </p>
            )}
            {!value.range && (
                <p className="text-xs leading-5 text-muted-foreground">
                    当前未设置统一范围。选择一项后，可统一替换以上操作的范围。
                </p>
            )}
            <div className="flex flex-wrap items-center gap-2">
                <Button
                    id="role-scope-save"
                    type="button"
                    size="sm"
                    disabled={
                        busy ||
                        !value.range ||
                        (!dirty &&
                            !rows.some(
                                (row) => row.scopeType === "collaborative",
                            ))
                    }
                    onClick={() => void form.handleSubmit()}
                >
                    {replace.isPending ? "正在保存…" : "保存数据范围"}
                </Button>
                {dirty && (
                    <Button
                        id="role-scope-reset"
                        type="button"
                        size="sm"
                        variant="ghost"
                        disabled={replace.isPending}
                        onClick={() => {
                            form.reset()
                            setError(null)
                        }}
                    >
                        撤销选择
                    </Button>
                )}
            </div>
            <p className="text-xs leading-5 text-muted-foreground">
                保存后替换以上操作的原有范围并立即生效；其他操作不变。底部按钮仅保存岗位名称和操作权限。
            </p>
            {error && (
                <div
                    role="alert"
                    className="space-y-2 text-sm text-destructive"
                >
                    <p>{error}</p>
                    <Button
                        id="role-scope-reload"
                        type="button"
                        size="sm"
                        variant="outline"
                        disabled={replace.isPending}
                        onClick={async () => {
                            await onReload()
                            form.reset()
                            setError(null)
                        }}
                    >
                        放弃选择并重新读取
                    </Button>
                </div>
            )}
        </div>
    )
}
