import { inquiryPath } from "../api"

/** 只有已确认成功的命令才能结束当前采购任务页面。 */
export function inquiryTaskCompletion(
    inquiryId: string,
    command: { path: string; body: Record<string, unknown> },
) {
    if (typeof command.body.task_id !== "string") return null
    const root = inquiryPath(true, inquiryId)
    const messages: Record<string, string> = {
        [`${root}/publish`]: "本轮反馈已发布，等待销售处理。",
        [`${root}/supplement`]:
            "已要求销售补充需求，补充后将重新分派采购任务。",
        [`${root}/reassign`]: "责任采购已改派，请由新的责任采购继续处理。",
    }
    const message = messages[command.path]
    return message ? { taskId: command.body.task_id, message } : null
}
