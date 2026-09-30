"use client"

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
    return (
        <section className="space-y-3" aria-label="追加授权">
            <div className="flex flex-wrap items-center justify-between gap-2">
                <h3 className="font-medium">
                    追加授权{" "}
                    <span className="ml-1 text-xs font-normal text-muted-foreground">
                        {value.grants.length} 条
                    </span>
                </h3>
            </div>
            {!value.grants.length ? (
                <p className="py-5 text-center text-muted-foreground">
                    {business.default_self
                        ? "尚未追加范围，当前仅包含本人负责的数据。"
                        : "尚未授权，请添加可访问的数据范围。"}
                </p>
            ) : (
                <div className="divide-y border-y">
                    {value.grants.map((grant) => {
                        const id = `person-scope-grant-${grantAutomationKey(grant.key)}`
                        const expanded = value.editor?.key === grant.key
                        const editable =
                            canEditGrant(grant, business) ||
                            Boolean(grant.editor) ||
                            expanded
                        return (
                            <div key={grant.key} className="min-w-0 py-3">
                                <div className="flex items-start justify-between gap-3">
                                    <p className="min-w-0 break-words pt-1 font-medium leading-6">
                                        {incompleteKeys.has(grant.key)
                                            ? "待完善的授权范围"
                                            : personTermsDescription(
                                                  grant.terms,
                                                  labels,
                                              )}
                                    </p>
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
                                                        ? `${id}-fields`
                                                        : undefined
                                                }
                                                onClick={() => onEdit(grant)}
                                            >
                                                {expanded ? "收起" : "编辑"}
                                                {expanded ? (
                                                    <ChevronUpIcon data-icon="inline-end" />
                                                ) : (
                                                    <ChevronDownIcon data-icon="inline-end" />
                                                )}
                                            </Button>
                                        )}
                                        <Button
                                            id={`${id}-remove`}
                                            type="button"
                                            variant="ghost"
                                            size="icon-sm"
                                            aria-label={`移除${personTermsDescription(grant.terms, labels)}`}
                                            onClick={() => onRemove(grant.key)}
                                        >
                                            <Trash2Icon />
                                        </Button>
                                    </div>
                                </div>
                                <fieldset className="mt-2 flex flex-wrap items-center gap-x-5 gap-y-2">
                                    <legend className="sr-only">
                                        适用操作
                                    </legend>
                                    <span
                                        className="text-xs text-muted-foreground"
                                        aria-hidden
                                    >
                                        适用操作
                                    </span>
                                    {business.actions.map((action) => (
                                        <label
                                            key={action}
                                            htmlFor={`${id}-action-${toAutomationIdSegment(action)}`}
                                            className="flex items-center gap-2 text-xs"
                                        >
                                            <Checkbox
                                                id={`${id}-action-${toAutomationIdSegment(action)}`}
                                                checked={grant.actions.includes(
                                                    action,
                                                )}
                                                onCheckedChange={(checked) =>
                                                    onActionsChange(
                                                        grant.key,
                                                        checked
                                                            ? [
                                                                  ...grant.actions,
                                                                  action,
                                                              ]
                                                            : grant.actions.filter(
                                                                  (item) =>
                                                                      item !==
                                                                      action,
                                                              ),
                                                    )
                                                }
                                            />
                                            {actionLabel(action)}
                                        </label>
                                    ))}
                                </fieldset>
                                {!grant.actions.length && (
                                    <p
                                        role="status"
                                        className="mt-2 text-xs text-destructive"
                                    >
                                        请至少选择一项适用操作，或移除此条范围。
                                    </p>
                                )}
                                {!editable && (
                                    <p className="mt-2 text-xs text-muted-foreground">
                                        保留的原授权条件。如需调整，请移除此条并添加新范围。
                                    </p>
                                )}
                                {expanded && value.editor && (
                                    <div className="mt-3">
                                        <PersonalGrantEditor
                                            initial={value.editor}
                                            business={business}
                                            units={units}
                                            name={name}
                                            idPrefix={id}
                                            onChange={onEditorChange}
                                        />
                                    </div>
                                )}
                            </div>
                        )
                    })}
                </div>
            )}
            <Button
                id="person-scope-add"
                type="button"
                variant="outline"
                size="sm"
                className="w-full border-dashed"
                disabled={value.grants.length >= 32}
                aria-describedby={
                    value.grants.length >= 32
                        ? "person-scope-grant-limit"
                        : undefined
                }
                onClick={onAdd}
            >
                <PlusIcon data-icon="inline-start" />
                添加范围
            </Button>
            {incompleteKeys.size > 0 && (
                <p
                    id="person-scope-editor-hint"
                    className="text-xs text-muted-foreground"
                >
                    未完善的范围会保留在草稿中，请填写完整后统一保存。
                </p>
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
