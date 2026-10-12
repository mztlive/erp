import { apiGet, apiPost } from "@/lib/api"
import { classifyFormalCommandError } from "@/lib/formal-command"
import { getErrorCode } from "@/lib/api/errors"
import { normalizeFixed } from "@/lib/fixed-decimal"
import type { AllocationSessionView, SaveAllocationDraftInput } from "../types"
import type { BackendCustomerReceipt } from "./dto"
import {
    buildPool,
    createAllocationSession,
    receiptReceivedAtInput,
} from "./session"
import { mapCustomerReceiptApproval } from "../lib/customer-receipt-approval"
import {
    prepareReceiptCommand,
    readReceiptSession,
    writeReceiptSession,
} from "../lib/receipt-session-storage"

export function isReceiptOutcomeUnknown(error: unknown) {
    return (
        getErrorCode(error) === "OUTCOME_UNKNOWN" ||
        classifyFormalCommandError(error) === "unknown"
    )
}

export async function restoreReceiptSession(
    owner: string,
    id: string,
): Promise<AllocationSessionView | null> {
    const stored = readReceiptSession(owner, id)
    let session = stored?.session
    if (!session) {
        if (id.startsWith("alloc_cust_")) return null
        const receipt = await apiGet<BackendCustomerReceipt>(
            `/admin/customer-receipts/${encodeURIComponent(id)}/draft`,
        )
        session = await createAllocationSession({
            mode: "receipt",
            counterpartyPartyId: receipt.counterparty_party_id,
            existingFactId: id,
        })
        session = { ...session, draftSessionId: id, ownerId: owner }
    } else if (session.existingFactId && !stored?.pending) {
        const latest = await createAllocationSession({
            mode: "receipt",
            counterpartyPartyId: session.counterpartyPartyId,
            existingFactId: session.existingFactId,
            ...session.returnContext,
        })
        if (latest.status !== "draft")
            return { ...latest, ownerId: owner, draftSessionId: id }
        // 并发修改不得把旧表单套在新版本上提交。
        if (latest.existingFactVersion !== session.existingFactVersion) {
            throw new Error(
                "该回款草稿已被其他页面修改，请返回列表重新打开；本页暂存内容仍保留。",
            )
        }
        session = {
            ...session,
            approval: latest.approval,
            serverStatus: latest.serverStatus,
        }
    }
    const { pool } = await buildPool(
        "receipt",
        session.counterpartyPartyId,
        session.returnContext,
    )
    return {
        ...session,
        pool,
        recoveryPending: stored?.pending?.kind,
        allocations: session.allocations.map((line) => {
            const current = pool.find(
                (target) =>
                    target.targetId === line.targetId &&
                    target.targetKind === line.targetKind,
            )
            return {
                ...line,
                openAmount: current?.openAmount ?? "0",
                baselineVersion: current?.baselineVersion ?? 0,
            }
        }),
    }
}

export async function saveReceiptDraft(
    session: AllocationSessionView,
    input: SaveAllocationDraftInput,
): Promise<AllocationSessionView> {
    // 完整草稿保存要求合法到账值；不完整的输入由本页暂存负责。
    const amount = normalizeFixed(input.fact.amount ?? "", {
        maxScale: 2,
        outputScale: 2,
        allowNegative: false,
    })
    if (
        !input.fact.receivedAt ||
        !Number.isFinite(new Date(input.fact.receivedAt).getTime())
    )
        throw new Error("请填写有效的到账时间。")
    const recovering = Boolean(
        session.ownerId &&
        readReceiptSession(session.ownerId, session.draftSessionId)?.pending,
    )
    const command = prepareReceiptCommand(session, "save", {
        ...input,
        fact: {
            ...input.fact,
            amount,
            receivedAt: new Date(input.fact.receivedAt).toISOString(),
        },
    })
    const original = command.session
    const fact = command.input.fact
    try {
        const saved = await apiPost<BackendCustomerReceipt>(
            "/admin/customer-receipts/drafts",
            {
                receipt_id: original.existingFactId ?? null,
                expected_version: original.existingFactVersion ?? null,
                receipt: {
                    receipt_no:
                        original.existingFactNo ??
                        `SK-${original.draftSessionId.replace("alloc_cust_", "")}`,
                    counterparty_party_id: original.counterpartyPartyId,
                    customer_id: original.existingFactId
                        ? (original.receiptCustomerId ?? null)
                        : original.customerId || null,
                    received_at: Math.floor(
                        new Date(fact.receivedAt!).getTime() / 1000,
                    ),
                    amount: fact.amount,
                    bank_reference: fact.bankReference || null,
                },
                allocations: command.input.allocations.map((line) => ({
                    receivable_entry_id: line.targetId,
                    allocated_amount: line.amount || "0",
                })),
                idempotency_key: command.input.idempotencyKey,
            },
        )
        const next: AllocationSessionView = {
            ...original,
            existingFactId: saved.id,
            existingFactNo: saved.receipt_no,
            existingFactVersion: saved.version,
            receiptCustomerId: saved.customer_id ?? null,
            fact: {
                amount: saved.amount,
                receivedAt: receiptReceivedAtInput(saved.received_at),
                bankReference: saved.bank_reference ?? "",
            },
            allocations: command.input.allocations,
            editVersion: original.editVersion + 1,
            savedAt: new Date().toISOString(),
            approval: mapCustomerReceiptApproval(saved.approval),
            recoveryPending: undefined,
            serverStatus: saved.status,
            status: saved.status === "draft" ? "draft" : "posted",
        }
        writeReceiptSession(next, null)
        return next
    } catch (error) {
        // 已有未确认操作的恢复失败仍保持锁定，不能据此生成另一笔登记。
        if (!recovering && !isReceiptOutcomeUnknown(error))
            writeReceiptSession(session, null)
        throw error
    }
}
