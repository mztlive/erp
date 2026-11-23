/**
 * 轻量工作台队列 ID 序列缓存（`sessionStorage`）。
 * 列表页打开独立详情页签前写入当前页 ID 序列，详情页据此即时呈现「上一项 / 下一项」
 * 与 `autoNext` 连续流转，避免新开页签后重复阻塞等待完整队列列表查询。
 */

export type WorkspaceQueueSequenceKind = "tasks" | "approvals" | "copies"

type QueueSequenceEntry = Readonly<{
    userId: string
    kind: WorkspaceQueueSequenceKind
    returnTo: string
    ids: readonly string[]
    updatedAt: number
}>

const STORAGE_KEY = "erp.workspace.queueSequence"
const MAX_ENTRIES = 8
const MAX_IDS_PER_ENTRY = 200
const SEQUENCE_TTL_MS = 15 * 60 * 1000

function normalizeUserId(userId: string | null | undefined): string {
    return typeof userId === "string" ? userId.trim() : ""
}

function normalizeReturnTo(returnTo: string): string {
    const trimmed = returnTo.trim()
    if (!trimmed) return ""
    try {
        const url = new URL(trimmed, "https://workspace.invalid")
        url.searchParams.delete("_workspace_frame")
        const search = url.searchParams.toString()
        return `${url.pathname}${search ? `?${search}` : ""}${url.hash}`
    } catch {
        return trimmed
    }
}

function readEntries(): QueueSequenceEntry[] {
    if (typeof window === "undefined") return []
    try {
        const raw = window.sessionStorage.getItem(STORAGE_KEY)
        if (!raw) return []
        const parsed = JSON.parse(raw)
        if (!Array.isArray(parsed)) return []
        const now = Date.now()
        return parsed.filter(
            (entry): entry is QueueSequenceEntry =>
                Boolean(entry) &&
                typeof entry === "object" &&
                typeof entry.userId === "string" &&
                entry.userId.trim().length > 0 &&
                (entry.kind === "tasks" ||
                    entry.kind === "approvals" ||
                    entry.kind === "copies") &&
                typeof entry.returnTo === "string" &&
                Array.isArray(entry.ids) &&
                entry.ids.every((id: unknown) => typeof id === "string") &&
                typeof entry.updatedAt === "number" &&
                now - entry.updatedAt <= SEQUENCE_TTL_MS,
        )
    } catch {
        return []
    }
}

function writeEntries(entries: readonly QueueSequenceEntry[]): void {
    if (typeof window === "undefined") return
    try {
        window.sessionStorage.setItem(
            STORAGE_KEY,
            JSON.stringify(entries.slice(0, MAX_ENTRIES)),
        )
    } catch {
        // 隐私模式或存储配额受限时静默降级为接口查询
    }
}

/** 按当前账号暂存队列可见条目的稳定 ID 序列，供独立详情页即时恢复上下项导航。 */
export function stashWorkspaceQueueSequence(
    userId: string | null | undefined,
    kind: WorkspaceQueueSequenceKind,
    returnTo: string,
    ids: readonly string[],
): void {
    const normalizedUserId = normalizeUserId(userId)
    const normalizedReturnTo = normalizeReturnTo(returnTo)
    if (!normalizedUserId || !normalizedReturnTo || ids.length === 0) return
    const uniqueIds = [...new Set(ids.filter(Boolean))].slice(
        0,
        MAX_IDS_PER_ENTRY,
    )
    if (uniqueIds.length === 0) return
    const rest = readEntries().filter(
        (entry) =>
            entry.userId === normalizedUserId &&
            !(entry.kind === kind && entry.returnTo === normalizedReturnTo),
    )
    writeEntries([
        {
            userId: normalizedUserId,
            kind,
            returnTo: normalizedReturnTo,
            ids: uniqueIds,
            updatedAt: Date.now(),
        },
        ...rest,
    ])
}

/** 按账号、队列类型与 `returnTo` 读取尚在有效期内的轻量 ID 序列。 */
export function readWorkspaceQueueSequence(
    userId: string | null | undefined,
    kind: WorkspaceQueueSequenceKind,
    returnTo: string,
): readonly string[] | null {
    const normalizedUserId = normalizeUserId(userId)
    const normalizedReturnTo = normalizeReturnTo(returnTo)
    if (!normalizedUserId || !normalizedReturnTo) return null
    const matched = readEntries().find(
        (entry) =>
            entry.userId === normalizedUserId &&
            entry.kind === kind &&
            entry.returnTo === normalizedReturnTo,
    )
    return matched && matched.ids.length > 0 ? matched.ids : null
}

/** 完成/处理某条记录后，将其从对应账号序列中移除并保留顺序，避免连续办理回到已处理项。 */
export function removeFromWorkspaceQueueSequence(
    userId: string | null | undefined,
    kind: WorkspaceQueueSequenceKind,
    returnTo: string,
    completedId: string,
): readonly string[] | null {
    const normalizedUserId = normalizeUserId(userId)
    const normalizedReturnTo = normalizeReturnTo(returnTo)
    if (!normalizedUserId || !normalizedReturnTo || !completedId) return null
    const entries = readEntries()
    let updatedIds: readonly string[] | null = null
    const nextEntries = entries
        .map((entry) => {
            if (
                entry.userId !== normalizedUserId ||
                entry.kind !== kind ||
                entry.returnTo !== normalizedReturnTo
            ) {
                return entry
            }
            const remaining = entry.ids.filter((id) => id !== completedId)
            updatedIds = remaining
            return {
                ...entry,
                ids: remaining,
                updatedAt: Date.now(),
            }
        })
        .filter((entry) => entry.ids.length > 0)
    writeEntries(nextEntries)
    return updatedIds
}
