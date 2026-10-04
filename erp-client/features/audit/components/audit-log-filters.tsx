"use client"

import { useEffect } from "react"
import { useAppForm } from "@/components/form"
import {
    ListSearchField,
    ListWorkspaceFilterBar,
} from "@/components/business/list-workspace"
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
    total,
    loading,
}: {
    applied: AuditLogFilters
    onApply: (values: AuditLogFilters) => void
    total: number
    loading: boolean
}) {
    const form = useAppForm({
        defaultValues: applied,
        onSubmit: ({ value }) => onApply(value),
    })
    useEffect(() => {
        form.reset(applied)
    }, [applied, form])

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
            density="compact"
            onSubmit={() => void form.handleSubmit()}
            resultStatus={loading ? "正在查询…" : `共 ${total} 条记录`}
            chips={chips}
            onClearChip={(key) => onApply({ ...applied, [key]: "" })}
            onClearAll={() =>
                onApply({
                    actorAccount: "",
                    action: "",
                    eventResult: "",
                    resourceNumber: "",
                })
            }
            search={
                <form.Field name="resourceNumber">
                    {(field) => (
                        <ListSearchField
                            id="business-audit-filters-resource-number"
                            value={field.state.value}
                            onChange={field.handleChange}
                            aria-label="业务编号"
                            placeholder="按业务编号查询"
                        />
                    )}
                </form.Field>
            }
            commonFilters={
                <div className="grid min-w-0 flex-1 grid-cols-1 gap-3 sm:grid-cols-3">
                    <form.AppField name="actorAccount">
                        {(field) => (
                            <field.TextField
                                id="business-audit-filters-actor-account"
                                label="操作账号"
                                placeholder="全部账号"
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="action">
                        {(field) => (
                            <field.SelectField
                                id="business-audit-filters-action"
                                label="业务动作"
                                placeholder="全部动作"
                                options={actions}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="eventResult">
                        {(field) => (
                            <field.SelectField
                                id="business-audit-filters-result"
                                label="执行结果"
                                placeholder="全部结果"
                                options={BUSINESS_AUDIT_RESULTS}
                            />
                        )}
                    </form.AppField>
                </div>
            }
        />
    )
}
