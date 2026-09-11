import type { ApiError } from "@/lib/api"
import type { PublicChoiceView } from "../types"

export type Pick = { selected: boolean; quantity: string }
export type Picks = Record<string, Pick>
export type PendingSelectionRequest =
    | {
          kind: "save"
          confirm: boolean
          input: {
              idempotencyKey: string
              expectedSessionVersion: number
              choices: { item_id: string; quantity?: number }[]
          }
      }
    | {
          kind: "submit"
          input: { idempotencyKey: string; expectedSessionVersion: number }
      }

/** 从已保存清单恢复表单选择与份数。 */
export const picksFromChoices = (choices: PublicChoiceView[]): Picks => {
    return Object.fromEntries(
        choices.map((choice) => [
            choice.item_id,
            { selected: true, quantity: String(choice.quantity ?? 1) },
        ]),
    )
}

/** 区分失效、冲突、确定失败和结果未知，决定恢复路径。 */
export const requestFailure = (
    error: unknown,
): "ended" | "conflict" | "unknown" | "invalid" => {
    const value = error as Partial<ApiError> | undefined
    if (
        value?.status === 404 ||
        value?.status === 403 ||
        value?.code === "SELECTION_ENDED"
    )
        return "ended"
    if (value?.status === 409) return "conflict"
    if (
        !value ||
        value.kind === "Network" ||
        value.kind === "Parse" ||
        (value.status ?? 500) >= 500
    )
        return "unknown"
    return "invalid"
}

/** 恢复缓存只接受已知请求形态，防止损坏缓存误发请求。 */
export const readPendingRequest = (
    key: string,
): PendingSelectionRequest | null => {
    if (typeof window === "undefined") return null
    try {
        const value = JSON.parse(
            sessionStorage.getItem(key) ?? "null",
        ) as PendingSelectionRequest | null
        if (
            !value ||
            !["save", "submit"].includes(value.kind) ||
            typeof value.input?.idempotencyKey !== "string" ||
            !Number.isSafeInteger(value.input.expectedSessionVersion)
        )
            return null
        if (
            value.kind === "save" &&
            (!Array.isArray(value.input.choices) ||
                value.input.choices.some(
                    (item) => typeof item.item_id !== "string",
                ))
        )
            return null
        return value
    } catch {
        return null
    }
}
