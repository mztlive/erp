"use client"
import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { z } from "zod"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
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
    personTermsDescription,
    type PersonGrantEditorInput,
    type PersonScopeInput,
    type PersonScopeList,
    type PersonScopeGrant,
} from "../../api/person-data-scopes"
import { ProposedScopeSummary } from "./personal-scope-summary"
import {
    PersonalGrantEditor,
    grantEditorDefaults,
    canEditGrant,
} from "./personal-grant-editor"

/** 授权规范条件构成稳定身份，编码完整键避免重复行或目标名导致 DOM ID 冲突。 */
function grantAutomationKey(key: string) {
    return toAutomationIdSegment(
        Array.from(key, (character) =>
            character.codePointAt(0)!.toString(16),
        ).join("-"),
    )
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
    const business = data.businesses.find((item) => item.resource === resource)
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
                value.actions.some(
                    (action) => !business.actions.includes(action),
                )
            )
                context.addIssue({
                    code: "custom",
                    message: "操作权限已变化，请刷新后核对",
                })
            if (value.editor)
                context.addIssue({
                    code: "custom",
                    message: "请先将当前授权加入列表，或取消此条编辑",
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
                    message: "追加授权包含已失效的操作，请刷新后核对",
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
                await save.mutateAsync({ value, version })
                onSaved()
                onDone()
            } catch (cause) {
                setError(getErrorMessage(cause, "保存失败，请刷新后重试"))
            }
        },
    })
    const value = useStore(form.store, (state) => state.values)
    React.useEffect(() => {
        onDraftChange({ values: value, policyVersion: version })
    }, [value, version, onDraftChange])
    const updateEditor = React.useCallback(
        (editor: PersonGrantEditorInput) => {
            form.setFieldValue("editor", editor)
        },
        [form],
    )
    const labels = new Map(units.map((unit) => [unit.id, unit.name]))
    const validation = schema.safeParse(value)
    const applyGrant = (grant: PersonScopeGrant) => {
        const previousKey = value.editor?.key
        const remaining = value.grants.filter(
            (entry) => entry.key !== previousKey,
        )
        const duplicate = remaining.find((entry) => entry.key === grant.key)
        form.setFieldValue(
            "grants",
            duplicate
                ? remaining.map((entry) =>
                      entry.key === grant.key
                          ? {
                                ...entry,
                                actions: [
                                    ...new Set([
                                        ...entry.actions,
                                        ...grant.actions,
                                    ]),
                                ],
                            }
                          : entry,
                  )
                : [...remaining, grant],
        )
        form.setFieldValue("editor", null)
    }
    const leave = () => {
        if (save.isPending) return
        if (initialDraft || JSON.stringify(value) !== JSON.stringify(defaults))
            setDiscarding(true)
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
                className="max-h-[85dvh] overflow-y-auto sm:max-w-2xl"
                finalFocus={() =>
                    document.getElementById(
                        `person-scope-${toAutomationIdSegment(resource)}-edit`,
                    )
                }
            >
                <DialogHeader>
                    <DialogTitle>
                        {name}的{resourceLabel(resource)}数据范围
                    </DialogTitle>
                    <DialogDescription>
                        按操作合并基础范围和追加授权。修改与移除将在保存后一起生效。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4 text-sm"
                    onSubmit={(event) => {
                        event.preventDefault()
                        event.stopPropagation()
                        void form.handleSubmit()
                    }}
                >
                    <fieldset disabled={save.isPending} className="space-y-4">
                        {stale && (
                            <div
                                role="status"
                                className="text-xs text-amber-700"
                            >
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
                                <section className="space-y-2 rounded-md border p-3">
                                    <h3 className="font-medium">
                                        基础范围
                                        {legacy.length ? "（转换后生效）" : ""}
                                    </h3>
                                    <p>
                                        {business.default_self
                                            ? `${name}负责的数据`
                                            : "此业务没有默认本人范围，请按实际职责添加授权。"}
                                    </p>
                                    {business.default_self && (
                                        <p className="text-xs text-muted-foreground">
                                            自动适用于已有的
                                            {business.actions
                                                .map(actionLabel)
                                                .join("、")}
                                            操作。按当前负责人判断，不按创建人判断。
                                        </p>
                                    )}
                                </section>
                                {legacy.length > 0 && (
                                    <section className="space-y-3 rounded-md border border-amber-400/50 bg-amber-50/30 p-3">
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
                                            <p className="text-xs text-amber-800">
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
                                <section className="space-y-3">
                                    <div className="flex items-center justify-between">
                                        <h3 className="font-medium">
                                            追加授权{" "}
                                            <span className="text-muted-foreground">
                                                {value.grants.length} 条
                                            </span>
                                        </h3>
                                        <Button
                                            id="person-scope-add"
                                            type="button"
                                            variant="outline"
                                            size="sm"
                                            disabled={
                                                Boolean(value.editor) ||
                                                value.grants.length >= 32
                                            }
                                            aria-describedby={
                                                value.grants.length >= 32
                                                    ? "person-scope-grant-limit"
                                                    : undefined
                                            }
                                            onClick={() =>
                                                form.setFieldValue(
                                                    "editor",
                                                    grantEditorDefaults(
                                                        business,
                                                    ),
                                                )
                                            }
                                        >
                                            添加授权范围
                                        </Button>
                                    </div>
                                    {value.grants.length >= 32 && (
                                        <p
                                            id="person-scope-grant-limit"
                                            role="status"
                                            className="text-xs text-muted-foreground"
                                        >
                                            每项业务最多保留 32
                                            条追加授权。可以修改已有授权，或移除部分授权后再添加。
                                        </p>
                                    )}
                                    {!value.grants.length && (
                                        <p className="text-xs text-muted-foreground">
                                            暂无追加授权。
                                            {business.default_self
                                                ? "当前只保留基础范围。"
                                                : "请添加此人可处理的数据范围。"}
                                        </p>
                                    )}
                                    <form.Field name="grants">
                                        {(field) => (
                                            <div className="space-y-2">
                                                {field.state.value.map(
                                                    (grant) => {
                                                        const id = `person-scope-grant-${grantAutomationKey(grant.key)}`
                                                        return (
                                                            <div
                                                                key={grant.key}
                                                                className="space-y-2 rounded-md border p-3"
                                                            >
                                                                <p className="leading-6">
                                                                    {personTermsDescription(
                                                                        grant.terms,
                                                                        labels,
                                                                    )}
                                                                </p>
                                                                <div className="flex flex-wrap items-center justify-between gap-2">
                                                                    <p className="text-xs text-muted-foreground">
                                                                        适用：
                                                                        {grant.actions
                                                                            .map(
                                                                                actionLabel,
                                                                            )
                                                                            .join(
                                                                                "、",
                                                                            )}
                                                                    </p>
                                                                    <div className="flex gap-1">
                                                                        {canEditGrant(
                                                                            grant,
                                                                            business,
                                                                        ) && (
                                                                            <Button
                                                                                id={`${id}-edit`}
                                                                                type="button"
                                                                                variant="ghost"
                                                                                size="sm"
                                                                                disabled={Boolean(
                                                                                    value.editor,
                                                                                )}
                                                                                onClick={() =>
                                                                                    form.setFieldValue(
                                                                                        "editor",
                                                                                        grantEditorDefaults(
                                                                                            business,
                                                                                            grant,
                                                                                        ),
                                                                                    )
                                                                                }
                                                                            >
                                                                                修改
                                                                            </Button>
                                                                        )}
                                                                        <Button
                                                                            id={`${id}-remove`}
                                                                            type="button"
                                                                            variant="ghost"
                                                                            size="sm"
                                                                            disabled={Boolean(
                                                                                value.editor,
                                                                            )}
                                                                            onClick={() =>
                                                                                field.handleChange(
                                                                                    field.state.value.filter(
                                                                                        (
                                                                                            entry,
                                                                                        ) =>
                                                                                            entry.key !==
                                                                                            grant.key,
                                                                                    ),
                                                                                )
                                                                            }
                                                                        >
                                                                            移除
                                                                        </Button>
                                                                    </div>
                                                                </div>
                                                                {!canEditGrant(
                                                                    grant,
                                                                    business,
                                                                ) && (
                                                                    <p className="text-xs text-muted-foreground">
                                                                        保留的原授权条件。如需调整，请移除此条并添加新范围。
                                                                    </p>
                                                                )}
                                                            </div>
                                                        )
                                                    },
                                                )}
                                            </div>
                                        )}
                                    </form.Field>
                                    {value.editor && (
                                        <PersonalGrantEditor
                                            key={value.editor.key ?? "new"}
                                            initial={value.editor}
                                            business={business}
                                            units={units}
                                            name={name}
                                            onChange={updateEditor}
                                            onApply={applyGrant}
                                            onCancel={() =>
                                                form.setFieldValue(
                                                    "editor",
                                                    null,
                                                )
                                            }
                                        />
                                    )}
                                    <p className="text-xs text-muted-foreground">
                                        相同范围会合并适用操作。移除一条授权后，其他授权覆盖的数据仍然可用。
                                    </p>
                                </section>
                                <ProposedScopeSummary
                                    name={name}
                                    value={value}
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
                    <div className="sticky bottom-0 flex justify-end gap-2 border-t bg-popover pt-4">
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
                        <Button
                            id="person-scope-save"
                            size="sm"
                            type="submit"
                            disabled={
                                save.isPending || stale || !validation.success
                            }
                        >
                            {save.isPending ? "保存中…" : "保存数据范围"}
                        </Button>
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
