/**
 * 将 TanStack Form 的 meta.errors 规整为 shadcn FieldError 可用结构。
 */
export function toFieldErrors(
    errors: unknown[] | undefined,
): Array<{ message?: string } | undefined> {
    if (!errors?.length) return []

    return errors.map((error) => {
        if (error == null) return undefined
        if (typeof error === "string") return { message: error }
        if (typeof error === "object" && "message" in error) {
            const message = (error as { message?: unknown }).message
            return {
                message:
                    typeof message === "string" ? message : String(message),
            }
        }
        return { message: String(error) }
    })
}
