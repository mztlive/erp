"use client"

import { KeyRoundIcon } from "lucide-react"
import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Checkbox } from "@/components/ui/checkbox"
import type { RoleOption } from "../../hooks/use-role-filter"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { BusinessDiffPanel } from "@/components/business"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useAdminMutations } from "../../hooks/queries"
import {
    usePreviewOrganizationChangeMutation,
    useSubmitOrganizationChangeMutation,
} from "@/features/organization/hooks/queries"
import { unitLabel } from "@/features/organization/lib/tree"
import { impactChanges } from "@/features/organization/lib/impact"
import type {
    OrganizationChangeReceipt,
    OrganizationChangeRequest,
    OrganizationStateView,
} from "@/features/organization/types"
import type { AdminAccount } from "../../types"

export type AccountProfileSnapshot = {
    account: AdminAccount
    view: OrganizationStateView | undefined
}
const schema = z.object({
    name: z.string().trim().min(1, "请输入姓名").max(64, "姓名最多64字"),
    roleIds: z.array(z.string()),
    unitId: z.string(),
    reason: z.string().max(1000),
})
type Values = z.infer<typeof schema>
const sameRoles = (left: readonly string[], right: readonly string[]) =>
    JSON.stringify([...left].sort()) === JSON.stringify([...right].sort())

/** 所有字段只写草稿，预览和统一保存前不提交组织或账号变更。 */
export function AccountProfileEditor({
    snapshot,
    currentVersion,
    canName,
    onPasswordChange,
    assignableRoles,
    roleLabels,
    rolesReady,
    canOrganization,
    onDone,
}: {
    snapshot: AccountProfileSnapshot
    currentVersion: number | undefined
    onPasswordChange: () => void
    canName: boolean
    assignableRoles: readonly RoleOption[]
    roleLabels: readonly RoleOption[]
    rolesReady: boolean
    canOrganization: boolean
    onDone: (saved: boolean) => void
}) {
    const { account, view } = snapshot
    const person = view?.people.find((person) => person.id === account.id)
    const preview = usePreviewOrganizationChangeMutation()
    const submit = useSubmitOrganizationChangeMutation()
    const admin = useAdminMutations()
    const [key] = React.useState(() => crypto.randomUUID())
    const [receipt, setReceipt] =
        React.useState<OrganizationChangeReceipt | null>(null)
    const [previewed, setPreviewed] = React.useState<string | null>(null)
    const [error, setError] = React.useState<string | null>(null)
    const [completed, setCompleted] = React.useState(false)
    const pending = preview.isPending || submit.isPending || admin.isUpdating
    const build = (value: Values): OrganizationChangeRequest => ({
        expected_version: view!.organizationVersion,
        idempotency_key: key,
        reason: value.reason.trim(),
        change: {
            operation: "update_person_profile",
            profile: {
                user_id: account.id,
                expected_name: account.name,
                expected_role_ids: account.role_ids,
                role_ids: sameRoles(value.roleIds, account.role_ids)
                    ? null
                    : value.roleIds,
                name:
                    value.name.trim() !== account.name
                        ? value.name.trim()
                        : null,
                org_unit_id:
                    value.unitId !== (person?.own_org_unit_id ?? "")
                        ? value.unitId
                        : null,
            },
        },
    })
    const form = useAppForm({
        defaultValues: {
            name: account.name,
            roleIds: account.role_ids,
            unitId: person?.own_org_unit_id ?? "",
            reason: "",
        } as Values,
        validators: { onChange: schema },
        onSubmit: async ({ value }) => {
            setError(null)
            try {
                if (
                    !sameRoles(value.roleIds, account.role_ids) &&
                    !value.roleIds.length
                ) {
                    setError("至少选择一个角色。")
                    return
                }
                const organizationChanged =
                    value.unitId !== (person?.own_org_unit_id ?? "")
                if (
                    (organizationChanged && !canOrganization) ||
                    (value.name.trim() !== account.name && !canName) ||
                    (!sameRoles(value.roleIds, account.role_ids) &&
                        (!canName || !rolesReady))
                ) {
                    setError(
                        "当前权限已变化，不能保存这些修改。草稿已保留，请联系管理员或取消编辑。",
                    )
                    return
                }
                if (canOrganization && view) {
                    const request = build(value)
                    const prepared =
                        receipt && previewed === JSON.stringify(request)
                    if (
                        !prepared &&
                        currentVersion !== view.organizationVersion
                    ) {
                        setError(
                            "组织配置已变化，当前草稿已保留。请取消编辑并刷新后重新核对。",
                        )
                        return
                    }
                    if (!value.reason.trim()) {
                        setError("请填写本次资料变更的原因。")
                        return
                    }
                    if (!prepared) {
                        setReceipt(await preview.mutateAsync(request))
                        setPreviewed(JSON.stringify(request))
                        return
                    }
                    await submit.mutateAsync(request)
                } else {
                    await admin.updateAdmin({
                        id: account.id,
                        payload: {
                            ...(value.name.trim() !== account.name
                                ? { name: value.name.trim() }
                                : {}),
                            ...(!sameRoles(value.roleIds, account.role_ids)
                                ? { role_ids: value.roleIds }
                                : {}),
                        },
                    })
                }
                setCompleted(true)
                onDone(true)
            } catch (cause) {
                setError(
                    getErrorMessage(
                        cause,
                        "保存失败，修改已保留，请核对后重试。",
                    ),
                )
            }
        },
    })
    const values = useStore(form.store, (state) => state.values)
    const changed =
        !sameRoles(values.roleIds, account.role_ids) ||
        values.name.trim() !== account.name ||
        values.unitId !== (person?.own_org_unit_id ?? "")
    const confirmed =
        receipt && view && previewed === JSON.stringify(build(values))
    React.useEffect(() => {
        if (!changed || completed) return
        const unload = (event: BeforeUnloadEvent) => {
            event.preventDefault()
            event.returnValue = ""
        }
        const navigation = (window as Window & { navigation?: EventTarget })
            .navigation
        const leave = (event: Event) => {
            if (
                event.cancelable &&
                !window.confirm("账号资料尚未保存，确定放弃修改并离开？")
            ) {
                event.preventDefault()
                event.stopPropagation()
            }
        }
        const links = (event: MouseEvent) => {
            if (
                !navigation &&
                event.target instanceof Element &&
                event.target.closest("a[href]")
            )
                leave(event)
        }
        window.addEventListener("beforeunload", unload)
        navigation?.addEventListener("navigate", leave)
        document.addEventListener("click", links, true)
        return () => {
            window.removeEventListener("beforeunload", unload)
            navigation?.removeEventListener("navigate", leave)
            document.removeEventListener("click", links, true)
        }
    }, [changed, completed])
    const unitOptions =
        view?.units
            .filter((unit) => unit.enabled)
            .map((unit) => ({ value: unit.id, label: unit.name })) ?? []
    return (
        <form
            className="space-y-4 text-sm"
            onSubmit={(event) => {
                event.preventDefault()
                void form.handleSubmit()
            }}
        >
            <div className="flex items-center justify-between gap-3">
                <h2 className="font-semibold">账号资料 · 编辑中</h2>
                <span className="text-xs text-muted-foreground">
                    修改在统一保存后生效
                </span>
            </div>
            <fieldset disabled={pending} className="space-y-4">
                <div className="grid gap-4 sm:grid-cols-2 xl:grid-cols-3">
                    <form.AppField name="name">
                        {(field) => (
                            <field.TextField
                                id="account-profile-name"
                                label="姓名"
                                disabled={!canName}
                                required
                            />
                        )}
                    </form.AppField>
                    <div>
                        <p className="mb-2 text-xs text-muted-foreground">
                            登录账号
                        </p>
                        <p className="py-2">
                            {account.account}{" "}
                            <span className="ml-2 text-xs text-muted-foreground">
                                不可修改
                            </span>
                        </p>
                    </div>
                    <form.AppField name="unitId">
                        {(field) => (
                            <field.SelectField
                                id="account-profile-department"
                                label="所属部门"
                                options={unitOptions}
                                disabled={!canOrganization}
                                allowClear={false}
                                placeholder={
                                    person?.own_org_unit_id
                                        ? unitLabel(
                                              view?.units ?? [],
                                              person.own_org_unit_id,
                                          )
                                        : "未分配部门"
                                }
                            />
                        )}
                    </form.AppField>
                    {canName && (
                        <div>
                            <p className="mb-2 text-xs text-muted-foreground">
                                登录密码
                            </p>
                            <Button
                                id="account-profile-password"
                                type="button"
                                variant="outline"
                                size="sm"
                                onClick={onPasswordChange}
                            >
                                <KeyRoundIcon data-icon="inline-start" />
                                修改密码
                            </Button>
                            <p className="mt-1 text-xs text-muted-foreground">
                                密码单独保存。
                            </p>
                        </div>
                    )}
                    <form.AppField name="roleIds" mode="array">
                        {(field) => (
                            <section className="space-y-2 sm:col-span-2 xl:col-span-3">
                                <h3 className="text-xs text-muted-foreground">
                                    角色
                                </h3>
                                <p className="text-xs text-muted-foreground">
                                    角色决定可以执行哪些操作；可处理哪些数据在“数据范围”中设置。
                                </p>
                                {!rolesReady && canName && (
                                    <p
                                        role="status"
                                        className="text-muted-foreground"
                                    >
                                        角色选项尚未就绪，暂不能调整角色。
                                    </p>
                                )}
                                <div className="grid gap-2 sm:grid-cols-2 xl:grid-cols-3">
                                    {[
                                        ...assignableRoles,
                                        ...account.role_ids
                                            .filter(
                                                (id) =>
                                                    !assignableRoles.some(
                                                        (role) =>
                                                            role.id === id,
                                                    ),
                                            )
                                            .map((id) => ({
                                                id,
                                                name:
                                                    roleLabels.find(
                                                        (role) =>
                                                            role.id === id,
                                                    )?.name ?? "角色信息待确认",
                                            })),
                                    ].map((role) => (
                                        <label
                                            key={role.id}
                                            id={`account-profile-role-label-${toAutomationIdSegment(role.id)}`}
                                            htmlFor={`account-profile-role-option-${toAutomationIdSegment(role.id)}`}
                                            className="flex items-center gap-2 rounded-md border px-3 py-2"
                                        >
                                            <Checkbox
                                                id={`account-profile-role-option-${toAutomationIdSegment(role.id)}`}
                                                disabled={
                                                    !canName || !rolesReady
                                                }
                                                checked={field.state.value.includes(
                                                    role.id,
                                                )}
                                                onCheckedChange={(checked) =>
                                                    field.handleChange(
                                                        checked
                                                            ? [
                                                                  ...field.state
                                                                      .value,
                                                                  role.id,
                                                              ]
                                                            : field.state.value.filter(
                                                                  (id) =>
                                                                      id !==
                                                                      role.id,
                                                              ),
                                                    )
                                                }
                                            />
                                            <span>{role.name}</span>
                                        </label>
                                    ))}
                                </div>
                                {!field.state.value.length && (
                                    <p
                                        role="alert"
                                        className="text-destructive"
                                    >
                                        至少选择一个角色。
                                    </p>
                                )}
                            </section>
                        )}
                    </form.AppField>
                </div>
                {canOrganization && (
                    <form.AppField name="reason">
                        {(field) => (
                            <field.TextareaField
                                id="account-profile-reason"
                                label="变更原因"
                                required
                                rows={2}
                            />
                        )}
                    </form.AppField>
                )}
            </fieldset>
            {confirmed && (
                <div className="space-y-2 rounded-md border p-3">
                    <p className="font-medium">请确认本次修改</p>
                    {!sameRoles(values.roleIds, account.role_ids) && (
                        <p>
                            角色：
                            {account.role_ids
                                .map(
                                    (id) =>
                                        roleLabels.find(
                                            (role) => role.id === id,
                                        )?.name ?? "角色信息待确认",
                                )
                                .join("、") || "未分配"}{" "}
                            →{" "}
                            {values.roleIds
                                .map(
                                    (id) =>
                                        [
                                            ...assignableRoles,
                                            ...roleLabels,
                                        ].find((role) => role.id === id)
                                            ?.name ?? "角色信息待确认",
                                )
                                .join("、")}
                        </p>
                    )}
                    {values.name.trim() !== account.name && (
                        <p>
                            姓名：{account.name} → {values.name.trim()}
                        </p>
                    )}
                    {view &&
                        values.unitId !== (person?.own_org_unit_id ?? "") && (
                            <BusinessDiffPanel
                                title="所属部门变更"
                                changes={impactChanges(receipt, view)}
                            />
                        )}
                    <p className="text-xs text-muted-foreground">
                        所属部门变更会影响“本人所属部门”范围；不会改派单据。姓名、角色与所属部门将一起保存。
                    </p>
                </div>
            )}
            {error && (
                <p role="alert" className="text-sm text-destructive">
                    {error}
                </p>
            )}
            <div className="flex justify-end gap-2 border-t pt-3">
                <Button
                    id="account-profile-cancel"
                    type="button"
                    variant="outline"
                    disabled={pending}
                    onClick={() => {
                        if (
                            !changed ||
                            window.confirm("确定放弃未保存的账号资料？")
                        )
                            onDone(false)
                    }}
                >
                    取消
                </Button>
                <LoadingButton
                    loading={pending}
                    id="account-profile-save"
                    type="submit"
                    disabled={!changed || pending}
                >
                    {pending ? "处理中…" : confirmed ? "确认保存" : "保存"}
                </LoadingButton>
            </div>
        </form>
    )
}
