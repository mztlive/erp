import { z } from "zod"

import {
    BPM_REASON_MAX_UTF8_BYTES,
    bpmReasonSchema,
    utf8ByteLength,
} from "@/lib/validation/reason-length"

/** 决定表单。驳回原因必填，通过原因可选，均受 BPM 512 UTF-8 字节上限约束。 */
export const decisionFormSchema = z
    .object({
        decision: z.enum(["APPROVE", "REJECT"]),
        reason: z.string(),
    })
    .superRefine((value, ctx) => {
        const trimmed = value.reason.trim()
        if (value.decision === "REJECT" && trimmed.length === 0) {
            ctx.addIssue({
                code: "custom",
                path: ["reason"],
                message: "驳回时必须填写原因",
            })
            return
        }
        const bytes = utf8ByteLength(trimmed)
        if (bytes > BPM_REASON_MAX_UTF8_BYTES) {
            const label = value.decision === "REJECT" ? "驳回原因" : "审批意见"
            ctx.addIssue({
                code: "custom",
                path: ["reason"],
                message: `${label}超出长度限制（当前 ${bytes} 字节，上限 ${BPM_REASON_MAX_UTF8_BYTES} 字节，约 170 个汉字）`,
            })
        }
    })

/** 受阻取消与撤回共用的原因表单（BPM 512 UTF-8 字节上限）。 */
export const reasonFormSchema = z.object({
    reason: bpmReasonSchema({ required: true, label: "原因" }),
})

/** 撤回审批原因表单（BPM 512 UTF-8 字节上限）。 */
export const withdrawReasonFormSchema = z.object({
    reason: bpmReasonSchema({ required: true, label: "撤回原因" }),
})

/** 升级未提交绑定。不允许选择任意历史定义。 */
export const upgradeBindingFormSchema = z.object({
    reason: bpmReasonSchema({ required: true, label: "更新原因" }),
})
