import {
    FormalCommandKeyLedger,
    type FormalCommandIdentity,
    type FormalCommandSettlement,
} from "@/lib/formal-command"
import { inquiryStorageKey, writeInquiryLocalValue } from "@/lib/inquiry-local"
import type { CreateSalesOrderInput } from "../types"

export type InquirySalePayload = Omit<CreateSalesOrderInput, "idempotencyKey">
export function inquirySaleScope(id: string, round: number) {
    return `inquiry-${id}-${round}`
}
export function inquirySaleCommandKey(userId: string, scope: string) {
    return inquiryStorageKey(userId, `${scope}:sales-create-command`)
}
export function inquirySaleDraftKey(userId: string, scope: string) {
    return inquiryStorageKey(userId, `${scope}:sales-create-draft`)
}

/** 原命令损坏时失败关闭，禁止静默换键重建销售。 */
export function readInquirySaleCommand(
    userId: string,
    scope: string,
): FormalCommandIdentity<InquirySalePayload> | undefined {
    if (!userId || typeof localStorage === "undefined") return undefined
    const raw = localStorage.getItem(inquirySaleCommandKey(userId, scope))
    if (!raw) return undefined
    try {
        const command = JSON.parse(
            raw,
        ) as FormalCommandIdentity<InquirySalePayload>
        if (
            !command.idempotencyKey ||
            typeof command.idempotencyKey !== "string" ||
            !command.payload?.inquirySource ||
            !Array.isArray(command.payload.lineItems)
        )
            throw new Error("invalid")
        if (
            inquirySaleScope(
                command.payload.inquirySource.inquiry_id,
                command.payload.inquirySource.feedback_round,
            ) !== scope
        )
            throw new Error("scope")
        return command
    } catch {
        throw new Error(
            "上次开单记录暂时无法读取，请先核对销售单列表，保留本地记录并联系管理员处理。",
        )
    }
}

/** 仅持久化询价创建销售的原命令，其他销售动作沿用既有账本。 */
export class InquirySalesCommandLedger extends FormalCommandKeyLedger {
    readonly #userId: string
    readonly #scope: string
    readonly #fresh = new Set<string>()
    readonly #uncertain = new Set<string>()
    constructor(userId: string, scope: string) {
        super()
        this.#userId = userId
        this.#scope = scope
    }
    override peek<T>(slot: string): FormalCommandIdentity<T> | undefined {
        if (slot !== "create") return super.peek<T>(slot)
        return readInquirySaleCommand(this.#userId, this.#scope) as
            | FormalCommandIdentity<T>
            | undefined
    }
    override acquire<T>(
        slot: string,
        prefix: string,
        payload: T,
    ): FormalCommandIdentity<T> {
        if (slot !== "create") return super.acquire(slot, prefix, payload)
        if (!this.#userId) throw new Error("正在核对当前账号，请稍候再提交。")
        const previous = this.peek<T>(slot)
        if (previous) return previous
        const command = super.acquire(slot, prefix, payload)
        this.#fresh.add(command.idempotencyKey)
        try {
            writeInquiryLocalValue(
                inquirySaleCommandKey(this.#userId, this.#scope),
                command,
            )
        } catch {
            super.settle(slot, "failed")
            throw new Error(
                "无法保存本次开单记录，尚未发送请求。请允许本地存储后重试。",
            )
        }
        return command
    }
    override settle(slot: string, settlement: FormalCommandSettlement): void {
        if (slot === "create" && this.#userId) {
            const command = this.peek<InquirySalePayload>(slot)
            if (command && settlement === "unknown")
                this.#uncertain.add(command.idempotencyKey)
            // 刷新恢复或曾经响应未知时，后续业务拒绝不能证明原创建未成功。
            if (
                command &&
                settlement === "failed" &&
                (!this.#fresh.has(command.idempotencyKey) ||
                    this.#uncertain.has(command.idempotencyKey))
            )
                return
            if (settlement !== "unknown")
                localStorage.removeItem(
                    inquirySaleCommandKey(this.#userId, this.#scope),
                )
        }
        super.settle(slot, settlement)
    }
}
