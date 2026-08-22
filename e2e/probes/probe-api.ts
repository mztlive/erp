/* eslint-disable no-console */
// 探测：客户中心/客户详情 API 响应（诊断 customerName 未回填）
import { request } from "@playwright/test"

async function main() {
    const ctx = await request.newContext({ baseURL: "http://127.0.0.1:10001" })
    const login = await ctx.post("/login", {
        data: { account: "xiaoshou", password: "123456", account_kind: "admin" },
    })
    const token = ((await login.json()).data as { token: string }).token
    const auth = { Authorization: `Bearer ${token}` }
    const id = "f4eecc7ca40a4100983e8a64380e8cf4"
    for (const url of [
        `/admin/customer-profiles/${id}`,
        `/admin/customers/${id}`,
        `/admin/customer-profiles?customer_id=${id}`,
    ]) {
        const r = await ctx.get(url, { headers: auth })
        console.log(url, "->", r.status())
        console.log((await r.text()).slice(0, 700))
        console.log("----")
    }
}
main().catch((e) => {
    console.error(e)
    process.exit(1)
})
