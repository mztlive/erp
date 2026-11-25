"use client"

import { PlusIcon } from "lucide-react"

import { MoneyValue } from "@/components/business"
import { Button } from "@/components/ui/button"
import { StatusBadge } from "@/components/ui/status-badge"
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
        <section className="erp-raised-surface space-y-3 rounded-lg border bg-card p-4">
            <h3 className="font-semibold">
                待核销应收账款
                <span className="ml-2 text-xs font-normal text-muted-foreground">
                    仅 {session.counterpartyPartyName}
                </span>
            </h3>
            <ul className="max-h-72 space-y-2 overflow-auto">
                {session.pool.length === 0 ? (
                    <li className="text-body-compact text-muted-foreground">
                        该结算公司下暂无待核销的应收账款
                    </li>
                ) : (
                    session.pool.map((t) => {
                        const selected = allocations.some(
                            (a) => a.targetId === t.targetId,
                        )
                        return (
                            <li
                                key={t.targetId}
                                className="flex items-center justify-between gap-2 rounded-lg border px-3 py-2"
                            >
                                <div className="min-w-0">
                                    <div className="truncate text-body-compact font-medium">
                                        {t.label}
                                    </div>
                                    <div className="text-xs text-muted-foreground">
                                        待核销余额{" "}
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
                                    <StatusBadge
                                        tone="success"
                                        label="已加入"
                                    />
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
