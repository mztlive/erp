import { z } from "zod"
import type {
    AllocationSessionView,
    PostAllocationInput,
    SaveAllocationDraftInput,
} from "../types"

const line = z.object({
    lineKey: z.string(),
    targetId: z.string(),
    targetKind: z.enum(["receivable_entry", "receivable_account"]),
    label: z.string(),
    salesOrderNo: z.string(),
    openAmount: z.string(),
    amount: z.string(),
    baselineVersion: z.number(),
})
const fact = z.object({
    receivedAt: z.string().optional(),
    amount: z.string().optional(),
    bankReference: z.string().optional(),
    receiptFileNames: z.array(z.string()).optional(),
})
const sessionSchema = z
    .object({
        draftSessionId: z.string(),
        ownerId: z.string(),
        mode: z.literal("receipt"),
        counterpartyPartyId: z.string(),
        counterpartyPartyName: z.string(),
        customerId: z.string(),
        customerName: z.string(),
        status: z.enum(["draft", "posted"]),
        fact,
        allocations: z.array(line),
        pool: z.array(
            z
                .object({
                    targetId: z.string(),
                    targetKind: z.enum([
                        "receivable_entry",
                        "receivable_account",
                    ]),
                    label: z.string(),
                    salesOrderId: z.string(),
                    salesOrderNo: z.string(),
                    openAmount: z.string(),
                    counterpartyPartyId: z.string(),
                    baselineVersion: z.number(),
                })
                .passthrough(),
        ),
        existingFactId: z.string().optional(),
        existingFactNo: z.string().optional(),
        existingFactVersion: z.number().optional(),
        receiptCustomerId: z.string().nullable().optional(),
        editVersion: z.number(),
        factAmount: z.string(),
        proposedAllocatedTotal: z.string(),
        proposedUnallocated: z.string(),
        submitPolicy: z.object({
            allowUnallocatedRemainder: z.boolean(),
            label: z.string(),
        }),
        leaseValid: z.boolean(),
        note: z.string(),
        returnContext: z
            .object({
                returnTo: z.string().optional(),
                from: z.string().optional(),
                salesOrderId: z.string().optional(),
                receivableAccountId: z.string().optional(),
            })
            .optional(),
        savedAt: z.string().optional(),
    })
    .passthrough()
const inputSchema = z.object({
    draftSessionId: z.string(),
    editVersion: z.number(),
    idempotencyKey: z.string(),
    fact,
    allocations: z.array(line),
})
const recordSchema = z.object({
    version: z.literal(1),
    session: sessionSchema,
    pending: z
        .object({
            kind: z.enum(["save", "post"]),
            session: sessionSchema,
            input: inputSchema,
        })
        .optional(),
})
export type ReceiptPendingCommand = {
    kind: "save" | "post"
    session: AllocationSessionView
    input: PostAllocationInput
}
export type ReceiptSessionRecord = {
    version: 1
    session: AllocationSessionView
    pending?: ReceiptPendingCommand
}
const prefix = "erp.receipt-draft.v1:"
function key(owner: string, id: string) {
    return `${prefix}${encodeURIComponent(owner)}:${encodeURIComponent(id)}`
}

export function readReceiptSession(
    owner: string,
    id: string,
): ReceiptSessionRecord | null {
    const raw = sessionStorage.getItem(key(owner, id))
    if (!raw) return null
    let parsed: z.infer<typeof recordSchema>
    try {
        parsed = recordSchema.parse(JSON.parse(raw))
    } catch {
        throw new Error(
            "本页暂存内容无法读取，请返回列表重新打开已保存的回款草稿。",
        )
    }
    if (
        parsed.session.ownerId !== owner ||
        parsed.session.draftSessionId !== id ||
        (parsed.pending &&
            (parsed.pending.session.ownerId !== owner ||
                parsed.pending.input.draftSessionId !== id))
    ) {
        throw new Error("本页暂存内容与当前账号或回款不一致。")
    }
    return parsed as ReceiptSessionRecord
}

/** 浏览器写入失败时抛错；提交前必须成功保留原命令。 */
export function writeReceiptSession(
    session: AllocationSessionView,
    pending?: ReceiptPendingCommand | null,
) {
    if (session.mode !== "receipt") return
    if (!session.ownerId) throw new Error("当前账号尚未加载，请稍后重试。")
    const current = readReceiptSession(session.ownerId, session.draftSessionId)
    const record: ReceiptSessionRecord = {
        version: 1,
        session,
        pending:
            pending === undefined ? current?.pending : (pending ?? undefined),
    }
    try {
        sessionStorage.setItem(
            key(session.ownerId, session.draftSessionId),
            JSON.stringify(record, (field, value) =>
                field === "receiptFiles" ? undefined : value,
            ),
        )
    } catch {
        throw new Error("浏览器无法暂存本次回款，请检查存储空间后重试。")
    }
}

export function removeReceiptSession(session: AllocationSessionView) {
    if (session.ownerId)
        sessionStorage.removeItem(key(session.ownerId, session.draftSessionId))
}

export function prepareReceiptCommand(
    session: AllocationSessionView,
    kind: "save" | "post",
    input: SaveAllocationDraftInput | PostAllocationInput,
): ReceiptPendingCommand {
    if (!session.ownerId) throw new Error("当前账号尚未加载，请稍后重试。")
    const pending = readReceiptSession(
        session.ownerId,
        session.draftSessionId,
    )?.pending
    if (pending) {
        if (pending.kind !== kind) throw new Error("请先核对上次操作结果。")
        return pending
    }
    const command: ReceiptPendingCommand = {
        kind,
        session,
        input: {
            ...input,
            idempotencyKey:
                "idempotencyKey" in input
                    ? input.idempotencyKey
                    : crypto.randomUUID(),
        },
    }
    writeReceiptSession(
        { ...session, fact: input.fact, allocations: input.allocations },
        command,
    )
    return command
}
