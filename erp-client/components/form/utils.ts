import { getErrorMessage } from "@/lib/api/errors"

/**
 * 将 TanStack Form 的 meta.errors 规整为 shadcn FieldError 可用结构。
 */
export function toFieldErrors(
    errors: unknown[] | undefined,
): Array<{ message?: string } | undefined> {
    if (!errors?.length) return []

    return errors.map((error) => {
        if (error == null) return undefined
        const message =
            typeof error === "object" && "message" in error
                ? (error as { message?: unknown }).message
                : error
        return {
            message: getErrorMessage(
                message,
                "填写内容未通过检查，请修改后重试。",
            ),
        }
    })
}
