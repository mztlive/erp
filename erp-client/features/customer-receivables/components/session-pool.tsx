"use client"

import { PlusIcon } from "lucide-react"

import { MoneyValue } from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import type {
    AllocationDraftLine,
    AllocationSessionView,
    AllocationTarget,
} from "@/features/customer-receivables/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function SessionPool({
    session,
    allocations,
    disabled,
    onAdd,
}: {
    session: AllocationSessionView
    allocations: readonly AllocationDraftLine[]
    disabled: boolean
    onAdd: (target: AllocationTarget) => void
}) {
    return (
        <section className="erp-raised-surface space-y-3 rounded-2xl border bg-card p-4">
            <h3 className="text-sm font-semibold">
                同主体待核销池
                <span className="ml-2 text-xs font-normal text-muted-foreground">
                    仅 {session.counterpartyPartyName}
                </span>
            </h3>
            <p className="text-xs text-muted-foreground">
                仅同主体的开放应收可分配；跨主体即使同名客户也不返回。
            </p>
            <ul className="max-h-72 space-y-2 overflow-auto">
                {session.pool.length === 0 ? (
                    <li className="text-sm text-muted-foreground">
                        当前主体无开放目标
                    </li>
                ) : (
                    session.pool.map((t) => {
                        const selected = allocations.some(
                            (a) => a.targetId === t.targetId,
                        )
                        return (
                            <li
                                key={t.targetId}
                                className="flex items-center justify-between gap-2 rounded-xl border px-3 py-2"
                            >
                                <div className="min-w-0">
                                    <div className="truncate text-sm font-medium">
                                        {t.label}
                                    </div>
                                    <div className="text-xs text-muted-foreground">
                                        开放{" "}
                                        <MoneyValue
                                            value={t.openAmount}
                                            taxBasis="gross"
                                        />
                                        {t.dueDate
                                            ? ` · 到期 ${t.dueDate}`
                                            : null}
                                    </div>
                                </div>
                                {selected ? (
                                    <Badge variant="success">已加入</Badge>
                                ) : (
                                    <Button
                                        id={`customer-receivables-session-pool-${toAutomationIdSegment(t.targetId)}-add`}
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        disabled={disabled}
                                        onClick={() => onAdd(t)}
                                    >
                                        <PlusIcon
                                            data-icon="inline-start"
                                            aria-hidden="true"
                                        />
                                        加入
                                    </Button>
                                )}
                            </li>
                        )
                    })
                )}
            </ul>
        </section>
    )
}
