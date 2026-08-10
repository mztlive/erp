import { FormalActionResult } from "@/components/business"
import type { HistoryBackfillCommandResult } from "@/features/history-backfill/types"

export function HistoryBackfillResultBanner({
    result,
}: {
    result: HistoryBackfillCommandResult | null
}) {
    if (!result) return null
    const status =
        result.status === "COMMITTED"
            ? "succeeded"
            : result.status === "BLOCKED"
              ? "blocked"
              : result.status === "RESULT_UNKNOWN"
                ? "unknown"
                : "rejected"
    return (
        <FormalActionResult
            status={status}
            title={result.title}
            description={result.description}
            facts={[
                ...(result.jobNo
                    ? [{ label: "任务号", value: result.jobNo }]
                    : []),
                ...(result.nextStep
                    ? [{ label: "下一步", value: result.nextStep }]
                    : []),
            ]}
        />
    )
}
