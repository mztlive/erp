/* eslint-disable no-console */
import { request } from "@playwright/test"
async function main() {
    const ctx = await request.newContext({ baseURL: "http://127.0.0.1:10001" })
    const login = await ctx.post("/login", { data: { account: "caigou", password: "123456", account_kind: "admin" } })
    const token = ((await login.json()).data as { token: string }).token
    const r = await ctx.get("/admin/work-items?scope=mine&sort=priority_due&timezone=Asia%2FShanghai&page=1&page_size=50", { headers: { Authorization: `Bearer ${token}` } })
    const body = (await r.json()).data
    console.log("total:", body.total, "items:", body.items?.length)
    for (const it of (body.items ?? [])) {
        console.log(JSON.stringify({ id: it.work_item_id ?? it.workItemId, stable: it.stable_number ?? it.stableNumber, obj: it.business_object_id, kind: it.business_object_kind, status: it.status }))
    }
}
main().catch((e) => { console.error(e); process.exit(1) })
