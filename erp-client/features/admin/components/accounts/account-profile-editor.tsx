"use client"

import { useOrganizationStateQuery } from "@/features/organization/hooks/queries"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Field, FieldLabel } from "@/components/ui/field"
import type { RoleOption } from "../../hooks/use-role-filter"
import { RoleOptionsPanel } from "./role-options-panel"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { LoadingButton } from "@/components/ui/loading-button"
import { BusinessDiffPanel, BusinessFailureState } from "@/components/business"
import { getErrorMessage } from "@/lib/api/errors"
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
    assignableRoles,
    roleLabels,
    rolesReady,
    roleOptionsError,
    onRetryRoleOptions,
    canOrganization,
    onDone,
}: {
    snapshot: AccountProfileSnapshot
    currentVersion: number | undefined
    canName: boolean
    assignableRoles: readonly RoleOption[]
    roleLabels: readonly RoleOption[]
    rolesReady: boolean
    roleOptionsError?: unknown
    onRetryRoleOptions?: () => void
    canOrganization: boolean
    onDone: (saved: boolean) => void
}) {
    const directory = useOrganizationStateQuery(false)
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
    const confirmRef = React.useRef<HTMLDivElement>(null)
    React.useEffect(() => {
        if (!confirmed) return
        confirmRef.current?.scrollIntoView({
            block: "nearest",
            behavior: "smooth",
        })
    }, [confirmed])
    const close = () => {
        if (pending) return
        if (
            !changed ||
            completed ||
            window.confirm("确定放弃未保存的账号资料？")
        )
            onDone(false)
    }
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
        (directory.data?.units ?? view?.units)
            ?.filter((unit) => unit.enabled)
            .map((unit) => ({ value: unit.id, label: unit.name })) ?? []
    const roleOptions = React.useMemo(
        () => [
            ...assignableRoles,
            ...account.role_ids
                .filter((id) => !assignableRoles.some((role) => role.id === id))
                .map((id) => {
                    const label = roleLabels.find((role) => role.id === id)
                    return {
                        id,
                        name: label?.name ?? "角色信息待确认",
                        permissions: label?.permissions,
                    }
                }),
        ],
        [account.role_ids, assignableRoles, roleLabels],
    )
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open) close()
            }}
        >
            <DialogContent
                closeButtonId="account-profile-close"
                className="flex max-h-[90dvh] flex-col gap-5 overflow-hidden sm:max-w-3xl"
            >
                <DialogHeader className="shrink-0 pr-8">
                    <div className="flex flex-wrap items-center gap-2">
                        <DialogTitle>编辑账号资料</DialogTitle>
                        {changed && !completed ? (
                            <span className="rounded-md bg-warning-soft px-2 py-1 text-xs font-medium text-warning-soft-foreground">
                                有未保存的修改
                            </span>
                        ) : null}
                    </div>
                    <DialogDescription>
                        姓名、所属部门和角色统一保存后生效；修改密码请在“账号安全”中单独操作。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex min-h-0 flex-col gap-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <fieldset
                        disabled={pending}
                        className="min-h-0 space-y-5 overflow-y-auto pr-1"
                    >
                        <div className="grid gap-4 sm:grid-cols-2">
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
                            <Field>
                                <FieldLabel>登录账号</FieldLabel>
                                <div className="flex h-control items-center gap-2 rounded-lg border border-input bg-muted/35 px-3 text-sm">
                                    <span className="min-w-0 truncate text-muted-foreground">
                                        {account.account}
                                    </span>
                                    <span className="ml-auto shrink-0 text-xs text-muted-foreground">
                                        不可修改
                                    </span>
                                </div>
                            </Field>
                            <form.AppField name="unitId">
                                {(field) => (
                                    <field.SelectField
                                        id="account-profile-department"
                                        label="所属部门"
                                        className="sm:col-span-2"
                                        options={unitOptions}
                                        onRefresh={() =>
                                            directory.refetch({
                                                throwOnError: true,
                                            })
                                        }
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
                        <div className="space-y-3 border-t border-border/70 pt-4">
                            <div className="space-y-1">
                                <h3
                                    id="account-profile-roles-label"
                                    className="text-xs font-medium text-muted-foreground"
                                >
                                    角色
                                </h3>
                                <p className="text-xs leading-5 text-muted-foreground">
                                    角色决定可以执行哪些操作；可处理哪些数据在“数据范围”中设置。
                                </p>
                            </div>
                            <form.AppField name="roleIds" mode="array">
                                {(field) => (
                                    <div className="grid gap-3 lg:grid-cols-[minmax(0,20rem)_minmax(0,1fr)] lg:items-start">
                                        <div className="space-y-2">
                                            <RoleOptionsPanel
                                                id="account-profile-roles"
                                                options={roleOptions}
                                                selected={field.state.value}
                                                disabled={
                                                    !canName || !rolesReady
                                                }
                                                invalid={
                                                    field.state.meta
                                                        .isTouched &&
                                                    !field.state.value.length
                                                }
                                                onToggle={(roleId, checked) => {
                                                    field.handleChange(
                                                        checked
                                                            ? [
                                                                  ...field.state
                                                                      .value,
                                                                  roleId,
                                                              ]
                                                            : field.state.value.filter(
                                                                  (id) =>
                                                                      id !==
                                                                      roleId,
                                                              ),
                                                    )
                                                    field.handleBlur()
                                                }}
                                            />
                                            {roleOptionsError !== undefined ? (
                                                <BusinessFailureState
                                                    id="account-profile-role-options-retry"
                                                    error={roleOptionsError}
                                                    onRetry={onRetryRoleOptions}
                                                />
                                            ) : !rolesReady && canName ? (
                                                <p
                                                    role="status"
                                                    className="text-xs text-muted-foreground"
                                                >
                                                    角色选项尚未就绪，暂不能调整角色。
                                                </p>
                                            ) : null}
                                            {!field.state.value.length ? (
                                                <p
                                                    role="alert"
                                                    className="text-xs text-destructive"
                                                >
                                                    至少选择一个角色。
                                                </p>
                                            ) : null}
                                        </div>
                                    </div>
                                )}
                            </form.AppField>
                        </div>
                        {canOrganization ? (
                            <div className="border-t border-border/70 pt-4">
                                <form.AppField name="reason">
                                    {(field) => (
                                        <field.TextareaField
                                            id="account-profile-reason"
                                            label="变更原因"
                                            description="组织类变更需要记录原因，保存前会展示影响预览。"
                                            required
                                            rows={2}
                                        />
                                    )}
                                </form.AppField>
                            </div>
                        ) : null}
                        {confirmed ? (
                            <div
                                ref={confirmRef}
                                className="space-y-2 rounded-xl border border-border/70 bg-muted/30 p-4"
                            >
                                <p className="font-medium">请确认本次修改</p>
                                {!sameRoles(
                                    values.roleIds,
                                    account.role_ids,
                                ) && (
                                    <p>
                                        角色：
                                        {account.role_ids
                                            .map(
                                                (id) =>
                                                    roleLabels.find(
                                                        (role) =>
                                                            role.id === id,
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
                                                    ].find(
                                                        (role) =>
                                                            role.id === id,
                                                    )?.name ?? "角色信息待确认",
                                            )
                                            .join("、")}
                                    </p>
                                )}
                                {values.name.trim() !== account.name && (
                                    <p>
                                        姓名：{account.name} →{" "}
                                        {values.name.trim()}
                                    </p>
                                )}
                                {view &&
                                    values.unitId !==
                                        (person?.own_org_unit_id ?? "") && (
                                        <BusinessDiffPanel
                                            title="所属部门变更"
                                            changes={impactChanges(
                                                receipt,
                                                view,
                                            )}
                                        />
                                    )}
                                <p className="text-xs text-muted-foreground">
                                    所属部门变更会影响“本人所属部门”范围；不会改派单据。姓名、角色与所属部门将一起保存。
                                </p>
                            </div>
                        ) : null}
                        {error ? (
                            <p
                                role="alert"
                                className="text-sm text-destructive"
                            >
                                {error}
                            </p>
                        ) : null}
                    </fieldset>
                    <DialogFooter className="shrink-0 border-t border-border pt-4">
                        <Button
                            id="account-profile-cancel"
                            type="button"
                            variant="outline"
                            disabled={pending}
                            onClick={close}
                        >
                            取消
                        </Button>
                        <LoadingButton
                            loading={pending}
                            id="account-profile-save"
                            type="submit"
                            disabled={!changed || pending}
                        >
                            {pending
                                ? "处理中…"
                                : confirmed
                                  ? "确认保存"
                                  : "保存"}
                        </LoadingButton>
                    </DialogFooter>
                </form>
            </DialogContent>
        </Dialog>
    )
}
