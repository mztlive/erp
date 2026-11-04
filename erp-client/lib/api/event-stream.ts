import { getApiBaseUrl } from "./client"
import { getToken, notifyUnauthorized } from "./session"
import { fromAuth, fromHttpResponse } from "./errors"

/** SSE 使用 Authorization 请求头，凭证不进入 URL；生命周期由 Query 取消信号控制。 */
export async function apiEventStream(
    path: string,
    signal: AbortSignal,
    receive: (event: string, data: string) => void,
) {
    const token = getToken()
    const response = await fetch(`${getApiBaseUrl()}${path}`, {
        headers: {
            Authorization: `Bearer ${token ?? ""}`,
            Accept: "text/event-stream",
        },
        signal: AbortSignal.any([signal, AbortSignal.timeout(75_000)]),
        cache: "no-store",
    })
    if (response.status === 401) {
        notifyUnauthorized("admin", token)
        throw fromAuth(401)
    }
    if (!response.ok) throw fromHttpResponse(response.status)
    if (
        !response.body ||
        !response.headers.get("content-type")?.includes("text/event-stream")
    ) {
        throw new Error("实时消息暂不可用，请稍后重试")
    }
    const reader = response.body
        .pipeThrough(new TextDecoderStream())
        .getReader()
    let buffer = ""
    try {
        while (true) {
            const { value, done } = await reader.read()
            if (done) return
            buffer = (buffer + value).replaceAll("\r\n", "\n")
            if (buffer.length > 65_536) throw new Error("实时消息格式异常")
            let boundary: number
            while ((boundary = buffer.indexOf("\n\n")) >= 0) {
                const frame = buffer.slice(0, boundary)
                buffer = buffer.slice(boundary + 2)
                const lines = frame.split("\n")
                const event =
                    lines
                        .find((line) => line.startsWith("event:"))
                        ?.slice(6)
                        .trim() ?? "message"
                const data = lines
                    .filter((line) => line.startsWith("data:"))
                    .map((line) => line.slice(5).trimStart())
                    .join("\n")
                if (event === "auth") {
                    notifyUnauthorized("admin", token)
                    throw fromAuth(401)
                }
                if (event === "error")
                    throw new Error("实时消息暂不可用，请稍后重试")
                if (data) receive(event, data)
            }
        }
    } finally {
        await reader.cancel().catch(() => undefined)
        reader.releaseLock()
    }
}
