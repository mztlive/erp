"use client"
import * as React from "react"
import { LockKeyholeIcon } from "lucide-react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { LoadingButton } from "@/components/ui/loading-button"
import { Checkbox } from "@/components/ui/checkbox"
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
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import { getErrorMessage } from "@/lib/api/errors"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { PersonalGrantDraft } from "../../hooks/use-personal-grant-draft"
import { useSavePersonScope } from "../../hooks/use-person-data-scopes"
import {
    personScopeDefaults,
    personScopeDescription,
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
    const legacy = data.items.filter(
        (item) =>
            item.resource === resource &&
            business?.actions.includes(item.action) &&
            !item.expression.additive,
    )
    const complexLegacy = legacy.filter(
        (item) =>
            item.expression.history_read || item.expression.condition !== null,
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
            if (legacy.length && !value.replace_legacy)
                context.addIssue({
                    code: "custom",
                    message: "请核对原授权，并确认转换为基础范围与追加授权",
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
            replace_legacy: input.replace_legacy,
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
    const labels = new Map(units.map((unit) => [unit.id, unit.name]))
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
                if (!open) leave()
            }}
        >
            <DialogContent
                id="person-scope-dialog"
                closeButtonId="person-scope-dialog-close"
                showCloseButton={!save.isPending}
                className="flex max-h-[85dvh] flex-col gap-0 overflow-hidden p-0 sm:max-w-3xl"
                finalFocus={() =>
                    document.getElementById(
                        `person-scope-${toAutomationIdSegment(resource)}-edit`,
                    )
                }
            >
                <DialogHeader className="shrink-0 px-6 pt-6 pb-5">
                    <DialogTitle>管理数据范围</DialogTitle>
                    <DialogDescription>
                        {name} · {resourceLabel(resource)}
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="flex min-h-0 flex-col text-sm"
                    onSubmit={(event) => {
                        event.preventDefault()
                        event.stopPropagation()
                        void form.handleSubmit()
                    }}
                >
                    <fieldset
                        disabled={save.isPending}
                        className="min-h-0 space-y-5 overflow-y-auto px-6 pb-5"
                    >
                        {stale && (
                            <div role="status" className="text-xs text-warning">
                                配置已变化，当前草稿不能直接保存。
                                <Button
                                    id="person-scope-reload"
                                    type="button"
                                    variant="link"
                                    onClick={() => {
                                        if (
                                            window.confirm(
                                                "放弃当前草稿并刷新配置？",
                                            )
                                        ) {
                                            onDone()
                                            onReload()
                                        }
                                    }}
                                >
                                    刷新配置
                                </Button>
                            </div>
                        )}
                        {business && (
                            <>
                                <div className="flex items-start gap-2 border-b pb-4 text-xs leading-5 text-muted-foreground">
                                    <LockKeyholeIcon
                                        className="mt-0.5 size-4 shrink-0"
                                        aria-hidden
                                    />
                                    <p>
                                        {business.default_self ? (
                                            <>
                                                <span className="font-medium text-foreground">
                                                    默认包含{name}负责的数据
                                                </span>{" "}
                                                ·
                                                按当前负责人判断，适用于下列可配置操作
                                                {legacy.length
                                                    ? "（转换后生效）"
                                                    : ""}
                                            </>
                                        ) : (
                                            "此业务没有默认本人范围，请添加授权。"
                                        )}
                                    </p>
                                </div>
                                {legacy.length > 0 && (
                                    <section className="space-y-3 rounded-md border border-warning-border bg-warning-soft p-3">
                                        <h3 className="font-medium">
                                            原授权仍按原条件生效
                                        </h3>
                                        {legacy.map((scope) => (
                                            <p
                                                key={scope.id}
                                                className="text-xs leading-6"
                                            >
                                                {actionLabel(scope.action)}：
                                                {personScopeDescription(
                                                    scope,
                                                    labels,
                                                )}
                                            </p>
                                        ))}
                                        <p className="text-xs text-muted-foreground">
                                            可直接转换的范围已列为追加授权。
                                            {business.default_self &&
                                                "默认本人已归入基础范围。"}
                                            保存将把以上操作统一转换为新方式。
                                        </p>
                                        {complexLegacy.length > 0 && (
                                            <p className="text-xs text-warning-soft-foreground">
                                                {complexLegacy
                                                    .map((scope) =>
                                                        actionLabel(
                                                            scope.action,
                                                        ),
                                                    )
                                                    .join("、")}
                                                包含历史参与读取或附加限制，不能直接转换。转换后这些原条件将被移除，请在下面重新添加所需范围，并核对最终结果。
                                            </p>
                                        )}
                                        <form.Field name="replace_legacy">
                                            {(field) => (
                                                <label
                                                    htmlFor="person-scope-convert-legacy"
                                                    className="flex items-start gap-2 text-xs leading-5"
                                                >
                                                    <Checkbox
                                                        id="person-scope-convert-legacy"
                                                        checked={
                                                            field.state.value
                                                        }
                                                        onCheckedChange={(
                                                            checked,
                                                        ) =>
                                                            field.handleChange(
                                                                checked ===
                                                                    true,
                                                            )
                                                        }
                                                    />
                                                    我已核对原授权与最终范围，确认转换并替换原授权条件。
                                                </label>
                                            )}
                                        </form.Field>
                                    </section>
                                )}
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
                                <ProposedScopeSummary
                                    value={value}
                                    incompleteKeys={incompleteKeys}
                                    business={business}
                                    units={units}
                                />
                            </>
                        )}
                        {!validation.success && (
                            <p
                                role="status"
                                className="text-xs text-muted-foreground"
                            >
                                {validation.error.issues[0]?.message}
                            </p>
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
                    <div className="flex shrink-0 flex-wrap items-center justify-end gap-2 border-t bg-popover px-6 py-4">
                        <p className="mr-auto text-xs text-muted-foreground">
                            所有修改将在保存后生效
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
                                onClick={onDone}
                            >
                                放弃修改
                            </AlertDialogAction>
                        </AlertDialogFooter>
                    </AlertDialogContent>
                </AlertDialog>
            </DialogContent>
        </Dialog>
    )
}
