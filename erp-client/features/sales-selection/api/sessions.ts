import type { PublicChoiceView } from "@/features/sales-selection/types"

/** 内部会话快照。 */
export type BookSessionView = Readonly<{
    book_id: string
    expected_version: number
    selections: readonly PublicChoiceView[]
    total_amount?: string | null
    updated_at: number
}>
