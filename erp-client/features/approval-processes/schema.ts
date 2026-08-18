import { z } from "zod"

/** 创建草稿表单校验。必须显式选择来源。 */
export const createDraftSchema = z.object({
    name: z.string().trim().min(1, "请输入审批流程名称").max(64, "名称过长"),
    draft_source: z
        .union([
            z.literal(""),
            z.literal("EMPTY"),
            z.literal("CURRENT_PUBLISHED"),
        ])
        .refine(
            (value) => value === "EMPTY" || value === "CURRENT_PUBLISHED",
            "请选择空白流程或复制当前已发布版本",
        ),
})

const editorNodeSchema = z.object({
    client_id: z.string().min(1),
    node_id: z.string().nullable(),
    node_name: z
        .string()
        .trim()
        .min(1, "请输入节点名称")
        .max(64, "节点名称过长"),
    assignee_user_id: z.string().min(1, "请选择一位审批人"),
    assignee_name: z.string(),
    node_purpose: z.string().nullable(),
    unsaved_purpose_slot: z.boolean(),
})

/** 草稿编辑器校验：至少一个节点，每人恰好一位审批人。 */
export const definitionEditorSchema = z.object({
    name: z.string().trim().min(1, "请输入审批流程名称").max(64, "名称过长"),
    nodes: z.array(editorNodeSchema).min(1, "至少需要一个审批节点"),
})
