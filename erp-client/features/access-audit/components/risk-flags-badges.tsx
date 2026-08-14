"use client"

import { Badge } from "@/components/ui/badge"
import { riskLabel } from "@/features/access-audit/lib/risk-labels"

function RiskFlagsBadges({ flags }: { flags: readonly string[] }) {
    return flags.length ? (
        <div className="flex flex-wrap gap-1">
            {flags.map((f) => (
                <Badge key={f} variant="warning">
                    {riskLabel(f)}
                </Badge>
            ))}
        </div>
    ) : (
        "—"
    )
}

export { RiskFlagsBadges }
