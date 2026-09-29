"use client"

import {
    scopeDescription,
    scopeCapability,
    type ScopeRule,
} from "../lib/scope-description"
import type { OrgUnit } from "../types"

export function ScopeRulesView({
    rules,
    roles = [],
    units = [],
}: {
    rules: readonly ScopeRule[]
    roles?: readonly { id: string; name: string }[]
    units?: readonly OrgUnit[]
}) {
    return (
        <div className="space-y-5">
            {(["role", "user"] as const).map((subject) => {
                const rows = rules.filter(
                    (rule) => rule.subject_type === subject,
                )
                if (!rows.length) return null
                return (
                    <section key={subject} className="space-y-2">
                        <h3 className="font-medium">
                            {subject === "role"
                                ? "角色提供的数据范围"
                                : "个人范围限制"}
                        </h3>
                        <p className="text-xs text-muted-foreground">
                            {subject === "role"
                                ? "同一角色必须同时提供操作权限和对应范围；不同动作分别计算。"
                                : "仅收窄角色授权及合法历史读取，不提供额外权限。"}
                        </p>
                        <ul className="divide-y rounded-lg border px-4">
                            {rows.map((rule) => (
                                <li
                                    key={rule.id}
                                    className="space-y-1 py-3 text-sm"
                                >
                                    <p className="font-medium">
                                        {scopeCapability(rule)}
                                    </p>
                                    <p>{scopeDescription(rule, units)}</p>
                                    <p className="text-xs text-muted-foreground">
                                        {subject === "role"
                                            ? `来源：${roles.find((role) => role.id === rule.subject_id)?.name ?? "角色信息待确认"}`
                                            : "个人限制 · 与角色授权求交"}
                                    </p>
                                </li>
                            ))}
                        </ul>
                    </section>
                )
            })}
        </div>
    )
}
