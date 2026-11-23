/**
 * 任务与审批原因长度校验唯一真源。
 *
 * 严格对齐后端四档真实上限：
 * - BPM 审批决定 / 撤回 / 终止受阻 / 升级绑定 / 重提原因：512 UTF-8 字节（约 170 汉字）
 * - 放弃退回重办 (ABANDON_REWORK)：512 字符（必填）
 * - 任务转交 (REASSIGN)：150 字符（必填）
 * - 任务关闭说明 (CLOSE comment)：100 字符（可选）
 */

import { z } from "zod"

export const BPM_REASON_MAX_UTF8_BYTES = 512
export const WORK_ITEM_REASSIGN_REASON_MAX_CHARS = 150
export const WORK_ITEM_CLOSE_COMMENT_MAX_CHARS = 100
export const WORK_ITEM_ABANDON_REASON_MAX_CHARS = 512

const utf8Encoder = new TextEncoder()

/** 计算字符串的 UTF-8 字节长度（与后端 BPM Rust `str::len()` 一致）。 */
export function utf8ByteLength(value: string): number {
    return utf8Encoder.encode(value).length
}

/** 计算字符串的 Unicode 标量字符数（与后端 Rust `str::chars().count()` 一致）。 */
function unicodeCharLength(value: string): number {
    return Array.from(value).length
}

/** 按单汉字 3 字节估算当前剩余可输入的汉字数量。 */
export function remainingApproxCjkChars(
    value: string,
    maxBytes = BPM_REASON_MAX_UTF8_BYTES,
): number {
    const remainingBytes = maxBytes - utf8ByteLength(value)
    return remainingBytes > 0 ? Math.floor(remainingBytes / 3) : 0
}

/** BPM 原因校验器：校验去首尾空白后的必填性与 UTF-8 字节上限（512 字节）。 */
export function bpmReasonSchema(options: {
    required: boolean
    label?: string
}): z.ZodString {
    const label = options.label ?? "原因"
    const approxMaxCjkChars = Math.floor(BPM_REASON_MAX_UTF8_BYTES / 3)
    return z.string().superRefine((raw, ctx) => {
        const trimmed = raw.trim()
        if (options.required && trimmed.length === 0) {
            ctx.addIssue({
                code: "custom",
                message: `请填写${label}`,
            })
            return
        }
        const bytes = utf8ByteLength(trimmed)
        if (bytes > BPM_REASON_MAX_UTF8_BYTES) {
            ctx.addIssue({
                code: "custom",
                message: `${label}超出长度限制（当前 ${bytes} 字节，上限 ${BPM_REASON_MAX_UTF8_BYTES} 字节，约 ${approxMaxCjkChars} 个汉字）`,
            })
        }
    })
}

/** 按字符数限制的通用原因校验器（与后端 `chars().count()` 一致）。 */
export function charLimitSchema(options: {
    maxChars: number
    required: boolean
    label?: string
}): z.ZodString {
    const label = options.label ?? "原因"
    return z.string().superRefine((raw, ctx) => {
        const trimmed = raw.trim()
        if (options.required && trimmed.length === 0) {
            ctx.addIssue({
                code: "custom",
                message: `请填写${label}`,
            })
            return
        }
        const chars = unicodeCharLength(trimmed)
        if (chars > options.maxChars) {
            ctx.addIssue({
                code: "custom",
                message: `${label}不能超过 ${options.maxChars} 个字（当前 ${chars} 字）`,
            })
        }
    })
}

export const workItemReassignReasonSchema: z.ZodString = charLimitSchema({
    maxChars: WORK_ITEM_REASSIGN_REASON_MAX_CHARS,
    required: true,
    label: "转交原因",
})

export const workItemCloseCommentSchema: z.ZodString = charLimitSchema({
    maxChars: WORK_ITEM_CLOSE_COMMENT_MAX_CHARS,
    required: false,
    label: "关闭说明",
})

export const workItemAbandonReasonSchema: z.ZodString = charLimitSchema({
    maxChars: WORK_ITEM_ABANDON_REASON_MAX_CHARS,
    required: true,
    label: "放弃原因",
})

/** 格式化 BPM UTF-8 字节实时计数提示文案。 */
export function formatByteCounterText(
    value: string,
    maxBytes = BPM_REASON_MAX_UTF8_BYTES,
): string {
    const used = utf8ByteLength(value)
    if (used > maxBytes) {
        const exceeded = used - maxBytes
        return `已用 ${used}/${maxBytes} 字节（已超出 ${exceeded} 字节）`
    }
    const remainingChars = Math.floor((maxBytes - used) / 3)
    return `已用 ${used}/${maxBytes} 字节（约还可输 ${remainingChars} 字）`
}

/** 格式化字符数实时计数提示文案。 */
export function formatCharCounterText(value: string, maxChars: number): string {
    const used = unicodeCharLength(value)
    if (used > maxChars) {
        return `${used}/${maxChars} 字（已超出 ${used - maxChars} 字）`
    }
    return `${used}/${maxChars} 字`
}
