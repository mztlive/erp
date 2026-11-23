"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { InfoIcon, XIcon } from "lucide-react"
import { z } from "zod"

import {
    BusinessDiffPanel,
    BusinessFailureState,
    DetailWorkspaceDirtyBadge,
    DiscardConfirmDialog,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldLabel } from "@/components/ui/field"
import { LoadingButton } from "@/components/ui/loading-button"
import { RequiredMark } from "@/components/ui/required-mark"
import {
    useOrganizationStateQuery,
    usePreviewOrganizationChangeMutation,
    useSubmitOrganizationChangeMutation,
} from "@/features/organization/hooks/queries"
import { impactChanges } from "@/features/organization/lib/impact"
import { unitLabel } from "@/features/organization/lib/tree"
import type {
    OrganizationChangeReceipt,
    OrganizationChangeRequest,
    OrganizationStateView,
} from "@/features/organization/types"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { useAdminMutations } from "../../hooks/queries"
import type { RoleOption } from "../../hooks/use-role-filter"
import type { AdminAccount } from "../../types"
import { RoleOptionsPanel } from "./role-options-panel"

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
    canEditRoles,
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
    canEditRoles: boolean
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
    const person = view?.people.find((item) => item.id === account.id)
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
                        (!canEditRoles || !rolesReady))
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

    const [discardOpen, setDiscardOpen] = React.useState(false)
    const close = () => {
        if (pending) return
        if (!changed || completed) {
            onDone(false)
            return
        }
        setDiscardOpen(true)
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

    const footerHint = !changed
        ? undefined
        : confirmed
          ? "请核对上方修改内容，确认后生效"
          : canOrganization
            ? "点击「保存」先预览变更影响"
            : undefined

    return (
        <>
            <Dialog
                open
                onOpenChange={(open) => {
                    if (!open) close()
                }}
            >
                <DialogContent
                    closeButtonId="account-profile-close"
                    showCloseButton={!pending}
                    className="flex max-h-[90dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-xl"
                >
                    <DialogHeader className="shrink-0 border-b border-border/70 px-5 py-4 pr-14">
                        <div className="flex flex-wrap items-center gap-2">
                            <DialogTitle>编辑账号资料</DialogTitle>
                            {changed && !completed ? (
                                <DetailWorkspaceDirtyBadge label="有未保存的修改" />
                            ) : null}
                        </div>
                        <DialogDescription className="num font-medium text-foreground/80">
                            {account.account}
                        </DialogDescription>
                    </DialogHeader>

                    <form
                        className="flex min-h-0 flex-1 flex-col"
                        onSubmit={(event) => {
                            event.preventDefault()
                            void form.handleSubmit()
                        }}
                    >
                        <fieldset
                            disabled={pending}
                            className="min-h-0 flex-1 space-y-4 overflow-y-auto px-5 py-4"
                        >
                            {canName || canOrganization ? (
                                <div className="grid gap-4 sm:grid-cols-2">
                                    {canName ? (
                                        <form.AppField name="name">
                                            {(field) => (
                                                <field.TextField
                                                    id="account-profile-name"
                                                    label="姓名"
                                                    required
                                                />
                                            )}
                                        </form.AppField>
                                    ) : null}
                                    {canOrganization ? (
                                        <form.AppField name="unitId">
                                            {(field) => (
                                                <field.SelectField
                                                    id="account-profile-department"
                                                    label="所属部门"
                                                    className={
                                                        canName
                                                            ? undefined
                                                            : "sm:col-span-2"
                                                    }
                                                    options={unitOptions}
                                                    onRefresh={() =>
                                                        directory.refetch({
                                                            throwOnError: true,
                                                        })
                                                    }
                                                    allowClear={false}
                                                    placeholder={
                                                        person?.own_org_unit_id
                                                            ? unitLabel(
                                                                  view?.units ??
                                                                      [],
                                                                  person.own_org_unit_id,
                                                              )
                                                            : "未分配部门"
                                                    }
                                                />
                                            )}
                                        </form.AppField>
                                    ) : null}
                                </div>
                            ) : null}

                            {canEditRoles ? (
                                <form.AppField name="roleIds" mode="array">
                                    {(field) => {
                                        const isInvalid =
                                            field.state.meta.isTouched &&
                                            !field.state.value.length
                                        return (
                                            <Field
                                                data-invalid={
                                                    isInvalid || undefined
                                                }
                                            >
                                                <FieldLabel
                                                    id="account-profile-roles-label"
                                                    htmlFor="account-profile-roles-trigger"
                                                >
                                                    角色
                                                    <RequiredMark />
                                                </FieldLabel>
                                                <RoleOptionsPanel
                                                    id="account-profile-roles"
                                                    options={roleOptions}
                                                    selected={field.state.value}
                                                    disabled={!rolesReady}
                                                    invalid={isInvalid}
                                                    onToggle={(
                                                        roleId,
                                                        checked,
                                                    ) => {
                                                        field.handleChange(
                                                            checked
                                                                ? [
                                                                      ...field
                                                                          .state
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
                                                {field.state.value.length >
                                                0 ? (
                                                    <div className="flex flex-wrap items-center gap-1.5 pt-0.5">
                                                        {field.state.value.map(
                                                            (roleId) => {
                                                                const matched =
                                                                    roleOptions.find(
                                                                        (
                                                                            role,
                                                                        ) =>
                                                                            role.id ===
                                                                            roleId,
                                                                    )
                                                                const label =
                                                                    matched?.name ??
                                                                    "角色信息待确认"
                                                                return (
                                                                    <span
                                                                        key={
                                                                            roleId
                                                                        }
                                                                        className="inline-flex max-w-full items-center gap-1 rounded-md bg-muted/75 py-0.5 pr-1 pl-2 text-xs text-foreground"
                                                                    >
                                                                        <span className="truncate">
                                                                            {
                                                                                label
                                                                            }
                                                                        </span>
                                                                        <button
                                                                            id={`account-profile-role-remove-${toAutomationIdSegment(roleId)}`}
                                                                            type="button"
                                                                            disabled={
                                                                                !rolesReady
                                                                            }
                                                                            aria-label={`移除角色 ${label}`}
                                                                            onClick={() => {
                                                                                field.handleChange(
                                                                                    field.state.value.filter(
                                                                                        (
                                                                                            id,
                                                                                        ) =>
                                                                                            id !==
                                                                                            roleId,
                                                                                    ),
                                                                                )
                                                                                field.handleBlur()
                                                                            }}
                                                                            className="inline-flex size-4 shrink-0 cursor-pointer items-center justify-center rounded-xs text-muted-foreground transition-colors hover:bg-background hover:text-foreground focus-visible:outline-2 focus-visible:outline-primary disabled:cursor-default disabled:opacity-50"
                                                                        >
                                                                            <XIcon
                                                                                className="size-3"
                                                                                aria-hidden="true"
                                                                            />
                                                                        </button>
                                                                    </span>
                                                                )
                                                            },
                                                        )}
                                                    </div>
                                                ) : null}
                                                {roleOptionsError !==
                                                undefined ? (
                                                    <BusinessFailureState
                                                        id="account-profile-role-options-retry"
                                                        error={roleOptionsError}
                                                        onRetry={
                                                            onRetryRoleOptions
                                                        }
                                                    />
                                                ) : !rolesReady ? (
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
                                            </Field>
                                        )
                                    }}
                                </form.AppField>
                            ) : null}

                            {canOrganization ? (
                                <form.AppField name="reason">
                                    {(field) => (
                                        <field.TextareaField
                                            id="account-profile-reason"
                                            label="变更原因"
                                            placeholder="如：岗位调整、职责变更"
                                            required
                                            rows={2}
                                        />
                                    )}
                                </form.AppField>
                            ) : null}

                            {confirmed ? (
                                <div
                                    ref={confirmRef}
                                    className="space-y-2.5 rounded-lg border border-info-border/80 bg-info-soft/45 p-3.5"
                                >
                                    <div className="flex items-center gap-1.5 font-medium text-foreground">
                                        <InfoIcon
                                            className="size-4 shrink-0 text-info"
                                            aria-hidden="true"
                                        />
                                        <p>请确认本次修改</p>
                                    </div>
                                    <div className="space-y-1 text-body-compact">
                                        {values.name.trim() !== account.name ? (
                                            <p>
                                                <span className="text-muted-foreground">
                                                    姓名：
                                                </span>
                                                {account.name} →{" "}
                                                <span className="font-medium">
                                                    {values.name.trim()}
                                                </span>
                                            </p>
                                        ) : null}
                                        {!sameRoles(
                                            values.roleIds,
                                            account.role_ids,
                                        ) ? (
                                            <p>
                                                <span className="text-muted-foreground">
                                                    角色：
                                                </span>
                                                {account.role_ids
                                                    .map(
                                                        (id) =>
                                                            roleLabels.find(
                                                                (role) =>
                                                                    role.id ===
                                                                    id,
                                                            )?.name ??
                                                            "角色信息待确认",
                                                    )
                                                    .join("、") ||
                                                    "未分配"}{" "}
                                                →{" "}
                                                <span className="font-medium">
                                                    {values.roleIds
                                                        .map(
                                                            (id) =>
                                                                [
                                                                    ...assignableRoles,
                                                                    ...roleLabels,
                                                                ].find(
                                                                    (role) =>
                                                                        role.id ===
                                                                        id,
                                                                )?.name ??
                                                                "角色信息待确认",
                                                        )
                                                        .join("、")}
                                                </span>
                                            </p>
                                        ) : null}
                                    </div>
                                    {view &&
                                    values.unitId !==
                                        (person?.own_org_unit_id ?? "") ? (
                                        <BusinessDiffPanel
                                            title="所属部门变更"
                                            changes={impactChanges(
                                                receipt,
                                                view,
                                            )}
                                        />
                                    ) : null}
                                    <p className="text-xs text-muted-foreground">
                                        所属部门变更会影响「本人所属部门」范围，不会改派既有单据；姓名、角色与所属部门将一起保存。
                                    </p>
                                </div>
                            ) : null}

                            {error ? (
                                <Alert variant="destructive" role="alert">
                                    <AlertTitle>无法保存</AlertTitle>
                                    <AlertDescription>{error}</AlertDescription>
                                </Alert>
                            ) : null}
                        </fieldset>

                        <DialogFooter className="shrink-0 items-center border-t border-border/70 px-5 py-3 sm:justify-between">
                            <p className="min-w-0 text-xs text-muted-foreground">
                                {footerHint ?? ""}
                            </p>
                            <div className="flex shrink-0 items-center gap-2">
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
                            </div>
                        </DialogFooter>
                    </form>
                </DialogContent>
            </Dialog>
            <DiscardConfirmDialog
                idPrefix="account-profile-discard"
                open={discardOpen}
                onOpenChange={setDiscardOpen}
                title="放弃未保存的账号资料？"
                description="当前修改尚未保存，关闭后将丢失。"
                confirmLabel="放弃修改"
                cancelLabel="继续编辑"
                onConfirm={() => {
                    setDiscardOpen(false)
                    onDone(false)
                }}
            />
        </>
    )
}
