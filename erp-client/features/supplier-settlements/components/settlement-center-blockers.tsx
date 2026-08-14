"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

const CENTER_BLOCKER_ACTIONS = [
    "CONFIRM",
    "REVIEW_DECISION",
    "SUBMIT_REVIEW",
    "SOD_VIOLATION",
]

function isCenterBlocker(b: { action: string; code: string }): boolean {
    return (
        CENTER_BLOCKER_ACTIONS.includes(b.action) ||
        b.code === "SOD_VIOLATION" ||
        b.code === "BLOCKING_DIFFERENCES"
    )
}

function SettlementCenterBlockersAlert({
    blockers,
}: {
    blockers: Array<{ action: string; code: string; message: string }>
}) {
    const visible = blockers.filter(isCenterBlocker)
    if (visible.length === 0) return null
    return (
        <Alert variant="warning">
            <AlertTitle>动作门禁</AlertTitle>
            <AlertDescription>
                <ul className="list-inside list-disc text-sm">
                    {visible.map((b) => (
                        <li key={`${b.action}-${b.code}`}>{b.message}</li>
                    ))}
                </ul>
            </AlertDescription>
        </Alert>
    )
}

export { SettlementCenterBlockersAlert }
