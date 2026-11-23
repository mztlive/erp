"use client"
import * as React from "react"
import { LockKeyholeIcon } from "lucide-react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { DiscardConfirmDialog } from "@/components/business"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
    DialogDescription,
} from "@/components/ui/dialog"
import {
    AlertDialog,
    AlertDialogContent,
    AlertDialogHeader,
    AlertDialogTitle,
    AlertDialogDescription,
    AlertDialogFooter,
    AlertDialogCancel,
    AlertDialogAction,
} from "@/components/ui/alert-dialog"
import type { OrgUnit } from "@/features/organization/types"
import { resourceLabel } from "@/lib/permission-catalog"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { PersonalGrantDraft } from "../../hooks/use-personal-grant-draft"
import { useSavePersonScope } from "../../hooks/use-person-data-scopes"
import {
    personScopeDefaults,
    personDefaultScopeDescription,
    personGrantKey,
    type PersonGrantEditorInput,
    type PersonScopeInput,
    type PersonScopeList,
    type PersonScopeGrant,
} from "../../api/person-data-scopes"
import { ProposedScopeSummary } from "./personal-scope-summary"
import {
    grantEditorDefaults,
    grantEditorSchema,
    editorTerms,
} from "./personal-grant-editor"
import { PersonalGrantList } from "./personal-grant-list"

/** 保存时规范化条件并合并重复范围，各条草稿编辑期间保留自己的身份。 */
function mergedGrants(grants: PersonScopeGrant[]): PersonScopeGrant[] {
    const result = new Map<string, PersonScopeGrant>()
    for (const grant of grants) {
        const key = personGrantKey(grant.terms)
        const existing = result.get(key)
        result.set(key, {
            key,
            terms: grant.terms,
            actions: [
                ...new Set([...(existing?.actions ?? []), ...grant.actions]),
            ],
        })
    }
    return [...result.values()]
}

export function PersonalGrantForm({
    userId,
    resource,
    onSaved,
    name,
    data,
    units,
    draft,
    onDraftChange,
    onDone,
    onReload,
}: {
    userId: string
    resource: string
    onSaved: () => void
    name: string
    data: PersonScopeList
    units: OrgUnit[]
    draft: PersonalGrantDraft | null
    onDraftChange: (draft: PersonalGrantDraft | null) => void
    onDone: () => void
    onReload: () => void
}) {
    const save = useSavePersonScope(userId)
    const [discarding, setDiscarding] = React.useState(false)
    const [reloadConfirmOpen, setReloadConfirmOpen] = React.useState(false)
    const [initialDraft] = React.useState(draft)
    const [version] = React.useState(
        draft?.policyVersion ?? data.policy_version,
    )
    const [error, setError] = React.useState<string | null>(null)
    const [defaults] = React.useState(() => personScopeDefaults(data, resource))
    const option = data.businesses.find((item) => item.resource === resource)
    const business = React.useMemo(
        () =>
            option?.configurable_actions.length
                ? { ...option, actions: option.configurable_actions }
                : undefined,
        [option],
    )
    const stale = version !== data.policy_version
    const schema = z
        .custom<PersonScopeInput>()
        .superRefine((value, context) => {
            if (value.grants.length > 32)
                context.addIssue({
                    code: "custom",
                    message:
                        "每项业务最多保留 32 条追加授权，请合并或移除部分授权后保存",
                })
            if (value.actions.length > 32)
                context.addIssue({
                    code: "custom",
                    message: "一次最多保存 32 项操作，请刷新后核对业务配置",
                })
            if (value.grants.some((grant) => grant.terms.length > 16))
                context.addIssue({
                    code: "custom",
                    message:
                        "每条追加授权最多包含 16 个范围条件，请移除超限授权并重新添加",
                })
            if (
                !business ||
                !value.actions.length ||
                value.actions.some(
                    (action) => !business.actions.includes(action),
                )
            )
                context.addIssue({
                    code: "custom",
                    message: "操作权限已变化，请刷新后核对",
                })
            if (
                business &&
                value.grants.some(
                    (grant) =>
                        grant.editor &&
                        !grantEditorSchema(business).safeParse({
                            ...grant.editor,
                            actions: grant.actions,
                        }).success,
                )
            )
                context.addIssue({
                    code: "custom",
                    message: "请完善所有追加范围后保存，或移除不需要的条目",
                })
            if (
                value.grants.some(
                    (grant) =>
                        !grant.actions.length ||
                        grant.actions.some(
                            (action) => !value.actions.includes(action),
                        ),
                )
            )
                context.addIssue({
                    code: "custom",
                    message: "请为每条追加范围选择至少一项有效操作",
                })
        })
    const form = useAppForm({
        defaultValues: initialDraft?.values ?? defaults,
        validators: { onSubmit: schema },
        onSubmit: async ({ value }) => {
            if (stale) {
                setError("权限配置已变化，草稿已保留。请刷新后重新核对。")
                return
            }
            setError(null)
            try {
                await save.mutateAsync({
                    value: {
                        ...value,
                        grants: mergedGrants(value.grants),
                        editor: null,
                    },
                    version,
                })
                onSaved()
                onDone()
            } catch (cause) {
                setError(getErrorMessage(cause, "保存失败，请刷新后重试"))
            }
        },
    })
    const value = useStore(form.store, (state) => state.values)
    const dirtySnapshot = (input: PersonScopeInput) =>
        JSON.stringify({
            resource: input.resource,
            actions: [...input.actions].sort(),
            grants: input.grants
                .map(({ actions, terms }) =>
                    JSON.stringify({
                        actions: [...actions].sort(),
                        terms: personGrantKey(terms),
                    }),
                )
                .sort(),
        })
    const dirty = dirtySnapshot(value) !== dirtySnapshot(defaults)
    React.useEffect(() => {
        onDraftChange({ values: value, policyVersion: version, dirty })
    }, [value, version, dirty, onDraftChange])
    const updateEditor = React.useCallback(
        (editor: PersonGrantEditorInput) => {
            if (!business) return
            // 适用操作由行内复选框管理，不让编辑器旧快照覆盖当前操作。
            const grant = form
                .getFieldValue("grants")
                .find((entry) => entry.key === editor.key)
            if (!grant) return
            const current = { ...editor, actions: grant.actions }
            form.setFieldValue(
                "grants",
                form.getFieldValue("grants").map((entry) =>
                    entry.key === editor.key
                        ? {
                              ...entry,
                              terms: editorTerms(current, business),
                              editor: current,
                          }
                        : entry,
                ),
            )
            form.setFieldValue("editor", current)
        },
        [form, business],
    )
    const validation = schema.safeParse(value)
    const incompleteKeys = new Set(
        value.grants
            .filter(
                (grant) =>
                    !grant.actions.length ||
                    grant.actions.some(
                        (action) => !business?.actions.includes(action),
                    ) ||
                    (business &&
                        grant.editor &&
                        !grantEditorSchema(business).safeParse({
                            ...grant.editor,
                            actions: grant.actions,
                        }).success),
            )
            .map((grant) => grant.key),
    )
    // 编号在创建时分配并写入草稿，不依赖当前数组位置；删除、重排均不改变既有身份。
    const nextDraftNumber = React.useRef(1)
    const addGrant = () => {
        if (!business || value.grants.length >= 32) return
        let key: string
        do {
            key = `draft-${resource}-${nextDraftNumber.current++}`
        } while (value.grants.some((grant) => grant.key === key))
        const editor = { ...grantEditorDefaults(business), key }
        form.setFieldValue("grants", [
            ...value.grants,
            {
                key,
                actions: editor.actions,
                terms: editorTerms(editor, business),
                editor,
            },
        ])
        form.setFieldValue("editor", editor)
    }
    const editGrant = (grant: PersonScopeGrant) => {
        if (!business) return
        form.setFieldValue(
            "editor",
            value.editor?.key === grant.key
                ? null
                : {
                      ...(grant.editor ?? grantEditorDefaults(business, grant)),
                      actions: grant.actions,
                  },
        )
    }
    const removeGrant = (key: string) => {
        const remaining = value.grants.filter((grant) => grant.key !== key)
        form.setFieldValue("grants", remaining)
        if (value.editor?.key === key) {
            const next = remaining.find((grant) => grant.editor)
            form.setFieldValue("editor", next?.editor ?? null)
        }
    }
    const changeActions = (key: string, actions: string[]) => {
        form.setFieldValue(
            "grants",
            value.grants.map((grant) =>
                grant.key === key
                    ? {
                          ...grant,
                          actions,
                          ...(grant.editor
                              ? { editor: { ...grant.editor, actions } }
                              : {}),
                      }
                    : grant,
            ),
        )
    }
    const leave = () => {
        if (save.isPending) return
        if (dirty) setDiscarding(true)
        else onDone()
    }
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !save.isPending) leave()
            }}
        >
            <DialogContent
                id="person-scope-dialog"
                closeButtonId="person-scope-dialog-close"
                showCloseButton={!save.isPending}
                className="flex max-h-[85dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-4xl"
                finalFocus={() =>
                    document.getElementById(
                        `person-scope-${toAutomationIdSegment(resource)}-edit`,
                    )
                }
            >
                <DialogHeader className="shrink-0 border-b border-grid px-5 py-4 pr-16">
                    <DialogTitle>配置{resourceLabel(resource)}范围</DialogTitle>
                    <DialogDescription>
                        {name} · 设置可访问的数据及适用操作
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex min-h-0 flex-col text-body-compact"
                    onSubmit={(event) => {
                        event.preventDefault()
                        event.stopPropagation()
                        void form.handleSubmit()
                    }}
                >
                    <div className="grid min-h-0 gap-5 overflow-y-auto p-5 md:grid-cols-[minmax(0,1fr)_16rem]">
                        <fieldset
                            disabled={save.isPending}
                            className="min-w-0 space-y-4"
                        >
                            {stale && (
                                <div
                                    role="status"
                                    className="text-xs text-warning"
                                >
                                    配置已变化，当前草稿不能直接保存。
                                    <Button
                                        id="person-scope-reload"
                                        type="button"
                                        variant="link"
                                        onClick={() =>
                                            setReloadConfirmOpen(true)
                                        }
                                    >
                                        刷新配置
                                    </Button>
                                </div>
                            )}
                            {business && (
                                <>
                                    <div className="flex items-start gap-2 rounded-lg border border-border/70 bg-surface-sunken px-3 py-2.5 text-xs text-muted-foreground">
                                        <LockKeyholeIcon
                                            className="mt-0.5 size-4 shrink-0"
                                            aria-hidden
                                        />
                                        <p>
                                            {personDefaultScopeDescription(
                                                business,
                                            )}
                                        </p>
                                    </div>
                                    <PersonalGrantList
                                        value={value}
                                        business={business}
                                        units={units}
                                        name={name}
                                        incompleteKeys={incompleteKeys}
                                        onAdd={addGrant}
                                        onEdit={editGrant}
                                        onRemove={removeGrant}
                                        onActionsChange={changeActions}
                                        onEditorChange={updateEditor}
                                    />
                                </>
                            )}
                            {error && (
                                <p
                                    role="alert"
                                    className="text-xs text-destructive"
                                >
                                    {error}
                                </p>
                            )}
                        </fieldset>
                        {business && (
                            <aside className="self-start rounded-lg border border-border/70 bg-surface-sunken/70 p-4 md:sticky md:top-0">
                                <ProposedScopeSummary
                                    value={value}
                                    incompleteKeys={incompleteKeys}
                                    business={business}
                                    units={units}
                                />
                            </aside>
                        )}
                    </div>
                    <div className="flex shrink-0 flex-wrap items-center justify-end gap-2 border-t border-grid bg-popover px-5 py-3">
                        <p
                            id="person-scope-save-hint"
                            role="status"
                            className="mr-auto min-w-0 basis-full text-xs text-muted-foreground sm:flex-1 sm:basis-auto"
                        >
                            {stale
                                ? "请先刷新并核对配置"
                                : !validation.success
                                  ? incompleteKeys.size > 0
                                      ? `${incompleteKeys.size} 条范围待完善，请补充或移除后保存`
                                      : validation.error.issues[0]?.message
                                  : "保存后统一生效"}
                        </p>
                        <Button
                            id="person-scope-cancel"
                            size="sm"
                            type="button"
                            variant="outline"
                            disabled={save.isPending}
                            onClick={leave}
                        >
                            取消
                        </Button>
                        <LoadingButton
                            loading={save.isPending}
                            id="person-scope-save"
                            size="sm"
                            type="submit"
                            aria-describedby="person-scope-save-hint"
                            disabled={
                                save.isPending || stale || !validation.success
                            }
                        >
                            {save.isPending ? "保存中…" : "保存"}
                        </LoadingButton>
                    </div>
                </form>
                <AlertDialog open={discarding} onOpenChange={setDiscarding}>
                    <AlertDialogContent>
                        <AlertDialogHeader>
                            <AlertDialogTitle>放弃本次修改？</AlertDialogTitle>
                            <AlertDialogDescription>
                                追加授权尚未保存，放弃后将保留原有配置。
                            </AlertDialogDescription>
                        </AlertDialogHeader>
                        <AlertDialogFooter>
                            <AlertDialogCancel id="person-scope-discard-cancel">
                                继续编辑
                            </AlertDialogCancel>
                            <AlertDialogAction
                                id="person-scope-discard-confirm"
                                variant="destructive"
                                onClick={onDone}
                            >
                                放弃修改
                            </AlertDialogAction>
                        </AlertDialogFooter>
                    </AlertDialogContent>
                </AlertDialog>
                <DiscardConfirmDialog
                    idPrefix="person-scope-reload-discard"
                    open={reloadConfirmOpen}
                    onOpenChange={setReloadConfirmOpen}
                    title="放弃当前草稿并刷新配置？"
                    description="配置已变化，刷新后将清除当前未保存草稿并重新加载最新权限。"
                    confirmLabel="放弃草稿并刷新"
                    cancelLabel="继续核对"
                    onConfirm={() => {
                        setReloadConfirmOpen(false)
                        onDone()
                        onReload()
                    }}
                />
            </DialogContent>
        </Dialog>
    )
}
