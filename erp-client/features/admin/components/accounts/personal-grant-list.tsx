"use client"

import { useEffect } from "react"
import { cn } from "@/lib/utils"
import {
    ChevronDownIcon,
    ChevronUpIcon,
    PlusIcon,
    Trash2Icon,
} from "lucide-react"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import type { OrgUnit } from "@/features/organization/types"
import { actionLabel } from "@/lib/permission-catalog"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    personTermsDescription,
    type PersonScopeInput,
    type PersonScopeBusiness,
    type PersonScopeGrant,
    type PersonGrantEditorInput,
} from "../../api/person-data-scopes"
import { PersonalGrantEditor, canEditGrant } from "./personal-grant-editor"

/** 使用完整范围业务键编码，避免同名授权的自动化目标冲突。 */
function grantAutomationKey(key: string) {
    return toAutomationIdSegment(
        Array.from(key, (character) =>
            character.codePointAt(0)!.toString(16),
        ).join("-"),
    )
}

export function PersonalGrantList({
    value,
    business,
    units,
    name,
    incompleteKeys,
    onAdd,
    onEdit,
    onRemove,
    onActionsChange,
    onEditorChange,
}: {
    value: PersonScopeInput
    business: PersonScopeBusiness
    units: OrgUnit[]
    name: string
    incompleteKeys: Set<string>
    onAdd: () => void
    onEdit: (grant: PersonScopeGrant) => void
    onRemove: (key: string) => void
    onActionsChange: (key: string, actions: string[]) => void
    onEditorChange: (value: PersonGrantEditorInput) => void
}) {
    const labels = new Map(units.map((unit) => [unit.id, unit.name]))
    const editingKey = value.editor?.key
    useEffect(() => {
        if (!editingKey) return
        const trigger = document.getElementById(
            `person-scope-grant-${grantAutomationKey(editingKey)}-edit`,
        )
        trigger?.focus({ preventScroll: true })
        trigger?.scrollIntoView({ block: "nearest" })
    }, [editingKey])
    return (
        <section className="space-y-3" aria-label="追加授权">
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 className="font-medium">
                    追加范围
                    <span className="ml-2 text-xs font-normal text-muted-foreground">
                        {value.grants.length} 条
                    </span>
                </h3>
                <Button
                    id="person-scope-add"
                    type="button"
                    variant="outline"
                    size="sm"
                    disabled={value.grants.length >= 32}
                    aria-describedby={
                        value.grants.length >= 32
                            ? "person-scope-grant-limit"
                            : undefined
                    }
                    onClick={onAdd}
                >
                    <PlusIcon data-icon="inline-start" aria-hidden />
                    添加范围
                </Button>
            </div>
            {!value.grants.length ? (
                <div className="rounded-lg border border-dashed border-grid px-4 py-6 text-center text-sm text-muted-foreground">
                    {business.default_self
                        ? "仅包含本人负责的数据。需要扩大范围时，点击添加范围。"
                        : "尚未配置范围，点击添加范围开始设置。"}
                </div>
            ) : (
                <div className="space-y-3">
                    {value.grants.map((grant) => {
                        const id = `person-scope-grant-${grantAutomationKey(grant.key)}`
                        const expanded = value.editor?.key === grant.key
                        const editable =
                            canEditGrant(grant, business) ||
                            Boolean(grant.editor) ||
                            expanded
                        const incomplete = incompleteKeys.has(grant.key)
                        const description = incomplete
                            ? "待完善范围"
                            : personTermsDescription(grant.terms, labels)
                        return (
                            <div
                                key={grant.key}
                                className={cn(
                                    "min-w-0 rounded-lg border",
                                    expanded
                                        ? "border-foreground/25"
                                        : "border-grid",
                                )}
                            >
                                <div className="flex items-start justify-between gap-2 p-3">
                                    <div className="min-w-0 space-y-1 pt-1">
                                        <p className="font-medium leading-5 wrap-anywhere">
                                            {description}
                                        </p>
                                        {!expanded && editable && (
                                            <p className="text-xs leading-5 text-muted-foreground">
                                                {grant.actions.length ===
                                                business.actions.length
                                                    ? `适用于全部 ${grant.actions.length} 项操作`
                                                    : grant.actions
                                                          .map(actionLabel)
                                                          .join("、") ||
                                                      "尚未选择操作"}
                                            </p>
                                        )}
                                    </div>
                                    <div className="flex shrink-0 items-center gap-1">
                                        {editable && (
                                            <Button
                                                id={`${id}-edit`}
                                                type="button"
                                                variant="ghost"
                                                size="sm"
                                                aria-expanded={expanded}
                                                aria-controls={
                                                    expanded
                                                        ? `${id}-body`
                                                        : undefined
                                                }
                                                onClick={() => onEdit(grant)}
                                            >
                                                {expanded ? "收起" : "编辑"}
                                                {expanded ? (
                                                    <ChevronUpIcon
                                                        data-icon="inline-end"
                                                        aria-hidden
                                                    />
                                                ) : (
                                                    <ChevronDownIcon
                                                        data-icon="inline-end"
                                                        aria-hidden
                                                    />
                                                )}
                                            </Button>
                                        )}
                                        <Button
                                            id={`${id}-remove`}
                                            type="button"
                                            variant="ghost"
                                            size="icon-sm"
                                            aria-label={`移除${description}`}
                                            onClick={() => onRemove(grant.key)}
                                        >
                                            <Trash2Icon
                                                className="text-muted-foreground"
                                                aria-hidden
                                            />
                                        </Button>
                                    </div>
                                </div>
                                {(expanded || !editable) && (
                                    <div
                                        id={`${id}-body`}
                                        className="space-y-4 border-t border-grid p-4"
                                    >
                                        {expanded && value.editor && (
                                            <PersonalGrantEditor
                                                initial={value.editor}
                                                business={business}
                                                units={units}
                                                name={name}
                                                idPrefix={id}
                                                onChange={onEditorChange}
                                            />
                                        )}
                                        <fieldset className="border-t border-grid pt-3">
                                            <legend className="sr-only">
                                                适用操作
                                            </legend>
                                            <p
                                                className="mb-2 text-xs font-medium text-muted-foreground"
                                                aria-hidden
                                            >
                                                适用操作
                                            </p>
                                            <div className="grid gap-2 sm:grid-cols-2">
                                                {business.actions.map(
                                                    (action) => (
                                                        <label
                                                            key={action}
                                                            htmlFor={`${id}-action-${toAutomationIdSegment(action)}`}
                                                            className="flex min-h-8 cursor-pointer items-center gap-2 text-sm"
                                                        >
                                                            <Checkbox
                                                                id={`${id}-action-${toAutomationIdSegment(action)}`}
                                                                checked={grant.actions.includes(
                                                                    action,
                                                                )}
                                                                onCheckedChange={(
                                                                    checked,
                                                                ) =>
                                                                    onActionsChange(
                                                                        grant.key,
                                                                        checked
                                                                            ? [
                                                                                  ...grant.actions,
                                                                                  action,
                                                                              ]
                                                                            : grant.actions.filter(
                                                                                  (
                                                                                      item,
                                                                                  ) =>
                                                                                      item !==
                                                                                      action,
                                                                              ),
                                                                    )
                                                                }
                                                            />
                                                            {actionLabel(
                                                                action,
                                                            )}
                                                        </label>
                                                    ),
                                                )}
                                            </div>
                                        </fieldset>
                                        {!grant.actions.length && (
                                            <p
                                                role="status"
                                                className="text-xs text-destructive"
                                            >
                                                请至少选择一项适用操作。
                                            </p>
                                        )}
                                        {!editable && (
                                            <p className="text-xs leading-5 text-muted-foreground">
                                                保留的原授权条件。如需调整，请移除此条并添加新范围。
                                            </p>
                                        )}
                                    </div>
                                )}
                            </div>
                        )
                    })}
                </div>
            )}
            {value.grants.length >= 32 && (
                <p
                    id="person-scope-grant-limit"
                    className="text-xs text-muted-foreground"
                >
                    最多保留 32 条追加授权，请合并或移除部分范围后再添加。
                </p>
            )}
        </section>
    )
}
