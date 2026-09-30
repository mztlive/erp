"use client"
import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { FieldError } from "@/components/ui/field"
import { toFieldErrors, useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { ScopeTargetPicker } from "@/features/organization/components/scope-target-picker"
import {
    scopeDescription,
    type ScopeRule,
} from "@/features/organization/lib/scope-description"
import type { OrgUnit } from "@/features/organization/types"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getErrorMessage } from "@/lib/api/errors"
import type { PersonalGrantDraft } from "../../hooks/use-personal-grant-draft"
import { usePersonalGrantMutations } from "../../hooks/use-personal-business-grants"
import type {
    PersonalGrantList,
    PersonalGrantInput,
} from "../../api/personal-business-grants"

const grantSchema = z.object({
    role_id: z.string().min(1, "请选择依据角色"),
    resource: z.string().min(1, "请选择业务"),
    actions: z.array(z.string()).min(1, "至少选择一项操作"),
    org_unit_ids: z.array(z.string()).min(1, "至少选择一个部门"),
    include_descendants: z.boolean(),
})

export function PersonalGrantForm({
    userId,
    name,
    data,
    scopes,
    units,
    onDone,
    onReload,
    draft,
    onDraftChange,
}: {
    userId: string
    draft: PersonalGrantDraft | null
    onDraftChange: (draft: PersonalGrantDraft | null) => void
    name: string
    data: PersonalGrantList
    scopes: readonly ScopeRule[]
    units: OrgUnit[]
    onDone: () => void
    onReload: () => void
}) {
    const { create } = usePersonalGrantMutations(userId)
    const [error, setError] = React.useState<string | null>(null)
    const [discard, setDiscard] = React.useState(false)
    // Keep the original snapshot through selection; a concurrent change must produce a conflict.
    const [initialDraft] = React.useState(draft)
    const [policyVersion] = React.useState(
        draft?.policyVersion ?? data.policy_version,
    )
    const snapshot = data
    const changedVersion = data.policy_version !== policyVersion
    const firstRole = snapshot.roles[0]
    const [defaults] = React.useState<PersonalGrantInput>(() => ({
        role_id: firstRole?.id ?? "",
        resource: "",
        actions: [],
        org_unit_ids: [],
        include_descendants: false,
    }))
    const form = useAppForm({
        defaultValues: initialDraft?.values ?? defaults,
        validators: { onSubmit: grantSchema },
        onSubmit: async ({ value }) => {
            if (changedVersion) {
                setError("权限配置已变化，草稿已保留。请刷新配置后重新选择。")
                return
            }
            setError(null)
            try {
                await create.mutateAsync({
                    grant: value,
                    version: policyVersion,
                })
                onDone()
            } catch (failure) {
                setError(getErrorMessage(failure, "授权未保存，请刷新后重试。"))
            }
        },
    })
    const value = useStore(form.store, (state) => state.values)
    const changed = useStore(form.store, (state) => !state.isDefaultValue)
    const dirty = changed || initialDraft !== null
    React.useEffect(() => {
        onDraftChange({ values: value, policyVersion })
    }, [value, policyVersion, onDraftChange])
    const role = snapshot.roles.find((item) => item.id === value.role_id)
    const business = role?.resources.find(
        (item) => item.resource === value.resource,
    )
    const current = scopes.filter(
        (rule) =>
            rule.subject_type === "role" &&
            rule.subject_id === value.role_id &&
            rule.resource === value.resource &&
            rule.enabled !== false,
    )
    const limits = scopes.filter(
        (rule) =>
            rule.subject_type === "user" &&
            rule.resource === value.resource &&
            rule.enabled !== false,
    )
    const additions = snapshot.items.filter(
        (item) =>
            item.role_id === value.role_id &&
            item.resource === value.resource &&
            item.active_actions.length,
    )
    const targetLabels = value.org_unit_ids
        .map((id) => units.find((unit) => unit.id === id)?.name ?? "部门待确认")
        .join("、")
    return (
        <form
            className="space-y-4 rounded-md border bg-muted/20 p-4 text-sm"
            onSubmit={(event) => {
                event.preventDefault()
                event.stopPropagation()
                void form.handleSubmit()
            }}
        >
            <h3 className="font-semibold">给{name}扩大业务数据范围</h3>
            {changedVersion && (
                <div role="status" className="space-y-2 text-xs text-amber-700">
                    <p>
                        权限配置已变化，已保留你的选择。请重新读取配置再授权，原草稿不会自动套用新版本。
                    </p>
                    <Button
                        id="personal-grant-stale-refresh"
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => setDiscard(true)}
                    >
                        刷新配置后重新选择
                    </Button>
                </div>
            )}
            <div className="grid gap-3 sm:grid-cols-2">
                <label className="space-y-1.5" htmlFor="personal-grant-role">
                    <span>操作权限来自</span>
                    <select
                        id="personal-grant-role"
                        aria-describedby="personal-grant-role-error"
                        className="h-control w-full rounded-md border bg-background px-2"
                        value={value.role_id}
                        disabled={create.isPending}
                        onChange={(event) => {
                            form.setFieldValue("role_id", event.target.value)
                            form.setFieldValue("resource", "")
                            form.setFieldValue("actions", [])
                            form.setFieldValue("org_unit_ids", [])
                            form.setFieldValue("include_descendants", false)
                        }}
                    >
                        {snapshot.roles.map((item) => (
                            <option key={item.id} value={item.id}>
                                {item.name}
                            </option>
                        ))}
                    </select>
                    <form.Field name="role_id">
                        {(field) => (
                            <FieldError
                                id="personal-grant-role-error"
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        )}
                    </form.Field>
                </label>
                <label
                    className="space-y-1.5"
                    htmlFor="personal-grant-resource"
                >
                    <span>选择业务</span>
                    <select
                        id="personal-grant-resource"
                        aria-describedby="personal-grant-resource-error"
                        className="h-control w-full rounded-md border bg-background px-2"
                        value={value.resource}
                        disabled={create.isPending}
                        onChange={(event) => {
                            form.setFieldValue("resource", event.target.value)
                            form.setFieldValue("actions", [])
                            form.setFieldValue("org_unit_ids", [])
                            form.setFieldValue("include_descendants", false)
                        }}
                    >
                        <option value="">请选择业务</option>
                        {role?.resources.map((item) => (
                            <option key={item.resource} value={item.resource}>
                                {resourceLabel(item.resource)}
                            </option>
                        ))}
                    </select>
                    <form.Field name="resource">
                        {(field) => (
                            <FieldError
                                id="personal-grant-resource-error"
                                errors={toFieldErrors(field.state.meta.errors)}
                            />
                        )}
                    </form.Field>
                </label>
            </div>
            {business && (
                <>
                    <div className="space-y-1 text-xs leading-5 text-muted-foreground">
                        <p>角色共有范围：</p>
                        {current.length ? (
                            current.map((rule) => (
                                <p key={rule.id}>
                                    {rule.actions?.map(actionLabel).join("、")}
                                    ：{scopeDescription(rule, units)}
                                </p>
                            ))
                        ) : (
                            <p>
                                该角色尚未配置此业务范围，不默认视为本人或全部数据。
                            </p>
                        )}
                        {additions.length > 0 && (
                            <p>
                                此人已有附加范围：
                                {additions
                                    .map(
                                        (item) =>
                                            `${item.org_unit_ids.map((id) => units.find((unit) => unit.id === id)?.name ?? "部门待确认").join("、")}（${item.active_actions.map(actionLabel).join("、")}）`,
                                    )
                                    .join("；")}
                            </p>
                        )}
                        {limits.length > 0 && (
                            <p>
                                个人限制：
                                {limits
                                    .map(
                                        (rule) =>
                                            `${rule.actions?.map(actionLabel).join("、")}：${scopeDescription(rule, units)}`,
                                    )
                                    .join("；")}
                                。扩大后的范围仍受这些限制约束。
                            </p>
                        )}
                    </div>
                    <fieldset className="space-y-2" disabled={create.isPending}>
                        <legend className="mb-2 font-medium">适用操作</legend>
                        <div className="flex flex-wrap gap-x-5 gap-y-3">
                            {business.actions.map((action) => (
                                <label
                                    key={action}
                                    className="flex items-center gap-2"
                                    htmlFor={`personal-grant-action-${toAutomationIdSegment(action)}`}
                                >
                                    <Checkbox
                                        id={`personal-grant-action-${toAutomationIdSegment(action)}`}
                                        checked={value.actions.includes(action)}
                                        onCheckedChange={(checked) =>
                                            form.setFieldValue(
                                                "actions",
                                                checked
                                                    ? [...value.actions, action]
                                                    : value.actions.filter(
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
                        <form.Field name="actions">
                            {(field) => (
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            )}
                        </form.Field>
                        <p className="text-xs text-muted-foreground">
                            只能选择该人员通过此角色已拥有的操作；扩大范围不会增加新操作。
                        </p>
                    </fieldset>
                    <div className="space-y-2">
                        <p className="font-medium">允许处理的部门</p>
                        <ScopeTargetPicker
                            id="personal-grant-departments"
                            dimension="internal_org"
                            value={value.org_unit_ids}
                            onChange={(ids) =>
                                form.setFieldValue("org_unit_ids", ids)
                            }
                            units={units.filter((unit) => unit.enabled)}
                            disabled={create.isPending}
                        />
                        <form.Field name="org_unit_ids">
                            {(field) => (
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            )}
                        </form.Field>
                        <label
                            className="flex items-center gap-2"
                            htmlFor="personal-grant-descendants"
                        >
                            <Checkbox
                                id="personal-grant-descendants"
                                checked={value.include_descendants}
                                disabled={create.isPending}
                                onCheckedChange={(checked) =>
                                    form.setFieldValue(
                                        "include_descendants",
                                        checked === true,
                                    )
                                }
                            />
                            包含下级部门
                        </label>
                    </div>
                    <div
                        className="space-y-1 rounded-md bg-muted p-3 text-xs leading-5"
                        aria-live="polite"
                    >
                        <p className="font-medium">保存后的效果</p>
                        {value.actions.length && value.org_unit_ids.length ? (
                            <p>
                                在原有范围上，为{name}增加{targetLabels}
                                {value.include_descendants
                                    ? "及其下级部门"
                                    : ""}
                                的{resourceLabel(value.resource)}范围，适用于
                                {value.actions.map(actionLabel).join("、")}。
                            </p>
                        ) : (
                            <p>选择操作和部门后显示授权效果。</p>
                        )}
                        <p>
                            其他人员及其他业务不变。个人限制、关联单据范围、审批资格和业务状态仍按原规则检查。
                        </p>
                    </div>
                </>
            )}
            {error && (
                <div
                    role="alert"
                    className="space-y-2 text-xs text-destructive"
                >
                    <p>{error}</p>
                    <Button
                        id="personal-grant-refresh"
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={create.isPending}
                        onClick={() => {
                            setDiscard(true)
                        }}
                    >
                        刷新配置后重新选择
                    </Button>
                </div>
            )}
            {discard ? (
                <div className="flex flex-wrap items-center gap-2 text-xs">
                    <span>放弃未保存的选择？</span>
                    <Button
                        id="personal-grant-discard-confirm"
                        type="button"
                        size="sm"
                        variant="outline"
                        onClick={() => {
                            onReload()
                            onDone()
                        }}
                    >
                        放弃选择
                    </Button>
                    <Button
                        id="personal-grant-discard-cancel"
                        type="button"
                        size="sm"
                        variant="ghost"
                        onClick={() => setDiscard(false)}
                    >
                        继续编辑
                    </Button>
                </div>
            ) : (
                <div className="flex gap-2">
                    <Button
                        id="personal-grant-save"
                        type="submit"
                        size="sm"
                        disabled={
                            create.isPending ||
                            changedVersion ||
                            !value.resource ||
                            !value.actions.length ||
                            !value.org_unit_ids.length
                        }
                    >
                        {create.isPending ? "保存中…" : "确认授权"}
                    </Button>
                    <Button
                        id="personal-grant-cancel"
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={create.isPending}
                        onClick={() => (dirty ? setDiscard(true) : onDone())}
                    >
                        取消
                    </Button>
                </div>
            )}
        </form>
    )
}
