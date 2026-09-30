"use client"
import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Checkbox } from "@/components/ui/checkbox"
import type { RoleOption } from "../../hooks/use-role-filter"
import { Button } from "@/components/ui/button"
import { BusinessDiffPanel } from "@/components/business"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useAdminMutations } from "../../hooks/queries"
import {
    usePreviewOrganizationChangeMutation,
    useSubmitOrganizationChangeMutation,
} from "@/features/organization/hooks/queries"
import { isRelationActive, unitLabel } from "@/features/organization/lib/tree"
import { impactChanges } from "@/features/organization/lib/impact"
import { shanghaiDateTimeToUnix } from "@/features/organization/lib/change-payload"
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
const additionSchema = z.object({
    key: z.string(),
    roleId: z.string().min(1, "请选择角色"),
    unitId: z.string().min(1, "请选择部门"),
    descendants: z.enum(["true", "false"]),
    validTo: z
        .string()
        .refine(
            (value) => !value || shanghaiDateTimeToUnix(value) !== null,
            "有效期格式不正确",
        ),
})
const schema = z.object({
    name: z.string().trim().min(1, "请输入姓名").max(64, "姓名最多64字"),
    roleIds: z.array(z.string()),
    unitId: z.string(),
    removed: z.array(z.string()),
    additions: z.array(additionSchema),
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
    assignableRoles,
    roleLabels,
    rolesReady,
    canOrganization,
    onDone,
}: {
    snapshot: AccountProfileSnapshot
    currentVersion: number | undefined
    canName: boolean
    assignableRoles: readonly RoleOption[]
    roleLabels: readonly RoleOption[]
    rolesReady: boolean
    canOrganization: boolean
    onDone: (saved: boolean) => void
}) {
    const { account, view } = snapshot
    const person = view?.people.find((person) => person.id === account.id)
    const grants =
        view?.management.filter(
            (grant) =>
                grant.user_id === account.id &&
                isRelationActive(grant.valid_from, grant.valid_to, view.asOf),
        ) ?? []
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
                remove_management_ids: value.removed,
                add_management: value.additions.map((row) => ({
                    role_id: row.roleId,
                    org_unit_id: row.unitId,
                    include_descendants: row.descendants === "true",
                    valid_to: shanghaiDateTimeToUnix(row.validTo),
                })),
            },
        },
    })
    const form = useAppForm({
        defaultValues: {
            name: account.name,
            roleIds: account.role_ids,
            unitId: person?.own_org_unit_id ?? "",
            removed: [],
            additions: [],
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
                    value.unitId !== (person?.own_org_unit_id ?? "") ||
                    value.removed.length > 0 ||
                    value.additions.length > 0
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
                const missingRole =
                    grants.some(
                        (grant) =>
                            !value.removed.includes(grant.id) &&
                            !value.roleIds.includes(grant.role_id),
                    ) ||
                    value.additions.some(
                        (grant) => !value.roleIds.includes(grant.roleId),
                    )
                if (
                    missingRole &&
                    !sameRoles(value.roleIds, account.role_ids)
                ) {
                    setError(
                        "部门管理关系使用了未选择的角色，请同时移除该关系或保留角色。",
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
        values.unitId !== (person?.own_org_unit_id ?? "") ||
        values.removed.length > 0 ||
        values.additions.length > 0
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
    const roleOptions =
        view?.roles
            .filter((role) => role.enabled && values.roleIds.includes(role.id))
            .map((role) => ({ value: role.id, label: role.name })) ?? []
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
                </div>
                <form.AppField name="roleIds" mode="array">
                    {(field) => (
                        <section className="space-y-2 border-t pt-3">
                            <h3 className="font-medium">已分配角色</h3>
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
                                                    (role) => role.id === id,
                                                ),
                                        )
                                        .map((id) => ({
                                            id,
                                            name:
                                                roleLabels.find(
                                                    (role) => role.id === id,
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
                                            disabled={!canName || !rolesReady}
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
                                <p role="alert" className="text-destructive">
                                    至少选择一个角色。
                                </p>
                            )}
                        </section>
                    )}
                </form.AppField>
                <section className="space-y-2 border-t pt-3">
                    <div className="flex flex-wrap items-center justify-between gap-2">
                        <h3 className="font-medium">部门管理关系</h3>
                        {canOrganization && (
                            <Button
                                id="account-profile-management-add"
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={!roleOptions.length}
                                onClick={() =>
                                    form.pushFieldValue("additions", {
                                        key: crypto.randomUUID(),
                                        unitId: "",
                                        roleId:
                                            roleOptions.length === 1
                                                ? roleOptions[0].value
                                                : "",
                                        descendants: "false",
                                        validTo: "",
                                    })
                                }
                            >
                                添加管理部门
                            </Button>
                        )}
                    </div>
                    <p className="text-xs text-muted-foreground">
                        登记此人负责管理的部门。业务数据访问范围仍在“数据范围”设置。
                    </p>
                    {!grants.length && !values.additions.length && (
                        <p className="py-2 text-muted-foreground">
                            当前可见范围内没有部门管理关系。
                        </p>
                    )}
                    {grants.map((grant) => (
                        <div
                            key={grant.id}
                            className="flex flex-wrap items-center justify-between gap-2 border-b py-2"
                        >
                            <span
                                className={
                                    values.removed.includes(grant.id)
                                        ? "text-muted-foreground line-through"
                                        : ""
                                }
                            >
                                {unitLabel(view!.units, grant.org_unit_id)} ·{" "}
                                {view!.roles.find(
                                    (role) => role.id === grant.role_id,
                                )?.name ?? "角色信息待确认"}{" "}
                                ·{" "}
                                {grant.include_descendants
                                    ? "含下级"
                                    : "仅本级"}
                            </span>
                            {canOrganization && (
                                <Button
                                    id={`account-profile-management-remove-${toAutomationIdSegment(grant.id)}`}
                                    type="button"
                                    variant="ghost"
                                    size="sm"
                                    onClick={() =>
                                        form.setFieldValue(
                                            "removed",
                                            values.removed.includes(grant.id)
                                                ? values.removed.filter(
                                                      (id) => id !== grant.id,
                                                  )
                                                : [...values.removed, grant.id],
                                        )
                                    }
                                >
                                    {values.removed.includes(grant.id)
                                        ? "撤销移除"
                                        : "移除"}
                                </Button>
                            )}
                        </div>
                    ))}
                    {values.additions.map((row, index) => (
                        <div
                            key={row.key}
                            className="grid items-end gap-3 rounded-md border p-3 sm:grid-cols-2 xl:grid-cols-[1fr_1fr_8rem_1fr_auto]"
                        >
                            <form.AppField name={`additions[${index}].unitId`}>
                                {(field) => (
                                    <field.SelectField
                                        id={`account-profile-unit-${row.key}`}
                                        label="管理部门"
                                        options={unitOptions}
                                        required
                                    />
                                )}
                            </form.AppField>
                            <form.AppField name={`additions[${index}].roleId`}>
                                {(field) => (
                                    <field.SelectField
                                        id={`account-profile-role-${row.key}`}
                                        label="任职角色"
                                        options={roleOptions}
                                        required
                                    />
                                )}
                            </form.AppField>
                            <form.AppField
                                name={`additions[${index}].descendants`}
                            >
                                {(field) => (
                                    <field.SelectField
                                        id={`account-profile-descendants-${row.key}`}
                                        label="管理层级"
                                        options={[
                                            { value: "false", label: "仅本级" },
                                            { value: "true", label: "含下级" },
                                        ]}
                                        allowClear={false}
                                    />
                                )}
                            </form.AppField>
                            <form.AppField name={`additions[${index}].validTo`}>
                                {(field) => (
                                    <field.DateTimeField
                                        id={`account-profile-expiry-${row.key}`}
                                        label="有效期至（可选）"
                                    />
                                )}
                            </form.AppField>
                            <Button
                                id={`account-profile-draft-remove-${row.key}`}
                                type="button"
                                variant="ghost"
                                onClick={() =>
                                    form.removeFieldValue("additions", index)
                                }
                            >
                                移除
                            </Button>
                        </div>
                    ))}
                </section>
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
                        (values.unitId !== (person?.own_org_unit_id ?? "") ||
                            values.removed.length > 0 ||
                            values.additions.length > 0) && (
                            <BusinessDiffPanel
                                title="部门关系变更"
                                changes={impactChanges(receipt, view)}
                            />
                        )}
                    <p className="text-xs text-muted-foreground">
                        所属部门变更会影响“本人所属部门”范围；不会改派单据。姓名、角色与部门关系将一起保存。
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
                <Button
                    id="account-profile-save"
                    type="submit"
                    disabled={!changed || pending}
                >
                    {pending ? "处理中…" : confirmed ? "确认保存" : "保存"}
                </Button>
            </div>
        </form>
    )
}
