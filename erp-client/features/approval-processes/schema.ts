import { z } from "zod"
import type { DefinitionDetailView } from "./types"

/** 创建方式仅用于界面；跨类型复制先建空草稿，再走目标节点保存接口。 */
export const createDraftSchema = z.object({
    name: z.string().trim().min(1, "请输入审批流程名称").max(64, "名称过长"),
    draft_source: z.enum(["EMPTY", "CURRENT_PUBLISHED", "OTHER"]),
    copy_source: z.custom<DefinitionDetailView>().nullable(),
})

const editorNodeSchema = z
    .object({
        client_id: z.string().min(1),
        node_id: z.string().nullable(),
        node_name: z
            .string()
            .trim()
            .min(1, "请输入节点名称")
            .max(64, "节点名称过长"),
        assignee_source: z.enum([
            "SPECIFIED",
            "APPLICANT_TEAM_LEADER",
            "APPLICANT_DEPARTMENT_LEADER",
        ]),
        assignee_user_id: z.string(),
        assignee_name: z.string(),
        additional_assignees: z
            .array(z.object({ user_id: z.string().min(1), name: z.string() }))
            .max(49, "每个节点最多 50 名审批人"),
        node_purpose: z.string().nullable(),
        unsaved_purpose_slot: z.boolean(),
    })
    .refine(
        (node) =>
            node.assignee_source !== "SPECIFIED" ||
            Boolean(node.assignee_user_id),
        "请选择一位审批人",
    )
    .refine(
        (node) =>
            new Set([
                node.assignee_user_id,
                ...node.additional_assignees.map((person) => person.user_id),
            ]).size ===
            node.additional_assignees.length + 1,
        "同一节点审批人不能重复",
    )

/** 草稿编辑器校验：至少一个节点，每个节点至少一位审批人，多人按或签处理。 */
export const definitionEditorSchema = z.object({
    copyRules: z
        .array(
            z.object({
                trigger: z.enum([
                    "STARTED",
                    "NODE_APPROVED",
                    "APPROVED",
                    "REJECTED",
                    "CANCELLED",
                ]),
                nodeClientId: z.string().nullable(),
                recipients: z
                    .array(
                        z.object({
                            user_id: z.string().min(1),
                            name: z.string(),
                        }),
                    )
                    .min(1, "请选择抄送人")
                    .max(50),
            }),
        )
        .max(54),
    name: z.string().trim().min(1, "请输入审批流程名称").max(64, "名称过长"),
    nodes: z
        .array(editorNodeSchema)
        .min(1, "至少需要一个审批节点")
        .max(20, "最多配置 20 个审批节点"),
})
