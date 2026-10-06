import { isApiError } from "./errors"

export type CommandFailureDisposition = "unknown" | "conflict" | "rejected"

/** 结果未知保留原命令；明确拒绝可修正；版本冲突须先人工核对最新状态。 */
export function commandFailureDisposition(
    error: unknown,
): CommandFailureDisposition {
    if (!isApiError(error)) return "unknown"
    if (
        error.code === "COMMIT_OUTCOME_UNKNOWN" ||
        error.code === "OUTCOME_UNKNOWN" ||
        error.kind === "Network" ||
        error.kind === "Parse"
    )
        return "unknown"
    if (error.status === 409) return "conflict"
    if (
        error.status != null &&
        error.status >= 400 &&
        error.status < 500 &&
        error.status !== 408
    )
        return "rejected"
    return "unknown"
}
