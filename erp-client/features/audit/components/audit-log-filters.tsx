"use client"

import { useEffect, useRef } from "react"
import { useStore } from "@tanstack/react-form"
import { OptionCombobox } from "@/components/business"
import { useAppForm } from "@/components/form"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
    listWorkspaceFilterStatusText,
} from "@/components/business/list-workspace"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { useSearchHotkey } from "@/lib/use-search-hotkey"
import { BUSINESS_AUDIT_ACTIONS, BUSINESS_AUDIT_RESULTS } from "../lib/display"

export type AuditLogFilters = {
    actorAccount: string
    action: string
    eventResult: string
    resourceNumber: string
}

export function AuditLogFilterBar({
    applied,
    onApply,
    onClearAll,
    loading,
    failed,
}: {
    applied: AuditLogFilters
    onApply: (values: AuditLogFilters) => void
    onClearAll: () => void
    loading: boolean
    failed: boolean
}) {
    const form = useAppForm({
        defaultValues: applied,
        onSubmit: ({ value }) => onApply(value),
    })
    useEffect(() => {
        form.reset(applied)
    }, [applied, form])

    const searchInputRef = useRef<HTMLInputElement>(null)
    useSearchHotkey(searchInputRef)
    const hasPendingChanges = useStore(
        form.store,
        ({ values }) =>
            values.resourceNumber.trim() !== applied.resourceNumber.trim() ||
            values.actorAccount.trim() !== applied.actorAccount.trim() ||
            values.action !== applied.action ||
            values.eventResult !== applied.eventResult,
    )

    const actions = BUSINESS_AUDIT_ACTIONS.some(
        (item) => item.value === applied.action,
    )
        ? BUSINESS_AUDIT_ACTIONS
        : applied.action
          ? [
                ...BUSINESS_AUDIT_ACTIONS,
                { value: applied.action, label: "未登记动作", disabled: true },
            ]
          : BUSINESS_AUDIT_ACTIONS
    // 主栏显示待查询草稿；标签保留当前生效条件和即时移除入口。
    const chips = [
        ...(applied.resourceNumber
            ? [
                  {
                      key: "resourceNumber",
                      label: `业务编号：${applied.resourceNumber}`,
                  },
              ]
            : []),
        ...(applied.actorAccount
            ? [
                  {
                      key: "actorAccount",
                      label: `操作账号：${applied.actorAccount}`,
                  },
              ]
            : []),
        ...(applied.action
            ? [
                  {
                      key: "action",
                      label: `动作：${actions.find((item) => item.value === applied.action)?.label ?? "未登记动作"}`,
                  },
              ]
            : []),
        ...(applied.eventResult
            ? [
                  {
                      key: "eventResult",
                      label: `执行结果：${BUSINESS_AUDIT_RESULTS.find((item) => item.value === applied.eventResult)?.label ?? "未登记结果"}`,
                  },
              ]
            : []),
    ]

    return (
        <ListWorkspaceFilterBar
            idPrefix="business-audit-filters"
            formAriaLabel="业务操作查询条件"
            morePresentation="popover"
            onSubmit={() => void form.handleSubmit()}
            resultStatus={
                loading || failed
                    ? listWorkspaceFilterStatusText({
                          loading: loading && !failed,
                          failed,
                          noun: "条记录",
                      })
                    : null
            }
            chips={chips}
            onClearChip={(key) => onApply({ ...applied, [key]: "" })}
            onClearAll={onClearAll}
            hasPendingChanges={hasPendingChanges}
            search={
                <form.Field name="resourceNumber">
                    {(field) => (
                        <ListSearchField
                            id="business-audit-filters-resource-number"
                            searchInputRef={searchInputRef}
                            value={field.state.value}
                            onChange={field.handleChange}
                            aria-label="业务编号"
                            placeholder="按业务编号查询"
                        />
                    )}
                </form.Field>
            }
            primaryFilters={
                <>
                    <form.Field name="actorAccount">
                        {(field) => (
                            <div
                                className="w-40 max-w-full min-w-0"
                                data-has-value={
                                    field.state.value.trim()
                                        ? "true"
                                        : undefined
                                }
                            >
                                <InputGroup>
                                    <InputGroupAddon
                                        data-filter-label=""
                                        className="shrink-0 pr-0 font-normal whitespace-nowrap text-muted-foreground/80"
                                    >
                                        操作账号：
                                    </InputGroupAddon>
                                    <InputGroupInput
                                        id="business-audit-filters-actor-account"
                                        name={field.name}
                                        aria-label="操作账号"
                                        autoComplete="off"
                                        value={field.state.value}
                                        onBlur={field.handleBlur}
                                        onChange={(event) =>
                                            field.handleChange(
                                                event.target.value,
                                            )
                                        }
                                        placeholder="全部"
                                    />
                                </InputGroup>
                            </div>
                        )}
                    </form.Field>
                    <form.Field name="action">
                        {(field) => (
                            <OptionCombobox
                                id="business-audit-filters-action"
                                className="w-48 max-w-full min-w-0"
                                filterLabel="业务动作"
                                aria-label="业务动作"
                                placeholder="全部"
                                value={field.state.value || null}
                                onValueChange={(value) =>
                                    field.handleChange(value ?? "")
                                }
                                onBlur={field.handleBlur}
                                options={actions}
                            />
                        )}
                    </form.Field>
                    <form.Field name="eventResult">
                        {(field) => (
                            <OptionCombobox
                                id="business-audit-filters-result"
                                className="w-44 max-w-full min-w-0"
                                filterLabel="执行结果"
                                aria-label="执行结果"
                                placeholder="全部"
                                value={field.state.value || null}
                                onValueChange={(value) =>
                                    field.handleChange(value ?? "")
                                }
                                onBlur={field.handleBlur}
                                options={BUSINESS_AUDIT_RESULTS}
                            />
                        )}
                    </form.Field>
                </>
            }
        />
    )
}
