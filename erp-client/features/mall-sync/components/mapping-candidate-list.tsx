"use client"

import { Badge } from "@/components/ui/badge"
import type { MappingTaskView } from "@/features/mall-sync/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

type MappingCandidateListProps = {
    candidates: MappingTaskView["candidateTargets"]
    selectedCandidateId: string | null
    disabled: boolean
    onSelectCandidate: (candidateId: string) => void
}

export function MappingCandidateList({
    candidates,
    selectedCandidateId,
    disabled,
    onSelectCandidate,
}: MappingCandidateListProps) {
    return (
        <div>
            <h4 className="mb-2 text-sm font-semibold">ERP 候选</h4>
            <ul className="space-y-2">
                {candidates.map((c) => (
                    <li key={c.objectId}>
                        <button
                            id={`mall-sync-candidate-${toAutomationIdSegment(c.objectId)}`}
                            type="button"
                            disabled={c.eligibility !== "ELIGIBLE" || disabled}
                            onClick={() => onSelectCandidate(c.objectId)}
                            className={cn(
                                "w-full rounded-lg border px-3 py-2 text-left text-sm transition-colors",
                                selectedCandidateId === c.objectId
                                    ? "border-primary bg-accent"
                                    : "hover:bg-muted/60",
                                c.eligibility !== "ELIGIBLE" && "opacity-60",
                            )}
                        >
                            <div className="flex items-center justify-between gap-2">
                                <span className="font-medium">
                                    {c.stableNo}
                                </span>
                                <Badge
                                    variant={
                                        c.eligibility === "ELIGIBLE"
                                            ? "secondary"
                                            : "outline"
                                    }
                                >
                                    {c.eligibility === "ELIGIBLE"
                                        ? "可选"
                                        : "不可用"}
                                </Badge>
                            </div>
                            <p>{c.label}</p>
                            <p className="text-xs text-muted-foreground">
                                {c.reason}
                            </p>
                        </button>
                    </li>
                ))}
            </ul>
        </div>
    )
}
